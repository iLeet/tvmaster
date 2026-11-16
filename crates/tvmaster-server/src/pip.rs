//! Lets a guide channel show another channel live in a window: the guide asks
//! for a channel, this starts it (like a viewer would) and says where its
//! playlist is.

use crate::AppState;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tvmaster_core::ChannelKind;
use tvmaster_playout::PipHost;

/// How long to wait for a channel that has to start from nothing.
const WAIT: Duration = Duration::from_secs(14);

pub(crate) struct LocalChannels(pub AppState);

impl PipHost for LocalChannels {
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
