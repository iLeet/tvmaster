//! Reading the materialized schedule: guide programmes, now/next, and the
//! source playout follows.

use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tvmaster_core::{ItemKind, Zone};
use tvmaster_db::MediaRow;
use tvmaster_playout::{PlayItem, PlayKind, ScheduleSource, Upcoming};

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
        let is_show = it.kind == ItemKind::Show && it.media_id.is_some();
        match (is_show, it.cursor_pos.is_some(), out.last_mut()) {
            // A new slot begins with the first act of a show.
            (true, true, _) | (true, false, None) => {
                let id = it.media_id.expect("show item has media");
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
            // Later acts, commercials, bumpers and filler belong to the slot
            // before them, as a printed guide would show it.
            (is_show, _, Some(last)) if last.end_ms == it.start_ms => {
                last.end_ms = it.end_ms;
                if is_show {
                    last.runtime_end_ms = it.end_ms;
                }
            }
            _ => {}
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

/// "8:30 PM"
fn time_label(zone: &Zone, ms: i64) -> String {
    let t = zone.local(ms);
    let (h, m) = (chrono::Timelike::hour(&t), chrono::Timelike::minute(&t));
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {ampm}")
}

impl DbSchedule {
    fn zone_for(&self, db: &Connection, channel: &str) -> Zone {
        tvmaster_db::get_channel(db, channel)
            .ok()
            .flatten()
            .and_then(|c| Zone::parse(&c.lineup.timezone))
            .unwrap_or_else(Zone::utc)
    }
}

impl ScheduleSource for DbSchedule {
    fn item_at_or_after(&self, channel: &str, at_ms: i64) -> Option<PlayItem> {
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        let row = tvmaster_db::play_row_at_or_after(&db, channel, at_ms)
            .map_err(|e| tracing::error!(channel, "schedule lookup: {e}"))
            .ok()??;
        let it = &row.item;
        let kind = match (it.kind, it.bumper) {
            (ItemKind::Filler, _) => PlayKind::Slate,
            (ItemKind::Bumper, Some(b)) => PlayKind::Bumper(b),
            _ => PlayKind::Media,
        };
        Some(PlayItem {
            kind,
            media_id: it.media_id,
            path: row.path.map(PathBuf::from),
            has_audio: row.has_audio,
            in_ms: it.in_ms,
            subject: row.show,
            start_ms: it.start_ms,
            end_ms: it.end_ms,
        })
    }

    fn upcoming(&self, channel: &str, after_ms: i64, limit: usize) -> Upcoming {
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        let zone = self.zone_for(&db, channel);
        // A programme that starts a moment before the bumper ends still counts.
        let from = after_ms - 2_000;
        let progs = programs(&db, channel, from, from + 12 * 3_600_000).unwrap_or_default();
        let items: Vec<(String, String)> = progs
            .into_iter()
            .filter(|p| p.start_ms >= from)
            .take(limit)
            .map(|p| {
                let name = p.show.unwrap_or(p.title);
                (time_label(&zone, p.start_ms), name)
            })
            .collect();
        let hour = chrono::Timelike::hour(&zone.local(after_ms));
        Upcoming {
            items,
            evening: (17..24).contains(&hour),
        }
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
    fn time_labels_are_12_hour_in_the_channel_zone() {
        let z = Zone::parse("America/Toronto").unwrap();
        let at = |s: &str| {
            z.to_utc_ms(chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
        };
        assert_eq!(time_label(&z, at("2026-11-16 20:30")), "8:30 PM");
        assert_eq!(time_label(&z, at("2026-11-16 00:05")), "12:05 AM");
        assert_eq!(time_label(&z, at("2026-11-16 12:00")), "12:00 PM");
        assert_eq!(time_label(&z, at("2026-11-16 09:00")), "9:00 AM");
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
