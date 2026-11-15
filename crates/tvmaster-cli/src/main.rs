mod service;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tvmaster_media::{find_system_font, Encoder, FfmpegInfo};
use tvmaster_playout::{EngineConfig, Manager};
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
        /// Video encoder: software, videotoolbox or nvenc. Default: auto.
        #[arg(long, env = "TVMASTER_ENCODER")]
        encoder: Option<String>,
        /// Stop an on-demand channel after this many seconds without viewers.
        #[arg(long, default_value_t = 60)]
        idle_seconds: u64,
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
        } => serve(bind, data_dir, ffmpeg, ffprobe, encoder, idle_seconds).await,
        Command::Service {
            kind,
            bin,
            data_dir,
            bind,
        } => {
            let bin = match bin {
                Some(b) => b,
                None => std::env::current_exe().context("cannot locate this binary")?,
            };
            let data_dir = std::path::absolute(&data_dir).unwrap_or(data_dir);
            print!("{}", service::render(kind, &bin, &data_dir, &bind));
            Ok(())
        }
    }
}

async fn serve(
    bind: String,
    data_dir: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    encoder: Option<String>,
    idle_seconds: u64,
) -> Result<()> {
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let conn = tvmaster_db::open(data_dir.join("tvmaster.db"))?;

    let requested = match &encoder {
        Some(name) => Some(Encoder::parse(name).with_context(|| {
            format!("unknown encoder '{name}' (use software, videotoolbox or nvenc)")
        })?),
        None => None,
    };

    let (info, playout) = match FfmpegInfo::detect(&ffmpeg, &ffprobe) {
        Ok(info) => {
            let enc = info.choose_encoder(requested);
            tracing::info!(version = %info.version, encoder = ?enc, "ffmpeg ready");
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

    let state = AppState::new(conn, info, playout.clone());
    // Keeps every channel's schedule filled days ahead; the first pass runs now.
    state.spawn_autopilot();
    // Pushes "now playing" changes to connected remotes and players.
    state.spawn_now_watcher();
    state.start_always_on();

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("TVMaster listening on http://{bind}");
    axum::serve(listener, tvmaster_server::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;

    if let Some(p) = playout {
        p.stop_all();
        // Give the encoders a moment to exit.
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Ok(())
}
