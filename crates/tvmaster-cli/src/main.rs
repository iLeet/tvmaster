mod service;
mod tools;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tvmaster_media::{find_system_font, Encoder, FfmpegInfo};
use tvmaster_playout::{probe_encoder, EngineConfig, Manager};
use tvmaster_server::AppState;

#[derive(Parser)]
#[command(
    name = "tvmaster",
    version,
    about = "Self-hosted retro TV station and IPTV engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon (headless): API, streams and web player.
    Serve {
        /// Address to listen on.
        #[arg(long, default_value = "0.0.0.0:8787")]
        bind: String,
        /// Where the database and HLS output live.
        #[arg(long, default_value = "tvmaster-data", env = "TVMASTER_DATA_DIR")]
        data_dir: PathBuf,
        /// ffmpeg binary (name on PATH or full path).
        #[arg(long, default_value = "ffmpeg", env = "TVMASTER_FFMPEG")]
        ffmpeg: PathBuf,
        /// ffprobe binary (name on PATH or full path).
        #[arg(long, default_value = "ffprobe", env = "TVMASTER_FFPROBE")]
        ffprobe: PathBuf,
        /// Video encoder: software, videotoolbox, nvenc, qsv, vaapi or amf.
        /// Default: auto (VideoToolbox on a Mac, otherwise software). A hardware
        /// encoder that fails its start-up test falls back to software.
        #[arg(long, env = "TVMASTER_ENCODER")]
        encoder: Option<String>,
        /// Stop an on-demand channel after this many seconds without viewers.
        #[arg(long, default_value_t = 60, env = "TVMASTER_IDLE_SECONDS")]
        idle_seconds: u64,
        /// Require this token on every request (8-128 letters, digits, - _ . ~).
        /// Without one the server is open to the whole network.
        #[arg(long, env = "TVMASTER_TOKEN")]
        token: Option<String>,
        /// Read the token from this file (the first line), so it stays out of
        /// process listings and service definitions.
        #[arg(long, env = "TVMASTER_TOKEN_FILE", conflicts_with = "token")]
        token_file: Option<PathBuf>,
        /// Start with the token in this file the first time only; after that the
        /// admin UI can turn it off or replace it (a saved choice wins).
        #[arg(long, env = "TVMASTER_INITIAL_TOKEN_FILE", conflicts_with_all = ["token", "token_file"])]
        initial_token_file: Option<PathBuf>,
        /// Also serve HTTPS (on --tls-bind) with a self-signed certificate that is
        /// made once and kept in the data folder. Clients trust it by its fingerprint.
        #[arg(long, env = "TVMASTER_TLS")]
        tls: bool,
        /// Address for HTTPS. Setting it turns HTTPS on.
        #[arg(long, env = "TVMASTER_TLS_BIND")]
        tls_bind: Option<String>,
        /// Use your own certificate (PEM) instead of a self-signed one, for example
        /// from Let's Encrypt or `tailscale cert`. Needs --tls-key. It is re-read
        /// every few hours so renewals are picked up.
        #[arg(long, env = "TVMASTER_TLS_CERT", requires = "tls_key")]
        tls_cert: Option<PathBuf>,
        #[arg(long, env = "TVMASTER_TLS_KEY", requires = "tls_cert")]
        tls_key: Option<PathBuf>,
        /// Do not open the plain-HTTP port; serve only HTTPS. IPTV apps and many
        /// smart-TV browsers cannot use a self-signed certificate, so check yours can.
        #[arg(long, env = "TVMASTER_NO_HTTP")]
        no_http: bool,
    },
    /// Print a new random access token to use with --token.
    Token,
    /// Check FFmpeg and test every video encoder on this machine.
    Doctor {
        /// ffmpeg binary (name on PATH or full path).
        #[arg(long, default_value = "ffmpeg", env = "TVMASTER_FFMPEG")]
        ffmpeg: PathBuf,
        /// ffprobe binary (name on PATH or full path).
        #[arg(long, default_value = "ffprobe", env = "TVMASTER_FFPROBE")]
        ffprobe: PathBuf,
    },
    /// Print a service definition for running TVMaster unattended.
    Service {
        #[arg(value_enum)]
        kind: service::Kind,
        /// Path to the tvmaster binary.
        #[arg(long)]
        bin: Option<PathBuf>,
        #[arg(long, default_value = "tvmaster-data")]
        data_dir: PathBuf,
        #[arg(long, default_value = "0.0.0.0:8787")]
        bind: String,
        /// Have the service start with the access token in this file (the admin
        /// UI can turn it off or replace it afterwards).
        #[arg(long)]
        token_file: Option<PathBuf>,
        /// Have the service also serve HTTPS.
        #[arg(long)]
        tls: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Serve {
            bind,
            data_dir,
            ffmpeg,
            ffprobe,
            encoder,
            idle_seconds,
            token,
            token_file,
            initial_token_file,
            tls,
            tls_bind,
            tls_cert,
            tls_key,
            no_http,
        } => {
            let first_line = |f: &PathBuf| -> Result<String> {
                Ok(std::fs::read_to_string(f)
                    .with_context(|| format!("reading the token file {}", f.display()))?
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string())
            };
            let token = match &token_file {
                Some(f) => Some(first_line(f)?),
                None => token,
            };
            let initial_token = match &initial_token_file {
                Some(f) => Some(first_line(f)?),
                None => None,
            };
            serve(ServeOptions {
                bind,
                data_dir,
                ffmpeg,
                ffprobe,
                encoder,
                idle_seconds,
                token,
                initial_token,
                tls: tls || tls_bind.is_some() || tls_cert.is_some(),
                tls_bind: tls_bind.unwrap_or_else(|| "0.0.0.0:8788".into()),
                tls_files: tls_cert.zip(tls_key),
                no_http,
            })
            .await
        }
        Command::Doctor { ffmpeg, ffprobe } => doctor(&ffmpeg, &ffprobe),
        Command::Token => {
            println!("{}", tvmaster_server::generate_token());
            Ok(())
        }
        Command::Service {
            kind,
            bin,
            data_dir,
            bind,
            token_file,
            tls,
        } => {
            let bin = match bin {
                Some(b) => b,
                None => std::env::current_exe().context("cannot locate this binary")?,
            };
            let data_dir = std::path::absolute(&data_dir).unwrap_or(data_dir);
            let token_file = token_file.map(|f| std::path::absolute(&f).unwrap_or(f));
            print!(
                "{}",
                service::render(kind, &bin, &data_dir, &bind, token_file.as_deref(), tls)
            );
            Ok(())
        }
    }
}

