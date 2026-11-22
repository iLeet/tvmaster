//! The admin UI (static files compiled into the binary) and the small APIs it
//! needs: a folder picker, the list of shows, and system information.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The built web app. In debug builds files are read from `ui/` at run time
/// (so a rebuilt UI shows up without recompiling); release builds embed them.
#[derive(RustEmbed)]
#[folder = "ui/"]
struct Ui;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve(path: &str) -> Option<Response> {
    let file = Ui::get(path)?;
    let mut res = file.data.into_owned().into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    // Hashed bundles never change; everything else must be revalidated so an
    // upgrade (and the service worker) is picked up.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if path == "sw.js" {
        h.insert(
            "service-worker-allowed",
            HeaderValue::from_static("/admin/"),
        );
    }
    Some(res)
}

pub async fn redirect() -> Redirect {
    Redirect::permanent("/admin/")
}

pub async fn index() -> Response {
    serve("index.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// Static files, with a fallback to the app shell for client-side routes.
pub async fn asset(Path(path): Path<String>) -> Response {
    if let Some(res) = serve(&path) {
        return res;
    }
    let looks_like_file = path.rsplit('/').next().is_some_and(|n| n.contains('.'));
    if looks_like_file {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve("index.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

#[derive(Deserialize)]
pub struct FsQuery {
    path: Option<String>,
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR_STR))
}

/// Folders on the server, one level at a time, for the library picker. Only
/// directory names are returned (never files), hidden folders are skipped.
pub async fn fs_list(Query(q): Query<FsQuery>) -> ApiResult<Json<Value>> {
    let requested = q
        .path
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(home_dir);
    let dir = std::fs::canonicalize(&requested)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "that folder does not exist"))?;
    let entries = std::fs::read_dir(&dir).map_err(|e| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("cannot read that folder: {e}"),
        )
    })?;
    let mut dirs: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false)
        })
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (!name.starts_with('.')).then(|| (name, e.path().to_string_lossy().to_string()))
        })
        .collect();
    dirs.sort_by_key(|(n, _)| n.to_lowercase());
    dirs.truncate(1000);
    Ok(Json(json!({
        "path": dir.to_string_lossy(),
        "parent": dir.parent().map(|p| p.to_string_lossy().to_string()),
        "dirs": dirs.into_iter().map(|(name, path)| json!({ "name": name, "path": path })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct ShowsQuery {
    /// Comma-separated library ids; empty = every shows library.
    library_ids: Option<String>,
}

pub async fn shows(
    State(s): State<AppState>,
    Query(q): Query<ShowsQuery>,
) -> ApiResult<Json<Value>> {
    let ids: Vec<i64> = q
        .library_ids
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    let db = s.db();
    let settings = tvmaster_db::list_show_settings(&db)?;
    let info = tvmaster_db::list_show_info(&db)?;
    let mut shows = json!(tvmaster_db::list_shows(&db, &ids)?);
    // Attach each show's own settings (the ones that follow it onto every channel).
    if let Some(list) = shows.as_array_mut() {
        for row in list {
            let name = row["name"].as_str().unwrap_or("").to_lowercase();
            row["defaults"] = settings
                .iter()
                .find(|(n, _)| n.to_lowercase() == name)
                .map(|(_, o)| json!(o))
                .unwrap_or(Value::Null);
            row["info"] = info.get(&name).map(|i| json!(i)).unwrap_or(Value::Null);
        }
    }
    Ok(Json(shows))
}

#[derive(serde::Deserialize)]
pub struct ShowSettingsInput {
    show: String,
    #[serde(default)]
    settings: tvmaster_core::ShowOptions,
}

/// Every show that has settings of its own: `{ "Cowboy Bebop": {…} }`.
pub async fn show_settings(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let map: serde_json::Map<String, Value> = tvmaster_db::list_show_settings(&s.db())?
        .into_iter()
        .map(|(n, o)| (n, json!(o)))
        .collect();
    Ok(Json(Value::Object(map)))
}

/// Set (or, with empty settings, clear) a show's settings for every channel.
pub async fn put_show_settings(
    State(s): State<AppState>,
    Json(body): Json<ShowSettingsInput>,
) -> ApiResult<Json<Value>> {
    if body.show.trim().is_empty() || body.show.len() > 120 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "a show name is required",
        ));
    }
    body.settings
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("invalid settings: {e}")))?;
    tvmaster_db::set_show_settings(&s.db(), &body.show, &body.settings)?;
    Ok(Json(
        json!({ "show": body.show.trim(), "settings": body.settings }),
    ))
}

