//! FFmpeg capability detection.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    /// libx264 (works everywhere).
    Software,
    /// Apple VideoToolbox (macOS).
    VideoToolbox,
    /// NVIDIA NVENC.
    Nvenc,
    /// Intel Quick Sync (Windows and Linux).
    Qsv,
    /// VA-API (Linux: Intel and AMD GPUs).
    Vaapi,
    /// AMD AMF (Windows).
    Amf,
}

impl Encoder {
    /// Every encoder TVMaster can drive, in the order `doctor` reports them.
    pub const ALL: [Encoder; 6] = [
        Encoder::Software,
        Encoder::VideoToolbox,
        Encoder::Nvenc,
        Encoder::Qsv,
        Encoder::Vaapi,
        Encoder::Amf,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "software" | "x264" | "libx264" => Some(Encoder::Software),
            "videotoolbox" => Some(Encoder::VideoToolbox),
            "nvenc" => Some(Encoder::Nvenc),
            "qsv" | "quicksync" => Some(Encoder::Qsv),
            "vaapi" => Some(Encoder::Vaapi),
            "amf" => Some(Encoder::Amf),
            _ => None,
        }
    }

    /// The name used on the command line and in the API.
    pub fn name(self) -> &'static str {
        match self {
            Encoder::Software => "software",
            Encoder::VideoToolbox => "videotoolbox",
            Encoder::Nvenc => "nvenc",
            Encoder::Qsv => "qsv",
            Encoder::Vaapi => "vaapi",
            Encoder::Amf => "amf",
        }
    }

    /// FFmpeg's name for the H.264 encoder.
    pub fn codec(self) -> &'static str {
        match self {
            Encoder::Software => "libx264",
            Encoder::VideoToolbox => "h264_videotoolbox",
            Encoder::Nvenc => "h264_nvenc",
            Encoder::Qsv => "h264_qsv",
            Encoder::Vaapi => "h264_vaapi",
            Encoder::Amf => "h264_amf",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FfmpegInfo {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub version: String,
    pub has_libx264: bool,
    pub has_videotoolbox: bool,
    pub has_nvenc: bool,
    /// Every H.264 encoder this build of FFmpeg contains. Being compiled in
    /// does not mean the hardware is there: `tvmaster doctor` tries each one.
    pub compiled: Vec<Encoder>,
    /// Supports `-readrate_initial_burst` (faster stream start-up).
    pub has_readrate_burst: bool,
    /// The `drawtext` filter is built in (needed for text on the station slate).
    pub has_drawtext: bool,
}

fn run(bin: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", bin.display()))?;
    // Some builds print help to stderr.
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(s)
}

/// Does `bin args...` run and exit successfully?
pub fn run_ok(bin: &Path, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn parse_version(out: &str) -> String {
    out.lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap_or("unknown")
        .to_string()
}

impl FfmpegInfo {
    /// Detect capabilities of the given binaries (names resolved via `PATH`).
    pub fn detect(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Result<Self> {
        let ffmpeg = ffmpeg.into();
        let ffprobe = ffprobe.into();
        let version_out = run(&ffmpeg, &["-hide_banner", "-version"])?;
        if !version_out.starts_with("ffmpeg version") {
            bail!("{} does not look like ffmpeg", ffmpeg.display());
        }
        let encoders = run(&ffmpeg, &["-hide_banner", "-encoders"])?;
        let help = run(&ffmpeg, &["-hide_banner", "-h", "long"])?;
        let filters = run(&ffmpeg, &["-hide_banner", "-filters"])?;
        let compiled = Encoder::ALL
            .into_iter()
            .filter(|e| encoders.contains(&format!(" {} ", e.codec())))
            .collect();
        Ok(Self {
            compiled,
            version: parse_version(&version_out),
            has_libx264: encoders.contains(" libx264 "),
            has_videotoolbox: encoders.contains("h264_videotoolbox"),
            has_nvenc: encoders.contains("h264_nvenc"),
            has_readrate_burst: help.contains("readrate_initial_burst"),
            has_drawtext: filters.contains(" drawtext "),
            ffmpeg,
            ffprobe,
        })
    }

    /// Pick an encoder. `requested` (e.g. from config) wins if the build has it.
    /// Auto mode uses VideoToolbox on macOS and libx264 elsewhere; other
    /// hardware encoders are opt-in until they are tested.
    pub fn choose_encoder(&self, requested: Option<Encoder>) -> Encoder {
        match requested {
            Some(e) if self.compiled.contains(&e) => e,
            Some(_) => Encoder::Software,
            None if cfg!(target_os = "macos") && self.has_videotoolbox => Encoder::VideoToolbox,
            None => Encoder::Software,
        }
    }
}

/// A font file usable by `drawtext`, if one is installed in a standard place.
pub fn find_system_font() -> Option<PathBuf> {
    [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "C:/Windows/Fonts/arial.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_line() {
        assert_eq!(
            parse_version("ffmpeg version 9.0.2 Copyright (c) 2000-2026"),
            "9.0.2"
        );
        assert_eq!(parse_version(""), "unknown");
    }

    #[test]
    fn encoder_names() {
        for e in Encoder::ALL {
            assert_eq!(Encoder::parse(e.name()), Some(e));
        }
        assert_eq!(Encoder::parse("QuickSync"), Some(Encoder::Qsv));
        assert_eq!(Encoder::parse("VideoToolbox"), Some(Encoder::VideoToolbox));
        assert_eq!(Encoder::parse("x264"), Some(Encoder::Software));
        assert_eq!(Encoder::parse("nope"), None);
    }
}
