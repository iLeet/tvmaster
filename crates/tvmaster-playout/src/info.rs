//! The info channel's playout: pages drawn live, round and round, with music underneath. The
//! pages are made again every few seconds from what the station knows (headlines, what is on
//! tonight, the time), so the channel is always current without anyone touching it.

use crate::guide::{background_image, load_music, music_list_text, StopOnDrop, STALL};
use crate::profile::{guide_run_args, GuideRun, GUIDE_FPS};
use crate::runner::now_ms;
use crate::ts::ContinuityFixer;
use crate::{ChannelSpec, EngineConfig, TonightRow};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tvmaster_core::{InfoConfig, SlideKind, Zone};
use tvmaster_render::info::{
    render_info, InfoFrame, InfoPage, ListItem, Picture, TimedPage, FULL_PICTURE, SIDE_PICTURE,
};

/// How often the pages are made again, in stream seconds.
const REFRESH_SECS: u64 = 5;
/// Items on a list page.
const PER_PAGE: usize = 5;

/// Everything the pages are made from.
pub(crate) struct PageInputs<'a> {
    pub config: &'a InfoConfig,
    pub headlines: &'a [String],
    pub tonight: &'a [TonightRow],
    /// Pictures by their path on the server, already prepared (a page without its picture is
    /// left out, or shown without it).
    pub pictures: &'a HashMap<(String, bool), Picture>,
}

/// The pages of the channel, in order, for this moment.
pub(crate) fn build_pages(i: &PageInputs) -> Vec<TimedPage> {
    let cfg = i.config;
    let mut out = Vec::new();
    let default_clock = [tvmaster_core::InfoSlide {
        kind: SlideKind::Clock,
        ..Default::default()
    }];
    let slides: &[tvmaster_core::InfoSlide] = if cfg.slides.is_empty() {
        &default_clock
    } else {
        &cfg.slides
    };
    for s in slides {
        let secs = f32::from(s.seconds.unwrap_or(cfg.seconds));
        let pic = |full: bool| {
            s.picture
                .as_deref()
                .and_then(|p| i.pictures.get(&(p.to_string(), full)))
                .cloned()
        };
        match s.kind {
            SlideKind::Text => out.push(TimedPage {
                page: InfoPage::Text {
                    title: s.title.clone(),
                    lines: s
                        .lines
                        .iter()
                        .filter(|l| !l.trim().is_empty())
                        .cloned()
                        .collect(),
                    picture: pic(false),
                },
                secs,
            }),
            SlideKind::Clock => out.push(TimedPage {
                page: InfoPage::Clock,
                secs,
            }),
            SlideKind::Photo => {
                if let Some(picture) = pic(true) {
                    out.push(TimedPage {
                        page: InfoPage::Photo {
                            picture,
                            caption: s.title.clone(),
                        },
                        secs,
                    });
                }
            }
            SlideKind::Headlines => {
                let title = if s.title.trim().is_empty() {
                    "Headlines".to_string()
                } else {
                    s.title.clone()
                };
                if i.headlines.is_empty() {
                    out.push(TimedPage {
                        page: InfoPage::Text {
                            title: title.clone(),
                            lines: vec!["Fetching the latest headlines…".into()],
                            picture: None,
                        },
                        secs,
                    });
                }
                for (n, chunk) in i.headlines.chunks(PER_PAGE).enumerate() {
                    out.push(TimedPage {
                        page: InfoPage::List {
                            title: title.clone(),
                            items: chunk
                                .iter()
                                .enumerate()
                                .map(|(k, h)| ListItem {
                                    tag: (n * PER_PAGE + k + 1).to_string(),
                                    text: h.clone(),
                                    sub: String::new(),
                                })
                                .collect(),
                        },
                        secs,
                    });
                }
            }
            SlideKind::Tonight => {
                let title = if s.title.trim().is_empty() {
                    "On tonight".to_string()
                } else {
                    s.title.clone()
                };
                for chunk in i.tonight.chunks(PER_PAGE) {
                    out.push(TimedPage {
                        page: InfoPage::List {
                            title: title.clone(),
                            items: chunk
                                .iter()
                                .map(|r| ListItem {
                                    tag: r.number.to_string(),
                                    text: if r.now.is_empty() {
                                        r.channel.clone()
                                    } else {
                                        r.now.clone()
                                    },
                                    sub: tonight_sub(r),
                                })
                                .collect(),
                        },
                        secs,
                    });
                }
            }
        }
    }
    if out.is_empty() {
        out.push(TimedPage {
            page: InfoPage::Clock,
            secs: f32::from(cfg.seconds),
        });
    }
    out
}

