use crate::lineup::Lineup;
use serde::{Deserialize, Serialize};

/// How a channel picks what to air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    /// Weighted random across the whole library.
    Chaos,
    /// Random episodes per show.
    Shuffle,
    /// Episodes in order, remembering where it left off.
    #[default]
    Sequential,
    /// Time slots on a 30/60 minute grid.
    BlockGrid,
}

/// What happens when a show or pool runs out of unaired episodes (Autopilot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutPolicy {
    /// Continue in order and wrap to the first episode (default).
    #[default]
    KeepGoing,
    /// Hand the slot to the next show in the rotation queue.
    NextInRotation,
    /// Fall back to random content.
    ChaosFill,
    /// Sequential where possible, chaos for gaps and overflow.
    Hybrid,
}

impl Strategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Chaos => "chaos",
            Strategy::Shuffle => "shuffle",
            Strategy::Sequential => "sequential",
            Strategy::BlockGrid => "block_grid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "chaos" => Strategy::Chaos,
            "shuffle" => Strategy::Shuffle,
            "sequential" => Strategy::Sequential,
            "block_grid" => Strategy::BlockGrid,
            _ => return None,
        })
    }
}

impl RunOutPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            RunOutPolicy::KeepGoing => "keep_going",
            RunOutPolicy::NextInRotation => "next_in_rotation",
            RunOutPolicy::ChaosFill => "chaos_fill",
            RunOutPolicy::Hybrid => "hybrid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "keep_going" => RunOutPolicy::KeepGoing,
            "next_in_rotation" => RunOutPolicy::NextInRotation,
            "chaos_fill" => RunOutPolicy::ChaosFill,
            "hybrid" => RunOutPolicy::Hybrid,
            _ => return None,
        })
    }
}

/// What a channel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// Airs shows from the library on a schedule.
    #[default]
    Programming,
    /// The guide channel: a rendered, scrolling listing of every channel.
    Guide,
    /// A test-pattern channel: always airs its off-air screen (bars and a tone,
    /// snow, a card or a picture of your own). Nothing to schedule.
    Test,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Programming => "programming",
            ChannelKind::Guide => "guide",
            ChannelKind::Test => "test",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "programming" => Some(ChannelKind::Programming),
            "guide" => Some(ChannelKind::Guide),
            "test" => Some(ChannelKind::Test),
            _ => None,
        }
    }
}

/// Settings for a guide channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideConfig {
    /// How many hours ahead the grid shows (2-6).
    #[serde(default = "default_guide_hours")]
    pub hours: u32,
    /// Heading; blank = "<channel name> GUIDE".
    #[serde(default)]
    pub title: Option<String>,
    /// Extra lines for the scrolling ticker (promos, announcements). The ticker
    /// also announces what is on now.
    #[serde(default)]
    pub ticker: Vec<String>,
    /// Libraries (kind `music`) for background music. Silence if empty.
    #[serde(default)]
    pub music_library_ids: Vec<i64>,
}

fn default_guide_hours() -> u32 {
    3
}

impl Default for GuideConfig {
    fn default() -> Self {
        Self {
            hours: default_guide_hours(),
            title: None,
            ticker: Vec::new(),
            music_library_ids: Vec::new(),
        }
    }
}

impl GuideConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=6).contains(&self.hours) {
            return Err("guide.hours must be 2-6".into());
        }
        if self.ticker.len() > 20 || self.ticker.iter().any(|t| t.chars().count() > 200) {
            return Err("guide.ticker: at most 20 lines of 200 characters".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub number: u32,
    pub name: String,
    pub strategy: Strategy,
    pub run_out: RunOutPolicy,
    /// Autopilot on: edits do not re-roll the timeline automatically.
    pub autopilot: bool,
    /// 0 (fully structured) to 100 (complete chaos).
    pub chaos: u8,
    /// Seed for deterministic shuffling.
    pub seed: u64,
    /// The channel "started broadcasting" at this instant (epoch ms).
    pub epoch_ms: i64,
    /// Keep encoding even with no viewers (otherwise starts on first viewer).
    pub always_on: bool,
    /// Libraries this channel draws programming from.
    pub library_ids: Vec<i64>,
    /// Block grid, seasons, events and fill. Empty = just follow the strategy.
    pub lineup: Lineup,
    #[serde(default)]
    pub kind: ChannelKind,
    /// A group for organising channels ("Kids", "Movies"); shown in the
    /// playlist and the web player.
    #[serde(default)]
    pub group: Option<String>,
    /// Used when `kind` is `guide`.
    #[serde(default)]
    pub guide: GuideConfig,
    /// Audio tracks, subtitles, picture and on-screen graphics.
    #[serde(default)]
    pub options: crate::options::ChannelOptions,
}

/// A registered player (a TV) that a remote can control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tv {
    pub id: String,
    pub name: String,
    pub channel: Option<u32>,
    pub power: bool,
    pub volume: u8,
    pub muted: bool,
    /// A player is connected right now (not stored).
    #[serde(default)]
    pub online: bool,
}

impl Tv {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            channel: None,
            power: false,
            volume: 50,
            muted: false,
            online: false,
        }
    }
}
