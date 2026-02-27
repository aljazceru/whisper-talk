mod api;
mod app;
mod audio;
mod cli;
mod config;
mod device;
mod error;
mod injection;
mod input;
mod instance_lock;
mod integration;
mod logger;
mod paths;
mod transcription;
mod types;
mod visualizer;

use app::Application;
use config::ConfigManager;
use paths::Paths;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};

use clap::Parser;
pub use cli::Cli;

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

    if let cli::Commands::Daemon(args) = cli.command {
        return run_daemon(args).await;
    }

    cli::run_cli(cli)
}

async fn run_daemon(daemon: cli::DaemonArgs) -> anyhow::Result<()> {
    info!("whisper-talk daemon starting...");

    // Load config from file
    let paths = Paths::new()?;
    let config_manager = ConfigManager::new(paths)?;
    let config = config_manager.get_config().clone();

    info!(
        "Loaded config from {}",
        config_manager.get_config_path().display()
    );
    info!("Using shortcut: {}", config.shortcuts.primary_shortcut);
    info!("Using backend: {:?}", config.transcription.backend);
    info!("Using model: {}", config.transcription.model);

    let api_listener_addr = daemon.api_bind.or(config.api_bind);
    let mut app = Application::from_config(config)?;

    if let Err(e) = app.acquire_instance_lock() {
        error!("Failed to acquire instance lock: {}", e);
        error!("Another instance may already be running");
        return Err(e.into());
    }

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    app.start().await?;

    let mut api_task = None;
    let mut api_shutdown = None;

    if let Some(bind_addr) = api_listener_addr {
        if daemon.api_bind.is_some() {
            info!("Using CLI --api-bind: {}", bind_addr);
        } else {
            info!("Using config api_bind: {}", bind_addr);
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        api_task =
            Some(api::spawn_api_server(app.api_application(), bind_addr, shutdown_rx).await?);
        api_shutdown = Some(shutdown_tx);
    }

    if let Some(address) = api_listener_addr {
        info!("OpenAI-compatible API exposed on {}", address);
    }

    // Main event loop
    loop {
        select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
                if let Some(shutdown_tx) = api_shutdown.take() {
                    let _ = shutdown_tx.send(());
                }
                break;
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down...");
                if let Some(shutdown_tx) = api_shutdown.take() {
                    let _ = shutdown_tx.send(());
                }
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
                if let Some(shutdown_tx) = api_shutdown.take() {
                    let _ = shutdown_tx.send(());
                }
                break;
            }
        }
    }

    if let Some(api_task) = api_task {
        let _ = api_task.await;
    }

    app.stop().await?;

    info!("whisper-talk daemon stopped");

    Ok(())
}
