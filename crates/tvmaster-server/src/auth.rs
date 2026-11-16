//! Optional token authentication.
//!
//! With no token configured (the default) the server is open on the network,
//! as before. With one, every request must carry it, except the admin UI's
//! static files (so the login screen can load) and the auth endpoints.
//!
//! A token can be sent as `Authorization: Bearer <token>`, as `?token=<token>`
//! (for IPTV clients such as VLC and Plex that cannot set headers), or as the
//! `tvmaster_token` cookie that `POST /api/v1/auth/login` sets for browsers.

use crate::AppState;
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const COOKIE: &str = "tvmaster_token";

/// Tokens go into cookies and URLs, so only unreserved characters are allowed.
pub fn valid_token(t: &str) -> bool {
    (8..=128).contains(&t.len())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
}

/// A fresh random token (32 hex characters).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("the operating system has a random number source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

fn query_token(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

/// The token a request carries, if any.
fn presented(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| query.and_then(query_token))
        .or_else(|| cookie_token(headers))
}

/// Paths that never need the token.
fn is_public(path: &str) -> bool {
    path == "/"
        || path == "/admin"
        || path.starts_with("/admin/")
        || path == "/api/v1/health"
        || path == "/api/v1/auth"
        || path == "/api/v1/auth/login"
        || path == "/api/v1/auth/logout"
}

pub async fn guard(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let Some(token) = &s.token else {
        return next.run(req).await;
    };
    if is_public(req.uri().path()) {
        return next.run(req).await;
    }
    let ok =
        presented(req.headers(), req.uri().query()).is_some_and(|t| constant_time_eq(&t, token));
    if ok {
        return next.run(req).await;
    }
    let mut res = (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response();
    res.headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    res
}

/// `?token=...` to append to URLs handed to clients that cannot send headers
/// (empty when authentication is off).
pub fn url_suffix(s: &AppState) -> String {
    s.token
        .as_ref()
        .map(|t| format!("?token={t}"))
        .unwrap_or_default()
}

pub async fn status(State(s): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let authenticated = match &s.token {
        None => true,
        Some(t) => presented(&headers, None).is_some_and(|p| constant_time_eq(&p, t)),
    };
    Json(json!({ "required": s.token.is_some(), "authenticated": authenticated }))
}

#[derive(Deserialize)]
pub struct Login {
    token: String,
}

pub async fn login(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Response {
    match &s.token {
        None => Json(json!({ "ok": true })).into_response(),
        Some(t) if constant_time_eq(&body.token, t) => {
            let mut res = Json(json!({ "ok": true })).into_response();
            // Over HTTPS the cookie is marked Secure, so a browser never sends it in the clear.
            let secure = if crate::request_scheme(&headers) == "https" {
                "; Secure"
            } else {
                ""
            };
            let cookie = format!(
                "{COOKIE}={t}; Path=/; HttpOnly; SameSite=Strict{secure}; Max-Age=31536000"
            );
            res.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).expect("token is cookie-safe"),
            );
            res
        }
        Some(_) => {
            // Slow down guessing.
            tokio::time::sleep(Duration::from_millis(400)).await;
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "wrong token" })),
            )
                .into_response()
        }
    }
}

