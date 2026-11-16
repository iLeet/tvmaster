//! Per-channel picture, sound and presentation options: what a real TV
//! station would configure for how its channel looks and sounds.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn yes() -> bool {
    true
}

/// How loud things are relative to each other. Real stations keep shows and
/// commercials at one level; files from different sources are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loudness {
    /// Leave levels as they are.
    #[default]
    Off,
    /// Gentle automatic levelling (cheap on the CPU).
    Light,
    /// EBU R128 broadcast loudness: the same perceived volume everywhere.
    Broadcast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPrefs {
    /// Languages in order of preference (`eng`, `en`, `English` all work). Empty
    /// = the file's own default track.
    #[serde(default)]
    pub languages: Vec<String>,
    /// Skip director's-commentary tracks (unless they are all there is).
    #[serde(default = "yes")]
    pub avoid_commentary: bool,
    /// Skip audio-description tracks (the narrated-picture ones for blind viewers).
    #[serde(default = "yes")]
    pub avoid_description: bool,
    #[serde(default)]
    pub loudness: Loudness,
    /// Compress the dynamic range so explosions do not wake the house.
    #[serde(default)]
    pub night_mode: bool,
    /// Lift speech when mixing surround down to stereo.
    #[serde(default)]
    pub dialogue_boost: bool,
    /// One speaker, like a 1960s set.
    #[serde(default)]
    pub mono: bool,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self {
            languages: Vec::new(),
            avoid_commentary: true,
            avoid_description: true,
            loudness: Loudness::Off,
            night_mode: false,
            dialogue_boost: false,
            mono: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubMode {
    #[default]
    Off,
    /// Always, in the preferred language.
    Always,
    /// Only forced subtitles (foreign phrases and signs inside the programme).
    Forced,
    /// Subtitles exactly when the audio is not in a language you chose.
    Foreign,
    /// Like `always`, preferring subtitles for the deaf and hard of hearing.
    Sdh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubStyle {
    /// White text with a dark outline.
    #[default]
    Outline,
    /// White text on a translucent box, like closed captions.
    Box,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubPrefs {
    #[serde(default)]
    pub mode: SubMode,
    /// Languages to show, in order. Empty = the audio preferences' languages.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub size: SubSize,
    #[serde(default)]
    pub style: SubStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// The whole picture, with bars where the shapes differ.
    #[default]
    Fit,
    /// Fill the screen, cropping the edges.
    Fill,
    /// Stretch to fill (distorts).
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deinterlace {
    /// Only when the file says it is interlaced.
    #[default]
    Auto,
    Off,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "480")]
    P480,
    #[default]
    #[serde(rename = "720")]
    P720,
    #[serde(rename = "1080")]
    P1080,
}

impl Quality {
    /// Width, height and a sensible bitrate in kbit/s.
    pub fn size(self) -> (u32, u32, u32) {
        match self {
            Quality::P480 => (854, 480, 1500),
            Quality::P720 => (1280, 720, 3000),
            Quality::P1080 => (1920, 1080, 6000),
        }
    }
}

/// A picture treatment applied to everything the channel airs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    #[default]
    None,
    /// Soft, noisy, a little washed: a worn tape.
    Vhs,
    /// Scanlines and a vignette, like a tube TV.
    Crt,
    BlackWhite,
    Sepia,
    /// Grain and warmth.
    Film,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VideoOptions {
    #[serde(default)]
    pub aspect: Aspect,
    #[serde(default)]
    pub deinterlace: Deinterlace,
    #[serde(default)]
    pub quality: Quality,
    /// Frame rate; blank = 30.
    #[serde(default)]
    pub fps: Option<u32>,
    /// Video bitrate in kbit/s; blank = the quality's default.
    #[serde(default)]
    pub kbps: Option<u32>,
    #[serde(default)]
    pub look: Look,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

/// The little channel logo in the corner of the picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BugOptions {
    #[serde(default)]
    pub enabled: bool,
    /// What it says; blank = the channel number and name.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub position: Corner,
    /// 10-100 percent.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
}

fn default_opacity() -> u8 {
    60
}

impl Default for BugOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            text: String::new(),
            position: Corner::TopRight,
            opacity: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "bottom_right")]
    pub position: Corner,
    #[serde(default = "yes")]
    pub hour12: bool,
}

