//! Typed queries over the schema.

use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use serde::Serialize;
use tvmaster_core::{
    BumperKind, Channel, ChannelKind, Cursors, EpisodeCursor, ItemKind, RunOutPolicy,
    ScheduledItem, Strategy, Tv,
};

// ---------- libraries ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Library {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub enabled: bool,
}

fn library_from(r: &Row) -> Result<Library> {
    Ok(Library {
        id: r.get(0)?,
        path: r.get(1)?,
        kind: r.get(2)?,
        enabled: r.get::<_, i64>(3)? != 0,
    })
}

pub fn add_library(conn: &Connection, path: &str, kind: &str) -> Result<Library> {
    conn.execute(
        "INSERT OR IGNORE INTO libraries (path, kind) VALUES (?1, ?2)",
        params![path, kind],
    )?;
    conn.query_row(
        "SELECT id, path, kind, enabled FROM libraries WHERE path = ?1",
        [path],
        library_from,
    )
}

pub fn list_libraries(conn: &Connection) -> Result<Vec<Library>> {
    let mut st = conn.prepare("SELECT id, path, kind, enabled FROM libraries ORDER BY id")?;
    let rows = st.query_map([], library_from)?;
    rows.collect()
}

pub fn get_library(conn: &Connection, id: i64) -> Result<Option<Library>> {
    conn.query_row(
        "SELECT id, path, kind, enabled FROM libraries WHERE id = ?1",
        [id],
        library_from,
    )
    .optional()
}

// ---------- media ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaRow {
    pub id: i64,
    pub library_id: i64,
    pub path: String,
    pub show: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub title: Option<String>,
    pub duration_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub year: Option<i64>,
    /// JSON from `tvmaster_media::BreakInfo`, once analysed.
    pub breaks_json: Option<String>,
    /// JSON `tvmaster_core::Tracks`: its audio and subtitle streams (None until scanned by a version that reads them).
    pub tracks_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NewMedia {
    pub library_id: i64,
    pub path: String,
    pub show: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub title: Option<String>,
    pub duration_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub year: Option<i64>,
    /// Break points found while scanning (chapters). Later analysis may add more.
    pub breaks_json: Option<String>,
    pub tracks_json: Option<String>,
}

const MEDIA_COLS: &str = "id, library_id, path, show, season, episode, title, duration_ms, width, height, vcodec, acodec, year, breaks_json, tracks_json";

fn media_from(r: &Row) -> Result<MediaRow> {
    Ok(MediaRow {
        id: r.get(0)?,
        library_id: r.get(1)?,
        path: r.get(2)?,
        show: r.get(3)?,
        season: r.get(4)?,
        episode: r.get(5)?,
        title: r.get(6)?,
        duration_ms: r.get(7)?,
        width: r.get(8)?,
        height: r.get(9)?,
        vcodec: r.get(10)?,
        acodec: r.get(11)?,
        year: r.get(12)?,
        breaks_json: r.get(13)?,
        tracks_json: r.get(14)?,
    })
}

/// Flag every item in a library as missing; `upsert_media` clears the flag for
/// files that are still present, so a scan ends with only truly missing files
/// flagged.
pub fn mark_library_missing(conn: &Connection, library_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET missing = 1 WHERE library_id = ?1",
        [library_id],
    )?;
    Ok(())
}

pub fn upsert_media(conn: &Connection, m: &NewMedia) -> Result<i64> {
    conn.query_row(
        "INSERT INTO media_items
            (library_id, path, show, season, episode, title, duration_ms, width, height, vcodec, acodec, year, breaks_json, tracks_json, missing)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0)
         ON CONFLICT(path) DO UPDATE SET
            library_id = excluded.library_id, show = excluded.show, season = excluded.season,
            episode = excluded.episode, title = excluded.title,
            width = excluded.width, height = excluded.height, vcodec = excluded.vcodec,
            acodec = excluded.acodec, year = excluded.year, missing = 0,
            tracks_json = COALESCE(excluded.tracks_json, tracks_json),
            -- keep earlier analysis unless the file changed
            breaks_json = CASE WHEN excluded.duration_ms != duration_ms THEN excluded.breaks_json
                               ELSE COALESCE(excluded.breaks_json, breaks_json) END,
            duration_ms = excluded.duration_ms
         RETURNING id",
        params![
            m.library_id,
            m.path,
            m.show,
            m.season,
            m.episode,
            m.title,
            m.duration_ms,
            m.width,
            m.height,
            m.vcodec,
            m.acodec,
            m.year,
            m.breaks_json,
            m.tracks_json
        ],
        |r| r.get(0),
    )
}

/// Present media from the enabled libraries of `kind` among `library_ids`.
pub fn list_media_of_kind(
    conn: &Connection,
    library_ids: &[i64],
    kind: &str,
) -> Result<Vec<MediaRow>> {
    let mut out = Vec::new();
    let sql = format!(
        "SELECT {MEDIA_COLS} FROM media_items
         WHERE library_id = ?1 AND missing = 0
           AND library_id IN (SELECT id FROM libraries WHERE enabled = 1 AND kind = ?2)
         ORDER BY id"
    );
    let mut st = conn.prepare(&sql)?;
    for lib in library_ids {
        let rows = st.query_map(params![lib, kind], media_from)?;
        for r in rows {
            out.push(r?);
        }
    }
    Ok(out)
}

