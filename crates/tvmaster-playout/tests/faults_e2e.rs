//! Fault injection against real FFmpeg: the things that go wrong on a house
//! server. Each test puts a channel on air, breaks something underneath it,
//! and checks that the stream carries on. Skipped when FFmpeg is missing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{
    ChannelSpec, EngineConfig, Manager, PlayItem, PlayKind, ScheduleSource, Upcoming,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn gen_clip(dir: &Path, name: &str, secs: u32) -> PathBuf {
    let path = dir.join(name);
    let ok = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i"])
        .arg(format!("testsrc=size=320x240:rate=25:duration={secs}"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={secs}"))
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
        .arg(&path)
        .status()
        .unwrap()
        .success();
    assert!(ok, "clip generation failed");
    path
}

struct Schedule {
    items: Vec<PlayItem>,
    reported: Mutex<Vec<i64>>,
}

impl ScheduleSource for Schedule {
    fn item_at_or_after(&self, _channel: &str, at_ms: i64) -> Option<PlayItem> {
        self.items.iter().find(|i| i.end_ms > at_ms).cloned()
    }
    fn report_unplayable(&self, _channel: &str, media_id: i64) {
        self.reported.lock().unwrap().push(media_id);
    }
    fn upcoming(&self, _channel: &str, _after_ms: i64, _limit: usize) -> Upcoming {
        Upcoming::default()
    }
}

/// `count` back-to-back items of `secs` seconds, item `n` using `path(n)`, starting now.
fn schedule(count: i64, secs: i64, path: impl Fn(i64) -> PathBuf) -> Arc<Schedule> {
    let start = now_ms();
    let items = (0..count)
        .map(|n| PlayItem {
            kind: PlayKind::Media,
            media_id: Some(n),
            path: Some(path(n)),
            has_audio: true,
            in_ms: 0,
            subject: None,
            start_ms: start + n * secs * 1000,
            end_ms: start + (n + 1) * secs * 1000,
            is_show: false,
            show: None,
            subtitle: None,
            tracks: None,
        })
        .collect();
    Arc::new(Schedule {
        items,
        reported: Mutex::new(Vec::new()),
    })
}

struct Rig {
    manager: Arc<Manager>,
    root: PathBuf,
    media: PathBuf,
}

fn rig(test: &str) -> Option<Rig> {
    let info = FfmpegInfo::detect("ffmpeg", "ffprobe").ok()?;
    if !info.has_libx264 {
        return None;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-fault-{test}-{}", std::process::id()));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let mut cfg = EngineConfig::new(&info.ffmpeg, tvmaster_media::Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    Some(Rig {
        manager: Manager::new(cfg),
        root,
        media,
    })
}

impl Rig {
    fn start(&self, source: &Arc<Schedule>) {
        let spec = ChannelSpec {
            dayparts: Default::default(),
            info: None,
            id: "t".into(),
            name: "Test TV".into(),
            always_on: true,
            source: source.clone(),
            guide: None,
            options: Default::default(),
            number: 0,
            can_burn_text: false,
            signoff: Default::default(),
        };
        assert!(self.manager.start_if_absent(spec));
        self.manager.touch("t");
    }

    fn playlist(&self) -> String {
        std::fs::read_to_string(self.manager.hls_dir("t").join("index.m3u8")).unwrap_or_default()
    }

    /// The wall-clock time of the newest segment, which only ever moves
    /// forward while the channel is live, even across a restart (the
    /// segment numbers start again from zero).
    fn newest(&self) -> Option<String> {
        self.playlist()
            .lines()
            .filter_map(|l| l.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:"))
            .next_back()
            .map(str::to_string)
    }

    /// Wait for the stream to be live with something newer than `after`.
    async fn live_after(&self, after: Option<&str>, within: Duration) -> String {
        let t = Instant::now();
        loop {
            if self.playlist().contains("#EXTINF") {
                if let Some(n) = self.newest() {
                    if after.is_none_or(|a| n.as_str() > a) {
                        return n;
                    }
                }
            }
            assert!(
                t.elapsed() < within,
                "the stream did not come (back) within {within:?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn finish(self) {
        self.manager.stop("t");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Kill every process whose command line mentions `needle`.
#[cfg(unix)]
fn kill_matching(needle: &Path) -> bool {
    Command::new("pkill")
        .arg("-9")
        .arg("-f")
        .arg(needle)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The segmenter is killed: the watchdog restarts the channel and the
/// playlist comes back, newer than before.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn killing_the_segmenter_restarts_the_channel() {
    let Some(rig) = rig("segmenter") else { return };
    let clip = gen_clip(&rig.media, "a.mp4", 10);
    let sched = schedule(8, 8, |_| clip.clone());
    rig.start(&sched);
    let before = rig.live_after(None, Duration::from_secs(30)).await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert!(
        kill_matching(&rig.root.join("hls")),
        "no segmenter process found"
    );
    // The dead run's playlist is withdrawn, then the restart (after a short backoff) serves again.
    let after = rig.live_after(Some(&before), Duration::from_secs(40)).await;
    assert!(rig.manager.is_running("t"));
    // ...and it is genuinely live, not a leftover.
    rig.live_after(Some(&after), Duration::from_secs(15)).await;
    assert!(
        sched.reported.lock().unwrap().is_empty(),
        "a killed segmenter says nothing about the files"
    );
    rig.finish();
}

/// An item encoder is killed mid-item (out of memory, say): the stream
/// carries on, and the perfectly good file is NOT reported as unplayable.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_killed_item_encoder_does_not_condemn_the_file() {
    let Some(rig) = rig("encoder") else { return };
    let clip = gen_clip(&rig.media, "a.mp4", 10);
    let sched = schedule(8, 8, |_| clip.clone());
    rig.start(&sched);
    let before = rig.live_after(None, Duration::from_secs(30)).await;

    // Several kills: the encoder for each upcoming item dies as it starts.
    for _ in 0..3 {
        kill_matching(&rig.media);
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    let after = rig.live_after(Some(&before), Duration::from_secs(40)).await;
    rig.live_after(Some(&after), Duration::from_secs(15)).await;
    assert!(
        sched.reported.lock().unwrap().is_empty(),
        "reported {:?}",
        sched.reported.lock().unwrap()
    );
    rig.finish();
}

/// Files deleted from under a running channel, a corrupt file, and a drive
/// that is unplugged and plugged back in: each bad item is reported and
/// slated, the stream never stops, and good items play again afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn deleted_corrupt_and_unmounted_files_never_stop_the_stream() {
    let Some(rig) = rig("files") else { return };
    // Items 0-3 and 14+ use real clips on the "drive" (a folder we can move);
    // item 5 is a corrupt file; item 6 was deleted before its turn.
    let drive = rig.root.join("drive");
    std::fs::create_dir_all(&drive).unwrap();
    let good = gen_clip(&drive, "good.mp4", 10);
    let good_name = good.file_name().unwrap().to_owned();
    let corrupt = rig.media.join("corrupt.mp4");
    std::fs::write(&corrupt, b"this is not a video file at all").unwrap();
    let doomed = gen_clip(&rig.media, "doomed.mp4", 10);

    let drive_for = drive.clone();
    let corrupt_for = corrupt.clone();
    let doomed_for = doomed.clone();
    let sched = schedule(20, 4, move |n| match n {
        5 => corrupt_for.clone(),
        6 => doomed_for.clone(),
        _ => drive_for.join(&good_name),
    });
    rig.start(&sched);
    rig.live_after(None, Duration::from_secs(30)).await;

    std::fs::remove_file(&doomed).unwrap();
    // Unplug the drive for a while: items 8..=11 (about 32-48 s in) are encoded
    // during the outage window.
    let unplugged = rig.root.join("drive-unplugged");
    tokio::time::sleep(Duration::from_secs(8)).await;
    // Watch the playlist for the rest of the test: it must never disappear
    // (a restart takes the playlist away, which is a dark screen for viewers).
    let dark = Arc::new(Mutex::new(0u32));
    let watcher = {
        let (dir, dark) = (rig.manager.hls_dir("t").join("index.m3u8"), dark.clone());
        tokio::spawn(async move {
            loop {
                if !std::fs::read_to_string(&dir).is_ok_and(|t| t.contains("#EXTINF")) {
                    *dark.lock().unwrap() += 1;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
    };
    std::fs::rename(&drive, &unplugged).unwrap();
    tokio::time::sleep(Duration::from_secs(12)).await;
    std::fs::rename(&unplugged, &drive).unwrap();

    // Let the rest of the schedule run out.
    let last_end = sched.items.last().unwrap().end_ms;
    while now_ms() < last_end - 4000 {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    // The stream is still live after everything.
    let newest = rig.live_after(None, Duration::from_secs(5)).await;
    assert!(rig.manager.is_running("t"));

    let reported = sched.reported.lock().unwrap().clone();

    assert!(
        reported.contains(&5),
        "corrupt file not reported: {reported:?}"
    );
    assert!(
        reported.contains(&6),
        "deleted file not reported: {reported:?}"
    );
    assert!(
        reported.iter().any(|n| (7..=16).contains(n)),
        "nothing was reported during the outage: {reported:?}"
    );
    // After the drive is back, good items are not reported again.
    assert!(
        !reported.iter().any(|n| *n >= 17),
        "items after the drive returned were still unplayable: {reported:?}"
    );
    let _ = newest;
    watcher.abort();
    assert_eq!(
        *dark.lock().unwrap(),
        0,
        "the playlist disappeared while files were missing: the channel restarted"
    );
    rig.finish();
}

/// Nothing at all in the schedule: the channel holds the air with the slate
/// rather than going dark, and picks the schedule up when it appears.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_schedule_is_a_slate_not_a_dead_channel() {
    let Some(rig) = rig("empty") else { return };
    let sched = Arc::new(Schedule {
        items: Vec::new(),
        reported: Mutex::new(Vec::new()),
    });
    rig.start(&sched);
    let first = rig.live_after(None, Duration::from_secs(30)).await;
    rig.live_after(Some(&first), Duration::from_secs(20)).await;
    assert!(rig.manager.is_running("t"));
    rig.finish();
}