fn bottom_right() -> Corner {
    Corner::BottomRight
}

impl Default for ClockOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Corner::BottomRight,
            hour12: true,
        }
    }
}

/// A "now playing" lower third when each programme begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowTitleOptions {
    #[serde(default)]
    pub enabled: bool,
    /// How long it stays up, 3-30 seconds.
    #[serde(default = "default_title_secs")]
    pub seconds: u32,
}

fn default_title_secs() -> u32 {
    8
}

impl Default for ShowTitleOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OverlayOptions {
    #[serde(default)]
    pub bug: BugOptions,
    #[serde(default)]
    pub clock: ClockOptions,
    #[serde(default)]
    pub show_title: ShowTitleOptions,
}

/// Settings that differ for one show on this channel: anime in Japanese with
/// English subtitles, a sitcom in plain English, an old serial in black and white.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ShowOptions {
    #[serde(default)]
    pub audio_languages: Option<Vec<String>>,
    #[serde(default)]
    pub subtitles: Option<SubMode>,
    #[serde(default)]
    pub subtitle_languages: Option<Vec<String>>,
    #[serde(default)]
    pub look: Option<Look>,
    #[serde(default)]
    pub aspect: Option<Aspect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChannelOptions {
    #[serde(default)]
    pub audio: AudioPrefs,
    #[serde(default)]
    pub subtitles: SubPrefs,
    #[serde(default)]
    pub video: VideoOptions,
    #[serde(default)]
    pub overlay: OverlayOptions,
    /// Overrides by show name (compared ignoring case).
    #[serde(default)]
    pub shows: BTreeMap<String, ShowOptions>,
}

/// The settings that apply while one particular show airs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub audio: AudioPrefs,
    pub subtitles: SubPrefs,
    pub look: Look,
    pub aspect: Aspect,
}