/// Set the analysed break points for a file.
pub fn set_breaks(conn: &Connection, media_id: i64, breaks_json: &str) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET breaks_json = ?2 WHERE id = ?1",
        params![media_id, breaks_json],
    )?;
    Ok(())
}

/// Present items of a library that have not been analysed for breaks yet.
pub fn media_needing_analysis(conn: &Connection, library_id: i64) -> Result<Vec<MediaRow>> {
    let mut st = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM media_items
         WHERE library_id = ?1 AND missing = 0 AND breaks_json IS NULL ORDER BY id"
    ))?;
    let rows = st.query_map([library_id], media_from)?;
    rows.collect()
}

/// Present (not missing) media from the enabled `shows` libraries among `library_ids`.
pub fn list_media_in_libraries(conn: &Connection, library_ids: &[i64]) -> Result<Vec<MediaRow>> {
    list_media_of_kind(conn, library_ids, "shows")
}

/// A show in the libraries: how many episodes and how much airtime.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ShowSummary {
    pub name: String,
    pub episodes: i64,
    pub total_ms: i64,
    /// Audio languages found in this show's files (canonical three-letter codes).
    pub audio_languages: Vec<String>,
    /// Subtitle languages found, embedded or in files beside the video.
    pub subtitle_languages: Vec<String>,
    /// Files in this show whose tracks have been read (scanned by a version that does).
    pub files_with_tracks: i64,
}

/// The distinct shows in `shows` libraries (all of them if `library_ids` is
/// empty), by name, case-insensitively.
pub fn list_shows(conn: &Connection, library_ids: &[i64]) -> Result<Vec<ShowSummary>> {
    let mut st = conn.prepare(
        "SELECT MIN(COALESCE(m.show, '')), COUNT(*), SUM(m.duration_ms)
         FROM media_items m JOIN libraries l ON l.id = m.library_id
         WHERE m.missing = 0 AND l.enabled = 1 AND l.kind = 'shows'
         GROUP BY LOWER(COALESCE(m.show, ''))
         ORDER BY LOWER(COALESCE(m.show, ''))",
    )?;
    // Filtering by library happens in a second pass so one statement serves both cases.
    let all: Vec<(String, i64, i64)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_>>()?;
    if library_ids.is_empty() {
        return Ok(all
            .into_iter()
            .map(|(name, episodes, total_ms)| ShowSummary {
                name,
                episodes,
                total_ms,
                ..Default::default()
            })
            .collect::<Vec<_>>())
        .map(|mut v| {
            fill_languages(conn, &mut v);
            v
        });
    }
    let mut out = Vec::new();
    let mut by_lib = conn.prepare(
        "SELECT MIN(COALESCE(show, '')), COUNT(*), SUM(duration_ms)
         FROM media_items
         WHERE missing = 0 AND library_id = ?1
         GROUP BY LOWER(COALESCE(show, ''))",
    )?;
    let mut merged: std::collections::BTreeMap<String, ShowSummary> = Default::default();
    for lib in library_ids {
        for row in by_lib.query_map([lib], |r| {
            Ok(ShowSummary {
                name: r.get(0)?,
                episodes: r.get(1)?,
                total_ms: r.get(2)?,
                ..Default::default()
            })
        })? {
            let row = row?;
            merged
                .entry(row.name.to_lowercase())
                .and_modify(|e| {
                    e.episodes += row.episodes;
                    e.total_ms += row.total_ms;
                })
                .or_insert(row);
        }
    }
    out.extend(merged.into_values());
    fill_languages(conn, &mut out);
    Ok(out)
}

/// Add the audio and subtitle languages found in each show's files.
fn fill_languages(conn: &Connection, shows: &mut [ShowSummary]) {
    use std::collections::{BTreeMap, BTreeSet};
    let Ok(mut st) = conn.prepare(
        "SELECT LOWER(COALESCE(show, '')), tracks_json FROM media_items WHERE missing = 0 AND tracks_json IS NOT NULL",
    ) else {
        return;
    };
    let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    else {
        return;
    };
    let mut found: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>, i64)> = BTreeMap::new();
    for (show, json) in rows.flatten() {
        let Some(t) = tvmaster_core::Tracks::from_json(&json) else {
            continue;
        };
        let e = found.entry(show).or_default();
        e.0.extend(
            t.audio
                .iter()
                .map(|a| a.lang.clone())
                .filter(|l| !l.is_empty()),
        );
        e.1.extend(
            t.subs
                .iter()
                .map(|s| s.lang.clone())
                .chain(t.external.iter().map(|s| tvmaster_core::norm_lang(&s.lang)))
                .filter(|l| !l.is_empty()),
        );
        e.2 += 1;
    }
    for s in shows {
        if let Some((a, b, n)) = found.get(&s.name.to_lowercase()) {
            s.audio_languages = a.iter().cloned().collect();
            s.subtitle_languages = b.iter().cloned().collect();
            s.files_with_tracks = *n;
        }
    }
}

/// Delete a library and its files. Schedule items that reference those files
/// go too (they are a plan, not history worth keeping), since the schedule
/// would otherwise point at files that no longer exist.
pub fn delete_library(conn: &Connection, id: i64) -> Result<bool> {
    conn.execute(
        "DELETE FROM schedule_items WHERE media_id IN (SELECT id FROM media_items WHERE library_id = ?1)",
        [id],
    )?;
    Ok(conn.execute("DELETE FROM libraries WHERE id = ?1", [id])? > 0)
}