/// `GET /api/v1/ads/options`: the ready-made amounts of advertising and the cards around breaks, so
/// every app offers the same choices in the same words.
pub async fn ad_options() -> Json<Value> {
    Json(json!({
        "presets": tvmaster_core::AD_PRESETS,
        "cards": [
            { "id": "brb", "name": "We'll be right back", "about": "Opens each break." },
            { "id": "back_to", "name": "Back to the show", "about": "Closes a break inside a show." },
            { "id": "up_next", "name": "Up next", "about": "Closes the last break: what comes after this show." },
            { "id": "later_today", "name": "Later today", "about": "Now and then, a list of the next few programmes instead of \"up next\"." },
            { "id": "station_id", "name": "Station ID", "about": "The channel's name; also fills odd leftover seconds." },
        ],
    }))
}

#[derive(serde::Deserialize)]
pub struct ExplainBody {
    #[serde(default)]
    ads: tvmaster_core::AdConfig,
    #[serde(default = "d_slot")]
    slot_minutes: u32,
    #[serde(default = "d_show")]
    show_minutes: u32,
}
fn d_slot() -> u32 {
    30
}
fn d_show() -> u32 {
    22
}

/// `POST /api/v1/ads/explain`: what some break settings do to a typical slot, in plain words.
pub async fn ad_explain(Json(b): Json<ExplainBody>) -> ApiResult<Json<Value>> {
    b.ads
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let (slot, show) = (b.slot_minutes.clamp(5, 240), b.show_minutes.clamp(1, 240));
    Ok(Json(json!({
        "text": b.ads.describe(slot, show),
        "preset": b.ads.preset_id(),
    })))
}

pub async fn system(State(s): State<AppState>) -> Json<Value> {
    let ffmpeg = s.ffmpeg.as_ref().map(|f| {
        json!({
            "version": f.version,
            "encoder": s.playout.as_ref().map(|p| p.config().encoder.name()),
            "encoders": f.compiled.iter().map(|e| e.name()).collect::<Vec<_>>(),
            "has_libx264": f.has_libx264,
            "has_videotoolbox": f.has_videotoolbox,
            "has_nvenc": f.has_nvenc,
            "has_drawtext": f.has_drawtext,
            "has_subtitles": f.has_subtitles,
        })
    });
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "timezone": crate::station::effective_timezone(&s.db()),
        "ffmpeg": ffmpeg,
        "on_air": s.playout.as_ref().map(|p| p.running().into_iter().filter(|c| c != tvmaster_core::COMMERCIAL_CHANNEL).collect::<Vec<_>>()).unwrap_or_default(),
        "auth_required": s.token().is_some(),
        "data_dir": s.data_dir.as_ref().map(|d| d.to_string_lossy().to_string()),
        "network": network_json(&s.network),
    }))
}

