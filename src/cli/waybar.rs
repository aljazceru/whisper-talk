use anyhow::Result;
use clap::{Args, Subcommand};

use crate::integration::waybar::{handle_waybar, WaybarCommand};

#[derive(Args, Debug)]
pub struct WaybarArgs {
    #[command(subcommand)]
    pub command: WaybarCommands,
}

#[derive(Subcommand, Debug)]
pub enum WaybarCommands {
    /// Install the whisper-talk Waybar module
    Install,
    /// Remove the whisper-talk Waybar module
    Remove,
    /// Check if the Waybar module is installed
    Status,
}

pub fn run_waybar(args: WaybarArgs) -> Result<()> {
    let result = match args.command {
        WaybarCommands::Install => handle_waybar(WaybarCommand::Install),
        WaybarCommands::Remove => handle_waybar(WaybarCommand::Remove),
        WaybarCommands::Status => handle_waybar(WaybarCommand::Status),
    };

    result.map_err(|e| anyhow::anyhow!("{}", e))
}
