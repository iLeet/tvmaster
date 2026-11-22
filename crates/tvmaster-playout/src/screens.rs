//! Designed screens on air: the owner's designs for stand-by, sign-off and the break screens, kept
//! with the channel's options (the server puts the station's designs there for channels that have
//! none of their own) and drawn by `tvmaster-render`.

use crate::ChannelSpec;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tvmaster_render::design::{Design, Images, LayerKind, Picture, Vars};
use tvmaster_render::{Scene, Theme};

/// The kinds of screen a design can replace.
pub const KINDS: [&str; 7] = [
    "standby",
    "signoff",
    "up_next",
    "later_today",
    "brb",
    "back_to",
    "station_id",
];

/// The channel's design for a kind of screen, if it has one that reads properly.
pub fn design_for(spec: &ChannelSpec, kind: &str) -> Option<Arc<Design>> {
    let v = spec.options.screens.get(kind)?;
    let d: Design = serde_json::from_value(v.clone()).ok()?;
    d.problem().is_none().then(|| Arc::new(d))
}

/// A picture as RGBA, fitted inside `w` x `h` (or filling it, cropped, when `cover`). Blocks while
/// FFmpeg reads it.
fn load_one(ffmpeg: &Path, src: &Path, w: usize, h: usize, cover: bool) -> Option<Picture> {
    if !src.is_file() {
        return None;
    }
    let vf = if cover {
        format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},format=rgba")
    } else {
        format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=rgba")
    };
    let out = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(src)
        .args(["-vf", &vf, "-frames:v", "1", "-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    (out.status.success() && out.stdout.len() == w * h * 4).then(|| Picture {
        w,
        h,
        rgba: Arc::new(out.stdout),
    })
}

/// Every picture a design uses. A missing or unreadable one is left out (the design draws without it).
pub fn load_pictures(ffmpeg: &Path, d: &Design) -> Images {
    let mut out = Images::new();
    for path in d.pictures() {
        // the box it will be drawn in: the whole screen for a backdrop, else the largest layer box
        let mut cover = false;
        let (mut w, mut h) = (0usize, 0usize);
        if let tvmaster_render::design::Backdrop::Picture { path: p } = &d.backdrop {
            if *p == path {
                cover = true;
                (w, h) = (1280, 720);
            }
        }
        for l in &d.layers {
            if let LayerKind::Image {
                path: p,
                w: lw,
                h: lh,
            } = &l.kind
            {
                if *p == path && !cover {
                    w = w.max((*lw as usize * 1280 / 1000).clamp(16, 1280));
                    h = h.max((*lh as usize * 720 / 1000).clamp(16, 720));
                }
            }
        }
        if let Some(p) = load_one(ffmpeg, Path::new(&path), w.max(16), h.max(16), cover) {
            out.insert(path, p);
        }
    }
    out
}

/// What the `{...}` words say on a channel's own screens (stand-by and sign-off).
pub fn channel_vars(spec: &ChannelSpec, at_ms: i64) -> Vars {
    let zone = spec.dayparts.zone();
    let message = spec.options.off_air.message.trim();
    Vars {
        channel: spec.name.clone(),
        number: spec.number,
        message: if message.is_empty() {
            "PLEASE STAND BY".into()
        } else {
            message.to_string()
        },
        clock0_secs: zone.secs_of_day(at_ms),
        date: zone.date_text(at_ms),
        ..Vars::default()
    }
}

/// The words a break screen says, taken from the scene it would otherwise draw.
fn scene_vars(spec: &ChannelSpec, scene: &Scene, at_ms: i64) -> Option<(&'static str, Vars)> {
    let mut v = channel_vars(spec, at_ms);
    let kind = match scene {
        Scene::StationId { name } => {
            v.channel = name.clone();
            "station_id"
        }
        Scene::Brb { name, show } => {
            v.channel = name.clone();
            v.show = show.clone();
            "brb"
        }
        Scene::BackTo { name, show } => {
            v.channel = name.clone();
            v.show = show.clone();
            "back_to"
        }
        Scene::UpNext { name, title, time } => {
            v.channel = name.clone();
            v.show = title.clone();
            v.title = title.clone();
            v.time = time.clone();
            "up_next"
        }
        Scene::LaterToday { name, items, .. } => {
            v.channel = name.clone();
            v.items = items.clone();
            if let Some((time, title)) = items.first() {
                v.time = time.clone();
                v.title = title.clone();
                v.show = title.clone();
            }
            "later_today"
        }
        _ => return None,
    };
    Some((kind, v))
}

/// A designed version of a break screen, if the channel has a design for it; with the theme it is
/// drawn in (always the smooth one: a design is drawn at full size).
pub fn designed_scene(
    ffmpeg: &Path,
    spec: &ChannelSpec,
    scene: &Scene,
    theme: &Theme,
    at_ms: i64,
) -> Option<(Scene, Theme)> {
    let (kind, vars) = scene_vars(spec, scene, at_ms)?;
    let design = design_for(spec, kind)?;
    let images = Arc::new(load_pictures(ffmpeg, &design));
    let mut th = *theme;
    th.modern = true;
    Some((
        Scene::Design {
            design,
            vars,
            images,
        },
        th,
    ))
}
