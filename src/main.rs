mod api;
mod app;
mod audio;
mod cli;
mod config;
mod control;
mod device;
mod error;
#[cfg(feature = "gui")]
mod gui;
mod injection;
mod input;
mod instance_lock;
mod integration;
mod logger;
mod paths;
mod transcription;
mod types;
mod visualizer;

use app::{state::OwnedApplication, Application};
use config::ConfigManager;
use control::{Command, Control};
use instance_lock::InstanceLock;
use paths::Paths;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, oneshot};
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

    if let cli::Commands::Daemon(args) = &cli.command {
        return run_daemon(args.clone()).await;
    }

    cli::run_cli(cli)
}

#[cfg(feature = "gui")]
fn desktop_display_available() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some()
}

async fn run_daemon(daemon: cli::DaemonArgs) -> anyhow::Result<()> {
    info!("whisper-talk daemon starting...");

    // Load config from file
    let paths = Paths::new()?;
    let config_manager = ConfigManager::new(paths.clone())?;
    let config = config_manager.get_config().clone();

    // Acquire the instance lock before subsystems or the GUI start, so a
    // second daemon never opens a tray against a running instance.
    let _lock = match InstanceLock::acquire(&paths.lock_file) {
        Ok(lock) => lock,
        Err(e) => {
            error!("Failed to acquire instance lock: {}", e);
            error!("Another instance may already be running");
            return Err(e.into());
        }
    };

    info!(
        "Loaded config from {}",
        config_manager.get_config_path().display()
    );
    info!("Using shortcut: {}", config.shortcuts.primary_shortcut);
    info!("Using backend: {:?}", config.transcription.backend);
    info!("Using model: {}", config.transcription.model);

    let control = Control::new(config.clone());
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();

    #[cfg(feature = "gui")]
    let use_gui = !daemon.no_gui && desktop_display_available();
    #[cfg(not(feature = "gui"))]
    let use_gui = false;

    // Keep the sender alive so cmd_rx.recv() pends forever in headless mode.
    let _cmd_tx_keep = cmd_tx;

    if daemon.show_settings && !use_gui {
        info!("--show-settings ignored: GUI unavailable");
    }
    if !use_gui {
        info!("Running headless (no GUI)");
    }

    #[cfg(feature = "gui")]
    if use_gui {
        // GTK owns the main OS thread; the daemon runs on a dedicated worker
        // thread driving the shared Tokio runtime via Handle::block_on.
        let show = daemon.show_settings;
        let handle = tokio::runtime::Handle::current();
        let worker_control = control.clone();
        let worker = std::thread::spawn(move || {
            let result = handle.block_on(daemon_main(daemon, worker_control.clone(), cmd_rx, true));
            // Any worker exit means the daemon is gone: publish it so the GUI
            // notices and quits instead of hanging.
            if let Err(e) = &result {
                error!("daemon worker exited with error: {}", e);
                worker_control.push_error(format!("daemon exited: {}", e));
            }
            worker_control.set_status("stopped");
            result
        });

        let gui_result = gui::run(control.clone(), _cmd_tx_keep.clone(), show);
        if let Err(e) = &gui_result {
            error!("GUI exited with error: {}", e);
        }
        // GUI is gone (Quit command, closed channel or error): stop the worker.
        let _ = _cmd_tx_keep.send(Command::Quit);
        let _ = worker.join();
        return gui_result;
    }

    daemon_main(daemon, control, cmd_rx, false).await
}

type ApiShutdown = oneshot::Sender<()>;
type ApiTask = tokio::task::JoinHandle<()>;

async fn start_api(
    bind_addr: Option<std::net::SocketAddr>,
    application: &Application,
    api_task: &mut Option<ApiTask>,
    api_shutdown: &mut Option<ApiShutdown>,
) -> anyhow::Result<()> {
    let Some(bind_addr) = bind_addr else {
        return Ok(());
    };
    if api_task.is_some() {
        return Ok(());
    }
    let (tx, rx) = oneshot::channel::<()>();
    match api::spawn_api_server(application.clone(), bind_addr, rx).await {
        Ok(task) => {
            info!("OpenAI-compatible API exposed on {}", bind_addr);
            *api_task = Some(task);
            *api_shutdown = Some(tx);
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("API server failed to start: {}", e)),
    }
}

/// Build an application, attach control reporting and start subsystems.
/// On failure nothing is committed; the daemon keeps running so the GUI can
/// show the error and an Apply retries startup.
async fn try_start(
    config: crate::types::Config,
    control: &Control,
) -> anyhow::Result<OwnedApplication> {
    control.set_status("starting");
    let mut app = Application::from_config(config)?;
    app.set_control(control.clone());
    app.start().await?;
    Ok(app)
}

fn persist_config(paths: &Paths, config: &crate::types::Config) -> crate::error::Result<()> {
    let mut manager = ConfigManager::new(paths.clone())?;
    *manager.get_config_mut() = config.clone();
    manager.validate()?;
    manager.save()
}

