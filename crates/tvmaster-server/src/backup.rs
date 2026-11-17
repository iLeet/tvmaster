//! Backup and restore of the station: channels, lineups, libraries (their folders and what was
//! found in them), show names and details, episode positions, settings and uploaded pictures.
//! Not the video files themselves, and not this machine's access token.
//!
//! A backup is one `.tar.gz` file in `<data folder>/backups/`: a manifest, a copy of the database
//! and the pictures. Restoring first backs up what is there, so a mistake can be undone.

use crate::{plan::now_ms, ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path as FsPath, PathBuf};

const FORMAT: u32 = 1;
const DB_NAME: &str = "tvmaster.db";
const MANIFEST: &str = "manifest.json";
const AUTO_KEY: &str = "backup_auto";
const KEEP_KEY: &str = "backup_keep";
/// Largest backup accepted by upload or restore.
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// What belongs to this machine's access rather than to the station: never exported, never overwritten.
const KEEP_META: [&str; 3] = [crate::auth::META_KEY, crate::auth::OPEN_VIEWING_KEY, "hdhr_device_id"];
/// In the order they can be filled (parents first). The schedule is not kept: it is planned again.
const TABLES: [&str; 10] = [
    "libraries",
    "media_items",
    "channels",
    "channel_libraries",
    "cursors",
    "show_settings",
    "show_names",
    "show_info",
    "tvs",
    "meta",
];

fn err(status: StatusCode, m: impl Into<String>) -> ApiError {
    ApiError::new(status, m)
}

fn internal(m: impl std::fmt::Display) -> ApiError {
    err(StatusCode::INTERNAL_SERVER_ERROR, m.to_string())
}

fn valid_name(n: &str) -> bool {
    n.len() <= 100
        && n.ends_with(".tar.gz")
        && !n.starts_with('.')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn dir_of(s: &AppState) -> ApiResult<PathBuf> {
    let d = s
        .data_dir
        .clone()
        .ok_or_else(|| err(StatusCode::CONFLICT, "this server has no data folder to keep backups in"))?
        .join("backups");
    std::fs::create_dir_all(&d).map_err(|e| internal(format!("cannot make the backups folder: {e}")))?;
    Ok(d)
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub name: String,
    pub size: u64,
    pub created_ms: i64,
    pub label: String,
    /// Made by the nightly backup.
    pub auto: bool,
    pub version: String,
    pub counts: Value,
}

fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap_or(0)
}

/// Make a backup file now. Blocking.
fn create_archive(s: &AppState, label: &str, auto: bool) -> Result<Info, String> {
    let dir = dir_of(s).map_err(|e| e.message().to_string())?;
    let label: String = label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect();
    let base = format!("tvmaster-{}{}{}", if auto { "auto-" } else { "" }, stamp(), if label.is_empty() { String::new() } else { format!("-{label}") });
    let mut name = format!("{base}.tar.gz");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}.tar.gz");
        n += 1;
    }
    // A consistent copy of the live database, trimmed to what a backup keeps.
    let snapshot = dir.join(format!(".{base}.db"));
    let _ = std::fs::remove_file(&snapshot);
    {
        let db = s.db();
        db.execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])
            .map_err(|e| format!("could not copy the database: {e}"))?;
    }
    let counts = {
        let c = Connection::open(&snapshot).map_err(|e| e.to_string())?;
        c.execute("DELETE FROM schedule_items", []).map_err(|e| e.to_string())?;
        let keep = KEEP_META.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
        c.execute(&format!("DELETE FROM meta WHERE key IN ({keep})"), []).map_err(|e| e.to_string())?;
        let counts = json!({
            "channels": count(&c, "channels"),
            "libraries": count(&c, "libraries"),
            "files": count(&c, "media_items"),
        });
        let _ = c.execute_batch("VACUUM");
        counts
    };
    let assets: Vec<PathBuf> = s
        .assets_dir
        .as_ref()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    let created = now_ms();
    let manifest = json!({
        "format": FORMAT,
        "version": env!("CARGO_PKG_VERSION"),
        "created_ms": created,
        "label": label,
        "auto": auto,
        "data_dir": s.data_dir.as_ref().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
        "counts": counts,
        "pictures": assets.len(),
    });
    let part = dir.join(format!(".{name}.part"));
    let built = (|| -> std::io::Result<()> {
        let file = std::fs::File::create(&part)?;
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(file, flate2::Compression::default()));
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        h.set_mtime((created / 1000) as u64);
        h.set_cksum();
        tar.append_data(&mut h, MANIFEST, &bytes[..])?;
        tar.append_path_with_name(&snapshot, DB_NAME)?;
        for a in &assets {
            if let Some(n) = a.file_name().and_then(|n| n.to_str()) {
                tar.append_path_with_name(a, format!("assets/{n}"))?;
            }
        }
        tar.into_inner()?.finish()?.flush()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&snapshot);
    if let Err(e) = built {
        let _ = std::fs::remove_file(&part);
        return Err(format!("could not write the backup: {e}"));
    }
    std::fs::rename(&part, dir.join(&name)).map_err(|e| e.to_string())?;
    info_of(&dir.join(&name)).ok_or_else(|| "the new backup cannot be read".to_string())
}

