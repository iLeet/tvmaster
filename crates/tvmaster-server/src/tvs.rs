//! Virtual TVs and the remote API. A TV is a registered player with a stable
//! id; the remote changes its channel. Different TVs watch different channels
//! at the same time.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tvmaster_core::Tv;

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<Tv>>> {
    Ok(Json(tvmaster_db::list_tvs(&s.db())?))
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
    if body.id.is_empty()
        || body.id.len() > 32
        || !body
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "id must be 1-32 chars of a-z, 0-9, '-' or '_'",
        ));
    }
    let db = s.db();
    // Re-registering keeps the TV's current channel and volume.
    let tv = match tvmaster_db::get_tv(&db, &body.id)? {
        Some(mut existing) => {
            existing.name = body.name;
            existing
        }
        None => Tv::new(body.id, body.name),
    };
    tvmaster_db::upsert_tv(&db, &tv)?;
    Ok((StatusCode::CREATED, Json(tv)))
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Tv>> {
    tvmaster_db::get_tv(&s.db(), &id)?
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such tv"))
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
    let db = s.db();
    let mut tv = tvmaster_db::get_tv(&db, &id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such tv"))?;
    let mut numbers: Vec<u32> = tvmaster_db::list_channels(&db)?
        .into_iter()
        .map(|c| c.number)
        .collect();
    numbers.sort_unstable();

    let target = match (body.number, body.step.as_deref()) {
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
    Ok(Json(tv))
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
    let db = s.db();
    let mut tv = tvmaster_db::get_tv(&db, &id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such tv"))?;
    tv.power = body.on;
    tvmaster_db::upsert_tv(&db, &tv)?;
    Ok(Json(tv))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
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
        assert_eq!(step("up").await, 3); // from nothing: first channel
        assert_eq!(step("up").await, 5);
        assert_eq!(step("up").await, 7);
        assert_eq!(step("up").await, 3); // wraps
        assert_eq!(step("down").await, 7); // wraps back
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
        // re-registering keeps the channel
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
    }

    #[tokio::test]
    async fn power_toggle() {
        let app = setup().await;
        call(&app, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/power", r#"{"on":true}"#).await;
        assert_eq!(tv["power"], true);
        let (_, tv) = call(&app, "POST", "/api/v1/tvs/den/power", r#"{"on":false}"#).await;
        assert_eq!(tv["power"], false);
    }
}
