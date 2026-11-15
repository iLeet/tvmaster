//! FFmpeg/ffprobe integration: probing, library scanning and FFmpeg capability
//! detection. Break detection comes in a later milestone.

mod ffmpeg;
mod scan;

pub use ffmpeg::{Encoder, FfmpegInfo};
pub use scan::{find_videos, parse_name, ParsedName};

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
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
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
    Ok(ProbeInfo {
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
    fn rejects_garbage() {
        assert!(parse_ffprobe("not json").is_err());
    }

    #[test]
    fn tolerates_missing_sections() {
        let info = parse_ffprobe("{}").unwrap();
        assert_eq!(info, ProbeInfo::default());
    }
}
