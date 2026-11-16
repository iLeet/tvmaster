//! Keeping an eye on the station: re-reading watched folders so new episodes
//! appear by themselves, noticing what has gone wrong (a drive that is gone, a
//! channel that keeps crashing, FFmpeg missing) and saying so, in the app and,
//! if you set one up, to a web address (ntfy, Slack, Discord, Home Assistant, ...).

use crate::{ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path as FsPath, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tvmaster_core::ChannelKind;

const META_KEY: &str = "monitor";
/// How often conditions are checked.
const CHECK_EVERY: Duration = Duration::from_secs(30);
/// A new file must have stopped changing this long before it is read, so one
/// that is still being copied in is not indexed half-written.
const SETTLE: Duration = Duration::from_secs(60);
/// A channel that has failed this many times in ten minutes is reported.
const FAILING: usize = 3;

fn yes() -> bool {
    true
}
fn default_watch() -> u32 {
    15
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Minutes between re-reading every library's folder (0 = never, only when asked).
    #[serde(default = "default_watch")]
    pub watch_minutes: u32,
    /// Raise alerts when something goes wrong.
    #[serde(default = "yes")]
    pub alerts: bool,
    /// Also send each alert to this address (`http://` or `https://`).
    #[serde(default)]
    pub webhook: Option<String>,
    /// Send plain text instead of JSON (what ntfy wants).
    #[serde(default)]
    pub webhook_text: bool,
    /// Say when a problem has gone away.
    #[serde(default = "yes")]
    pub notify_recovery: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            watch_minutes: default_watch(),
            alerts: true,
            webhook: None,
            webhook_text: false,
            notify_recovery: true,
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.watch_minutes, 0 | 5 | 10 | 15 | 30 | 60 | 120 | 360) {
            return Err("watch_minutes must be 0, 5, 10, 15, 30, 60, 120 or 360".into());
        }
        if let Some(w) = self.webhook.as_deref().filter(|w| !w.is_empty()) {
            let ok = (w.starts_with("http://") || w.starts_with("https://"))
                && w.len() <= 500
                && !w.chars().any(|c| c.is_control() || c.is_whitespace());
            if !ok {
                return Err("the webhook must be one http:// or https:// address".into());
            }
        }
        Ok(())
    }

    fn webhook(&self) -> Option<&str> {
        self.webhook.as_deref().filter(|w| !w.is_empty())
    }
}

pub fn load(db: &rusqlite::Connection) -> Settings {
    tvmaster_db::meta_get(db, META_KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// Something that needs attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alert {
    /// Stable for one problem, so it can be raised once and cleared once.
    pub id: String,
    /// `error` or `warning`.
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
    pub since_ms: i64,
}

/// The result of the last folder check of one library.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Rescan {
    pub at_ms: i64,
    pub added: usize,
    pub removed: usize,
    pub back: usize,
}

#[derive(Default)]
pub struct Book {
    active: BTreeMap<String, Alert>,
    /// Consecutive checks each condition has held (an alert is raised on the second).
    seen: HashMap<String, u32>,
    last_rescan: HashMap<i64, Rescan>,
    last_watch: Option<Instant>,
}

// ---------- conditions ----------

fn now_ms() -> i64 {
    crate::plan::now_ms()
}

