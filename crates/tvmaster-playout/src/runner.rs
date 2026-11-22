//! One channel's playout: a long-running HLS segmenter fed by per-item FFmpeg
//! encodes, wrapped in a watchdog that restarts it on failure.
//!
//! Pacing: items are encoded ahead of real time into a bounded buffer. The
//! segmenter reads its input at 1x, so the buffer fills, back-pressure stalls
//! the encoder, and the next item is already encoded when the current one ends
//! (no gap at item boundaries).

use crate::itemplan::plan_item;
use crate::profile::{
    item_args, rendered_args, screen_args, segmenter_args, slate_args, ItemJob, ScreenInput,
};
use crate::ts::ContinuityFixer;
use crate::{ChannelSpec, EngineConfig, PlayItem, PlayKind};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tvmaster_core::BumperKind;
use tvmaster_render::{render, Scene, Theme, FPS};

/// Buffered MPEG-TS chunks between the encoder and the segmenter.
const CHUNK_BUFFER: usize = 256;
/// Skip slivers shorter than this (end of an item we tuned into).
const MIN_ITEM_MS: u64 = 250;
/// If the schedule position falls this far behind the wall clock (encoder
/// stall, slow CPU), jump forward to live instead of staying late forever.
const RESYNC_BEHIND_MS: i64 = 5_000;
/// If it runs this far ahead (an item produced less than planned), step back
/// to a normal buffer depth so the stream does not drift ahead of the guide.
const RESYNC_AHEAD_MS: i64 = 120_000;
const RESYNC_AHEAD_TARGET_MS: i64 = 30_000;
/// Never encode further ahead than this. Real video is held back by the full
/// buffer, but a slate is tiny and would race through minutes of schedule
/// while a drive is unplugged, leaving a long stretch of slate queued behind
/// it that airs even after the files are back.
const MAX_LEAD_MS: i64 = 45_000;
/// Slate length when the schedule has nothing at all.
const EMPTY_SLATE_MS: u64 = 5_000;
/// Longest single slate piece (keeps the schedule re-checked regularly).
const MAX_SLATE_MS: u64 = 30_000;
/// An override is aired in pieces this long, so clearing or changing it is noticed promptly.
const LIVE_CHUNK_MS: u64 = 5_000;
/// Consecutive unplayable items before giving up and letting the watchdog
/// restart (with backoff) instead of spinning.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Run the channel until cancelled, restarting on failure with backoff.
pub(crate) async fn run_forever(
    cfg: Arc<EngineConfig>,
    spec: Arc<ChannelSpec>,
    dir: std::path::PathBuf,
    cancel: CancellationToken,
    health: Arc<crate::manager::Health>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let started = Instant::now();
        let result = run_once(&cfg, &spec, &dir, &cancel).await;
        if cancel.is_cancelled() {
            break;
        }
        match result {
            Ok(()) => {
                tracing::warn!(channel = %spec.id, "playout ended unexpectedly");
                health.record_failure("playout ended unexpectedly".into());
            }
            Err(e) => {
                tracing::error!(channel = %spec.id, error = %format!("{e:#}"), "playout failed");
                health.record_failure(format!("{e:#}"));
            }
        }
        // Stop serving the dead run's playlist while we wait to restart.
        let _ = tokio::fs::remove_dir_all(&dir).await;
        if started.elapsed() > Duration::from_secs(60) {
            backoff = Duration::from_secs(1);
        } else {
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
        tracing::info!(channel = %spec.id, ?backoff, "restarting playout");
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = cancel.cancelled() => break,
        }
    }
    tracing::info!(channel = %spec.id, "playout stopped");
}

async fn clear_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        tokio::fs::remove_dir_all(dir).await.ok();
    }
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("creating {}", dir.display()))
}

