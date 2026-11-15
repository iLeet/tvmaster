//! Virtual TVs and the remote API. A TV is a registered player with a stable
//! id; the remote changes its channel. Different TVs watch different channels
//! at the same time. Every change is pushed to connected WebSocket clients.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
    Json,
};
use serde::Deserialize;
use tvmaster_core::Tv;

fn not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "no such tv")
}

pub(crate) fn valid_tv_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Register a TV, or rename an existing one (keeping its channel and volume).
pub(crate) fn register_inner(s: &AppState, id: &str, name: &str) -> ApiResult<Tv> {
    if !valid_tv_id(id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "id must be 1-32 chars of a-z, 0-9, '-' or '_'",
        ));
    }
    let tv = {
        let db = s.db();
        let tv = match tvmaster_db::get_tv(&db, id)? {
            Some(mut existing) => {
                existing.name = name.to_string();
                existing
            }
            None => Tv::new(id, name),
        };
        tvmaster_db::upsert_tv(&db, &tv)?;
        tv
    };
    Ok(s.tv_changed(tv))
}

/// Tune a TV by channel number, or step through the real channels (wrapping).
/// Turns the TV on.
pub(crate) fn set_channel_inner(
    s: &AppState,
    id: &str,
    number: Option<u32>,
    step: Option<&str>,
) -> ApiResult<Tv> {
    let tv = {
        let db = s.db();
        let mut tv = tvmaster_db::get_tv(&db, id)?.ok_or_else(not_found)?;
        let mut numbers: Vec<u32> = tvmaster_db::list_channels(&db)?
            .into_iter()
            .map(|c| c.number)
            .collect();
        numbers.sort_unstable();

        let target = match (number, step) {
            (Some(n), _) => {
                if !numbers.contains(&n) {
                    return Err(ApiError::new(
                        StatusCode::NOT_FOUND,
                        format!("no channel {n}"),
                    ));
                }
                n
            }
            (None, Some(dir @ ("up" | "down"))) => {
                if numbers.is_empty() {
                    return Err(ApiError::new(StatusCode::NOT_FOUND, "no channels"));
                }
                let pos = tv
                    .channel
                    .and_then(|c| numbers.iter().position(|n| *n == c));
                let len = numbers.len();
                let idx = match (pos, dir) {
                    (Some(p), "up") => (p + 1) % len,
                    (Some(p), _) => (p + len - 1) % len,
                    (None, "up") => 0,
                    (None, _) => len - 1,
                };
                numbers[idx]
            }
            _ => {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "send {\"number\": n} or {\"step\": \"up\"|\"down\"}",
                ))
            }
        };
        tv.channel = Some(target);
        tv.power = true;
        tvmaster_db::upsert_tv(&db, &tv)?;
        tv
    };
    Ok(s.tv_changed(tv))
}

pub(crate) fn set_power_inner(s: &AppState, id: &str, on: bool) -> ApiResult<Tv> {
    let tv = {
        let db = s.db();
        let mut tv = tvmaster_db::get_tv(&db, id)?.ok_or_else(not_found)?;
        tv.power = on;
        tvmaster_db::upsert_tv(&db, &tv)?;
        tv
    };
    Ok(s.tv_changed(tv))
}

pub(crate) fn set_volume_inner(
    s: &AppState,
    id: &str,
    level: Option<u8>,
    muted: Option<bool>,
) -> ApiResult<Tv> {
    if level.is_some_and(|l| l > 100) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "level must be 0-100",
        ));
    }
    if level.is_none() && muted.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "send {\"level\": 0-100} and/or {\"muted\": bool}",
        ));
    }
    let tv = {
        let db = s.db();
        let mut tv = tvmaster_db::get_tv(&db, id)?.ok_or_else(not_found)?;
        if let Some(l) = level {
            tv.volume = l;
            tv.muted = false; // turning the volume up un-mutes, like a real TV
        }
        if let Some(m) = muted {
            tv.muted = m;
        }
        tvmaster_db::upsert_tv(&db, &tv)?;
        tv
    };
    Ok(s.tv_changed(tv))
}

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<Tv>>> {
    let tvs = tvmaster_db::list_tvs(&s.db())?;
    Ok(Json(tvs.into_iter().map(|t| s.tv_view(t)).collect()))
}

