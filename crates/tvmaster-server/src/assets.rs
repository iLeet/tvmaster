//! Uploaded pictures (an off-air screen, a channel logo), so nobody has to type
//! a path that exists on the server. Files live in `<data dir>/assets/` under a
//! generated name; the API hands back the server path to put in the channel's
//! options, and a URL to preview it.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The biggest picture accepted.
pub const MAX_BYTES: usize = 12 * 1024 * 1024;

/// The format of an image by its first bytes (what the browser claims is not trusted).
fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("gif")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// `<hex>-<slug>.<ext>`: nothing in it can climb out of the folder.
fn valid_id(id: &str) -> bool {
    id.len() <= 90
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.contains("..")
        && id
            .rsplit('.')
            .next()
            .is_some_and(|e| matches!(e, "png" | "jpg" | "gif" | "webp"))
}

fn slug(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-');
    s.chars()
        .take(32)
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn dir(s: &AppState) -> ApiResult<PathBuf> {
    s.assets_dir.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_IMPLEMENTED,
            "uploads are not set up on this server",
        )
    })
}

fn describe(dir: &std::path::Path, id: &str, bytes: u64) -> Value {
    json!({
        "id": id,
        "path": dir.join(id).to_string_lossy(),
        "url": format!("/api/v1/assets/{id}"),
        "bytes": bytes,
    })
}

#[derive(Deserialize)]
pub struct UploadQuery {
    #[serde(default)]
    name: String,
}

/// `POST /api/v1/assets?name=logo.png` with the picture as the body.
pub async fn upload(
    State(s): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let dir = dir(&s)?;
    if body.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "no picture was sent",
        ));
    }
    if body.len() > MAX_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the picture is over 12 MB",
        ));
    }
    let ext = sniff(&body).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "that is not a png, jpg, gif or webp picture",
        )
    })?;
    // The same picture uploaded twice is one file.
    let hash = body.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    let name = slug(&q.name);
    let id = if name.is_empty() {
        format!("{hash:016x}.{ext}")
    } else {
        format!("{hash:016x}-{name}.{ext}")
    };
    tokio::fs::create_dir_all(&dir).await.map_err(io_error)?;
    let path = dir.join(&id);
    if !path.exists() {
        let tmp = dir.join(format!("{id}.part"));
        tokio::fs::write(&tmp, &body).await.map_err(io_error)?;
        tokio::fs::rename(&tmp, &path).await.map_err(io_error)?;
    }
    Ok(Json(describe(&dir, &id, body.len() as u64)))
}

fn io_error(e: std::io::Error) -> ApiError {
    tracing::error!("saving an upload failed: {e}");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "could not save the picture",
    )
}

/// `GET /api/v1/assets`: what has been uploaded, newest first.
pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let dir = dir(&s)?;
    let mut found: Vec<(std::time::SystemTime, Value)> = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let id = e.file_name().to_string_lossy().to_string();
            if !valid_id(&id) {
                continue;
            }
            if let Ok(m) = e.metadata().await {
                found.push((
                    m.modified().unwrap_or(std::time::UNIX_EPOCH),
                    describe(&dir, &id, m.len()),
                ));
            }
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    Ok(Json(Value::Array(found.into_iter().map(|f| f.1).collect())))
}

/// `GET /api/v1/assets/:id`: the picture itself.
pub async fn get_one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let dir = dir(&s)?;
    if !valid_id(&id) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such picture"));
    }
    let bytes = tokio::fs::read(dir.join(&id))
        .await
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "no such picture"))?;
    let ext = id.rsplit('.').next().unwrap_or("");
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime(ext)));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok((headers, bytes).into_response())
}

/// `DELETE /api/v1/assets/:id`.
pub async fn delete_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let dir = dir(&s)?;
    if !valid_id(&id) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such picture"));
    }
    tokio::fs::remove_file(dir.join(&id))
        .await
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "no such picture"))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_recognised_by_their_bytes() {
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0]),
            Some("png")
        );
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff(b"GIF89a....."), Some("gif"));
        assert_eq!(sniff(b"RIFF\x10\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"<svg></svg>"), None);
        assert_eq!(sniff(b"#!/bin/sh\nrm -rf /"), None);
    }

    #[test]
    fn ids_cannot_climb_out_of_the_folder() {
        assert!(valid_id("0123abcd-my-logo.png"));
        assert!(!valid_id("../etc/passwd"));
        assert!(!valid_id("a/b.png"));
        assert!(!valid_id("..png"));
        assert!(!valid_id("x.sh"));
    }

    async fn send(
        app: &axum::Router,
        method: &str,
        uri: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let req = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .body(axum::body::Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let st = res.status();
        (
            st,
            res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    #[tokio::test]
    async fn a_picture_can_be_uploaded_listed_fetched_and_deleted() {
        let dir = std::env::temp_dir().join(format!("tvm-assets-{}", crate::generate_token()));
        let app = crate::router(AppState::in_memory().with_assets_dir(&dir));
        let png = tvmaster_render::png::rgba_png(2, 2, &[255u8; 16]);

        let (st, body) = send(
            &app,
            "POST",
            "/api/v1/assets?name=My%20Logo.PNG",
            png.clone(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let v: Value = serde_json::from_slice(&body).unwrap();
        let id = v["id"].as_str().unwrap().to_string();
        assert!(id.ends_with("-my-logo.png"), "{id}");
        let path = PathBuf::from(v["path"].as_str().unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), png, "stored as sent");

        // The same bytes again are the same file.
        let (_, again) = send(
            &app,
            "POST",
            "/api/v1/assets?name=My%20Logo.PNG",
            png.clone(),
        )
        .await;
        assert_eq!(
            serde_json::from_slice::<Value>(&again).unwrap()["id"],
            v["id"]
        );

        let (st, list) = send(&app, "GET", "/api/v1/assets", vec![]).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(&list)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let (st, got) = send(&app, "GET", &format!("/api/v1/assets/{id}"), vec![]).await;
        assert_eq!((st, got), (StatusCode::OK, png));

        // Not a picture, empty, or a path trick.
        let (st, _) = send(&app, "POST", "/api/v1/assets", b"#!/bin/sh".to_vec()).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = send(&app, "POST", "/api/v1/assets", vec![]).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = send(&app, "GET", "/api/v1/assets/..%2F..%2Fetc%2Fpasswd", vec![]).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        let (st, _) = send(&app, "DELETE", &format!("/api/v1/assets/{id}"), vec![]).await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        assert!(!path.exists());
        let (st, _) = send(&app, "DELETE", &format!("/api/v1/assets/{id}"), vec![]).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn without_a_folder_uploads_are_off() {
        let app = crate::router(AppState::in_memory());
        let (st, _) = send(&app, "GET", "/api/v1/assets", vec![]).await;
        assert_eq!(st, StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn names_become_short_safe_slugs() {
        assert_eq!(slug("My Logo (final).PNG"), "my-logo--final");
        assert_eq!(slug("../../x.png"), "x");
        assert_eq!(slug(""), "");
    }
}
