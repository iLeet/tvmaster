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
/// Where a token set from the admin UI is kept.
pub const META_KEY: &str = "access_token";

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
    let Some(token) = s.token() else {
        return next.run(req).await;
    };
    if is_public(req.uri().path()) {
        return next.run(req).await;
    }
    let ok =
        presented(req.headers(), req.uri().query()).is_some_and(|t| constant_time_eq(&t, &token));
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
    s.token().map(|t| format!("?token={t}")).unwrap_or_default()
}

pub async fn status(State(s): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let token = s.token();
    let authenticated = match &token {
        None => true,
        Some(t) => presented(&headers, None).is_some_and(|p| constant_time_eq(&p, t)),
    };
    Json(json!({ "required": token.is_some(), "authenticated": authenticated }))
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
    match &s.token() {
        None => Json(json!({ "ok": true })).into_response(),
        Some(t) if constant_time_eq(&body.token, t) => {
            let mut res = Json(json!({ "ok": true })).into_response();
            set_cookie(&mut res, &headers, t);
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

/// Remember `token` in the browser (HttpOnly, and Secure over HTTPS so it is
/// never sent in the clear).
fn set_cookie(res: &mut Response, headers: &HeaderMap, token: &str) {
    let secure = if crate::request_scheme(headers) == "https" {
        "; Secure"
    } else {
        ""
    };
    let cookie =
        format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict{secure}; Max-Age=31536000");
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("token is cookie-safe"),
    );
}

/// `GET /api/v1/security`: is the token on, can it be changed here, and what
/// is it. Only reachable by someone who already has it (or when it is off).
pub async fn security(State(s): State<AppState>) -> Json<serde_json::Value> {
    let token = s.token();
    Json(json!({
        "enabled": token.is_some(),
        "locked": s.token_locked,
        "token": token,
    }))
}

#[derive(Deserialize)]
pub struct SecurityBody {
    enabled: bool,
    /// Use this token instead of a generated one (when enabling or replacing).
    #[serde(default)]
    token: Option<String>,
    /// Replace the current token with a fresh random one.
    #[serde(default)]
    rotate: bool,
}

/// `PUT /api/v1/security`: turn the access token on or off, or replace it.
/// The caller's browser is given the new cookie so it stays signed in.
pub async fn put_security(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SecurityBody>,
) -> Result<Response, crate::ApiError> {
    use crate::ApiError;
    if s.token_locked {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "the token was set with --token on the command line; restart without it to manage it here",
        ));
    }
    let current = s.token();
    let new = if !body.enabled {
        None
    } else if let Some(t) = body.token.filter(|t| !t.is_empty()) {
        if !valid_token(&t) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "a token is 8-128 characters: letters, digits, - _ . ~",
            ));
        }
        Some(t)
    } else if body.rotate || current.is_none() {
        Some(generate_token())
    } else {
        current
    };
    {
        let db = s.db();
        match &new {
            Some(t) => tvmaster_db::meta_set(&db, META_KEY, t),
            None => tvmaster_db::meta_set(&db, META_KEY, ""),
        }?;
    }
    *s.token.write().unwrap_or_else(|p| p.into_inner()) = new.clone();
    let mut res =
        Json(json!({ "enabled": new.is_some(), "locked": false, "token": new })).into_response();
    match &new {
        Some(t) => set_cookie(&mut res, &headers, t),
        None => {
            res.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_static(
                    "tvmaster_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
                ),
            );
        }
    }
    Ok(res)
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
    async fn the_token_can_be_turned_on_rotated_and_off_at_runtime() {
        use crate::test_util::call;
        let state = AppState::in_memory().with_token(None);
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/security", "").await;
        assert_eq!(
            (st, v["enabled"].as_bool(), v["locked"].as_bool()),
            (StatusCode::OK, Some(false), Some(false))
        );

        // Turn it on: a token is generated and saved.
        let (st, v) = call(&app, "PUT", "/api/v1/security", r#"{"enabled":true}"#).await;
        assert_eq!(st, StatusCode::OK);
        let token = v["token"].as_str().unwrap().to_string();
        assert!(valid_token(&token));
        assert_eq!(state.token().as_deref(), Some(token.as_str()));
        assert_eq!(
            get(&app, "/api/v1/channels", &[]).await,
            StatusCode::UNAUTHORIZED
        );
        let bearer = format!("Bearer {token}");
        assert_eq!(
            get(&app, "/api/v1/channels", &[("authorization", &bearer)]).await,
            StatusCode::OK
        );

        // It survives a restart (a new state on the same database would read it back).
        let saved = tvmaster_db::meta_get(&state.db(), META_KEY).unwrap();
        assert_eq!(saved.as_deref(), Some(token.as_str()));

        // Rotating needs the old token, and gives a new one.
        let put = |body: &str, auth: Option<&str>| {
            let mut b = axum::http::Request::builder()
                .method("PUT")
                .uri("/api/v1/security")
                .header("content-type", "application/json");
            if let Some(a) = auth {
                b = b.header("authorization", a);
            }
            b.body(axum::body::Body::from(body.to_string())).unwrap()
        };
        let res =
            tower::ServiceExt::oneshot(app.clone(), put(r#"{"enabled":true,"rotate":true}"#, None))
                .await
                .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let res = tower::ServiceExt::oneshot(
            app.clone(),
            put(r#"{"enabled":true,"rotate":true}"#, Some(&bearer)),
        )
        .await
        .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("tvmaster_token="));
        assert_ne!(state.token().unwrap(), token);

        // A chosen token must be valid.
        let bearer = format!("Bearer {}", state.token().unwrap());
        let res = tower::ServiceExt::oneshot(
            app.clone(),
            put(r#"{"enabled":true,"token":"short"}"#, Some(&bearer)),
        )
        .await
        .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let res = tower::ServiceExt::oneshot(
            app.clone(),
            put(
                r#"{"enabled":true,"token":"my-own-token-1"}"#,
                Some(&bearer),
            ),
        )
        .await
        .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(state.token().as_deref(), Some("my-own-token-1"));

        // Off again.
        let res = tower::ServiceExt::oneshot(
            app.clone(),
            put(r#"{"enabled":false}"#, Some("Bearer my-own-token-1")),
        )
        .await
        .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0"));
        assert_eq!(state.token(), None);
        assert_eq!(get(&app, "/api/v1/channels", &[]).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn a_saved_token_is_used_at_startup_and_a_command_line_one_is_locked() {
        let state = AppState::in_memory();
        tvmaster_db::meta_set(&state.db(), META_KEY, "saved-token-1234").unwrap();
        let state = state.with_token(None);
        assert_eq!(state.token().as_deref(), Some("saved-token-1234"));
        assert!(!state.token_locked);

        let locked = AppState::in_memory().with_token(Some("cli-token-12345".into()));
        let app = router(locked);
        let put = axum::http::Request::builder()
            .method("PUT")
            .uri("/api/v1/security")
            .header("content-type", "application/json")
            .header("authorization", "Bearer cli-token-12345")
            .body(axum::body::Body::from(r#"{"enabled":false}"#))
            .unwrap();
        let res = tower::ServiceExt::oneshot(app.clone(), put).await.unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let req = axum::http::Request::builder()
            .uri("/api/v1/security")
            .header("authorization", "Bearer cli-token-12345")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn an_installer_token_seeds_the_first_run_only() {
        let state = AppState::in_memory().with_token(None);
        state.seed_token(Some("installer-token-1".into()));
        assert_eq!(state.token().as_deref(), Some("installer-token-1"));
        // The owner turns it off; a restart with the same seed must respect that.
        tvmaster_db::meta_set(&state.db(), META_KEY, "").unwrap();
        let again = AppState::new(tvmaster_db::open_in_memory().unwrap(), None, None);
        tvmaster_db::meta_set(&again.db(), META_KEY, "").unwrap();
        let again = again.with_token(None);
        again.seed_token(Some("installer-token-1".into()));
        assert_eq!(again.token(), None, "a saved choice of 'off' wins");
        // Junk is ignored.
        let fresh = AppState::in_memory().with_token(None);
        fresh.seed_token(Some("no".into()));
        assert_eq!(fresh.token(), None);
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
