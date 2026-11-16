//! The pieces of an FFmpeg filter graph that give a channel its picture and
//! sound: aspect handling, retro looks, subtitles, loudness, overlays.
//! Pure string building, so each piece is unit tested without running FFmpeg.

use std::path::{Path, PathBuf};
use tvmaster_core::{Aspect, Corner, Look, Loudness, SubSize, SubStyle};

/// How the audio is treated on its way out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFx {
    pub loudness: Loudness,
    pub night: bool,
    /// Lift speech when folding surround down to stereo (only for 5.1 and up).
    pub boost: bool,
    pub mono: bool,
    /// Channel count of the chosen source track.
    pub channels: u32,
}

impl Default for AudioFx {
    fn default() -> Self {
        Self {
            loudness: Loudness::Off,
            night: false,
            boost: false,
            mono: false,
            channels: 2,
        }
    }
}

/// The `-af` chain. Always resamples to keep sync; the rest only when asked.
pub fn audio_chain(fx: &AudioFx) -> String {
    let mut f: Vec<String> = vec!["aresample=async=1".into()];
    if fx.boost && fx.channels >= 6 {
        // Dialogue lives in the centre channel: give it more weight than the fronts and rears.
        f.push("pan=stereo|FL=1.0*FC+0.6*FL+0.4*BL+0.3*LFE|FR=1.0*FC+0.6*FR+0.4*BR+0.3*LFE".into());
    }
    if fx.night {
        f.push("acompressor=threshold=0.04:ratio=5:attack=15:release=300:makeup=3".into());
    }
    match fx.loudness {
        Loudness::Off => {}
        Loudness::Light => f.push("dynaudnorm=f=300:g=11:p=0.9:m=8".into()),
        Loudness::Broadcast => f.push("loudnorm=I=-23:LRA=7:TP=-2".into()),
    }
    if fx.mono {
        f.push("pan=stereo|c0=0.5*c0+0.5*c1|c1=0.5*c0+0.5*c1".into());
    }
    f.join(",")
}

/// The treatment that makes everything look like it came out of one old set.
pub fn look_chain(look: Look) -> Option<&'static str> {
    match look {
        Look::None => None,
        Look::Vhs => Some("noise=alls=9:allf=t,gblur=sigma=0.9,eq=saturation=1.25:contrast=0.94:brightness=0.02,colorchannelmixer=rr=1:gg=0.97:bb=1.04"),
        Look::Crt => Some("drawgrid=w=iw:h=4:t=1:c=black@0.28,vignette=PI/5,eq=saturation=1.12:contrast=1.06"),
        Look::BlackWhite => Some("hue=s=0,eq=contrast=1.08"),
        Look::Sepia => Some("colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131"),
        Look::Film => Some("noise=alls=6:allf=t+u,eq=saturation=1.08:gamma=1.04,vignette=PI/6"),
    }
}

/// Scale a picture of any shape to `w` x `h` the way the channel wants.
pub fn scale_chain(aspect: Aspect, w: u32, h: u32) -> String {
    match aspect {
        Aspect::Fit => format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1"),
        Aspect::Fill => format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1"),
        Aspect::Stretch => format!("scale={w}:{h},setsar=1"),
    }
}

/// A subtitle burned into the picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Burn {
    /// A text file (the cache copy made for this item) rendered with libass.
    Text {
        path: PathBuf,
        size: SubSize,
        style: SubStyle,
    },
    /// A picture subtitle stream, overlaid as it is.
    Image { index: u32 },
}

/// A path as a filter option value: single-quoted, with colons and backslashes
/// made safe. Only paths this program made are passed (no quotes in them).
pub fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/").replace(':', "\\:");
    format!("'{s}'")
}