#[derive(Deserialize)]
pub struct NewTv {
    id: String,
    name: String,
}

pub async fn register(
    State(s): State<AppState>,
    Json(body): Json<NewTv>,
) -> ApiResult<(StatusCode, Json<Tv>)> {
    Ok((
        StatusCode::CREATED,
        Json(register_inner(&s, &body.id, &body.name)?),
    ))
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Tv>> {
    let tv = tvmaster_db::get_tv(&s.db(), &id)?.ok_or_else(not_found)?;
    Ok(Json(s.tv_view(tv)))
}

pub async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if !tvmaster_db::delete_tv(&s.db(), &id)? {
        return Err(not_found());
    }
    s.emit(serde_json::json!({ "type": "tv.removed", "id": id }));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ChannelChange {
    /// Channel number to tune to.
    number: Option<u32>,
    /// Or step through existing channels: "up" or "down" (wraps around).
    step: Option<String>,
}

pub async fn set_channel(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ChannelChange>,
) -> ApiResult<Json<Tv>> {
    Ok(Json(set_channel_inner(
        &s,
        &id,
        body.number,
        body.step.as_deref(),
    )?))
}

#[derive(Deserialize)]
pub struct PowerChange {
    on: bool,
}

pub async fn set_power(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PowerChange>,
) -> ApiResult<Json<Tv>> {
    Ok(Json(set_power_inner(&s, &id, body.on)?))
}

#[derive(Deserialize)]
pub struct VolumeChange {
    level: Option<u8>,
    muted: Option<bool>,
}

pub async fn set_volume(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VolumeChange>,
) -> ApiResult<Json<Tv>> {
    Ok(Json(set_volume_inner(&s, &id, body.level, body.muted)?))
}

/// `/tv/{id}.m3u8`: a stable URL for a TV. It redirects to the stream of the
/// channel the TV is tuned to *when it is opened*, so plain IPTV clients can be
/// pointed at "the living room" without knowing the channel. (To follow a
/// remote live, use the web player or the WebSocket API.)
pub async fn tv_stream(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Redirect> {
    let id = name.strip_suffix(".m3u8").unwrap_or(&name);
    let db = s.db();
    let tv = tvmaster_db::get_tv(&db, id)?.ok_or_else(not_found)?;
    if !tv.power {
        return Err(ApiError::new(StatusCode::CONFLICT, "tv is off"));
    }
    let number = tv
        .channel
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "tv has no channel selected"))?;
    let channel = tvmaster_db::list_channels(&db)?
        .into_iter()
        .find(|c| c.number == number)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "the tv's channel no longer exists"))?;
    Ok(Redirect::temporary(&format!(
        "/stream/{}/index.m3u8",
        channel.id
    )))
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    async fn setup() -> axum::Router {
        let app = router(AppState::in_memory());
        for (id, n) in [("one", 3), ("two", 5), ("three", 7)] {
            call(
                &app,
                "POST",
                "/api/v1/channels",
                &format!(r#"{{"id":"{id}","number":{n},"name":"{id}"}}"#),
            )
            .await;
        }
        app
    }

    #[tokio::test]
    async fn two_tvs_watch_different_channels() {
        let app = setup().await;
        call(
            &app,
            "POST",
            "/api/v1/tvs",
            r#"{"id":"living-room","name":"Living Room"}"#,
        )
        .await;
        call(
            &app,
            "POST",
            "/api/v1/tvs",
            r#"{"id":"bedroom","name":"Bedroom"}"#,
        )
        .await;
        call(
            &app,
            "POST",
            "/api/v1/tvs/living-room/channel",
            r#"{"number":3}"#,
        )
        .await;
        call(
            &app,
            "POST",
            "/api/v1/tvs/bedroom/channel",
            r#"{"number":7}"#,
        )
        .await;
        let (_, a) = call(&app, "GET", "/api/v1/tvs/living-room", "").await;
        let (_, b) = call(&app, "GET", "/api/v1/tvs/bedroom", "").await;
        assert_eq!(a["channel"], 3);
        assert_eq!(b["channel"], 7);
        assert_eq!(a["power"], true);
        assert_eq!(a["online"], false, "no player is connected");
    }

    #[tokio::test]
    async fn stepping_walks_real_channels_and_wraps() {
        let app = setup().await;
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let step = |dir: &'static str| {
            let app = app.clone();
            async move {
                let (_, tv) = call(
                    &app,
                    "POST",
                    "/api/v1/tvs/den/channel",
                    &format!(r#"{{"step":"{dir}"}}"#),
                )
                .await;
                tv["channel"].as_u64().unwrap()
            }
        };
        assert_eq!(step("up").await, 3);
        assert_eq!(step("up").await, 5);
        assert_eq!(step("up").await, 7);
        assert_eq!(step("up").await, 3);
        assert_eq!(step("down").await, 7);
    }

    #[tokio::test]
    async fn unknown_tv_or_channel_is_404_and_state_persists() {
        let app = setup().await;
        let (st, _) = call(&app, "POST", "/api/v1/tvs/nope/channel", r#"{"number":3}"#).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (st, _) = call(&app, "POST", "/api/v1/tvs/den/channel", r#"{"number":99}"#).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        call(&app, "POST", "/api/v1/tvs/den/channel", r#"{"number":5}"#).await;
        call(
            &app,
            "POST",
            "/api/v1/tvs",
            r#"{"id":"den","name":"The Den"}"#,
        )
        .await;
        let (_, tv) = call(&app, "GET", "/api/v1/tvs/den", "").await;
        assert_eq!(
            (tv["name"].as_str(), tv["channel"].as_u64()),
            (Some("The Den"), Some(5))
        );
        let (st, _) = call(&app, "POST", "/api/v1/tvs/den/channel", r#"{}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = call(&app, "POST", "/api/v1/tvs", r#"{"id":"Bad Id","name":"x"}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn power_and_volume() {
        let app = setup().await;
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/power", r#"{"on":true}"#).await;
        assert_eq!(tv["power"], true);
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/power", r#"{"on":false}"#).await;
        assert_eq!(tv["power"], false);

        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/volume", r#"{"muted":true}"#).await;
        assert_eq!(
            (tv["muted"].as_bool(), tv["volume"].as_u64()),
            (Some(true), Some(50))
        );
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/volume", r#"{"level":80}"#).await;
        assert_eq!(
            (tv["muted"].as_bool(), tv["volume"].as_u64()),
            (Some(false), Some(80)),
            "volume up un-mutes"
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/tvs/den/volume", r#"{"level":101}"#)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/tvs/den/volume", r#"{}"#)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/tvs/nope/volume", r#"{"level":1}"#)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn tv_url_redirects_to_the_tvs_current_channel() {
        let app = setup().await;
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        assert_eq!(
            call_raw(&app, "GET", "/tv/den.m3u8", "").await.0,
            StatusCode::CONFLICT,
            "off, nothing selected"
        );
        call(&app, "POST", "/api/v1/tvs/den/channel", r#"{"number":5}"#).await;
        let req = axum::http::Request::builder()
            .uri("/tv/den.m3u8")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(res.headers()["location"], "/stream/two/index.m3u8");
        // a different TV goes to a different channel
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"bed","name":"Bed"}"#).await;
        call(&app, "POST", "/api/v1/tvs/bed/channel", r#"{"number":7}"#).await;
        let req = axum::http::Request::builder()
            .uri("/tv/bed.m3u8")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(res.headers()["location"], "/stream/three/index.m3u8");
        assert_eq!(
            call_raw(&app, "GET", "/tv/nope.m3u8", "").await.0,
            StatusCode::NOT_FOUND
        );
    }
}
