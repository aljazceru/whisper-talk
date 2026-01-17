use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub use setup::{run_setup, SetupArgs};
pub use config::{run_config, ConfigArgs};
pub use waybar::{run_waybar, WaybarArgs};
pub use mic_osd::{run_mic_osd, MicOsdArgs};
pub use systemd::{run_systemd, SystemdArgs};
pub use model::{run_model, ModelArgs};
pub use backend::{run_backend, BackendArgs};
pub use state::{run_state, StateArgs};
pub use status::{run_status, StatusArgs};
pub use validate::{run_validate, ValidateArgs};
pub use uninstall::{run_uninstall, UninstallArgs};

mod setup;
mod config;
mod waybar;
mod mic_osd;
mod systemd;
mod model;
mod backend;
mod state;
mod status;
mod validate;
mod uninstall;

#[derive(Parser, Debug)]
#[command(name = "whisper-talk")]
#[command(about = "System-wide voice dictation for Linux", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub debug: bool,

    #[arg(long, global = true)]
    pub no_progress: bool,

    #[arg(long, global = true)]
    pub log_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Daemon,
    Setup(SetupArgs),
    Config(ConfigArgs),
    Waybar(WaybarArgs),
    MicOsd(MicOsdArgs),
    Systemd(SystemdArgs),
    Model(ModelArgs),
    Backend(BackendArgs),
    State(StateArgs),
    Status(StatusArgs),
    Validate(ValidateArgs),
    Uninstall(UninstallArgs),
}

pub fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Daemon => {
            println!("Daemon - not implemented yet");
            Ok(())
        }
        Commands::Setup(args) => run_setup(args),
        Commands::Config(args) => run_config(args),
        Commands::Waybar(args) => run_waybar(args),
        Commands::MicOsd(args) => run_mic_osd(args),
        Commands::Systemd(args) => run_systemd(args),
        Commands::Model(args) => run_model(args),
        Commands::Backend(args) => run_backend(args),
        Commands::State(args) => run_state(args),
        Commands::Status(args) => run_status(args),
        Commands::Validate(args) => run_validate(args),
        Commands::Uninstall(args) => run_uninstall(args),
    }
}
