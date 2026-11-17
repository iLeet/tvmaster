//! Live control: put the whole station, or single channels, on the test screen,
//! a blank or an emergency-alert screen, and take them off again. Overrides are
//! kept in the database, so a restart does not quietly bring the programming back.

use crate::{ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tvmaster_playout::{AlertLook, AlertSound, AlertStyle, LiveMode, LiveOverride};
use tvmaster_render::alert::{AlertTheme, Severity};

const KEY: &str = "live_overrides";
const MAX_EVENT: usize = 80;
const MAX_TEXT: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    /// `test_screen`, `off_air`, `black`, `static` or `alert`.
    pub mode: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    /// `full` (the whole screen) or `bar` (across the bottom of the programme).
    #[serde(default = "d_style")]
    pub style: String,
    /// `retro`, `modern`, `midnight`, `graphite`, `snow`, `forest`, `berry` or `mono`.
    #[serde(default = "d_theme")]
    pub theme: String,
    /// `emergency`, `warning` or `info`.
    #[serde(default = "d_severity")]
    pub severity: String,
    /// `none`, `attention`, `information`, `urgent` or `beeps`.
    #[serde(default = "d_sound")]
    pub sound: String,
    pub started_ms: i64,
    /// When it ends by itself (`None` = until cleared).
    #[serde(default)]
    pub until_ms: Option<i64>,
}

fn d_style() -> String {
    "full".into()
}
fn d_theme() -> String {
    "retro".into()
}
fn d_severity() -> String {
    "emergency".into()
}
fn d_sound() -> String {
    "attention".into()
}

pub type Book = BTreeMap<String, Entry>;

