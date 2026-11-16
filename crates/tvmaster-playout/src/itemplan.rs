//! Deciding, at the moment an item goes to air, which audio track and subtitle
//! it gets and which graphics go over it, and preparing the files FFmpeg needs
//! for that (a text subtitle cut out of the video, the logo, the title card).

use crate::looks::{corner_xy, AudioFx, Burn, ClockFx, ItemPlan, OverlayInput};
use crate::{ChannelSpec, EngineConfig, PlayItem};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tvmaster_core::{choose, fnv1a, Deinterlace, SubPick, Tracks};

fn cache_dir(cfg: &EngineConfig, what: &str) -> PathBuf {
    cfg.root.join("cache").join(what)
}

/// A file name safe to put inside an FFmpeg filter string.
fn safe_name(seed: &str, ext: &str) -> String {
    format!("{:016x}.{ext}", fnv1a(seed))
}

fn mtime_size(p: &Path) -> (u64, u64) {
    std::fs::metadata(p)
        .map(|m| {
            (
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                m.len(),
            )
        })
        .unwrap_or((0, 0))
}

/// A text subtitle as a file with a safe name: copied from beside the video, or
/// cut out of it once and kept.
async fn text_subtitle(
    cfg: &EngineConfig,
    media: &Path,
    pick: &SubPick,
    tracks: &Tracks,
) -> Option<PathBuf> {
    let dir = cache_dir(cfg, "subs");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    match pick {
        SubPick::External(path) => {
            let src = Path::new(path);
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("srt")
                .to_lowercase();
            let (m, n) = mtime_size(src);
            let out = dir.join(safe_name(&format!("{path}|{m}|{n}"), &ext));
            if out.exists() || tokio::fs::copy(src, &out).await.is_ok() {
                return Some(out);
            }
            None
        }
        SubPick::Embedded {
            index,
            image: false,
        } => {
            let codec = tracks
                .subs
                .iter()
                .find(|s| s.index == *index)
                .map(|s| s.codec.as_str())
                .unwrap_or("subrip");
            let (ext, codec_arg) = if matches!(codec, "ass" | "ssa") {
                ("ass", "copy")
            } else {
                ("srt", "srt")
            };
            let (m, n) = mtime_size(media);
            let out = dir.join(safe_name(
                &format!("{}|{index}|{m}|{n}", media.display()),
                ext,
            ));
            if out.exists() {
                return Some(out);
            }
            let tmp = out.with_extension(format!("{ext}.part"));
            let status = Command::new(&cfg.ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
                .arg(media)
                .args([
                    "-map",
                    &format!("0:{index}"),
                    "-c:s",
                    codec_arg,
                    "-f",
                    if ext == "ass" { "ass" } else { "srt" },
                ])
                .arg(&tmp)
                .status()
                .await
                .ok()?;
            if status.success() && tokio::fs::rename(&tmp, &out).await.is_ok() {
                Some(out)
            } else {
                let _ = tokio::fs::remove_file(&tmp).await;
                None
            }
        }
        SubPick::Embedded { image: true, .. } => None,
    }
}

/// Write `png` once under `<root>/cache/<what>/<name>` and return where it is.
async fn graphic(cfg: &EngineConfig, what: &str, name: &str, png: &[u8]) -> Option<PathBuf> {
    let dir = cache_dir(cfg, what);
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(name);
    if !path.exists() {
        let tmp = path.with_extension("part");
        tokio::fs::write(&tmp, png).await.ok()?;
        tokio::fs::rename(&tmp, &path).await.ok()?;
    }
    Some(path)
}

/// A logo picture of any common format, fitted into a corner-sized box with
/// the channel's logo opacity, as a PNG FFmpeg can overlay. Made once per
/// picture (changing the file or the settings makes a new one); `None` when the
/// file is missing or FFmpeg cannot read it, so the text logo is used instead.
async fn logo_png(cfg: &EngineConfig, src: &Path, opacity: u8) -> Option<PathBuf> {
    let (w, h) = (cfg.profile.width * 22 / 100, cfg.profile.height * 16 / 100);
    fit_picture(cfg, src, w, h, opacity).await
}

/// `src` (any common picture format) fitted inside `w` x `h` pixels, with the
/// given opacity, as a PNG in the cache. `None` when it cannot be read.
pub(crate) async fn fit_picture(
    cfg: &EngineConfig,
    src: &Path,
    w: u32,
    h: u32,
    opacity: u8,
) -> Option<PathBuf> {
    let meta = tokio::fs::metadata(src)
        .await
        .ok()
        .filter(|m| m.is_file())?;
    let stamp = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let key = format!("{}|{}|{stamp}|{opacity}|{w}x{h}", src.display(), meta.len());
    let dir = cache_dir(cfg, "logos");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(format!("{:016x}.png", fnv1a(&key)));
    if path.exists() {
        return Some(path);
    }
    let tmp = path.with_extension("part.png");
    let alpha = f64::from(opacity.clamp(10, 100)) / 100.0;
    let out = Command::new(&cfg.ffmpeg)
        .args(["-v", "error", "-y", "-i"])
        .arg(src)
        .args([
            "-vf",
            &format!(
                "scale=w={w}:h={h}:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,colorchannelmixer=aa={alpha}"
            ),
            "-frames:v",
            "1",
            "-update",
            "1",
        ])
        .arg(&tmp)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        tracing::warn!(
            "logo picture {} could not be used: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let _ = tokio::fs::remove_file(&tmp).await;
        return None;
    }
    tokio::fs::rename(&tmp, &path).await.ok()?;
    Some(path)
}

/// Everything about how this item should look and sound.
pub(crate) async fn plan_item(
    cfg: &EngineConfig,
    spec: &ChannelSpec,
    item: &PlayItem,
    offset_ms: u64,
) -> ItemPlan {
    let opts = &spec.options;
    let defaults = match item.show.clone() {
        Some(show) => {
            let (source, id) = (spec.source.clone(), spec.id.clone());
            tokio::task::spawn_blocking(move || source.show_defaults(&id, &show))
                .await
                .ok()
                .flatten()
        }
        None => None,
    };
    let eff = opts.for_show_with(item.show.as_deref(), defaults.as_ref());
    let mut plan = ItemPlan {
        look: eff.look,
        aspect: eff.aspect,
        ..ItemPlan::default()
    };
    let tracks = item.tracks.clone().unwrap_or_default();
    plan.deinterlace = match opts.video.deinterlace {
        Deinterlace::Always => true,
        Deinterlace::Off => false,
        Deinterlace::Auto => tracks.interlaced,
    };

    // Audio: only programmes get to choose among tracks; commercials and clips use what they have.
    let mut audio = AudioFx {
        loudness: eff.audio.loudness,
        night: eff.audio.night_mode,
        boost: eff.audio.dialogue_boost,
        mono: eff.audio.mono,
        channels: 2,
    };
    if item.is_show && item.has_audio {
        let choice = choose(&tracks, &eff.audio, &eff.subtitles, spec.can_burn_text);
        plan.audio_index = choice.audio;
        if let Some(t) = choice
            .audio
            .and_then(|i| tracks.audio.iter().find(|t| t.index == i))
        {
            audio.channels = t.channels;
        }
        // Subtitles go on the programme, not on its commercials.
        if let (Some(pick), Some(media)) = (&choice.subtitle, item.path.as_deref()) {
            plan.burn = match pick {
                SubPick::Embedded { index, image: true } => Some(Burn::Image { index: *index }),
                other => text_subtitle(cfg, media, other, &tracks)
                    .await
                    .map(|path| Burn::Text {
                        path,
                        size: eff.subtitles.size,
                        style: eff.subtitles.style,
                    }),
            };
            if plan.burn.is_none() {
                tracing::warn!(channel = %spec.id, "could not prepare the subtitle; airing without it");
            }
        }
    }
    plan.audio = audio;

    // Graphics over the picture.
    let ov = &opts.overlay;
    let logo = match ov.bug.image.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(src) if ov.bug.enabled => logo_png(cfg, Path::new(src), ov.bug.opacity).await,
        _ => None,
    };
    if let Some(png) = logo {
        let (x, y) = corner_xy(ov.bug.position, 24);
        plan.overlays.push(OverlayInput {
            png,
            x,
            y,
            enable: None,
        });
    } else if ov.bug.enabled {
        let text = if ov.bug.text.trim().is_empty() {
            format!("{} {}", spec.number, spec.name)
        } else {
            ov.bug.text.clone()
        };
        let g = tvmaster_render::overlay::bug(&text, ov.bug.opacity);
        let name = format!("{:016x}.png", fnv1a(&format!("{text}|{}", ov.bug.opacity)));
        if let Some(png) = graphic(cfg, "bugs", &name, &g.png).await {
            let (x, y) = corner_xy(ov.bug.position, 24);
            plan.overlays.push(OverlayInput {
                png,
                x,
                y,
                enable: None,
            });
        }
    }
    if ov.show_title.enabled && item.is_show && item.in_ms == 0 && offset_ms == 0 {
        if let Some(title) = item.show.as_deref().filter(|t| !t.is_empty()) {
            let sub = item.subtitle.clone().unwrap_or_default();
            let width = cfg.profile.width * 45 / 100;
            let g = tvmaster_render::overlay::lower_third(title, &sub, width);
            let name = format!("{:016x}.png", fnv1a(&format!("{title}|{sub}|{width}")));
            if let Some(png) = graphic(cfg, "titles", &name, &g.png).await {
                let secs = ov.show_title.seconds;
                plan.overlays.push(OverlayInput {
                    png,
                    x: "40".into(),
                    y: "main_h-overlay_h-48".into(),
                    enable: Some(format!("between(t,0.5,{secs})")),
                });
            }
        }
    }
    if ov.clock.enabled {
        if let Some(font) = &cfg.slate_font {
            plan.clock = Some(ClockFx {
                font: font.clone(),
                corner: ov.clock.position,
                hour12: ov.clock.hour12,
            });
        }
    }
    plan
}

impl ItemPlan {
    /// Does this plan add anything that could stop FFmpeg from encoding?
    pub(crate) fn has_extras(&self) -> bool {
        self.burn.is_some() || !self.overlays.is_empty() || self.clock.is_some()
    }

    /// The same item without subtitles, graphics or clock: what to try when the
    /// fancy version fails, so a problem with an overlay never takes a good file off the air.
    pub(crate) fn plain(&self) -> ItemPlan {
        ItemPlan {
            burn: None,
            overlays: Vec::new(),
            clock: None,
            ..self.clone()
        }
    }
}
