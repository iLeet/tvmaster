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
        }
    }
}
