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

/// The colours of a guide channel. Each one is a complete look (background,
/// header, cells, ticker); `custom` builds one from two colours you pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideTheme {
    /// Colours derived from the channel, so each guide looks like its own station.
    #[default]
    Auto,
    /// Deep blue with yellow: the classic cable guide.
    Prevue,
    /// Amber on black, like an old monochrome monitor.
    Amber,
    /// Green phosphor.
    Phosphor,
    Neon,
    Sunset,
    Ocean,
    /// Cool grey with a warm accent.
    Slate,
    /// Light, like a printed listing.
    Paper,
    /// From `colors.primary` and `colors.accent`.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GuideColors {
    /// `#rrggbb`, for the `custom` theme.
    #[serde(default)]
    pub primary: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
}

/// How the listing is arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideStyle {
    /// Channels down the side, programmes across a time line.
    #[default]
    Grid,
    /// One big row per channel: what is on now, and what is next.
    NowNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideScroll {
    /// Hold a page of channels, then slide to the next.
    #[default]
    Page,
    /// Roll the list continuously, like the old cable guides.
    Smooth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideLayout {
    #[serde(default)]
    pub style: GuideStyle,
    /// Channels on screen at once (3-10).
    #[serde(default = "default_rows")]
    pub rows: u8,
    #[serde(default)]
    pub scroll: GuideScroll,
    /// Seconds each page stays up (4-30), for `page` scrolling.
    #[serde(default = "default_page_seconds")]
    pub page_seconds: u8,
    /// Minutes between the marks on the time line: 15, 30 or 60.
    #[serde(default = "default_time_step")]
    pub time_step: u8,
    #[serde(default = "yes")]
    pub channel_numbers: bool,
    #[serde(default = "yes")]
    pub channel_names: bool,
    /// A thin bar showing how far through its programme each channel is.
    #[serde(default)]
    pub progress: bool,
    /// CRT scanlines over everything.
    #[serde(default = "yes")]
    pub scanlines: bool,
}

fn yes() -> bool {
    true
}
fn default_rows() -> u8 {
    6
}
fn default_page_seconds() -> u8 {
    9
}
fn default_time_step() -> u8 {
    30
}

impl Default for GuideLayout {
    fn default() -> Self {
        Self {
            style: GuideStyle::Grid,
            rows: default_rows(),
            scroll: GuideScroll::Page,
            page_seconds: default_page_seconds(),
            time_step: default_time_step(),
            channel_numbers: true,
            channel_names: true,
            progress: false,
            scanlines: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideHeader {
    /// The heading text.
    #[serde(default = "yes")]
    pub title: bool,
    #[serde(default = "yes")]
    pub clock: bool,
    #[serde(default = "yes")]
    pub date: bool,
    /// 12-hour clock and times (`9:30 PM`); off = 24-hour (`21:30`).
    #[serde(default = "yes")]
    pub hour12: bool,
    /// A logo picture on the server (png, jpg, gif, webp) at the left of the heading.
    #[serde(default)]
    pub logo: Option<String>,
}

impl Default for GuideHeader {
    fn default() -> Self {
        Self {
            title: true,
            clock: true,
            date: true,
            hour12: true,
            logo: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TickerSpeed {
    Slow,
    #[default]
    Normal,
    Fast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideTicker {
    /// Show the scrolling line along the bottom at all.
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub speed: TickerSpeed,
    /// Announce what is on now ("NOW ON 4: Cheers") between your own lines.
    #[serde(default = "yes")]
    pub now_on: bool,
}

impl Default for GuideTicker {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: TickerSpeed::Normal,
            now_on: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipMode {
    #[default]
    Off,
    /// Always the same channel (`pip.channel`).
    Channel,
    /// Cycle through channels, `pip.seconds` each.
    Rotate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipPlacement {
    /// A band across the top: video on the left, what is on beside it.
    TopLeft,
    TopRight,
    /// Docked at the side of the grid.
    SideLeft,
    #[default]
    SideRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipAudio {
    /// The guide's music keeps playing.
    #[default]
    Music,
    /// You hear the channel in the window.
    Channel,
}

/// A live video window showing one of your channels inside the guide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuidePip {
    #[serde(default)]
    pub mode: PipMode,
    /// For `channel`: which one.
    #[serde(default)]
    pub channel: Option<String>,
    /// For `rotate`: which ones, in order. Empty = every channel in the guide.
    #[serde(default)]
    pub channels: Vec<String>,
    /// Seconds on each channel when rotating (10-300).
    #[serde(default = "default_pip_seconds")]
    pub seconds: u16,
    #[serde(default)]
    pub placement: PipPlacement,
    #[serde(default)]
    pub size: PipSize,
    #[serde(default)]
    pub audio: PipAudio,
    /// A name card (channel, what is on and when) beside the window.
    #[serde(default = "yes")]
    pub info: bool,
}

fn default_pip_seconds() -> u16 {
    30
}

impl Default for GuidePip {
    fn default() -> Self {
        Self {
            mode: PipMode::Off,
            channel: None,
            channels: Vec::new(),
            seconds: default_pip_seconds(),
            placement: PipPlacement::SideRight,
            size: PipSize::Medium,
            audio: PipAudio::Music,
            info: true,
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
    #[serde(default)]
    pub theme: GuideTheme,
    #[serde(default)]
    pub colors: GuideColors,
    #[serde(default)]
    pub layout: GuideLayout,
    #[serde(default)]
    pub header: GuideHeader,
    #[serde(default)]
    pub ticker_options: GuideTicker,
    /// Only these channels (by id), in this order. Empty = every programming channel by number.
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub pip: GuidePip,
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
            theme: GuideTheme::Auto,
            colors: GuideColors::default(),
            layout: GuideLayout::default(),
            header: GuideHeader::default(),
            ticker_options: GuideTicker::default(),
            channels: Vec::new(),
            pip: GuidePip::default(),
        }
    }
}

/// `#rrggbb` (or `rrggbb`) as red, green, blue.
pub fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl GuideConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=6).contains(&self.hours) {
            return Err("guide.hours must be 2-6".into());
        }
        if self.ticker.len() > 20 || self.ticker.iter().any(|t| t.chars().count() > 200) {
            return Err("guide.ticker: at most 20 lines of 200 characters".into());
        }
        if self
            .title
            .as_deref()
            .is_some_and(|t| t.chars().count() > 60 || t.chars().any(char::is_control))
        {
            return Err("guide.title is at most 60 characters".into());
        }
        for (what, c) in [
            ("primary", &self.colors.primary),
            ("accent", &self.colors.accent),
        ] {
            if let Some(c) = c.as_deref().filter(|c| !c.trim().is_empty()) {
                if parse_hex_color(c).is_none() {
                    return Err(format!("guide.colors.{what} must look like #3366ff"));
                }
            }
        }
        let l = &self.layout;
        if !(3..=10).contains(&l.rows) {
            return Err("guide.layout.rows must be 3-10".into());
        }
        if !(4..=30).contains(&l.page_seconds) {
            return Err("guide.layout.page_seconds must be 4-30".into());
        }
        if !matches!(l.time_step, 15 | 30 | 60) {
            return Err("guide.layout.time_step must be 15, 30 or 60".into());
        }
        if let Some(logo) = &self.header.logo {
            if logo.len() > 500 || logo.chars().any(char::is_control) {
                return Err("the guide logo's path is not valid".into());
            }
        }
        if self.channels.len() > 200 || self.channels.iter().any(|c| c.len() > 100) {
            return Err("guide.channels: at most 200 channel ids".into());
        }
        let p = &self.pip;
        if !(10..=300).contains(&p.seconds) {
            return Err("guide.pip.seconds must be 10-300".into());
        }
        if p.channels.len() > 50 || p.channels.iter().any(|c| c.len() > 100) {
            return Err("guide.pip.channels: at most 50 channel ids".into());
        }
        if p.mode == PipMode::Channel && p.channel.as_deref().is_none_or(|c| c.trim().is_empty()) {
            return Err("the video window needs a channel to show".into());
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

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod guide_tests {
    use super::*;

    #[test]
    fn a_guide_saved_before_the_new_options_loads_with_todays_look() {
        let old: GuideConfig = serde_json::from_str(
            r#"{"hours":4,"title":"Retro","ticker":["hi"],"music_library_ids":[2]}"#,
        )
        .unwrap();
        assert_eq!(old.hours, 4);
        assert_eq!(old.theme, GuideTheme::Auto);
        assert_eq!(old.layout, GuideLayout::default());
        assert_eq!(old.layout.rows, 6);
        assert!(old.ticker_options.enabled && old.ticker_options.now_on);
        assert_eq!(old.pip.mode, PipMode::Off);
        assert!(old.validate().is_ok());
        let none: GuideConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(none, GuideConfig::default());
    }

    #[test]
    fn every_option_survives_a_round_trip() {
        let mut g = GuideConfig::default();
        g.theme = GuideTheme::Custom;
        g.colors.primary = Some("#3366ff".into());
        g.layout.style = GuideStyle::NowNext;
        g.layout.scroll = GuideScroll::Smooth;
        g.layout.rows = 8;
        g.header.hour12 = false;
        g.ticker_options.speed = TickerSpeed::Fast;
        g.pip.mode = PipMode::Rotate;
        g.pip.placement = PipPlacement::TopLeft;
        g.pip.audio = PipAudio::Channel;
        let json = serde_json::to_value(&g).unwrap();
        assert_eq!(json["theme"], "custom");
        assert_eq!(json["layout"]["style"], "now_next");
        assert_eq!(json["pip"]["placement"], "top_left");
        assert_eq!(serde_json::from_value::<GuideConfig>(json).unwrap(), g);
    }

    #[test]
    fn nonsense_settings_are_refused_with_a_reason() {
        let bad = |f: &dyn Fn(&mut GuideConfig)| {
            let mut g = GuideConfig::default();
            f(&mut g);
            g.validate().unwrap_err()
        };
        assert!(bad(&|g| g.layout.rows = 2).contains("rows"));
        assert!(bad(&|g| g.layout.rows = 11).contains("rows"));
        assert!(bad(&|g| g.layout.page_seconds = 1).contains("page_seconds"));
        assert!(bad(&|g| g.layout.time_step = 20).contains("time_step"));
        assert!(bad(&|g| g.colors.accent = Some("blue".into())).contains("accent"));
        assert!(bad(&|g| g.pip.seconds = 3).contains("seconds"));
        assert!(bad(&|g| g.pip.mode = PipMode::Channel).contains("channel"));
        assert!(bad(&|g| g.hours = 9).contains("hours"));
        let mut ok = GuideConfig::default();
        ok.pip.mode = PipMode::Channel;
        ok.pip.channel = Some("retro".into());
        ok.colors.primary = Some("#00ff88".into());
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn colours_parse_with_or_without_the_hash() {
        assert_eq!(parse_hex_color("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex_color("00FF7f"), Some([0, 255, 127]));
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#gggggg"), None);
    }
}