/// What is wrong right now.
fn conditions(s: &AppState) -> Vec<Alert> {
    let mut out = Vec::new();
    let since_ms = now_ms();
    let mut add = |id: String, severity, title: String, detail: String| {
        out.push(Alert {
            id,
            severity,
            title,
            detail,
            since_ms,
        });
    };
    if s.ffmpeg.is_none() {
        add(
            "system:ffmpeg".into(),
            "error",
            "FFmpeg was not found".into(),
            "Channels cannot go on air until FFmpeg is installed.".into(),
        );
    }
    let db = s.db();
    let libraries = tvmaster_db::list_libraries(&db).unwrap_or_default();
    let mut live_media: HashMap<i64, i64> = HashMap::new();
    for lib in libraries.iter().filter(|l| l.enabled) {
        if !FsPath::new(&lib.path).is_dir() {
            let name = FsPath::new(&lib.path)
                .file_name()
                .map_or_else(|| lib.path.clone(), |n| n.to_string_lossy().to_string());
            add(
                format!("library:{}:offline", lib.id),
                "error",
                format!("The “{name}” library is offline"),
                format!(
                    "{} cannot be reached (an unplugged drive or a NAS that is down?). Its files are left out of the schedule and come back by themselves.",
                    lib.path
                ),
            );
        }
        live_media.insert(lib.id, tvmaster_db::count_media(&db, lib.id).unwrap_or(0));
    }
    for ch in tvmaster_db::list_channels(&db).unwrap_or_default() {
        if ch.kind == ChannelKind::Programming && !ch.library_ids.is_empty() {
            let playable: i64 = ch
                .library_ids
                .iter()
                .filter_map(|id| live_media.get(id))
                .sum();
            if playable == 0 {
                add(
                    format!("channel:{}:empty", ch.id),
                    "warning",
                    format!("Channel {} {} has nothing to play", ch.number, ch.name),
                    "All of its libraries are empty, switched off or offline, so it shows its off-air screen.".into(),
                );
            }
        }
        if let Some(h) = s.playout.as_ref().and_then(|p| p.health(&ch.id)) {
            if h.recent_failures >= FAILING {
                add(
                    format!("channel:{}:failing", ch.id),
                    "error",
                    format!("Channel {} {} keeps failing", ch.number, ch.name),
                    format!(
                        "Its playout stopped {} times in ten minutes. Last reason: {}",
                        h.recent_failures,
                        h.last_error.unwrap_or_else(|| "unknown".into())
                    ),
                );
            }
        }
    }
    out
}

/// One monitoring pass: compare what is wrong with what was already reported,
/// raise the new and clear the gone. Returns (raised, cleared) for tests.
pub(crate) fn check(s: &AppState) -> (Vec<Alert>, Vec<Alert>) {
    let settings = load(&s.db());
    let wrong = if settings.alerts {
        conditions(s)
    } else {
        Vec::new()
    };
    let mut raised = Vec::new();
    let mut cleared = Vec::new();
    {
        let mut book = s.monitor.lock().unwrap_or_else(|p| p.into_inner());
        let ids: HashSet<&String> = wrong.iter().map(|a| &a.id).collect();
        book.seen.retain(|id, _| ids.contains(id));
        for a in wrong.iter() {
            let n = book.seen.entry(a.id.clone()).or_insert(0);
            *n += 1;
            // Twice in a row, so a channel restarting or a share remounting is not a false alarm.
            if *n >= 2 || a.id == "system:ffmpeg" {
                if !book.active.contains_key(&a.id) {
                    book.active.insert(a.id.clone(), a.clone());
                    raised.push(a.clone());
                } else if let Some(cur) = book.active.get_mut(&a.id) {
                    cur.detail = a.detail.clone();
                }
            }
        }
        let gone: Vec<String> = book
            .active
            .keys()
            .filter(|id| !ids.contains(id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(a) = book.active.remove(&id) {
                cleared.push(a);
            }
        }
    }
    for a in &raised {
        s.emit(json!({ "type": "alert.raised", "alert": a }));
        tracing::warn!(alert = %a.id, "{}: {}", a.title, a.detail);
        send(
            &settings,
            &format!("TVMaster: {}", a.title),
            &a.detail,
            a.severity,
            &a.id,
        );
    }
    for a in &cleared {
        s.emit(json!({ "type": "alert.cleared", "id": a.id }));
        tracing::info!(alert = %a.id, "resolved: {}", a.title);
        if settings.notify_recovery {
            send(
                &settings,
                &format!("TVMaster: back to normal: {}", a.title),
                "This is fine again.",
                "ok",
                &a.id,
            );
        }
    }
    (raised, cleared)
}

// ---------- telling someone ----------

/// POST to the webhook, if there is one. Uses `curl` (on every Mac and Linux
/// and on current Windows), so the server needs no HTTP client of its own.
fn send(settings: &Settings, title: &str, message: &str, severity: &str, id: &str) {
    let Some(url) = settings.webhook().map(str::to_string) else {
        return;
    };
    let clean = |t: &str| t.chars().filter(|c| !c.is_control()).collect::<String>();
    let (title, message) = (clean(title), clean(message));
    let text_mode = settings.webhook_text;
    let (severity, id) = (severity.to_string(), id.to_string());
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-sS", "-m", "15", "-X", "POST"]);
        if text_mode {
            cmd.args([
                "-H",
                &format!("Title: {title}"),
                "-H",
                "Content-Type: text/plain; charset=utf-8",
                "-d",
                &message,
            ]);
        } else {
            let body = json!({
                "title": title, "message": message, "severity": severity, "id": id,
                // The keys Slack and Discord incoming webhooks look for.
                "text": format!("{title}: {message}"), "content": format!("{title}: {message}"),
            });
            cmd.args([
                "-H",
                "Content-Type: application/json",
                "-d",
                &body.to_string(),
            ]);
        }
        cmd.arg(&url);
        match cmd.output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => tracing::warn!(
                "the alert webhook failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => tracing::warn!("could not run curl to send an alert: {e}"),
        }
    });
}

