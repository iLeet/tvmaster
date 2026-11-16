//! End-to-end: the guide channel renders live frames with music under them.
//! Skipped when FFmpeg is not installed.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tvmaster_media::{Encoder, FfmpegInfo};
use tvmaster_playout::{
    ChannelSpec, EngineConfig, GuideData, GuideSpec, Manager, PlayItem, ScheduleSource,
};
use tvmaster_render::{GuideCell, GuideRow};

struct GuideSource {
    calls: AtomicUsize,
}

impl ScheduleSource for GuideSource {
    fn item_at_or_after(&self, _: &str, _: i64) -> Option<PlayItem> {
        None
    }

    fn report_unplayable(&self, _: &str, _: i64) {}

    fn guide_data(&self, _: &str, now_ms: i64) -> Option<GuideData> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) as i64;
        let half = 30 * 60_000;
        Some(GuideData {
            title: "Test Guide".into(),
            now_ms,
            window_start_ms: now_ms - 10 * 60_000,
            window_ms: 3 * 60 * 60_000,
            clock: format!("9:{:02} PM", n % 60),
            date: "SUN NOV 15".into(),
            marks: vec![(now_ms, "9:00 PM".into())],
            rows: vec![GuideRow {
                number: 3,
                name: "Guide".into(),
                cells: vec![GuideCell {
                    start_ms: now_ms - half,
                    end_ms: now_ms + half,
                    title: "Something On".into(),
                }],
            }],
            ticker: "Stay tuned".into(),
        })
    }
}

fn frozen(playlist: &Path) -> std::path::PathBuf {
    let mut text = std::fs::read_to_string(playlist).unwrap();
    text.push_str("#EXT-X-ENDLIST\n");
    let f = playlist.with_file_name("frozen.m3u8");
    std::fs::write(&f, text).unwrap();
    f
}

#[tokio::test(flavor = "multi_thread")]
async fn guide_channel_draws_frames_and_loops_music() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        eprintln!("libx264 missing; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-guide-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // a short tone, shorter than the run, so looping matters
    let music = root.join("tone.wav");
    assert!(Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i"
        ])
        .arg("sine=frequency=440:duration=3")
        .arg(&music)
        .status()
        .unwrap()
        .success());

    let source = Arc::new(GuideSource {
        calls: AtomicUsize::new(0),
    });
    let mut cfg = EngineConfig::new(&info.ffmpeg, Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    let manager = Manager::new(cfg);
    assert!(manager.start_if_absent(ChannelSpec {
        id: "guide".into(),
        name: "Guide".into(),
        always_on: true,
        source: source.clone(),
        guide: Some(GuideSpec { music: vec![music] }),
        options: Default::default(),
        number: 0,
        can_burn_text: false,
        signoff: Default::default(),
    }));

    let playlist = manager.hls_dir("guide").join("index.m3u8");
    let t0 = Instant::now();
    while !std::fs::read_to_string(&playlist)
        .unwrap_or_default()
        .contains("#EXTINF")
    {
        assert!(
            t0.elapsed() < Duration::from_secs(30),
            "no playlist after 30s"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    eprintln!("guide tune-in latency: {:?}", t0.elapsed());
    tokio::time::sleep(Duration::from_secs(12)).await;

    let f = frozen(&playlist);
    // video: continuous, and the pictures are not blank
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(&f)
        .output()
        .unwrap();
    let mut v: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(v.len() > 200, "video packets: {}", v.len());
    let gap = v.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    assert!(gap < 0.2, "gap {gap}");

    // audio: present for the whole span even though the tone is only 3s long
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(&f)
        .output()
        .unwrap();
    let a: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    let span =
        a.iter().cloned().fold(f64::MIN, f64::max) - a.iter().cloned().fold(f64::MAX, f64::min);
    assert!(span > 6.0, "music loops under the whole stream: {span}s");

    // a decoded frame has real content (text and colour), not a flat picture
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&f)
        .args([
            "-ss",
            "2",
            "-frames:v",
            "1",
            "-vf",
            "scale=64:36",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .unwrap();
    let px = out.stdout;
    assert_eq!(px.len(), 64 * 36 * 3);
    let mean = px.iter().map(|b| *b as f64).sum::<f64>() / px.len() as f64;
    let var = px.iter().map(|b| (*b as f64 - mean).powi(2)).sum::<f64>() / px.len() as f64;
    assert!(var.sqrt() > 20.0, "flat picture: stddev {}", var.sqrt());

    assert!(
        source.calls.load(Ordering::SeqCst) >= 2,
        "the listing is refreshed while streaming"
    );
    manager.stop("guide");
    let _ = std::fs::remove_dir_all(&root);
}
