//! FFmpeg/ffprobe integration: probing, library scanning and FFmpeg capability
//! detection. Break detection comes in a later milestone.

mod breaks;
mod ffmpeg;
mod scan;

pub use breaks::{analyze, detect_black, from_black, from_chapters, parse_blackdetect, BreakInfo};
pub use ffmpeg::{find_system_font, run_ok, Encoder, FfmpegInfo};
pub use scan::{find_audio, find_sidecar_subs, find_videos, parse_name, parse_year, ParsedName};

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub start_ms: u64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProbeInfo {
    pub duration_ms: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub chapters: Vec<Chapter>,
    /// Every audio and subtitle stream, so a channel can choose between them.
    pub tracks: tvmaster_core::Tracks,
}

#[derive(Deserialize)]
struct RawProbe {
    #[serde(default)]
    streams: Vec<RawStream>,
    #[serde(default)]
    chapters: Vec<RawChapter>,
    format: Option<RawFormat>,
}

#[derive(Deserialize)]
struct RawStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    channels: Option<u32>,
    field_order: Option<String>,
    #[serde(default)]
    tags: Option<RawStreamTags>,
    #[serde(default)]
    disposition: Option<RawDisposition>,
}

#[derive(Deserialize, Default)]
struct RawStreamTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawDisposition {
    #[serde(default)]
    default: u8,
    #[serde(default)]
    forced: u8,
    #[serde(default)]
    hearing_impaired: u8,
    #[serde(default)]
    visual_impaired: u8,
    #[serde(default)]
    comment: u8,
}

fn tracks_of(streams: &[RawStream]) -> tvmaster_core::Tracks {
    let mut t = tvmaster_core::Tracks::default();
    for (pos, s) in streams.iter().enumerate() {
        let index = s.index.unwrap_or(pos as u32);
        let tags = s.tags.as_ref();
        let (lang, title) = (
            tags.and_then(|t| t.language.as_deref()).unwrap_or(""),
            tags.and_then(|t| t.title.as_deref()).unwrap_or(""),
        );
        let d = s.disposition.as_ref();
        let flag = |f: fn(&RawDisposition) -> u8| d.is_some_and(|d| f(d) != 0);
        match s.codec_type.as_deref() {
            Some("audio") => t.audio.push(tvmaster_core::audio_track(
                index,
                lang,
                s.codec_name.as_deref().unwrap_or(""),
                s.channels.unwrap_or(2),
                title,
                flag(|d| d.default),
                flag(|d| d.comment),
                flag(|d| d.visual_impaired),
            )),
            Some("subtitle") => t.subs.push(tvmaster_core::sub_track(
                index,
                lang,
                s.codec_name.as_deref().unwrap_or(""),
                title,
                flag(|d| d.default),
                flag(|d| d.forced),
                flag(|d| d.hearing_impaired),
            )),
            Some("video") if !t.interlaced => {
                t.interlaced = matches!(s.field_order.as_deref(), Some("tt" | "bb" | "tb" | "bt"));
            }
            _ => {}
        }
    }
    t
}

#[derive(Deserialize)]
struct RawChapter {
    start_time: Option<String>,
    tags: Option<RawTags>,
}

#[derive(Deserialize)]
struct RawTags {
    title: Option<String>,
}

#[derive(Deserialize)]
struct RawFormat {
    duration: Option<String>,
}

fn secs_to_ms(s: &str) -> Option<u64> {
    s.parse::<f64>().ok().map(|v| (v * 1000.0).round() as u64)
}

/// Parse the JSON produced by `ffprobe -print_format json -show_format
/// -show_streams -show_chapters`.
pub fn parse_ffprobe(json: &str) -> Result<ProbeInfo> {
    let raw: RawProbe = serde_json::from_str(json).context("invalid ffprobe JSON")?;
    let video = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    let audio = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    let tracks = tracks_of(&raw.streams);
    Ok(ProbeInfo {
        tracks,
        duration_ms: raw
            .format
            .and_then(|f| f.duration)
            .and_then(|d| secs_to_ms(&d))
            .unwrap_or(0),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        video_codec: video.and_then(|v| v.codec_name.clone()),
        audio_codec: audio.and_then(|a| a.codec_name.clone()),
        chapters: raw
            .chapters
            .into_iter()
            .filter_map(|c| {
                Some(Chapter {
                    start_ms: c.start_time.as_deref().and_then(secs_to_ms)?,
                    title: c.tags.and_then(|t| t.title),
                })
            })
            .collect(),
    })
}

