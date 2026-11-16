//! A picture of what a guide would look like, so the editors can show it while
//! you change the settings (nothing has to be saved or on air).

use crate::{plan, ApiError, ApiResult, AppState};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use tvmaster_core::{GuideConfig, PipMode};
use tvmaster_render::GuideView;

#[derive(Deserialize)]
pub struct Body {
    /// The channel's name (for the default heading).
    #[serde(default)]
    name: String,
    /// The channel's id (so "auto" colours match what will air).
    #[serde(default)]
    id: String,
    #[serde(default)]
    guide: GuideConfig,
    /// Seconds into the stream (the ticker and the scrolling move).
    #[serde(default)]
    t: f32,
}

/// `POST /api/v1/guide/preview`: one frame, as a PNG.
pub async fn preview(State(s): State<AppState>, Json(body): Json<Body>) -> ApiResult<Response> {
    body.guide
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let png = tokio::task::spawn_blocking(move || {
        let id = if body.id.is_empty() {
            "guide"
        } else {
            &body.id
        };
        let name = if body.name.trim().is_empty() {
            "Guide"
        } else {
            &body.name
        };
        let data = {
            let db = s.db();
            let tz = crate::channels::system_timezone();
            plan::build_guide_with(&db, name, &tz, &body.guide, plan::now_ms())?
        };
        let look = match &s.ffmpeg {
            Some(f) => tvmaster_playout::guide_look_with_pictures(&body.guide, id, &f.ffmpeg),
            None => tvmaster_playout::guide_look(&body.guide, id),
        };
        // A window shows its card for the first channel, with no video behind it.
        let view = GuideView {
            window_row: (body.guide.pip.mode != PipMode::Off && !data.rows.is_empty()).then_some(0),
            window_live: false,
        };
        let c = tvmaster_render::render_guide(&data, &look, &view, body.t.clamp(0.0, 3600.0));
        let mut rgba = Vec::with_capacity(c.w * c.h * 4);
        for px in c.px.as_chunks::<3>().0 {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        Some(tvmaster_render::png::rgba_png(
            c.w as u32, c.h as u32, &rgba,
        ))
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not draw the guide",
        )
    })?;
    let mut res = png.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use crate::test_util::call_raw;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn a_preview_is_a_png_of_the_settings_it_was_given() {
        let app = router(AppState::in_memory());
        let (st, a) = call_raw(&app, "POST", "/api/v1/guide/preview", r#"{"name":"Retro"}"#).await;
        assert_eq!(st, StatusCode::OK);
        assert!(a.starts_with("\u{fffd}PNG"), "a PNG (lossily read as text)");
        let (_, b) = call_raw(
            &app,
            "POST",
            "/api/v1/guide/preview",
            r#"{"name":"Retro","guide":{"theme":"amber","layout":{"style":"now_next"}}}"#,
        )
        .await;
        assert_ne!(a, b, "other settings, other picture");
        let (st, e) = call_raw(
            &app,
            "POST",
            "/api/v1/guide/preview",
            r#"{"guide":{"layout":{"rows":1}}}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(e.contains("rows"), "{e}");
    }
}
