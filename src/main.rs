mod app;
mod config;
mod domain;
mod parser;
mod pipeline;
mod scanner;
mod tmdb;
mod tui;
mod util;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "archivist", version, about)]
struct Cli {
    /// Path to a config.toml. Default: ./config.toml or next to the exe.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Plan actions but do not touch the filesystem.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    let cfg = config::Config::load(cli.config.as_deref()).context("load config")?;
    if cli.dry_run {
        tracing::warn!("--dry-run enabled: filesystem writes will be skipped");
    }

    let app = app::App::new(cfg);
    app.run().await
}

fn init_tracing() -> Result<()> {
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .map(|d| d.join("archivist.log"))
        .unwrap_or_else(|| PathBuf::from("archivist.log"));

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open log file {}", log_path.display()))?;
    let env = EnvFilter::try_from_env("RECOVER_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env)
        .with_writer(file)
        .with_ansi(false)
        .init();
    eprintln!("archivist: log → {}", log_path.display());
    Ok(())
}