impl ChannelOptions {
    pub fn for_show(&self, show: Option<&str>) -> Effective {
        let mut e = Effective {
            audio: self.audio.clone(),
            subtitles: self.subtitles.clone(),
            look: self.video.look,
            aspect: self.video.aspect,
        };
        let over = show.and_then(|s| {
            self.shows
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(s.trim()))
                .map(|(_, v)| v)
        });
        if let Some(o) = over {
            if let Some(l) = &o.audio_languages {
                e.audio.languages = l.clone();
            }
            if let Some(m) = o.subtitles {
                e.subtitles.mode = m;
            }
            if let Some(l) = &o.subtitle_languages {
                e.subtitles.languages = l.clone();
            }
            if let Some(l) = o.look {
                e.look = l;
            }
            if let Some(a) = o.aspect {
                e.aspect = a;
            }
        }
        e
    }

    pub fn validate(&self) -> Result<(), String> {
        let lang = |l: &str| -> Result<(), String> {
            if l.trim().is_empty()
                || l.len() > 24
                || !l
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
            {
                Err(format!(
                    "'{l}' is not a language (use a name or code such as English, en or eng)"
                ))
            } else {
                Ok(())
            }
        };
        for l in self.audio.languages.iter().chain(&self.subtitles.languages) {
            lang(l)?;
        }
        if self.audio.languages.len() > 8 || self.subtitles.languages.len() > 8 {
            return Err("at most 8 languages".into());
        }
        if let Some(f) = self.video.fps {
            if !(15..=60).contains(&f) {
                return Err("frame rate must be 15-60".into());
            }
        }
        if let Some(k) = self.video.kbps {
            if !(300..=40_000).contains(&k) {
                return Err("bitrate must be 300-40000 kbit/s".into());
            }
        }
        if !(10..=100).contains(&self.overlay.bug.opacity) {
            return Err("the channel bug's opacity must be 10-100".into());
        }
        if self.overlay.bug.text.chars().count() > 24 {
            return Err("the channel bug's text is at most 24 characters".into());
        }
        if !(3..=30).contains(&self.overlay.show_title.seconds) {
            return Err("the programme title stays up 3-30 seconds".into());
        }
        if self.shows.len() > 300 {
            return Err("too many per-show settings".into());
        }
        for (name, s) in &self.shows {
            if name.trim().is_empty() || name.len() > 120 {
                return Err("a per-show setting needs a show name".into());
            }
            for l in s
                .audio_languages
                .iter()
                .chain(s.subtitle_languages.iter())
                .flatten()
            {
                lang(l)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_object_is_the_default_so_old_channels_keep_working() {
        let o: ChannelOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, ChannelOptions::default());
        assert!(o.audio.avoid_commentary && o.audio.avoid_description);
        assert_eq!(o.video.quality.size(), (1280, 720, 3000));
        assert!(!o.overlay.bug.enabled);
    }

    #[test]
    fn options_round_trip_with_friendly_names() {
        let json = r#"{"audio":{"languages":["jpn","eng"],"loudness":"broadcast","night_mode":true},
            "subtitles":{"mode":"foreign","languages":["English"],"style":"box","size":"large"},
            "video":{"aspect":"fill","quality":"1080","fps":24,"look":"crt"},
            "overlay":{"bug":{"enabled":true,"text":"CH 4","position":"bottom_left","opacity":80},"clock":{"enabled":true,"hour12":false},"show_title":{"enabled":true,"seconds":10}},
            "shows":{"Cowboy Bebop":{"audio_languages":["jpn"],"subtitles":"always","look":"vhs"}}}"#;
        let o: ChannelOptions = serde_json::from_str(json).unwrap();
        assert_eq!(o.audio.loudness, Loudness::Broadcast);
        assert_eq!(o.video.quality, Quality::P1080);
        assert_eq!(o.overlay.bug.position, Corner::BottomLeft);
        assert!(o.validate().is_ok());
        let back: ChannelOptions =
            serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back, o);
    }

    #[test]
    fn per_show_settings_override_the_channel_and_match_any_case() {
        let mut o = ChannelOptions::default();
        o.audio.languages = vec!["eng".into()];
        o.subtitles.mode = SubMode::Off;
        o.video.look = Look::None;
        o.shows.insert(
            "Cowboy Bebop".into(),
            ShowOptions {
                audio_languages: Some(vec!["jpn".into()]),
                subtitles: Some(SubMode::Always),
                look: Some(Look::Vhs),
                ..Default::default()
            },
        );
        let bebop = o.for_show(Some("cowboy bebop "));
        assert_eq!(bebop.audio.languages, vec!["jpn".to_string()]);
        assert_eq!(bebop.subtitles.mode, SubMode::Always);
        assert_eq!(bebop.look, Look::Vhs);
        let other = o.for_show(Some("Cheers"));
        assert_eq!(other.audio.languages, vec!["eng".to_string()]);
        assert_eq!(other.subtitles.mode, SubMode::Off);
        assert_eq!(o.for_show(None).look, Look::None);
    }

    #[test]
    fn validation_catches_nonsense() {
        let bad = |f: &dyn Fn(&mut ChannelOptions)| {
            let mut o = ChannelOptions::default();
            f(&mut o);
            o.validate().is_err()
        };
        assert!(bad(&|o| o.audio.languages = vec!["en;rm -rf".into()]));
        assert!(bad(&|o| o.audio.languages = vec!["".into()]));
        assert!(bad(&|o| o.video.fps = Some(500)));
        assert!(bad(&|o| o.video.kbps = Some(10)));
        assert!(bad(&|o| o.overlay.bug.opacity = 5));
        assert!(bad(&|o| o.overlay.bug.text = "x".repeat(40)));
        assert!(bad(&|o| o.overlay.show_title.seconds = 1));
        assert!(bad(&|o| {
            o.shows.insert("  ".into(), ShowOptions::default());
        }));
        assert!(!bad(
            &|o| o.audio.languages = vec!["English".into(), "ja".into(), "pt-BR".into()]
        ));
    }
}
