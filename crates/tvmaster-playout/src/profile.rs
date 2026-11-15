//! FFmpeg command construction: the per-item encoder and the HLS segmenter.
//! Kept free of process spawning so the argument lists are unit-testable.

use std::path::Path;
use tvmaster_media::Encoder;

/// The normalized output every item is converted to, so mixed sources
/// (4:3 retro ads, 1080p shows, no-audio clips) all join seamlessly.
#[derive(Debug, Clone)]
pub struct OutputProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    /// Keyframe interval in seconds (also the HLS segment length).
    pub gop_secs: u32,
}

impl Default for OutputProfile {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 30,
            video_kbps: 3000,
            audio_kbps: 128,
            gop_secs: 2,
        }
    }
}

/// One item (or the remaining part of one) to encode.
#[derive(Debug, Clone)]
pub struct ItemJob<'a> {
    pub file: &'a Path,
    /// Start this far into the file.
    pub offset_ms: u64,
    /// Play this much of it.
    pub duration_ms: u64,
    /// Output timestamp offset so consecutive items form one continuous stream.
    pub ts_offset_ms: u64,
    pub has_audio: bool,
}

fn secs(ms: u64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

fn video_args(enc: Encoder, p: &OutputProfile) -> Vec<String> {
    let gop = (p.fps * p.gop_secs).to_string();
    let rate = format!("{}k", p.video_kbps);
    let maxrate = format!("{}k", p.video_kbps * 12 / 10);
    let bufsize = format!("{}k", p.video_kbps * 2);
    let mut a: Vec<String> = Vec::new();
    let mut push = |xs: &[&str]| a.extend(xs.iter().map(|s| s.to_string()));
    match enc {
        Encoder::Software => {
            push(&[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high",
            ]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop, "-keyint_min", &gop, "-sc_threshold", "0"]);
        }
        Encoder::VideoToolbox => {
            push(&["-c:v", "h264_videotoolbox", "-profile:v", "high"]);
            push(&["-b:v", &rate, "-maxrate", &maxrate]);
            push(&["-g", &gop, "-allow_sw", "1"]);
        }
        Encoder::Nvenc => {
            push(&["-c:v", "h264_nvenc", "-preset", "p4", "-profile:v", "high"]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop]);
        }
    }
    a
}