// ---------- watching folders ----------

/// Re-read one library's folder without touching what has not changed: new
/// files are probed and added, files that are gone are flagged, files that came
/// back are cleared. Returns `None` when the folder cannot be reached (nothing
/// is flagged then: an unplugged drive is not a deleted library).
pub(crate) async fn rescan(s: &AppState, lib: &tvmaster_db::Library) -> Option<Rescan> {
    let ffmpeg = s.ffmpeg.clone()?;
    let root = PathBuf::from(&lib.path);
    if !root.is_dir() {
        return None;
    }
    let kind = lib.kind.clone();
    let (root2, kind2) = (root.clone(), kind.clone());
    let files = tokio::task::spawn_blocking(move || crate::libraries::list_files(&root2, &kind2))
        .await
        .ok()?;
    // Reaching the folder but finding nothing while the library has files is a mount that came up
    // empty, not a purge: leave it alone.
    let known = tvmaster_db::library_media_paths(&s.db(), lib.id).ok()?;
    if files.is_empty() && known.iter().any(|(_, _, missing)| !missing) {
        return None;
    }
    let on_disk: HashSet<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    let known_paths: HashSet<&String> = known.iter().map(|(_, p, _)| p).collect();
    let settled = |f: &PathBuf| {
        std::fs::metadata(f)
            .and_then(|m| m.modified())
            .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() >= SETTLE)
            .unwrap_or(false)
    };
    let fresh: Vec<PathBuf> = files
        .iter()
        .filter(|f| !known_paths.contains(&f.to_string_lossy().to_string()) && settled(f))
        .cloned()
        .collect();

    let (ffprobe, id, kind3, root3) = (ffmpeg.ffprobe.clone(), lib.id, kind.clone(), root.clone());
    let items = if fresh.is_empty() {
        Vec::new()
    } else {
        tokio::task::spawn_blocking(move || {
            crate::libraries::probe_files(&root3, &fresh, &ffprobe, id, &kind3).1
        })
        .await
        .ok()?
    };
    let (mut removed, mut back) = (0, 0);
    {
        let db = s.db();
        for m in &items {
            let _ = tvmaster_db::upsert_media(&db, m);
        }
        for (mid, path, missing) in &known {
            if !missing && !on_disk.contains(path) {
                if tvmaster_db::mark_media_missing(&db, *mid).is_ok() {
                    removed += 1;
                }
            } else if *missing
                && on_disk.contains(path)
                && tvmaster_db::mark_media_present(&db, *mid).is_ok()
            {
                back += 1;
            }
        }
    }
    let note = Rescan {
        at_ms: now_ms(),
        added: items.len(),
        removed,
        back,
    };
    s.monitor
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .last_rescan
        .insert(lib.id, note.clone());
    if note.added + note.removed + note.back > 0 {
        tracing::info!(
            library = lib.id,
            added = note.added,
            removed = note.removed,
            back = note.back,
            "watched folder changed"
        );
        // Channels on autopilot pick up the change, and new shows get their ad breaks found.
        {
            let db = s.db();
            let now = now_ms();
            for c in tvmaster_db::list_channels(&db)
                .unwrap_or_default()
                .iter()
                .filter(|c| c.library_ids.contains(&lib.id) && c.autopilot)
            {
                let _ = crate::autopilot::reroll(&db, c, now);
            }
        }
        if lib.kind == "shows" {
            crate::libraries::start_analysis(s, lib.id);
        }
        s.emit(json!({ "type": "library.changed", "library": lib.id, "added": note.added, "removed": note.removed, "back": note.back }));
    }
    Some(note)
}

