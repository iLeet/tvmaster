//! Deciding, at the moment an item goes to air, which audio track and subtitle
//! it gets and which graphics go over it, and preparing the files FFmpeg needs
//! for that (a text subtitle cut out of the video, the logo, the title card).

use crate::looks::{corner_xy, AudioFx, Burn, ClockFx, ItemPlan, OverlayInput};
use crate::{ChannelSpec, EngineConfig, PlayItem};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tvmaster_core::{choose, fnv1a, Corner, Deinterlace, SubPick, Tracks};

fn cache_dir(cfg: &EngineConfig, what: &str) -> PathBuf {
    cfg.root.join("cache").join(what)
}

/// A file name safe to put inside an FFmpeg filter string.
fn safe_name(seed: &str, ext: &str) -> String {
    format!("{:016x}.{ext}", fnv1a(seed))
}

fn mtime_size(p: &Path) -> (u64, u64) {
    std::fs::metadata(p)
        .map(|m| {
            (
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                m.len(),
            )
        })
        .unwrap_or((0, 0))
}

/// A text subtitle as a file with a safe name: copied from beside the video, or
/// cut out of it once and kept.
async fn text_subtitle(
    cfg: &EngineConfig,
    media: &Path,
    pick: &SubPick,
    tracks: &Tracks,
) -> Option<PathBuf> {
    let dir = cache_dir(cfg, "subs");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    match pick {
        SubPick::External(path) => {
            let src = Path::new(path);
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("srt")
                .to_lowercase();
            let (m, n) = mtime_size(src);
            let out = dir.join(safe_name(&format!("{path}|{m}|{n}"), &ext));
            if out.exists() || tokio::fs::copy(src, &out).await.is_ok() {
                return Some(out);
            }
            None
        }
        SubPick::Embedded {
            index,
            image: false,
        } => {
            let codec = tracks
                .subs
                .iter()
                .find(|s| s.index == *index)
                .map(|s| s.codec.as_str())
                .unwrap_or("subrip");
            let (ext, codec_arg) = if matches!(codec, "ass" | "ssa") {
                ("ass", "copy")
            } else {
                ("srt", "srt")
            };
            let (m, n) = mtime_size(media);
            let out = dir.join(safe_name(
                &format!("{}|{index}|{m}|{n}", media.display()),
                ext,
            ));
            if out.exists() {
                return Some(out);
            }
            let tmp = out.with_extension(format!("{ext}.part"));
            let status = Command::new(&cfg.ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
                .arg(media)
                .args([
                    "-map",
                    &format!("0:{index}"),
                    "-c:s",
                    codec_arg,
                    "-f",
                    if ext == "ass" { "ass" } else { "srt" },
                ])
                .arg(&tmp)
                .status()
                .await
                .ok()?;
            if status.success() && tokio::fs::rename(&tmp, &out).await.is_ok() {
                Some(out)
            } else {
                let _ = tokio::fs::remove_file(&tmp).await;
                None
            }
        }
        SubPick::Embedded { image: true, .. } => None,
    }
}

/// Write `png` once under `<root>/cache/<what>/<name>` and return where it is.
async fn graphic(cfg: &EngineConfig, what: &str, name: &str, png: &[u8]) -> Option<PathBuf> {
    let dir = cache_dir(cfg, what);
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(name);
    if !path.exists() {
        let tmp = path.with_extension("part");
        tokio::fs::write(&tmp, png).await.ok()?;
        tokio::fs::rename(&tmp, &path).await.ok()?;
    }
    Some(path)
}

/// A logo picture of any common format, fitted into a corner-sized box with
/// the channel's logo opacity, as a PNG FFmpeg can overlay. Made once per
/// picture (changing the file or the settings makes a new one); `None` when the
/// file is missing or FFmpeg cannot read it, so the text logo is used instead.
async fn logo_png(cfg: &EngineConfig, src: &Path, opacity: u8) -> Option<PathBuf> {
    let (w, h) = (cfg.profile.width * 22 / 100, cfg.profile.height * 16 / 100);
    fit_picture(cfg, src, w, h, opacity).await
}

/// `src` (any common picture format) fitted inside `w` x `h` pixels, with the
/// given opacity, as a PNG in the cache. `None` when it cannot be read.
pub(crate) async fn fit_picture(
    cfg: &EngineConfig,
    src: &Path,
    w: u32,
    h: u32,
    opacity: u8,
) -> Option<PathBuf> {
    let meta = tokio::fs::metadata(src)
        .await
        .ok()
        .filter(|m| m.is_file())?;
    let stamp = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let key = format!("{}|{}|{stamp}|{opacity}|{w}x{h}", src.display(), meta.len());
    let dir = cache_dir(cfg, "logos");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(format!("{:016x}.png", fnv1a(&key)));
    if path.exists() {
        return Some(path);
    }
    let tmp = path.with_extension("part.png");
    let alpha = f64::from(opacity.clamp(10, 100)) / 100.0;
    let out = Command::new(&cfg.ffmpeg)
        .args(["-v", "error", "-y", "-i"])
        .arg(src)
        .args([
            "-vf",
            &format!(
                "scale=w={w}:h={h}:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,colorchannelmixer=aa={alpha}"
            ),
            "-frames:v",
            "1",
            "-update",
            "1",
        ])
        .arg(&tmp)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        tracing::warn!(
            "logo picture {} could not be used: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let _ = tokio::fs::remove_file(&tmp).await;
        return None;
    }
    tokio::fs::rename(&tmp, &path).await.ok()?;
    Some(path)
}

/// Everything about how this item should look and sound.
pub(crate) async fn plan_item(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    item: &PlayItem,
    offset_ms: u64,
) -> ItemPlan {
    let opts = &spec.options;
    let defaults = match item.show.clone() {
        Some(show) => {
            let (source, id) = (spec.source.clone(), spec.id.clone());
            tokio::task::spawn_blocking(move || source.show_defaults(&id, &show))
                .await
                .ok()
                .flatten()
        }
        None => None,
    };
    let eff = opts.for_show_with(item.show.as_deref(), defaults.as_ref());
    let mut plan = ItemPlan {
        look: eff.look,
        aspect: eff.aspect,
        ..ItemPlan::default()
    };
    let tracks = item.tracks.clone().unwrap_or_default();
    plan.deinterlace = match opts.video.deinterlace {
        Deinterlace::Always => true,
        Deinterlace::Off => false,
        Deinterlace::Auto => tracks.interlaced,
    };

    // Audio: only programmes get to choose among tracks; commercials and clips use what they have.
    let mut audio = AudioFx {
        loudness: eff.audio.loudness,
        night: eff.audio.night_mode,
        boost: eff.audio.dialogue_boost,
        mono: eff.audio.mono,
        channels: 2,
    };
    if item.is_show && item.has_audio {
        let choice = choose(&tracks, &eff.audio, &eff.subtitles, spec.can_burn_text);
        plan.audio_index = choice.audio;
        if let Some(t) = choice
            .audio
            .and_then(|i| tracks.audio.iter().find(|t| t.index == i))
        {
            audio.channels = t.channels;
        }
        // Subtitles go on the programme, not on its commercials.
        if let (Some(pick), Some(media)) = (&choice.subtitle, item.path.as_deref()) {
            plan.burn = match pick {
                SubPick::Embedded { index, image: true } => Some(Burn::Image { index: *index }),
                other => text_subtitle(cfg, media, other, &tracks)
                    .await
                    .map(|path| Burn::Text {
                        path,
                        size: eff.subtitles.size,
                        style: eff.subtitles.style,
                    }),
            };
            if plan.burn.is_none() {
                tracing::warn!(channel = %spec.id, "could not prepare the subtitle; airing without it");
            }
        }
    }
    plan.audio = audio;

    // Graphics over the picture.
    let ov = &opts.overlay;
    // The ticker is prepared first: the other graphics keep out of its way.
    let live = {
        let (source, id) = (spec.source.clone(), spec.id.clone());
        tokio::task::spawn_blocking(move || source.live_override(&id))
            .await
            .ok()
            .flatten()
    };
    let alert_bar = matches!(
        &live,
        Some(crate::LiveOverride {
            mode: crate::LiveMode::Alert {
                style: tvmaster_render::alert::AlertStyle::Bar,
                ..
            },
            ..
        })
    );
    let ticker = if alert_bar {
        None
    } else {
        prepare_ticker(cfg, spec, item, ov).await
    };
    let ticker_height = ticker.as_ref().map_or(0, |t| t.height);
    let ticker_top = ticker.as_ref().map(|t| t.top);
    let on_ticker_edge = |c: Corner| {
        ticker_top.is_some_and(|top| top == matches!(c, Corner::TopLeft | Corner::TopRight))
    };
    let logo = match ov.bug.image.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(src) if ov.bug.enabled => logo_png(cfg, Path::new(src), ov.bug.opacity).await,
        _ => None,
    };
    if let Some(png) = logo {
        let (x, y) = corner_xy(
            ov.bug.position,
            24 + if on_ticker_edge(ov.bug.position) {
                ticker_height
            } else {
                0
            },
        );
        plan.overlays.push(OverlayInput {
            png,
            x,
            y,
            enable: None,
        });
    } else if ov.bug.enabled {
        let text = if ov.bug.text.trim().is_empty() {
            format!("{} {}", spec.number, spec.name)
        } else {
            ov.bug.text.clone()
        };
        let th = crate::spec_theme_at(spec, item.start_ms);
        let g = tvmaster_render::overlay::bug_themed(&text, ov.bug.opacity, &th);
        let name = format!(
            "{:016x}.png",
            fnv1a(&format!("{text}|{}|{th:?}", ov.bug.opacity))
        );
        if let Some(png) = graphic(cfg, "bugs", &name, &g.png).await {
            let (x, y) = corner_xy(
                ov.bug.position,
                24 + if on_ticker_edge(ov.bug.position) {
                    ticker_height
                } else {
                    0
                },
            );
            plan.overlays.push(OverlayInput {
                png,
                x,
                y,
                enable: None,
            });
        }
    }
    if ov.show_title.enabled && item.is_show && item.in_ms == 0 && offset_ms == 0 {
        if let Some(title) = item.show.as_deref().filter(|t| !t.is_empty()) {
            let sub = item.subtitle.clone().unwrap_or_default();
            let width = cfg.profile.width * 45 / 100;
            let th = crate::spec_theme_at(spec, item.start_ms);
            let g = tvmaster_render::overlay::lower_third_themed(title, &sub, width, &th);
            let name = format!(
                "{:016x}.png",
                fnv1a(&format!("{title}|{sub}|{width}|{th:?}"))
            );
            if let Some(png) = graphic(cfg, "titles", &name, &g.png).await {
                let secs = ov.show_title.seconds;
                plan.overlays.push(OverlayInput {
                    png,
                    x: "40".into(),
                    y: format!(
                        "main_h-overlay_h-{}",
                        48 + if ticker_top == Some(false) {
                            ticker_height
                        } else {
                            0
                        }
                    ),
                    enable: Some(format!("between(t,0.5,{secs})")),
                });
            }
        }
    }
    // An alert across the bottom of the programme.
    if let Some(crate::LiveOverride {
        mode:
            crate::LiveMode::Alert {
                event,
                text,
                look,
                style: tvmaster_render::alert::AlertStyle::Bar,
                sound,
            },
        started_ms,
    }) = live
    {
        if spec.guide.is_none() && spec.info.is_none() {
            let width = cfg.profile.width;
            let k = width as f32 / 1280.0;
            let bar = tvmaster_render::alert::alert_bar(&look, &event, &text, width);
            let key = format!("{event}|{text}|{look:?}|{width}");
            let (bar_name, crawl_name) = (
                format!("{:016x}-bar.png", fnv1a(&key)),
                format!("{:016x}-crawl.png", fnv1a(&key)),
            );
            let margin = cfg.profile.height * 5 / 100;
            if let (Some(bar_png), Some(crawl_png)) = (
                graphic(cfg, "alerts", &bar_name, &bar.bar.png).await,
                graphic(cfg, "alerts", &crawl_name, &bar.crawl.png).await,
            ) {
                plan.overlays.push(OverlayInput {
                    png: bar_png,
                    x: "0".into(),
                    y: format!("main_h-overlay_h-{margin}"),
                    enable: None,
                });
                let band = bar.band_h;
                plan.overlays.push(OverlayInput {
                    png: crawl_png,
                    x: format!("main_w-mod(t*{:.0}\\,main_w+overlay_w)", 170.0 * k),
                    y: format!("main_h-{margin}-{band}+({band}-overlay_h)/2"),
                    enable: None,
                });
                let into = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
                    - started_ms)
                    .max(0);
                plan.extra_audio = sound.source(into);
            }
        }
    }
    if let Some(t) = ticker {
        plan.overlays.extend(t.overlays);
    }
    if ov.clock.enabled {
        if let Some(font) = &cfg.slate_font {
            // The clock moves to the other edge if the ticker is where it would be.
            let corner = match (
                ov.clock.position,
                ticker_height > 0 && on_ticker_edge(ov.clock.position),
            ) {
                (Corner::TopLeft, true) => Corner::BottomLeft,
                (Corner::TopRight, true) => Corner::BottomRight,
                (Corner::BottomLeft, true) => Corner::TopLeft,
                (Corner::BottomRight, true) => Corner::TopRight,
                (c, _) => c,
            };
            plan.clock = Some(ClockFx {
                font: font.clone(),
                corner,
                hour12: ov.clock.hour12,
            });
        }
    }
    plan
}

