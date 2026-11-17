//! Pure, deterministic domain logic for TVMaster: models, cursors, lineups and
//! the scheduler. No I/O lives here so it can be simulated with a fake clock.

mod ads;
mod catalog;
mod clock;
mod cursor;
mod lineup;
mod model;
mod options;
mod rng;
mod scheduler;
mod showname;
mod timefmt;
mod tracks;

pub use ads::{
    choose_break_points, loose_budget_ms, pack_pod, AdClip, AdConfig, AdInputs, AdPool, AdPreset,
    BumperKind, BumperToggles, AD_PRESETS,
};
pub use catalog::{Catalog, Ep, MediaRef, ShowEps};
pub use clock::Zone;
pub use cursor::EpisodeCursor;
pub use lineup::{
    Advance, Block, CatchUp, Event, Lineup, Order, Season, SignOff, SignOffStyle, Source,
    OFF_AIR_LABEL, SIGNOFF_LABEL,
};
pub use model::{
    parse_hex_color, BackgroundStyle, Channel, ChannelKind, GraphicsStyle, GuideAppearance,
    GuideBackground, GuideColors, GuideConfig, GuideCorners, GuideHeader, GuideLayout, GuidePip,
    GuideScroll, GuideStyle, GuideTextSize, GuideTheme, GuideTicker, GuideTypeface, HeaderStyle,
    PipAudio, PipMode, PipPlacement, PipSize, RunOutPolicy, Strategy, ThemeChoice, TickerPosition,
    TickerSpeed, Tv,
};
pub use options::{
    Aspect, AudioPrefs, BugOptions, ChannelOptions, ClockOptions, Corner, Deinterlace, Effective,
    Look, Loudness, OffAir, OverlayOptions, Quality, Screen, ShowOptions, ShowTitleOptions,
    SubMode, SubPrefs, SubSize, SubStyle, TickerOptions, VideoOptions,
};
pub use rng::fnv1a;
pub use scheduler::{
    align_start, commit_aired, generate, rebuild_reservations, Cursors, ItemKind, SchedConfig,
    ScheduledItem,
};
pub use showname::split_season_suffix;
pub use timefmt::xmltv_time;
pub use tracks::{
    audio_track, choose, choose_audio, choose_subtitle, is_image_codec, norm_lang, sub_track,
    AudioTrack, ExternalSub, SubPick, SubTrack, TrackChoice, Tracks,
};