pub(crate) fn load(db: &rusqlite::Connection) -> Book {
    tvmaster_db::meta_get(db, KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

fn save(s: &AppState, book: &Book) {
    let json = serde_json::to_string(book).unwrap_or_else(|_| "{}".into());
    let _ = tvmaster_db::meta_set(&s.db(), KEY, &json);
}

/// What playout should air on `channel` right now.
pub(crate) fn current(book: &Book, channel: &str, now_ms: i64) -> Option<LiveOverride> {
    let e = book.get(channel)?;
    if e.until_ms.is_some_and(|u| u <= now_ms) {
        return None;
    }
    let mode = match e.mode.as_str() {
        "test_screen" => LiveMode::TestScreen,
        "off_air" => LiveMode::OffAir,
        "black" => LiveMode::Black,
        "static" => LiveMode::Static,
        "alert" => LiveMode::Alert {
            event: e.title.clone(),
            text: e.message.clone(),
            look: AlertLook {
                theme: AlertTheme::parse(&e.theme).unwrap_or_default(),
                severity: Severity::parse(&e.severity).unwrap_or_default(),
            },
            style: AlertStyle::parse(&e.style).unwrap_or_default(),
            sound: AlertSound::parse(&e.sound).unwrap_or_default(),
        },
        _ => return None,
    };
    Some(LiveOverride {
        mode,
        started_ms: e.started_ms,
    })
}

/// Sample alerts to try, or to start from. Every one says plainly that it is a test.
pub fn samples() -> Value {
    json!([
        {
            "id": "eas_test",
            "name": "Required test (EAS style)",
            "severity": "emergency", "sound": "attention",
            "event": "Emergency alert test",
            "text": "This is a test of the Emergency Alert System. This is only a test. If this had been an actual emergency, you would have been told where to tune for news and official information. This concludes this test of the Emergency Alert System."
        },
        {
            "id": "civil_emergency",
            "name": "Civil emergency (sample)",
            "severity": "emergency", "sound": "urgent",
            "event": "Civil emergency message",
            "text": "SAMPLE ONLY - THIS IS A TEST. Authorities in your area have issued a civil emergency message. Stay tuned to this station and local officials for instructions. No action is required; this is only a test."
        },
        {
            "id": "weather_warning",
            "name": "Severe weather (sample)",
            "severity": "warning", "sound": "attention",
            "event": "Tornado warning",
            "text": "SAMPLE ONLY - THIS IS A TEST. A tornado warning is in effect for your area until 9:00 PM. Take shelter now in a basement or an interior room on the lowest floor. This is only a test."
        },
        {
            "id": "child_alert",
            "name": "Child abduction (sample)",
            "severity": "emergency", "sound": "attention",
            "event": "Child abduction emergency",
            "text": "SAMPLE ONLY - THIS IS A TEST. An alert has been issued for a missing child in your area. If you have information, call your local police. This is only a test of the alert system."
        },
        {
            "id": "all_clear",
            "name": "Technical difficulties",
            "severity": "info", "sound": "information",
            "event": "Please stand by",
            "text": "We are experiencing technical difficulties. Programming will resume shortly. Thank you for your patience."
        }
    ])
}

/// The choices an alert offers, for the apps to build their pickers from.
pub fn options() -> Value {
    let pairs = |p: &[(&str, &str)]| -> Value {
        p.iter()
            .map(|(v, l)| json!({ "value": v, "label": l }))
            .collect::<Vec<_>>()
            .into()
    };
    json!({
        "styles": [
            { "value": "full", "label": "Full screen" },
            { "value": "bar", "label": "Bar across the bottom" },
        ],
        "themes": pairs(&AlertTheme::ALL),
        "severities": pairs(&Severity::ALL),
        "sounds": pairs(&AlertSound::ALL),
    })
}

fn entry_json(channel: &str, e: &Entry) -> Value {
    json!({
        "channel": channel, "mode": e.mode, "title": e.title, "message": e.message,
        "started_ms": e.started_ms, "until_ms": e.until_ms,
        "style": e.style, "theme": e.theme, "severity": e.severity, "sound": e.sound,
    })
}

fn state_json(s: &AppState) -> Value {
    let now = crate::plan::now_ms();
    let book = s.live.lock().unwrap_or_else(|p| p.into_inner());
    let active: Vec<Value> = book
        .iter()
        .filter(|(_, e)| e.until_ms.is_none_or(|u| u > now))
        .map(|(c, e)| entry_json(c, e))
        .collect();
    json!({ "overrides": active, "samples": samples(), "options": options() })
}

/// `GET /api/v1/live`
pub async fn get_live(State(s): State<AppState>) -> Json<Value> {
    Json(state_json(&s))
}

#[derive(Deserialize)]
pub struct SetBody {
    /// Channel ids, or `["all"]`.
    channels: Vec<String>,
    /// `test_screen`, `off_air`, `black`, `static` or `alert`.
    mode: String,
    /// For an alert: the event ("Tornado warning").
    #[serde(default)]
    title: Option<String>,
    /// For an alert: the message.
    #[serde(default)]
    message: Option<String>,
    /// One of the sample alerts (`eas_test`...), instead of writing one.
    #[serde(default)]
    sample: Option<String>,
    /// Go back to programming by itself after this many minutes.
    #[serde(default)]
    minutes: Option<u32>,
    /// For an alert: `full` or `bar`.
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    /// For an alert: `none`, `attention`, `information`, `urgent` or `beeps`.
    #[serde(default)]
    sound: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewBody {
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    t: f32,
}

/// `POST /api/v1/live/preview`: one frame of an alert as it would look, as a PNG (nothing goes on air).
pub async fn preview(Json(b): Json<PreviewBody>) -> ApiResult<axum::response::Response> {
    use axum::http::{header, HeaderValue};
    use axum::response::IntoResponse;
    let bad = |what: &str| ApiError::new(StatusCode::BAD_REQUEST, format!("unknown {what}"));
    let look = AlertLook {
        theme: AlertTheme::parse(b.theme.as_deref().unwrap_or("retro"))
            .ok_or_else(|| bad("theme"))?,
        severity: Severity::parse(b.severity.as_deref().unwrap_or("emergency"))
            .ok_or_else(|| bad("severity"))?,
    };
    let style =
        AlertStyle::parse(b.style.as_deref().unwrap_or("full")).ok_or_else(|| bad("style"))?;
    let title = if b.title.trim().is_empty() {
        "Emergency alert test".to_string()
    } else {
        b.title.chars().take(80).collect()
    };
    let message = if b.message.trim().is_empty() {
        "This is a test. No action is required.".to_string()
    } else {
        b.message.chars().take(600).collect()
    };
    let t = b.t.clamp(0.0, 3600.0);
    let png = tokio::task::spawn_blocking(move || {
        let c = tvmaster_render::alert::alert_preview(&look, style, &title, &message, t);
        let mut rgba = Vec::with_capacity(c.w * c.h * 4);
        for px in c.px.as_chunks::<3>().0 {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        tvmaster_render::png::rgba_png(c.w as u32, c.h as u32, &rgba)
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not draw the alert",
        )
    })?;
    let mut res = png.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

#[derive(Deserialize)]
pub struct ClearBody {
    #[serde(default)]
    channels: Vec<String>,
}

fn resolve_channels(s: &AppState, wanted: &[String]) -> ApiResult<Vec<String>> {
    let all: Vec<String> = tvmaster_db::list_channels(&s.db())?
        .into_iter()
        .map(|c| c.id)
        .collect();
    if wanted.is_empty() || wanted.iter().any(|w| w == "all") {
        return Ok(all);
    }
    for w in wanted {
        if !all.contains(w) {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                format!("no channel '{w}'"),
            ));
        }
    }
    Ok(wanted.to_vec())
}

/// Channels that are on air restart so the change shows within seconds.
fn restart_running(s: &AppState, channels: &[String]) {
    let Some(playout) = &s.playout else { return };
    for id in channels {
        if playout.is_running(id) {
            playout.stop(id);
            let _ = crate::stream::ensure_on_air(s, id, true);
        }
    }
}

/// `POST /api/v1/live`: take channels over.
pub async fn set_live(
    State(s): State<AppState>,
    Json(body): Json<SetBody>,
) -> ApiResult<Json<Value>> {
    if body.channels.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "name the channels, or [\"all\"]",
        ));
    }
    let (mut title, mut message) = (
        body.title.clone().unwrap_or_default(),
        body.message.clone().unwrap_or_default(),
    );
    if let Some(id) = &body.sample {
        let found = samples()
            .as_array()
            .and_then(|a| a.iter().find(|x| x["id"] == id.as_str()).cloned())
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "no such sample alert"))?;
        if title.trim().is_empty() {
            title = found["event"].as_str().unwrap_or_default().to_string();
        }
        if message.trim().is_empty() {
            message = found["text"].as_str().unwrap_or_default().to_string();
        }
    }
    if !["test_screen", "off_air", "black", "static", "alert"].contains(&body.mode.as_str()) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "mode must be test_screen, off_air, black, static or alert",
        ));
    }
    // The look of an alert: what was asked, else the sample's own, else the usual.
    let sample = body.sample.as_deref().and_then(|id| {
        samples()
            .as_array()
            .and_then(|a| a.iter().find(|x| x["id"] == id).cloned())
    });
    let pick = |asked: &Option<String>, key: &str, default: &str| -> String {
        asked
            .clone()
            .or_else(|| {
                sample
                    .as_ref()
                    .and_then(|s| s[key].as_str().map(str::to_string))
            })
            .unwrap_or_else(|| default.to_string())
    };
    let (style, theme, severity, sound) = (
        pick(&body.style, "style", "full"),
        pick(&body.theme, "theme", "retro"),
        pick(&body.severity, "severity", "emergency"),
        pick(&body.sound, "sound", "attention"),
    );
    if body.mode == "alert" {
        if AlertStyle::parse(&style).is_none()
            || AlertTheme::parse(&theme).is_none()
            || Severity::parse(&severity).is_none()
            || AlertSound::parse(&sound).is_none()
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "unknown style, theme, severity or sound (see GET /api/v1/live for the choices)",
            ));
        }
        if title.trim().is_empty() || message.trim().is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "an alert needs a title and a message (or a sample)",
            ));
        }
        if title.chars().count() > MAX_EVENT || message.chars().count() > MAX_TEXT {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "keep the title under {MAX_EVENT} and the message under {MAX_TEXT} characters"
                ),
            ));
        }
    } else {
        title.clear();
        message.clear();
    }
    if body.minutes.is_some_and(|m| m == 0 || m > 7 * 24 * 60) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "minutes must be 1 to 10080",
        ));
    }
    let channels = resolve_channels(&s, &body.channels)?;
    let now = crate::plan::now_ms();
    {
        let mut book = s.live.lock().unwrap_or_else(|p| p.into_inner());
        for c in &channels {
            book.insert(
                c.clone(),
                Entry {
                    mode: body.mode.clone(),
                    title: title.clone(),
                    message: message.clone(),
                    style: style.clone(),
                    theme: theme.clone(),
                    severity: severity.clone(),
                    sound: sound.clone(),
                    started_ms: now,
                    until_ms: body.minutes.map(|m| now + i64::from(m) * 60_000),
                },
            );
        }
        save(&s, &book);
    }
    restart_running(&s, &channels);
    s.emit(json!({ "type": "live.changed" }));
    tracing::warn!(channels = ?channels, mode = %body.mode, "live override set");
    Ok(Json(state_json(&s)))
}