fn tonight_sub(r: &TonightRow) -> String {
    let mut s = r.channel.clone();
    if !r.until.is_empty() {
        s.push_str(&format!(" · until {}", r.until));
    }
    if !r.next.is_empty() {
        s.push_str(&format!(" · then {} at {}", r.next, r.at));
    }
    s
}

/// The ticker text: the channel's own lines, then the headlines.
pub(crate) fn ticker_text(cfg: &InfoConfig, headlines: &[String]) -> String {
    if !cfg.ticker_enabled {
        return String::new();
    }
    cfg.ticker
        .iter()
        .chain(headlines.iter())
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("   *   ")
}

/// Prepare every picture the pages use (blocking: runs FFmpeg once for each).
fn load_pictures(ffmpeg: &Path, cfg: &InfoConfig) -> HashMap<(String, bool), Picture> {
    let mut out = HashMap::new();
    for s in &cfg.slides {
        let Some(p) = s.picture.as_deref().filter(|p| !p.trim().is_empty()) else {
            continue;
        };
        // A notice has its picture beside the text; a picture page fills the screen.
        let full = s.kind == SlideKind::Photo;
        if out.contains_key(&(p.to_string(), full)) {
            continue;
        }
        let (w, h) = if full { FULL_PICTURE } else { SIDE_PICTURE };
        if let Some(rgb) = background_image(ffmpeg, Path::new(p), w, h, 0) {
            out.insert((p.to_string(), full), Picture { w, h, rgb });
        } else {
            tracing::warn!(picture = %p, "an info page's picture could not be read");
        }
    }
    out
}

/// How many pages the channel has right now (a headlines page can run to several).
pub fn page_count(cfg: &InfoConfig, headlines: &[String], tonight: &[TonightRow]) -> usize {
    build_pages(&PageInputs {
        config: cfg,
        headlines,
        tonight,
        pictures: &HashMap::new(),
    })
    .len()
}

/// One page of the channel as a picture (a PNG), for the editor. `page` counts from 0 and wraps.
/// Blocks (runs FFmpeg once for each picture).
#[allow(clippy::too_many_arguments)]
pub fn preview_png(
    ffmpeg: &Path,
    cfg: &InfoConfig,
    name: &str,
    theme: &tvmaster_render::Theme,
    headlines: &[String],
    tonight: &[TonightRow],
    zone: Zone,
    now_ms: i64,
    page: usize,
) -> Vec<u8> {
    let pictures = load_pictures(ffmpeg, cfg);
    let pages = build_pages(&PageInputs {
        config: cfg,
        headlines,
        tonight,
        pictures: &pictures,
    });
    let page = page % pages.len().max(1);
    let t: f32 = pages[..page].iter().map(|p| p.secs.max(1.0)).sum::<f32>() + 2.0;
    let mut th = *theme;
    th.modern = true;
    let heading = cfg
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| name.to_string());
    let (clock, date) = (zone.clock_text(now_ms, cfg.hour12), zone.date_text(now_ms));
    let ticker = ticker_text(cfg, headlines);
    let c = render_info(
        &InfoFrame {
            heading: &heading,
            clock: &clock,
            date: &date,
            show_clock: cfg.clock,
            ticker: &ticker,
            pages: &pages,
        },
        &th,
        t,
    );
    let mut rgba = Vec::with_capacity(c.w * c.h * 4);
    for px in c.px.as_chunks::<3>().0 {
        rgba.extend_from_slice(px);
        rgba.push(255);
    }
    tvmaster_render::png::rgba_png(c.w as u32, c.h as u32, &rgba)
}