struct ServeOptions {
    bind: String,
    data_dir: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    encoder: Option<String>,
    idle_seconds: u64,
    token: Option<String>,
    initial_token: Option<String>,
    tls: bool,
    tls_bind: String,
    tls_files: Option<(PathBuf, PathBuf)>,
    no_http: bool,
}

fn port_of(addr: &str) -> Option<u16> {
    addr.rsplit(':').next().and_then(|p| p.parse().ok())
}

async fn serve(o: ServeOptions) -> Result<()> {
    let ServeOptions {
        bind,
        data_dir,
        ffmpeg,
        ffprobe,
        encoder,
        idle_seconds,
        token,
        initial_token,
        tls,
        tls_bind,
        tls_files,
        no_http,
    } = o;
    anyhow::ensure!(tls || !no_http, "--no-http needs HTTPS: add --tls");
    if let Some(t) = &token {
        anyhow::ensure!(
            tvmaster_server::valid_token(t),
            "the token must be 8-128 characters: letters, digits, - _ . ~ (run `tvmaster token` for one)"
        );
    }
    if let Some(t) = &initial_token {
        anyhow::ensure!(
            tvmaster_server::valid_token(t),
            "the initial token must be 8-128 characters: letters, digits, - _ . ~"
        );
    }
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let conn = tvmaster_db::open(data_dir.join("tvmaster.db"))?;
    let (ffmpeg, ffprobe) = (
        tools::resolve(ffmpeg, "ffmpeg"),
        tools::resolve(ffprobe, "ffprobe"),
    );

    let requested = match &encoder {
        Some(name) => Some(Encoder::parse(name).with_context(|| {
            format!("unknown encoder '{name}' (use software, videotoolbox or nvenc)")
        })?),
        None => None,
    };

    let (info, playout) = match FfmpegInfo::detect(&ffmpeg, &ffprobe) {
        Ok(info) => {
            let mut enc = info.choose_encoder(requested);
            if enc != Encoder::Software {
                // Compiled in does not mean the hardware is there.
                let probe = probe_encoder(&info.ffmpeg, enc);
                if !probe.ok {
                    tracing::warn!(
                        encoder = enc.name(),
                        "the {} encoder failed its test ({}); using software encoding. Run `tvmaster doctor` for details",
                        enc.name(),
                        probe.error.as_deref().unwrap_or("unknown error")
                    );
                    enc = Encoder::Software;
                }
            }
            tracing::info!(version = %info.version, encoder = enc.name(), "ffmpeg ready");
            let mut cfg = EngineConfig::new(&info.ffmpeg, enc, &data_dir);
            cfg.readrate_burst = info.has_readrate_burst;
            cfg.idle_timeout = Duration::from_secs(idle_seconds);
            if info.has_drawtext {
                cfg.slate_font = find_system_font();
            }
            let manager = Manager::new(cfg);
            (Some(info), Some(manager))
        }
        Err(e) => {
            tracing::warn!("ffmpeg unavailable, streaming disabled: {e:#}");
            (None, None)
        }
    };

    // HTTPS: the certificate is made (or loaded) before anything starts listening.
    let tls_material = if tls {
        let provided = tls_files.as_ref().map(|(c, k)| (c.as_path(), k.as_path()));
        let m = tvmaster_server::tls::load_or_create(&data_dir, provided)
            .map_err(|e| anyhow::anyhow!("TLS certificate: {e}"))?;
        Some(m)
    } else {
        None
    };
    let network = tvmaster_server::NetworkInfo {
        http_port: if no_http { None } else { port_of(&bind) },
        https_port: if tls { port_of(&tls_bind) } else { None },
        tls_fingerprint: tls_material.as_ref().map(|m| m.fingerprint.clone()),
        tls_self_signed: tls_material.as_ref().is_some_and(|m| m.self_signed),
    };
    let state = AppState::new(conn, info, playout.clone())
        .with_token(token)
        .with_assets_dir(data_dir.join("assets"))
        .with_network(network);
    state.seed_token(initial_token);
    if state.access_token().is_none() && !bind.starts_with("127.") && !bind.starts_with("localhost")
    {
        tracing::warn!(
            "no access token is set: anyone on your network can control this server (turn one on in Settings)"
        );
    }
    // Keeps every channel's schedule filled days ahead; the first pass runs now.
    state.spawn_autopilot();
    // Pushes "now playing" changes to connected remotes and players.
    state.spawn_now_watcher();
    // Re-reads watched folders and reports problems (a drive that is gone, a channel that keeps crashing).
    state.spawn_monitor();
    state.start_always_on();

    // One signal stops both listeners.
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
            stop.notify_waiters();
            // Notify only wakes those already waiting; a second signal below keeps it sticky.
            stop.notify_one();
        });
    }
    let mut servers = tokio::task::JoinSet::new();

    if !no_http {
        let listener = tokio::net::TcpListener::bind(&bind)
            .await
            .with_context(|| format!("binding {bind}"))?;
        tracing::info!("TVMaster listening on http://{bind}");
        let (app, stop) = (tvmaster_server::router(state.clone()), stop.clone());
        servers.spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { stop.notified().await })
                .await
                .map_err(anyhow::Error::from)
        });
    }
    if let Some(m) = &tls_material {
        let addr: std::net::SocketAddr = tls_bind
            .parse()
            .with_context(|| format!("--tls-bind '{tls_bind}' must look like 0.0.0.0:8788"))?;
        tracing::info!("TVMaster listening on https://{tls_bind}");
        tracing::info!(
            "HTTPS certificate ({}) SHA-256 {}",
            if m.self_signed {
                "self-signed"
            } else {
                "yours"
            },
            m.fingerprint
        );
        let handle = axum_server::Handle::new();
        let (app, m, stop2, h2) = (
            tvmaster_server::router_https(state.clone()),
            m.clone(),
            stop.clone(),
            handle.clone(),
        );
        tokio::spawn(async move {
            stop2.notified().await;
            h2.graceful_shutdown(Some(Duration::from_secs(5)));
        });
        servers.spawn(async move {
            tvmaster_server::tls::serve(addr, app, &m, handle)
                .await
                .map_err(anyhow::Error::from)
        });
    }
    // Run until a listener ends (a failure) or everything has shut down.
    while let Some(done) = servers.join_next().await {
        done.context("a listener panicked")??;
    }

    if let Some(p) = playout {
        p.stop_all();
        // Give the encoders a moment to exit.
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Ok(())
}

