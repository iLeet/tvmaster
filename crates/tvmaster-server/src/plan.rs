//! Reading the materialized schedule: guide programmes, now/next, and the
//! source playout follows.

use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tvmaster_core::{Channel, ChannelKind, ItemKind, Zone};
use tvmaster_db::MediaRow;
use tvmaster_playout::{GuideData, PlayItem, PlayKind, ScheduleSource, Upcoming};
use tvmaster_render::{GuideCell, GuideRow};

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
            // Off-air time (a finished show, an empty slot, sign-off) is not part of the show before it.
            (_, _, _) if is_off_air(&it) => {}
            // Filler belongs to the show only when it carries the same slot label (the padding does; a gap does not).
            (is_show, _, Some(last))
                if last.end_ms == it.start_ms
                    && (it.kind != ItemKind::Filler || it.label == last.label) =>
            {
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

fn is_off_air(it: &tvmaster_core::ScheduledItem) -> bool {
    it.kind == ItemKind::Filler
        && matches!(
            it.label.as_deref(),
            Some(tvmaster_core::SIGNOFF_LABEL) | Some(tvmaster_core::OFF_AIR_LABEL)
        )
}

/// Stretches of `[from, to)` that no programme covers, as "Off air" cells.
fn with_off_air_gaps(
    cells: Vec<GuideCell>,
    from: i64,
    to: i64,
    zone: &Zone,
    hour12: bool,
) -> Vec<GuideCell> {
    let mut out = Vec::with_capacity(cells.len() + 2);
    let mut at = from;
    let gap = |out: &mut Vec<GuideCell>, a: i64, b: i64| {
        if b - a >= 60_000 {
            out.push(GuideCell {
                start_ms: a,
                end_ms: b,
                range: range_label(zone, a, b, hour12),
                title: "Off air".to_string(),
            });
        }
    };
    for c in cells {
        if c.start_ms > at {
            gap(&mut out, at, c.start_ms.min(to));
        }
        at = at.max(c.end_ms);
        out.push(c);
    }
    if at < to {
        gap(&mut out, at, to);
    }
    out
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
    pub(crate) live: Arc<Mutex<crate::live::Book>>,
}

/// "8:30 PM", or "20:30" on a 24-hour clock.
fn time_label(zone: &Zone, ms: i64) -> String {
    clock_label(zone, ms, true)
}

fn clock_label(zone: &Zone, ms: i64, hour12: bool) -> String {
    let t = zone.local(ms);
    let (h, m) = (chrono::Timelike::hour(&t), chrono::Timelike::minute(&t));
    if !hour12 {
        return format!("{h:02}:{m:02}");
    }
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {ampm}")
}

/// "9:00-9:30 PM" (the meridiem is written once when both ends share it).
fn range_label(zone: &Zone, start: i64, end: i64, hour12: bool) -> String {
    let (a, b) = (
        clock_label(zone, start, hour12),
        clock_label(zone, end, hour12),
    );
    if !hour12 {
        return format!("{a}-{b}");
    }
    match (a.rsplit_once(' '), b.rsplit_once(' ')) {
        (Some((ta, ma)), Some((_, mb))) if ma == mb => format!("{ta}-{b}"),
        _ => format!("{a}-{b}"),
    }
}

/// The listing a guide channel draws: its channels (every programming
/// channel unless the guide names some), from the current time step, plus a
/// ticker with what is on now.
pub fn build_guide(db: &Connection, guide_id: &str, now_ms: i64) -> Option<GuideData> {
    let guide = tvmaster_db::get_channel(db, guide_id).ok().flatten()?;
    build_guide_with(
        db,
        &guide.name,
        &guide.lineup.timezone,
        &guide.guide,
        now_ms,
    )
}

/// The same, for settings that are not saved (a preview while editing).
pub fn build_guide_with(
    db: &Connection,
    guide_name: &str,
    guide_timezone: &str,
    cfg: &tvmaster_core::GuideConfig,
    now_ms: i64,
) -> Option<GuideData> {
    let step = i64::from(cfg.layout.time_step) * 60_000;
    let hour12 = cfg.header.hour12;
    let channels = tvmaster_db::list_channels(db).ok()?;
    let zone = Zone::parse(
        &channels
            .iter()
            .find(|c| c.kind == ChannelKind::Programming)
            .map(|c| c.lineup.timezone.clone())
            .unwrap_or_default(),
    )
    .or_else(|| Zone::parse(guide_timezone))
    .unwrap_or_else(Zone::utc);
    let start = zone.floor_grid(now_ms, step.min(30 * 60_000));
    let window_ms = cfg.hours as i64 * 3_600_000;

    // The channels to list: the ones the guide names, in its order, or all by number.
    let programming = || {
        channels
            .iter()
            .filter(|c| c.kind == ChannelKind::Programming)
    };
    let listed: Vec<&Channel> = if cfg.channels.is_empty() {
        programming().collect()
    } else {
        cfg.channels
            .iter()
            .filter_map(|id| programming().find(|c| &c.id == id))
            .collect()
    };

    let mut rows = Vec::new();
    let mut ticker: Vec<String> = cfg.ticker.clone();
    for ch in listed {
        let progs = programs(db, &ch.id, start, start + window_ms).unwrap_or_default();
        if cfg.ticker_options.now_on {
            if let (Some(now), _) = now_and_next(&progs, now_ms) {
                ticker.push(format!(
                    "NOW ON {}: {}",
                    ch.number,
                    now.show.clone().unwrap_or(now.title.clone())
                ));
            }
        }
        rows.push(GuideRow {
            id: ch.id.clone(),
            number: ch.number,
            name: ch.name.clone(),
            cells: with_off_air_gaps(
                progs
                    .into_iter()
                    .map(|p| GuideCell {
                        start_ms: p.start_ms,
                        end_ms: p.end_ms,
                        range: range_label(&zone, p.start_ms, p.end_ms, hour12),
                        title: p.show.unwrap_or(p.title),
                    })
                    .collect(),
                start,
                start + window_ms,
                &zone,
                hour12,
            ),
        });
    }
    let title = cfg
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("{guide_name} GUIDE"));
    let local = zone.local(now_ms);
    Some(GuideData {
        title,
        now_ms,
        window_start_ms: start,
        window_ms,
        clock: clock_label(&zone, now_ms, hour12),
        date: local.format("%a %b %-d").to_string().to_uppercase(),
        marks: (0..window_ms / step)
            .map(|i| {
                let at = start + i * step;
                (at, clock_label(&zone, at, hour12))
            })
            .collect(),
        rows,
        ticker: ticker.join("   *   "),
    })
}

