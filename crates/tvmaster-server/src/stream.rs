//! HLS delivery. Many clients share one encode per channel; a channel starts
//! on the first playlist request and stops after the idle timeout.

use crate::autopilot;
use crate::plan::now_ms;
use crate::{ApiError, ApiResult, AppState};
use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tvmaster_playout::Manager;

/// How long to wait for a freshly started channel to produce its first segments.
const START_TIMEOUT: Duration = Duration::from_secs(25);

fn is_segment_name(name: &str) -> bool {
    name.strip_prefix("seg_")
        .and_then(|r| r.strip_suffix(".ts"))
        .map(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

/// Make sure `channel` is on air (starting it if needed) and count this
/// request as a viewer. Returns the playout manager.
fn ensure_on_air(s: &AppState, channel: &str, start: bool) -> ApiResult<Arc<Manager>> {
    let playout = s.playout.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available; streaming is disabled",
        )
    })?;
    if start && !playout.touch(channel) {
        // Decide under the database lock, but build the spec after releasing it:
        // building a guide channel's spec reads the database again.
        let ch = {
            let db = s.db();
            let Some(ch) = tvmaster_db::get_channel(&db, channel)? else {
                return Err(ApiError::new(StatusCode::NOT_FOUND, "no such channel"));
            };
            // The guide is drawn live; programming channels need media and a schedule.
            if ch.kind == tvmaster_core::ChannelKind::Programming {
                if autopilot::catalog_for(&db, &ch)?.is_empty() {
                    return Err(ApiError::new(
                        StatusCode::CONFLICT,
                        "channel has no playable media",
                    ));
                }
                // Make sure there is a schedule to follow before going on air.
                autopilot::maintain(&db, &ch, now_ms())?;
            }
            ch
        };
        playout.start_if_absent(s.channel_spec(&ch));
    }
    playout.touch(channel);
    Ok(playout)
}

pub async fn hls(
    State(s): State<AppState>,
    Path((channel, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    let is_playlist = file == "index.m3u8";
    if !is_playlist && !is_segment_name(&file) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not found"));
    }
    let playout = ensure_on_air(&s, &channel, is_playlist)?;

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

    // Clients that cannot send headers (VLC, Plex) resolve segment URLs without
    // the playlist's query, so each segment line carries the token itself.
    let bytes = if is_playlist && s.token.is_some() {
        let suffix = crate::auth::url_suffix(&s);
        String::from_utf8_lossy(&bytes)
            .lines()
            .map(|l| {
                if is_segment_name(l.trim()) {
                    format!("{l}{suffix}\n")
                } else {
                    format!("{l}\n")
                }
            })
            .collect::<String>()
            .into_bytes()
    } else {
        bytes
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
fn ensure_on_air_for_test(s: &AppState, channel: &str) -> ApiResult<Arc<Manager>> {
    ensure_on_air(s, channel, true)
}

/// The numbers of the segments listed in a playlist, in order.
fn playlist_segments(playlist: &str) -> Vec<(u64, String)> {
    playlist
        .lines()
        .filter(|l| is_segment_name(l.trim()))
        .filter_map(|l| {
            let name = l.trim();
            let n = name
                .strip_prefix("seg_")?
                .strip_suffix(".ts")?
                .parse()
                .ok()?;
            Some((n, name.to_string()))
        })
        .collect()
}

/// `/stream/{channel}.ts`: the channel as one continuous MPEG-TS stream, for
/// tuner-style clients (Plex, Jellyfin, Emby, VLC). It follows the HLS output,
/// starting a few seconds behind live, and keeps the channel on air while the
/// connection is open.
pub async fn ts(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Response> {
    let channel = name
        .strip_suffix(".ts")
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?
        .to_string();
    let playout = ensure_on_air(&s, &channel, true)?;
    let dir = playout.hls_dir(&channel);

    // Wait for the first segments, like the HLS playlist request does.
    let started = Instant::now();
    let first = loop {
        let text = tokio::fs::read_to_string(dir.join("index.m3u8"))
            .await
            .unwrap_or_default();
        let segs = playlist_segments(&text);
        if !segs.is_empty() {
            break segs;
        }
        if started.elapsed() > START_TIMEOUT {
            return Err(ApiError::new(StatusCode::NOT_FOUND, "not available yet"));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        // Begin two segments back so the client has something to decode at once.
        let mut last: Option<u64> = None;
        let mut pending: Vec<(u64, String)> = first[first.len().saturating_sub(2)..].to_vec();
        loop {
            for (n, file) in pending.drain(..) {
                if last.is_some_and(|l| n <= l) {
                    continue;
                }
                // A segment can vanish between listing and reading; skip it.
                if let Ok(bytes) = tokio::fs::read(dir.join(&file)).await {
                    if tx.send(Ok(Bytes::from(bytes))).await.is_err() {
                        return; // the client went away
                    }
                    last = Some(n);
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
            if tx.is_closed() {
                return;
            }
            playout.touch(&channel);
            let text = tokio::fs::read_to_string(dir.join("index.m3u8"))
                .await
                .unwrap_or_default();
            pending = playlist_segments(&text)
                .into_iter()
                .filter(|(n, _)| last.is_none_or(|l| *n > l))
                .collect();
        }
    });

    let mut res =
        Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};

    #[test]
    fn playlists_yield_their_segments_in_order() {
        let text = "#EXTM3U\n#EXTINF:2.0,\nseg_10.ts\n#EXTINF:2.0,\nseg_11.ts\nnot_a_segment.ts\n../seg_12.ts\n";
        assert_eq!(
            playlist_segments(text),
            vec![(10, "seg_10.ts".to_string()), (11, "seg_11.ts".to_string())]
        );
        assert!(playlist_segments("").is_empty());
    }

    /// Starting a guide channel builds its spec (which reads the database)
    /// after the request has finished with the database lock. Holding both
    /// once deadlocked the server on the first request for the guide.
    #[tokio::test(flavor = "multi_thread")]
    async fn starting_a_guide_channel_does_not_deadlock() {
        use tvmaster_core::{Channel, ChannelKind};
        let manager = tvmaster_playout::Manager::new(tvmaster_playout::EngineConfig::new(
            "ffmpeg-that-does-not-exist",
            tvmaster_media::Encoder::Software,
            std::env::temp_dir().join("tvmaster-deadlock-test"),
        ));
        let state = AppState::new(tvmaster_db::open_in_memory().unwrap(), None, Some(manager));
        let guide = Channel {
            id: "guide".into(),
            number: 3,
            name: "Guide".into(),
            strategy: Default::default(),
            run_out: Default::default(),
            autopilot: true,
            chaos: 0,
            seed: 1,
            epoch_ms: 0,
            always_on: false,
            library_ids: vec![],
            lineup: Default::default(),
            kind: ChannelKind::Guide,
            group: None,
            guide: Default::default(),
        };
        tvmaster_db::upsert_channel(&state.db(), &guide).unwrap();

        // The full request path, which decides under the database lock.
        let s = state.clone();
        let started = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || super::ensure_on_air_for_test(&s, "guide").is_ok()),
        )
        .await
        .expect("ensure_on_air deadlocked")
        .unwrap();
        assert!(started);
        state.playout.as_ref().unwrap().stop_all();
    }

    #[tokio::test]
    async fn ts_without_ffmpeg_is_503_and_rejects_other_names() {
        let app = router(AppState::in_memory());
        assert_eq!(
            call(&app, "GET", "/stream/x.ts", "").await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            call(&app, "GET", "/stream/x.txt", "").await.0,
            StatusCode::NOT_FOUND
        );
    }

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
