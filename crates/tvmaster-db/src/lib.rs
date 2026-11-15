//! SQLite persistence: schema, connection helpers and typed queries.

mod store;

pub use store::*;

use rusqlite::Connection;
use std::path::Path;

/// Schema applied on open. Idempotent (`IF NOT EXISTS`); real migrations come
/// once the schema stabilises.
pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS libraries (
    id      INTEGER PRIMARY KEY,
    path    TEXT NOT NULL UNIQUE,
    kind    TEXT NOT NULL CHECK (kind IN ('shows', 'commercials', 'bumpers', 'music')),
    enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS media_items (
    id          INTEGER PRIMARY KEY,
    library_id  INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path        TEXT NOT NULL UNIQUE,
    show        TEXT,
    season      INTEGER,
    episode     INTEGER,
    title       TEXT,
    year        INTEGER,
    duration_ms INTEGER NOT NULL,
    width       INTEGER,
    height      INTEGER,
    vcodec      TEXT,
    acodec      TEXT,
    breaks_json TEXT,
    missing     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS channels (
    id        TEXT PRIMARY KEY,
    number    INTEGER NOT NULL UNIQUE,
    name      TEXT NOT NULL,
    strategy  TEXT NOT NULL,
    run_out   TEXT NOT NULL,
    autopilot INTEGER NOT NULL DEFAULT 1,
    chaos     INTEGER NOT NULL DEFAULT 0,
    seed      INTEGER NOT NULL DEFAULT 0,
    epoch_ms  INTEGER NOT NULL DEFAULT 0,
    always_on INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS channel_libraries (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    PRIMARY KEY (channel_id, library_id)
);

-- Reserved vs committed episode cursors (see tvmaster-core::EpisodeCursor).
CREATE TABLE IF NOT EXISTS cursors (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    show       TEXT NOT NULL,
    reserved   INTEGER NOT NULL DEFAULT 0,
    committed  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, show)
);

CREATE TABLE IF NOT EXISTS schedule_items (
    id         INTEGER PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    start_ms   INTEGER NOT NULL,
    end_ms     INTEGER NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('show', 'commercial', 'bumper', 'filler')),
    media_id   INTEGER REFERENCES media_items(id),
    in_ms      INTEGER NOT NULL DEFAULT 0,
    out_ms     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_schedule_channel_time
    ON schedule_items (channel_id, start_ms);

CREATE TABLE IF NOT EXISTS tvs (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    channel INTEGER,
    power   INTEGER NOT NULL DEFAULT 0,
    volume  INTEGER NOT NULL DEFAULT 50,
    muted   INTEGER NOT NULL DEFAULT 0
);
"#;

pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_and_accepts_rows() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO channels (id, number, name, strategy, run_out) VALUES ('c3', 3, 'Guide', 'sequential', 'keep_going')",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM channels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
    }
}
