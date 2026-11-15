//! Typed queries over the schema.

use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use serde::Serialize;
use tvmaster_core::{Channel, RunOutPolicy, Strategy, Tv};

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
}

const MEDIA_COLS: &str = "id, library_id, path, show, season, episode, title, duration_ms, width, height, vcodec, acodec";

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
            (library_id, path, show, season, episode, title, duration_ms, width, height, vcodec, acodec, missing)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)
         ON CONFLICT(path) DO UPDATE SET
            library_id = excluded.library_id, show = excluded.show, season = excluded.season,
            episode = excluded.episode, title = excluded.title, duration_ms = excluded.duration_ms,
            width = excluded.width, height = excluded.height, vcodec = excluded.vcodec,
            acodec = excluded.acodec, missing = 0
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
            m.acodec
        ],
        |r| r.get(0),
    )
}

/// Present (not missing) media from the enabled libraries of kind `shows`
/// among `library_ids`.
pub fn list_media_in_libraries(conn: &Connection, library_ids: &[i64]) -> Result<Vec<MediaRow>> {
    let mut out = Vec::new();
    let sql = format!(
        "SELECT {MEDIA_COLS} FROM media_items
         WHERE library_id = ?1 AND missing = 0
           AND library_id IN (SELECT id FROM libraries WHERE enabled = 1 AND kind = 'shows')
         ORDER BY id"
    );
    let mut st = conn.prepare(&sql)?;
    for lib in library_ids {
        let rows = st.query_map([lib], media_from)?;
        for r in rows {
            out.push(r?);
        }
    }
    Ok(out)
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
    })
}

const CHANNEL_COLS: &str =
    "id, number, name, strategy, run_out, autopilot, chaos, seed, epoch_ms, always_on";

fn channel_library_ids(conn: &Connection, channel_id: &str) -> Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT library_id FROM channel_libraries WHERE channel_id = ?1 ORDER BY library_id",
    )?;
    let rows = st.query_map([channel_id], |r| r.get(0))?;
    rows.collect()
}

pub fn upsert_channel(conn: &Connection, c: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (id, number, name, strategy, run_out, autopilot, chaos, seed, epoch_ms, always_on)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
            number = excluded.number, name = excluded.name, strategy = excluded.strategy,
            run_out = excluded.run_out, autopilot = excluded.autopilot, chaos = excluded.chaos,
            seed = excluded.seed, epoch_ms = excluded.epoch_ms, always_on = excluded.always_on",
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
            c.always_on as i64
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

// ---------- TVs ----------

fn tv_from(r: &Row) -> Result<Tv> {
    Ok(Tv {
        id: r.get(0)?,
        name: r.get(1)?,
        channel: r.get::<_, Option<i64>>(2)?.map(|n| n as u32),
        power: r.get::<_, i64>(3)? != 0,
        volume: r.get::<_, i64>(4)?.clamp(0, 100) as u8,
        muted: r.get::<_, i64>(5)? != 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

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
}
