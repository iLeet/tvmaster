//! The guide channel's playout: rendered frames piped into FFmpeg, with the
//! guide's music underneath and, when asked, another channel playing live in a
//! window.
//!
//! Without a window one long encoder run does everything. A window that
//! rotates between channels is a series of runs (an FFmpeg input cannot switch
//! source), joined by continuing the timestamps and the music, so the viewer
//! sees one stream.

use crate::profile::{guide_run_args, GuideRun, LogoInput, PipInput, GUIDE_FPS};
use crate::runner::{cache_music, now_ms};
use crate::ts::ContinuityFixer;
use crate::{ChannelSpec, EngineConfig};
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
use tvmaster_core::{GuideConfig, GuideScroll, GuideStyle, GuideTheme, PipMode, TickerSpeed};
use tvmaster_render::{
    geometry, render_guide, GuideData, GuideLook, GuideView, Palette, Placement, Theme, WindowLook,
    WindowSize, HEIGHT, WIDTH,
};

/// How long the encoder may go without producing anything before the run is
/// abandoned (a video window whose channel stalled would otherwise freeze the guide).
pub(crate) const STALL: Duration = Duration::from_secs(8);
/// After a window's channel fails, leave it alone this long.
const PIP_RETRY: Duration = Duration::from_secs(30);
/// How often the listing is re-read, in stream seconds.
const REFRESH_SECS: u64 = 5;
/// How often a channel shown in the window is told it still has a viewer.
const KEEPALIVE: Duration = Duration::from_secs(10);

/// The palette a theme choice describes.
pub fn palette_for(
    theme: GuideTheme,
    colors: &tvmaster_core::GuideColors,
    channel_id: &str,
) -> Palette {
    match theme {
        GuideTheme::Auto => Palette::from_theme(&Theme::for_channel(channel_id)),
        GuideTheme::Prevue => Palette::prevue(),
        GuideTheme::Amber => Palette::amber(),
        GuideTheme::Phosphor => Palette::phosphor(),
        GuideTheme::Neon => Palette::neon(),
        GuideTheme::Sunset => Palette::sunset(),
        GuideTheme::Ocean => Palette::ocean(),
        GuideTheme::Slate => Palette::slate(),
        GuideTheme::Paper => Palette::paper(),
        GuideTheme::Midnight => Palette::midnight(),
        GuideTheme::Graphite => Palette::graphite(),
        GuideTheme::Snow => Palette::snow(),
        GuideTheme::Forest => Palette::forest(),
        GuideTheme::Berry => Palette::berry(),
        GuideTheme::Mono => Palette::mono(),
        GuideTheme::Kids => Palette::kids(),
        GuideTheme::Bubblegum => Palette::bubblegum(),
        GuideTheme::Lavender => Palette::lavender(),
        GuideTheme::Mint => Palette::mint(),
        GuideTheme::Arctic => Palette::arctic(),
        GuideTheme::Cherry => Palette::cherry(),
        GuideTheme::Gold => Palette::gold(),
        GuideTheme::Coffee => Palette::coffee(),
        GuideTheme::Custom => {
            let auto = Theme::for_channel(channel_id);
            let pick = |s: &Option<String>, fallback| {
                s.as_deref()
                    .and_then(tvmaster_core::parse_hex_color)
                    .unwrap_or(fallback)
            };
            Palette::custom_on(
                pick(&colors.primary, auto.primary),
                pick(&colors.accent, auto.accent),
                colors
                    .background
                    .as_deref()
                    .and_then(tvmaster_core::parse_hex_color),
            )
        }
    }
}

/// The colours and style for a channel's cards, bumpers and screens. With the
/// automatic retro theme this is the original per-channel look, unchanged.
pub fn card_theme(choice: &tvmaster_core::ThemeChoice, channel_id: &str) -> Theme {
    let modern = choice.style == tvmaster_core::GraphicsStyle::Modern;
    if choice.theme == GuideTheme::Auto && !modern {
        return Theme::for_channel(channel_id);
    }
    Theme::from_palette(
        &palette_for(choice.theme, &choice.colors, channel_id),
        modern,
    )
}