pub async fn logout() -> Response {
    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("tvmaster_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call_raw;
    use crate::{router, AppState};

    fn secured() -> axum::Router {
        router(AppState::in_memory().with_token(Some("s3cret-token-123".into())))
    }

    async fn get(app: &axum::Router, uri: &str, headers: &[(&str, &str)]) -> StatusCode {
        let mut b = axum::http::Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        tower::ServiceExt::oneshot(app.clone(), b.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[test]
    fn token_rules() {
        assert!(valid_token("abcdef12345"));
        assert!(valid_token(&generate_token()));
        for bad in [
            "short",
            "has space 12345",
            "semi;colon12345",
            "quote\"12345",
            &"x".repeat(129),
        ] {
            assert!(!valid_token(bad), "{bad}");
        }
        assert_ne!(generate_token(), generate_token());
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[tokio::test]
    async fn open_servers_stay_open() {
        let app = router(AppState::in_memory());
        assert_eq!(get(&app, "/api/v1/channels", &[]).await, StatusCode::OK);
        let (_, body) = call_raw(&app, "GET", "/api/v1/auth", "").await;
        assert!(body.contains("\"required\":false") && body.contains("\"authenticated\":true"));
    }

    #[tokio::test]
    async fn a_token_protects_everything_but_the_shell_and_login() {
        let app = secured();
        for uri in [
            "/api/v1/channels",
            "/api/v1/tvs",
            "/iptv/playlist.m3u",
            "/iptv/guide.xml",
            "/lineup.json",
            "/discover.json",
            "/stream/x/index.m3u8",
            "/stream/x.ts",
            "/api/v1/ws",
        ] {
            assert_eq!(get(&app, uri, &[]).await, StatusCode::UNAUTHORIZED, "{uri}");
        }
        for uri in ["/", "/admin/", "/api/v1/health", "/api/v1/auth"] {
            assert_eq!(get(&app, uri, &[]).await, StatusCode::OK, "{uri}");
        }
    }

    #[tokio::test]
    async fn header_query_and_cookie_all_work_and_wrong_tokens_do_not() {
        let app = secured();
        let t = "s3cret-token-123";
        assert_eq!(
            get(
                &app,
                "/api/v1/channels",
                &[("authorization", &format!("Bearer {t}"))]
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            get(&app, &format!("/iptv/playlist.m3u?token={t}"), &[]).await,
            StatusCode::OK
        );
        assert_eq!(
            get(
                &app,
                "/api/v1/channels",
                &[("cookie", &format!("a=b; tvmaster_token={t}; c=d"))]
            )
            .await,
            StatusCode::OK
        );
        for bad in ["nope", "s3cret-token-12", "s3cret-token-1234"] {
            assert_eq!(
                get(
                    &app,
                    "/api/v1/channels",
                    &[("authorization", &format!("Bearer {bad}"))]
                )
                .await,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                get(&app, &format!("/api/v1/channels?token={bad}"), &[]).await,
                StatusCode::UNAUTHORIZED
            );
        }
    }

    #[tokio::test]
    async fn login_sets_an_http_only_cookie_and_logout_clears_it() {
        let app = secured();
        let req = |body: &str| {
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap()
        };
        let res = tower::ServiceExt::oneshot(app.clone(), req(r#"{"token":"wrong-token-xx"}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let res = tower::ServiceExt::oneshot(app.clone(), req(r#"{"token":"s3cret-token-123"}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            cookie.contains("tvmaster_token=s3cret-token-123")
                && cookie.contains("HttpOnly")
                && cookie.contains("SameSite=Strict")
        );
        // the browser then sends it back
        let pair = cookie.split(';').next().unwrap();
        assert_eq!(
            get(&app, "/api/v1/channels", &[("cookie", pair)]).await,
            StatusCode::OK
        );
        let (_, st) = {
            let r = tower::ServiceExt::oneshot(
                app.clone(),
                axum::http::Request::builder()
                    .uri("/api/v1/auth")
                    .header("cookie", pair)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
            (
                r.status(),
                String::from_utf8_lossy(
                    &http_body_util::BodyExt::collect(r.into_body())
                        .await
                        .unwrap()
                        .to_bytes(),
                )
                .to_string(),
            )
        };
        assert!(st.contains("\"authenticated\":true"));
        let res = tower::ServiceExt::oneshot(
            app.clone(),
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert!(res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn playlists_and_segments_carry_the_token_for_clients_without_headers() {
        let state = AppState::in_memory().with_token(Some("s3cret-token-123".into()));
        let app = router(state);
        let t = "s3cret-token-123";
        let (st, _) = call_raw(
            &app,
            "POST",
            &format!("/api/v1/channels?token={t}"),
            r#"{"id":"a","number":1,"name":"A"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let (_, m3u) = call_raw(&app, "GET", &format!("/iptv/playlist.m3u?token={t}"), "").await;
        assert!(
            m3u.contains(&format!("/stream/a/index.m3u8?token={t}")),
            "{m3u}"
        );
        assert!(m3u.contains(&format!("guide.xml?token={t}")));
        let (_, lineup) = call_raw(&app, "GET", &format!("/lineup.json?token={t}"), "").await;
        assert!(lineup.contains(&format!("/stream/a.ts?token={t}")));
    }
}
