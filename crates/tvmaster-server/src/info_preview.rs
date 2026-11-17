//! A picture of one page of an info channel, drawn from the settings as they are being edited.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use tvmaster_core::{InfoConfig, ThemeChoice, Zone};

#[derive(Deserialize)]
pub struct Body {
    info: InfoConfig,
    /// The channel's name, for the heading.
    #[serde(default)]
    name: String,
    /// The channel, if it exists, so its look and its time zone are used.
    #[serde(default)]
    id: Option<String>,
    /// A look to draw it in, instead of the channel's.
    #[serde(default)]
    theme: Option<ThemeChoice>,
    /// Which page, counting from 0.
    #[serde(default)]
    page: usize,
}

/// `POST /api/v1/info/preview`: one page as a PNG (`x-page-count` says how many there are).
pub async fn preview(State(s): State<AppState>, Json(b): Json<Body>) -> ApiResult<Response> {
    b.info
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let Some(ffmpeg) = s.ffmpeg.as_ref().map(|f| f.ffmpeg.clone()) else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available",
        ));
    };
    let now = crate::plan::now_ms();
    let id = b.id.clone().unwrap_or_else(|| "preview".into());
    let (choice, zone, tonight, headlines) = {
        let db = s.db();
        let choice = b
            .theme
            .clone()
            .unwrap_or_else(|| crate::theme::effective_at(&db, &id, now));
        let zone = tvmaster_db::get_channel(&db, &id)
            .ok()
            .flatten()
            .and_then(|c| Zone::parse(&c.lineup.timezone))
            .or_else(|| Zone::parse(&crate::station::effective_timezone(&db)))
            .unwrap_or_else(Zone::utc);
        let tonight = crate::plan::tonight_rows_in(&db, &zone, b.info.hour12, now);
        let headlines = s
            .headlines
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(|e| e.items.clone())
            .unwrap_or_default();
        (choice, zone, tonight, headlines)
    };
    // Headlines that have not been fetched yet are stood in for, so the page can be judged.
    let headlines = if headlines.is_empty() && !b.info.feeds.is_empty() {
        vec![
            "Local news headlines appear here".to_string(),
            "Each one on its own line".to_string(),
            "Refreshed every fifteen minutes".to_string(),
        ]
    } else {
        headlines
    };
    let count = tvmaster_playout::info_page_count(&b.info, &headlines, &tonight);
    let (info, name, page) = (b.info, b.name, b.page);
    let png = tokio::task::spawn_blocking(move || {
        let th = tvmaster_playout::card_theme(&choice, &id);
        tvmaster_playout::info_preview_png(
            &ffmpeg, &info, &name, &th, &headlines, &tonight, zone, now, page,
        )
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut res = png.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-page-count", HeaderValue::from(count));
    Ok(res)
}
