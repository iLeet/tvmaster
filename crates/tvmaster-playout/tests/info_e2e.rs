//! End-to-end: an info channel draws its pages live, with the clock, the ticker and music. Skipped
//! when FFmpeg is not installed.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tvmaster_core::{InfoConfig, InfoSlide, SlideKind};
use tvmaster_media::{Encoder, FfmpegInfo};
use tvmaster_playout::{
    ChannelSpec, EngineConfig, InfoSpec, Manager, PlayItem, ScheduleSource, TonightRow,
};

struct Source {
    asked: AtomicUsize,
}

impl ScheduleSource for Source {
    fn item_at_or_after(&self, _: &str, _: i64) -> Option<PlayItem> {
        None
    }
    fn report_unplayable(&self, _: &str, _: i64) {}
    fn headlines(&self, _: &str) -> Vec<String> {
        vec![
            "Town fair opens Saturday".into(),
            "Bridge repairs finish early".into(),
        ]
    }
    fn tonight(&self, _: &str, _: i64) -> Vec<TonightRow> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        vec![TonightRow {
            number: 4,
            channel: "Retro".into(),
            now: "Cheers".into(),
            until: "9:00 PM".into(),
            next: "Frasier".into(),
            at: "9:00 PM".into(),
        }]
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
async fn an_info_channel_draws_its_pages_and_keeps_the_stream_continuous() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        eprintln!("libx264 missing; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-info-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = Arc::new(Source {
        asked: AtomicUsize::new(0),
    });
    let mut cfg = EngineConfig::new(&info.ffmpeg, Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    let manager = Manager::new(cfg);
    let config = InfoConfig {
        slides: vec![
            InfoSlide {
                kind: SlideKind::Text,
                title: "Garage sale".into(),
                lines: vec!["Saturday, 8 to 2".into()],
                seconds: Some(4),
                ..Default::default()
            },
            InfoSlide {
                kind: SlideKind::Tonight,
                seconds: Some(4),
                ..Default::default()
            },
            InfoSlide {
                kind: SlideKind::Headlines,
                seconds: Some(4),
                ..Default::default()
            },
        ],
        feeds: vec!["https://example.com/feed.xml".into()],
        ..Default::default()
    };
    assert!(manager.start_if_absent(ChannelSpec {
        dayparts: Default::default(),
        info: Some(InfoSpec {
            config,
            music: vec![],
            zone: None,
        }),
        id: "news".into(),
        name: "News".into(),
        always_on: true,
        source: source.clone(),
        guide: None,
        options: Default::default(),
        number: 6,
        can_burn_text: false,
        signoff: Default::default(),
    }));

    let playlist = manager.hls_dir("news").join("index.m3u8");
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
    tokio::time::sleep(Duration::from_secs(12)).await;
    let f = frozen(&playlist);

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
    assert!(v.len() > 150, "video packets: {}", v.len());
    let gap = v.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    assert!(gap < 0.2, "gap {gap}");
    // There is sound too (silence, with no music), for players that need an audio stream.
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(&f)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&out.stdout).trim().is_empty());

    // Two frames a few seconds apart are different pages.
    let grab = |at: &str| {
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&f)
            .args([
                "-ss",
                at,
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
        assert_eq!(out.stdout.len(), 64 * 36 * 3);
        out.stdout
    };
    let (a, b) = (grab("1"), grab("5"));
    let sd = |px: &[u8]| {
        let mean = px.iter().map(|b| *b as f64).sum::<f64>() / px.len() as f64;
        (px.iter().map(|b| (*b as f64 - mean).powi(2)).sum::<f64>() / px.len() as f64).sqrt()
    };
    assert!(sd(&a) > 15.0, "flat picture {}", sd(&a));
    assert_ne!(a, b, "the page changed");
    assert!(
        source.asked.load(Ordering::SeqCst) >= 1,
        "what is on tonight was asked for"
    );
    manager.stop("news");
    let _ = std::fs::remove_dir_all(&root);
}
