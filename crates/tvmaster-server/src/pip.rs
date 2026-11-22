//! Lets a guide channel show another channel live in a window: the guide asks
//! for a channel, this starts it (like a viewer would) and says where its
//! playlist is.

use crate::AppState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tvmaster_core::ChannelKind;
use tvmaster_playout::{ChannelSpec, PipHost, PlayItem, PlayKind, ScheduleSource};

/// How long to wait for a channel that has to start from nothing.
const WAIT: Duration = Duration::from_secs(14);

pub(crate) struct LocalChannels(pub AppState);

impl PipHost for LocalChannels {
    fn open_for(&self, channel: &str, libraries: &[i64]) -> Option<PathBuf> {
        if channel == tvmaster_core::COMMERCIAL_CHANNEL {
            return self.open_commercials(libraries);
        }
        self.open(channel)
    }

    fn open(&self, channel: &str) -> Option<PathBuf> {
        let s = &self.0;
        let playout = s.playout.clone()?;
        {
            let db = s.db();
            let ch = tvmaster_db::get_channel(&db, channel).ok()??;
            // A guide cannot sit inside a guide.
            if ch.kind == ChannelKind::Guide {
                return None;
            }
        }
        crate::stream::ensure_on_air(s, channel, true).ok()?;
        let playlist = playout.hls_dir(channel).join("index.m3u8");
        let started = Instant::now();
        loop {
            let ready =
                std::fs::read_to_string(&playlist).is_ok_and(|t| t.matches("#EXTINF").count() >= 2);
            if ready {
                return Some(playlist);
            }
            if started.elapsed() > WAIT {
                return None;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

// -------------------------------------------------------------- the commercials window

/// What a commercials window plays: the station's commercials in a fixed shuffled order, round
/// and round, as if they were a channel's schedule. There is no channel behind it: nothing is
/// stored, it has no number and it is listed nowhere. It is only the picture in the guide's window.
struct AdLoop {
    /// (media id, path, length in ms, has audio)
    clips: Vec<(i64, PathBuf, u64, bool)>,
    total_ms: u64,
    epoch_ms: i64,
}

impl AdLoop {
    fn new(mut clips: Vec<(i64, PathBuf, u64, bool)>, epoch_ms: i64) -> Self {
        // A fixed shuffle (the same each time for the same clips), so the loop is stable.
        let mut seed = tvmaster_core::fnv1a(
            &clips
                .iter()
                .map(|c| c.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        for i in (1..clips.len()).rev() {
            // xorshift: any fixed shuffle will do
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let j = (seed % (i as u64 + 1)) as usize;
            clips.swap(i, j);
        }
        let total_ms = clips.iter().map(|c| c.2).sum();
        Self {
            clips,
            total_ms,
            epoch_ms,
        }
    }
}

impl ScheduleSource for AdLoop {
    fn item_at_or_after(&self, _channel: &str, at_ms: i64) -> Option<PlayItem> {
        if self.total_ms == 0 {
            return None;
        }
        let t = (at_ms - self.epoch_ms).rem_euclid(self.total_ms as i64) as u64;
        let mut cum = 0u64;
        for (id, path, dur, audio) in &self.clips {
            if t < cum + dur {
                let start = at_ms - (t - cum) as i64;
                return Some(PlayItem {
                    kind: PlayKind::Media,
                    media_id: Some(*id),
                    path: Some(path.clone()),
                    has_audio: *audio,
                    in_ms: 0,
                    subject: None,
                    start_ms: start,
                    end_ms: start + *dur as i64,
                    is_show: false,
                    show: None,
                    subtitle: None,
                    tracks: None,
                });
            }
            cum += dur;
        }
        None
    }

    fn report_unplayable(&self, _channel: &str, _media_id: i64) {}
}

/// Which clips the commercials stream is playing now (so a change in the libraries restarts it).
fn running_clips() -> &'static Mutex<Vec<i64>> {
    static K: OnceLock<Mutex<Vec<i64>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(Vec::new()))
}

impl LocalChannels {
    /// The window's commercials: start the light internal stream if it is not running and return
    /// its playlist once it has video.
    fn open_commercials(&self, libraries: &[i64]) -> Option<PathBuf> {
        let s = &self.0;
        let playout = s.playout.clone()?;
        let id = tvmaster_core::COMMERCIAL_CHANNEL;
        let clips: Vec<(i64, PathBuf, u64, bool)> = {
            let db = s.db();
            let libs: Vec<i64> = if libraries.is_empty() {
                tvmaster_db::list_libraries(&db)
                    .ok()?
                    .into_iter()
                    .filter(|l| l.kind == "commercials" && l.enabled)
                    .map(|l| l.id)
                    .collect()
            } else {
                libraries.to_vec()
            };
            tvmaster_db::list_media_of_kind(&db, &libs, "commercials")
                .ok()?
                .into_iter()
                .filter(|m| m.duration_ms >= 1000)
                .map(|m| {
                    (
                        m.id,
                        PathBuf::from(m.path),
                        m.duration_ms as u64,
                        m.acodec.is_some(),
                    )
                })
                .collect()
        };
        if clips.is_empty() {
            return None;
        }
        let key: Vec<i64> = clips.iter().map(|c| c.0).collect();
        {
            let mut running = running_clips().lock().unwrap_or_else(|p| p.into_inner());
            if *running != key {
                // other commercials: start the stream over with them
                playout.stop(id);
                *running = key;
            }
        }
        if !playout.is_running(id) {
            let mut options = tvmaster_core::ChannelOptions::default();
            // Small and light: it is only seen in a window the size of a postage stamp.
            options.video.quality = tvmaster_core::Quality::P480;
            options.video.fps = Some(15);
            options.video.kbps = Some(450);
            let spec = ChannelSpec {
                dayparts: Default::default(),
                info: None,
                guide: None,
                id: id.to_string(),
                name: "Commercials".to_string(),
                number: 0,
                options,
                can_burn_text: false,
                signoff: Default::default(),
                always_on: false,
                source: Arc::new(AdLoop::new(clips, crate::plan::now_ms())),
            };
            playout.start_if_absent(spec);
        }
        playout.touch(id);
        let playlist = playout.hls_dir(id).join("index.m3u8");
        let started = Instant::now();
        loop {
            let ready =
                std::fs::read_to_string(&playlist).is_ok_and(|t| t.matches("#EXTINF").count() >= 2);
            if ready {
                return Some(playlist);
            }
            if started.elapsed() > WAIT {
                return None;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}