async fn run_once(
    cfg: &Arc<EngineConfig>,
    spec: &Arc<ChannelSpec>,
    dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    clear_dir(dir).await?;
    tracing::info!(channel = %spec.id, "starting playout");
    // This channel's own picture size, frame rate and bitrate.
    let channel_cfg = Arc::new(EngineConfig {
        profile: cfg.profile.for_channel(&spec.options.video),
        ..(**cfg).clone()
    });
    let cfg = &channel_cfg;

    let mut seg = Command::new(&cfg.ffmpeg)
        .args(segmenter_args(dir, &cfg.profile, cfg.readrate_burst))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
    let mut stdin = seg.stdin.take().context("segmenter has no stdin")?;

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(CHUNK_BUFFER);
    let mut writer = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            stdin.write_all(&chunk).await?;
        }
        stdin.shutdown().await.ok();
        Ok::<(), std::io::Error>(())
    });
    // A guide channel the operator has taken over is aired like any other channel until it is given back.
    let taken_over = spec.source.live_override(&spec.id).is_some();
    let mut producer = if spec.guide.is_some() && !taken_over {
        tokio::spawn(crate::guide::produce_guide(cfg.clone(), spec.clone(), tx))
    } else if spec.info.is_some() && !taken_over {
        tokio::spawn(crate::info::produce_info(cfg.clone(), spec.clone(), tx))
    } else {
        tokio::spawn(produce(cfg.clone(), spec.clone(), tx))
    };

    let result = tokio::select! {
        _ = cancel.cancelled() => Ok(()),
        r = &mut producer => match r {
            Ok(Ok(())) => Err(anyhow!("producer ended")),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(anyhow!("producer task failed: {e}")),
        },
        r = &mut writer => Err(anyhow!("segmenter input closed: {r:?}")),
        s = seg.wait() => Err(anyhow!("segmenter exited: {s:?}")),
    };
    producer.abort();
    writer.abort();
    let _ = seg.kill().await;
    result
}

struct Outcome {
    bytes: u64,
    ok: bool,
    /// FFmpeg was killed (out of memory, a user, a crash), as opposed to
    /// exiting with an error about the file. The file is not at fault.
    signaled: bool,
}

