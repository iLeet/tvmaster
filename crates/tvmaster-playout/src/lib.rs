//! Per-channel playout: FFmpeg orchestration, watchdog and HLS output.

mod manager;
mod profile;
mod runner;
mod ts;

pub use manager::Manager;
pub use profile::{item_args, segmenter_args, ItemJob, OutputProfile};

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tvmaster_core::Timeline;
use tvmaster_media::Encoder;

/// A playable source file.
#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub has_audio: bool,
}

/// Everything the runner needs to put one channel on air.
#[derive(Debug, Clone)]
pub struct ChannelSpec {
    pub id: String,
    pub always_on: bool,
    pub timeline: Timeline,
    pub files: HashMap<i64, MediaFile>,
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
        }
    }
}
