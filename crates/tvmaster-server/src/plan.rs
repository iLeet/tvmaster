//! Reading the materialized schedule: guide programmes, now/next, and the
//! source playout follows.

use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tvmaster_core::ItemKind;
use tvmaster_db::MediaRow;
use tvmaster_playout::{PlayItem, ScheduleSource};

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A programme as shown to clients (guide, now/next, schedule).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Program {
    pub media_id: i64,
    pub show: Option<String>,
    pub title: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// Slot start.
    pub start_ms: i64,
    /// Slot end: includes the filler that pads the show to the grid, as a
    /// printed guide would show it.
    pub end_ms: i64,
    /// When the show itself ends.
    pub runtime_end_ms: i64,
    /// Block or event this slot belongs to.
    pub label: Option<String>,
}

/// Slack when reading items, so a slot that straddles the window edge still
/// gets its show and its padding.
const SLOT_MARGIN_MS: i64 = 2 * 3_600_000;

/// Guide programmes overlapping `[from_ms, to_ms)`.
pub fn programs(
    conn: &Connection,
    channel_id: &str,
    from_ms: i64,
    to_ms: i64,
) -> rusqlite::Result<Vec<Program>> {
    let items = tvmaster_db::schedule_between(
        conn,
        channel_id,
        from_ms - SLOT_MARGIN_MS,
        to_ms + SLOT_MARGIN_MS,
    )?;
    let mut media: HashMap<i64, Option<MediaRow>> = HashMap::new();
    let mut out: Vec<Program> = Vec::new();
    for it in items {
        match (it.kind, it.media_id) {
            (ItemKind::Show, Some(id)) => {
                let m = match media.get(&id) {
                    Some(m) => m.clone(),
                    None => {
                        let m = tvmaster_db::get_media(conn, id)?;
                        media.insert(id, m.clone());
                        m
                    }
                };
                let title = m
                    .as_ref()
                    .and_then(|m| m.title.clone())
                    .filter(|t| !t.is_empty())
                    .or_else(|| m.as_ref().and_then(|m| m.show.clone()))
                    .unwrap_or_else(|| "Untitled".to_string());
                out.push(Program {
                    media_id: id,
                    show: m.as_ref().and_then(|m| m.show.clone()),
                    title,
                    season: m.as_ref().and_then(|m| m.season),
                    episode: m.as_ref().and_then(|m| m.episode),
                    start_ms: it.start_ms,
                    end_ms: it.end_ms,
                    runtime_end_ms: it.end_ms,
                    label: it.label,
                });
            }
            _ => {
                // Filler pads the show before it out to the end of its slot.
                if let Some(last) = out.last_mut() {
                    if last.end_ms == it.start_ms {
                        last.end_ms = it.end_ms;
                    }
                }
            }
        }
    }
    out.retain(|p| p.end_ms > from_ms && p.start_ms < to_ms);
    Ok(out)
}

/// The programme on air at `now_ms` and the one after it.
pub fn now_and_next(progs: &[Program], now_ms: i64) -> (Option<Program>, Option<Program>) {
    let now = progs
        .iter()
        .find(|p| p.start_ms <= now_ms && now_ms < p.end_ms)
        .cloned();
    let next = progs.iter().find(|p| p.start_ms > now_ms).cloned();
    (now, next)
}

/// The schedule playout follows: reads from the database, and queues
/// re-rolls for channels whose files turn out to be unplayable.
pub struct DbSchedule {
    pub(crate) db: Arc<Mutex<Connection>>,
    pub(crate) dirty: Arc<Mutex<HashSet<String>>>,
    pub(crate) wake: Arc<Notify>,
}

impl ScheduleSource for DbSchedule {
    fn item_at_or_after(&self, channel: &str, at_ms: i64) -> Option<PlayItem> {
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        let row = tvmaster_db::play_row_at_or_after(&db, channel, at_ms)
            .map_err(|e| tracing::error!(channel, "schedule lookup: {e}"))
            .ok()??;
        Some(PlayItem {
            media_id: row.item.media_id,
            path: row.path.map(PathBuf::from),
            has_audio: row.has_audio,
            start_ms: row.item.start_ms,
            end_ms: row.item.end_ms,
        })
    }

    fn report_unplayable(&self, channel: &str, media_id: i64) {
        {
            let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
            if let Err(e) = tvmaster_db::mark_media_missing(&db, media_id) {
                tracing::error!(media_id, "marking media missing: {e}");
            }
        }
        tracing::warn!(
            channel,
            media_id,
            "file flagged missing; re-rolling the schedule"
        );
        self.dirty
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(channel.to_string());
        self.wake.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(start: i64, end: i64) -> Program {
        Program {
            media_id: 1,
            show: None,
            title: "t".into(),
            season: None,
            episode: None,
            start_ms: start,
            end_ms: end,
            runtime_end_ms: end,
            label: None,
        }
    }

    #[test]
    fn now_and_next_picks_current_and_following() {
        let progs = vec![p(0, 10), p(10, 20), p(30, 40)];
        let (now, next) = now_and_next(&progs, 12);
        assert_eq!(now.unwrap().start_ms, 10);
        assert_eq!(next.unwrap().start_ms, 30);
        // in a hole between programmes there is no "now"
        let (now, next) = now_and_next(&progs, 25);
        assert!(now.is_none());
        assert_eq!(next.unwrap().start_ms, 30);
    }
}