/// Follow the schedule from "now", encoding each item (or the slate) and
/// sending its MPEG-TS into `tx`. Output timestamps stay continuous across
/// items; the schedule position `vt` follows the schedule's wall-clock times.
async fn produce(
    cfg: Arc<EngineConfig>,
    spec: Arc<ChannelSpec>,
    tx: mpsc::Sender<Vec<u8>>,
) -> Result<()> {
    let mut vt = now_ms();
    let mut ts_offset_ms: u64 = 0;
    let mut failures: u32 = 0;
    let mut fixer = ContinuityFixer::new();

    loop {
        let wall = now_ms();
        if vt < wall - RESYNC_BEHIND_MS {
            tracing::warn!(channel = %spec.id, behind_ms = wall - vt, "behind live, skipping ahead");
            vt = wall;
        } else if vt > wall + RESYNC_AHEAD_MS {
            tracing::warn!(channel = %spec.id, ahead_ms = vt - wall, "ahead of schedule, stepping back");
            vt = wall + RESYNC_AHEAD_TARGET_MS;
        }

        if vt > wall + MAX_LEAD_MS {
            tokio::time::sleep(Duration::from_millis((vt - wall - MAX_LEAD_MS) as u64)).await;
            continue;
        }

        // The operator has taken the channel over (test screen, emergency alert...).
        let live = {
            let (source, id) = (spec.source.clone(), spec.id.clone());
            tokio::task::spawn_blocking(move || source.live_override(&id))
                .await
                .map_err(|e| anyhow!("override lookup failed: {e}"))?
        };
        // A bar alert rides over the programme; everything else replaces the picture.
        let bar_alert = live.as_ref().filter(|o| {
            matches!(
                &o.mode,
                crate::LiveMode::Alert {
                    style: tvmaster_render::alert::AlertStyle::Bar,
                    ..
                }
            ) && spec.guide.is_none()
                && spec.info.is_none()
        });
        if let Some(ov) = live.as_ref().filter(|_| bar_alert.is_none()) {
            let chunk = LIVE_CHUNK_MS;
            live_screen(&cfg, &spec, ov, chunk, ts_offset_ms, &tx, &mut fixer).await?;
            ts_offset_ms += chunk;
            vt += chunk as i64;
            continue;
        }

        let (source, id) = (spec.source.clone(), spec.id.clone());
        let next = tokio::task::spawn_blocking(move || source.item_at_or_after(&id, vt))
            .await
            .map_err(|e| anyhow!("schedule lookup failed: {e}"))?;

        let Some(item) = next else {
            if let Some(ov) = bar_alert {
                live_screen(
                    &cfg,
                    &spec,
                    ov,
                    EMPTY_SLATE_MS,
                    ts_offset_ms,
                    &tx,
                    &mut fixer,
                )
                .await?;
                ts_offset_ms += EMPTY_SLATE_MS;
                vt += EMPTY_SLATE_MS as i64;
                continue;
            }
            // Nothing scheduled at all: hold the air with the slate.
            slate(&cfg, &spec, EMPTY_SLATE_MS, ts_offset_ms, &tx, &mut fixer).await?;
            ts_offset_ms += EMPTY_SLATE_MS;
            vt += EMPTY_SLATE_MS as i64;
            continue;
        };

        if item.start_ms > vt {
            // A hole in the schedule: slate until the next item starts.
            let gap = ((item.start_ms - vt) as u64).min(MAX_SLATE_MS);
            match bar_alert {
                Some(ov) => {
                    live_screen(&cfg, &spec, ov, gap, ts_offset_ms, &tx, &mut fixer).await?
                }
                None => slate(&cfg, &spec, gap, ts_offset_ms, &tx, &mut fixer).await?,
            }
            ts_offset_ms += gap;
            vt += gap as i64;
            continue;
        }

        let offset = (vt - item.start_ms) as u64;
        let remaining = (item.end_ms - vt).max(0) as u64;
        if remaining < MIN_ITEM_MS {
            vt = item.end_ms;
            continue;
        }

        // Nothing to lay a bar over (a stand-by screen, a bumper): the alert takes the whole screen.
        if let Some(ov) = bar_alert.filter(|_| item.kind != PlayKind::Media) {
            let chunk = LIVE_CHUNK_MS;
            live_screen(&cfg, &spec, ov, chunk, ts_offset_ms, &tx, &mut fixer).await?;
            ts_offset_ms += chunk;
            vt += chunk as i64;
            continue;
        }

        match item.kind {
            PlayKind::Slate => {
                // In pieces, so a plan changed while the stand-by screen is up is noticed within seconds.
                let piece = remaining.min(MAX_SLATE_MS);
                slate(&cfg, &spec, piece, ts_offset_ms, &tx, &mut fixer).await?;
                ts_offset_ms += piece;
                vt += piece as i64;
                failures = 0;
                continue;
            }
            PlayKind::SignOff => {
                // Bars and snow come with a tone and hiss, the way a station signed off.
                let tone = matches!(
                    spec.signoff,
                    tvmaster_core::Screen::Bars | tvmaster_core::Screen::Static
                );
                off_air_screen_as(
                    "signoff",
                    &cfg,
                    &spec,
                    spec.signoff,
                    tone,
                    remaining,
                    ts_offset_ms,
                    &tx,
                    &mut fixer,
                )
                .await?;
                ts_offset_ms += remaining;
                failures = 0;
            }
            PlayKind::Bumper(kind) => {
                let scene = bumper_scene(&spec, kind, &item).await;
                let theme = crate::spec_theme_at(&spec, item.start_ms);
                // The owner's own design for this screen, if there is one.
                let (scene, theme) = if spec.options.screens.is_empty() {
                    (scene, theme)
                } else {
                    let (ff, sp, sc, th, at) = (
                        cfg.ffmpeg.clone(),
                        spec.clone(),
                        scene.clone(),
                        theme,
                        item.start_ms,
                    );
                    tokio::task::spawn_blocking(move || {
                        crate::screens::designed_scene(&ff, &sp, &sc, &th, at)
                    })
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or((scene, theme))
                };
                let outcome = run_rendered(
                    &cfg,
                    scene,
                    theme,
                    offset,
                    remaining,
                    ts_offset_ms,
                    &tx,
                    &mut fixer,
                )
                .await?;
                if outcome.bytes > 0 {
                    ts_offset_ms += remaining;
                } else {
                    tracing::warn!(channel = %spec.id, ?kind, "bumper failed to render, airing the slate");
                    slate(&cfg, &spec, remaining, ts_offset_ms, &tx, &mut fixer).await?;
                    ts_offset_ms += remaining;
                }
                failures = 0;
            }
            PlayKind::Media => {
                let playable = item.path.as_deref().filter(|p| p.exists());
                match (item.media_id, playable) {
                    (Some(_), Some(path)) => {
                        let plan = plan_item(&cfg, &spec, &item, offset).await;
                        let mut job = ItemJob {
                            file: path,
                            offset_ms: item.in_ms + offset,
                            duration_ms: remaining,
                            ts_offset_ms,
                            has_audio: item.has_audio,
                            plan,
                        };
                        let args = item_args(&job, &cfg.profile, cfg.encoder);
                        let mut outcome = run_encoder(&cfg, args, &tx, &mut fixer).await?;
                        // Subtitles, logo or clock are extras: if the item fails to
                        // encode with them, try again without, so a problem with an
                        // extra never takes a good file off the air.
                        if !outcome.ok
                            && outcome.bytes == 0
                            && !outcome.signaled
                            && job.plan.has_extras()
                        {
                            tracing::warn!(channel = %spec.id, file = %path.display(), "item failed with its subtitles/graphics; retrying without them");
                            job.plan = job.plan.plain();
                            let args = item_args(&job, &cfg.profile, cfg.encoder);
                            outcome = run_encoder(&cfg, args, &tx, &mut fixer).await?;
                        }
                        if outcome.bytes > 0 {
                            ts_offset_ms += remaining;
                        }
                        if outcome.ok && outcome.bytes > 0 {
                            failures = 0;
                        } else {
                            tracing::warn!(channel = %spec.id, file = %path.display(), signaled = outcome.signaled, "item failed to encode");
                            // An encoder that was killed says nothing about the
                            // file; only a file that really fails is taken off the air.
                            if !outcome.signaled {
                                report_unplayable(&spec, item.media_id).await;
                            }
                            if outcome.bytes == 0 {
                                slate(&cfg, &spec, remaining, ts_offset_ms, &tx, &mut fixer)
                                    .await?;
                                ts_offset_ms += remaining;
                            }
                            failures += 1;
                        }
                    }
                    (media, _) => {
                        tracing::warn!(channel = %spec.id, ?media, "file missing, airing the slate");
                        report_unplayable(&spec, media).await;
                        slate(&cfg, &spec, remaining, ts_offset_ms, &tx, &mut fixer).await?;
                        ts_offset_ms += remaining;
                        // Not counted as a failure: an unplugged drive can take
                        // many items in a row, the slate covers each for its full
                        // length (so this cannot spin), and restarting the channel
                        // would only take the stream off the air.
                    }
                }
            }
        }
        if failures >= MAX_CONSECUTIVE_FAILURES {
            bail!("{failures} consecutive unplayable items");
        }
        vt = item.end_ms;
    }
}