/// `POST /api/v1/live/clear`: put channels back on their programming (all of them when none are named).
pub async fn clear_live(
    State(s): State<AppState>,
    body: Option<Json<ClearBody>>,
) -> ApiResult<Json<Value>> {
    let wanted = body.map(|b| b.0.channels).unwrap_or_default();
    let channels = if wanted.is_empty() || wanted.iter().any(|w| w == "all") {
        s.live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect()
    } else {
        wanted
    };
    clear_channels(&s, &channels);
    Ok(Json(state_json(&s)))
}

fn clear_channels(s: &AppState, channels: &[String]) {
    let removed: Vec<String> = {
        let mut book = s.live.lock().unwrap_or_else(|p| p.into_inner());
        let removed = channels
            .iter()
            .filter(|c| book.remove(*c).is_some())
            .cloned()
            .collect();
        save(s, &book);
        removed
    };
    if !removed.is_empty() {
        restart_running(s, &removed);
        s.emit(json!({ "type": "live.changed" }));
        tracing::info!(channels = ?removed, "live override cleared");
    }
}

/// Channels whose timed override has run out go back to programming.
pub(crate) fn expire(s: &AppState) {
    let now = crate::plan::now_ms();
    let done: Vec<String> = s
        .live
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|(_, e)| e.until_ms.is_some_and(|u| u <= now))
        .map(|(c, _)| c.clone())
        .collect();
    if !done.is_empty() {
        clear_channels(s, &done);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    async fn two_channels() -> (AppState, axum::Router) {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let lib = tvmaster_db::add_library(&state.db(), "/x", "shows").unwrap();
        for (id, n) in [("a", 4), ("b", 5)] {
            let body = format!(
                r#"{{"id":"{id}","number":{n},"name":"{id}","library_ids":[{}]}}"#,
                lib.id
            );
            call(&app, "POST", "/api/v1/channels", &body).await;
        }
        (state, app)
    }

    #[tokio::test]
    async fn the_whole_station_can_be_taken_over_and_given_back() {
        let (state, app) = two_channels().await;
        let (st, v) = call(&app, "GET", "/api/v1/live", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["overrides"].as_array().unwrap().len(), 0);
        assert!(v["samples"].as_array().unwrap().len() >= 4);

        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/live",
            r#"{"channels":["all"],"mode":"test_screen"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["overrides"].as_array().unwrap().len(), 2);
        assert!(
            crate::live::current(&state.live.lock().unwrap(), "a", crate::plan::now_ms()).is_some()
        );

        // one channel goes to an emergency alert from a sample
        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/live",
            r#"{"channels":["b"],"mode":"alert","sample":"eas_test"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        let b = v["overrides"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["channel"] == "b")
            .unwrap();
        assert_eq!(b["mode"], "alert");
        assert!(b["message"].as_str().unwrap().contains("only a test"));

        // survives a restart (the book is read back from the database)
        let again = crate::live::load(&state.db());
        assert_eq!(again.len(), 2);

        let (_, v) = call(&app, "POST", "/api/v1/live/clear", r#"{"channels":["a"]}"#).await;
        assert_eq!(v["overrides"].as_array().unwrap().len(), 1);
        let (_, v) = call(&app, "POST", "/api/v1/live/clear", "{}").await;
        assert_eq!(v["overrides"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn bad_requests_are_refused_and_timed_overrides_expire() {
        let (state, app) = two_channels().await;
        for body in [
            r#"{"channels":[],"mode":"black"}"#,
            r#"{"channels":["all"],"mode":"disco"}"#,
            r#"{"channels":["all"],"mode":"alert"}"#,
            r#"{"channels":["nope"],"mode":"black"}"#,
            r#"{"channels":["all"],"mode":"black","minutes":0}"#,
        ] {
            let (st, _) = call(&app, "POST", "/api/v1/live", body).await;
            assert!(st.is_client_error(), "{body}");
        }
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/live",
            r#"{"channels":["a"],"mode":"black","minutes":1}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let now = crate::plan::now_ms();
        assert!(crate::live::current(&state.live.lock().unwrap(), "a", now).is_some());
        assert!(
            crate::live::current(&state.live.lock().unwrap(), "a", now + 61_000).is_none(),
            "a timed override stops counting once its time is up"
        );
        state.live.lock().unwrap().get_mut("a").unwrap().until_ms = Some(now - 1);
        crate::live::expire(&state);
        assert!(state.live.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn an_alert_has_a_look_a_sound_and_a_preview() {
        let (state, app) = two_channels().await;
        let (st, v) = call(&app, "GET", "/api/v1/live", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(v["options"]["themes"].as_array().unwrap().len() >= 8);
        assert!(v["options"]["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["value"] == "none"));

        // a sample brings its own severity and sound; asking overrides them
        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/live",
            r#"{"channels":["a"],"mode":"alert","sample":"all_clear","style":"bar","theme":"modern"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        let a = &v["overrides"][0];
        assert_eq!(
            (a["style"].as_str(), a["theme"].as_str()),
            (Some("bar"), Some("modern"))
        );
        assert_eq!(
            (a["severity"].as_str(), a["sound"].as_str()),
            (Some("info"), Some("information"))
        );
        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/live",
            r#"{"channels":["b"],"mode":"alert","sample":"eas_test","sound":"none"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        let cur = {
            let book = state.live.lock().unwrap();
            crate::live::current(&book, "b", crate::plan::now_ms()).unwrap()
        };
        assert!(matches!(
            cur.mode,
            tvmaster_playout::LiveMode::Alert {
                sound: tvmaster_playout::AlertSound::None,
                ..
            }
        ));
        for body in [
            r#"{"channels":["a"],"mode":"alert","sample":"eas_test","theme":"disco"}"#,
            r#"{"channels":["a"],"mode":"alert","sample":"eas_test","style":"corner"}"#,
            r#"{"channels":["a"],"mode":"alert","sample":"eas_test","sound":"loud"}"#,
        ] {
            let (st, _) = call(&app, "POST", "/api/v1/live", body).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        }

        let (st, p1) = call_raw(
            &app,
            "POST",
            "/api/v1/live/preview",
            r#"{"theme":"modern","severity":"warning"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(p1.starts_with("\u{fffd}PNG"));
        let (_, p2) = call_raw(
            &app,
            "POST",
            "/api/v1/live/preview",
            r#"{"theme":"modern","style":"bar"}"#,
        )
        .await;
        assert_ne!(p1, p2, "a bar is not the full screen");
        let (st, _) = call_raw(&app, "POST", "/api/v1/live/preview", r#"{"theme":"nope"}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
}