/// The look a guide's settings describe.
pub fn guide_look(c: &GuideConfig, channel_id: &str) -> GuideLook {
    let palette = palette_for(c.theme, &c.colors, channel_id);
    let mut look = GuideLook::new(palette);
    look.now_next = c.layout.style == GuideStyle::NowNext;
    look.rows = usize::from(c.layout.rows);
    look.smooth = c.layout.scroll == GuideScroll::Smooth;
    look.page_secs = f32::from(c.layout.page_seconds);
    look.numbers = c.layout.channel_numbers;
    look.names = c.layout.channel_names;
    look.progress = c.layout.progress;
    look.scanlines = c.layout.scanlines;
    look.title = c.header.title;
    look.clock = c.header.clock;
    look.date = c.header.date;
    let a = &c.look;
    look.smooth_text = a.typeface == tvmaster_core::GuideTypeface::Smooth;
    look.rounded = a.corners == tvmaster_core::GuideCorners::Rounded;
    look.text_scale = match a.text_size {
        tvmaster_core::GuideTextSize::Small => 0.8,
        tvmaster_core::GuideTextSize::Medium => 1.0,
        tvmaster_core::GuideTextSize::Large => 1.5,
    };
    look.background = match a.background.style {
        tvmaster_core::BackgroundStyle::Solid => tvmaster_render::Background::Solid,
        tvmaster_core::BackgroundStyle::Stripes => tvmaster_render::Background::Stripes,
        // A picture is loaded separately (see `background_image`); until then, the gradient.
        _ => tvmaster_render::Background::Gradient,
    };
    look.header_minimal = a.header_style == tvmaster_core::HeaderStyle::Minimal;
    look.ticker_top = a.ticker_position == tvmaster_core::TickerPosition::Top;
    look.cell_times = a.cell_times;
    look.now_marker = a.now_marker;
    look.logo = c
        .header
        .logo
        .as_deref()
        .is_some_and(|l| !l.trim().is_empty());
    look.ticker = c.ticker_options.enabled;
    look.ticker_speed = match c.ticker_options.speed {
        TickerSpeed::Slow => 40.0,
        TickerSpeed::Normal => 70.0,
        TickerSpeed::Fast => 120.0,
    };
    if c.pip.mode != PipMode::Off {
        look.window = Some(WindowLook {
            placement: match c.pip.placement {
                tvmaster_core::PipPlacement::TopLeft => Placement::TopLeft,
                tvmaster_core::PipPlacement::TopRight => Placement::TopRight,
                tvmaster_core::PipPlacement::SideLeft => Placement::SideLeft,
                tvmaster_core::PipPlacement::SideRight => Placement::SideRight,
            },
            size: match c.pip.size {
                tvmaster_core::PipSize::Small => WindowSize::Small,
                tvmaster_core::PipSize::Medium => WindowSize::Medium,
                tvmaster_core::PipSize::Large => WindowSize::Large,
            },
            info: c.pip.info,
        });
    }
    look
}