/// The graphic for a bumper, with its words filled in from the live schedule.
async fn bumper_scene(spec: &Arc<ChannelSpec>, kind: BumperKind, item: &PlayItem) -> Scene {
    let name = spec.name.clone();
    let show = item.subject.clone().unwrap_or_default();
    match kind {
        BumperKind::StationId => Scene::StationId { name },
        BumperKind::Brb => Scene::Brb { name, show },
        BumperKind::BackTo => Scene::BackTo { name, show },
        BumperKind::UpNext | BumperKind::LaterToday => {
            let (source, id, after) = (spec.source.clone(), spec.id.clone(), item.end_ms);
            let limit = if kind == BumperKind::UpNext { 1 } else { 4 };
            let up = tokio::task::spawn_blocking(move || source.upcoming(&id, after, limit))
                .await
                .unwrap_or_default();
            match (kind, up.items.first()) {
                (_, None) => Scene::StationId { name },
                (BumperKind::UpNext, Some((time, title))) => Scene::UpNext {
                    name,
                    title: title.clone(),
                    time: time.clone(),
                },
                _ => Scene::LaterToday {
                    name,
                    evening: up.evening,
                    items: up.items,
                },
            }
        }
    }
}

/// Encode a rendered bumper: frames are drawn on a blocking thread and fed to
/// FFmpeg, starting `offset_ms` into the bumper (if someone tuned in mid-way).
#[allow(clippy::too_many_arguments)]
async fn run_rendered(
    cfg: &EngineConfig,
    scene: Scene,
    theme: Theme,
    offset_ms: u64,
    duration_ms: u64,
    ts_offset_ms: u64,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<Outcome> {
    let total_secs = (offset_ms + duration_ms) as f32 / 1000.0;
    let frames = (duration_ms as f64 / 1000.0 * FPS as f64).ceil() as u64;
    let (fw, fh) = theme.frame_size();
    let args = rendered_args(fw, fh, duration_ms, ts_offset_ms, &cfg.profile, cfg.encoder);
    let mut child = Command::new(&cfg.ffmpeg)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
    let mut stdin = child.stdin.take().context("encoder has no stdin")?;
    let mut out = child.stdout.take().context("encoder has no stdout")?;

    // Draw in small batches so the async runtime is never blocked.
    let writer = tokio::spawn(async move {
        const BATCH: u64 = 6;
        let mut i = 0;
        while i < frames {
            let n = BATCH.min(frames - i);
            let (scene, theme) = (scene.clone(), theme);
            let start = i;
            let chunk = tokio::task::spawn_blocking(move || {
                let mut buf = Vec::with_capacity(n as usize * fw * fh * 3);
                for k in 0..n {
                    let t = offset_ms as f32 / 1000.0 + (start + k) as f32 / FPS as f32;
                    buf.extend_from_slice(&render(&scene, &theme, t, total_secs).px);
                }
                buf
            })
            .await
            .map_err(std::io::Error::other)?;
            stdin.write_all(&chunk).await?;
            i += n;
        }
        stdin.shutdown().await.ok();
        Ok::<(), std::io::Error>(())
    });

    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let n = out.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        let packets = fixer.process(&buf[..n]);
        if !packets.is_empty() {
            tx.send(packets)
                .await
                .map_err(|_| anyhow!("segmenter writer closed"))?;
        }
    }
    fixer.end_item();
    let status = child.wait().await?;
    writer.abort();
    Ok(Outcome {
        bytes,
        ok: status.success(),
        signaled: status.code().is_none(),
    })
}

