//! Per-channel playout: FFmpeg orchestration, watchdog and HLS output.
//!
//! A channel is driven by a [`ScheduleSource`] (the materialized schedule).
//! Playout always follows the schedule's wall-clock times, covers gaps and
//! unplayable files with a station slate, and resyncs to live if it falls
//! behind, so a channel never goes dark.

mod guide;
mod itemplan;
mod looks;
mod manager;
mod probe;
mod profile;
mod runner;
mod sound;
mod ts;

pub use guide::{background_image, guide_look, guide_look_with_pictures};
pub use looks::{audio_chain, AudioFx, Burn, ClockFx, ItemPlan, OverlayInput};
pub use manager::{ChannelHealth, Manager};
pub use probe::{probe_encoder, EncoderProbe};
pub use profile::{
    guide_run_args, item_args, rendered_args, rendered_args_with_audio, screen_args,
    segmenter_args, slate_args, GuideRun, ItemJob, LogoInput, OutputProfile, PipInput, ScreenInput,
};
pub use sound::AlertSound;
pub use tvmaster_render::alert::{AlertLook, AlertStyle};
pub use tvmaster_render::GuideData;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tvmaster_core::BumperKind;
use tvmaster_media::Encoder;

/// What a schedule item is to playout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayKind {
    /// A file: a show (or one act of it), a commercial, or a user's bumper clip.
    Media,
    /// A station bumper drawn by the renderer.
    Bumper(BumperKind),
    /// Filler: the station slate.
    Slate,
    /// The station is off the air for the night: colour bars and a tone, or snow.
    SignOff,
}

/// One thing on the schedule, as playout sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayItem {
    pub kind: PlayKind,
    pub media_id: Option<i64>,
    pub path: Option<PathBuf>,
    pub has_audio: bool,
    /// Where in the file this item starts (acts of a show start part-way in).
    pub in_ms: u64,
    /// For generated bumpers about a show: the show's name.
    pub subject: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    /// This is the programme itself (not a commercial or a clip).
    pub is_show: bool,
    /// The show's name, for per-show settings and the title card.
    pub show: Option<String>,
    /// "S01E02 - Title", for the title card.
    pub subtitle: Option<String>,
    /// The file's audio and subtitle tracks, when it has been scanned for them.
    pub tracks: Option<tvmaster_core::Tracks>,
}

/// What the operator has put on a channel in place of its programming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveMode {
    /// Colour bars and a 1 kHz tone.
    TestScreen,
    /// The channel's own off-air screen (its card, picture, bars or snow).
    OffAir,
    Black,
    Static,
    /// An emergency-alert screen with the attention tones.
    Alert {
        event: String,
        text: String,
        look: AlertLook,
        style: AlertStyle,
        sound: AlertSound,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveOverride {
    pub mode: LiveMode,
    /// When it went on air, so an alert plays its tone once at the start.
    pub started_ms: i64,
}

/// The programmes coming up, formatted for on-screen graphics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Upcoming {
    /// (time, title), soonest first.
    pub items: Vec<(String, String)>,
    /// Show "Coming up tonight" rather than "Later today".
    pub evening: bool,
}

/// Where playout reads its schedule from. Called from blocking-safe threads.
pub trait ScheduleSource: Send + Sync + 'static {
    /// The item on air at `at_ms`, or the next one to start after it.
    fn item_at_or_after(&self, channel: &str, at_ms: i64) -> Option<PlayItem>;
    /// A scheduled file could not be played (missing/corrupt). The schedule
    /// should replace it; playout covers the time with the slate meanwhile.
    fn report_unplayable(&self, channel: &str, media_id: i64);
    /// Up to `limit` programmes starting at or after `after_ms`, for "up next"
    /// and "later today" bumpers. Resolved when the bumper airs, so it always
    /// matches the current schedule.
    fn upcoming(&self, _channel: &str, _after_ms: i64, _limit: usize) -> Upcoming {
        Upcoming::default()
    }
    /// Settings for `show` that apply on every channel (its audio language,
    /// subtitles, look), read when the show airs so a change takes effect on the next item.
    fn show_defaults(&self, _channel: &str, _show: &str) -> Option<tvmaster_core::ShowOptions> {
        None
    }
    /// What the operator has put on this channel instead of its programming, if anything.
    fn live_override(&self, _channel: &str) -> Option<LiveOverride> {
        None
    }
    /// The listing to draw on a guide channel at `now_ms`.
    fn guide_data(&self, _channel: &str, _now_ms: i64) -> Option<GuideData> {
        None
    }
}

/// Lets a guide show another channel live in a window.
pub trait PipHost: Send + Sync + 'static {
    /// Make sure `channel` is on air (starting it if need be) and counted as
    /// watched, and return its HLS playlist once it has video. `None` when it
    /// cannot be shown: unknown, a guide itself, or not up within a few
    /// seconds. Blocks.
    fn open(&self, channel: &str) -> Option<PathBuf>;
}

/// What a guide channel needs besides the schedule data.
#[derive(Clone, Default)]
pub struct GuideSpec {
    /// Music files to loop under the guide (silence if empty).
    pub music: Vec<PathBuf>,
    /// The guide's settings: colours, layout, heading, ticker, video window.
    pub config: tvmaster_core::GuideConfig,
    /// How the video window gets its channel (`None` = no window).
    pub pip_host: Option<Arc<dyn PipHost>>,
}

impl std::fmt::Debug for GuideSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuideSpec")
            .field("music", &self.music)
            .field("config", &self.config)
            .field("pip_host", &self.pip_host.is_some())
            .finish()
    }
}

/// Everything the runner needs to put one channel on air.
#[derive(Clone)]
pub struct ChannelSpec {
    pub id: String,
    /// Shown on the station slate.
    pub name: String,
    pub always_on: bool,
    pub source: Arc<dyn ScheduleSource>,
    /// Set for guide channels, which are rendered instead of scheduled.
    pub guide: Option<GuideSpec>,
    /// Audio, subtitles, picture and graphics.
    pub options: tvmaster_core::ChannelOptions,
    /// The channel number, for the on-screen logo.
    pub number: u32,
    /// FFmpeg can render text subtitles (libass is built in).
    pub can_burn_text: bool,
    /// What airs during the sign-off hours.
    pub signoff: tvmaster_core::SignOffStyle,
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
