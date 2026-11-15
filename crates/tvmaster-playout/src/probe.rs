//! Does an encoder actually work on this machine? FFmpeg lists every encoder
//! it was built with, GPU or not, so the only way to know is to encode.

use crate::profile::{slate_args, OutputProfile};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tvmaster_media::Encoder;

/// Length of the test clip.
const CLIP_MS: u64 = 3000;
/// A probe that takes longer than this is treated as a hang.
const LIMIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct EncoderProbe {
    pub encoder: Encoder,
    pub ok: bool,
    /// How many times faster than real time the test clip encoded. A channel
    /// needs well above 1.0 (the playout buffer is only a few seconds).
    pub speed: Option<f64>,
    /// FFmpeg's last error line when it failed.
    pub error: Option<String>,
}

/// The line of FFmpeg's output that says why it failed. The last line is often
/// a consequence ("Nothing was written into output file"); the cause comes
/// earlier and says something like "Cannot load libcuda" or "No VA display".
fn reason(stderr: &str) -> String {
    const CAUSES: [&str; 8] = [
        "cannot",
        "failed",
        "error",
        "unable",
        "no such",
        "not found",
        "no device",
        "not supported",
    ];
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines
        .iter()
        .find(|l| {
            let l = l.to_ascii_lowercase();
            !l.contains("nothing was written") && CAUSES.iter().any(|c| l.contains(c))
        })
        .or(lines.last())
        .map_or("encoder produced no output".to_string(), |l| l.to_string())
}

/// Encode a short clip with `enc` at the playout profile and report the result.
pub fn probe_encoder(ffmpeg: &Path, enc: Encoder) -> EncoderProbe {
    let profile = OutputProfile::default();
    let args = slate_args("probe", CLIP_MS, 0, &profile, enc, None);
    let failed = |error: String| EncoderProbe {
        encoder: enc,
        ok: false,
        speed: None,
        error: Some(error),
    };
    let started = Instant::now();
    let mut child = match Command::new(ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return failed(format!("could not start {}: {e}", ffmpeg.display())),
    };
    // Drain stdout on a thread so a full pipe cannot stall the encoder.
    let mut stdout = child.stdout.take().expect("piped");
    let drain = std::thread::spawn(move || {
        let mut n = 0u64;
        let mut buf = [0u8; 64 * 1024];
        while let Ok(k) = std::io::Read::read(&mut stdout, &mut buf) {
            if k == 0 {
                break;
            }
            n += k as u64;
        }
        n
    });
    let mut stderr = child.stderr.take().expect("piped");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
        s
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if started.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return failed("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return failed(e.to_string()),
        }
    };
    let elapsed = started.elapsed().as_secs_f64();
    let bytes = drain.join().unwrap_or(0);
    let stderr = err_reader.join().unwrap_or_default();
    if !status.success() || bytes == 0 {
        return failed(reason(&stderr));
    }
    EncoderProbe {
        encoder: enc,
        ok: true,
        speed: Some(CLIP_MS as f64 / 1000.0 / elapsed.max(0.001)),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cause_beats_the_consequence() {
        let out = "[vaapi @ 0x1] Failed to initialise VAAPI connection: -1 (unknown libva error).\n[out#0/mpegts @ 0x2] Nothing was written into output file, because at least one of its streams received no packets.\n";
        assert!(reason(out).contains("Failed to initialise VAAPI"));
        assert_eq!(reason("one\ntwo\n"), "two");
        assert_eq!(reason(""), "encoder produced no output");
    }

    #[test]
    fn a_missing_ffmpeg_fails_the_probe_with_a_reason() {
        let p = probe_encoder(Path::new("ffmpeg-that-does-not-exist"), Encoder::Software);
        assert!(!p.ok);
        assert!(p.error.unwrap().contains("could not start"));
    }

    #[test]
    fn software_encoding_passes_when_ffmpeg_is_installed() {
        if !tvmaster_media::run_ok(Path::new("ffmpeg"), &["-version"]) {
            return;
        }
        let p = probe_encoder(Path::new("ffmpeg"), Encoder::Software);
        assert!(p.ok, "{:?}", p.error);
        assert!(p.speed.unwrap() > 0.0);
    }

    /// An encoder the build lacks fails cleanly instead of hanging or panicking.
    #[test]
    fn an_encoder_the_build_lacks_fails_with_ffmpegs_message() {
        if !tvmaster_media::run_ok(Path::new("ffmpeg"), &["-version"]) {
            return;
        }
        // AMF needs an AMD GPU and the Windows driver, so it fails on a CI Linux box and a Mac.
        if cfg!(windows) {
            return;
        }
        let p = probe_encoder(Path::new("ffmpeg"), Encoder::Amf);
        assert!(!p.ok);
        assert!(p.error.is_some());
    }
}