impl AppState {
    /// Re-read watched folders on schedule and keep an eye out for problems.
    /// Needs a Tokio runtime.
    pub fn spawn_monitor(&self) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::spawn(async move {
            // Give a freshly started server a moment before judging anything.
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                let settings = load(&state.db());
                let due = settings.watch_minutes > 0
                    && state
                        .monitor
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .last_watch
                        .is_none_or(|t| {
                            t.elapsed()
                                >= Duration::from_secs(u64::from(settings.watch_minutes) * 60)
                        });
                if due {
                    state
                        .monitor
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .last_watch = Some(Instant::now());
                    let libs = tvmaster_db::list_libraries(&state.db()).unwrap_or_default();
                    for lib in libs.iter().filter(|l| l.enabled) {
                        let _ = rescan(&state, lib).await;
                    }
                }
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || check(&s)).await;
                tokio::time::sleep(CHECK_EVERY).await;
            }
        })
    }
}

// ---------- API ----------

fn snapshot(s: &AppState) -> Value {
    let settings = load(&s.db());
    let book = s.monitor.lock().unwrap_or_else(|p| p.into_inner());
    json!({
        "settings": settings,
        "alerts": book.active.values().collect::<Vec<_>>(),
        "rescans": book.last_rescan,
        "curl": which_curl(),
    })
}

fn which_curl() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `GET /api/v1/monitor`: the settings, the problems right now, and when each library was last re-read.
pub async fn get(State(s): State<AppState>) -> Json<Value> {
    Json(snapshot(&s))
}

/// `PUT /api/v1/monitor`: change the settings.
pub async fn put(State(s): State<AppState>, Json(body): Json<Settings>) -> ApiResult<Json<Value>> {
    body.validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    tvmaster_db::meta_set(
        &s.db(),
        META_KEY,
        &serde_json::to_string(&body).unwrap_or_default(),
    )?;
    // A changed interval takes effect at once.
    s.monitor
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .last_watch = None;
    Ok(Json(snapshot(&s)))
}

/// `POST /api/v1/monitor/test`: send a test message to the webhook.
pub async fn test(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let settings = load(&s.db());
    if settings.webhook().is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "set a webhook address first",
        ));
    }
    send(
        &settings,
        "TVMaster: test message",
        "Alerts will arrive here.",
        "ok",
        "test",
    );
    Ok(Json(json!({ "sent": true })))
}

