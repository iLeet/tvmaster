use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tvmaster_db::NewMedia;
use tvmaster_media::{
    analyze as analyze_breaks, find_audio, find_videos, from_chapters, parse_name, parse_year,
    probe_with, BreakInfo,
};

const KINDS: &[&str] = &["shows", "commercials", "bumpers", "music"];
const PROBE_THREADS: usize = 4;

/// Progress of background break detection for one library.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub running: bool,
}

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let analysis = s.analysis.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let db = s.db();
    let libs = tvmaster_db::list_libraries(&db)?;
    let out: Vec<Value> = libs
        .into_iter()
        .map(|l| {
            let items = tvmaster_db::count_media(&db, l.id).unwrap_or(0);
            json!({ "id": l.id, "path": l.path, "kind": l.kind, "enabled": l.enabled, "items": items,
                    "analysis": analysis.get(&l.id) })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct NewLibrary {
    path: String,
    kind: Option<String>,
    /// Make the folder if it does not exist (for a library that uploads go into).
    #[serde(default)]
    create: bool,
}

pub async fn add(
    State(s): State<AppState>,
    Json(body): Json<NewLibrary>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let kind = body.kind.unwrap_or_else(|| "shows".to_string());
    if !KINDS.contains(&kind.as_str()) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("kind must be one of {KINDS:?}"),
        ));
    }
    let path = PathBuf::from(&body.path);
    if body.create && !path.exists() {
        if !path.is_absolute() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "give the full path of the folder to make",
            ));
        }
        std::fs::create_dir_all(&path).map_err(|e| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("could not make that folder: {e}"),
            )
        })?;
    }
    if !path.is_dir() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "path is not a directory on the server",
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "cannot resolve path"))?;
    let lib = tvmaster_db::add_library(&s.db(), &canonical.to_string_lossy(), &kind)?;
    Ok((StatusCode::CREATED, Json(json!(lib))))
}

/// `POST /api/v1/media/station-folder`: make the station's own media folder, with a library
/// for shows, commercials, bumper clips and music inside it, so files can be uploaded straight
/// from any app. Safe to repeat; folders and libraries that exist are left alone.
pub async fn station_folder(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let (root, libs) = ensure_station_folder(&s)?;
    let made: Vec<Value> = libs
        .iter()
        .map(|l| json!({ "id": l.id, "path": l.path, "kind": l.kind }))
        .collect();
    Ok(Json(
        json!({ "root": root.to_string_lossy(), "libraries": made }),
    ))
}

/// The station's own media folder and its four libraries, made if they are not there yet.
pub(crate) fn ensure_station_folder(
    s: &AppState,
) -> ApiResult<(PathBuf, Vec<tvmaster_db::Library>)> {
    let Some(data) = s.data_dir.clone() else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "this server has no data folder to keep media in",
        ));
    };
    let mut made = Vec::new();
    for kind in KINDS {
        let dir = data.join("media").join(kind);
        std::fs::create_dir_all(&dir).map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not make {}: {e}", dir.display()),
            )
        })?;
        let canonical = dir.canonicalize().unwrap_or(dir);
        let path = canonical.to_string_lossy().to_string();
        let db = s.db();
        let existing = tvmaster_db::list_libraries(&db)?
            .into_iter()
            .find(|l| l.path == path);
        made.push(match existing {
            Some(l) => l,
            None => tvmaster_db::add_library(&db, &path, kind)?,
        });
    }
    Ok((data.join("media"), made))
}

