//! Show names, grouping and details. What a folder is called is the show's
//! *detected* name; the station can call it something else, and several
//! folders can be one show ("Bleach season 01", "Bleach season 02" → "Bleach").

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

const AUTO_KEY: &str = "auto_clean_names";

/// A rule: folders detected as `raw` are the show `title` (and, if set, this season).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NameRule {
    pub raw: String,
    pub title: String,
    pub season: Option<i64>,
}

/// How detected names become show titles: explicit rules first, then (unless
/// switched off) a trailing "season N" is split off the folder name.
#[derive(Debug, Clone, Default)]
pub struct NameMap {
    rules: HashMap<String, (String, Option<i64>)>,
    titles: HashSet<String>,
    auto: bool,
}

fn key(s: &str) -> String {
    s.trim().to_lowercase()
}

impl NameMap {
    pub fn load(conn: &Connection) -> Result<Self> {
        let auto = meta(conn, AUTO_KEY).is_none_or(|v| v != "0");
        let mut m = NameMap {
            auto,
            ..Default::default()
        };
        for r in list_name_rules(conn)? {
            m.titles.insert(key(&r.title));
            m.rules.insert(r.raw, (r.title, r.season));
        }
        Ok(m)
    }

    /// The title and season for a detected folder name.
    pub fn resolve(&self, raw: &str) -> (String, Option<i64>) {
        if let Some((t, s)) = self.rules.get(&key(raw)) {
            return (t.clone(), *s);
        }
        if self.auto {
            let (base, season) = tvmaster_core::split_season_suffix(raw);
            if season.is_some() {
                return (base, season);
            }
        }
        (raw.trim().to_string(), None)
    }

    /// Whether a rule of the user's own covers this detected name.
    pub fn has_rule(&self, raw: &str) -> bool {
        self.rules.contains_key(&key(raw))
    }

    /// The title a name in a lineup or setting stands for: a title already in use
    /// stays as it is, a detected name becomes its title.
    pub fn title(&self, name: &str) -> String {
        if self.titles.contains(&key(name)) {
            return name.to_string();
        }
        self.resolve(name).0
    }
}

fn meta(conn: &Connection, k: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [k], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

pub fn auto_clean_enabled(conn: &Connection) -> bool {
    meta(conn, AUTO_KEY).is_none_or(|v| v != "0")
}

pub fn set_auto_clean(conn: &Connection, on: bool) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![AUTO_KEY, if on { "1" } else { "0" }],
    )?;
    Ok(())
}

pub fn list_name_rules(conn: &Connection) -> Result<Vec<NameRule>> {
    let mut st = conn.prepare("SELECT raw, title, season FROM show_names ORDER BY raw")?;
    let rows = st.query_map([], |r| {
        Ok(NameRule {
            raw: r.get(0)?,
            title: r.get(1)?,
            season: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// Make folders detected as `raw` the show `title` (season optional).
pub fn set_name_rule(conn: &Connection, raw: &str, title: &str, season: Option<i64>) -> Result<()> {
    conn.execute(
        "INSERT INTO show_names (raw, title, season) VALUES (?1, ?2, ?3)
         ON CONFLICT(raw) DO UPDATE SET title = excluded.title, season = excluded.season",
        params![key(raw), title.trim(), season],
    )?;
    Ok(())
}

pub fn delete_name_rule(conn: &Connection, raw: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM show_names WHERE raw = ?1", [key(raw)])? > 0)
}

// ---------- details ----------

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ShowInfo {
    /// The show's title (as the station calls it).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Parental rating or classification, free text (`TV-PG`, `PG`, `14+`).
    #[serde(default)]
    pub rating: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// A poster picture (a file on the server, or an uploaded asset).
    #[serde(default)]
    pub poster: Option<String>,
}

impl ShowInfo {
    pub fn is_empty(&self) -> bool {
        self.description.trim().is_empty()
            && self.rating.trim().is_empty()
            && self.tags.is_empty()
            && self.poster.as_deref().is_none_or(|p| p.trim().is_empty())
    }
}

pub fn list_show_info(conn: &Connection) -> Result<BTreeMap<String, ShowInfo>> {
    let mut st = conn.prepare("SELECT title, json FROM show_info")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (k, json) = row?;
        if let Ok(i) = serde_json::from_str::<ShowInfo>(&json) {
            out.insert(k, i);
        }
    }
    Ok(out)
}

pub fn set_show_info(conn: &Connection, title: &str, info: &ShowInfo) -> Result<()> {
    if info.is_empty() {
        conn.execute("DELETE FROM show_info WHERE title = ?1", [key(title)])?;
        return Ok(());
    }
    let mut info = info.clone();
    info.title = title.trim().to_string();
    conn.execute(
        "INSERT INTO show_info (title, json) VALUES (?1, ?2)
         ON CONFLICT(title) DO UPDATE SET json = excluded.json",
        params![key(title), serde_json::to_string(&info).unwrap_or_else(|_| "{}".into())],
    )?;
    Ok(())
}

// ---------- applying names everywhere ----------

/// Bring everything that names a show in line with the current rules: lineups,
/// each channel's per-show settings, episode positions and the show settings
/// that follow a show onto every channel. Safe to run again and again.
/// Returns how many channels were changed.
pub fn normalize_names(conn: &Connection) -> Result<usize> {
    let map = NameMap::load(conn)?;
    let mut changed_channels = 0;
    for mut ch in crate::list_channels(conn)? {
        let mut changed = ch.lineup.map_shows(&mut |n| map.title(n));
        let mut shows = BTreeMap::new();
        for (name, o) in std::mem::take(&mut ch.options.shows) {
            let t = map.title(&name);
            changed |= t != name;
            shows.entry(t).or_insert(o);
        }
        ch.options.shows = shows;
        if changed {
            crate::upsert_channel(conn, &ch)?;
            changed_channels += 1;
        }
    }
    // Episode positions follow the show to its new name (the new name's own wins).
    let mut st = conn.prepare("SELECT channel_id, key FROM cursors WHERE key LIKE '%show:%'")?;
    let rows: Vec<(String, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_>>()?;
    for (channel, k) in rows {
        let Some(at) = k.rfind("show:") else { continue };
        let (prefix, name) = (&k[..at], &k[at + 5..]);
        let new = key(&map.title(name));
        if new != name {
            conn.execute(
                "UPDATE OR IGNORE cursors SET key = ?3 WHERE channel_id = ?1 AND key = ?2",
                params![channel, k, format!("{prefix}show:{new}")],
            )?;
        }
    }
    // Show settings that followed the old name.
    for (name, o) in crate::list_show_settings(conn)? {
        let t = map.title(&name);
        if t != name {
            if crate::get_show_settings(conn, &t)?.is_none() {
                crate::set_show_settings(conn, &t, &o)?;
            }
            crate::set_show_settings(conn, &name, &tvmaster_core::ShowOptions::default())?;
        }
    }
    Ok(changed_channels)
}
