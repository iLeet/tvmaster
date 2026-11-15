//! HTTP API: libraries, channels, Virtual TVs/remote, HLS streams, M3U/XMLTV
//! and the web player. See docs/API.md.

mod channels;
mod iptv;
mod libraries;
mod plan;
mod stream;
mod tvs;

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard};
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::Manager;

const PLAYER_HTML: &str = include_str!("player.html");

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    pub(crate) playout: Option<Arc<Manager>>,
    pub(crate) ffmpeg: Option<Arc<FfmpegInfo>>,
}

impl AppState {
    pub fn new(
        conn: Connection,
        ffmpeg: Option<FfmpegInfo>,
        playout: Option<Arc<Manager>>,
    ) -> Self {
        Self {
            db: Arc::new(Mutex::new(conn)),
            playout,
            ffmpeg: ffmpeg.map(Arc::new),
        }
    }

    /// In-memory database, no FFmpeg. For tests.
    pub fn in_memory() -> Self {
        Self::new(
            tvmaster_db::open_in_memory().expect("in-memory db"),
            None,
            None,
        )
    }

    pub(crate) fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Put `always_on` channels on air (call once at startup).
    pub fn start_always_on(&self) {
        let Some(playout) = &self.playout else { return };
        let ids: Vec<String> = match tvmaster_db::list_channels(&self.db()) {
            Ok(c) => c
                .into_iter()
                .filter(|c| c.always_on)
                .map(|c| c.id)
                .collect(),
            Err(e) => {
                tracing::error!("listing channels: {e}");
                return;
            }
        };
        for id in ids {
            match plan::build_spec(&self.db(), &id) {
                Ok(Some(spec)) => {
                    playout.start_if_absent(spec);
                }
                Ok(None) => tracing::warn!(channel = %id, "always-on channel has nothing to play"),
                Err(e) => tracing::error!(channel = %id, "{e:?}"),
            }
        }
    }
}

/// JSON error response.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!("database error: {e}");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { Html(PLAYER_HTML) }))
        .route("/api/v1/health", get(health))
        .route(
            "/api/v1/libraries",
            get(libraries::list).post(libraries::add),
        )
        .route("/api/v1/libraries/:id/scan", post(libraries::scan))
        .route(
            "/api/v1/channels",
            get(channels::list).post(channels::upsert),
        )
        .route(
            "/api/v1/channels/:id",
            get(channels::get).delete(channels::delete),
        )
        .route("/api/v1/channels/:id/schedule", get(channels::schedule))
        .route("/api/v1/tvs", get(tvs::list).post(tvs::register))
        .route("/api/v1/tvs/:id", get(tvs::get))
        .route("/api/v1/tvs/:id/channel", post(tvs::set_channel))
        .route("/api/v1/tvs/:id/power", post(tvs::set_power))
        .route("/stream/:channel/:file", get(stream::hls))
        .route("/iptv/playlist.m3u", get(iptv::playlist))
        .route("/iptv/guide.xml", get(iptv::guide))
        .with_state(state)
}

async fn health(axum::extract::State(s): axum::extract::State<AppState>) -> Json<Value> {
    let ffmpeg = s.ffmpeg.as_ref().map(|f| {
        json!({
            "version": f.version,
            "encoder": s.playout.as_ref().map(|p| format!("{:?}", p.config().encoder)),
        })
    });
    let on_air = s.playout.as_ref().map(|p| p.running()).unwrap_or_default();
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "ffmpeg": ffmpeg,
        "on_air": on_air,
    }))
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    pub async fn call_raw(
        app: &Router,
        method: &str,
        uri: &str,
        body: &str,
    ) -> (StatusCode, String) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "tv.local:8787")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    pub async fn call(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
        let (status, text) = call_raw(app, method, uri, body).await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;

    #[tokio::test]
    async fn health_is_ok() {
        let app = router(AppState::in_memory());
        let (status, body) = call(&app, "GET", "/api/v1/health", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["ffmpeg"], Value::Null);
    }

    #[tokio::test]
    async fn root_serves_player() {
        let app = router(AppState::in_memory());
        let (status, html) = call_raw(&app, "GET", "/", "").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("hls.js"));
    }
}
