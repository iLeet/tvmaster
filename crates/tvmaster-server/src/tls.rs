//! HTTPS. A listener that serves the same router over TLS, with either a
//! certificate you provide (Let's Encrypt, Tailscale, your own CA) or a
//! self-signed one made on first use and kept in the data folder.
//!
//! A self-signed certificate protects the access token from anyone sniffing
//! the network, but nothing vouches for the server's identity: clients either
//! trust it explicitly (browsers: click through once; the Mac app: it shows
//! the fingerprint and remembers your decision) or pin its fingerprint.

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A generated certificate is replaced when it has less than this left.
const RENEW_BEFORE_SECS: i64 = 30 * 24 * 3600;
const VALID_DAYS: i64 = 3 * 365;
/// A provided certificate is re-read this often, so renewals are picked up.
const RELOAD_EVERY: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Clone)]
pub struct TlsMaterial {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    /// SHA-256 of the certificate, as `AA:BB:...`: what clients compare.
    pub fingerprint: String,
    pub self_signed: bool,
    /// Where a provided certificate lives, so it can be re-read.
    pub files: Option<(PathBuf, PathBuf)>,
}

/// SHA-256 of a DER certificate as colon-separated upper-case hex.
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The fingerprint of the first certificate in a PEM file.
pub fn fingerprint_of_pem(pem: &[u8]) -> Result<String, String> {
    let mut reader = pem;
    let cert = rustls_pemfile::certs(&mut reader)
        .next()
        .ok_or("no certificate found in the PEM data")?
        .map_err(|e| format!("reading the certificate: {e}"))?;
    Ok(fingerprint(cert.as_ref()))
}

/// This machine's name and its IPv4 addresses on a network. IPv6 is left out on
/// purpose: the many temporary privacy addresses change daily and would only clutter links.
pub fn host_names() -> (String, Vec<IpAddr>) {
    let host = gethostname::gethostname().to_string_lossy().to_string();
    let mut ips: Vec<IpAddr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.ip())
        .filter(|ip| matches!(ip, IpAddr::V4(v) if !v.is_loopback() && !v.is_link_local() && !v.is_unspecified()))
        .collect();
    ips.sort_by_key(|ip| ip.to_string());
    ips.dedup();
    (host, ips)
}

/// Everything a certificate for this machine should be valid for.
pub fn certificate_names() -> Vec<String> {
    let (host, ips) = host_names();
    let mut names = vec!["localhost".to_string(), "127.0.0.1".into(), "::1".into()];
    if !host.is_empty() {
        let short = host.split('.').next().unwrap_or(&host).to_string();
        names.push(short.clone());
        names.push(format!("{short}.local"));
        if host != short {
            names.push(host);
        }
    }
    names.extend(ips.iter().map(|i| i.to_string()));
    names.sort();
    names.dedup();
    names
}

