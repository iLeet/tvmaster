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
    /// Each episode airs once; when a show has run out its slot shows the
    /// channel's off-air screen (the test pattern, if that is what you chose)
    /// instead of repeating or filling with something else.
    OffAir,
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
            RunOutPolicy::OffAir => "off_air",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "keep_going" => RunOutPolicy::KeepGoing,
            "next_in_rotation" => RunOutPolicy::NextInRotation,
            "chaos_fill" => RunOutPolicy::ChaosFill,
            "hybrid" => RunOutPolicy::Hybrid,
            "off_air" => RunOutPolicy::OffAir,
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
    /// An information channel: pages of announcements, headlines, what is on tonight, the time
    /// and pictures, with a clock and a ticker. Nothing to schedule.
    Info,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Programming => "programming",
            ChannelKind::Guide => "guide",
            ChannelKind::Test => "test",
            ChannelKind::Info => "info",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "programming" => Some(ChannelKind::Programming),
            "guide" => Some(ChannelKind::Guide),
            "test" => Some(ChannelKind::Test),
            "info" => Some(ChannelKind::Info),
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
    /// Deep navy with a cyan highlight.
    Midnight,
    /// Charcoal with an orange highlight.
    Graphite,
    /// Clean white with a blue highlight.
    Snow,
    /// Deep green with an amber highlight.
    Forest,
    /// Plum with a pink highlight.
    Berry,
    /// Black and white.
    Mono,
    /// Bright, friendly colours for children: sky blue, sunshine yellow and tangerine. Alerts on
    /// this theme are gentle.
    Kids,
    /// Pink and purple, light.
    Bubblegum,
    /// Soft purple with a pink highlight, light.
    Lavender,
    /// Fresh green with an orange highlight, light.
    Mint,
    /// Pale ice blue with a warm highlight, light.
    Arctic,
    /// Deep red with a gold highlight.
    Cherry,
    /// Black with gold.
    Gold,
    /// Dark brown with a cream highlight.
    Coffee,
    /// A 2000s cable box: blue programme cells with the one on now in orange, a light channel column, cream info panel.
    Cable,
    /// Deep blue for frosted glass, with a white highlight.
    Glass,
    /// Embers: dark red-brown with orange and gold.
    Ember,
    /// Deep teal with a mint highlight.
    Teal,
    /// Plum and blossom pink.
    Sakura,
    /// Warm sand, light.
    Sand,
    /// Royal blue with gold.
    Royal,
    /// Purple with a cyan highlight.
    Vapor,
    /// Near black with white.
    Noir,
    /// Black with bright green text, like an old terminal.
    Terminal,
    /// Dusky purple with a peach highlight.
    Dusk,
    /// Dark green with a lime highlight.
    Lime,
    /// Deep blue with an icy highlight.
    Ice,
    /// Dark wine red with a pale rose highlight.
    Rosewood,
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
    /// For `custom`: the background colour (blank = a dark shade of the main colour).
    #[serde(default)]
    pub background: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideTypeface {
    /// The chunky pixel font, drawn small and scaled up: the retro look.
    #[default]
    Pixel,
    /// A smooth modern typeface, drawn at full 1280x720 resolution.
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideCorners {
    #[default]
    Square,
    Rounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideTextSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundStyle {
    #[default]
    Gradient,
    Solid,
    Stripes,
    /// A picture of your own behind the listing (`background.image`).
    Image,
    /// Soft colour drifting slowly behind frosted-glass panels.
    Aurora,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideBackground {
    #[serde(default)]
    pub style: BackgroundStyle,
    /// A picture on the server (png, jpg, gif, webp), fitted to fill the screen.
    #[serde(default)]
    pub image: Option<String>,
    /// How much the picture is darkened so the listing stays readable, 0-90 percent.
    #[serde(default = "default_dim")]
    pub dim: u8,
}

fn default_dim() -> u8 {
    45
}

impl Default for GuideBackground {
    fn default() -> Self {
        Self {
            style: BackgroundStyle::Gradient,
            image: None,
            dim: default_dim(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderStyle {
    /// A filled band across the top.
    #[default]
    Bar,
    /// Just the words over the background, with a thin line under them.
    Minimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TickerPosition {
    #[default]
    Bottom,
    Top,
}

/// How the guide is drawn, apart from its colours and what it lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideAppearance {
    #[serde(default)]
    pub typeface: GuideTypeface,
    #[serde(default)]
    pub corners: GuideCorners,
    /// The size of the programme and channel text.
    #[serde(default)]
    pub text_size: GuideTextSize,
    #[serde(default)]
    pub background: GuideBackground,
    #[serde(default)]
    pub header_style: HeaderStyle,
    #[serde(default)]
    pub ticker_position: TickerPosition,
    /// Show each programme's start and end under its name when there is room.
    #[serde(default)]
    pub cell_times: bool,
    /// The vertical line at "now" on the time grid.
    #[serde(default = "yes")]
    pub now_marker: bool,
    /// Frosted glass: cells, channel names and cards are see-through panels with a bright edge.
    /// Looks best on the soft-colour background and with smooth type.
    #[serde(default)]
    pub glass: bool,
}

impl Default for GuideAppearance {
    fn default() -> Self {
        Self {
            typeface: GuideTypeface::Pixel,
            corners: GuideCorners::Square,
            text_size: GuideTextSize::Medium,
            background: GuideBackground::default(),
            header_style: HeaderStyle::Bar,
            ticker_position: TickerPosition::Bottom,
            cell_times: false,
            now_marker: true,
            glass: false,
        }
    }
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
    /// Only commercials, from `pip.libraries`, in the window. They play from a light, internal
    /// stream of their own that is not a channel (no number, not listed anywhere).
    Commercials,
    /// Channels and commercials taking turns: a channel for `seconds`, then commercials for
    /// `commercial_seconds`, then the next channel.
    Mix,
}

/// The id of the channel that plays only commercials, made for a guide's "Commercials only" window.
pub const COMMERCIAL_CHANNEL: &str = "commercial-break";

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
    /// For `commercials` and `mix`: which commercial libraries it plays. Empty = every commercials library.
    #[serde(default)]
    pub libraries: Vec<i64>,
    /// For `mix`: seconds of commercials between channels (10-300).
    #[serde(default = "default_commercial_seconds")]
    pub commercial_seconds: u16,
    /// The card beside a commercial window: its heading. Blank = "AD BREAK".
    #[serde(default)]
    pub ad_title: String,
    /// The lines on that card, two at a time, turning over every `ad_rotate_seconds`: station
    /// promos, announcements, anything. Blank = the station's two standard lines. `{clock}`
    /// and `{date}` are filled in.
    #[serde(default)]
    pub ad_lines: Vec<String>,
    /// Seconds each pair of lines stays up (2-60).
    #[serde(default = "default_ad_rotate")]
    pub ad_rotate_seconds: u16,
}

fn default_commercial_seconds() -> u16 {
    30
}

fn default_ad_rotate() -> u16 {
    6
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
            libraries: Vec::new(),
            commercial_seconds: default_commercial_seconds(),
            ad_title: String::new(),
            ad_lines: Vec::new(),
            ad_rotate_seconds: default_ad_rotate(),
        }
    }
}

/// How the station's graphics are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsStyle {
    /// Chunky pixel type, scanlines and a classic broadcast feel.
    #[default]
    Retro,
    /// Smooth type, soft panels and clean animation at full resolution.
    Modern,
}

/// One look for everything the station draws itself: the guide, station and
/// break cards, the stand-by and sign-off screens, alerts, the now-playing
/// card, the channel logo and the clock. Set once for the station, and a
/// channel may choose its own.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ThemeChoice {
    /// `auto` = colours derived from each channel, so every channel has its own.
    #[serde(default)]
    pub theme: GuideTheme,
    /// For the `custom` theme.
    #[serde(default)]
    pub colors: GuideColors,
    #[serde(default)]
    pub style: GraphicsStyle,
    /// Alerts are gentle: soft colours and words, a soft chime and nothing flashing, so a child is
    /// not frightened. The kids theme is always gentle.
    #[serde(default)]
    pub gentle: bool,
}

impl ThemeChoice {
    /// Alerts in this look are gentle.
    pub fn is_gentle(&self) -> bool {
        self.gentle || self.theme == GuideTheme::Kids
    }

    pub fn validate(&self) -> Result<(), String> {
        for (what, c) in [
            ("primary", &self.colors.primary),
            ("accent", &self.colors.accent),
            ("background", &self.colors.background),
        ] {
            if let Some(c) = c.as_deref().filter(|c| !c.trim().is_empty()) {
                if parse_hex_color(c).is_none() {
                    return Err(format!("theme.colors.{what} must look like #3366ff"));
                }
            }
        }
        Ok(())
    }

    /// A guide that follows the station theme (its own theme is `auto`) takes
    /// the station's colours and style; one with its own theme is left alone.
    pub fn apply_to_guide(&self, g: &mut GuideConfig) {
        if g.theme != GuideTheme::Auto {
            return;
        }
        g.theme = self.theme;
        g.colors = self.colors.clone();
        if self.style == GraphicsStyle::Modern {
            g.look.typeface = GuideTypeface::Smooth;
            g.look.corners = GuideCorners::Rounded;
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
    pub look: GuideAppearance,
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
            look: GuideAppearance::default(),
            header: GuideHeader::default(),
            ticker_options: GuideTicker::default(),
            channels: Vec::new(),
            pip: GuidePip::default(),
        }
    }
}

// ------------------------------------------------------------------ info channels

/// What a page on an info channel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideKind {
    /// A title and some lines of your own, with a picture if you like.
    #[default]
    Text,
    /// The latest headlines from the channel's news feeds.
    Headlines,
    /// What is on now, and what comes next, on every channel.
    Tonight,
    /// The time and the date, large.
    Clock,
    /// A picture, full screen, with a caption.
    Photo,
}

impl SlideKind {
    pub const ALL: [(&'static str, &'static str, &'static str); 5] = [
        (
            "text",
            "Notice",
            "A title and a few lines, and a picture if you like",
        ),
        (
            "headlines",
            "Headlines",
            "The latest from the channel's news feeds",
        ),
        (
            "tonight",
            "On tonight",
            "What is on now and next, on every channel",
        ),
        ("clock", "Clock", "The time and the date, large"),
        (
            "photo",
            "Picture",
            "A picture across the whole screen, with a caption",
        ),
    ];
}

/// One page of an info channel.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InfoSlide {
    #[serde(default)]
    pub kind: SlideKind,
    /// The page's title (or the caption of a picture).
    #[serde(default)]
    pub title: String,
    /// Lines of text, for a notice.
    #[serde(default)]
    pub lines: Vec<String>,
    /// A picture on the server (png, jpg, gif, webp): beside a notice's text, or across the screen
    /// for a picture page.
    #[serde(default)]
    pub picture: Option<String>,
    /// How long this page stays up, in seconds (3-300). Blank = the channel's own.
    #[serde(default)]
    pub seconds: Option<u16>,
}

/// An information channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoConfig {
    /// What the channel is, in a few words, for the guide ("Local news and headlines"). Blank = made
    /// from the pages.
    #[serde(default)]
    pub about: Option<String>,
    /// The heading; blank = the channel's name.
    #[serde(default)]
    pub title: Option<String>,
    /// The pages, in order, round and round. None = the clock.
    #[serde(default)]
    pub slides: Vec<InfoSlide>,
    /// How long each page stays up, in seconds (3-120).
    #[serde(default = "default_slide_seconds")]
    pub seconds: u16,
    /// News feeds (RSS or Atom addresses) for the headlines and the ticker.
    #[serde(default)]
    pub feeds: Vec<String>,
    /// Lines for the ticker along the bottom.
    #[serde(default)]
    pub ticker: Vec<String>,
    /// The scrolling line along the bottom: your lines and the headlines.
    #[serde(default = "yes")]
    pub ticker_enabled: bool,
    /// The time and date at the top right.
    #[serde(default = "yes")]
    pub clock: bool,
    /// 12-hour clock (`9:30 PM`); off = 24-hour.
    #[serde(default = "yes")]
    pub hour12: bool,
    /// Libraries (kind `music`) for background music. Silence if empty.
    #[serde(default)]
    pub music_library_ids: Vec<i64>,
}

