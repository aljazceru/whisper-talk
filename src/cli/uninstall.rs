use anyhow::Result;
use clap::Args;
use dialoguer::Confirm;
use std::fs;
use std::process::Command;

use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// Keep downloaded models
    #[arg(long)]
    pub keep_models: bool,

    /// Remove user from input/audio groups (requires sudo)
    #[arg(long)]
    pub remove_permissions: bool,

    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
}

pub fn run_uninstall(args: UninstallArgs) -> Result<()> {
    let paths = Paths::new()?;

    println!("whisper-talk Uninstall\n");
    println!("{}", "=".repeat(50));

    // Show what will be removed
    println!("\nThe following will be removed:");
    println!("  - Configuration files: {}", paths.config_dir.display());
    println!("  - State files: {}", paths.state_dir.display());
    println!("  - Data files: {}", paths.data_dir.display());
    println!("  - Systemd service: ~/.config/systemd/user/whisper-talk.service");
    println!("  - Waybar module: ~/.local/share/whisper-talk/waybar/");

    if !args.keep_models {
        println!("  - Downloaded models: {}", paths.models_dir.display());
    } else {
        println!("  - Downloaded models: KEPT (--keep-models)");
    }

    if args.remove_permissions {
        println!("  - User groups: input, audio (requires sudo)");
    }

    println!();

    // Confirm unless --yes
    if !args.yes {
        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to uninstall whisper-talk?")
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Uninstall cancelled.");
            return Ok(());
        }
    }

    println!("\nUninstalling...\n");

    // Stop daemon if running
    println!("Stopping daemon...");
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "whisper-talk"])
        .status();

    // Disable and remove systemd service
    println!("Removing systemd service...");
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "whisper-talk"])
        .status();

    if let Some(home) = dirs::home_dir() {
        let service_path = home.join(".config/systemd/user/whisper-talk.service");
        if service_path.exists() {
            fs::remove_file(&service_path)?;
            println!("  Removed: {}", service_path.display());
        }
    }

    // Remove Waybar module
    println!("Removing Waybar module...");
    if let Some(home) = dirs::home_dir() {
        // Remove module script
        let waybar_script = home.join(".local/share/whisper-talk/waybar");
        if waybar_script.exists() {
            fs::remove_dir_all(&waybar_script)?;
            println!("  Removed: {}", waybar_script.display());
        }

        // Try to remove from Waybar config
        let waybar_config = home.join(".config/waybar/config");
        if waybar_config.exists() {
            if let Ok(content) = fs::read_to_string(&waybar_config) {
                if content.contains("custom/whisper-talk") {
                    // Use the waybar removal function
                    let _ = crate::integration::waybar::handle_waybar(
                        crate::integration::waybar::WaybarCommand::Remove,
                    );
                }
            }
        }
    }

    // Remove state files
    println!("Removing state files...");
    if paths.state_dir.exists() {
        fs::remove_dir_all(&paths.state_dir)?;
        println!("  Removed: {}", paths.state_dir.display());
    }

    // Remove data files (but maybe not models)
    println!("Removing data files...");
    if paths.data_dir.exists() {
        fs::remove_dir_all(&paths.data_dir)?;
        println!("  Removed: {}", paths.data_dir.display());
    }

    // Remove models unless --keep-models
    if !args.keep_models {
        println!("Removing models...");
        if paths.models_dir.exists() {
            fs::remove_dir_all(&paths.models_dir)?;
            println!("  Removed: {}", paths.models_dir.display());
        }
    }

    // Remove config files last
    println!("Removing configuration...");
    if paths.config_dir.exists() {
        fs::remove_dir_all(&paths.config_dir)?;
        println!("  Removed: {}", paths.config_dir.display());
    }

    // Remove permissions if requested
    if args.remove_permissions {
        println!("\nRemoving group memberships (requires sudo)...");

        let username = std::env::var("USER").unwrap_or_else(|_| "".to_string());
        if !username.is_empty() {
            for group in &["input", "audio"] {
                println!("  Removing from {}: {}", group, username);
                let status = Command::new("sudo")
                    .args(["gpasswd", "-d", &username, group])
                    .status();

                match status {
                    Ok(s) if s.success() => println!("    ✓ Removed from {}", group),
                    _ => println!(
                        "    ✗ Failed to remove from {} (may require manual sudo)",
                        group
                    ),
                }
            }
        }
    }

    println!("\n{}", "=".repeat(50));
    println!("\nwhisper-talk has been uninstalled.");

    if args.keep_models {
        println!("\nModels were preserved at: {}", paths.models_dir.display());
    }

    if !args.remove_permissions {
        println!("\nNote: User group memberships (input, audio) were not removed.");
        println!("Run with --remove-permissions to remove group memberships.");
    }

    Ok(())
}