/// The guide's background picture, scaled to fill a `w` x `h` frame (cropping the
/// overflow) and darkened by `dim` percent, as raw RGB. `None` if FFmpeg cannot read it.
/// Blocks (runs FFmpeg once).
pub fn background_image(
    ffmpeg: &Path,
    src: &Path,
    w: usize,
    h: usize,
    dim: u8,
) -> Option<Arc<Vec<u8>>> {
    let keep = 1.0 - f64::from(dim.min(90)) / 100.0;
    let out = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(src)
        .args([
            "-vf",
            &format!(
                "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},colorchannelmixer=rr={keep}:gg={keep}:bb={keep},format=rgb24"
            ),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    (out.status.success() && out.stdout.len() == w * h * 3).then(|| Arc::new(out.stdout))
}

/// [`guide_look`] with the background picture, if the guide has one, loaded.
/// Blocks while FFmpeg reads the picture.
pub fn guide_look_with_pictures(c: &GuideConfig, channel_id: &str, ffmpeg: &Path) -> GuideLook {
    let mut look = guide_look(c, channel_id);
    let bg = &c.look.background;
    if bg.style == tvmaster_core::BackgroundStyle::Image {
        let (w, h) = look.size();
        if let Some(img) = bg
            .image
            .as_deref()
            .filter(|i| !i.trim().is_empty())
            .and_then(|i| background_image(ffmpeg, Path::new(i), w, h, bg.dim))
        {
            look.background = tvmaster_render::Background::Image(img);
        }
    }
    look
}

// ------------------------------------------------------------------ music

/// Stops a background task when dropped, so cancelling the guide (stopping the
/// channel drops its future) also ends the tasks it started.
pub(crate) struct StopOnDrop<T>(pub(crate) tokio::task::JoinHandle<T>);

impl<T> Drop for StopOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The guide's music, converted to one format, with each track's length so a
/// new run can pick up where the last one was.
pub(crate) struct Music {
    tracks: Vec<(PathBuf, f64)>,
    total: f64,
}

/// A media file's length in seconds, as FFmpeg reports it.
async fn duration_secs(ffmpeg: &Path, file: &Path) -> Option<f64> {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-i"])
        .arg(file)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    parse_duration(&String::from_utf8_lossy(&out.stderr))
}

/// "Duration: 00:03:21.45" in FFmpeg's banner.
fn parse_duration(banner: &str) -> Option<f64> {
    let rest = banner.split("Duration: ").nth(1)?;
    let t = rest.split(',').next()?.trim();
    let mut parts = t.split(':');
    let (h, m, s): (f64, f64, f64) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    Some(h * 3600.0 + m * 60.0 + s)
}

pub(crate) async fn load_music(cfg: &EngineConfig, files: &[PathBuf]) -> Option<Music> {
    let mut tracks = Vec::new();
    for f in files.iter().take(24) {
        if let Some(c) = cache_music(cfg, f).await {
            let secs = duration_secs(&cfg.ffmpeg, &c).await.unwrap_or(0.0);
            tracks.push((c, secs));
        }
    }
    if tracks.is_empty() {
        return None;
    }
    let total = tracks.iter().map(|t| t.1).sum();
    Some(Music { tracks, total })
}

/// The concat list that plays the music from `offset` seconds in (wrapping
/// round the whole set), so a later run carries on instead of starting over.
pub(crate) fn music_list_text(m: &Music, offset: f64) -> String {
    let mut at = if m.total > 1.0 {
        offset.rem_euclid(m.total)
    } else {
        0.0
    };
    let mut first = 0;
    for (i, (_, d)) in m.tracks.iter().enumerate() {
        if at < *d || *d <= 0.0 {
            first = i;
            break;
        }
        at -= d;
        first = i;
    }
    let mut list = String::new();
    // Repeated so the list is long even without -stream_loop support.
    for round in 0..20 {
        for k in 0..m.tracks.len() {
            let (path, _) = &m.tracks[(first + k) % m.tracks.len()];
            list.push_str(&format!(
                "file '{}'\n",
                path.to_string_lossy().replace('\'', "'\\''")
            ));
            if round == 0 && k == 0 && at > 0.05 {
                list.push_str(&format!("inpoint {at:.3}\n"));
            }
        }
    }
    list
}

// ------------------------------------------------------------------ the video window

/// Where the window goes in the output picture, in pixels (even, as FFmpeg's
/// pad and overlay like).
fn scale_rect(r: tvmaster_render::Rect, out_w: u32, out_h: u32) -> (i32, i32, i32, i32) {
    let (fx, fy) = (out_w as f32 / WIDTH as f32, out_h as f32 / HEIGHT as f32);
    let even = |v: f32| (v.round() as i32) & !1;
    (
        even(r.x as f32 * fx),
        even(r.y as f32 * fy),
        even(r.w as f32 * fx).max(2),
        even(r.h as f32 * fy).max(2),
    )
}

/// Which channel the window shows next. `rotation` counts how many have been shown.
fn pick_window_channel(
    c: &GuideConfig,
    rows: &[tvmaster_render::GuideRow],
    rotation: usize,
    failed: &HashMap<String, Instant>,
) -> Option<String> {
    let usable = |id: &String| failed.get(id).is_none_or(|t| t.elapsed() >= PIP_RETRY);
    match c.pip.mode {
        PipMode::Off => None,
        PipMode::Channel => c.pip.channel.clone().filter(usable),
        PipMode::Commercials => Some(tvmaster_core::COMMERCIAL_CHANNEL.to_string()).filter(usable),
        PipMode::Rotate => {
            let all: Vec<String> = if c.pip.channels.is_empty() {
                rows.iter().map(|r| r.id.clone()).collect()
            } else {
                c.pip.channels.clone()
            };
            let ok: Vec<&String> = all.iter().filter(|id| usable(id)).collect();
            (!ok.is_empty()).then(|| ok[rotation % ok.len()].clone())
        }
    }
}

// ------------------------------------------------------------------ the run loop

/// What one encoder run wrote.
struct RunResult {
    /// Frames of listing handed to the encoder.
    frames: u64,
    ok: bool,
}

/// Draw the guide channel. The listing is refreshed from the schedule every few
/// seconds of stream time.
pub(crate) async fn produce_guide(
    cfg: Arc<EngineConfig>,
    spec: Arc<ChannelSpec>,
    tx: mpsc::Sender<Vec<u8>>,
) -> Result<()> {
    let gspec = spec.guide.clone().context("not a guide channel")?;
    let config = gspec.config.clone();
    let (ff, cid) = (cfg.ffmpeg.clone(), spec.id.clone());
    let cfg2 = config.clone();
    let look = tokio::task::spawn_blocking(move || guide_look_with_pictures(&cfg2, &cid, &ff))
        .await
        .map_err(|e| anyhow!("loading the guide's look: {e}"))?;
    let geo = geometry(&look);
    let (src_w, src_h) = look.size();
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
    let music = load_music(&cfg, &gspec.music).await;
    let logo = match (&config.header.logo, geo.logo) {
        (Some(path), Some(r)) if !path.trim().is_empty() => {
            let (x, y, w, h) = scale_rect(r, cfg.profile.width, cfg.profile.height);
            crate::itemplan::fit_picture(&cfg, Path::new(path), w as u32, h as u32, 100)
                .await
                .map(|png| (png, x, y))
        }
        _ => None,
    };

    let start_wall = now_ms();
    let mut frames_so_far: u64 = 0;
    // The channels in the guide, for a window that rotates through all of them.
    let mut rows: Vec<tvmaster_render::GuideRow> = Vec::new();
    if config.pip.mode == PipMode::Rotate {
        let (src, ch) = (spec.source.clone(), spec.id.clone());
        if let Ok(Some(d)) =
            tokio::task::spawn_blocking(move || src.guide_data(&ch, now_ms())).await
        {
            rows = d.rows;
        }
    }
    let mut fixer = ContinuityFixer::new();
    let mut failed: HashMap<String, Instant> = HashMap::new();
    let mut rotation = 0usize;
    let mut run_no = 0u32;
    let mut failures = 0u32;

    loop {
        // Which channel is in the window this run?
        let want = pick_window_channel(&config, &rows, rotation, &failed);
        let mut pip: Option<(String, PathBuf)> = None;
        if let (Some(ch), Some(host)) = (want.clone(), gspec.pip_host.clone()) {
            let c2 = ch.clone();
            match tokio::task::spawn_blocking(move || host.open(&c2)).await {
                Ok(Some(playlist)) => pip = Some((ch, playlist)),
                _ => {
                    tracing::info!(channel = %spec.id, window = %ch, "the video window's channel is not available");
                    failed.insert(ch, Instant::now());
                }
            }
        }
        // A window on one channel runs as long as it can; one that rotates, or has
        // nothing to show yet, is cut into runs so it moves on or tries again.
        let secs_to_frames = |s: u64| s * u64::from(GUIDE_FPS);
        let run_frames = match (config.pip.mode, pip.is_some()) {
            (PipMode::Off, _) | (PipMode::Channel | PipMode::Commercials, true) => None,
            (PipMode::Channel | PipMode::Commercials, false) => {
                Some(secs_to_frames(PIP_RETRY.as_secs()))
            }
            (PipMode::Rotate, true) => Some(secs_to_frames(u64::from(config.pip.seconds))),
            (PipMode::Rotate, false) => Some(secs_to_frames(20)),
        };

        // Music carries on from where the stream is.
        let elapsed = frames_so_far as f64 / f64::from(GUIDE_FPS);
        let list = match &music {
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

        let ts_offset_ms = (elapsed * 1000.0) as u64;
        let pip_args = pip.as_ref().and_then(|(_, playlist)| {
            geo.window.map(|r| {
                let (x, y, w, h) = scale_rect(r, cfg.profile.width, cfg.profile.height);
                PipInput {
                    playlist,
                    x,
                    y,
                    w,
                    h,
                    audio: config.pip.audio == tvmaster_core::PipAudio::Channel,
                }
            })
        });
        let args = guide_run_args(
            &GuideRun {
                src_w,
                src_h,
                smooth_scale: look.smooth_text,
                music_list: list.as_deref(),
                pip: pip_args,
                logo: logo
                    .as_ref()
                    .map(|(png, x, y)| LogoInput { png, x: *x, y: *y }),
                duration_ms: run_frames.map(|f| f * 1000 / u64::from(GUIDE_FPS)),
                ts_offset_ms,
            },
            &cfg.profile,
            cfg.encoder,
        );

        // Keep the channel in the window on air while it is being watched.
        let keepalive = match (&pip, gspec.pip_host.clone()) {
            (Some((ch, _)), Some(host)) => {
                let ch = ch.clone();
                Some(StopOnDrop(tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(KEEPALIVE).await;
                        let (h, c) = (host.clone(), ch.clone());
                        let _ = tokio::task::spawn_blocking(move || h.open(&c)).await;
                    }
                })))
            }
            _ => None,
        };

        let window_id = pip.as_ref().map(|(c, _)| c.clone());
        let started = Instant::now();
        let result = run_once(
            &cfg,
            &spec,
            &look,
            window_id.clone(),
            args,
            run_frames,
            frames_so_far,
            start_wall,
            &mut rows,
            &tx,
            &mut fixer,
        )
        .await;
        drop(keepalive);
        let result = result?;
        frames_so_far += result.frames;
        rotation += 1;

        if result.ok {
            failures = 0;
            continue;
        }
        match window_id {
            // The window's channel went wrong: carry on without it for a while.
            Some(ch) => {
                tracing::warn!(channel = %spec.id, window = %ch, "the video window stopped; carrying on without it");
                failed.insert(ch, Instant::now());
            }
            None => {
                failures += 1;
                if failures >= 3 || started.elapsed() > Duration::from_secs(30) {
                    return Err(anyhow!("the guide encoder keeps failing"));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

/// One encoder run: feed it frames, pass its output on. Ends when `frames`
/// frames have been sent (a rotation), when the encoder stops or stalls, or
/// never (a run without a rotation).
#[allow(clippy::too_many_arguments)]
async fn run_once(
    cfg: &EngineConfig,
    spec: &Arc<ChannelSpec>,
    look: &GuideLook,
    window: Option<String>,
    args: Vec<String>,
    run_frames: Option<u64>,
    first_frame: u64,
    start_wall: i64,
    rows: &mut Vec<tvmaster_render::GuideRow>,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<RunResult> {
    const BATCH: u64 = 3;
    let mut child = Command::new(&cfg.ffmpeg)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
    let mut stdin = child.stdin.take().context("guide encoder has no stdin")?;
    let mut out = child.stdout.take().context("guide encoder has no stdout")?;

    let written = Arc::new(AtomicU64::new(0));
    let (source, id) = (spec.source.clone(), spec.id.clone());
    let look = look.clone();
    let ad_window = spec
        .guide
        .as_ref()
        .is_some_and(|g| g.config.pip.mode == PipMode::Commercials);
    let (written_w, window_w) = (written.clone(), window.clone());
    let (row_tx, mut row_rx) = mpsc::unbounded_channel::<Vec<tvmaster_render::GuideRow>>();
    let writer = StopOnDrop(tokio::spawn(async move {
        let mut data = GuideData::default();
        let mut k: u64 = 0;
        loop {
            if let Some(total) = run_frames {
                if k >= total {
                    break;
                }
            }
            let frame = first_frame + k;
            if k.is_multiple_of(REFRESH_SECS * u64::from(GUIDE_FPS)) {
                // "Now" on screen is where the stream is. The encoder runs ahead of
                // real time, so the frame clock leads; when a video window holds the
                // stream back to real time the wall clock does.
                let on_stream = start_wall + (frame as f64 * 1000.0 / f64::from(GUIDE_FPS)) as i64;
                let wall = on_stream.max(now_ms());
                let (src, ch) = (source.clone(), id.clone());
                if let Ok(Some(d)) =
                    tokio::task::spawn_blocking(move || src.guide_data(&ch, wall)).await
                {
                    data = d;
                    let _ = row_tx.send(data.rows.clone());
                }
            }
            let n = run_frames.map_or(BATCH, |t| BATCH.min(t - k));
            let view = GuideView {
                window_row: window_w
                    .as_ref()
                    .and_then(|w| data.rows.iter().position(|r| &r.id == w)),
                window_live: window_w.is_some(),
                window_ad: window_w.is_some() && ad_window,
            };
            let (d, l) = (data.clone(), look.clone());
            let chunk = tokio::task::spawn_blocking(move || {
                let mut buf = Vec::with_capacity(n as usize * l.size().0 * l.size().1 * 3);
                for j in 0..n {
                    let t = (frame + j) as f32 / GUIDE_FPS as f32;
                    buf.extend_from_slice(&render_guide(&d, &l, &view, t).px);
                }
                buf
            })
            .await
            .map_err(std::io::Error::other)?;
            stdin.write_all(&chunk).await?;
            written_w.fetch_add(n, Ordering::Relaxed);
            k += n;
        }
        // Closing stdin lets the encoder finish what it has.
        stdin.shutdown().await.ok();
        drop(stdin);
        Ok::<(), std::io::Error>(())
    }));

    let mut buf = vec![0u8; 64 * 1024];
    let mut ok = true;
    loop {
        while let Ok(r) = row_rx.try_recv() {
            *rows = r;
        }
        let n = match tokio::time::timeout(STALL, out.read(&mut buf)).await {
            Ok(r) => r?,
            Err(_) => {
                tracing::warn!(channel = %spec.id, "the guide encoder stalled");
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
    let status = if ok {
        child.wait().await?
    } else {
        let _ = child.kill().await;
        child.wait().await?
    };
    let frames = written.load(Ordering::Relaxed);
    // A run cut short on purpose is a success; one that was meant to go on forever never is.
    let finished = run_frames.is_some_and(|t| frames >= t);
    Ok(RunResult {
        frames,
        ok: ok && status.success() && finished,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvmaster_core::{PipAudio, PipPlacement, PipSize};

    #[test]
    fn durations_are_read_from_the_banner() {
        let b = "Input #0, aac\n  Duration: 00:03:21.45, bitrate: 128 kb/s\n";
        assert!((parse_duration(b).unwrap() - 201.45).abs() < 1e-6);
        assert_eq!(parse_duration("Duration: N/A, bitrate: N/A"), None);
        assert_eq!(parse_duration("nothing"), None);
    }

    fn music() -> Music {
        Music {
            tracks: vec![
                (PathBuf::from("/c/a.aac"), 100.0),
                (PathBuf::from("/c/b.aac"), 50.0),
                (PathBuf::from("/c/it's.aac"), 30.0),
            ],
            total: 180.0,
        }
    }

    #[test]
    fn music_carries_on_from_where_the_stream_is() {
        let from_start = music_list_text(&music(), 0.0);
        assert!(from_start.starts_with("file '/c/a.aac'\nfile '/c/b.aac'\n"));
        assert!(!from_start.contains("inpoint"), "{from_start}");

        let mid_first = music_list_text(&music(), 40.0);
        assert!(
            mid_first.starts_with("file '/c/a.aac'\ninpoint 40.000\nfile '/c/b.aac'\n"),
            "{mid_first}"
        );

        let in_second = music_list_text(&music(), 130.0);
        assert!(
            in_second.starts_with(
                "file '/c/b.aac'\ninpoint 30.000\nfile '/c/it'\\''s.aac'\nfile '/c/a.aac'\n"
            ),
            "{in_second}"
        );

        // Past the end of the whole set it wraps round.
        let wrapped = music_list_text(&music(), 180.0 + 40.0);
        assert_eq!(wrapped, mid_first);
        // Many repeats, so the list never runs dry.
        assert!(from_start.matches("file '").count() >= 60);
    }

    #[test]
    fn tracks_of_unknown_length_just_start_at_the_top() {
        let m = Music {
            tracks: vec![(PathBuf::from("/c/a.aac"), 0.0)],
            total: 0.0,
        };
        assert!(!music_list_text(&m, 500.0).contains("inpoint"));
    }

    #[test]
    fn the_window_lands_on_even_pixels_at_any_output_size() {
        let r = tvmaster_render::Rect {
            x: 421,
            y: 45,
            w: 192,
            h: 108,
        };
        for (w, h) in [(1280, 720), (854, 480), (640, 360), (1920, 1080)] {
            let (x, y, pw, ph) = scale_rect(r, w, h);
            assert!(
                [x, y, pw, ph].iter().all(|v| v % 2 == 0),
                "{w}x{h}: {x},{y} {pw}x{ph}"
            );
            assert!(x + pw <= w as i32 && y + ph <= h as i32);
            let aspect = pw as f32 / ph as f32;
            assert!((aspect - 16.0 / 9.0).abs() < 0.08, "{w}x{h}: {aspect}");
        }
    }

    fn rows(ids: &[&str]) -> Vec<tvmaster_render::GuideRow> {
        ids.iter()
            .map(|i| tvmaster_render::GuideRow {
                id: i.to_string(),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn the_window_picks_its_channel_by_mode() {
        let mut c = GuideConfig::default();
        let none = HashMap::new();
        assert_eq!(pick_window_channel(&c, &rows(&["a"]), 0, &none), None);

        c.pip.mode = PipMode::Channel;
        c.pip.channel = Some("retro".into());
        assert_eq!(
            pick_window_channel(&c, &[], 5, &none).as_deref(),
            Some("retro")
        );

        c.pip.mode = PipMode::Rotate;
        let r = rows(&["a", "b", "c"]);
        let seq: Vec<_> = (0..4)
            .map(|i| pick_window_channel(&c, &r, i, &none).unwrap())
            .collect();
        assert_eq!(
            seq,
            ["a", "b", "c", "a"],
            "every channel in the guide, in turn"
        );
        c.pip.channels = vec!["z".into(), "y".into()];
        let seq: Vec<_> = (0..3)
            .map(|i| pick_window_channel(&c, &r, i, &none).unwrap())
            .collect();
        assert_eq!(seq, ["z", "y", "z"], "or just the ones chosen");

        // A channel that just failed is skipped for a while.
        c.pip.channels.clear();
        let mut bad = HashMap::new();
        bad.insert("b".to_string(), Instant::now());
        let seq: Vec<_> = (0..4)
            .map(|i| pick_window_channel(&c, &r, i, &bad).unwrap())
            .collect();
        assert_eq!(seq, ["a", "c", "a", "c"]);
        bad.insert("a".into(), Instant::now());
        bad.insert("c".into(), Instant::now());
        assert_eq!(
            pick_window_channel(&c, &r, 0, &bad),
            None,
            "all failed: no window"
        );
    }

    #[test]
    fn the_settings_become_a_look() {
        let mut c = GuideConfig::default();
        let l = guide_look(&c, "g");
        assert_eq!(l.palette, Palette::from_theme(&Theme::for_channel("g")));
        assert!(l.window.is_none() && !l.now_next && l.ticker && l.scanlines && l.rows == 6);

        c.theme = GuideTheme::Amber;
        c.layout.style = GuideStyle::NowNext;
        c.layout.scroll = GuideScroll::Smooth;
        c.layout.rows = 8;
        c.layout.page_seconds = 15;
        c.ticker_options.speed = TickerSpeed::Fast;
        c.ticker_options.enabled = false;
        c.header.logo = Some("/x/logo.png".into());
        c.pip.mode = PipMode::Channel;
        c.pip.placement = PipPlacement::TopLeft;
        c.pip.size = PipSize::Large;
        c.pip.audio = PipAudio::Channel;
        let l = guide_look(&c, "g");
        assert_eq!(l.palette, Palette::amber());
        assert!(l.now_next && l.smooth && l.logo && !l.ticker);
        assert_eq!(
            (l.rows, l.page_secs as u32, l.ticker_speed as u32),
            (8, 15, 120)
        );
        let w = l.window.unwrap();
        assert_eq!(
            (w.placement, w.size),
            (Placement::TopLeft, WindowSize::Large)
        );

        c.theme = GuideTheme::Custom;
        c.colors.primary = Some("#2080ff".into());
        c.colors.accent = Some("not a colour".into());
        let l = guide_look(&c, "g");
        let auto = Theme::for_channel("g");
        assert_eq!(
            l.palette,
            Palette::custom([0x20, 0x80, 0xff], auto.accent),
            "a bad colour falls back"
        );
    }
}