pub fn set_library_enabled(conn: &Connection, id: i64, enabled: bool) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE libraries SET enabled = ?2 WHERE id = ?1",
        params![id, enabled as i64],
    )? > 0)
}

pub fn delete_tv(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM tvs WHERE id = ?1", [id])? > 0)
}

pub fn get_media(conn: &Connection, id: i64) -> Result<Option<MediaRow>> {
    conn.query_row(
        &format!("SELECT {MEDIA_COLS} FROM media_items WHERE id = ?1"),
        [id],
        media_from,
    )
    .optional()
}

pub fn count_media(conn: &Connection, library_id: i64) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE library_id = ?1 AND missing = 0",
        [library_id],
        |r| r.get(0),
    )
}

// ---------- channels ----------

fn channel_from(r: &Row) -> Result<Channel> {
    let strategy: String = r.get(3)?;
    let run_out: String = r.get(4)?;
    Ok(Channel {
        id: r.get(0)?,
        number: r.get::<_, i64>(1)? as u32,
        name: r.get(2)?,
        strategy: Strategy::parse(&strategy).unwrap_or_default(),
        run_out: RunOutPolicy::parse(&run_out).unwrap_or_default(),
        autopilot: r.get::<_, i64>(5)? != 0,
        chaos: r.get::<_, i64>(6)?.clamp(0, 100) as u8,
        seed: r.get::<_, i64>(7)? as u64,
        epoch_ms: r.get(8)?,
        always_on: r.get::<_, i64>(9)? != 0,
        library_ids: Vec::new(),
        lineup: r
            .get::<_, Option<String>>(10)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        kind: ChannelKind::parse(&r.get::<_, String>(11)?).unwrap_or_default(),
        group: r.get(12)?,
        guide: r
            .get::<_, Option<String>>(13)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        options: r
            .get::<_, Option<String>>(14)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
    })
}

const CHANNEL_COLS: &str =
    "id, number, name, strategy, run_out, autopilot, chaos, seed, epoch_ms, always_on, lineup_json, kind, grp, guide_json, options_json";

fn channel_library_ids(conn: &Connection, channel_id: &str) -> Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT library_id FROM channel_libraries WHERE channel_id = ?1 ORDER BY library_id",
    )?;
    let rows = st.query_map([channel_id], |r| r.get(0))?;
    rows.collect()
}

pub fn upsert_channel(conn: &Connection, c: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (id, number, name, strategy, run_out, autopilot, chaos, seed, epoch_ms, always_on, lineup_json, kind, grp, guide_json, options_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
            number = excluded.number, name = excluded.name, strategy = excluded.strategy,
            run_out = excluded.run_out, autopilot = excluded.autopilot, chaos = excluded.chaos,
            seed = excluded.seed, epoch_ms = excluded.epoch_ms, always_on = excluded.always_on,
            lineup_json = excluded.lineup_json, kind = excluded.kind, grp = excluded.grp,
            guide_json = excluded.guide_json, options_json = excluded.options_json",
        params![
            c.id,
            c.number as i64,
            c.name,
            c.strategy.as_str(),
            c.run_out.as_str(),
            c.autopilot as i64,
            c.chaos as i64,
            c.seed as i64,
            c.epoch_ms,
            c.always_on as i64,
            serde_json::to_string(&c.lineup).unwrap_or_else(|_| "{}".into()),
            c.kind.as_str(),
            c.group,
            serde_json::to_string(&c.guide).unwrap_or_else(|_| "{}".into()),
            serde_json::to_string(&c.options).unwrap_or_else(|_| "{}".into())
        ],
    )?;
    conn.execute(
        "DELETE FROM channel_libraries WHERE channel_id = ?1",
        [&c.id],
    )?;
    for lib in &c.library_ids {
        conn.execute(
            "INSERT OR IGNORE INTO channel_libraries (channel_id, library_id) VALUES (?1, ?2)",
            params![c.id, lib],
        )?;
    }
    Ok(())
}

pub fn list_channels(conn: &Connection) -> Result<Vec<Channel>> {
    let mut st = conn.prepare(&format!(
        "SELECT {CHANNEL_COLS} FROM channels ORDER BY number"
    ))?;
    let mut chans: Vec<Channel> = st.query_map([], channel_from)?.collect::<Result<_>>()?;
    for c in &mut chans {
        c.library_ids = channel_library_ids(conn, &c.id)?;
    }
    Ok(chans)
}

pub fn get_channel(conn: &Connection, id: &str) -> Result<Option<Channel>> {
    let c = conn
        .query_row(
            &format!("SELECT {CHANNEL_COLS} FROM channels WHERE id = ?1"),
            [id],
            channel_from,
        )
        .optional()?;
    match c {
        Some(mut c) => {
            c.library_ids = channel_library_ids(conn, &c.id)?;
            Ok(Some(c))
        }
        None => Ok(None),
    }
}

pub fn delete_channel(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM channels WHERE id = ?1", [id])? > 0)
}

// ---------- schedule ----------

fn kind_str(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Show => "show",
        ItemKind::Commercial => "commercial",
        ItemKind::Bumper => "bumper",
        ItemKind::Filler => "filler",
    }
}