fn generate(names: &[String]) -> Result<(String, String, i64), String> {
    use rcgen::{
        CertificateParams, DnType, ExtendedKeyUsagePurpose, Ia5String, KeyPair, KeyUsagePurpose,
        SanType,
    };
    let mut params = CertificateParams::default();
    params.subject_alt_names = names
        .iter()
        .map(|n| match n.parse::<IpAddr>() {
            Ok(ip) => Ok(SanType::IpAddress(ip)),
            Err(_) => Ia5String::try_from(n.clone())
                .map(SanType::DnsName)
                .map_err(|e| format!("bad name {n}: {e}")),
        })
        .collect::<Result<_, _>>()?;
    params
        .distinguished_name
        .push(DnType::CommonName, "TVMaster");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "TVMaster (self-signed)");
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let now = chrono::Utc::now();
    let ymd = |t: chrono::DateTime<chrono::Utc>| {
        use chrono::Datelike;
        rcgen::date_time_ymd(t.year(), t.month() as u8, t.day() as u8)
    };
    // Back-dated a day so a client with a slightly slow clock still accepts it.
    params.not_before = ymd(now - chrono::Duration::days(1));
    let not_after = now + chrono::Duration::days(VALID_DAYS);
    params.not_after = ymd(not_after);
    let key = KeyPair::generate().map_err(|e| format!("making a key: {e}"))?;
    let cert = params
        .self_signed(&key)
        .map_err(|e| format!("signing: {e}"))?;
    Ok((cert.pem(), key.serialize_pem(), not_after.timestamp()))
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("writing {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// The certificate to serve. `provided` wins; otherwise a self-signed one is
/// loaded from `<dir>/tls/` or made there (and renewed before it expires, which
/// is the only time its fingerprint changes).
pub fn load_or_create(dir: &Path, provided: Option<(&Path, &Path)>) -> Result<TlsMaterial, String> {
    if let Some((cert, key)) = provided {
        let cert_pem =
            std::fs::read(cert).map_err(|e| format!("reading {}: {e}", cert.display()))?;
        let key_pem = std::fs::read(key).map_err(|e| format!("reading {}: {e}", key.display()))?;
        let fingerprint = fingerprint_of_pem(&cert_pem)?;
        return Ok(TlsMaterial {
            cert_pem,
            key_pem,
            fingerprint,
            self_signed: false,
            files: Some((cert.into(), key.into())),
        });
    }
    let tls_dir = dir.join("tls");
    let (cert_path, key_path, meta_path) = (
        tls_dir.join("cert.pem"),
        tls_dir.join("key.pem"),
        tls_dir.join("cert.json"),
    );
    let now = chrono::Utc::now().timestamp();
    if let (Ok(cert_pem), Ok(key_pem), Ok(meta)) = (
        std::fs::read(&cert_path),
        std::fs::read(&key_path),
        std::fs::read_to_string(&meta_path),
    ) {
        let not_after = serde_json::from_str::<serde_json::Value>(&meta)
            .ok()
            .and_then(|v| v["not_after"].as_i64());
        if let (Some(na), Ok(fp)) = (not_after, fingerprint_of_pem(&cert_pem)) {
            if na - now > RENEW_BEFORE_SECS {
                return Ok(TlsMaterial {
                    cert_pem,
                    key_pem,
                    fingerprint: fp,
                    self_signed: true,
                    files: None,
                });
            }
        }
    }
    std::fs::create_dir_all(&tls_dir)
        .map_err(|e| format!("creating {}: {e}", tls_dir.display()))?;
    let names = certificate_names();
    let (cert_pem, key_pem, not_after) = generate(&names)?;
    write_private(&key_path, key_pem.as_bytes())?;
    std::fs::write(&cert_path, &cert_pem)
        .map_err(|e| format!("writing {}: {e}", cert_path.display()))?;
    let meta = serde_json::json!({ "not_after": not_after, "names": names });
    std::fs::write(&meta_path, meta.to_string())
        .map_err(|e| format!("writing {}: {e}", meta_path.display()))?;
    let fingerprint = fingerprint_of_pem(cert_pem.as_bytes())?;
    Ok(TlsMaterial {
        cert_pem: cert_pem.into_bytes(),
        key_pem: key_pem.into_bytes(),
        fingerprint,
        self_signed: true,
        files: None,
    })
}

/// Serve `app` over TLS until `handle` shuts it down.
pub async fn serve(
    addr: SocketAddr,
    app: Router,
    material: &TlsMaterial,
    handle: Handle,
) -> std::io::Result<()> {
    // rustls needs to be told which crypto backend to use; ignore "already set".
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config =
        RustlsConfig::from_pem(material.cert_pem.clone(), material.key_pem.clone()).await?;
    if let Some((cert, key)) = material.files.clone() {
        let config = config.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(RELOAD_EVERY).await;
                match config.reload_from_pem_file(&cert, &key).await {
                    Ok(()) => tracing::info!("reloaded the TLS certificate"),
                    Err(e) => tracing::warn!("could not reload the TLS certificate: {e}"),
                }
            }
        });
    }
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(app.into_make_service())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tvmaster-tls-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_certificate_is_made_once_and_then_reused_with_the_same_fingerprint() {
        let d = tmp("reuse");
        let a = load_or_create(&d, None).unwrap();
        assert!(a.self_signed);
        let b = load_or_create(&d, None).unwrap();
        assert_eq!(
            a.fingerprint, b.fingerprint,
            "a restart must not change the certificate clients have pinned"
        );
        assert_eq!(a.cert_pem, b.cert_pem);
        // AA:BB:... with 32 bytes
        assert_eq!(a.fingerprint.split(':').count(), 32);
        assert!(a
            .fingerprint
            .chars()
            .all(|c| c == ':' || c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_only_readable_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("perm");
        load_or_create(&d, None).unwrap();
        let mode = std::fs::metadata(d.join("tls/key.pem"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "key is {mode:o}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_certificate_close_to_expiry_is_replaced() {
        let d = tmp("renew");
        let a = load_or_create(&d, None).unwrap();
        // Say it expires in a week.
        let soon = chrono::Utc::now().timestamp() + 7 * 24 * 3600;
        std::fs::write(
            d.join("tls/cert.json"),
            format!(r#"{{"not_after":{soon},"names":[]}}"#),
        )
        .unwrap();
        let b = load_or_create(&d, None).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        // and the replacement is good for years again
        let c = load_or_create(&d, None).unwrap();
        assert_eq!(b.fingerprint, c.fingerprint);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_damaged_certificate_is_replaced_not_fatal() {
        let d = tmp("damaged");
        let a = load_or_create(&d, None).unwrap();
        std::fs::write(d.join("tls/cert.pem"), b"garbage").unwrap();
        let b = load_or_create(&d, None).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_provided_certificate_is_used_as_is() {
        let d = tmp("provided");
        let made = load_or_create(&d, None).unwrap();
        let (cert, key) = (d.join("tls/cert.pem"), d.join("tls/key.pem"));
        let other = tmp("provided-empty");
        let p = load_or_create(&other, Some((&cert, &key))).unwrap();
        assert!(!p.self_signed);
        assert_eq!(p.fingerprint, made.fingerprint);
        assert!(
            !other.join("tls").exists(),
            "nothing is generated when you bring your own"
        );
        assert!(load_or_create(&other, Some((&d.join("nope.pem"), &key))).is_err());
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn names_cover_localhost_and_this_machine() {
        let names = certificate_names();
        for want in ["localhost", "127.0.0.1", "::1"] {
            assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
        }
        let (host, _) = host_names();
        if !host.is_empty() {
            let short = host.split('.').next().unwrap().to_string();
            assert!(names.contains(&format!("{short}.local")));
        }
    }

    #[test]
    fn fingerprints_are_stable_and_formatted() {
        assert_eq!(
            fingerprint(b""),
            "E3:B0:C4:42:98:FC:1C:14:9A:FB:F4:C8:99:6F:B9:24:27:AE:41:E4:64:9B:93:4C:A4:95:99:1B:78:52:B8:55"
        );
        assert!(fingerprint_of_pem(b"no pem here").is_err());
    }
}