pub async fn scan(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let ffmpeg = s
        .ffmpeg
        .clone()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "ffmpeg is not available"))?;
    let lib = tvmaster_db::get_library(&s.db(), id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;

    let root = PathBuf::from(&lib.path);
    if !root.is_dir() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "library folder is not reachable (offline?); existing items are kept",
        ));
    }

    let ffprobe = ffmpeg.ffprobe.clone();
    let lib_id = lib.id;
    let kind = lib.kind.clone();
    let (found, items) =
        tokio::task::spawn_blocking(move || probe_library(&root, &ffprobe, lib_id, &kind))
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let skipped = found - items.len();
    {
        let db = s.db();
        tvmaster_db::mark_library_missing(&db, lib.id)?;
        for m in &items {
            tvmaster_db::upsert_media(&db, m)?;
        }
    }

    // New folders may need their names tidied in lineups before the plan is redone.
    let _ = tvmaster_db::normalize_names(&s.db());
    // Channels on autopilot pick up new and removed files right away.
    {
        let db = s.db();
        let now = crate::plan::now_ms();
        for c in tvmaster_db::list_channels(&db)?
            .iter()
            .filter(|c| c.library_ids.contains(&lib.id) && c.autopilot)
        {
            crate::autopilot::reroll(&db, c, now)?;
        }
    }

    // Find ad-break points in the background so scans stay quick.
    let analysing = if lib.kind == "shows" {
        start_analysis(&s, lib.id)
    } else {
        0
    };

    Ok(Json(
        json!({ "found": found, "indexed": items.len(), "skipped": skipped, "analysing": analysing }),
    ))
}

/// Where an uploaded file goes: `rel` inside `root`, never outside it. Folders
/// in `rel` are made as needed; every part must be a plain name.
fn upload_target(root: &FsPath, rel: &str, kind: &str) -> Result<PathBuf, &'static str> {
    if rel.is_empty() || rel.len() > 400 || rel.starts_with('/') || rel.starts_with('\\') {
        return Err("give a relative path like Season 1/Episode.mkv");
    }
    let mut out = root.to_path_buf();
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    for (i, p) in parts.iter().enumerate() {
        let bad = p.is_empty()
            || *p == "."
            || *p == ".."
            || p.starts_with('.')
            || p.len() > 200
            || p.chars()
                .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'));
        if bad {
            return Err("that path has a name that is not allowed (no .., hidden names or special characters)");
        }
        if i == parts.len() - 1 {
            let ext = FsPath::new(p)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !tvmaster_media::library_extensions(kind).contains(&ext.as_str()) {
                return Err("that kind of file is not accepted by this library");
            }
        }
        out.push(p);
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) overwrite: bool,
}

/// `PUT /api/v1/libraries/{id}/files?path=Show/Season 1/Episode.mkv`: put a file
/// into a library's folder on the server. The body is the file, streamed to disk
/// (no size limit), then indexed so it is on the schedule without a scan.
pub async fn upload_file(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    axum::extract::Query(q): axum::extract::Query<FileQuery>,
    body: axum::body::Body,
) -> ApiResult<Json<Value>> {
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;
    let lib = tvmaster_db::get_library(&s.db(), id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;
    let root = PathBuf::from(&lib.path);
    if !root.is_dir() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "the library folder is not reachable",
        ));
    }
    let dest = upload_target(&root, &q.path, &lib.kind)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    if dest.exists() && !q.overwrite {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "a file with that name is already there",
        ));
    }
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("cannot make the folder: {e}"),
            )
        })?;
    }
    let part = dest.with_file_name(format!(
        ".{}.part",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload")
    ));
    let io = |e: std::io::Error| {
        ApiError::new(
            StatusCode::INSUFFICIENT_STORAGE,
            format!("could not save the file: {e}"),
        )
    };
    let mut file = tokio::fs::File::create(&part).await.map_err(io)?;
    let mut stream = body.into_data_stream();
    let mut bytes: u64 = 0;
    let result: Result<(), ApiError> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("the upload was interrupted: {e}"),
                )
            })?;
            bytes += chunk.len() as u64;
            file.write_all(&chunk).await.map_err(io)?;
        }
        file.flush().await.map_err(io)?;
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    if bytes == 0 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "the file was empty"));
    }
    tokio::fs::rename(&part, &dest).await.map_err(io)?;
    // Index it now rather than waiting for the next watch pass.
    let note = crate::monitor::rescan_with(&s, &lib, std::time::Duration::ZERO).await;
    let indexed = tvmaster_db::library_media_paths(&s.db(), lib.id)?
        .iter()
        .any(|(_, p, missing)| !missing && FsPath::new(p) == dest);
    Ok(Json(json!({
        "path": dest.to_string_lossy(),
        "bytes": bytes,
        "indexed": indexed,
        "added": note.map_or(0, |n| n.added),
    })))
}