fn kind_from(s: &str) -> ItemKind {
    match s {
        "show" => ItemKind::Show,
        "commercial" => ItemKind::Commercial,
        "bumper" => ItemKind::Bumper,
        _ => ItemKind::Filler,
    }
}

const SCHED_COLS: &str =
    "kind, media_id, start_ms, end_ms, cursor_key, cursor_pos, label, in_ms, bumper";

fn item_from(r: &Row) -> Result<ScheduledItem> {
    let kind: String = r.get(0)?;
    Ok(ScheduledItem {
        kind: kind_from(&kind),
        media_id: r.get(1)?,
        start_ms: r.get(2)?,
        end_ms: r.get(3)?,
        cursor_key: r.get(4)?,
        cursor_pos: r.get::<_, Option<i64>>(5)?.map(|p| p as u64),
        label: r.get(6)?,
        in_ms: r.get::<_, i64>(7)?.max(0) as u64,
        bumper: r
            .get::<_, Option<String>>(8)?
            .and_then(|b| BumperKind::parse(&b)),
    })
}

pub fn insert_schedule(conn: &Connection, channel_id: &str, items: &[ScheduledItem]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut st = tx.prepare(
            "INSERT INTO schedule_items (channel_id, kind, media_id, start_ms, end_ms, cursor_key, cursor_pos, label, in_ms, bumper)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for it in items {
            st.execute(params![
                channel_id,
                kind_str(it.kind),
                it.media_id,
                it.start_ms,
                it.end_ms,
                it.cursor_key,
                it.cursor_pos.map(|p| p as i64),
                it.label,
                it.in_ms as i64,
                it.bumper.map(|b| b.as_str())
            ])?;
        }
    }
    tx.commit()
}

/// Items overlapping `[from_ms, to_ms)`, in time order.
pub fn schedule_between(
    conn: &Connection,
    channel_id: &str,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<ScheduledItem>> {
    let mut st = conn.prepare(&format!(
        "SELECT {SCHED_COLS} FROM schedule_items
         WHERE channel_id = ?1 AND end_ms > ?2 AND start_ms < ?3 ORDER BY start_ms"
    ))?;
    let rows = st.query_map(params![channel_id, from_ms, to_ms], item_from)?;
    rows.collect()
}

pub fn schedule_last_end(conn: &Connection, channel_id: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT MAX(end_ms) FROM schedule_items WHERE channel_id = ?1",
        [channel_id],
        |r| r.get(0),
    )
}

/// Remove every item that starts at or after `start_ms`.
pub fn delete_schedule_from(conn: &Connection, channel_id: &str, start_ms: i64) -> Result<usize> {
    conn.execute(
        "DELETE FROM schedule_items WHERE channel_id = ?1 AND start_ms >= ?2",
        params![channel_id, start_ms],
    )
}

/// Remove items that ended before `end_ms` (history trimming).
pub fn prune_schedule_before(conn: &Connection, channel_id: &str, end_ms: i64) -> Result<usize> {
    conn.execute(
        "DELETE FROM schedule_items WHERE channel_id = ?1 AND end_ms < ?2",
        params![channel_id, end_ms],
    )
}

/// What playout needs for one schedule item: the item plus its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayRow {
    pub item: ScheduledItem,
    pub path: Option<String>,
    pub has_audio: bool,
    /// The show's name (for bumpers "about" a show).
    pub show: Option<String>,
    /// The file's own show name (None for files without one), for per-show settings.
    pub show_name: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub title: Option<String>,
    /// JSON `tvmaster_core::Tracks`, when the file has been scanned for them.
    pub tracks_json: Option<String>,
}

/// The item on air at `at_ms`, or the next one to start after it.
pub fn play_row_at_or_after(
    conn: &Connection,
    channel_id: &str,
    at_ms: i64,
) -> Result<Option<PlayRow>> {
    conn.query_row(
        "SELECT s.kind, s.media_id, s.start_ms, s.end_ms, s.cursor_key, s.cursor_pos, s.label, s.in_ms, s.bumper,
                m.path, m.acodec, COALESCE(m.show, m.title), m.show, m.season, m.episode, m.title, m.tracks_json
         FROM schedule_items s LEFT JOIN media_items m ON m.id = s.media_id
         WHERE s.channel_id = ?1 AND s.end_ms > ?2 ORDER BY s.start_ms LIMIT 1",
        params![channel_id, at_ms],
        |r| {
            Ok(PlayRow {
                item: item_from(r)?,
                path: r.get(9)?,
                has_audio: r.get::<_, Option<String>>(10)?.is_some(),
                show: r.get(11)?,
                show_name: r.get(12)?,
                season: r.get(13)?,
                episode: r.get(14)?,
                title: r.get(15)?,
                tracks_json: r.get(16)?,
            })
        },
    )
    .optional()
}

/// The commercials most recently scheduled before `before_ms` (oldest first).
pub fn recent_commercials(
    conn: &Connection,
    channel_id: &str,
    before_ms: i64,
    limit: usize,
) -> Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT media_id FROM schedule_items
         WHERE channel_id = ?1 AND kind = 'commercial' AND start_ms < ?2 AND media_id IS NOT NULL
         ORDER BY start_ms DESC LIMIT ?3",
    )?;
    let mut ids: Vec<i64> = st
        .query_map(params![channel_id, before_ms, limit as i64], |r| r.get(0))?
        .collect::<Result<_>>()?;
    ids.reverse();
    Ok(ids)
}