/// The libass filter for `path`, sized for an output `height` pixels tall.
pub fn subtitle_filter(path: &Path, size: SubSize, style: SubStyle, height: u32) -> String {
    // libass lays text out on a 288-line canvas unless told otherwise; scale the
    // font so it reads the same on 480p and 1080p.
    let base = match size {
        SubSize::Small => 16.0,
        SubSize::Medium => 22.0,
        SubSize::Large => 30.0,
    };
    let font = (base * height as f32 / 288.0 * 0.62).round().max(10.0);
    let st = match style {
        // BorderStyle 1: outline + shadow
        SubStyle::Outline => format!("FontSize={font},Outline={},Shadow=1,PrimaryColour=&H00FFFFFF&,OutlineColour=&H00000000&,BorderStyle=1,MarginV={}", (height as f32 / 240.0).max(1.0).round(), (height as f32 * 0.05).round()),
        // BorderStyle 3: opaque box behind the text
        SubStyle::Box => format!("FontSize={font},BorderStyle=3,Outline=2,PrimaryColour=&H00FFFFFF&,OutlineColour=&HB0000000&,BackColour=&H90000000&,MarginV={}", (height as f32 * 0.05).round()),
    };
    format!(
        "subtitles=filename={}:force_style='{st}'",
        filter_path(path)
    )
}

/// `x:y` expressions placing an overlay in a corner with a margin.
pub fn corner_xy(c: Corner, margin: u32) -> (String, String) {
    let (left, top) = match c {
        Corner::TopLeft => (true, true),
        Corner::TopRight => (false, true),
        Corner::BottomLeft => (true, false),
        Corner::BottomRight => (false, false),
    };
    (
        if left {
            format!("{margin}")
        } else {
            format!("main_w-overlay_w-{margin}")
        },
        if top {
            format!("{margin}")
        } else {
            format!("main_h-overlay_h-{margin}")
        },
    )
}

/// A graphic laid over the picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayInput {
    pub png: PathBuf,
    pub x: String,
    pub y: String,
    /// An FFmpeg `enable` expression (e.g. only the first seconds); `None` = always.
    pub enable: Option<String>,
}

/// A running clock drawn with FFmpeg's own text rendering (needs `drawtext`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockFx {
    pub font: PathBuf,
    pub corner: Corner,
    pub hour12: bool,
}

pub fn clock_filter(c: &ClockFx, height: u32) -> String {
    let (x, y) = match c.corner {
        Corner::TopLeft => ("24".to_string(), "24".to_string()),
        Corner::TopRight => ("w-text_w-24".into(), "24".into()),
        Corner::BottomLeft => ("24".into(), "h-text_h-24".into()),
        Corner::BottomRight => ("w-text_w-24".into(), "h-text_h-24".into()),
    };
    // strftime inside drawtext: colons and percent signs are escaped for the filter parser.
    let fmt = if c.hour12 { "%-I\\:%M %p" } else { "%H\\:%M" };
    let font = c
        .font
        .to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:");
    format!(
        "drawtext=fontfile='{font}':text='%{{localtime\\:{fmt}}}':fontcolor=white:fontsize={}:box=1:boxcolor=black@0.5:boxborderw=8:x={x}:y={y}",
        (height / 22).max(14)
    )
}

/// Everything decided about one item before FFmpeg runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPlan {
    /// FFmpeg stream index of the audio to use; `None` = first audio stream.
    pub audio_index: Option<u32>,
    pub audio: AudioFx,
    pub burn: Option<Burn>,
    pub look: Look,
    pub aspect: Aspect,
    pub deinterlace: bool,
    pub clock: Option<ClockFx>,
    /// Drawn over the picture, in order.
    pub overlays: Vec<OverlayInput>,
}