#[derive(Deserialize)]
pub struct FetchBody {
    url: String,
    /// Where it goes inside the library (blank = the file's own name).
    #[serde(default)]
    path: String,
    #[serde(default)]
    overwrite: bool,
}

/// `POST /api/v1/libraries/{id}/fetch`: have the server download a file from a web address into a
/// library (an old commercial from an archive, say), then add it to the schedule. Only http and https.
pub async fn fetch(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<FetchBody>,
) -> ApiResult<Json<Value>> {
    let url = b.url.trim().to_string();
    let lower = url.to_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://"))
        || url.len() > 2000
        || url.chars().any(|c| c.is_control() || c == ' ')
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "give one http:// or https:// address",
        ));
    }
    let lib = tvmaster_db::get_library(&s.db(), id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;
    let root = PathBuf::from(&lib.path);
    if !root.is_dir() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "the library folder is not reachable",
        ));
    }
    let name = if b.path.trim().is_empty() {
        let tail = url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");
        let tail = percent_decode(tail);
        if tail.is_empty() {
            "download.mp4".to_string()
        } else {
            tail
        }
    } else {
        b.path.trim().to_string()
    };
    let dest = upload_target(&root, &name, &lib.kind)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    if dest.exists() && !b.overwrite {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "a file with that name is already there",
        ));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("cannot make the folder: {e}"),
            )
        })?;
    }
    let part = dest.with_extension("part");
    let out = {
        let (url, part) = (url.clone(), part.clone());
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("curl")
                .args([
                    "-fsSL",
                    "--proto",
                    "=http,https",
                    "--proto-redir",
                    "=http,https",
                    "--max-filesize",
                    "4294967296",
                    "--max-time",
                    "3600",
                    "-o",
                ])
                .arg(&part)
                .arg("--")
                .arg(&url)
                .output()
        })
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    let out = out.map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "curl is not installed on this server",
        )
    })?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&part);
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "could not download that: {}",
                if why.is_empty() {
                    "the address did not answer".to_string()
                } else {
                    why
                }
            ),
        ));
    }
    let bytes = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if bytes == 0 {
        let _ = std::fs::remove_file(&part);
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "that address sent nothing",
        ));
    }
    std::fs::rename(&part, &dest)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let note = crate::monitor::rescan_with(&s, &lib, std::time::Duration::ZERO).await;
    let indexed = tvmaster_db::library_media_paths(&s.db(), lib.id)?
        .iter()
        .any(|(_, p, missing)| !missing && FsPath::new(p) == dest);
    Ok(Json(
        json!({ "path": dest.to_string_lossy(), "bytes": bytes, "indexed": indexed, "added": note.map_or(0, |n| n.added) }),
    ))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// `DELETE /api/v1/libraries/{id}/files?path=...`: remove a file from a library's folder.