/// Convert a music file to a common format in the cache so different files
/// can be joined in one playlist. Returns the cached path.
pub(crate) async fn cache_music(cfg: &EngineConfig, src: &Path) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = tokio::fs::metadata(src).await.ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, meta.len()).hash(&mut h);
    let dir = cfg.root.join("cache").join("music");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let out = dir.join(format!("{:016x}.aac", h.finish()));
    if tokio::fs::metadata(&out)
        .await
        .map(|m| m.len() > 0)
        .unwrap_or(false)
    {
        return Some(out);
    }
    let tmp = out.with_extension("part");
    let status = Command::new(&cfg.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args([
            "-vn", "-ac", "2", "-ar", "48000", "-c:a", "aac", "-b:a", "128k", "-f", "adts",
        ])
        .arg(&tmp)
        .stdin(Stdio::null())
        .status()
        .await
        .ok()?;
    if !status.success() {
        let _ = tokio::fs::remove_file(&tmp).await;
        tracing::warn!(file = %src.display(), "skipping unreadable music file");
        return None;
    }
    tokio::fs::rename(&tmp, &out).await.ok()?;
    Some(out)
}

async fn report_unplayable(spec: &Arc<ChannelSpec>, media_id: Option<i64>) {
    if let Some(media) = media_id {
        let (source, id) = (spec.source.clone(), spec.id.clone());
        let _ = tokio::task::spawn_blocking(move || source.report_unplayable(&id, media)).await;
    }
}

