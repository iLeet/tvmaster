//! End-to-end: designed screens (an "Up next" and a stand-by) go through real FFmpeg and come out as
//! a continuous, animated stream. Skipped when FFmpeg is not installed.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tvmaster_core::BumperKind;
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{
    ChannelSpec, EngineConfig, Manager, PlayItem, PlayKind, ScheduleSource, Upcoming,
};

struct Fixed(Vec<PlayItem>);

impl ScheduleSource for Fixed {
    fn item_at_or_after(&self, _c: &str, at_ms: i64) -> Option<PlayItem> {
        self.0.iter().find(|i| i.end_ms > at_ms).cloned()
    }
    fn report_unplayable(&self, _c: &str, _m: i64) {}
    fn upcoming(&self, _c: &str, _after: i64, _limit: usize) -> Upcoming {
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

fn item(kind: PlayKind, start: i64, secs: i64) -> PlayItem {
    PlayItem {
        kind,
        media_id: None,
        path: None,
        has_audio: false,
        in_ms: 0,
        subject: Some("Test Show".into()),
        start_ms: start,
        end_ms: start + secs * 1000,
        is_show: false,
        show: None,
        subtitle: None,
        tracks: None,
    }
}

fn frame_hash(playlist: &Path, at: &str) -> String {
    let mut text = std::fs::read_to_string(playlist).unwrap();
    text.push_str("#EXT-X-ENDLIST\n");
    let frozen = playlist.with_file_name("frozen.m3u8");
    std::fs::write(&frozen, text).unwrap();
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", at, "-i"])
        .arg(&frozen)
        .args(["-frames:v", "1", "-f", "md5", "-"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_designed_up_next_and_stand_by_air_and_move() {
    let Ok(info) = FfmpegInfo::detect("ffmpeg", "ffprobe") else {
        eprintln!("ffmpeg not installed; skipping");
        return;
    };
    if !info.has_libx264 {
        eprintln!("libx264 missing; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-screens-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();

    let start = now_ms();
    let items = vec![
        item(PlayKind::Bumper(BumperKind::UpNext), start, 4),
        item(PlayKind::Slate, start + 4_000, 6),
    ];
    let design = |name: &str| {
        serde_json::json!({
            "name": name,
            "backdrop": { "type": "aurora", "colors": [] },
            "layers": [
                { "type": "shape", "w": 600, "h": 300, "glass": true, "radius": 40, "anim": { "enter": "zoom" } },
                { "type": "text", "text": "{channel} {title} {time}", "size": 60, "anim": { "enter": "slide_up", "motion": "float" } },
                { "type": "clock", "seconds": true, "x": 500, "y": 800 }
            ]
        })
    };
    let mut options = tvmaster_core::ChannelOptions::default();
    options.screens.insert("up_next".into(), design("up"));
    options.screens.insert("standby".into(), design("stand"));

    let mut cfg = EngineConfig::new(&info.ffmpeg, tvmaster_media::Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    let manager = Manager::new(cfg);
    assert!(manager.start_if_absent(ChannelSpec {
        dayparts: Default::default(),
        info: None,
        id: "screens".into(),
        name: "Test TV".into(),
        always_on: true,
        source: Arc::new(Fixed(items)),
        guide: None,
        options,
        number: 7,
        can_burn_text: false,
        signoff: Default::default(),
    }));
    assert!(manager.touch("screens"));
    let playlist = manager.hls_dir("screens").join("index.m3u8");
    let t0 = Instant::now();
    while !std::fs::read_to_string(&playlist)
        .unwrap_or_default()
        .contains("#EXTINF")
    {
        assert!(t0.elapsed() < Duration::from_secs(40), "no playlist");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(11)).await;

    // The designed "up next" (first 4 seconds) is not the same picture a second apart: it animates.
    let (a, b) = (frame_hash(&playlist, "0.3"), frame_hash(&playlist, "2.5"));
    assert!(!a.is_empty() && !b.is_empty(), "frames decoded");
    assert_ne!(a, b, "the up-next design moves");
    // and the stand-by that follows keeps moving too (its clock and its glow)
    let (c, d) = (frame_hash(&playlist, "5.0"), frame_hash(&playlist, "9.0"));
    assert!(!c.is_empty() && !d.is_empty());
    assert_ne!(c, d, "the stand-by design moves");

    manager.stop("screens");
    let _ = std::fs::remove_dir_all(&root);
}