impl Default for ItemPlan {
    fn default() -> Self {
        Self {
            audio_index: None,
            audio: AudioFx::default(),
            burn: None,
            look: Look::None,
            aspect: Aspect::Fit,
            deinterlace: false,
            clock: None,
            overlays: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_audio_chain_always_keeps_sync_and_adds_only_what_is_asked() {
        assert_eq!(audio_chain(&AudioFx::default()), "aresample=async=1");
        let f = AudioFx {
            loudness: Loudness::Broadcast,
            night: true,
            boost: true,
            mono: true,
            channels: 6,
        };
        let chain = audio_chain(&f);
        let order: Vec<usize> = [
            "pan=stereo|FL=1.0*FC",
            "acompressor",
            "loudnorm",
            "c0=0.5*c0",
        ]
        .iter()
        .map(|k| chain.find(k).unwrap())
        .collect();
        assert!(
            order.windows(2).all(|w| w[0] < w[1]),
            "mix down first, then dynamics, then loudness, then mono: {chain}"
        );
        assert!(audio_chain(&AudioFx {
            loudness: Loudness::Light,
            ..Default::default()
        })
        .contains("dynaudnorm"));
    }

    #[test]
    fn dialogue_boost_is_skipped_for_stereo_sources() {
        let f = AudioFx {
            boost: true,
            channels: 2,
            ..Default::default()
        };
        assert!(
            !audio_chain(&f).contains("FC"),
            "a stereo file has no centre channel to lift"
        );
    }

    #[test]
    fn aspect_modes_scale_pad_crop_or_stretch() {
        assert!(scale_chain(Aspect::Fit, 1280, 720).contains("pad=1280:720"));
        assert!(scale_chain(Aspect::Fill, 1280, 720).contains("crop=1280:720"));
        let s = scale_chain(Aspect::Stretch, 640, 480);
        assert!(s.starts_with("scale=640:480,") && !s.contains("pad") && !s.contains("crop"));
    }

    #[test]
    fn every_look_but_none_has_a_filter_and_none_has_no_cost() {
        assert_eq!(look_chain(Look::None), None);
        for l in [
            Look::Vhs,
            Look::Crt,
            Look::BlackWhite,
            Look::Sepia,
            Look::Film,
        ] {
            assert!(look_chain(l).is_some_and(|c| !c.is_empty()));
        }
        assert!(look_chain(Look::Crt).unwrap().contains("drawgrid"));
    }

    #[test]
    fn subtitle_paths_are_quoted_and_text_scales_with_the_picture() {
        let f = subtitle_filter(
            Path::new("/cache/subs/ab12.srt"),
            SubSize::Medium,
            SubStyle::Outline,
            720,
        );
        assert!(f.starts_with("subtitles=filename='/cache/subs/ab12.srt':force_style='"));
        assert!(f.contains("BorderStyle=1"));
        assert!(
            subtitle_filter(Path::new("/x.srt"), SubSize::Medium, SubStyle::Box, 720)
                .contains("BorderStyle=3")
        );
        let size = |h| {
            let f = subtitle_filter(Path::new("/x.srt"), SubSize::Large, SubStyle::Outline, h);
            f.split("FontSize=")
                .nth(1)
                .unwrap()
                .split(',')
                .next()
                .unwrap()
                .parse::<f32>()
                .unwrap()
        };
        assert!(size(1080) > size(480));
        assert_eq!(
            filter_path(Path::new("C:\\subs\\a.srt")),
            "'C\\:/subs/a.srt'"
        );
    }

    #[test]
    fn corners_become_expressions_with_a_margin() {
        assert_eq!(corner_xy(Corner::TopLeft, 20), ("20".into(), "20".into()));
        assert_eq!(
            corner_xy(Corner::BottomRight, 24),
            ("main_w-overlay_w-24".into(), "main_h-overlay_h-24".into())
        );
    }

    #[test]
    fn the_clock_escapes_what_the_filter_parser_would_misread() {
        let c = clock_filter(
            &ClockFx {
                font: PathBuf::from("/fonts/a.ttf"),
                corner: Corner::BottomRight,
                hour12: true,
            },
            720,
        );
        assert!(c.contains("%{localtime\\:%-I\\:%M %p}"), "{c}");
        assert!(c.contains("x=w-text_w-24:y=h-text_h-24"));
        let c24 = clock_filter(
            &ClockFx {
                font: PathBuf::from("/fonts/a.ttf"),
                corner: Corner::TopLeft,
                hour12: false,
            },
            480,
        );
        assert!(c24.contains("%H\\:%M"));
    }
}