/// Hold the air with whatever the channel shows when nothing is on: its off-air
/// screen (a station card unless the owner chose bars, snow, black or a picture).
/// One piece of an operator override.
async fn live_screen(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    ov: &crate::LiveOverride,
    duration_ms: u64,
    ts_offset_ms: u64,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<()> {
    use crate::LiveMode;
    use tvmaster_core::Screen;
    match &ov.mode {
        LiveMode::TestScreen => {
            off_air_screen(
                cfg,
                spec,
                Screen::Bars,
                true,
                duration_ms,
                ts_offset_ms,
                tx,
                fixer,
            )
            .await
        }
        LiveMode::OffAir => slate(cfg, spec, duration_ms, ts_offset_ms, tx, fixer).await,
        LiveMode::Black => {
            off_air_screen(
                cfg,
                spec,
                Screen::Black,
                false,
                duration_ms,
                ts_offset_ms,
                tx,
                fixer,
            )
            .await
        }
        LiveMode::Static => {
            off_air_screen(
                cfg,
                spec,
                Screen::Static,
                true,
                duration_ms,
                ts_offset_ms,
                tx,
                fixer,
            )
            .await
        }
        LiveMode::Alert {
            event,
            text,
            look,
            sound,
            ..
        } => {
            let into = (now_ms() - ov.started_ms).max(0);
            let audio = sound
                .source(into)
                .unwrap_or_else(|| "anullsrc=r=48000:cl=stereo".to_string());
            let (look, event, text) = (*look, event.clone(), text.clone());
            let (w, h) = tvmaster_render::alert::screen_size(&look);
            let frames = (duration_ms as f64 / 1000.0 * FPS as f64).ceil() as u64;
            let args = crate::rendered_args_with_audio(
                w,
                h,
                duration_ms,
                ts_offset_ms,
                &cfg.profile,
                cfg.encoder,
                &audio,
            );
            let mut child = Command::new(&cfg.ffmpeg)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
            let mut stdin = child.stdin.take().context("encoder has no stdin")?;
            let mut out = child.stdout.take().context("encoder has no stdout")?;
            let offset_s = (now_ms() - ov.started_ms).max(0) as f32 / 1000.0;
            let writer = tokio::spawn(async move {
                let chunk = tokio::task::spawn_blocking(move || {
                    let mut buf = Vec::with_capacity(frames as usize * w * h * 3);
                    for k in 0..frames {
                        let t = offset_s + k as f32 / FPS as f32;
                        buf.extend_from_slice(
                            &tvmaster_render::alert::alert_screen(&look, &event, &text, t).px,
                        );
                    }
                    buf
                })
                .await
                .map_err(std::io::Error::other)?;
                stdin.write_all(&chunk).await?;
                stdin.shutdown().await.ok();
                Ok::<(), std::io::Error>(())
            });
            let mut buf = vec![0u8; 64 * 1024];
            let mut bytes = 0u64;
            loop {
                let n = out.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                let packets = fixer.process(&buf[..n]);
                if !packets.is_empty() {
                    tx.send(packets)
                        .await
                        .map_err(|_| anyhow!("segmenter writer closed"))?;
                }
            }
            let _ = child.wait().await;
            writer.abort();
            if bytes == 0 {
                tracing::warn!(channel = %spec.id, "alert failed to render, airing the test screen");
                return off_air_screen(
                    cfg,
                    spec,
                    Screen::Bars,
                    true,
                    duration_ms,
                    ts_offset_ms,
                    tx,
                    fixer,
                )
                .await;
            }
            Ok(())
        }
    }
}

async fn slate(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    duration_ms: u64,
    ts_offset_ms: u64,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<()> {
    let oa = &spec.options.off_air;
    off_air_screen(
        cfg,
        spec,
        oa.screen,
        oa.tone,
        duration_ms,
        ts_offset_ms,
        tx,
        fixer,
    )
    .await
}

/// The station card as a picture, made once per channel and message.
async fn card_png(cfg: &EngineConfig, spec: &ChannelSpec) -> Option<std::path::PathBuf> {
    let message = spec.options.off_air.message.clone();
    let theme = crate::spec_theme_at(spec, now_ms());
    let key = format!(
        "{}|{}|{}|{}|{:?}",
        spec.id, spec.number, spec.name, message, theme
    );
    let dir = cfg.root.join("cache").join("cards");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(format!("{:016x}.png", tvmaster_core::fnv1a(&key)));
    if !path.exists() {
        let g = tvmaster_render::overlay::station_card(&theme, spec.number, &spec.name, &message);
        let tmp = path.with_extension("part");
        tokio::fs::write(&tmp, &g.png).await.ok()?;
        tokio::fs::rename(&tmp, &path).await.ok()?;
    }
    Some(path)
}

/// Air one of the off-air screens for `duration_ms`. If it cannot be made (a
/// missing picture, a failing filter), fall back to the plain slate so the
/// channel never goes dark.
#[allow(clippy::too_many_arguments)]
async fn off_air_screen(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    screen: tvmaster_core::Screen,
    tone: bool,
    duration_ms: u64,
    ts_offset_ms: u64,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<()> {
    off_air_screen_as(
        "standby",
        cfg,
        spec,
        screen,
        tone,
        duration_ms,
        ts_offset_ms,
        tx,
        fixer,
    )
    .await
}

/// The same, for the sign-off hours (`kind` = "signoff") or for stand-by: where the owner has
/// designed that screen and the screen chosen is the station card, the design airs instead.
#[allow(clippy::too_many_arguments)]
async fn off_air_screen_as(
    kind: &str,
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    screen: tvmaster_core::Screen,
    tone: bool,
    duration_ms: u64,
    ts_offset_ms: u64,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<()> {
    use tvmaster_core::Screen;
    if screen == Screen::Card && !spec.options.screens.is_empty() {
        let design = crate::screens::design_for(spec, kind)
            .or_else(|| crate::screens::design_for(spec, "standby"));
        if let Some(design) = design {
            let (ff, d) = (cfg.ffmpeg.clone(), design.clone());
            let images =
                tokio::task::spawn_blocking(move || crate::screens::load_pictures(&ff, &d))
                    .await
                    .unwrap_or_default();
            let mut theme = crate::spec_theme_at(spec, now_ms());
            theme.modern = true;
            let scene = Scene::Design {
                design,
                vars: crate::screens::channel_vars(spec, now_ms()),
                images: Arc::new(images),
            };
            let outcome =
                run_rendered(cfg, scene, theme, 0, duration_ms, ts_offset_ms, tx, fixer).await?;
            if outcome.ok && outcome.bytes > 0 {
                return Ok(());
            }
            tracing::warn!(channel = %spec.id, kind, "designed screen failed to render, using the plain card");
        }
    }
    let picture = match screen {
        Screen::Image => spec
            .options
            .off_air
            .image
            .as_deref()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
            .map(|p| (p, false)),
        Screen::Card => card_png(cfg, spec).await.map(|p| (p, true)),
        _ => None,
    };
    let args = match (screen, &picture) {
        (Screen::Bars, _) => Some(screen_args(
            ScreenInput::Bars,
            tone,
            duration_ms,
            ts_offset_ms,
            &cfg.profile,
            cfg.encoder,
        )),
        (Screen::Static, _) => Some(screen_args(
            ScreenInput::Static,
            tone,
            duration_ms,
            ts_offset_ms,
            &cfg.profile,
            cfg.encoder,
        )),
        (Screen::Black, _) => Some(screen_args(
            ScreenInput::Black,
            tone,
            duration_ms,
            ts_offset_ms,
            &cfg.profile,
            cfg.encoder,
        )),
        (Screen::Card | Screen::Image, Some((path, pixel_art))) => Some(screen_args(
            ScreenInput::Picture {
                path,
                pixel_art: *pixel_art,
            },
            tone,
            duration_ms,
            ts_offset_ms,
            &cfg.profile,
            cfg.encoder,
        )),
        // A picture that is gone: the card, below.
        (Screen::Card | Screen::Image, None) => None,
    };
    if let Some(args) = args {
        let outcome = run_encoder(cfg, args, tx, fixer).await?;
        if outcome.ok && outcome.bytes > 0 {
            return Ok(());
        }
        tracing::warn!(channel = %spec.id, ?screen, "off-air screen failed to encode, using the plain slate");
    }
    let args = slate_args(
        &spec.name,
        duration_ms,
        ts_offset_ms,
        &cfg.profile,
        cfg.encoder,
        cfg.slate_font.as_deref(),
    );
    let outcome = run_encoder(cfg, args, tx, fixer).await?;
    if outcome.ok && outcome.bytes > 0 {
        Ok(())
    } else {
        // Without even the slate there is nothing to air: restart via the watchdog.
        bail!("the station slate failed to encode")
    }
}

async fn run_encoder(
    cfg: &EngineConfig,
    args: Vec<String>,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<Outcome> {
    let mut child = Command::new(&cfg.ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
    let mut out = child.stdout.take().context("encoder has no stdout")?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let n = out.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        let packets = fixer.process(&buf[..n]);
        if !packets.is_empty() {
            tx.send(packets)
                .await
                .map_err(|_| anyhow!("segmenter writer closed"))?;
        }
    }
    fixer.end_item();
    let status = child.wait().await?;
    Ok(Outcome {
        bytes,
        ok: status.success(),
        signaled: status.code().is_none(),
    })
}