/// Files currently flagged missing, as (id, path), for the enabled libraries.
pub fn list_missing_media(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut st = conn.prepare(
        "SELECT m.id, m.path FROM media_items m JOIN libraries l ON l.id = m.library_id
         WHERE m.missing = 1 AND l.enabled = 1 ORDER BY m.id",
    )?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// The file is back: clear its missing flag.
pub fn mark_media_present(conn: &Connection, media_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET missing = 0 WHERE id = ?1",
        [media_id],
    )?;
    Ok(())
}

pub fn mark_media_missing(conn: &Connection, media_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET missing = 1 WHERE id = ?1",
        [media_id],
    )?;
    Ok(())
}

// ---------- cursors ----------

pub fn load_cursors(conn: &Connection, channel_id: &str) -> Result<Cursors> {
    let mut st =
        conn.prepare("SELECT key, reserved, committed FROM cursors WHERE channel_id = ?1")?;
    let rows = st.query_map([channel_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            EpisodeCursor {
                reserved: r.get::<_, i64>(1)? as u64,
                committed: r.get::<_, i64>(2)? as u64,
            },
        ))
    })?;
    rows.collect()
}

pub fn save_cursors(conn: &Connection, channel_id: &str, cursors: &Cursors) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM cursors WHERE channel_id = ?1", [channel_id])?;
    {
        let mut st = tx.prepare(
            "INSERT INTO cursors (channel_id, key, reserved, committed) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (k, c) in cursors {
            st.execute(params![
                channel_id,
                k,
                c.reserved as i64,
                c.committed as i64
            ])?;
        }
    }
    tx.commit()
}

// ---------- meta ----------

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------- TVs ----------

fn tv_from(r: &Row) -> Result<Tv> {
    Ok(Tv {
        id: r.get(0)?,
        name: r.get(1)?,
        channel: r.get::<_, Option<i64>>(2)?.map(|n| n as u32),
        power: r.get::<_, i64>(3)? != 0,
        volume: r.get::<_, i64>(4)?.clamp(0, 100) as u8,
        muted: r.get::<_, i64>(5)? != 0,
        online: false,
    })
}

pub fn upsert_tv(conn: &Connection, tv: &Tv) -> Result<()> {
    conn.execute(
        "INSERT INTO tvs (id, name, channel, power, volume, muted) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, channel = excluded.channel,
            power = excluded.power, volume = excluded.volume, muted = excluded.muted",
        params![
            tv.id,
            tv.name,
            tv.channel.map(|c| c as i64),
            tv.power as i64,
            tv.volume as i64,
            tv.muted as i64
        ],
    )?;
    Ok(())
}

pub fn list_tvs(conn: &Connection) -> Result<Vec<Tv>> {
    let mut st =
        conn.prepare("SELECT id, name, channel, power, volume, muted FROM tvs ORDER BY id")?;
    let rows = st.query_map([], tv_from)?;
    rows.collect()
}

pub fn get_tv(conn: &Connection, id: &str) -> Result<Option<Tv>> {
    conn.query_row(
        "SELECT id, name, channel, power, volume, muted FROM tvs WHERE id = ?1",
        [id],
        tv_from,
    )
    .optional()
}

// ---------- per-show settings ----------

/// A show's settings, as set once for every channel.
pub fn get_show_settings(
    conn: &Connection,
    show: &str,
) -> Result<Option<tvmaster_core::ShowOptions>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM show_settings WHERE show = ?1",
            [show.trim().to_lowercase()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// Every show that has settings: (name as written, settings).
pub fn list_show_settings(conn: &Connection) -> Result<Vec<(String, tvmaster_core::ShowOptions)>> {
    let mut st = conn.prepare("SELECT name, json FROM show_settings ORDER BY show")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (name, json) = row?;
        if let Ok(o) = serde_json::from_str(&json) {
            out.push((name, o));
        }
    }
    Ok(out)
}

