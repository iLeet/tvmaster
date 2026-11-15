//! Per-channel playout: FFmpeg orchestration, watchdog and HLS output.
//!
//! A channel is driven by a [`ScheduleSource`] (the materialized schedule).
//! Playout always follows the schedule's wall-clock times, covers gaps and
//! unplayable files with a station slate, and resyncs to live if it falls
//! behind, so a channel never goes dark.

mod manager;
mod profile;
mod runner;
mod ts;

pub use manager::Manager;
pub use profile::{item_args, segmenter_args, slate_args, ItemJob, OutputProfile};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tvmaster_media::Encoder;

/// One thing on the schedule, as playout sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayItem {
    /// `None` for scheduled filler (plays the slate).
    pub media_id: Option<i64>,
    pub path: Option<PathBuf>,
    pub has_audio: bool,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Where playout reads its schedule from. Called from blocking-safe threads.
pub trait ScheduleSource: Send + Sync + 'static {
    /// The item on air at `at_ms`, or the next one to start after it.
    fn item_at_or_after(&self, channel: &str, at_ms: i64) -> Option<PlayItem>;
    /// A scheduled file could not be played (missing/corrupt). The schedule
    /// should replace it; playout covers the time with the slate meanwhile.
    fn report_unplayable(&self, channel: &str, media_id: i64);
}

/// Everything the runner needs to put one channel on air.
#[derive(Clone)]
pub struct ChannelSpec {
    pub id: String,
    /// Shown on the station slate.
    pub name: String,
    pub always_on: bool,
    pub source: Arc<dyn ScheduleSource>,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub ffmpeg: PathBuf,
    pub encoder: Encoder,
    pub readrate_burst: bool,
    pub profile: OutputProfile,
    /// Directory under which `hls/<channel>/` output is written.
    pub root: PathBuf,
    /// Stop an on-demand channel after this long with no viewers.
    pub idle_timeout: Duration,
    /// How often idle channels are checked.
    pub reap_interval: Duration,
    /// Font for text on the slate (needs FFmpeg's `drawtext`); `None` = plain slate.
    pub slate_font: Option<PathBuf>,
}

impl EngineConfig {
    pub fn new(ffmpeg: impl Into<PathBuf>, encoder: Encoder, root: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            encoder,
            readrate_burst: false,
            profile: OutputProfile::default(),
            root: root.into(),
            idle_timeout: Duration::from_secs(60),
            reap_interval: Duration::from_secs(5),
            slate_font: None,
        }
    }
}
