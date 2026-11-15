use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    /// Run the daemon (headless): API, streams and web UI.
    Serve {
        /// Address to listen on.
        #[arg(long, default_value = "0.0.0.0:8787")]
        bind: String,
        /// SQLite database path.
        #[arg(long, default_value = "tvmaster.db")]
        db: PathBuf,
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
        Command::Serve { bind, db } => {
            // Opening applies the schema; the connection is wired into the
            // server in a later milestone.
            tvmaster_db::open(&db)?;
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!("TVMaster listening on http://{bind}");
            axum::serve(listener, tvmaster_server::router()).await?;
        }
    }
    Ok(())
}