/// Where this server can be reached, for clients that build links to it.
fn network_json(n: &crate::NetworkInfo) -> Value {
    let (hostname, ips) = crate::tls::host_names();
    let short = hostname.split('.').next().unwrap_or("").to_string();
    // Names first (they survive a DHCP change), then addresses.
    let mut hosts: Vec<String> = Vec::new();
    if !short.is_empty() {
        hosts.push(format!("{short}.local"));
    }
    hosts.extend(ips.iter().map(|i| i.to_string()));
    json!({
        "hostname": hostname,
        "hosts": hosts,
        "http_port": n.http_port,
        "https_port": n.https_port,
        "tls": {
            "enabled": n.https_port.is_some(),
            "self_signed": n.tls_self_signed,
            "fingerprint": n.tls_fingerprint,
        },
    })
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn the_admin_shell_and_client_routes_are_served() {
        let app = router(AppState::in_memory());
        let (st, html) = call_raw(&app, "GET", "/admin/", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(html.contains("<html") || html.contains("<!doctype"));
        // client-side routes fall back to the shell; missing files are 404
        let (st, again) = call_raw(&app, "GET", "/admin/channels/retro", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(again, html);
        assert_eq!(
            call_raw(&app, "GET", "/admin/assets/missing.js", "")
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        // /admin redirects to the trailing-slash form so relative URLs resolve
        let req = axum::http::Request::builder()
            .uri("/admin")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(res.headers()["location"], "/admin/");
    }

    #[tokio::test]
    async fn the_folder_picker_lists_only_visible_directories() {
        let dir = std::env::temp_dir().join(format!("tvmaster-fs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Beta")).unwrap();
        std::fs::create_dir_all(dir.join("alpha")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("file.txt"), b"x").unwrap();
        let app = router(AppState::in_memory());
        let uri = format!("/api/v1/fs/list?path={}", dir.display());
        let (st, v) = call(&app, "GET", &uri, "").await;
        assert_eq!(st, StatusCode::OK);
        let names: Vec<&str> = v["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["alpha", "Beta"], "sorted, no files, no hidden");
        assert!(v["parent"].is_string());
        // going into a child works and "parent" leads back
        let child = v["dirs"][0]["path"].as_str().unwrap();
        let (_, c) = call(&app, "GET", &format!("/api/v1/fs/list?path={child}"), "").await;
        assert_eq!(c["dirs"].as_array().unwrap().len(), 0);
        assert_eq!(
            std::path::Path::new(c["parent"].as_str().unwrap()),
            std::fs::canonicalize(&dir).unwrap()
        );
        // bad paths are a clean error; no path means the home folder
        assert_eq!(
            call(&app, "GET", "/api/v1/fs/list?path=/definitely/not/here", "")
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &app,
                "GET",
                &format!("/api/v1/fs/list?path={}/file.txt", dir.display()),
                ""
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "GET", "/api/v1/fs/list", "").await.0,
            StatusCode::OK
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn shows_and_system_info() {
        let state = AppState::in_memory();
        {
            let db = state.db();
            let lib = tvmaster_db::add_library(&db, "/tv", "shows").unwrap();
            for (i, show) in ["Cheers", "Cheers", "Taxi"].iter().enumerate() {
                tvmaster_db::upsert_media(
                    &db,
                    &tvmaster_db::NewMedia {
                        library_id: lib.id,
                        path: format!("/tv/{i}.mp4"),
                        show: Some(show.to_string()),
                        duration_ms: 60_000,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        let app = router(state);
        let (_, shows) = call(&app, "GET", "/api/v1/shows", "").await;
        assert_eq!(shows[0]["name"], "Cheers");
        assert_eq!(shows[0]["episodes"], 2);
        assert_eq!(shows[1]["name"], "Taxi");
        let (_, sys) = call(&app, "GET", "/api/v1/system", "").await;
        assert!(sys["version"].is_string() && sys["os"].is_string());
        assert_eq!(sys["auth_required"], false);
        assert_eq!(sys["ffmpeg"], serde_json::Value::Null);
    }
}

#[cfg(test)]
mod ads_tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn break_presets_and_the_plain_words_come_from_the_server() {
        let app = router(AppState::in_memory());
        let (st, v) = call(&app, "GET", "/api/v1/ads/options", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["presets"].as_array().unwrap().len(), 6);
        assert!(v["presets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "short" && p["mid_breaks"] == false));
        assert_eq!(v["cards"].as_array().unwrap().len(), 5);
        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/ads/explain",
            r#"{"ads":{},"slot_minutes":60,"show_minutes":44}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(
            v["text"].as_str().unwrap().contains("60-minute slot"),
            "{v}"
        );
        assert_eq!(v["preset"], "classic");
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/ads/explain",
            r#"{"ads":{"pod_seconds":3}}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
}