fn default_slide_seconds() -> u16 {
    12
}

impl Default for InfoConfig {
    fn default() -> Self {
        Self {
            about: None,
            title: None,
            slides: Vec::new(),
            seconds: default_slide_seconds(),
            feeds: Vec::new(),
            ticker: Vec::new(),
            ticker_enabled: true,
            clock: true,
            hour12: true,
            music_library_ids: Vec::new(),
        }
    }
}

impl InfoConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(3..=120).contains(&self.seconds) {
            return Err("info.seconds must be 3-120".into());
        }
        if self.slides.len() > 40 {
            return Err("info.slides: at most 40 pages".into());
        }
        for (i, s) in self.slides.iter().enumerate() {
            let n = i + 1;
            if s.title.chars().count() > 80 || s.title.chars().any(char::is_control) {
                return Err(format!("page {n}: the title is at most 80 characters"));
            }
            if s.lines.len() > 12 || s.lines.iter().any(|l| l.chars().count() > 200) {
                return Err(format!("page {n}: at most 12 lines of 200 characters"));
            }
            if s.seconds.is_some_and(|v| !(3..=300).contains(&v)) {
                return Err(format!("page {n}: seconds must be 3-300"));
            }
            if let Some(p) = &s.picture {
                if p.len() > 500 || p.chars().any(char::is_control) {
                    return Err(format!("page {n}: the picture's path is not valid"));
                }
            }
            if s.kind == SlideKind::Photo
                && s.picture.as_deref().is_none_or(|p| p.trim().is_empty())
            {
                return Err(format!("page {n}: a picture page needs a picture"));
            }
            if s.kind == SlideKind::Text
                && s.title.trim().is_empty()
                && s.lines.iter().all(|l| l.trim().is_empty())
            {
                return Err(format!("page {n}: a notice needs a title or some text"));
            }
        }
        if self.slides.iter().any(|s| s.kind == SlideKind::Headlines) && self.feeds.is_empty() {
            return Err("a headlines page needs at least one news feed".into());
        }
        if self.feeds.len() > 8
            || self.feeds.iter().any(|f| {
                f.len() > 500
                    || !(f.starts_with("http://") || f.starts_with("https://"))
                    || f.chars().any(|c| c.is_control() || c.is_whitespace())
            })
        {
            return Err("info.feeds: up to 8 addresses starting with http:// or https://".into());
        }
        if self.ticker.len() > 20 || self.ticker.iter().any(|t| t.chars().count() > 200) {
            return Err("info.ticker: at most 20 lines of 200 characters".into());
        }
        if self
            .title
            .as_deref()
            .is_some_and(|t| t.chars().count() > 60 || t.chars().any(char::is_control))
        {
            return Err("info.title is at most 60 characters".into());
        }
        if self
            .about
            .as_deref()
            .is_some_and(|t| t.chars().count() > 80 || t.chars().any(char::is_control))
        {
            return Err("info.about is at most 80 characters".into());
        }
        Ok(())
    }

    /// What the guide says this channel is.
    pub fn describe(&self) -> String {
        if let Some(a) = self
            .about
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            return a.to_string();
        }
        let has = |k: SlideKind| self.slides.iter().any(|s| s.kind == k);
        let mut parts = Vec::new();
        if has(SlideKind::Headlines) {
            parts.push("Headlines");
        }
        if has(SlideKind::Tonight) {
            parts.push("What's on tonight");
        }
        if has(SlideKind::Text) {
            parts.push("Notices");
        }
        if has(SlideKind::Photo) {
            parts.push("Pictures");
        }
        if parts.is_empty() || (parts.is_empty() && has(SlideKind::Clock)) {
            return "Time and date".to_string();
        }
        parts.join(", ")
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
            ("background", &self.colors.background),
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
        let bg = &self.look.background;
        if bg.dim > 90 {
            return Err("guide.look.background.dim must be 0-90".into());
        }
        if let Some(img) = &bg.image {
            if img.len() > 500 || img.chars().any(char::is_control) {
                return Err("the guide background picture's path is not valid".into());
            }
        }
        if bg.style == BackgroundStyle::Image
            && bg.image.as_deref().is_none_or(|i| i.trim().is_empty())
        {
            return Err("a picture background needs a picture".into());
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
        if p.libraries.len() > 50 {
            return Err("guide.pip.libraries: at most 50 libraries".into());
        }
        if !(10..=300).contains(&p.commercial_seconds) {
            return Err("guide.pip.commercial_seconds must be 10-300".into());
        }
        if !(2..=60).contains(&p.ad_rotate_seconds) {
            return Err("guide.pip.ad_rotate_seconds must be 2-60".into());
        }
        if p.ad_title.chars().count() > 40 {
            return Err("the commercial card's heading is at most 40 letters".into());
        }
        if p.ad_lines.len() > 40 || p.ad_lines.iter().any(|l| l.chars().count() > 120) {
            return Err("the commercial card has at most 40 lines of at most 120 letters".into());
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
    /// Used when `kind` is `info`.
    #[serde(default)]
    pub info: InfoConfig,
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
        assert_eq!(old.look, GuideAppearance::default());
        assert_eq!(old.look.typeface, GuideTypeface::Pixel);
        assert!(old.look.now_marker && !old.look.cell_times);
        assert_eq!(old.look.background.style, BackgroundStyle::Gradient);
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
        g.look.typeface = GuideTypeface::Smooth;
        g.look.corners = GuideCorners::Rounded;
        g.look.text_size = GuideTextSize::Large;
        g.look.background.style = BackgroundStyle::Image;
        g.look.background.image = Some("/data/assets/ab-bg.jpg".into());
        g.look.header_style = HeaderStyle::Minimal;
        g.look.ticker_position = TickerPosition::Top;
        g.look.cell_times = true;
        g.look.now_marker = false;
        g.colors.background = Some("#101830".into());
        let json = serde_json::to_value(&g).unwrap();
        assert_eq!(json["theme"], "custom");
        assert_eq!(json["layout"]["style"], "now_next");
        assert_eq!(json["pip"]["placement"], "top_left");
        assert_eq!(json["look"]["typeface"], "smooth");
        assert_eq!(json["look"]["background"]["style"], "image");
        assert_eq!(json["look"]["ticker_position"], "top");
        assert_eq!(json["colors"]["background"], "#101830");
        assert_eq!(
            serde_json::from_str::<GuideConfig>(r#"{"theme":"midnight"}"#)
                .unwrap()
                .theme,
            GuideTheme::Midnight
        );
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
        assert!(bad(&|g| g.look.background.dim = 95).contains("dim"));
        assert!(bad(&|g| g.look.background.style = BackgroundStyle::Image).contains("picture"));
        assert!(bad(&|g| g.colors.background = Some("navy".into())).contains("background"));
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
