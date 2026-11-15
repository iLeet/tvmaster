//! HLS delivery. Many clients share one encode per channel; a channel starts
//! on the first playlist request and stops after the idle timeout.

use crate::plan::build_spec;
use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::time::{Duration, Instant};

/// How long to wait for a freshly started channel to produce its first segments.
const START_TIMEOUT: Duration = Duration::from_secs(25);

fn is_segment_name(name: &str) -> bool {
    name.strip_prefix("seg_")
        .and_then(|r| r.strip_suffix(".ts"))
        .map(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

pub async fn hls(
    State(s): State<AppState>,
    Path((channel, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    let playout = s.playout.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available; streaming is disabled",
        )
    })?;
    let is_playlist = file == "index.m3u8";
    if !is_playlist && !is_segment_name(&file) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not found"));
    }

    if is_playlist && !playout.touch(&channel) {
        let spec = build_spec(&s.db(), &channel)?;
        let Some(spec) = spec else {
            let exists = tvmaster_db::get_channel(&s.db(), &channel)?.is_some();
            return Err(if exists {
                ApiError::new(StatusCode::CONFLICT, "channel has no playable media")
            } else {
                ApiError::new(StatusCode::NOT_FOUND, "no such channel")
            });
        };
        playout.start_if_absent(spec);
        playout.touch(&channel);
    } else {
        playout.touch(&channel);
    }

    let path = playout.hls_dir(&channel).join(&file);
    let started = Instant::now();
    let bytes = loop {
        match tokio::fs::read(&path).await {
            Ok(b) if !is_playlist || String::from_utf8_lossy(&b).contains("#EXTINF") => break b,
            _ if is_playlist && started.elapsed() < START_TIMEOUT => {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            _ => return Err(ApiError::new(StatusCode::NOT_FOUND, "not available yet")),
        }
    };

    let (content_type, cache) = if is_playlist {
        ("application/vnd.apple.mpegurl", "no-cache")
    } else {
        ("video/mp2t", "public, max-age=60")
    };
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};

    #[test]
    fn segment_name_validation() {
        assert!(is_segment_name("seg_123.ts"));
        assert!(!is_segment_name("seg_.ts"));
        assert!(!is_segment_name("seg_1.ts/../x"));
        assert!(!is_segment_name("../seg_1.ts"));
        assert!(!is_segment_name("index.m3u8"));
    }

    #[tokio::test]
    async fn streaming_without_ffmpeg_is_503() {
        let app = router(AppState::in_memory());
        let (st, _) = call(&app, "GET", "/stream/x/index.m3u8", "").await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    }
}