pub async fn delete_file(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    axum::extract::Query(q): axum::extract::Query<FileQuery>,
) -> ApiResult<StatusCode> {
    let lib = tvmaster_db::get_library(&s.db(), id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;
    let root = PathBuf::from(&lib.path);
    let dest = upload_target(&root, &q.path, &lib.kind)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    tokio::fs::remove_file(&dest)
        .await
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "no such file"))?;
    // Flag it missing and let channels on autopilot re-plan without it. (A folder scan would
    // refuse to act on a library that has just become empty, so this does it directly.)
    {
        let db = s.db();
        for (mid, path, missing) in tvmaster_db::library_media_paths(&db, lib.id)? {
            if !missing && FsPath::new(&path) == dest {
                tvmaster_db::mark_media_missing(&db, mid)?;
            }
        }
        let now = crate::plan::now_ms();
        for c in tvmaster_db::list_channels(&db)?
            .iter()
            .filter(|c| c.library_ids.contains(&lib.id) && c.autopilot)
        {
            crate::autopilot::reroll(&db, c, now)?;
        }
    }
    s.emit(json!({ "type": "library.changed", "library": lib.id, "added": 0, "removed": 1, "back": 0 }));
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a library and its indexed files (the files on disk are untouched).
/// Channels that used it lose it and, on autopilot, are re-rolled.
pub async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let affected: Vec<tvmaster_core::Channel> = tvmaster_db::list_channels(&s.db())?
        .into_iter()
        .filter(|c| c.library_ids.contains(&id))
        .collect();
    if !tvmaster_db::delete_library(&s.db(), id)? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such library"));
    }
    s.analysis
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&id);
    // The files are gone: plan again from now (a manual-lock channel is only topped up).
    let now = crate::plan::now_ms();
    for c in &affected {
        // Bind first: a guard in an `if let` scrutinee lives through its body.
        let fresh = tvmaster_db::get_channel(&s.db(), &c.id)?;
        if let Some(fresh) = fresh {
            if fresh.autopilot {
                crate::autopilot::rebuild(&s.db(), &fresh, now)?;
            } else {
                crate::autopilot::maintain(&s.db(), &fresh, now)?;
            }
            s.emit(json!({ "type": "schedule.changed", "channel": fresh.id }));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

fn reroll_users(s: &AppState, channels: &[tvmaster_core::Channel]) -> ApiResult<()> {
    let now = crate::plan::now_ms();
    for c in channels.iter().filter(|c| c.autopilot) {
        let fresh = tvmaster_db::get_channel(&s.db(), &c.id)?;
        if let Some(fresh) = fresh {
            crate::autopilot::reroll(&s.db(), &fresh, now)?;
            s.emit(json!({ "type": "schedule.changed", "channel": fresh.id }));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct LibraryPatch {
    enabled: bool,
}

/// Turn a library off (its files stop being scheduled) or back on.
pub async fn patch(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<LibraryPatch>,
) -> ApiResult<Json<Value>> {
    if !tvmaster_db::set_library_enabled(&s.db(), id, body.enabled)? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such library"));
    }
    let affected: Vec<tvmaster_core::Channel> = tvmaster_db::list_channels(&s.db())?
        .into_iter()
        .filter(|c| c.library_ids.contains(&id))
        .collect();
    reroll_users(&s, &affected)?;
    Ok(Json(json!({ "id": id, "enabled": body.enabled })))
}

/// Start detecting ad-break points (chapters are already known; this scans
/// for the black frames between acts). Returns how many files were queued.
pub(crate) fn start_analysis(s: &AppState, library_id: i64) -> usize {
    let Some(ffmpeg) = s.ffmpeg.clone() else {
        return 0;
    };
    let rows = match tvmaster_db::media_needing_analysis(&s.db(), library_id) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("listing files to analyse: {e}");
            return 0;
        }
    };
    {
        let mut progress = s.analysis.lock().unwrap_or_else(|p| p.into_inner());
        if rows.is_empty() || progress.get(&library_id).is_some_and(|p| p.running) {
            return 0;
        }
        progress.insert(
            library_id,
            Progress {
                total: rows.len(),
                done: 0,
                running: true,
            },
        );
    }
    let queued = rows.len();
    let state = s.clone();
    tokio::task::spawn_blocking(move || {
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(row) = rows.get(i) else { break };
                    let info = match analyze_breaks(
                        &ffmpeg.ffmpeg,
                        FsPath::new(&row.path),
                        row.duration_ms.max(0) as u64,
                        &[],
                    ) {
                        Ok(info) => info,
                        Err(e) => {
                            tracing::warn!(file = %row.path, "break detection failed: {e:#}");
                            BreakInfo {
                                src: "none".into(),
                                points: vec![],
                            }
                        }
                    };
                    if let Err(e) = tvmaster_db::set_breaks(&state.db(), row.id, &info.to_json()) {
                        tracing::error!("saving breaks: {e}");
                    }
                    if let Some(p) = state
                        .analysis
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get_mut(&library_id)
                    {
                        p.done += 1;
                    }
                });
            }
        });
        if let Some(p) = state
            .analysis
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_mut(&library_id)
        {
            p.running = false;
        }
        // Channels with commercials pick up the new break points.
        let db = state.db();
        let now = crate::plan::now_ms();
        if let Ok(channels) = tvmaster_db::list_channels(&db) {
            for c in channels.iter().filter(|c| {
                c.library_ids.contains(&library_id) && c.autopilot && c.lineup.ads.is_some()
            }) {
                let _ = crate::autopilot::reroll(&db, c, now);
            }
        }
        tracing::info!(library_id, "break detection finished");
    });
    queued
}

