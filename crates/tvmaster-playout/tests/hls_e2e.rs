//! End-to-end: real FFmpeg turns three mismatched clips into one continuous
//! HLS stream. Skipped when FFmpeg is not installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tvmaster_core::{Entry, Timeline};
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{ChannelSpec, EngineConfig, Manager, MediaFile};

fn gen_clip(dir: &Path, name: &str, size: &str, rate: u32, secs: u32, audio: bool) -> PathBuf {
    let path = dir.join(name);
    let mut c = Command::new("ffmpeg");
    c.args(["-hide_banner", "-loglevel", "error", "-y"]);
    c.args([
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=size={size}:rate={rate}:duration={secs}"),
    ]);
    if audio {
        c.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:duration={secs}"),
        ]);
    }
    c.args(["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    if audio {
        c.args(["-c:a", "aac"]);
    }
    c.arg(&path);
    assert!(c.status().unwrap().success(), "clip generation failed");
    path
}

/// Probe a *finished copy* of the live playlist: a live playlist (no ENDLIST)
/// makes ffprobe wait for new segments forever.
fn packet_times(playlist: &Path, stream: &str) -> Vec<f64> {
    let mut text = std::fs::read_to_string(playlist).unwrap();
    text.push_str("#EXT-X-ENDLIST\n");
    let frozen = playlist.with_file_name("frozen.m3u8");
    std::fs::write(&frozen, text).unwrap();
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", stream])
        .args(["-show_entries", "packet=pts_time", "-of", "csv=p=0"])
        .arg(&frozen)
        .output()
        .unwrap();
    eprintln!(
        "ffprobe({stream}) status {:?}, stdout head: {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(3)
            .collect::<Vec<_>>()
    );
    if out.stdout.is_empty() {
        eprintln!(
            "ffprobe({stream}) stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut v: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn max_gap(times: &[f64]) -> f64 {
    times.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max)
}

#[tokio::test(flavor = "multi_thread")]
async fn mixed_clips_become_one_continuous_stream() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        eprintln!("libx264 missing; skipping");
        return;
    }

    let root = std::env::temp_dir().join(format!("tvmaster-e2e-{}", std::process::id()));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    // Different sizes, frame rates, and one with no audio: all must normalize.
    let a = gen_clip(&media, "a.mp4", "640x360", 25, 3, true);
    let b = gen_clip(&media, "b.mp4", "320x240", 30, 4, false);
    let c = gen_clip(&media, "c.mp4", "480x360", 24, 3, true);

    let files: HashMap<i64, MediaFile> = [(1, a, true), (2, b, false), (3, c, true)]
        .into_iter()
        .map(|(id, path, has_audio)| (id, MediaFile { path, has_audio }))
        .collect();
    let entries = vec![
        Entry {
            media_id: 1,
            duration_ms: 3000,
        },
        Entry {
            media_id: 2,
            duration_ms: 4000,
        },
        Entry {
            media_id: 3,
            duration_ms: 3000,
        },
    ];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let timeline = Timeline::new(entries, now, 0, false).unwrap();

    let mut cfg = EngineConfig::new(&info.ffmpeg, tvmaster_media::Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    let manager = Manager::new(cfg);
    assert!(manager.start_if_absent(ChannelSpec {
        id: "test".into(),
        always_on: true,
        timeline,
        files,
    }));
    assert!(
        !manager.start_if_absent(ChannelSpec {
            id: "test".into(),
            always_on: true,
            timeline: Timeline::new(
                vec![Entry {
                    media_id: 1,
                    duration_ms: 1000
                }],
                now,
                0,
                false
            )
            .unwrap(),
            files: HashMap::new(),
        }),
        "second start must be a no-op"
    );
    assert!(manager.touch("test"));

    // First playlist should appear quickly (tune-in latency).
    let playlist = manager.hls_dir("test").join("index.m3u8");
    let t0 = Instant::now();
    while !playlist.exists()
        || !std::fs::read_to_string(&playlist)
            .unwrap_or_default()
            .contains("#EXTINF")
    {
        assert!(
            t0.elapsed() < Duration::from_secs(30),
            "no playlist after 30s"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    eprintln!("tune-in latency: {:?}", t0.elapsed());

    // Let it run past both item boundaries (3s, 7s) and into the next pass (10s).
    tokio::time::sleep(Duration::from_secs(14)).await;

    let text = std::fs::read_to_string(&playlist).unwrap();
    assert!(text.contains("#EXT-X-PROGRAM-DATE-TIME"));
    let video = packet_times(&playlist, "v:0");
    let audio = packet_times(&playlist, "a:0");
    assert!(video.len() > 100, "too few video packets: {}", video.len());
    assert!(audio.len() > 100, "too few audio packets: {}", audio.len());
    let (vg, ag) = (max_gap(&video), max_gap(&audio));
    eprintln!(
        "video packets {} max gap {vg:.3}s; audio packets {} max gap {ag:.3}s",
        video.len(),
        audio.len()
    );
    assert!(vg < 0.25, "video gap at an item boundary: {vg}");
    assert!(ag < 0.25, "audio gap at an item boundary: {ag}");

    // Stream must span the boundaries (a 10s window at least).
    let span = video.last().unwrap() - video.first().unwrap();
    assert!(span > 9.0, "playlist spans only {span}s");

    manager.stop("test");
    assert!(!manager.is_running("test"));
    let _ = std::fs::remove_dir_all(&root);
}
