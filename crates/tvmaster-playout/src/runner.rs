//! One channel's playout: a long-running HLS segmenter fed by per-item FFmpeg
//! encodes, wrapped in a watchdog that restarts it on failure.
//!
//! Pacing: items are encoded ahead of real time into a bounded buffer. The
//! segmenter reads its input at 1x, so the buffer fills, back-pressure stalls
//! the encoder, and the next item is already encoded when the current one ends
//! (no gap at item boundaries).

use crate::profile::{item_args, rendered_args, segmenter_args, slate_args, ItemJob};
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
use tvmaster_render::{render, Scene, Theme, FPS, HEIGHT, WIDTH};

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
/// Slate length when the schedule has nothing at all.
const EMPTY_SLATE_MS: u64 = 5_000;
/// Longest single slate piece (keeps the schedule re-checked regularly).
const MAX_SLATE_MS: u64 = 30_000;
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
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let started = Instant::now();
        let result = run_once(&cfg, &spec, &dir, &cancel).await;
        if cancel.is_cancelled() {
            break;
        }
        match result {
            Ok(()) => tracing::warn!(channel = %spec.id, "playout ended unexpectedly"),
            Err(e) => {
                tracing::error!(channel = %spec.id, error = %format!("{e:#}"), "playout failed")
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
    let mut producer = tokio::spawn(produce(cfg.clone(), spec.clone(), tx));

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

        let (source, id) = (spec.source.clone(), spec.id.clone());
        let next = tokio::task::spawn_blocking(move || source.item_at_or_after(&id, vt))
            .await
            .map_err(|e| anyhow!("schedule lookup failed: {e}"))?;

        let Some(item) = next else {
            // Nothing scheduled at all: hold the air with the slate.
            slate(&cfg, &spec, EMPTY_SLATE_MS, ts_offset_ms, &tx, &mut fixer).await?;
            ts_offset_ms += EMPTY_SLATE_MS;
            vt += EMPTY_SLATE_MS as i64;
            continue;
        };

        if item.start_ms > vt {
            // A hole in the schedule: slate until the next item starts.
            let gap = ((item.start_ms - vt) as u64).min(MAX_SLATE_MS);
            slate(&cfg, &spec, gap, ts_offset_ms, &tx, &mut fixer).await?;
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

        match item.kind {
            PlayKind::Slate => {
                slate(&cfg, &spec, remaining, ts_offset_ms, &tx, &mut fixer).await?;
                ts_offset_ms += remaining;
                failures = 0;
            }
            PlayKind::Bumper(kind) => {
                let scene = bumper_scene(&spec, kind, &item).await;
                let theme = Theme::for_channel(&spec.id);
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
                        let job = ItemJob {
                            file: path,
                            offset_ms: item.in_ms + offset,
                            duration_ms: remaining,
                            ts_offset_ms,
                            has_audio: item.has_audio,
                        };
                        let args = item_args(&job, &cfg.profile, cfg.encoder);
                        let outcome = run_encoder(&cfg, args, &tx, &mut fixer).await?;
                        if outcome.bytes > 0 {
                            ts_offset_ms += remaining;
                        }
                        if outcome.ok && outcome.bytes > 0 {
                            failures = 0;
                        } else {
                            tracing::warn!(channel = %spec.id, file = %path.display(), "item failed to encode");
                            report_unplayable(&spec, item.media_id).await;
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
                        failures += 1;
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
    let args = rendered_args(
        WIDTH,
        HEIGHT,
        duration_ms,
        ts_offset_ms,
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
                let mut buf = Vec::with_capacity(n as usize * WIDTH * HEIGHT * 3);
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
    })
}

async fn report_unplayable(spec: &Arc<ChannelSpec>, media_id: Option<i64>) {
    if let Some(media) = media_id {
        let (source, id) = (spec.source.clone(), spec.id.clone());
        let _ = tokio::task::spawn_blocking(move || source.report_unplayable(&id, media)).await;
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
    })
}