/// Save a show's settings; settings with nothing in them remove the entry.
pub fn set_show_settings(
    conn: &Connection,
    show: &str,
    settings: &tvmaster_core::ShowOptions,
) -> Result<()> {
    let key = show.trim().to_lowercase();
    if *settings == tvmaster_core::ShowOptions::default() {
        conn.execute("DELETE FROM show_settings WHERE show = ?1", [key])?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO show_settings (show, name, json) VALUES (?1, ?2, ?3)
         ON CONFLICT(show) DO UPDATE SET name = excluded.name, json = excluded.json",
        params![
            key,
            show.trim(),
            serde_json::to_string(settings).unwrap_or_else(|_| "{}".into())
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;
    use tvmaster_core::Lineup;

    fn chan(id: &str, number: u32, libs: Vec<i64>) -> Channel {
        Channel {
            id: id.into(),
            number,
            name: format!("Channel {number}"),
            strategy: Strategy::Shuffle,
            run_out: RunOutPolicy::KeepGoing,
            autopilot: true,
            chaos: 10,
            seed: 99,
            epoch_ms: 1_000,
            always_on: false,
            library_ids: libs,
            lineup: Lineup::default(),
            kind: ChannelKind::Programming,
            group: None,
            guide: Default::default(),
            options: Default::default(),
        }
    }

    fn new_media(lib: i64, path: &str, ms: i64) -> NewMedia {
        NewMedia {
            library_id: lib,
            path: path.into(),
            duration_ms: ms,
            ..Default::default()
        }
    }

    #[test]
    fn library_add_is_idempotent() {
        let c = open_in_memory().unwrap();
        let a = add_library(&c, "/tv", "shows").unwrap();
        let b = add_library(&c, "/tv", "shows").unwrap();
        assert_eq!(a, b);
        assert_eq!(list_libraries(&c).unwrap().len(), 1);
    }

    #[test]
    fn scan_marks_missing_and_upsert_revives() {
        let c = open_in_memory().unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        upsert_media(&c, &new_media(lib.id, "/tv/a.mp4", 1000)).unwrap();
        upsert_media(&c, &new_media(lib.id, "/tv/b.mp4", 2000)).unwrap();
        assert_eq!(count_media(&c, lib.id).unwrap(), 2);

        mark_library_missing(&c, lib.id).unwrap();
        upsert_media(&c, &new_media(lib.id, "/tv/a.mp4", 1500)).unwrap();
        assert_eq!(count_media(&c, lib.id).unwrap(), 1);
        let rows = list_media_in_libraries(&c, &[lib.id]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].duration_ms, 1500);
    }

    #[test]
    fn only_shows_libraries_feed_channels() {
        let c = open_in_memory().unwrap();
        let shows = add_library(&c, "/tv", "shows").unwrap();
        let ads = add_library(&c, "/ads", "commercials").unwrap();
        upsert_media(&c, &new_media(shows.id, "/tv/a.mp4", 1000)).unwrap();
        upsert_media(&c, &new_media(ads.id, "/ads/x.mp4", 30000)).unwrap();
        let rows = list_media_in_libraries(&c, &[shows.id, ads.id]).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn channel_roundtrip_with_libraries() {
        let c = open_in_memory().unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        let ch = chan("ch1", 5, vec![lib.id]);
        upsert_channel(&c, &ch).unwrap();
        assert_eq!(get_channel(&c, "ch1").unwrap().unwrap(), ch);

        let mut edited = ch.clone();
        edited.name = "Renamed".into();
        edited.library_ids = vec![];
        upsert_channel(&c, &edited).unwrap();
        assert_eq!(list_channels(&c).unwrap(), vec![edited]);
        assert!(delete_channel(&c, "ch1").unwrap());
        assert!(get_channel(&c, "ch1").unwrap().is_none());
    }

    fn item(start: i64, end: i64, id: i64, pos: u64) -> ScheduledItem {
        ScheduledItem {
            kind: ItemKind::Show,
            media_id: Some(id),
            start_ms: start,
            end_ms: end,
            cursor_key: Some("show:a".into()),
            cursor_pos: Some(pos),
            label: Some("Prime".into()),
            in_ms: 0,
            bumper: None,
        }
    }

    #[test]
    fn lineup_roundtrips_through_the_channel() {
        let c = open_in_memory().unwrap();
        let mut ch = chan("ch1", 5, vec![]);
        ch.lineup = serde_json::from_str(
            r#"{"timezone":"America/Toronto","blocks":[{"name":"P","start":"20:00","end":"21:00","source":{"type":"chaos"}}]}"#,
        )
        .unwrap();
        upsert_channel(&c, &ch).unwrap();
        assert_eq!(get_channel(&c, "ch1").unwrap().unwrap().lineup, ch.lineup);
    }

    #[test]
    fn schedule_insert_query_and_trim() {
        let c = open_in_memory().unwrap();
        upsert_channel(&c, &chan("ch1", 5, vec![])).unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        let ids: Vec<i64> = (1..=3)
            .map(|i| upsert_media(&c, &new_media(lib.id, &format!("/tv/{i}.mp4"), 10)).unwrap())
            .collect();
        assert_eq!(schedule_last_end(&c, "ch1").unwrap(), None);
        let items = vec![
            item(0, 10, ids[0], 0),
            item(10, 20, ids[1], 1),
            item(20, 30, ids[2], 2),
        ];
        insert_schedule(&c, "ch1", &items).unwrap();
        assert_eq!(schedule_last_end(&c, "ch1").unwrap(), Some(30));
        assert_eq!(
            schedule_between(&c, "ch1", 5, 15).unwrap(),
            items[..2].to_vec()
        );
        assert_eq!(
            schedule_between(&c, "ch1", 10, 20).unwrap(),
            items[1..2].to_vec()
        );
        assert_eq!(delete_schedule_from(&c, "ch1", 20).unwrap(), 1);
        assert_eq!(prune_schedule_before(&c, "ch1", 11).unwrap(), 1);
        assert_eq!(
            schedule_between(&c, "ch1", 0, 100).unwrap(),
            items[1..2].to_vec()
        );
    }

    #[test]
    fn play_row_finds_current_then_next_and_joins_the_file() {
        let c = open_in_memory().unwrap();
        upsert_channel(&c, &chan("ch1", 5, vec![])).unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        let m = upsert_media(
            &c,
            &NewMedia {
                library_id: lib.id,
                path: "/tv/a.mp4".into(),
                duration_ms: 10,
                acodec: Some("aac".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut filler = item(10, 20, 0, 0);
        filler.kind = ItemKind::Filler;
        filler.media_id = None;
        insert_schedule(&c, "ch1", &[item(0, 10, m, 0), filler]).unwrap();
        let now = play_row_at_or_after(&c, "ch1", 5).unwrap().unwrap();
        assert_eq!(now.path.as_deref(), Some("/tv/a.mp4"));
        assert!(now.has_audio);
        let next = play_row_at_or_after(&c, "ch1", 10).unwrap().unwrap();
        assert_eq!(next.item.kind, ItemKind::Filler);
        assert_eq!(next.path, None);
        assert!(play_row_at_or_after(&c, "ch1", 20).unwrap().is_none());
        mark_media_missing(&c, m).unwrap();
        assert!(list_media_in_libraries(&c, &[lib.id]).unwrap().is_empty());
    }

    #[test]
    fn cursors_roundtrip_per_channel() {
        let c = open_in_memory().unwrap();
        upsert_channel(&c, &chan("a", 1, vec![])).unwrap();
        upsert_channel(&c, &chan("b", 2, vec![])).unwrap();
        let mut cur = Cursors::new();
        cur.insert(
            "show:x".into(),
            EpisodeCursor {
                reserved: 5,
                committed: 3,
            },
        );
        save_cursors(&c, "a", &cur).unwrap();
        assert_eq!(load_cursors(&c, "a").unwrap(), cur);
        assert!(load_cursors(&c, "b").unwrap().is_empty());
        cur.get_mut("show:x").unwrap().committed = 4;
        save_cursors(&c, "a", &cur).unwrap();
        assert_eq!(load_cursors(&c, "a").unwrap()["show:x"].committed, 4);
    }

    #[test]
    fn breaks_survive_rescans_but_not_file_changes() {
        let c = open_in_memory().unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        let id = upsert_media(&c, &new_media(lib.id, "/tv/a.mp4", 1000)).unwrap();
        assert_eq!(media_needing_analysis(&c, lib.id).unwrap().len(), 1);
        set_breaks(&c, id, r#"{"src":"black","points":[5]}"#).unwrap();
        assert!(media_needing_analysis(&c, lib.id).unwrap().is_empty());
        // a rescan of the same file keeps the analysis
        upsert_media(&c, &new_media(lib.id, "/tv/a.mp4", 1000)).unwrap();
        assert!(get_media(&c, id).unwrap().unwrap().breaks_json.is_some());
        // a different length means a different file: analysis starts over
        upsert_media(&c, &new_media(lib.id, "/tv/a.mp4", 2000)).unwrap();
        assert!(get_media(&c, id).unwrap().unwrap().breaks_json.is_none());
        // chapter-derived breaks supplied at scan time are stored
        let mut m = new_media(lib.id, "/tv/a.mp4", 2000);
        m.breaks_json = Some(r#"{"src":"chapters","points":[9]}"#.into());
        m.year = Some(1987);
        upsert_media(&c, &m).unwrap();
        let row = get_media(&c, id).unwrap().unwrap();
        assert_eq!(row.year, Some(1987));
        assert!(row.breaks_json.unwrap().contains("chapters"));
    }

    #[test]
    fn kinds_are_listed_separately() {
        let c = open_in_memory().unwrap();
        let shows = add_library(&c, "/tv", "shows").unwrap();
        let ads = add_library(&c, "/ads", "commercials").unwrap();
        upsert_media(&c, &new_media(shows.id, "/tv/a.mp4", 1000)).unwrap();
        upsert_media(&c, &new_media(ads.id, "/ads/x.mp4", 30_000)).unwrap();
        let both = [shows.id, ads.id];
        assert_eq!(
            list_media_of_kind(&c, &both, "commercials").unwrap().len(),
            1
        );
        assert_eq!(list_media_of_kind(&c, &both, "shows").unwrap().len(), 1);
        assert!(list_media_of_kind(&c, &both, "bumpers").unwrap().is_empty());
    }

    #[test]
    fn bumpers_and_acts_roundtrip_and_recent_commercials_are_ordered() {
        let c = open_in_memory().unwrap();
        upsert_channel(&c, &chan("ch1", 5, vec![])).unwrap();
        let lib = add_library(&c, "/tv", "shows").unwrap();
        let ids: Vec<i64> = (1..=4)
            .map(|i| upsert_media(&c, &new_media(lib.id, &format!("/tv/{i}.mp4"), 10)).unwrap())
            .collect();
        let mk = |kind, start, media: i64, bumper, in_ms| ScheduledItem {
            kind,
            media_id: Some(media),
            start_ms: start,
            end_ms: start + 10,
            cursor_key: None,
            cursor_pos: None,
            label: None,
            in_ms,
            bumper,
        };
        let items = vec![
            mk(ItemKind::Show, 0, ids[0], None, 0),
            mk(ItemKind::Bumper, 10, ids[0], Some(BumperKind::Brb), 0),
            mk(ItemKind::Commercial, 20, ids[1], None, 0),
            mk(ItemKind::Commercial, 30, ids[2], None, 0),
            mk(ItemKind::Show, 40, ids[0], None, 12_345),
            mk(ItemKind::Bumper, 50, ids[3], None, 0), // a user's clip
        ];
        insert_schedule(&c, "ch1", &items).unwrap();
        assert_eq!(schedule_between(&c, "ch1", 0, 100).unwrap(), items);
        assert_eq!(
            recent_commercials(&c, "ch1", 100, 10).unwrap(),
            vec![ids[1], ids[2]]
        );
        assert_eq!(recent_commercials(&c, "ch1", 100, 1).unwrap(), vec![ids[2]]);
        assert_eq!(recent_commercials(&c, "ch1", 25, 10).unwrap(), vec![ids[1]]);
        let row = play_row_at_or_after(&c, "ch1", 12).unwrap().unwrap();
        assert_eq!(row.item.bumper, Some(BumperKind::Brb));
        assert_eq!(row.show, None);
    }

    #[test]
    fn shows_are_summarised_across_libraries() {
        let c = open_in_memory().unwrap();
        let a = add_library(&c, "/a", "shows").unwrap();
        let b = add_library(&c, "/b", "shows").unwrap();
        let ads = add_library(&c, "/ads", "commercials").unwrap();
        let mk = |lib: i64, path: &str, show: &str, ms: i64| {
            let mut m = new_media(lib, path, ms);
            m.show = Some(show.into());
            upsert_media(&c, &m).unwrap();
        };
        mk(a.id, "/a/1", "Cheers", 1000);
        mk(a.id, "/a/2", "cheers", 1000);
        mk(b.id, "/b/1", "Taxi", 500);
        mk(b.id, "/b/2", "Cheers", 1000);
        mk(ads.id, "/ads/1", "Spot", 30);
        let all = list_shows(&c, &[]).unwrap();
        assert_eq!(
            all.iter()
                .map(|s| (s.name.to_lowercase(), s.episodes))
                .collect::<Vec<_>>(),
            vec![("cheers".to_string(), 3), ("taxi".to_string(), 1)]
        );
        let only_b = list_shows(&c, &[b.id]).unwrap();
        assert_eq!(only_b.len(), 2);
        let both = list_shows(&c, &[a.id, b.id]).unwrap();
        assert_eq!(
            both.iter()
                .find(|s| s.name.eq_ignore_ascii_case("cheers"))
                .unwrap()
                .episodes,
            3
        );
        assert_eq!(
            both.iter()
                .find(|s| s.name.eq_ignore_ascii_case("cheers"))
                .unwrap()
                .total_ms,
            3000
        );
        // missing files drop out
        mark_library_missing(&c, b.id).unwrap();
        assert!(list_shows(&c, &[])
            .unwrap()
            .iter()
            .all(|s| !s.name.eq_ignore_ascii_case("taxi")));
    }

    #[test]
    fn libraries_and_tvs_can_be_removed_and_disabled() {
        let c = open_in_memory().unwrap();
        let lib = add_library(&c, "/a", "shows").unwrap();
        upsert_media(&c, &new_media(lib.id, "/a/1", 10)).unwrap();
        upsert_channel(&c, &chan("ch", 1, vec![lib.id])).unwrap();
        assert!(set_library_enabled(&c, lib.id, false).unwrap());
        assert!(
            list_media_in_libraries(&c, &[lib.id]).unwrap().is_empty(),
            "disabled libraries do not feed channels"
        );
        assert!(delete_library(&c, lib.id).unwrap());
        assert!(!delete_library(&c, lib.id).unwrap());
        assert!(
            get_channel(&c, "ch")
                .unwrap()
                .unwrap()
                .library_ids
                .is_empty(),
            "channel keeps existing without the library"
        );
        upsert_tv(&c, &Tv::new("den", "Den")).unwrap();
        assert!(delete_tv(&c, "den").unwrap());
        assert!(!delete_tv(&c, "den").unwrap());
    }

    #[test]
    fn meta_roundtrip() {
        let c = open_in_memory().unwrap();
        assert_eq!(meta_get(&c, "k").unwrap(), None);
        meta_set(&c, "k", "1").unwrap();
        meta_set(&c, "k", "2").unwrap();
        assert_eq!(meta_get(&c, "k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn tv_roundtrip() {
        let c = open_in_memory().unwrap();
        let mut tv = Tv::new("den", "Den");
        tv.channel = Some(4);
        tv.power = true;
        upsert_tv(&c, &tv).unwrap();
        assert_eq!(get_tv(&c, "den").unwrap().unwrap(), tv);
        assert_eq!(list_tvs(&c).unwrap().len(), 1);
        assert!(get_tv(&c, "nope").unwrap().is_none());
    }

    #[test]
    fn show_settings_are_kept_by_lower_case_name_and_cleared_when_empty() {
        use tvmaster_core::{ShowOptions, SubMode};
        let c = crate::open_in_memory().unwrap();
        assert_eq!(get_show_settings(&c, "Bebop").unwrap(), None);
        let s = ShowOptions {
            audio_languages: Some(vec!["jpn".into()]),
            subtitles: Some(SubMode::Always),
            ..Default::default()
        };
        set_show_settings(&c, "Cowboy Bebop", &s).unwrap();
        assert_eq!(
            get_show_settings(&c, "  cowboy BEBOP ").unwrap(),
            Some(s.clone())
        );
        // changing it replaces it, and the name keeps the spelling it was saved with
        let s2 = ShowOptions {
            subtitles: Some(SubMode::Off),
            ..Default::default()
        };
        set_show_settings(&c, "cowboy bebop", &s2).unwrap();
        let all = list_show_settings(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "cowboy bebop");
        assert_eq!(all[0].1, s2);
        // nothing in it: removed
        set_show_settings(&c, "Cowboy Bebop", &ShowOptions::default()).unwrap();
        assert!(list_show_settings(&c).unwrap().is_empty());
    }
}