/// `POST /api/v1/monitor/rescan`: re-read every library's folder now.
pub async fn rescan_now(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let libs = tvmaster_db::list_libraries(&s.db())?;
    let (mut added, mut removed, mut back, mut offline) = (0, 0, 0, 0);
    for lib in libs.iter().filter(|l| l.enabled) {
        match rescan(&s, lib).await {
            Some(n) => {
                added += n.added;
                removed += n.removed;
                back += n.back;
            }
            None => offline += 1,
        }
    }
    Ok(Json(
        json!({ "added": added, "removed": removed, "back": back, "unreachable": offline }),
    ))
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};

    #[test]
    fn settings_are_checked() {
        assert!(Settings::default().validate().is_ok());
        let mut s = Settings::default();
        s.watch_minutes = 7;
        assert!(s.validate().is_err());
        s.watch_minutes = 0;
        s.webhook = Some("ftp://x".into());
        assert!(s.validate().is_err());
        s.webhook = Some("https://ntfy.sh/my topic".into());
        assert!(s.validate().is_err(), "no spaces");
        s.webhook = Some("https://ntfy.sh/my-topic".into());
        assert!(s.validate().is_ok());
        s.webhook = Some(String::new());
        assert!(s.validate().is_ok() && s.webhook().is_none());
    }

    #[tokio::test]
    async fn settings_round_trip_and_defaults_are_sensible() {
        let app = router(AppState::in_memory());
        let (st, v) = call(&app, "GET", "/api/v1/monitor", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["settings"]["watch_minutes"], 15);
        assert_eq!(v["settings"]["alerts"], true);
        let (st, v) = call(
            &app,
            "PUT",
            "/api/v1/monitor",
            r#"{"watch_minutes":5,"webhook":"https://ntfy.sh/tv","webhook_text":true}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["settings"]["watch_minutes"], 5);
        let (_, v) = call(&app, "GET", "/api/v1/monitor", "").await;
        assert_eq!(v["settings"]["webhook"], "https://ntfy.sh/tv");
        assert_eq!(
            call(&app, "PUT", "/api/v1/monitor", r#"{"watch_minutes":3}"#)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/monitor/test", "").await.0,
            StatusCode::OK
        );
        let (_, v) = call(&app, "PUT", "/api/v1/monitor", r#"{"webhook":null}"#).await;
        assert!(v["settings"]["webhook"].is_null());
        assert_eq!(
            call(&app, "POST", "/api/v1/monitor/test", "").await.0,
            StatusCode::BAD_REQUEST,
            "nowhere to send it"
        );
    }

    #[test]
    fn a_problem_is_raised_on_the_second_sighting_and_cleared_when_it_is_gone() {
        let state = AppState::in_memory();
        let dir = std::env::temp_dir().join(format!("tvm-mon-{}", crate::generate_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = tvmaster_db::add_library(&state.db(), &dir.to_string_lossy(), "shows").unwrap();
        // FFmpeg is missing in a test state: raised at once.
        let (raised, _) = check(&state);
        assert_eq!(
            raised.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["system:ffmpeg"]
        );

        // The folder disappears: noticed, but only reported the second time.
        std::fs::remove_dir_all(&dir).unwrap();
        let (raised, _) = check(&state);
        assert!(raised.is_empty(), "one sighting is not enough");
        let (raised, _) = check(&state);
        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].id, format!("library:{}:offline", lib.id));
        assert!(raised[0].detail.contains(&*dir.to_string_lossy()));
        let (again, _) = check(&state);
        assert!(again.is_empty(), "reported once, not every check");
        assert_eq!(state.monitor.lock().unwrap().active.len(), 2);

        // It comes back: cleared.
        std::fs::create_dir_all(&dir).unwrap();
        let (_, cleared) = check(&state);
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].id, format!("library:{}:offline", lib.id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_video(path: &FsPath, settled: bool) {
        let ok = std::process::Command::new("ffmpeg")
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
                "yuv420p",
            ])
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "make a test video");
        if settled {
            // as if it had been there a while
            let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            f.set_modified(SystemTime::now() - Duration::from_secs(3600))
                .unwrap();
        }
    }

    #[tokio::test]
    async fn a_watched_folder_picks_up_new_files_and_notices_removed_and_returning_ones() {
        let Ok(info) = tvmaster_media::FfmpegInfo::detect("ffmpeg", "ffprobe") else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("tvm-watch-{}", crate::generate_token()));
        std::fs::create_dir_all(dir.join("Show")).unwrap();
        let state = AppState::new(tvmaster_db::open_in_memory().unwrap(), Some(info), None);
        let lib = tvmaster_db::add_library(&state.db(), &dir.to_string_lossy(), "shows").unwrap();
        make_video(&dir.join("Show/Show.S01E01.mp4"), true);

        let n = rescan(&state, &lib).await.expect("reachable");
        assert_eq!((n.added, n.removed, n.back), (1, 0, 0));
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 1);
        let n = rescan(&state, &lib).await.unwrap();
        assert_eq!(
            (n.added, n.removed, n.back),
            (0, 0, 0),
            "nothing changed, nothing probed"
        );

        // A file still being copied in (modified a moment ago) waits until it has settled.
        make_video(&dir.join("Show/Show.S01E02.mp4"), false);
        let n = rescan(&state, &lib).await.unwrap();
        assert_eq!(n.added, 0, "too new to trust");
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join("Show/Show.S01E02.mp4"))
            .unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        let n = rescan(&state, &lib).await.unwrap();
        assert_eq!(n.added, 1);
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 2);

        // One is deleted, then restored.
        std::fs::rename(dir.join("Show/Show.S01E01.mp4"), dir.join("moved.tmp")).unwrap();
        let n = rescan(&state, &lib).await.unwrap();
        assert_eq!((n.added, n.removed), (0, 1));
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 1);
        std::fs::rename(dir.join("moved.tmp"), dir.join("Show/Show.S01E01.mp4")).unwrap();
        let n = rescan(&state, &lib).await.unwrap();
        assert_eq!((n.added, n.removed, n.back), (0, 0, 1));
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 2);

        // A folder that cannot be reached is left alone: nothing is flagged missing.
        let gone = dir.with_extension("away");
        std::fs::rename(&dir, &gone).unwrap();
        assert!(rescan(&state, &lib).await.is_none());
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 2);
        // Nor one that is reachable but came up empty (a mount that is not ready).
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            rescan(&state, &lib).await.is_none(),
            "empty is suspicious, not a purge"
        );
        assert_eq!(tvmaster_db::count_media(&state.db(), lib.id).unwrap(), 2);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&gone);
    }

    /// A web server that answers once and hands back what it was sent.
    fn catch_one_request() -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = c.read(&mut buf) {
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if body.len() >= len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let _ =
                c.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
            let _ = tx.send(String::from_utf8_lossy(&got).to_string());
        });
        (url, rx)
    }

    #[tokio::test]
    async fn an_alert_reaches_the_webhook_as_json_or_as_text() {
        if !which_curl() {
            eprintln!("curl not installed; skipping");
            return;
        }
        let state = AppState::in_memory();
        let app = router(state.clone());
        // JSON, with the keys Slack and Discord look for.
        let (url, rx) = catch_one_request();
        let body = format!(r#"{{"webhook":"{url}"}}"#);
        assert_eq!(
            call(&app, "PUT", "/api/v1/monitor", &body).await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/monitor/test", "").await.0,
            StatusCode::OK
        );
        let got = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the webhook was called");
        assert!(got.starts_with("POST /hook"), "{got}");
        assert!(
            got.to_lowercase()
                .contains("content-type: application/json"),
            "{got}"
        );
        let json: Value = serde_json::from_str(got.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(json["title"], "TVMaster: test message");
        assert!(json["text"]
            .as_str()
            .unwrap()
            .contains("Alerts will arrive here"));
        assert_eq!(json["content"], json["text"]);

        // Plain text, with the title as a header (ntfy).
        let (url, rx) = catch_one_request();
        let body = format!(r#"{{"webhook":"{url}","webhook_text":true}}"#);
        call(&app, "PUT", "/api/v1/monitor", &body).await;
        call(&app, "POST", "/api/v1/monitor/test", "").await;
        let got = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the webhook was called");
        assert!(
            got.to_lowercase().contains("title: tvmaster: test message"),
            "{got}"
        );
        assert!(got.ends_with("Alerts will arrive here."), "{got}");
    }

    #[test]
    fn alerts_can_be_turned_off() {
        let state = AppState::in_memory();
        tvmaster_db::meta_set(&state.db(), META_KEY, r#"{"alerts":false}"#).unwrap();
        for _ in 0..3 {
            let (raised, _) = check(&state);
            assert!(raised.is_empty());
        }
    }

    #[test]
    fn a_library_with_no_files_left_makes_its_channels_report_nothing_to_play() {
        let state = AppState::in_memory();
        let lib = tvmaster_db::add_library(&state.db(), "/", "shows").unwrap();
        let body = format!(
            r#"{{"id":"a","number":4,"name":"A","library_ids":[{}]}}"#,
            lib.id
        );
        let ch: tvmaster_core::Channel = {
            // build through the real API so the row is valid
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let app = router(state.clone());
                let (st, v) = call(&app, "POST", "/api/v1/channels", &body).await;
                assert!(
                    st.is_success() || st == StatusCode::CONFLICT || st.is_client_error(),
                    "{st} {v}"
                );
                tvmaster_db::get_channel(&state.db(), "a")
                    .unwrap()
                    .unwrap_or_else(|| panic!("channel saved: {v}"))
            })
        };
        let wrong = conditions(&state);
        assert!(
            wrong
                .iter()
                .any(|a| a.id == format!("channel:{}:empty", ch.id)),
            "{wrong:?}"
        );
    }
}