/// The manifest of a backup file, or `None` if it is not one.
fn read_manifest(path: &FsPath) -> Option<Value> {
    let f = std::fs::File::open(path).ok()?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    for e in ar.entries().ok()?.take(4) {
        let mut e = e.ok()?;
        if e.path().ok()?.to_str() == Some(MANIFEST) {
            let mut buf = Vec::new();
            e.by_ref().take(1 << 20).read_to_end(&mut buf).ok()?;
            let v: Value = serde_json::from_slice(&buf).ok()?;
            return (v["format"].as_u64()? as u32 <= FORMAT).then_some(v);
        }
    }
    None
}

fn info_of(path: &FsPath) -> Option<Info> {
    let m = read_manifest(path)?;
    Some(Info {
        name: path.file_name()?.to_string_lossy().to_string(),
        size: std::fs::metadata(path).ok()?.len(),
        created_ms: m["created_ms"].as_i64().unwrap_or(0),
        label: m["label"].as_str().unwrap_or("").to_string(),
        auto: m["auto"].as_bool().unwrap_or(false),
        version: m["version"].as_str().unwrap_or("").to_string(),
        counts: m["counts"].clone(),
    })
}

fn list_all(dir: &FsPath) -> Vec<Info> {
    let mut out: Vec<Info> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(valid_name))
                .filter_map(|p| info_of(&p))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|i| std::cmp::Reverse(i.created_ms));
    out
}

fn setting(db: &Connection, key: &str) -> Option<String> {
    tvmaster_db::meta_get(db, key).ok().flatten()
}

fn auto_on(db: &Connection) -> bool {
    setting(db, AUTO_KEY).is_none_or(|v| v != "0")
}

fn keep_n(db: &Connection) -> usize {
    setting(db, KEEP_KEY).and_then(|v| v.parse().ok()).unwrap_or(7).clamp(1, 60)
}

/// Delete the oldest nightly backups beyond `keep`.
fn prune(dir: &FsPath, keep: usize) {
    for i in list_all(dir).into_iter().filter(|i| i.auto).skip(keep) {
        let _ = std::fs::remove_file(dir.join(i.name));
    }
}

/// Unpack a backup into `into`, accepting only the files a backup holds.
fn unpack(path: &FsPath, into: &FsPath) -> Result<Value, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    std::fs::create_dir_all(into.join("assets")).map_err(|e| e.to_string())?;
    let mut got_db = false;
    for e in ar.entries().map_err(|_| "that is not a TVMaster backup".to_string())? {
        let mut e = e.map_err(|_| "that backup is damaged".to_string())?;
        if !e.header().entry_type().is_file() {
            continue;
        }
        let p = e.path().map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        let dest = if p == MANIFEST || p == DB_NAME {
            into.join(&p)
        } else if let Some(n) = p.strip_prefix("assets/") {
            if n.is_empty() || n.contains('/') || n.starts_with('.') {
                return Err("that backup holds a file in the wrong place".into());
            }
            into.join("assets").join(n)
        } else {
            return Err(format!("that backup holds something unexpected ({p})"));
        };
        if p == DB_NAME {
            got_db = true;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut e.by_ref().take(MAX_BYTES), &mut out).map_err(|e| e.to_string())?;
    }
    let manifest: Value = std::fs::read(into.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("that is not a TVMaster backup (no manifest)")?;
    if manifest["format"].as_u64().unwrap_or(0) as u32 > FORMAT {
        return Err("that backup was made by a newer TVMaster; update this server first".into());
    }
    if !got_db {
        return Err("that backup holds no database".into());
    }
    Ok(manifest)
}

