//! Finding natural places to cut a show for commercials: chapter markers
//! first, then the black frames broadcasters left between acts. The result is
//! cached per file; shows with neither get a synthetic act structure when
//! the schedule is built.

use crate::Chapter;
use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Ignore candidates this close to the start or end of the file.
const EDGE_MS: u64 = 60_000;
/// Two candidates closer than this are one break.
const MIN_GAP_MS: u64 = 60_000;
/// Shortest black gap that counts as an act break (seconds).
const MIN_BLACK_SECS: f64 = 0.15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakInfo {
    /// "chapters", "black" or "none" (analysed, nothing found).
    pub src: String,
    /// Candidate break points, in milliseconds from the start.
    pub points: Vec<u64>,
}

impl BreakInfo {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

fn thin(mut points: Vec<u64>, duration_ms: u64) -> Vec<u64> {
    points.sort_unstable();
    let hi = duration_ms.saturating_sub(EDGE_MS);
    let mut out: Vec<u64> = Vec::new();
    for p in points {
        if p >= EDGE_MS && p <= hi && out.last().is_none_or(|l| p - l >= MIN_GAP_MS) {
            out.push(p);
        }
    }
    out
}

/// Break candidates from chapter markers (each chapter start is a candidate).
pub fn from_chapters(chapters: &[Chapter], duration_ms: u64) -> Option<BreakInfo> {
    let points = thin(chapters.iter().map(|c| c.start_ms).collect(), duration_ms);
    (!points.is_empty()).then(|| BreakInfo {
        src: "chapters".into(),
        points,
    })
}

fn re_black() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"black_start:([0-9.]+)\s+black_end:([0-9.]+)").expect("valid regex")
    })
}

/// Parse FFmpeg's `blackdetect` log lines into (start, end) seconds.
pub fn parse_blackdetect(log: &str) -> Vec<(f64, f64)> {
    re_black()
        .captures_iter(log)
        .filter_map(|c| Some((c[1].parse().ok()?, c[2].parse().ok()?)))
        .collect()
}

/// Break candidates from black gaps: the middle of each one.
pub fn from_black(intervals: &[(f64, f64)], duration_ms: u64) -> BreakInfo {
    let points = intervals
        .iter()
        .filter(|(s, e)| e - s >= MIN_BLACK_SECS)
        .map(|(s, e)| (((s + e) / 2.0) * 1000.0) as u64)
        .collect();
    let points = thin(points, duration_ms);
    BreakInfo {
        src: if points.is_empty() { "none" } else { "black" }.into(),
        points,
    }
}

/// Scan a file for black frames. Decodes the whole file at low resolution, so
/// it takes a fraction of the running time; do not call it on the request path.
pub fn detect_black(ffmpeg: &Path, file: &Path) -> Result<Vec<(f64, f64)>> {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(file)
        .args([
            "-an",
            "-sn",
            "-vf",
            "scale=160:-2,blackdetect=d=0.15:pix_th=0.10",
            "-f",
            "null",
            "-",
        ])
        .output()
        .with_context(|| format!("running {}", ffmpeg.display()))?;
    Ok(parse_blackdetect(&String::from_utf8_lossy(&out.stderr)))
}

/// Find break candidates for a file: chapters if it has usable ones,
/// otherwise a black-frame scan.
pub fn analyze(
    ffmpeg: &Path,
    file: &Path,
    duration_ms: u64,
    chapters: &[Chapter],
) -> Result<BreakInfo> {
    if let Some(info) = from_chapters(chapters, duration_ms) {
        return Ok(info);
    }
    Ok(from_black(&detect_black(ffmpeg, file)?, duration_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(ms: u64) -> Chapter {
        Chapter {
            start_ms: ms,
            title: None,
        }
    }

    #[test]
    fn chapters_become_candidates_without_the_edges() {
        let chapters = [
            ch(0),
            ch(5 * 60_000),
            ch(11 * 60_000),
            ch(11 * 60_000 + 20_000),
            ch(21 * 60_000 + 30_000),
        ];
        let info = from_chapters(&chapters, 22 * 60_000).unwrap();
        assert_eq!(info.src, "chapters");
        // the start, the near-duplicate and the last 90s are dropped
        assert_eq!(info.points, vec![5 * 60_000, 11 * 60_000]);
        assert!(from_chapters(&[ch(0)], 22 * 60_000).is_none());
    }

    #[test]
    fn parses_blackdetect_output() {
        let log = "[blackdetect @ 0x7f] black_start:312.4 black_end:313.1 black_duration:0.7\n\
                   frame= 100\n[blackdetect @ 0x7f] black_start:640 black_end:641.5 black_duration:1.5\n";
        assert_eq!(parse_blackdetect(log), vec![(312.4, 313.1), (640.0, 641.5)]);
        assert!(parse_blackdetect("nothing here").is_empty());
    }

    #[test]
    fn black_gaps_become_midpoints_and_noise_is_ignored() {
        let iv = [
            (0.0, 2.0),
            (312.4, 313.0),
            (312.9, 313.5),
            (500.0, 500.05),
            (640.0, 641.0),
            (1_315.0, 1_320.0),
        ];
        let info = from_black(&iv, 22 * 60_000);
        assert_eq!(info.src, "black");
        // 0-2s (edge) dropped, near-duplicates merged, 50ms flicker ignored, last-minute dropped
        assert_eq!(info.points, vec![312_700, 640_500]);
        assert_eq!(from_black(&[], 22 * 60_000).src, "none");
    }

    /// Real FFmpeg: a clip with two black gaps is analysed to two break points.
    #[test]
    fn finds_black_gaps_in_a_real_file() {
        let ffmpeg = Path::new("ffmpeg");
        if Command::new(ffmpeg).arg("-version").output().is_err() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        let dir = std::env::temp_dir().join(format!("tvmaster-breaks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("acts.mp4");
        let ok = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=320x180:rate=5:duration=220")
            .args(["-vf", "drawbox=x=0:y=0:w=iw:h=ih:color=black:t=fill:enable='between(t,70,71.5)+between(t,150,151.5)'"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&file)
            .status();
        if !matches!(ok, Ok(s) if s.success()) {
            eprintln!("could not generate the clip (no libx264?); skipping");
            return;
        }
        let info = analyze(ffmpeg, &file, 220_000, &[]).unwrap();
        assert_eq!(info.src, "black");
        assert_eq!(info.points.len(), 2, "{:?}", info.points);
        assert!(info.points[0].abs_diff(70_750) < 600, "{:?}", info.points);
        assert!(info.points[1].abs_diff(150_750) < 600, "{:?}", info.points);
        // chapters win when present
        let chapters = [Chapter {
            start_ms: 100_000,
            title: None,
        }];
        assert_eq!(
            analyze(ffmpeg, &file, 220_000, &chapters).unwrap().src,
            "chapters"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_roundtrip() {
        let info = BreakInfo {
            src: "black".into(),
            points: vec![1, 2],
        };
        assert_eq!(BreakInfo::from_json(&info.to_json()), Some(info));
        assert_eq!(BreakInfo::from_json("garbage"), None);
    }
}
