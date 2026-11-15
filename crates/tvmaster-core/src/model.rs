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
