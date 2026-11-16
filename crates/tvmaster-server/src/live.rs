//! Live control: put the whole station, or single channels, on the test screen,
//! a blank or an emergency-alert screen, and take them off again. Overrides are
//! kept in the database, so a restart does not quietly bring the programming back.

use crate::{ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tvmaster_playout::{LiveMode, LiveOverride};

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
    pub started_ms: i64,
    /// When it ends by itself (`None` = until cleared).
    #[serde(default)]
    pub until_ms: Option<i64>,
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
            "event": "Emergency alert test",
            "text": "This is a test of the Emergency Alert System. This is only a test. If this had been an actual emergency, you would have been told where to tune for news and official information. This concludes this test of the Emergency Alert System."
        },
        {
            "id": "civil_emergency",
            "name": "Civil emergency (sample)",
            "event": "Civil emergency message",
            "text": "SAMPLE ONLY - THIS IS A TEST. Authorities in your area have issued a civil emergency message. Stay tuned to this station and local officials for instructions. No action is required; this is only a test."
        },
        {
            "id": "weather_warning",
            "name": "Severe weather (sample)",
            "event": "Tornado warning",
            "text": "SAMPLE ONLY - THIS IS A TEST. A tornado warning is in effect for your area until 9:00 PM. Take shelter now in a basement or an interior room on the lowest floor. This is only a test."
        },
        {
            "id": "child_alert",
            "name": "Child abduction (sample)",
            "event": "Child abduction emergency",
            "text": "SAMPLE ONLY - THIS IS A TEST. An alert has been issued for a missing child in your area. If you have information, call your local police. This is only a test of the alert system."
        },
        {
            "id": "all_clear",
            "name": "Technical difficulties",
            "event": "Please stand by",
            "text": "We are experiencing technical difficulties. Programming will resume shortly. Thank you for your patience."
        }
    ])
}

fn entry_json(channel: &str, e: &Entry) -> Value {
    json!({
        "channel": channel, "mode": e.mode, "title": e.title, "message": e.message,
        "started_ms": e.started_ms, "until_ms": e.until_ms,
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
    json!({ "overrides": active, "samples": samples() })
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
    if body.mode == "alert" {
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
    use crate::test_util::call;
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
}