async fn daemon_main(
    daemon: cli::DaemonArgs,
    control: Control,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    // true on the GUI worker thread: keep serving commands after failures so
    // the GUI can show errors and Apply retries. false headless: preserve the
    // original fail-fast behavior so systemd Restart kicks in.
    keep_alive: bool,
) -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let config = control.0.lock().config.clone();
    let api_bind = daemon.api_bind.or(config.api_bind);

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    let mut app: Option<OwnedApplication> = match try_start(config, &control).await {
        Ok(app) => Some(app),
        Err(e) => {
            error!("Daemon startup failed: {}", e);
            control.push_error(format!("startup failed: {}", e));
            control.set_status("startup failed");
            if !keep_alive {
                return Err(e);
            }
            None
        }
    };

    let mut api_task: Option<ApiTask> = None;
    let mut api_shutdown: Option<ApiShutdown> = None;
    if let Some(a) = &app {
        match start_api(
            api_bind,
            &a.api_application(),
            &mut api_task,
            &mut api_shutdown,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                error!("{}", e);
                control.push_error(format!("{}", e));
                if !keep_alive {
                    return Err(e);
                }
            }
        }
    }

    enum Pending {
        None,
        Apply {
            config: Box<crate::types::Config>,
            persist: bool,
        },
        Stop,
    }

    loop {
        // The event loop runs on a cloned handle so `app` is never borrowed
        // across the select.
        let app_handle = app.as_ref().map(|a| a.api_application());
        let mut pending = Pending::None;

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
                match ConfigManager::new(Paths::new()?)
                    .map(|m| m.get_config().clone())
                {
                    Ok(new_config) => {
                        pending = Pending::Apply { config: Box::new(new_config), persist: false };
                    }
                    Err(e) => {
                        error!("Failed to reload config: {}", e);
                        control.push_error(format!("config reload failed: {}", e));
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(Command::Apply(config)) => {
                        pending = Pending::Apply { config, persist: true };
                    }
                    Some(Command::Quit) | None => {
                        pending = Pending::Stop;
                    }
                }
            }
            _ = async {
                match app_handle {
                    Some(handle) => handle.run_event_loop().await,
                    None => std::future::pending().await,
                }
            } => {
                info!("Event loop exited");
                break;
            }
        }

        match pending {
            Pending::None => {}
            Pending::Stop => break,
            Pending::Apply {
                config: new_config,
                persist,
            } => {
                let new_config = *new_config;
                match app.as_mut() {
                    Some(a) => {
                        control.set_status("applying settings");
                        control.set_busy(true);
                        // Model initialization can block; run it on the
                        // blocking pool to keep the runtime responsive.
                        let application = a.api_application();
                        let apply_status = application.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            application.update_config(new_config, persist)
                        })
                        .await
                        .map_err(|e| anyhow::anyhow!("apply task failed: {}", e))
                        .and_then(|r| r.map_err(|e| anyhow::anyhow!("{}", e)));
                        match result {
                            Ok(()) => {
                                info!("Config applied");
                            }
                            Err(e) => {
                                error!("Failed to apply config: {}", e);
                                control.push_error(format!("settings rejected: {}", e));
                            }
                        }
                        // Restores busy/status to the true recording state.
                        apply_status.write_status_files();
                    }
                    None => {
                        // Startup failed earlier: an Apply retries starting
                        // the daemon with the new config, then persists it
                        // only after a successful start so a bad config never
                        // overwrites the last working file.
                        control.set_status("applying settings");
                        control.set_busy(true);
                        match try_start(new_config.clone(), &control).await {
                            Ok(mut candidate) => {
                                if persist {
                                    if let Err(e) = persist_config(&paths, &new_config) {
                                        let e = anyhow::anyhow!(e);
                                        error!("Failed to persist config: {}", e);
                                        control.push_error(format!("settings rejected: {}", e));
                                        // Don't keep a running daemon whose
                                        // config doesn't match the file.
                                        let _ = candidate.stop().await;
                                        control.set_status("startup failed");
                                        control.set_busy(false);
                                        continue;
                                    }
                                }
                                control.0.lock().config = new_config;
                                if let Err(e) = start_api(
                                    api_bind,
                                    &candidate.api_application(),
                                    &mut api_task,
                                    &mut api_shutdown,
                                )
                                .await
                                {
                                    error!("{}", e);
                                    control.push_error(format!("{}", e));
                                    if !keep_alive {
                                        let _ = candidate.stop().await;
                                        return Err(e);
                                    }
                                }
                                app = Some(candidate);
                                info!("Daemon started after settings apply");
                            }
                            Err(e) => {
                                error!("Daemon startup failed: {}", e);
                                control.push_error(format!("startup failed: {}", e));
                                control.set_status("startup failed");
                            }
                        }
                        control.set_busy(false);
                    }
                }
            }
        }
    }

    if let Some(shutdown_tx) = api_shutdown.take() {
        let _ = shutdown_tx.send(());
    }
    if let Some(api_task) = api_task {
        let _ = api_task.await;
    }

    if let Some(a) = app.as_mut() {
        a.stop().await?;
    }

    control.set_status("stopped");
    info!("whisper-talk daemon stopped");

    Ok(())
}