/// "S01E02 - Title" for the on-screen programme card.
fn episode_label(season: Option<i64>, episode: Option<i64>, title: Option<&str>) -> Option<String> {
    let code = match (season, episode) {
        (Some(s), Some(e)) => Some(format!("S{s:02}E{e:02}")),
        (None, Some(e)) => Some(format!("E{e:02}")),
        _ => None,
    };
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    match (code, title) {
        (Some(c), Some(t)) if !t.eq_ignore_ascii_case(&c) => Some(format!("{c} - {t}")),
        (Some(c), _) => Some(c),
        (None, Some(t)) => Some(t.to_string()),
        _ => None,
    }
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
            (ItemKind::Filler, _) if it.label.as_deref() == Some(tvmaster_core::SIGNOFF_LABEL) => {
                PlayKind::SignOff
            }
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
            is_show: it.kind == ItemKind::Show,
            show: row.show_name.clone(),
            subtitle: episode_label(row.season, row.episode, row.title.as_deref()),
            tracks: row
                .tracks_json
                .as_deref()
                .and_then(tvmaster_core::Tracks::from_json),
        })
    }

    fn live_override(&self, channel: &str) -> Option<tvmaster_playout::LiveOverride> {
        let book = self.live.lock().unwrap_or_else(|p| p.into_inner());
        crate::live::current(&book, channel, now_ms())
    }

    fn show_defaults(&self, _channel: &str, show: &str) -> Option<tvmaster_core::ShowOptions> {
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        tvmaster_db::get_show_settings(&db, show).ok().flatten()
    }

    fn guide_data(&self, channel: &str, now_ms: i64) -> Option<GuideData> {
        let db = self.db.lock().unwrap_or_else(|p| p.into_inner());
        build_guide(&db, channel, now_ms)
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
    fn off_air_time_after_a_show_is_not_part_of_the_show_in_the_guide() {
        use tvmaster_core::{ScheduledItem, OFF_AIR_LABEL};
        let conn = tvmaster_db::open_in_memory().unwrap();
        let lib = tvmaster_db::add_library(&conn, "/x", "shows").unwrap();
        let media = tvmaster_db::upsert_media(
            &conn,
            &tvmaster_db::NewMedia {
                library_id: lib.id,
                path: "/x/a.mkv".into(),
                show: Some("Mickey".into()),
                season: Some(1),
                episode: Some(1),
                title: Some("One".into()),
                duration_ms: 1_380_000,
                width: None,
                height: None,
                vcodec: None,
                acodec: None,
                year: None,
                breaks_json: None,
                tracks_json: None,
            },
        )
        .unwrap();
        let item = |kind, start, end, label: Option<&str>| ScheduledItem {
            kind,
            media_id: (kind == ItemKind::Show).then_some(media),
            start_ms: start,
            end_ms: end,
            cursor_key: (kind == ItemKind::Show).then(|| "show:mickey".to_string()),
            cursor_pos: (kind == ItemKind::Show).then_some(0),
            label: label.map(str::to_string),
            in_ms: 0,
            bumper: None,
        };
        let h = 3_600_000;
        tvmaster_db::upsert_channel(
            &conn,
            &Channel {
                id: "c".into(),
                number: 1,
                name: "C".into(),
                strategy: tvmaster_core::Strategy::BlockGrid,
                run_out: tvmaster_core::RunOutPolicy::OffAir,
                autopilot: true,
                chaos: 0,
                seed: 1,
                epoch_ms: 0,
                always_on: false,
                library_ids: vec![lib.id],
                lineup: Default::default(),
                kind: Default::default(),
                group: None,
                guide: Default::default(),
                options: Default::default(),
            },
        )
        .unwrap();
        tvmaster_db::insert_schedule(
            &conn,
            "c",
            &[
                item(ItemKind::Show, 0, 23 * 60_000, Some("Mickey")),
                // padding to the half hour stays with the show...
                item(ItemKind::Filler, 23 * 60_000, 30 * 60_000, Some("Mickey")),
                // ...but the time after it, spent on the off-air screen, does not
                item(
                    ItemKind::Filler,
                    30 * 60_000,
                    45 * 60_000,
                    Some(OFF_AIR_LABEL),
                ),
                // gaps planned by an earlier version carried no label at all
                item(ItemKind::Filler, 45 * 60_000, h, None),
            ],
        )
        .unwrap();
        let progs = programs(&conn, "c", 0, h).unwrap();
        assert_eq!(progs.len(), 1);
        assert_eq!(
            progs[0].end_ms,
            30 * 60_000,
            "the show's slot ends where its padding does"
        );
        let cells = with_off_air_gaps(
            vec![GuideCell {
                start_ms: 0,
                end_ms: 30 * 60_000,
                range: String::new(),
                title: "Mickey".into(),
            }],
            0,
            h,
            &Zone::utc(),
            true,
        );
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[1].title, "Off air");
        assert_eq!((cells[1].start_ms, cells[1].end_ms), (30 * 60_000, h));
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
