use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::ConfigManager;
use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct MicOsdArgs {
    #[command(subcommand)]
    pub command: MicOsdCommands,
}

#[derive(Subcommand, Debug)]
pub enum MicOsdCommands {
    /// Enable the microphone visualization overlay
    Enable,
    /// Disable the microphone visualization overlay
    Disable,
    /// Check if Mic OSD is enabled
    Status,
}

pub fn run_mic_osd(args: MicOsdArgs) -> Result<()> {
    let paths = Paths::new()?;

    match args.command {
        MicOsdCommands::Enable => run_mic_osd_enable(&paths),
        MicOsdCommands::Disable => run_mic_osd_disable(&paths),
        MicOsdCommands::Status => run_mic_osd_status(&paths),
    }
}

fn run_mic_osd_enable(paths: &Paths) -> Result<()> {
    let mut config_manager = ConfigManager::new(paths.clone())?;

    config_manager.get_config_mut().feedback.mic_osd_enabled = true;
    config_manager.save()?;

    println!("Mic OSD enabled");
    println!("\nThe microphone visualization overlay will appear when recording.");
    println!("Restart the daemon to apply changes: systemctl restart --user whisper-talk");

    Ok(())
}

fn run_mic_osd_disable(paths: &Paths) -> Result<()> {
    let mut config_manager = ConfigManager::new(paths.clone())?;

    config_manager.get_config_mut().feedback.mic_osd_enabled = false;
    config_manager.save()?;

    println!("Mic OSD disabled");
    println!("\nRestart the daemon to apply changes: systemctl restart --user whisper-talk");

    Ok(())
}

fn run_mic_osd_status(paths: &Paths) -> Result<()> {
    let config_manager = ConfigManager::new(paths.clone())?;
    let config = config_manager.get_config();

    let status = if config.feedback.mic_osd_enabled {
        "enabled"
    } else {
        "disabled"
    };

    println!("Mic OSD: {}", status);

    // Check if the feature is compiled in
    #[cfg(feature = "mic-osd")]
    {
        println!("GTK4 support: available");
    }

    #[cfg(not(feature = "mic-osd"))]
    {
        println!("GTK4 support: not compiled (build with --features mic-osd)");
    }

    Ok(())
}
