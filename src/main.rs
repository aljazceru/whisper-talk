mod app;
mod audio;
mod cli;
mod config;
mod device;
mod error;
mod integration;
mod injection;
mod input;
mod instance_lock;
mod logger;
mod paths;
mod transcription;
mod types;
mod visualizer;

use app::Application;
use config::ConfigManager;
use paths::Paths;
use tokio::signal::unix::{signal, SignalKind};
use tokio::select;
use tracing::{error, info};

pub use cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on CLI flags
    if let Err(e) = logger::init_logging(
        cli.quiet,
        cli.verbose,
        cli.debug,
        cli.no_progress,
        cli.log_file.as_deref(),
    ) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    if matches!(cli.command, cli::Commands::Daemon) {
        return run_daemon().await;
    }

    cli::run_cli(cli)
}

async fn run_daemon() -> anyhow::Result<()> {
    info!("whisper-talk daemon starting...");

    // Load config from file
    let paths = Paths::new()?;
    let config_manager = ConfigManager::new(paths)?;
    let config = config_manager.get_config().clone();

    info!("Loaded config from {}", config_manager.get_config_path().display());
    info!("Using shortcut: {}", config.shortcuts.primary_shortcut);
    info!("Using backend: {:?}", config.transcription.backend);
    info!("Using model: {}", config.transcription.model);

    let mut app = Application::new(config)?;

    if let Err(e) = app.acquire_instance_lock() {
        error!("Failed to acquire instance lock: {}", e);
        error!("Another instance may already be running");
        return Err(e.into());
    }

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    app.start().await?;

    // Main event loop
    loop {
        select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
                break;
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down...");
                break;
            }
            _ = sighup.recv() => {
                info!("Received SIGHUP, reloading config...");
                // Reload configuration
                if let Ok(new_paths) = Paths::new() {
                    if let Ok(new_config_manager) = ConfigManager::new(new_paths) {
                        let new_config = new_config_manager.get_config().clone();
                        if let Err(e) = app.update_config(new_config) {
                            error!("Failed to reload config: {}", e);
                        } else {
                            info!("Config reloaded successfully");
                        }
                    }
                }
            }
            _ = app.run_event_loop() => {
                info!("Event loop exited");
                break;
            }
        }
    }

    app.stop().await?;

    info!("whisper-talk daemon stopped");

    Ok(())
}
