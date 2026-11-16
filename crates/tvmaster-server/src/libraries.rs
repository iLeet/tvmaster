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
    let music = kind == "music";
    let files = if music {
        find_audio(root)
    } else {
        find_videos(root)
    };
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
}