/// What the ticker adds to an item: its pictures, where they go, and how tall it is.
struct PreparedTicker {
    overlays: Vec<OverlayInput>,
    height: u32,
    top: bool,
}

/// The ticker for this item, or `None` when it is off, has nothing to say, or the item is not a programme.
async fn prepare_ticker(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    item: &PlayItem,
    ov: &tvmaster_core::OverlayOptions,
) -> Option<PreparedTicker> {
    let tk = &ov.ticker;
    if !tk.enabled || spec.guide.is_some() {
        return None;
    }
    let mut parts: Vec<String> = tk
        .lines
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if !tk.feeds.is_empty() {
        let (source, id) = (spec.source.clone(), spec.id.clone());
        let heads = tokio::task::spawn_blocking(move || source.headlines(&id))
            .await
            .unwrap_or_default();
        parts.extend(heads.into_iter().take(12));
    }
    if tk.up_next {
        let (source, id, after) = (spec.source.clone(), spec.id.clone(), item.end_ms);
        let up = tokio::task::spawn_blocking(move || source.upcoming(&id, after, 1))
            .await
            .unwrap_or_default();
        if let Some((time, title)) = up.items.first() {
            parts.push(format!("Up next: {title} at {time}"));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let text = parts.join("   •   ");
    let th = crate::spec_theme_at(spec, item.start_ms);
    let width = cfg.profile.width;
    let g = tvmaster_render::ticker::ticker(&th, &tk.label, &text, width);
    let key = format!("{text}|{}|{th:?}|{width}", tk.label);
    let h = fnv1a(&key);
    let strip = graphic(cfg, "tickers", &format!("{h:016x}-strip.png"), &g.strip.png).await?;
    let crawl = graphic(cfg, "tickers", &format!("{h:016x}-crawl.png"), &g.crawl.png).await?;
    let chip = match &g.chip {
        Some(c) => graphic(cfg, "tickers", &format!("{h:016x}-chip.png"), &c.png).await,
        None => None,
    };
    let top = tk.position == tvmaster_core::TickerPosition::Top;
    let k = width as f32 / 1280.0;
    let speed = match tk.speed {
        tvmaster_core::TickerSpeed::Slow => 90.0,
        tvmaster_core::TickerSpeed::Normal => 140.0,
        tvmaster_core::TickerSpeed::Fast => 200.0,
    } * k;
    let (sh, ch) = (g.height as i32, g.crawl.h as i32);
    let strip_y = if top {
        "0".to_string()
    } else {
        "main_h-overlay_h".to_string()
    };
    let crawl_y = if top {
        format!("{}", (sh - ch) / 2)
    } else {
        format!("main_h-{}", sh - (sh - ch) / 2)
    };
    // Always on, or appearing now and then and crossing the screen once each time.
    let (crawl_x, enable) = if tk.every_minutes == 0 {
        (format!("main_w-mod(t*{speed:.0}\\,main_w+overlay_w)"), None)
    } else {
        let period = (tk.every_minutes as f32 * 60.0).max(30.0);
        let show = ((width as f32 + g.crawl.w as f32) / speed + 1.0).min(period - 1.0);
        (
            format!("main_w-mod(t\\,{period:.0})*{speed:.0}"),
            Some(format!("lt(mod(t\\,{period:.0})\\,{show:.1})")),
        )
    };
    let mut overlays = vec![
        OverlayInput {
            png: strip,
            x: "0".into(),
            y: strip_y.clone(),
            enable: enable.clone(),
        },
        OverlayInput {
            png: crawl,
            x: crawl_x,
            y: crawl_y,
            enable: enable.clone(),
        },
    ];
    if let Some(chip) = chip {
        overlays.push(OverlayInput {
            png: chip,
            x: "0".into(),
            y: strip_y,
            enable,
        });
    }
    Some(PreparedTicker {
        overlays,
        height: g.height,
        top,
    })
}

impl ItemPlan {
    /// Does this plan add anything that could stop FFmpeg from encoding?
    pub(crate) fn has_extras(&self) -> bool {
        self.burn.is_some()
            || !self.overlays.is_empty()
            || self.clock.is_some()
            || self.extra_audio.is_some()
    }

    /// The same item without subtitles, graphics or clock: what to try when the
    /// fancy version fails, so a problem with an overlay never takes a good file off the air.
    pub(crate) fn plain(&self) -> ItemPlan {
        ItemPlan {
            burn: None,
            overlays: Vec::new(),
            clock: None,
            extra_audio: None,
            ..self.clone()
        }
    }
}
