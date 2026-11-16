//! HTTP API: libraries, channels, Virtual TVs/remote, HLS streams, M3U/XMLTV
//! and the web player. See docs/API.md.

mod admin;
mod assets;
mod auth;
mod autopilot;
mod channels;
mod guide_preview;
mod hdhr;
mod iptv;
mod libraries;
mod live;
mod monitor;
mod pip;
mod plan;
#[cfg(test)]
mod soak;
mod station;
mod stream;
pub mod tls;
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
    /// When set, every request except the UI shell must present it.
    pub(crate) token: Arc<std::sync::RwLock<Option<String>>>,
    /// True when the token came from the command line, so the UI may not change it.
    pub(crate) token_locked: bool,
    /// With a token on, anyone may still watch (streams, playlists, the player); only managing needs the token.
    pub(crate) open_viewing: Arc<std::sync::atomic::AtomicBool>,
    /// JSON events pushed to WebSocket clients.
    pub(crate) events: broadcast::Sender<String>,
    /// How many connected players each TV has (online while > 0).
    presence: Arc<Mutex<HashMap<String, usize>>>,
    /// Which ports and certificate the listeners use, for `/api/v1/system`.
    network: Arc<NetworkInfo>,
    /// Where uploaded pictures are kept (`None` = uploads are off).
    pub(crate) assets_dir: Option<std::path::PathBuf>,
    /// The server's data folder (database, caches, uploads), for the UI to suggest where media can live.
    pub(crate) data_dir: Option<std::path::PathBuf>,
    /// Problems being reported and what the folder watcher last saw.
    pub(crate) monitor: Arc<Mutex<monitor::Book>>,
    /// What the operator has put on each channel in place of its programming.
    pub(crate) live: Arc<Mutex<live::Book>>,
}

/// What the daemon is listening on, so clients can build links to it.
#[derive(Debug, Clone, Default)]
pub struct NetworkInfo {
    pub http_port: Option<u16>,
    pub https_port: Option<u16>,
    /// SHA-256 of the certificate on the HTTPS port.
    pub tls_fingerprint: Option<String>,
    pub tls_self_signed: bool,
}

/// Set by the HTTPS listener (and removed on the HTTP one) so handlers that
/// write links know which scheme the client used.
pub(crate) const SCHEME_HEADER: &str = "x-tvmaster-scheme";

/// `https` when the request came in over TLS, directly or through a reverse
/// proxy that says so; otherwise `http`.
pub(crate) fn request_scheme(headers: &axum::http::HeaderMap) -> &'static str {
    let is = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("https"))
    };
    if is(SCHEME_HEADER) || is("x-forwarded-proto") {
        "https"
    } else {
        "http"
    }
}

/// How often the autopilot loop tops up schedules and commits aired episodes.
const AUTOPILOT_INTERVAL: Duration = Duration::from_secs(60);
/// How often files flagged missing are looked for again (an unplugged drive
/// or a rebooted NAS that has come back).
const REVIVE_INTERVAL: Duration = Duration::from_secs(120);

impl AppState {
    pub fn new(
        conn: Connection,
        ffmpeg: Option<FfmpegInfo>,
        playout: Option<Arc<Manager>>,
    ) -> Self {
        let live = live::load(&conn);
        Self {
            live: Arc::new(Mutex::new(live)),
            db: Arc::new(Mutex::new(conn)),
            playout,
            ffmpeg: ffmpeg.map(Arc::new),
            dirty: Arc::new(Mutex::new(HashSet::new())),
            wake: Arc::new(Notify::new()),
            analysis: Arc::new(Mutex::new(HashMap::new())),
            token: Arc::new(std::sync::RwLock::new(None)),
            token_locked: false,
            open_viewing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            events: broadcast::channel(256).0,
            presence: Arc::new(Mutex::new(HashMap::new())),
            network: Arc::new(NetworkInfo::default()),
            assets_dir: None,
            data_dir: None,
            monitor: Arc::new(Mutex::new(monitor::Book::default())),
        }
    }

