use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{error, info, warn};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn init_logging(
    quiet: bool,
    verbose: bool,
    debug: bool,
    no_progress: bool,
    log_file: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    PROGRESS_ENABLED.store(!no_progress, Ordering::SeqCst);

    let level = if quiet {
        tracing::Level::ERROR
    } else if debug {
        tracing::Level::DEBUG
    } else if verbose {
        tracing::Level::INFO
    } else {
        tracing::Level::WARN
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_span_events(FmtSpan::NONE);

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer);

    if let Some(log_path) = log_file {
        let file = std::fs::File::create(log_path)?;
        let file_layer = fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_file(true)
            .with_line_number(true);

        subscriber.with(file_layer).try_init()?;
    } else {
        subscriber.try_init()?;
    }

    Ok(())
}

#[allow(dead_code)]
pub fn is_progress_enabled() -> bool {
    PROGRESS_ENABLED.load(Ordering::SeqCst)
}

#[allow(dead_code)]
pub fn log_info(msg: &str) {
    info!("{}", msg);
}

#[allow(dead_code)]
pub fn log_success(msg: &str) {
    info!("{}", msg);
}

#[allow(dead_code)]
pub fn log_warning(msg: &str) {
    warn!("{}", msg);
}

#[allow(dead_code)]
pub fn log_error(msg: &str) {
    error!("{}", msg);
}

#[allow(dead_code)]
pub fn log_debug(msg: &str) {
    tracing::debug!("{}", msg);
}

#[allow(dead_code)]
pub fn set_progress_enabled(enabled: bool) {
    PROGRESS_ENABLED.store(enabled, Ordering::SeqCst);
}