/// Arguments to encode one item to MPEG-TS on stdout.
pub fn item_args(job: &ItemJob, p: &OutputProfile, enc: Encoder) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if job.offset_ms > 0 {
        a.extend(["-ss".into(), secs(job.offset_ms)]);
    }
    a.extend(["-i".into(), job.file.to_string_lossy().to_string()]);
    if !job.has_audio {
        // Silent track so every item has the same stream layout.
        a.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            secs(job.duration_ms),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]);
    }
    a.extend(["-t".into(), secs(job.duration_ms)]);
    a.extend(["-map".into(), "0:v:0".into()]);
    a.extend([
        "-map".into(),
        if job.has_audio { "0:a:0" } else { "1:a:0" }.into(),
    ]);
    a.extend([
        "-vf".into(),
        format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},format=yuv420p",
            w = p.width,
            h = p.height,
            fps = p.fps
        ),
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
    ]);
    if job.has_audio {
        a.extend(["-af".into(), "aresample=async=1".into()]);
    }
    a.extend([
        "-output_ts_offset".into(),
        secs(job.ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

fn slate_text(s: &str) -> String {
    // drawtext treats : ' \\ % specially; keep the slate text to safe characters.
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " .,-!&".contains(*c))
        .take(60)
        .collect()
}

/// Arguments for the station slate: a plain card with silence, used for gaps,
/// scheduled filler and files that cannot be played. Text is drawn only when a
/// font is supplied (FFmpeg must have `drawtext`).
pub fn slate_args(
    text: &str,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
    font: Option<&Path>,
) -> Vec<String> {
    let dur = secs(duration_ms);
    let mut vf = String::new();
    if let Some(font) = font {
        let font = font
            .to_string_lossy()
            .replace('\\', "/")
            .replace(':', "\\:");
        vf.push_str(&format!(
            "drawtext=fontfile='{font}':text='{}':fontcolor=white:fontsize={}:x=(w-text_w)/2:y=(h-text_h)/2,",
            slate_text(text),
            p.height / 14
        ));
    }
    vf.push_str("format=yuv420p");
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend([
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur.clone(),
        "-i".into(),
        format!("color=c=0x0b1220:s={}x{}:r={}", p.width, p.height, p.fps),
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur,
        "-i".into(),
        "anullsrc=r=48000:cl=stereo".into(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        vf,
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
        "-output_ts_offset".into(),
        secs(ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// Arguments for a rendered bumper: raw RGB frames of `src_w` x `src_h` arrive
/// on stdin, are scaled up without smoothing (crisp pixel art) and encoded
/// with silence.
pub fn rendered_args(
    src_w: usize,
    src_h: usize,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
) -> Vec<String> {
    let dur = secs(duration_ms);
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend([
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgb24".into(),
        "-video_size".into(),
        format!("{src_w}x{src_h}"),
        "-framerate".into(),
        p.fps.to_string(),
        "-i".into(),
        "pipe:0".into(),
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur.clone(),
        "-i".into(),
        "anullsrc=r=48000:cl=stereo".into(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        format!(
            "scale={}:{}:flags=neighbor,setsar=1,fps={},format=yuv420p",
            p.width, p.height, p.fps
        ),
        "-t".into(),
        dur,
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
        "-output_ts_offset".into(),
        secs(ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// Arguments for the long-running segmenter that turns the continuous
/// MPEG-TS on stdin into a rolling HLS playlist in `dir`.
pub fn segmenter_args(dir: &Path, p: &OutputProfile, readrate_burst: bool) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Pace input at real time; burst the first few seconds so tune-in is fast.
    if readrate_burst {
        a.extend([
            "-readrate".into(),
            "1".into(),
            "-readrate_initial_burst".into(),
            (p.gop_secs * 3).to_string(),
        ]);
    } else {
        a.push("-re".into());
    }
    a.extend(["-f", "mpegts", "-i", "pipe:0", "-c", "copy", "-f", "hls"].map(String::from));
    a.extend(["-hls_time".into(), p.gop_secs.to_string()]);
    a.extend(["-hls_list_size".into(), "10".into()]);
    a.extend([
        "-hls_flags".into(),
        "delete_segments+independent_segments+omit_endlist+program_date_time+temp_file".into(),
        "-hls_start_number_source".into(),
        "epoch".into(),
        "-hls_segment_filename".into(),
        dir.join("seg_%d.ts").to_string_lossy().to_string(),
        dir.join("index.m3u8").to_string_lossy().to_string(),
    ]);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn job(has_audio: bool, offset: u64) -> (PathBuf, u64, u64, bool) {
        (PathBuf::from("/tv/a.mkv"), offset, 20_000, has_audio)
    }

    fn build(has_audio: bool, offset: u64) -> Vec<String> {
        let (f, off, dur, audio) = job(has_audio, offset);
        item_args(
            &ItemJob {
                file: &f,
                offset_ms: off,
                duration_ms: dur,
                ts_offset_ms: 5_000,
                has_audio: audio,
            },
            &OutputProfile::default(),
            Encoder::Software,
        )
    }

    fn has_pair(a: &[String], k: &str, v: &str) -> bool {
        a.windows(2).any(|w| w[0] == k && w[1] == v)
    }

    #[test]
    fn seeks_only_when_offset_nonzero() {
        assert!(!build(true, 0).contains(&"-ss".to_string()));
        assert!(has_pair(&build(true, 12_500), "-ss", "12.500"));
    }

    #[test]
    fn silent_items_get_generated_audio() {
        let a = build(false, 0);
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&a, "-map", "1:a:0"));
        let b = build(true, 0);
        assert!(!b.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&b, "-map", "0:a:0"));
    }

    #[test]
    fn timestamps_continue_across_items() {
        let a = build(true, 0);
        assert!(has_pair(&a, "-output_ts_offset", "5.000"));
        assert!(has_pair(&a, "-f", "mpegts"));
        assert_eq!(a.last().unwrap(), "pipe:1");
    }

    #[test]
    fn normalizes_to_profile() {
        let a = build(true, 0);
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("scale=1280:720"));
        assert!(vf.contains("pad=1280:720"));
        assert!(vf.contains("fps=30"));
    }

    #[test]
    fn rendered_bumpers_scale_up_without_smoothing() {
        let a = rendered_args(
            640,
            360,
            4_000,
            2_000,
            &OutputProfile::default(),
            Encoder::Software,
        );
        assert!(has_pair(&a, "-video_size", "640x360"));
        assert!(has_pair(&a, "-f", "rawvideo"));
        assert!(has_pair(&a, "-t", "4.000"));
        assert!(has_pair(&a, "-output_ts_offset", "2.000"));
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("scale=1280:720:flags=neighbor"));
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
    }

    #[test]
    fn slate_is_silent_video_with_matching_layout() {
        let a = slate_args(
            "Retro TV",
            5_000,
            1_000,
            &OutputProfile::default(),
            Encoder::Software,
            None,
        );
        assert!(a.iter().any(|x| x.starts_with("color=c=")));
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&a, "-output_ts_offset", "1.000"));
        assert!(has_pair(&a, "-t", "5.000"));
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert_eq!(vf, "format=yuv420p", "no text without a font");
    }

    #[test]
    fn slate_text_is_sanitized_when_a_font_is_given() {
        let font = Path::new("/fonts/a.ttf");
        let a = slate_args(
            "Bob's: 100% \\ TV",
            1_000,
            0,
            &OutputProfile::default(),
            Encoder::Software,
            Some(font),
        );
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("text='Bobs 100  TV'"), "{vf}");
        assert!(vf.contains("fontfile='/fonts/a.ttf'"));
    }

    #[test]
    fn segmenter_uses_burst_when_available() {
        let d = Path::new("/tmp/x");
        let a = segmenter_args(d, &OutputProfile::default(), true);
        assert!(has_pair(&a, "-readrate_initial_burst", "6"));
        let b = segmenter_args(d, &OutputProfile::default(), false);
        assert!(b.contains(&"-re".to_string()));
        assert!(b.last().unwrap().ends_with("index.m3u8"));
    }
}
