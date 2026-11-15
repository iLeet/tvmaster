//! HTTP API: libraries, channels, Virtual TVs/remote, HLS streams, M3U/XMLTV
//! and the web player. See docs/API.md.

mod autopilot;
mod channels;
mod hdhr;
mod iptv;
mod libraries;
mod plan;
mod stream;
mod tvs;
mod ws;

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{broadcast, Notify};
use tvmaster_core::{Channel, ChannelKind, Tv};
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{ChannelSpec, GuideSpec, Manager};

const PLAYER_HTML: &str = include_str!("player.html");

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    pub(crate) playout: Option<Arc<Manager>>,
    pub(crate) ffmpeg: Option<Arc<FfmpegInfo>>,
    /// Channels whose schedule must be re-rolled (e.g. a file turned out to be unplayable).
    dirty: Arc<Mutex<HashSet<String>>>,
    /// Wakes the autopilot loop early.
    wake: Arc<Notify>,
    /// Background break-detection progress per library.
    pub(crate) analysis: Arc<Mutex<HashMap<i64, libraries::Progress>>>,
    /// JSON events pushed to WebSocket clients.
    pub(crate) events: broadcast::Sender<String>,
    /// How many connected players each TV has (online while > 0).
    presence: Arc<Mutex<HashMap<String, usize>>>,
}

/// How often the autopilot loop tops up schedules and commits aired episodes.
const AUTOPILOT_INTERVAL: Duration = Duration::from_secs(60);

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
            dirty: Arc::new(Mutex::new(HashSet::new())),
            wake: Arc::new(Notify::new()),
            analysis: Arc::new(Mutex::new(HashMap::new())),
            events: broadcast::channel(256).0,
            presence: Arc::new(Mutex::new(HashMap::new())),
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

    /// Push an event to every connected WebSocket client.
    pub(crate) fn emit(&self, event: Value) {
        let _ = self.events.send(event.to_string());
    }

    /// A TV with its live `online` flag filled in.
    pub(crate) fn tv_view(&self, mut tv: Tv) -> Tv {
        tv.online = self
            .presence
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&tv.id)
            .is_some_and(|n| *n > 0);
        tv
    }

    /// Announce a TV's new state to everyone listening.
    pub(crate) fn tv_changed(&self, tv: Tv) -> Tv {
        let tv = self.tv_view(tv);
        self.emit(json!({ "type": "tv.state", "tv": tv }));
        tv
    }

    pub(crate) fn presence_inc(&self, id: &str) {
        *self
            .presence
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(id.to_string())
            .or_insert(0) += 1;
        self.announce(id);
    }

    pub(crate) fn presence_dec(&self, id: &str) {
        {
            let mut p = self.presence.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(n) = p.get_mut(id) {
                *n = n.saturating_sub(1);
            }
        }
        self.announce(id);
    }

    fn announce(&self, id: &str) {
        if let Ok(Some(tv)) = tvmaster_db::get_tv(&self.db(), id) {
            self.tv_changed(tv);
        }
    }

    /// Watch what is on air and push `channel.now` when a channel moves on to
    /// its next programme. Needs a Tokio runtime.
    pub fn spawn_now_watcher(&self) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::spawn(async move {
            let mut last: HashMap<String, Option<(i64, i64)>> = HashMap::new();
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let now = plan::now_ms();
                let events: Vec<Value> = {
                    let db = state.db();
                    let mut out = Vec::new();
                    for ch in tvmaster_db::list_channels(&db)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|c| c.kind == ChannelKind::Programming)
                    {
                        let progs = plan::programs(&db, &ch.id, now, now + 12 * 3_600_000)
                            .unwrap_or_default();
                        let (cur, next) = plan::now_and_next(&progs, now);
                        let key = cur.as_ref().map(|p| (p.media_id, p.start_ms));
                        if last
                            .insert(ch.id.clone(), key)
                            .is_some_and(|prev| prev != key)
                        {
                            out.push(json!({ "type": "channel.now", "channel": ch.id, "now": cur, "next": next }));
                        }
                    }
                    out
                };
                for e in events {
                    state.emit(e);
                }
            }
        })
    }

    /// What playout needs to put a channel on air.
    pub(crate) fn channel_spec(&self, channel: &Channel) -> ChannelSpec {
        let guide = (channel.kind == ChannelKind::Guide).then(|| GuideSpec {
            music: tvmaster_db::list_media_of_kind(
                &self.db(),
                &channel.guide.music_library_ids,
                "music",
            )
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.path.into())
            .collect(),
        });
        ChannelSpec {
            guide,
            id: channel.id.clone(),
            name: channel.name.clone(),
            always_on: channel.always_on,
            source: Arc::new(plan::DbSchedule {
                db: self.db.clone(),
                dirty: self.dirty.clone(),
                wake: self.wake.clone(),
            }),
        }
    }

    /// Put `always_on` channels on air (call once at startup).
    pub fn start_always_on(&self) {
        let Some(playout) = &self.playout else { return };
        let channels = match tvmaster_db::list_channels(&self.db()) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("listing channels: {e}");
                return;
            }
        };
        for ch in channels.into_iter().filter(|c| c.always_on) {
            if ch.kind == ChannelKind::Programming {
                if let Err(e) = autopilot::maintain(&self.db(), &ch, plan::now_ms()) {
                    tracing::error!(channel = %ch.id, "scheduling: {e}");
                }
            }
            playout.start_if_absent(self.channel_spec(&ch));
        }
    }

    /// Start the autopilot loop: keeps every channel's schedule filled days
    /// ahead, advances the episode cursors as items air and re-rolls channels
    /// with unplayable files. Runs one pass immediately. Needs a Tokio runtime.
    pub fn spawn_autopilot(&self) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        autopilot::fill_blank_timezones(&state.db(), &channels::system_timezone());
        tokio::spawn(async move {
            loop {
                let dirty: Vec<String> = state
                    .dirty
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .drain()
                    .collect();
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    autopilot::tick(&s.db(), &dirty, plan::now_ms());
                })
                .await;
                tokio::select! {
                    _ = tokio::time::sleep(AUTOPILOT_INTERVAL) => {}
                    _ = state.wake.notified() => {}
                }
            }
        })
    }
}

