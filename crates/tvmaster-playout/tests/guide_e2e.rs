//! End-to-end: the guide channel renders live frames with music under them.
//! Skipped when FFmpeg is not installed.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tvmaster_media::{Encoder, FfmpegInfo};
use tvmaster_playout::{
    ChannelSpec, EngineConfig, GuideData, GuideSpec, Manager, PipHost, PlayItem, ScheduleSource,
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
            rows: ["red", "green"]
                .iter()
                .enumerate()
                .map(|(i, id)| GuideRow {
                    id: id.to_string(),
                    number: 3 + i as u32,
                    name: id.to_string(),
                    cells: vec![GuideCell {
                        start_ms: now_ms - half,
                        end_ms: now_ms + half,
                        title: "Something On".into(),
                        range: "9:00-9:30 PM".into(),
                    }],
                })
                .collect(),
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
        guide: Some(GuideSpec {
            music: vec![music],
            ..Default::default()
        }),
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

/// Stands in for the server: "opens" a channel by making a live HLS stream of
/// one flat colour, named for it.
struct ColourChannels {
    root: std::path::PathBuf,
    procs: std::sync::Mutex<std::collections::HashMap<String, std::process::Child>>,
    opened: std::sync::Mutex<Vec<String>>,
}

impl PipHost for ColourChannels {
    fn open(&self, channel: &str) -> Option<std::path::PathBuf> {
        self.opened.lock().unwrap().push(channel.to_string());
        let dir = self.root.join(format!("pip-{channel}"));
        let playlist = dir.join("index.m3u8");
        let mut procs = self.procs.lock().unwrap();
        if !procs.contains_key(channel) {
            std::fs::create_dir_all(&dir).ok()?;
            let child = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                ])
                .arg(format!("color=c={channel}:s=320x180:r=15"))
                .args(["-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=48000"])
                .args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-g",
                    "15",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .args(["-c:a", "aac", "-t", "150", "-f", "hls", "-hls_time", "2"])
                .args([
                    "-hls_list_size",
                    "6",
                    "-hls_flags",
                    "delete_segments+omit_endlist",
                ])
                .arg(&playlist)
                .stdin(std::process::Stdio::null())
                .spawn()
                .ok()?;
            procs.insert(channel.to_string(), child);
        }
        drop(procs);
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(12) {
            if std::fs::read_to_string(&playlist).is_ok_and(|t| t.matches("#EXTINF").count() >= 2) {
                return Some(playlist);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        None
    }
}

impl Drop for ColourChannels {
    fn drop(&mut self) {
        for c in self.procs.lock().unwrap().values_mut() {
            let _ = c.kill();
        }
    }
}

/// The picture's colour at `at` seconds in, averaged over a small square at (x, y).
fn colour_at(playlist: &Path, at: f64, x: i32, y: i32) -> [u8; 3] {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &at.to_string(), "-i"])
        .arg(playlist)
        .args(["-frames:v", "1", "-vf"])
        .arg(format!("crop=12:12:{x}:{y},scale=1:1"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert_eq!(out.stdout.len(), 3, "a frame at {at}s");
    [out.stdout[0], out.stdout[1], out.stdout[2]]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rotating_video_window_shows_each_channel_in_turn_without_breaking_the_stream() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        eprintln!("libx264 missing; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-pip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // Music made of two different tones, so it is clear whether it carried on or started over.
    let music = root.join("tones.wav");
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
        .arg("sine=frequency=440:duration=40")
        .arg(&music)
        .status()
        .unwrap()
        .success());

    let host = Arc::new(ColourChannels {
        root: root.clone(),
        procs: Default::default(),
        opened: Default::default(),
    });
    let mut config = tvmaster_core::GuideConfig::default();
    config.pip.mode = tvmaster_core::PipMode::Rotate;
    config.pip.channels = vec!["red".into(), "green".into()];
    config.pip.seconds = 10;
    config.pip.placement = tvmaster_core::PipPlacement::SideRight;
    config.pip.size = tvmaster_core::PipSize::Medium;
    let geo = tvmaster_render::geometry(&tvmaster_playout::guide_look(&config, "guide"));
    let w = geo.window.expect("a window");
    // the middle of the window in the 1280x720 output
    let (cx, cy) = ((w.x + w.w / 2) * 2 - 6, (w.y + w.h / 2) * 2 - 6);

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
        guide: Some(GuideSpec {
            music: vec![music],
            config,
            pip_host: Some(host.clone()),
        }),
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
            t0.elapsed() < Duration::from_secs(40),
            "no playlist after 40s"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Long enough for two full rotations plus the start of a third.
    tokio::time::sleep(Duration::from_secs(34)).await;

    let f = frozen(&playlist);
    if let Ok(dir) = std::env::var("TVM_SAVE_FRAMES") {
        for (i, at) in [4.0, 15.0].iter().enumerate() {
            let _ = Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-ss", &at.to_string(), "-i"])
                .arg(&f)
                .args(["-frames:v", "1"])
                .arg(format!("{dir}/pip-{i}.png"))
                .status();
        }
    }
    // The playlist is a sliding window, so look at whatever part is left: the
    // window should show one colour for about ten seconds, then the other.
    let total = String::from_utf8_lossy(
        &Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&f)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .parse::<f64>()
    .unwrap();
    assert!(total > 12.0, "enough stream to look at: {total}s");
    let kinds: Vec<char> = (0..)
        .map(|i| 0.5 + i as f64 * 1.5)
        .take_while(|t| *t < total - 1.0)
        .map(|t| {
            let c = colour_at(&f, t, cx, cy);
            if c[0] > 150 && c[1] < 80 && c[2] < 80 {
                'R'
            } else if c[1] > 100 && c[0] < 80 && c[2] < 80 {
                'G'
            } else {
                '?'
            }
        })
        .collect();
    let seq: String = kinds.iter().collect();
    eprintln!("window colours every 1.5s over {total}s: {seq}");
    assert!(
        seq.contains('R') && seq.contains('G'),
        "both channels were shown: {seq}"
    );
    assert!(
        seq.matches('?').count() <= 3,
        "the window is video nearly all the time: {seq}"
    );
    // Each stays for several samples before the other takes over (a rotation is 10 s).
    let turns = seq
        .replace('?', "")
        .chars()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0] != w[1])
        .count();
    assert!(
        (1..=3).contains(&turns),
        "channels take turns, not flicker: {seq}"
    );
    // The listing around the window is still the guide, not video.
    let left = colour_at(&f, 3.0, 60, 400);
    assert!(
        !(left[0] > 150 && left[1] < 80 && left[2] < 80),
        "the guide itself is not red: {left:?}"
    );

    // One continuous stream across the joins: no gaps, no backwards timestamps.
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
    assert!(v.len() > 400, "video packets: {}", v.len());
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let gap = v.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    assert!(gap < 0.25, "a gap of {gap}s in the video");
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
    let agap = a.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    assert!(agap < 0.25, "a gap of {agap}s in the audio");

    let opened = host.opened.lock().unwrap().clone();
    assert!(
        opened.iter().filter(|c| *c == "red").count() >= 2 && opened.contains(&"green".to_string()),
        "{opened:?}"
    );
    manager.stop("guide");
    // Stopping the guide stops it asking for its window's channels: nothing keeps them on air.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = host.opened.lock().unwrap().len();
    tokio::time::sleep(Duration::from_secs(13)).await;
    assert_eq!(
        host.opened.lock().unwrap().len(),
        before,
        "a stopped guide went on keeping its channel alive"
    );
    drop(host);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_video_window_whose_channel_is_missing_shows_no_signal_and_the_guide_carries_on() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        return;
    }
    struct Nothing;
    impl PipHost for Nothing {
        fn open(&self, _: &str) -> Option<std::path::PathBuf> {
            None
        }
    }
    let root = std::env::temp_dir().join(format!("tvmaster-pip-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut config = tvmaster_core::GuideConfig::default();
    config.pip.mode = tvmaster_core::PipMode::Channel;
    config.pip.channel = Some("gone".into());
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
        source,
        guide: Some(GuideSpec {
            music: vec![],
            config,
            pip_host: Some(Arc::new(Nothing))
        }),
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
            "the guide never came up"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(6)).await;
    let f = frozen(&playlist);
    // The window is black with grey words, and the stream is flowing.
    let w = tvmaster_render::geometry(&tvmaster_playout::guide_look(
        &tvmaster_core::GuideConfig {
            pip: tvmaster_core::GuidePip {
                mode: tvmaster_core::PipMode::Channel,
                channel: Some("x".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        "guide",
    ))
    .window
    .unwrap();
    let c = colour_at(&f, 3.0, (w.x + 4) * 2, (w.y + 4) * 2);
    assert!(c.iter().all(|v| *v < 40), "the window is dark: {c:?}");
    manager.stop("guide");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_modern_guide_with_a_picture_background_streams_at_full_resolution() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        return;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-modern-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // A pure green picture to put behind the listing.
    let pic = root.join("bg.png");
    assert!(ffmpeg_ok(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x00c000:s=320x180",
        "-frames:v",
        "1",
        pic.to_str().unwrap()
    ]));
    let mut config = tvmaster_core::GuideConfig::default();
    config.look.typeface = tvmaster_core::GuideTypeface::Smooth;
    config.look.corners = tvmaster_core::GuideCorners::Rounded;
    config.look.background.style = tvmaster_core::BackgroundStyle::Image;
    config.look.background.image = Some(pic.to_string_lossy().to_string());
    config.look.background.dim = 0;
    config.layout.scanlines = false;
    config.theme = tvmaster_core::GuideTheme::Midnight;
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
        source,
        guide: Some(GuideSpec {
            music: vec![],
            config,
            pip_host: None
        }),
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
            "the guide never came up"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    let f = frozen(&playlist);
    // 1280x720 out, and the picture is behind the listing: the strip under the rows is green.
    let size = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(&f)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&size.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .all(|l| l == "1280,720"));
    let c = colour_at(&f, 2.0, 1270, 470);
    assert!(
        c[1] > 150 && c[0] < 40 && c[2] < 40,
        "the picture shows behind the listing: {c:?}"
    );
    manager.stop("guide");
    let _ = std::fs::remove_dir_all(&root);
}

fn ffmpeg_ok(args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