/// Run (or re-run) break detection for a library's files that have none yet.
pub async fn analyze(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let lib = tvmaster_db::get_library(&s.db(), id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;
    if lib.kind != "shows" {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "only shows libraries have ad breaks",
        ));
    }
    if s.ffmpeg.is_none() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available",
        ));
    }
    let queued = start_analysis(&s, id);
    Ok((StatusCode::ACCEPTED, Json(json!({ "queued": queued }))))
}

/// Find and probe every video under `root`. Returns (files found, probed items).
fn probe_library(
    root: &FsPath,
    ffprobe: &FsPath,
    library_id: i64,
    kind: &str,
) -> (usize, Vec<NewMedia>) {
    let files = list_files(root, kind);
    probe_files(root, &files, ffprobe, library_id, kind)
}

/// The media files under `root` for a library of this kind.
pub(crate) fn list_files(root: &FsPath, kind: &str) -> Vec<PathBuf> {
    if kind == "music" {
        find_audio(root)
    } else {
        find_videos(root)
    }
}

/// Probe these files. Returns (files given, probed items).
pub(crate) fn probe_files(
    root: &FsPath,
    files: &[PathBuf],
    ffprobe: &FsPath,
    library_id: i64,
    kind: &str,
) -> (usize, Vec<NewMedia>) {
    let music = kind == "music";
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, NewMedia)>> = Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for _ in 0..PROBE_THREADS {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else { break };
                let info = match probe_with(ffprobe, file) {
                    Ok(info)
                        if info.duration_ms > 0
                            && (info.video_codec.is_some()
                                || (music && info.audio_codec.is_some())) =>
                    {
                        info
                    }
                    Ok(_) => {
                        tracing::warn!(file = %file.display(), "skipping: no video or zero length");
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(file = %file.display(), "skipping: {e:#}");
                        continue;
                    }
                };
                let name = parse_name(root, file);
                let media = NewMedia {
                    library_id,
                    path: file.to_string_lossy().to_string(),
                    show: name.show,
                    season: name.season.map(|v| v as i64),
                    episode: name.episode.map(|v| v as i64),
                    title: Some(name.title).filter(|t| !t.is_empty()),
                    duration_ms: info.duration_ms as i64,
                    width: info.width.map(|v| v as i64),
                    height: info.height.map(|v| v as i64),
                    vcodec: info.video_codec,
                    acodec: info.audio_codec,
                    year: parse_year(root, file).map(|y| y as i64),
                    // Chapter markers are free, so they are read now; black-frame
                    // detection is slow and runs in the background afterwards.
                    breaks_json: (kind == "shows")
                        .then(|| from_chapters(&info.chapters, info.duration_ms))
                        .flatten()
                        .map(|b| b.to_json()),
                    // Audio and subtitle tracks, and subtitle files beside the video.
                    tracks_json: (kind == "shows").then(|| {
                        let mut t = info.tracks.clone();
                        t.external = tvmaster_media::find_sidecar_subs(file);
                        t.to_json()
                    }),
                };
                results.lock().unwrap().push((i, media));
            });
        }
    });

    let mut v = results.into_inner().unwrap();
    v.sort_by_key(|(i, _)| *i);
    (files.len(), v.into_iter().map(|(_, m)| m).collect())
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;
    use tvmaster_db::{add_library, upsert_media, NewMedia};

    fn seeded() -> (AppState, i64, i64) {
        let state = AppState::in_memory();
        let (a, b) = {
            let db = state.db();
            let a = add_library(&db, "/a", "shows").unwrap();
            let b = add_library(&db, "/b", "shows").unwrap();
            for (lib, show) in [(a.id, "Alpha"), (b.id, "Beta")] {
                upsert_media(
                    &db,
                    &NewMedia {
                        library_id: lib,
                        path: format!("/{show}/1.mp4"),
                        show: Some(show.into()),
                        duration_ms: 20 * 60_000,
                        acodec: Some("aac".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            (a.id, b.id)
        };
        (state, a, b)
    }

    #[tokio::test]
    async fn disabling_and_removing_libraries_updates_channels() {
        let (state, a, b) = seeded();
        let app = router(state.clone());
        let body = format!(
            r#"{{"id":"c","number":1,"name":"C","strategy":"sequential","library_ids":[{a},{b}]}}"#
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let shows = |app: axum::Router| async move {
            let (_, g) = call(&app, "GET", "/api/v1/channels/c/schedule?hours=2", "").await;
            g.as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p["show"].as_str().map(str::to_string))
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(shows(app.clone()).await.len(), 2);

        // turning a library off takes its shows off the air (after the locked slot)
        let (st, _) = call(
            &app,
            "PATCH",
            &format!("/api/v1/libraries/{b}"),
            r#"{"enabled":false}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, items) = call(
            &app,
            "GET",
            "/api/v1/channels/c/schedule?hours=3&detail=items",
            "",
        )
        .await;
        let last = items.as_array().unwrap().last().unwrap();
        assert_ne!(last["title"], "Beta", "{last}");
        call(
            &app,
            "PATCH",
            &format!("/api/v1/libraries/{b}"),
            r#"{"enabled":true}"#,
        )
        .await;

        // removing deletes it, drops it from the channel, and 404s afterwards
        assert_eq!(
            call(&app, "DELETE", &format!("/api/v1/libraries/{a}"), "")
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "DELETE", &format!("/api/v1/libraries/{a}"), "")
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let (_, ch) = call(&app, "GET", "/api/v1/channels/c", "").await;
        assert_eq!(ch["library_ids"], serde_json::json!([b]));
        assert_eq!(
            call(
                &app,
                "PATCH",
                "/api/v1/libraries/999",
                r#"{"enabled":true}"#
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn tvs_can_be_removed() {
        let app = router(AppState::in_memory());
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        assert_eq!(
            call(&app, "DELETE", "/api/v1/tvs/den", "").await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "DELETE", "/api/v1/tvs/den", "").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&app, "GET", "/api/v1/tvs/den", "").await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn uploads_cannot_leave_the_library_folder_or_bring_the_wrong_kind_of_file() {
        let root = std::path::Path::new("/media/bumpers");
        let ok = |p: &str, k: &str| super::upload_target(root, p, k);
        assert_eq!(
            ok("Retro/Bump 1.mp4", "bumpers").unwrap(),
            root.join("Retro").join("Bump 1.mp4")
        );
        assert!(ok("../etc/passwd.mp4", "bumpers").is_err());
        assert!(ok("a/../../b.mp4", "bumpers").is_err());
        assert!(ok("/abs/file.mp4", "bumpers").is_err());
        assert!(ok("..\\x.mp4", "bumpers").is_err());
        assert!(ok(".hidden.mp4", "bumpers").is_err());
        assert!(ok("a//b.mp4", "bumpers").is_err());
        assert!(ok("bad:name.mp4", "bumpers").is_err());
        assert!(ok("script.sh", "bumpers").is_err());
        assert!(
            ok("clip.mp3", "bumpers").is_err(),
            "audio is for music libraries"
        );
        assert!(ok("song.mp3", "music").is_ok());
        assert!(
            ok("Show S01E01.srt", "shows").is_ok(),
            "subtitles go beside shows"
        );
        assert!(ok("Show S01E01.srt", "commercials").is_err());
        assert!(ok("", "shows").is_err());
    }

    #[tokio::test]
    async fn the_station_media_folder_makes_a_library_for_each_kind_and_is_safe_to_repeat() {
        use crate::test_util::call;
        let dir = std::env::temp_dir().join(format!("tvm-station-{}", crate::generate_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::in_memory().with_data_dir(&dir);
        let app = router(state);
        let (st, v) = call(&app, "POST", "/api/v1/media/station-folder", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["libraries"].as_array().unwrap().len(), 4);
        for k in ["shows", "commercials", "bumpers", "music"] {
            assert!(dir.join("media").join(k).is_dir(), "{k} folder");
        }
        let (_, again) = call(&app, "POST", "/api/v1/media/station-folder", "").await;
        assert_eq!(
            again["libraries"], v["libraries"],
            "the same libraries, none added twice"
        );
        let (_, libs) = call(&app, "GET", "/api/v1/libraries", "").await;
        assert_eq!(libs.as_array().unwrap().len(), 4);
        // No data folder, no station folder.
        let (st, _) = call(
            &router(AppState::in_memory()),
            "POST",
            "/api/v1/media/station-folder",
            "",
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_web_addresses_can_be_fetched_into_a_library() {
        use crate::test_util::call;
        let dir = std::env::temp_dir().join(format!("tvm-fetch-{}", crate::generate_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::in_memory().with_data_dir(&dir);
        let app = router(state);
        let (_, v) = call(&app, "POST", "/api/v1/media/station-folder", "").await;
        let id = v["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["kind"] == "commercials")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/a.mp4",
            "javascript:alert(1)",
            "http://exa mple.com/a.mp4",
            "",
        ] {
            let body = serde_json::json!({ "url": url }).to_string();
            let (st, _) = call(
                &app,
                "POST",
                &format!("/api/v1/libraries/{id}/fetch"),
                &body,
            )
            .await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{url}");
        }
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/libraries/999/fetch",
            r#"{"url":"http://example.com/a.mp4"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_video_can_be_uploaded_into_a_library_and_is_on_the_schedule_at_once() {
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let Ok(info) = tvmaster_media::FfmpegInfo::detect("ffmpeg", "ffprobe") else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("tvm-up-{}", crate::generate_token()));
        let video = dir.join("src.mp4");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=10:duration=2",
                "-pix_fmt",
                "yuv420p"
            ])
            .arg(&video)
            .status()
            .unwrap()
            .success());
        let bytes = std::fs::read(&video).unwrap();
        let state = AppState::new(tvmaster_db::open_in_memory().unwrap(), Some(info), None);
        let app = router(state.clone());
        let lib_dir = dir.join("bumpers");
        let (st, lib) = call(
            &app,
            "POST",
            "/api/v1/libraries",
            &format!(
                r#"{{"path":"{}","kind":"bumpers","create":true}}"#,
                lib_dir.display()
            ),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{lib}");
        assert!(lib_dir.is_dir(), "the folder was made");
        let id = lib["id"].as_i64().unwrap();

        let put = |uri: String, body: Vec<u8>, method: &'static str| {
            let app = app.clone();
            async move {
                let req = axum::http::Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(axum::body::Body::from(body))
                    .unwrap();
                let res = app.oneshot(req).await.unwrap();
                let st = res.status();
                let b = res.into_body().collect().await.unwrap().to_bytes();
                (
                    st,
                    serde_json::from_slice::<serde_json::Value>(&b).unwrap_or_default(),
                )
            }
        };
        let (st, r) = put(
            format!("/api/v1/libraries/{id}/files?path=Retro/Bump%201.mp4"),
            bytes.clone(),
            "PUT",
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{r}");
        assert_eq!(r["indexed"], true, "{r}");
        assert_eq!(
            std::fs::read(lib_dir.join("Retro/Bump 1.mp4")).unwrap(),
            bytes,
            "stored exactly as sent"
        );
        assert!(
            !lib_dir.join("Retro/.Bump 1.mp4.part").exists(),
            "no half-written file left"
        );
        let (_, libs) = call(&app, "GET", "/api/v1/libraries", "").await;
        assert_eq!(libs[0]["items"], 1);

        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=Retro/Bump%201.mp4"),
                bytes.clone(),
                "PUT"
            )
            .await
            .0,
            StatusCode::CONFLICT,
            "not silently replaced"
        );
        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=Retro/Bump%201.mp4&overwrite=true"),
                bytes.clone(),
                "PUT"
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=..%2Fescape.mp4"),
                bytes.clone(),
                "PUT"
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=x.exe"),
                bytes.clone(),
                "PUT"
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=empty.mp4"),
                vec![],
                "PUT"
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert!(!dir.join("escape.mp4").exists());
        assert_eq!(
            put(
                "/api/v1/libraries/999/files?path=a.mp4".into(),
                bytes,
                "PUT"
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );

        assert_eq!(
            put(
                format!("/api/v1/libraries/{id}/files?path=Retro/Bump%201.mp4"),
                vec![],
                "DELETE"
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        let (_, libs) = call(&app, "GET", "/api/v1/libraries", "").await;
        assert_eq!(
            libs[0]["items"], 0,
            "removing the file takes it off the schedule too"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