/// JSON error response.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn message(&self) -> &str {
        &self.message
    }

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
        .route("/api/v1/libraries/:id/analyze", post(libraries::analyze))
        .route(
            "/api/v1/channels",
            get(channels::list).post(channels::upsert),
        )
        .route(
            "/api/v1/channels/:id",
            get(channels::get).delete(channels::delete),
        )
        .route("/api/v1/channels/:id/schedule", get(channels::schedule))
        .route(
            "/api/v1/channels/:id/lineup",
            get(channels::get_lineup).put(channels::put_lineup),
        )
        .route("/api/v1/channels/:id/reroll", post(channels::reroll))
        .route("/api/v1/channels/:id/cursors", get(channels::cursors))
        .route("/api/v1/tvs", get(tvs::list).post(tvs::register))
        .route("/api/v1/tvs/:id", get(tvs::get))
        .route("/api/v1/tvs/:id/channel", post(tvs::set_channel))
        .route("/api/v1/tvs/:id/power", post(tvs::set_power))
        .route("/api/v1/tvs/:id/volume", post(tvs::set_volume))
        .route("/api/v1/ws", get(ws::handler))
        .route("/tv/:name", get(tvs::tv_stream))
        .route("/discover.json", get(hdhr::discover))
        .route("/lineup.json", get(hdhr::lineup))
        .route("/lineup_status.json", get(hdhr::lineup_status))
        .route(
            "/lineup.post",
            post(hdhr::lineup_post).get(hdhr::lineup_post),
        )
        .route("/device.xml", get(hdhr::device_xml))
        .route("/stream/:name", get(stream::ts))
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
