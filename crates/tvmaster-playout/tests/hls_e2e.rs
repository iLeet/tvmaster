//! End-to-end: real FFmpeg turns three mismatched clips into one continuous
//! HLS stream. Skipped when FFmpeg is not installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tvmaster_core::BumperKind;
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{
    ChannelSpec, EngineConfig, Manager, PlayItem, PlayKind, ScheduleSource, Upcoming,
};

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

/// A fixed schedule plus a record of what playout reported as unplayable.
struct TestSchedule {
    items: Vec<PlayItem>,
    unplayable: Mutex<Vec<i64>>,
}

impl ScheduleSource for TestSchedule {
    fn item_at_or_after(&self, _channel: &str, at_ms: i64) -> Option<PlayItem> {
        self.items.iter().find(|i| i.end_ms > at_ms).cloned()
    }

    fn report_unplayable(&self, _channel: &str, media_id: i64) {
        self.unplayable.lock().unwrap().push(media_id);
    }

    fn upcoming(&self, _channel: &str, _after_ms: i64, _limit: usize) -> Upcoming {
        Upcoming {
            items: vec![("8:30 PM".into(), "Next Show".into())],
            evening: true,
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::test(flavor = "multi_thread")]
async fn mixed_clips_slate_and_missing_file_become_one_continuous_stream() {
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

    // A realistic mix, repeated: act one of a show (from the start), a
    // commercial, a rendered "we'll be right back" and "up next" bumper, act
    // two of the show (starting part-way in), a silent clip, scheduled filler,
    // a file that does not exist.
    let start = now_ms();
    let mut items = Vec::new();
    let mut t = start;
    for _ in 0..3 {
        for (kind, media_id, path, has_audio, in_ms, secs) in [
            (PlayKind::Media, Some(1), Some(a.clone()), true, 0, 2),
            (PlayKind::Media, Some(2), Some(c.clone()), true, 0, 2),
            (
                PlayKind::Bumper(BumperKind::Brb),
                Some(1),
                None,
                false,
                0,
                3,
            ),
            (PlayKind::Media, Some(1), Some(a.clone()), true, 2_000, 1),
            (
                PlayKind::Bumper(BumperKind::UpNext),
                None,
                None,
                false,
                0,
                3,
            ),
            (PlayKind::Media, Some(3), Some(b.clone()), false, 0, 3),
            (PlayKind::Slate, None, None, false, 0, 1),
            (
                PlayKind::Media,
                Some(99),
                Some(media.join("gone.mp4")),
                true,
                0,
                2,
            ),
        ] {
            items.push(PlayItem {
                kind,
                media_id,
                path,
                has_audio,
                in_ms,
                subject: Some("Test Show".into()),
                start_ms: t,
                end_ms: t + secs * 1000,
                is_show: false,
                show: None,
                subtitle: None,
                tracks: None,
            });
            t += secs * 1000;
        }
    }
    let schedule = Arc::new(TestSchedule {
        items,
        unplayable: Mutex::new(Vec::new()),
    });

    let mut cfg = EngineConfig::new(&info.ffmpeg, tvmaster_media::Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    let manager = Manager::new(cfg);
    let spec = || ChannelSpec {
        dayparts: Default::default(),
        info: None,
        id: "test".into(),
        name: "Test TV".into(),
        always_on: true,
        source: schedule.clone(),
        guide: None,
        options: Default::default(),
        number: 0,
        can_burn_text: false,
        signoff: Default::default(),
    };
    // A previous run's playlist must never be served after a (re)start.
    let stale_dir = manager.hls_dir("test");
    std::fs::create_dir_all(&stale_dir).unwrap();
    std::fs::write(
        stale_dir.join("index.m3u8"),
        "#EXTM3U\n#EXTINF:2.0,\nseg_stale.ts\n",
    )
    .unwrap();
    assert!(manager.start_if_absent(spec()));
    assert!(
        !std::fs::read_to_string(stale_dir.join("index.m3u8"))
            .unwrap_or_default()
            .contains("seg_stale"),
        "stale playlist removed synchronously on start"
    );
    assert!(
        !manager.start_if_absent(spec()),
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

    // Run past every boundary (17 seconds per pass).
    tokio::time::sleep(Duration::from_secs(19)).await;

    let text = std::fs::read_to_string(&playlist).unwrap();
    assert!(text.contains("#EXT-X-PROGRAM-DATE-TIME"));
    let video = packet_times(&playlist, "v:0");
    let audio = packet_times(&playlist, "a:0");
    assert!(video.len() > 300, "too few video packets: {}", video.len());
    assert!(audio.len() > 300, "too few audio packets: {}", audio.len());
    let (vg, ag) = (max_gap(&video), max_gap(&audio));
    eprintln!(
        "video packets {} max gap {vg:.3}s; audio packets {} max gap {ag:.3}s",
        video.len(),
        audio.len()
    );
    assert!(vg < 0.25, "video gap at an item boundary: {vg}");
    assert!(ag < 0.25, "audio gap at an item boundary: {ag}");

    let span = video.last().unwrap() - video.first().unwrap();
    assert!(span > 16.0, "stream covers only {span}s");

    // Playout encodes ahead of real time, so it may reach the missing file in
    // every repeat of the pattern; each time it is reported and slated.
    let reported = schedule.unplayable.lock().unwrap().clone();
    assert!(
        !reported.is_empty() && reported.iter().all(|id| *id == 99),
        "{reported:?}"
    );

    manager.stop("test");
    assert!(!manager.is_running("test"));
    let _ = std::fs::remove_dir_all(&root);
}