/// Run `ffprobe` on a file (expects `ffprobe` on `PATH`).
pub fn probe(path: &Path) -> Result<ProbeInfo> {
    probe_with(Path::new("ffprobe"), path)
}

/// Run a specific `ffprobe` binary on a file.
pub fn probe_with(ffprobe: &Path, path: &Path) -> Result<ProbeInfo> {
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ])
        .arg(path)
        .output()
        .context("failed to run ffprobe (is FFmpeg installed?)")?;
    if !out.status.success() {
        anyhow::bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse_ffprobe(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "streams": [
            {"codec_type": "audio", "codec_name": "aac"},
            {"codec_type": "video", "codec_name": "h264", "width": 720, "height": 480}
        ],
        "chapters": [
            {"start_time": "0.000000", "tags": {"title": "Cold open"}},
            {"start_time": "312.500000"}
        ],
        "format": {"duration": "1320.040000"}
    }"#;

    #[test]
    fn parses_duration_video_and_chapters() {
        let info = parse_ffprobe(SAMPLE).unwrap();
        assert_eq!(info.duration_ms, 1_320_040);
        assert_eq!(info.width, Some(720));
        assert_eq!(info.height, Some(480));
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert_eq!(info.chapters.len(), 2);
        assert_eq!(info.chapters[1].start_ms, 312_500);
        assert_eq!(info.chapters[0].title.as_deref(), Some("Cold open"));
    }

    #[test]
    fn reads_every_audio_and_subtitle_track_with_its_flags() {
        let json = r#"{"streams":[
            {"index":0,"codec_type":"video","codec_name":"h264","width":720,"height":480,"field_order":"tt"},
            {"index":1,"codec_type":"audio","codec_name":"aac","channels":2,"tags":{"language":"jpn"},"disposition":{"default":1}},
            {"index":2,"codec_type":"audio","codec_name":"ac3","channels":6,"tags":{"language":"eng","title":"English 5.1"}},
            {"index":3,"codec_type":"audio","codec_name":"aac","channels":2,"tags":{"language":"eng","title":"Director's Commentary"},"disposition":{"comment":1}},
            {"index":4,"codec_type":"subtitle","codec_name":"subrip","tags":{"language":"eng"}},
            {"index":5,"codec_type":"subtitle","codec_name":"subrip","tags":{"language":"en","title":"SDH"},"disposition":{"hearing_impaired":1}},
            {"index":6,"codec_type":"subtitle","codec_name":"hdmv_pgs_subtitle","tags":{"language":"fre"},"disposition":{"forced":1}}
        ],"format":{"duration":"60"}}"#;
        let t = parse_ffprobe(json).unwrap().tracks;
        assert!(t.interlaced);
        assert_eq!(t.audio.len(), 3);
        assert_eq!(
            (
                t.audio[0].index,
                t.audio[0].lang.as_str(),
                t.audio[0].default
            ),
            (1, "jpn", true)
        );
        assert_eq!((t.audio[1].lang.as_str(), t.audio[1].channels), ("eng", 6));
        assert!(t.audio[2].commentary);
        assert_eq!(t.subs.len(), 3);
        assert_eq!(t.subs[1].lang, "eng", "en is normalised");
        assert!(t.subs[1].sdh);
        assert!(t.subs[2].image && t.subs[2].forced);
        assert_eq!(t.subs[2].lang, "fra");
    }

    #[test]
    fn progressive_video_is_not_flagged_interlaced() {
        let t = parse_ffprobe(
            r#"{"streams":[{"index":0,"codec_type":"video","field_order":"progressive"}]}"#,
        )
        .unwrap()
        .tracks;
        assert!(!t.interlaced);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_ffprobe("not json").is_err());
    }

    #[test]
    fn tolerates_missing_sections() {
        let info = parse_ffprobe("{}").unwrap();
        assert_eq!(info, ProbeInfo::default());
    }
}
