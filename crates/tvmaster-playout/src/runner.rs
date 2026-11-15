//! One channel's playout: a long-running HLS segmenter fed by per-item FFmpeg
//! encodes, wrapped in a watchdog that restarts it on failure.
//!
//! Pacing: items are encoded ahead of real time into a bounded buffer. The
//! segmenter reads its input at 1x, so the buffer fills, back-pressure stalls
//! the encoder, and the next item is already encoded when the current one ends
//! (no gap at item boundaries).

use crate::profile::{item_args, segmenter_args, ItemJob};
use crate::ts::ContinuityFixer;
use crate::{ChannelSpec, EngineConfig};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Buffered MPEG-TS chunks between the encoder and the segmenter.
const CHUNK_BUFFER: usize = 256;
/// Skip slivers shorter than this (end of an item we tuned into).
const MIN_ITEM_MS: u64 = 500;
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

/// Walk the timeline from "now", encoding each item and sending its MPEG-TS
/// into `tx`. Output timestamps stay continuous across items.
async fn produce(
    cfg: Arc<EngineConfig>,
    spec: Arc<ChannelSpec>,
    tx: mpsc::Sender<Vec<u8>>,
) -> Result<()> {
    let start_now = now_ms();
    let mut ts_offset_ms: u64 = 0;
    let mut first = true;
    let mut failures: u32 = 0;
    let mut fixer = ContinuityFixer::new();

    for slot in spec.timeline.iter_from(start_now) {
        let offset = if first { slot.offset_at(start_now) } else { 0 };
        first = false;
        let remaining = slot.duration_ms().saturating_sub(offset);
        if remaining < MIN_ITEM_MS {
            tokio::task::yield_now().await;
            continue;
        }

        // Fallback ladder (M2 level): unplayable item -> skip to the next one.
        let playable = spec.files.get(&slot.media_id).filter(|f| f.path.exists());
        let Some(file) = playable else {
            tracing::warn!(channel = %spec.id, media = slot.media_id, "file missing, skipping");
            failures += 1;
            if failures >= MAX_CONSECUTIVE_FAILURES {
                bail!("{failures} consecutive unplayable items");
            }
            tokio::task::yield_now().await;
            continue;
        };

        let job = ItemJob {
            file: &file.path,
            offset_ms: offset,
            duration_ms: remaining,
            ts_offset_ms,
            has_audio: file.has_audio,
        };
        let outcome = encode_item(&cfg, &job, &tx, &mut fixer).await?;
        if outcome.bytes > 0 {
            ts_offset_ms += remaining;
        }
        if outcome.ok && outcome.bytes > 0 {
            failures = 0;
        } else {
            tracing::warn!(channel = %spec.id, file = %file.path.display(), "item failed to encode");
            failures += 1;
            if failures >= MAX_CONSECUTIVE_FAILURES {
                bail!("{failures} consecutive failed items");
            }
        }
    }
    Ok(())
}

async fn encode_item(
    cfg: &EngineConfig,
    job: &ItemJob<'_>,
    tx: &mpsc::Sender<Vec<u8>>,
    fixer: &mut ContinuityFixer,
) -> Result<Outcome> {
    let mut child = Command::new(&cfg.ffmpeg)
        .args(item_args(job, &cfg.profile, cfg.encoder))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", cfg.ffmpeg.display()))?;
    let mut out = child.stdout.take().context("item encoder has no stdout")?;
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
