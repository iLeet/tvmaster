//! HTTP API. Milestone 1 exposes health plus the Virtual TV / remote endpoints
//! (in-memory); streams, M3U/XMLTV, HDHomeRun and WebSocket sync come later.
//! See docs/API.md.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tvmaster_core::Tv;

#[derive(Clone, Default)]
pub struct AppState {
    tvs: Arc<Mutex<BTreeMap<String, Tv>>>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/tvs", get(list_tvs).post(register_tv))
        .route("/api/v1/tvs/:id", get(get_tv))
        .route("/api/v1/tvs/:id/channel", post(set_channel))
        .route("/api/v1/tvs/:id/power", post(set_power))
        .with_state(AppState::default())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_tvs(State(s): State<AppState>) -> Json<Vec<Tv>> {
    Json(s.tvs.lock().unwrap().values().cloned().collect())
}

#[derive(Deserialize)]
struct NewTv {
    id: String,
    name: String,
}

async fn register_tv(
    State(s): State<AppState>,
    Json(body): Json<NewTv>,
) -> (StatusCode, Json<Tv>) {
    let tv = Tv::new(body.id.clone(), body.name);
    s.tvs.lock().unwrap().insert(body.id, tv.clone());
    (StatusCode::CREATED, Json(tv))
}

async fn get_tv(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<Tv>, StatusCode> {
    s.tvs
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct ChannelChange {
    number: Option<u32>,
    step: Option<String>,
}

async fn set_channel(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ChannelChange>,
) -> Result<Json<Tv>, StatusCode> {
    let mut tvs = s.tvs.lock().unwrap();
    let tv = tvs.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    let current = tv.channel.unwrap_or(1);
    tv.channel = match (body.number, body.step.as_deref()) {
        (Some(n), _) => Some(n),
        (None, Some("up")) => Some(current.saturating_add(1)),
        (None, Some("down")) => Some(current.saturating_sub(1).max(1)),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    Ok(Json(tv.clone()))
}

#[derive(Deserialize)]
struct PowerChange {
    on: bool,
}

async fn set_power(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PowerChange>,
) -> Result<Json<Tv>, StatusCode> {
    let mut tvs = s.tvs.lock().unwrap();
    let tv = tvs.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    tv.power = body.on;
    Ok(Json(tv.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn call(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn health_is_ok() {
        let (status, body) = call(&router(), "GET", "/api/v1/health", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn two_tvs_watch_different_channels() {
        let app = router();
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"living-room","name":"Living Room"}"#).await;
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"bedroom","name":"Bedroom"}"#).await;
        call(&app, "POST", "/api/v1/tvs/living-room/channel", r#"{"number":3}"#).await;
        call(&app, "POST", "/api/v1/tvs/bedroom/channel", r#"{"number":7}"#).await;
        let (_, a) = call(&app, "GET", "/api/v1/tvs/living-room", "").await;
        let (_, b) = call(&app, "GET", "/api/v1/tvs/bedroom", "").await;
        assert_eq!(a["channel"], 3);
        assert_eq!(b["channel"], 7);
    }

    #[tokio::test]
    async fn channel_step_and_unknown_tv() {
        let app = router();
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        call(&app, "POST", "/api/v1/tvs/den/channel", r#"{"number":5}"#).await;
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/channel", r#"{"step":"up"}"#).await;
        assert_eq!(tv["channel"], 6);
        let (status, _) = call(&app, "POST", "/api/v1/tvs/nope/channel", r#"{"number":1}"#).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