    /// The data folder, reported to the UI.
    pub fn with_data_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    /// Keep uploaded pictures in this folder.
    pub fn with_assets_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.assets_dir = Some(dir.into());
        self
    }

    pub fn with_network(mut self, network: NetworkInfo) -> Self {
        self.network = Arc::new(network);
        self
    }

    /// Require this token on every request (see [`auth`]). Must be 8-128
    /// characters of letters, digits, `-`, `_`, `.` or `~`. A token given here
    /// is fixed (the UI cannot change it); with `None` the token the admin UI
    /// saved earlier, if any, is used and can be changed at runtime.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token_locked = token.is_some();
        let saved = if token.is_none() {
            tvmaster_db::meta_get(&self.db(), auth::META_KEY)
                .ok()
                .flatten()
                .filter(|t| auth::valid_token(t))
        } else {
            None
        };
        *self.token.write().unwrap_or_else(|p| p.into_inner()) = token.or(saved);
        let open = tvmaster_db::meta_get(&self.db(), auth::OPEN_VIEWING_KEY)
            .ok()
            .flatten()
            .is_some_and(|v| v == "1");
        self.open_viewing
            .store(open, std::sync::atomic::Ordering::Relaxed);
        self
    }

    /// Use `token` the first time only: when the admin UI has never saved a
    /// choice (on or off). Lets an installer start the service protected while
    /// the owner can still turn it off or replace it later.
    pub fn seed_token(&self, token: Option<String>) {
        let Some(t) = token.filter(|t| auth::valid_token(t)) else {
            return;
        };
        if self.token_locked
            || tvmaster_db::meta_get(&self.db(), auth::META_KEY)
                .ok()
                .flatten()
                .is_some()
        {
            return;
        }
        let _ = tvmaster_db::meta_set(&self.db(), auth::META_KEY, &t);
        *self.token.write().unwrap_or_else(|p| p.into_inner()) = Some(t);
    }

    /// The access token in force right now, if authentication is on.
    pub fn access_token(&self) -> Option<String> {
        self.token()
    }

    pub(crate) fn token(&self) -> Option<String> {
        self.token.read().unwrap_or_else(|p| p.into_inner()).clone()
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
            config: channel.guide.clone(),
            pip_host: (channel.guide.pip.mode != tvmaster_core::PipMode::Off).then(|| {
                Arc::new(pip::LocalChannels(self.clone())) as Arc<dyn tvmaster_playout::PipHost>
            }),
        });
        ChannelSpec {
            guide,
            id: channel.id.clone(),
            name: channel.name.clone(),
            number: channel.number,
            options: channel.options.clone(),
            can_burn_text: self.ffmpeg.as_ref().is_some_and(|f| f.has_subtitles),
            signoff: channel
                .lineup
                .signoff
                .as_ref()
                .map(|s| s.style)
                .unwrap_or_default(),
            always_on: channel.always_on,
            source: Arc::new(plan::DbSchedule {
                db: self.db.clone(),
                dirty: self.dirty.clone(),
                wake: self.wake.clone(),
                live: self.live.clone(),
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
        // (Two database guards in one expression would wait on each other forever.)
        let zone = station::effective_timezone(&state.db());
        autopilot::fill_blank_timezones(&state.db(), &zone);
        // A timed override ends by itself.
        let ticker = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let s = ticker.clone();
                let _ = tokio::task::spawn_blocking(move || live::expire(&s)).await;
            }
        });
        tokio::spawn(async move {
            let mut last_revive = std::time::Instant::now();
            loop {
                let mut dirty: Vec<String> = state
                    .dirty
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .drain()
                    .collect();
                let revive = last_revive.elapsed() >= REVIVE_INTERVAL;
                if revive {
                    last_revive = std::time::Instant::now();
                }
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let db = s.db();
                    if revive {
                        dirty.extend(autopilot::revive(&db));
                    }
                    autopilot::tick(&db, &dirty, plan::now_ms());
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

pub use auth::{generate_token, saved_access, valid_token};

/// The plain-HTTP router.
pub fn router(state: AppState) -> Router {
    build_router(state, false)
}

/// The same router for the HTTPS listener: links it writes say `https`.
pub fn router_https(state: AppState) -> Router {
    build_router(state, true)
}

fn build_router(state: AppState, https: bool) -> Router {
    let guard_state = state.clone();
    Router::new()
        .route("/", get(|| async { Html(PLAYER_HTML) }))
        .route("/api/v1/health", get(health))
        .route(
            "/api/v1/libraries",
            get(libraries::list).post(libraries::add),
        )
        .route(
            "/api/v1/libraries/:id",
            axum::routing::delete(libraries::remove).patch(libraries::patch),
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
        .route("/api/v1/tvs/:id", get(tvs::get).delete(tvs::remove))
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
        .route("/admin", get(admin::redirect))
        .route("/admin/", get(admin::index))
        .route("/admin/*path", get(admin::asset))
        .route("/api/v1/auth", get(auth::status))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route(
            "/api/v1/security",
            get(auth::security).put(auth::put_security),
        )
        .route("/api/v1/fs/list", get(admin::fs_list))
        .route("/api/v1/guide/preview", post(guide_preview::preview))
        .route(
            "/api/v1/libraries/:id/files",
            axum::routing::put(libraries::upload_file)
                .delete(libraries::delete_file)
                .layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route(
            "/api/v1/timezone",
            get(station::get_timezone).put(station::put_timezone),
        )
        .route("/api/v1/server/restart", post(station::server_restart))
        .route("/api/v1/live", get(live::get_live).post(live::set_live))
        .route("/api/v1/live/clear", post(live::clear_live))
        .route("/api/v1/channels/:id/reset", post(station::reset))
        .route("/api/v1/channels/:id/stream", post(station::stream_control))
        .route("/api/v1/monitor", get(monitor::get).put(monitor::put))
        .route("/api/v1/monitor/test", post(monitor::test))
        .route("/api/v1/monitor/rescan", post(monitor::rescan_now))
        .route(
            "/api/v1/assets",
            get(assets::list)
                .post(assets::upload)
                .layer(axum::extract::DefaultBodyLimit::max(
                    assets::MAX_BYTES + 1024,
                )),
        )
        .route(
            "/api/v1/assets/:id",
            get(assets::get_one).delete(assets::delete_one),
        )
        .route("/api/v1/shows", get(admin::shows))
        .route(
            "/api/v1/show-settings",
            get(admin::show_settings).put(admin::put_show_settings),
        )
        .route("/api/v1/system", get(admin::system))
        .route("/stream/:channel/:file", get(stream::hls))
        .route("/iptv/playlist.m3u", get(iptv::playlist))
        .route("/iptv/guide.xml", get(iptv::guide))
        .layer(axum::middleware::from_fn_with_state(
            guard_state,
            auth::guard,
        ))
        // Outermost: decide the scheme here, never trust what a client sent.
        .layer(axum::middleware::from_fn(
            move |mut req: axum::extract::Request, next: axum::middleware::Next| async move {
                req.headers_mut().remove(SCHEME_HEADER);
                // HTTP/2 (every browser over HTTPS) carries the host as the URI's
                // authority, not a Host header; handlers that write links read the header.
                if !req.headers().contains_key(axum::http::header::HOST) {
                    let authority = req
                        .uri()
                        .authority()
                        .and_then(|a| axum::http::HeaderValue::from_str(a.as_str()).ok());
                    if let Some(a) = authority {
                        req.headers_mut().insert(axum::http::header::HOST, a);
                    }
                }
                if https {
                    req.headers_mut()
                        .insert(SCHEME_HEADER, axum::http::HeaderValue::from_static("https"));
                }
                next.run(req).await
            },
        ))
        .with_state(state)
}

async fn health(axum::extract::State(s): axum::extract::State<AppState>) -> Json<Value> {
    let ffmpeg = s.ffmpeg.as_ref().map(|f| {
        json!({
            "version": f.version,
            "encoder": s.playout.as_ref().map(|p| p.config().encoder.name()),
        })
    });
    let on_air = s.playout.as_ref().map(|p| p.running()).unwrap_or_default();
    Json(json!({
        "status": "ok",
        // So apps scanning the network can tell a TVMaster server from anything else on port 8787.
        "service": "tvmaster",
        "name": gethostname::gethostname().to_string_lossy(),
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

    #[tokio::test]
    async fn the_player_works_offline_and_on_a_tv_remote() {
        let app = router(AppState::in_memory());
        let (_, html) = call_raw(&app, "GET", "/", "").await;
        assert!(
            html.contains("/admin/vendor/hls.min.js"),
            "hls.js is served by the server, not a CDN"
        );
        assert!(
            !html.contains("cdnjs"),
            "a TV on a network without internet still plays"
        );
        for needle in [
            "ChannelUp",
            "427",
            "428",
            "Web0S",
            "Tizen",
            "ColorF0Red",
            "GoBack",
            "10009",
            "461",
        ] {
            assert!(html.contains(needle), "remote support for {needle}");
        }
        let (status, js) = call_raw(&app, "GET", "/admin/vendor/hls.min.js", "").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            js.len() > 300_000 && js.contains("Hls"),
            "the bundled player library"
        );
    }
}