/// `tvmaster doctor`: what FFmpeg can do here, tested for real.
fn doctor(ffmpeg: &std::path::Path, ffprobe: &std::path::Path) -> Result<()> {
    let (ffmpeg, ffprobe) = (
        tools::resolve(ffmpeg.to_path_buf(), "ffmpeg"),
        tools::resolve(ffprobe.to_path_buf(), "ffprobe"),
    );
    let info = FfmpegInfo::detect(&ffmpeg, &ffprobe)
        .with_context(|| format!("FFmpeg not usable ({})", ffmpeg.display()))?;
    println!("FFmpeg {} ({})", info.version, info.ffmpeg.display());
    let yes = |b: bool| if b { "yes" } else { "no" };
    println!(
        "  ffprobe works      {}",
        yes(tvmaster_media::run_ok(&info.ffprobe, &["-version"]))
    );
    println!("  drawtext filter    {}", yes(info.has_drawtext));
    println!("  fast start-up      {}", yes(info.has_readrate_burst));
    println!(
        "  slate font         {}",
        find_system_font().map_or("none found".to_string(), |p| p.display().to_string())
    );
    println!("\nEncoders (each one encodes a 3 second 720p clip):");
    let mut best: Option<(Encoder, f64)> = None;
    let mut software_ok = false;
    for enc in Encoder::ALL {
        if !info.compiled.contains(&enc) {
            println!("  {:<13} not in this FFmpeg build", enc.name());
            continue;
        }
        let probe = probe_encoder(&info.ffmpeg, enc);
        match (probe.ok, probe.speed) {
            (true, Some(speed)) => {
                println!("  {:<13} works, {speed:.1}x real time", enc.name());
                if enc == Encoder::Software {
                    software_ok = true;
                } else if best.is_none_or(|(_, b)| speed > b) {
                    best = Some((enc, speed));
                }
            }
            _ => println!(
                "  {:<13} FAILED: {}",
                enc.name(),
                probe.error.unwrap_or_default()
            ),
        }
    }
    println!();
    match (best, software_ok) {
        (Some((enc, _)), _) if cfg!(target_os = "macos") && enc == Encoder::VideoToolbox => {
            println!("Auto mode will use videotoolbox.")
        }
        (Some((enc, _)), _) => println!(
            "Auto mode uses software encoding. For less CPU use: tvmaster serve --encoder {}",
            enc.name()
        ),
        (None, true) => println!("Auto mode uses software encoding (no working hardware encoder)."),
        (None, false) => anyhow::bail!("no video encoder works: TVMaster cannot stream"),
    }
    Ok(())
}
