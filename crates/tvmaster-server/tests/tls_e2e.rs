//! A real TLS listener, a real TLS client: the handshake, name checking,
//! certificate trust, HTTP/1.1 over it, and what the server writes into links
//! and cookies when the request came in over HTTPS.

use axum_server::Handle;
use rustls::pki_types::{CertificateDer, ServerName};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use tvmaster_server::{router, router_https, tls, AppState};

struct Server {
    addr: SocketAddr,
    cert_der: Vec<u8>,
    fingerprint: String,
    handle: Handle,
    dir: std::path::PathBuf,
}

async fn start(token: Option<&str>) -> Server {
    let dir = std::env::temp_dir().join(format!(
        "tvmaster-tls-e2e-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    let m = tls::load_or_create(&dir, None).unwrap();
    let state = AppState::in_memory().with_token(token.map(String::from));
    let handle = Handle::new();
    let (app, h, mm) = (router_https(state), handle.clone(), m.clone());
    tokio::spawn(async move {
        let _ = tls::serve("127.0.0.1:0".parse().unwrap(), app, &mm, h).await;
    });
    let addr = handle.listening().await.expect("listening");
    let mut pem = m.cert_pem.as_slice();
    let cert_der = rustls_pemfile::certs(&mut pem)
        .next()
        .unwrap()
        .unwrap()
        .to_vec();
    Server {
        addr,
        cert_der,
        fingerprint: m.fingerprint,
        handle,
        dir,
    }
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
}

fn client(trust: Option<&[u8]>) -> TlsConnector {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = rustls::RootCertStore::empty();
    if let Some(der) = trust {
        roots.add(CertificateDer::from(der.to_vec())).unwrap();
    }
    TlsConnector::from(Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// One HTTP/1.1 request over TLS; returns (status line, headers, body).
async fn get(
    s: &Server,
    trust: Option<&[u8]>,
    name: &str,
    request: &str,
) -> std::io::Result<(String, String, String)> {
    let tcp = tokio::net::TcpStream::connect(s.addr).await?;
    let mut stream = client(trust)
        .connect(ServerName::try_from(name.to_string()).unwrap(), tcp)
        .await?;
    stream.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    // The server closes the connection after `Connection: close`.
    let _ = stream.read_to_end(&mut raw).await;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let (status, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    Ok((status.to_string(), headers.to_string(), body.to_string()))
}

fn req(method: &str, path: &str, host: &str, extra: &str, body: &str) -> String {
    format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body}", body.len())
}

#[tokio::test]
async fn a_client_that_trusts_the_certificate_gets_answers_over_tls() {
    let s = start(None).await;
    let (status, _, body) = get(
        &s,
        Some(&s.cert_der),
        "localhost",
        &req("GET", "/api/v1/health", "localhost", "", ""),
    )
    .await
    .unwrap();
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("\"status\":\"ok\""), "{body}");
    s.handle.shutdown();
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[tokio::test]
async fn a_client_that_does_not_trust_it_cannot_connect() {
    let s = start(None).await;
    let err = get(
        &s,
        None,
        "localhost",
        &req("GET", "/api/v1/health", "localhost", "", ""),
    )
    .await;
    assert!(err.is_err(), "an unknown certificate must be refused");
    s.handle.shutdown();
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[tokio::test]
async fn the_certificate_is_checked_against_the_name_asked_for() {
    let s = start(None).await;
    let r = get(
        &s,
        Some(&s.cert_der),
        "not-this-server.example",
        &req("GET", "/api/v1/health", "x", "", ""),
    )
    .await;
    assert!(
        r.is_err(),
        "a name the certificate does not list must be refused"
    );
    // but the loopback address it does list works
    let ok = get(
        &s,
        Some(&s.cert_der),
        "127.0.0.1",
        &req("GET", "/api/v1/health", "127.0.0.1", "", ""),
    )
    .await;
    assert!(ok.is_ok());
    s.handle.shutdown();
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[tokio::test]
async fn links_and_cookies_written_over_https_say_https() {
    let token = "abcdefgh12345678";
    let s = start(Some(token)).await;
    let host = format!("tv.example:{}", s.addr.port());
    let (_, _, m3u) = get(
        &s,
        Some(&s.cert_der),
        "localhost",
        &req(
            "GET",
            &format!("/iptv/playlist.m3u?token={token}"),
            &host,
            "",
            "",
        ),
    )
    .await
    .unwrap();
    assert!(
        m3u.contains(&format!("https://{host}/iptv/guide.xml")),
        "{m3u}"
    );
    assert!(!m3u.contains("http://"), "{m3u}");

    let (st, headers, _) = get(
        &s,
        Some(&s.cert_der),
        "localhost",
        &req(
            "POST",
            "/api/v1/auth/login",
            &host,
            "Content-Type: application/json\r\n",
            &format!("{{\"token\":\"{token}\"}}"),
        ),
    )
    .await
    .unwrap();
    assert!(st.contains("200"), "{st}");
    let cookie = headers
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie"))
        .unwrap();
    assert!(
        cookie.contains("Secure")
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );

    // and the token is still required
    let (st, _, _) = get(
        &s,
        Some(&s.cert_der),
        "localhost",
        &req("GET", "/api/v1/channels", &host, "", ""),
    )
    .await
    .unwrap();
    assert!(st.contains("401"), "{st}");
    s.handle.shutdown();
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[tokio::test]
async fn the_fingerprint_clients_pin_is_the_one_the_server_presents() {
    let s = start(None).await;
    assert_eq!(tls::fingerprint(&s.cert_der), s.fingerprint);
    s.handle.shutdown();
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[tokio::test]
async fn plain_http_links_stay_http_and_a_forged_scheme_header_is_ignored() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    let app = router(AppState::in_memory());
    let res = app
        .oneshot(
            Request::builder()
                .uri("/iptv/playlist.m3u")
                .header("host", "tv.example:8787")
                .header("x-tvmaster-scheme", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = http_body_util::BodyExt::collect(res.into_body())
        .await
        .unwrap()
        .to_bytes();
    let m3u = String::from_utf8_lossy(&bytes);
    assert!(
        m3u.contains("http://tv.example:8787/iptv/guide.xml"),
        "{m3u}"
    );
    assert!(!m3u.contains("https://"), "{m3u}");
}