/// Draw the info channel until it is stopped.
pub(crate) async fn produce_info(
    cfg: Arc<EngineConfig>,
    spec: Arc<ChannelSpec>,
    tx: mpsc::Sender<Vec<u8>>,
) -> Result<()> {
    let ispec = spec.info.clone().context("not an info channel")?;
    let zone = ispec.zone.unwrap_or_else(Zone::utc);
    let config = ispec.config.clone();
    let ffmpeg = cfg.ffmpeg.clone();
    let c2 = config.clone();
    let pictures = Arc::new(
        tokio::task::spawn_blocking(move || load_pictures(&ffmpeg, &c2))
            .await
            .map_err(|e| anyhow!("loading the pictures: {e}"))?,
    );
    let dir = cfg.root.join("hls").join(
        spec.id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>(),
    );
    let music = load_music(&cfg, &ispec.music).await;
    let start_wall = now_ms();
    let mut frames_so_far: u64 = 0;
    let mut fixer = ContinuityFixer::new();
    let mut run_no = 0u32;
    let mut failures = 0u32;

    loop {
        let elapsed = frames_so_far as f64 / f64::from(GUIDE_FPS);
        let list: Option<PathBuf> = match &music {
            Some(m) => {
                let path = dir.join(format!("music-{}.txt", run_no % 2));
                tokio::fs::write(&path, music_list_text(m, elapsed))
                    .await
                    .ok()
                    .map(|_| path)
            }
            None => None,
        };
        run_no += 1;
        let args = guide_run_args(
            &GuideRun {
                src_w: 1280,
                src_h: 720,
                smooth_scale: true,
                music_list: list.as_deref(),
                pip: None,
                logo: None,
                duration_ms: None,
                ts_offset_ms: (elapsed * 1000.0) as u64,
            },
            &cfg.profile,
            cfg.encoder,
        );
        let mut child = Command::new(&cfg.ffmpeg)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
        let mut stdin = child.stdin.take().context("info encoder has no stdin")?;
        let mut out = child.stdout.take().context("info encoder has no stdout")?;

        let written = Arc::new(AtomicU64::new(0));
        let (written_w, spec_w, config_w, pictures_w) = (
            written.clone(),
            spec.clone(),
            config.clone(),
            pictures.clone(),
        );
        let first = frames_so_far;
        let writer = StopOnDrop(tokio::spawn(async move {
            const BATCH: u64 = 3;
            let mut k: u64 = 0;
            let mut pages: Vec<TimedPage> = Vec::new();
            let mut ticker = String::new();
            loop {
                let frame = first + k;
                if k.is_multiple_of(REFRESH_SECS * u64::from(GUIDE_FPS)) {
                    // "Now" is where the stream is; the encoder runs ahead of real time.
                    let on_stream =
                        start_wall + (frame as f64 * 1000.0 / f64::from(GUIDE_FPS)) as i64;
                    let wall = on_stream.max(now_ms());
                    let (src, id) = (spec_w.source.clone(), spec_w.id.clone());
                    let (cfg2, pics) = (config_w.clone(), pictures_w.clone());
                    let (p, tk) = tokio::task::spawn_blocking(move || {
                        let hl = src.headlines(&id);
                        let tonight = if cfg2.slides.iter().any(|s| s.kind == SlideKind::Tonight) {
                            src.tonight(&id, wall)
                        } else {
                            Vec::new()
                        };
                        let pages = build_pages(&PageInputs {
                            config: &cfg2,
                            headlines: &hl,
                            tonight: &tonight,
                            pictures: &pics,
                        });
                        (pages, ticker_text(&cfg2, &hl))
                    })
                    .await
                    .map_err(std::io::Error::other)?;
                    pages = p;
                    ticker = tk;
                }
                let wall = start_wall + (frame as f64 * 1000.0 / f64::from(GUIDE_FPS)) as i64;
                let wall = wall.max(now_ms());
                let mut th = crate::spec_theme_at(&spec_w, wall);
                // Info pages are always drawn in the smooth style.
                th.modern = true;
                let (clock, date) = (zone.clock_text(wall, config_w.hour12), zone.date_text(wall));
                let heading = config_w
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| spec_w.name.clone());
                let (p, tk, cfg3) = (pages.clone(), ticker.clone(), config_w.clone());
                let chunk = tokio::task::spawn_blocking(move || {
                    let mut buf = Vec::with_capacity(BATCH as usize * 1280 * 720 * 3);
                    for j in 0..BATCH {
                        let t = (frame + j) as f32 / GUIDE_FPS as f32;
                        let f = InfoFrame {
                            heading: &heading,
                            clock: &clock,
                            date: &date,
                            show_clock: cfg3.clock,
                            ticker: &tk,
                            pages: &p,
                        };
                        buf.extend_from_slice(&render_info(&f, &th, t).px);
                    }
                    buf
                })
                .await
                .map_err(std::io::Error::other)?;
                stdin.write_all(&chunk).await?;
                written_w.fetch_add(BATCH, Ordering::Relaxed);
                k += BATCH;
            }
            #[allow(unreachable_code)]
            Ok::<(), std::io::Error>(())
        }));

        let started = Instant::now();
        let mut buf = vec![0u8; 64 * 1024];
        let mut ok = true;
        loop {
            let n = match tokio::time::timeout(STALL, out.read(&mut buf)).await {
                Ok(r) => r?,
                Err(_) => {
                    tracing::warn!(channel = %spec.id, "the info encoder stalled");
                    ok = false;
                    break;
                }
            };
            if n == 0 {
                break;
            }
            let packets = fixer.process(&buf[..n]);
            if !packets.is_empty() && tx.send(packets).await.is_err() {
                return Err(anyhow!("segmenter writer closed"));
            }
        }
        fixer.end_item();
        drop(writer);
        if ok {
            let _ = child.wait().await;
        } else {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        frames_so_far += written.load(Ordering::Relaxed);
        failures += 1;
        if failures >= 3 || started.elapsed() > Duration::from_secs(30) {
            return Err(anyhow!("the info channel's encoder keeps failing"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvmaster_core::InfoSlide;

    fn slide(kind: SlideKind, title: &str) -> InfoSlide {
        InfoSlide {
            kind,
            title: title.into(),
            lines: vec!["a line".into()],
            ..Default::default()
        }
    }

    fn pages(cfg: &InfoConfig, hl: &[String], tonight: &[TonightRow]) -> Vec<TimedPage> {
        build_pages(&PageInputs {
            config: cfg,
            headlines: hl,
            tonight,
            pictures: &HashMap::new(),
        })
    }

    #[test]
    fn an_empty_channel_shows_the_clock() {
        let p = pages(&InfoConfig::default(), &[], &[]);
        assert_eq!(p.len(), 1);
        assert!(matches!(p[0].page, InfoPage::Clock));
    }

    #[test]
    fn headlines_and_tonight_run_over_as_many_pages_as_they_need() {
        let cfg = InfoConfig {
            slides: vec![
                slide(SlideKind::Headlines, ""),
                slide(SlideKind::Tonight, "Tonight"),
            ],
            seconds: 9,
            ..Default::default()
        };
        let hl: Vec<String> = (1..=12).map(|i| format!("Story {i}")).collect();
        let rows: Vec<TonightRow> = (1..=7)
            .map(|n| TonightRow {
                number: n,
                channel: format!("Ch {n}"),
                now: format!("Show {n}"),
                until: "9:00 PM".into(),
                next: "Next".into(),
                at: "9:00 PM".into(),
            })
            .collect();
        let p = pages(&cfg, &hl, &rows);
        // 12 headlines = 3 pages, 7 channels = 2 pages.
        assert_eq!(p.len(), 5);
        match &p[2].page {
            InfoPage::List { items, title } => {
                assert_eq!(title, "Headlines");
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].tag, "11");
            }
            _ => panic!("a list page"),
        }
        match &p[3].page {
            InfoPage::List { items, title } => {
                assert_eq!(title, "Tonight");
                assert_eq!(items[0].tag, "1");
                assert_eq!(items[0].text, "Show 1");
                assert_eq!(items[0].sub, "Ch 1 · until 9:00 PM · then Next at 9:00 PM");
            }
            _ => panic!("a list page"),
        }
        assert!(p.iter().all(|t| t.secs == 9.0));
        // Before the first fetch there is a friendly page, not a blank screen.
        let none = pages(&cfg, &[], &[]);
        assert!(matches!(none[0].page, InfoPage::Text { .. }));
    }

    #[test]
    fn a_page_can_take_its_own_time_and_a_missing_picture_is_left_out() {
        let mut photo = slide(SlideKind::Photo, "The lake");
        photo.picture = Some("/nowhere/lake.jpg".into());
        let mut notice = slide(SlideKind::Text, "Garage sale");
        notice.seconds = Some(30);
        let cfg = InfoConfig {
            slides: vec![photo, notice],
            ..Default::default()
        };
        let p = pages(&cfg, &[], &[]);
        assert_eq!(p.len(), 1, "the picture page has nothing to show");
        assert_eq!(p[0].secs, 30.0);
    }

    #[test]
    fn the_ticker_has_your_lines_then_the_headlines() {
        let cfg = InfoConfig {
            ticker: vec!["Welcome".into(), "  ".into()],
            ..Default::default()
        };
        let t = ticker_text(&cfg, &["Story one".into()]);
        assert_eq!(t, "Welcome   *   Story one");
        let off = InfoConfig {
            ticker_enabled: false,
            ..cfg
        };
        assert_eq!(ticker_text(&off, &["x".into()]), "");
    }

    #[test]
    fn the_clock_reads_in_the_channels_zone() {
        let z = Zone::parse("America/Toronto").unwrap();
        // 2026-11-16 02:30 UTC is 21:30 on the 15th in Toronto (EST).
        let ms = 1_794_796_200_000i64;
        assert_eq!(z.clock_text(ms, true), "9:30 PM");
        assert_eq!(z.clock_text(ms, false), "21:30");
        assert_eq!(z.date_text(ms), "SUN NOV 15");
    }
}
