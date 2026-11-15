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
use tvmaster_media::{find_videos, parse_name, probe_with};

const KINDS: &[&str] = &["shows", "commercials", "bumpers", "music"];
const PROBE_THREADS: usize = 4;

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let db = s.db();
    let libs = tvmaster_db::list_libraries(&db)?;
    let out: Vec<Value> = libs
        .into_iter()
        .map(|l| {
            let items = tvmaster_db::count_media(&db, l.id).unwrap_or(0);
            json!({ "id": l.id, "path": l.path, "kind": l.kind, "enabled": l.enabled, "items": items })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct NewLibrary {
    path: String,
    kind: Option<String>,
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
    let (found, items) =
        tokio::task::spawn_blocking(move || probe_library(&root, &ffprobe, lib_id))
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

    Ok(Json(
        json!({ "found": found, "indexed": items.len(), "skipped": skipped }),
    ))
}

/// Find and probe every video under `root`. Returns (files found, probed items).
fn probe_library(root: &FsPath, ffprobe: &FsPath, library_id: i64) -> (usize, Vec<NewMedia>) {
    let files = find_videos(root);
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, NewMedia)>> = Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for _ in 0..PROBE_THREADS {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else { break };
                let info = match probe_with(ffprobe, file) {
                    Ok(info) if info.duration_ms > 0 && info.video_codec.is_some() => info,
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
                };
                results.lock().unwrap().push((i, media));
            });
        }
    });

    let mut v = results.into_inner().unwrap();
    v.sort_by_key(|(i, _)| *i);
    (files.len(), v.into_iter().map(|(_, m)| m).collect())
}