fn columns(conn: &Connection, table: &str, schema: &str) -> rusqlite::Result<Vec<String>> {
    let mut st = conn.prepare("SELECT name FROM pragma_table_info(?1, ?2)")?;
    let rows = st.query_map([table, schema], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Replace what is in the live database with what is in the attached backup (`bk`).
fn apply(conn: &Connection, old_dir: &str, new_dir: &str) -> Result<(), String> {
    let e = |e: rusqlite::Error| e.to_string();
    let keep = KEEP_META.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
    let tx = conn.unchecked_transaction().map_err(e)?;
    for t in ["schedule_items", "cursors", "channel_libraries", "channels", "media_items", "libraries", "show_settings", "show_names", "show_info", "tvs"] {
        tx.execute(&format!("DELETE FROM main.{t}"), []).map_err(e)?;
    }
    tx.execute(&format!("DELETE FROM main.meta WHERE key NOT IN ({keep})"), []).map_err(e)?;
    for t in TABLES {
        let mine = columns(&tx, t, "main").map_err(e)?;
        let theirs = columns(&tx, t, "bk").map_err(e)?;
        let both: Vec<String> = mine.into_iter().filter(|c| theirs.contains(c)).collect();
        if both.is_empty() {
            continue;
        }
        let cols = both.join(", ");
        let filter = if t == "meta" { format!(" WHERE key NOT IN ({keep})") } else { String::new() };
        tx.execute(&format!("INSERT OR REPLACE INTO main.{t} ({cols}) SELECT {cols} FROM bk.{t}{filter}"), []).map_err(e)?;
    }
    // A backup made with another data folder points its pictures and libraries at that folder.
    if !old_dir.is_empty() && old_dir != new_dir {
        for (table, col) in [
            ("libraries", "path"),
            ("channels", "lineup_json"),
            ("channels", "guide_json"),
            ("channels", "options_json"),
            ("show_info", "json"),
            ("show_settings", "json"),
            ("meta", "value"),
        ] {
            tx.execute(&format!("UPDATE main.{table} SET {col} = REPLACE({col}, ?1, ?2) WHERE {col} LIKE '%' || ?1 || '%'"), [old_dir, new_dir]).map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

#[derive(Debug, Serialize)]
pub struct Restored {
    pub channels: i64,
    pub libraries: i64,
    pub files: i64,
    pub pictures: usize,
    /// Library folders that are not there on this machine (their files stay listed, unreachable).
    pub missing_folders: Vec<String>,
    /// The copy made of the station as it was just before.
    pub safety_copy: String,
}

fn restore_archive(s: &AppState, path: &FsPath) -> Result<Restored, String> {
    let dir = dir_of(s).map_err(|e| e.message().to_string())?;
    let work = dir.join(format!(".restore-{}", stamp()));
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| -> Result<Restored, String> {
        let manifest = unpack(path, &work)?;
        // Make sure the file is a database we can read before anything is touched.
        {
            let c = Connection::open_with_flags(work.join(DB_NAME), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|_| "that backup's database is damaged".to_string())?;
            columns(&c, "channels", "main").map_err(|_| "that backup's database is damaged".to_string())?;
        }
        let safety = create_archive(s, "before-restore", false)?;
        let new_dir = s.data_dir.as_ref().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
        let old_dir = manifest["data_dir"].as_str().unwrap_or("").to_string();
        let db = s.db();
        db.execute("ATTACH DATABASE ?1 AS bk", [work.join(DB_NAME).to_string_lossy().as_ref()]).map_err(|e| e.to_string())?;
        let applied = apply(&db, &old_dir, &new_dir);
        let _ = db.execute("DETACH DATABASE bk", []);
        applied?;
        let libs = tvmaster_db::list_libraries(&db).map_err(|e| e.to_string())?;
        let missing_folders = libs.iter().filter(|l| !FsPath::new(&l.path).is_dir()).map(|l| l.path.clone()).collect();
        let report = Restored {
            channels: count(&db, "channels"),
            libraries: libs.len() as i64,
            files: count(&db, "media_items"),
            pictures: 0,
            missing_folders,
            safety_copy: safety.name,
        };
        drop(db);
        let mut pictures = 0;
        if let Some(target) = &s.assets_dir {
            let _ = std::fs::create_dir_all(target);
            if let Ok(rd) = std::fs::read_dir(work.join("assets")) {
                for f in rd.flatten() {
                    if std::fs::copy(f.path(), target.join(f.file_name())).is_ok() {
                        pictures += 1;
                    }
                }
            }
        }
        Ok(Restored { pictures, ..report })
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

// ------------------------------------------------------------------ routes

/// `GET /api/v1/backups`: the backups kept on the server, and the nightly backup settings.
pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let dir = dir_of(&s)?;
    let (auto, keep) = {
        let db = s.db();
        (auto_on(&db), keep_n(&db))
    };
    let backups = tokio::task::spawn_blocking(move || list_all(&dir)).await.map_err(internal)?;
    Ok(Json(json!({ "auto": auto, "keep": keep, "backups": backups })))
}

#[derive(Deserialize, Default)]
pub struct CreateBody {
    #[serde(default)]
    label: String,
}

/// `POST /api/v1/backups`: back up now.
pub async fn create(State(s): State<AppState>, body: Option<Json<CreateBody>>) -> ApiResult<Json<Info>> {
    let label = body.map(|b| b.0.label).unwrap_or_default();
    dir_of(&s)?; // 409 when there is nowhere to keep backups
    let info = tokio::task::spawn_blocking(move || create_archive(&s, &label, false))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    Ok(Json(info))
}

#[derive(Deserialize)]
pub struct SettingsBody {
    auto: Option<bool>,
    keep: Option<u32>,
}

/// `PUT /api/v1/backups/settings`: the nightly backup on or off, and how many nightly backups to keep.
pub async fn put_settings(State(s): State<AppState>, Json(b): Json<SettingsBody>) -> ApiResult<Json<Value>> {
    let db = s.db();
    if let Some(a) = b.auto {
        tvmaster_db::meta_set(&db, AUTO_KEY, if a { "1" } else { "0" })?;
    }
    if let Some(k) = b.keep {
        if !(1..=60).contains(&k) {
            return Err(err(StatusCode::BAD_REQUEST, "keep between 1 and 60 nightly backups"));
        }
        tvmaster_db::meta_set(&db, KEEP_KEY, &k.to_string())?;
    }
    Ok(Json(json!({ "auto": auto_on(&db), "keep": keep_n(&db) })))
}

fn named(s: &AppState, name: &str) -> ApiResult<PathBuf> {
    if !valid_name(name) {
        return Err(err(StatusCode::BAD_REQUEST, "that is not a backup name"));
    }
    let p = dir_of(s)?.join(name);
    if !p.is_file() {
        return Err(err(StatusCode::NOT_FOUND, "no such backup"));
    }
    Ok(p)
}

/// `GET /api/v1/backups/:name`: download a backup.
pub async fn download(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Response> {
    let p = named(&s, &name)?;
    let bytes = tokio::fs::read(&p).await.map_err(internal)?;
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

/// `DELETE /api/v1/backups/:name`.
pub async fn remove(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    let p = named(&s, &name)?;
    std::fs::remove_file(p).map_err(internal)?;
    Ok(Json(json!({ "deleted": name })))
}

#[derive(Deserialize)]
pub struct UploadQuery {
    #[serde(default)]
    name: String,
}

/// `POST /api/v1/backups/upload?name=station.tar.gz`: put a backup file from elsewhere on the server
/// (then restore it). The body is the file.
pub async fn upload(State(s): State<AppState>, Query(q): Query<UploadQuery>, body: axum::body::Body) -> ApiResult<Json<Info>> {
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;
    let dir = dir_of(&s)?;
    let name = if valid_name(&q.name) { q.name.clone() } else { format!("tvmaster-uploaded-{}.tar.gz", stamp()) };
    let mut dest = dir.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{}-{n}.tar.gz", name.trim_end_matches(".tar.gz")));
        n += 1;
    }
    let part = dir.join(format!(".upload-{}.part", crate::generate_token()));
    let mut file = tokio::fs::File::create(&part).await.map_err(internal)?;
    let mut stream = body.into_data_stream();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| err(StatusCode::BAD_REQUEST, format!("the upload stopped: {e}")))?;
        total += chunk.len() as u64;
        if total > MAX_BYTES {
            drop(file);
            let _ = tokio::fs::remove_file(&part).await;
            return Err(err(StatusCode::PAYLOAD_TOO_LARGE, "that file is too large to be a backup"));
        }
        file.write_all(&chunk).await.map_err(internal)?;
    }
    file.flush().await.map_err(internal)?;
    drop(file);
    let info = tokio::task::spawn_blocking({
        let (part, dest) = (part.clone(), dest.clone());
        move || {
            read_manifest(&part)
                .ok_or_else(|| "that file is not a TVMaster backup".to_string())
                .and_then(|_| std::fs::rename(&part, &dest).map_err(|e| e.to_string()))
                .and_then(|_| info_of(&dest).ok_or_else(|| "that file is not a TVMaster backup".to_string()))
        }
    })
    .await
    .map_err(internal)?;
    match info {
        Ok(i) => Ok(Json(i)),
        Err(m) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(err(StatusCode::BAD_REQUEST, m))
        }
    }
}

/// `POST /api/v1/backups/:name/restore`: put the station back as the backup had it. What is there now is
/// backed up first, streams are stopped, and the schedules are planned again.
pub async fn restore(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Json<Restored>> {
    let p = named(&s, &name)?;
    if let Some(pl) = &s.playout {
        pl.stop_all();
    }
    let st = s.clone();
    let report = tokio::task::spawn_blocking(move || restore_archive(&st, &p))
        .await
        .map_err(internal)?
        .map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
    // Everything in memory that came from the old database.
    {
        let db = s.db();
        *s.live.lock().unwrap_or_else(|p| p.into_inner()) = crate::live::load(&db);
        let now = now_ms();
        for c in tvmaster_db::list_channels(&db)?.iter().filter(|c| c.kind == tvmaster_core::ChannelKind::Programming) {
            let _ = crate::autopilot::maintain(&db, c, now);
        }
        let _ = tvmaster_db::normalize_names(&db);
    }
    s.emit(json!({ "type": "station.restored" }));
    Ok(Json(report))
}

/// Make the nightly backup when the newest one is a day old, and tidy the old ones. Called every so often.
pub fn nightly(s: &AppState) {
    let (on, keep) = {
        let db = s.db();
        (auto_on(&db), keep_n(&db))
    };
    let Some(dir) = s.data_dir.as_ref().map(|d| d.join("backups")) else { return };
    if !on {
        return;
    }
    let newest = list_all(&dir).into_iter().filter(|i| i.auto).map(|i| i.created_ms).max().unwrap_or(0);
    if now_ms() - newest >= 24 * 3_600_000 {
        match create_archive(s, "", true) {
            Ok(i) => tracing::info!(name = %i.name, "nightly backup made"),
            Err(e) => tracing::warn!("nightly backup failed: {e}"),
        }
    }
    prune(&dir, keep);
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    fn station() -> (AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("tvm-backup-{}", crate::generate_token()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        let mut s = AppState::in_memory().with_data_dir(&dir);
        s.assets_dir = Some(dir.join("assets"));
        (s, dir)
    }

    fn seed(s: &AppState) {
        let db = s.db();
        let lib = tvmaster_db::add_library(&db, "/media/tv", "shows").unwrap();
        tvmaster_db::upsert_media(&db, &tvmaster_db::NewMedia { library_id: lib.id, path: "/media/tv/Cheers/a.mkv".into(), show: Some("Cheers".into()), duration_ms: 1_320_000, ..Default::default() }).unwrap();
        tvmaster_db::set_name_rule(&db, "Cheers", "Norm's Bar", None).unwrap();
        tvmaster_db::meta_set(&db, "station_theme", r#"{"theme":"midnight","style":"modern"}"#).unwrap();
        tvmaster_db::meta_set(&db, crate::auth::META_KEY, "this-machines-secret-token").unwrap();
    }

    #[tokio::test]
    async fn a_backup_restores_the_station_but_never_touches_the_access_token() {
        let (s, dir) = station();
        seed(&s);
        std::fs::write(dir.join("assets/logo.png"), b"png").unwrap();
        let app = router(s.clone());
        let (st, c) = call(&app, "POST", "/api/v1/channels", r#"{"id":"c1","number":3,"name":"Cartoons"}"#).await;
        assert_eq!(st, StatusCode::CREATED, "{c}");
        let (st, info) = call(&app, "POST", "/api/v1/backups", r#"{"label":"before change"}"#).await;
        assert_eq!(st, StatusCode::OK, "{info}");
        let name = info["name"].as_str().unwrap().to_string();
        assert_eq!(info["counts"]["channels"], 1);
        assert!(name.contains("beforechange"), "{name}");
        // Change things after the backup.
        call(&app, "DELETE", "/api/v1/channels/c1", "").await;
        tvmaster_db::set_name_rule(&s.db(), "Cheers", "Wrong", None).unwrap();
        tvmaster_db::meta_set(&s.db(), "station_theme", r#"{"theme":"snow","style":"retro"}"#).unwrap();
        tvmaster_db::meta_set(&s.db(), crate::auth::META_KEY, "a-newer-token-set-after").unwrap();
        std::fs::remove_file(dir.join("assets/logo.png")).unwrap();

        let (st, r) = call(&app, "POST", &format!("/api/v1/backups/{name}/restore"), "").await;
        assert_eq!(st, StatusCode::OK, "{r}");
        assert_eq!(r["channels"], 1);
        assert_eq!(r["files"], 1);
        assert_eq!(r["pictures"], 1);
        assert!(r["safety_copy"].as_str().unwrap().contains("before-restore"));
        assert_eq!(r["missing_folders"].as_array().unwrap().len(), 1, "/media/tv is not on this machine");
        let db = s.db();
        assert!(tvmaster_db::get_channel(&db, "c1").unwrap().is_some(), "the channel is back");
        assert_eq!(tvmaster_db::meta_get(&db, "station_theme").unwrap().unwrap(), r#"{"theme":"midnight","style":"modern"}"#);
        assert_eq!(tvmaster_db::meta_get(&db, crate::auth::META_KEY).unwrap().unwrap(), "a-newer-token-set-after", "this machine keeps its own token");
        assert_eq!(tvmaster_db::NameMap::load(&db).unwrap().resolve("Cheers").0, "Norm's Bar", "show names are back");
        assert!(dir.join("assets/logo.png").is_file(), "pictures are back");
        drop(db);
        // The backup holds no secrets.
        let bytes = std::fs::read(dir.join("backups").join(&name)).unwrap();
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut text).unwrap();
        assert!(!String::from_utf8_lossy(&text).contains("this-machines-secret-token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn backups_are_listed_downloaded_uploaded_and_deleted() {
        let (s, dir) = station();
        seed(&s);
        let app = router(s.clone());
        let (_, info) = call(&app, "POST", "/api/v1/backups", "{}").await;
        let name = info["name"].as_str().unwrap().to_string();
        let (_, l) = call(&app, "GET", "/api/v1/backups", "").await;
        assert_eq!(l["backups"].as_array().unwrap().len(), 1);
        assert_eq!(l["auto"], true);
        let bytes = std::fs::read(dir.join("backups").join(&name)).unwrap();
        // The same file arrives from another machine.
        use tower::ServiceExt;
        let req = axum::http::Request::builder().method("POST").uri("/api/v1/backups/upload?name=moved.tar.gz").body(axum::body::Body::from(bytes)).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let (_, l) = call(&app, "GET", "/api/v1/backups", "").await;
        assert_eq!(l["backups"].as_array().unwrap().len(), 2);
        // Something that is not a backup is refused and leaves nothing behind.
        let req = axum::http::Request::builder().method("POST").uri("/api/v1/backups/upload?name=x.tar.gz").body(axum::body::Body::from("not a backup")).unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::BAD_REQUEST);
        let leftovers = std::fs::read_dir(dir.join("backups")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with('.')).count();
        assert_eq!(leftovers, 0);
        let (st, _) = call(&app, "DELETE", "/api/v1/backups/moved.tar.gz", "").await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) = call(&app, "DELETE", "/api/v1/backups/..%2Fetc.tar.gz", "").await;
        assert_ne!(st, StatusCode::OK, "names cannot leave the backups folder");
        let (st, _) = call(&app, "POST", "/api/v1/backups/nope.tar.gz/restore", "").await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_backup_from_another_data_folder_points_at_this_one_and_nightly_ones_are_pruned() {
        let (s, dir) = station();
        {
            let db = s.db();
            let here = dir.to_string_lossy().to_string();
            tvmaster_db::meta_set(&db, "k", &format!("{here}/assets/logo.png")).unwrap();
        }
        let app = router(s.clone());
        let (_, info) = call(&app, "POST", "/api/v1/backups", "{}").await;
        let name = info["name"].as_str().unwrap().to_string();
        // Pretend the backup was made elsewhere: rewrite its manifest's data folder and its stored path.
        let other = "/old/place";
        let path = dir.join("backups").join(&name);
        let work = dir.join("unpacked");
        super::unpack(&path, &work).unwrap();
        let c = rusqlite::Connection::open(work.join(super::DB_NAME)).unwrap();
        c.execute("UPDATE meta SET value = REPLACE(value, ?1, ?2) WHERE key = 'k'", [dir.to_string_lossy().as_ref(), other]).unwrap();
        drop(c);
        let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(work.join(super::MANIFEST)).unwrap()).unwrap();
        m["data_dir"] = other.into();
        std::fs::write(work.join(super::MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        let moved = dir.join("backups/tvmaster-moved.tar.gz");
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(std::fs::File::create(&moved).unwrap(), flate2::Compression::fast()));
        tar.append_path_with_name(work.join(super::MANIFEST), super::MANIFEST).unwrap();
        tar.append_path_with_name(work.join(super::DB_NAME), super::DB_NAME).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let (st, r) = call(&app, "POST", "/api/v1/backups/tvmaster-moved.tar.gz/restore", "").await;
        assert_eq!(st, StatusCode::OK, "{r}");
        let k = tvmaster_db::meta_get(&s.db(), "k").unwrap().unwrap();
        assert_eq!(k, format!("{}/assets/logo.png", dir.to_string_lossy()), "paths follow the data folder");
        // Nightly: made when none is a day old, then kept to the limit.
        let (st, _) = call(&app, "PUT", "/api/v1/backups/settings", r#"{"keep":2}"#).await;
        assert_eq!(st, StatusCode::OK);
        super::nightly(&s);
        let (_, l) = call(&app, "GET", "/api/v1/backups", "").await;
        assert_eq!(l["backups"].as_array().unwrap().iter().filter(|b| b["auto"] == true).count(), 1);
        super::nightly(&s);
        assert_eq!(super::list_all(&dir.join("backups")).iter().filter(|b| b.auto).count(), 1, "not again within a day");
        let (st, _) = call(&app, "PUT", "/api/v1/backups/settings", r#"{"keep":0}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        // A server with no data folder cannot keep backups.
        let (st, _) = call(&router(AppState::in_memory()), "POST", "/api/v1/backups", "{}").await;
        assert_eq!(st, StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
