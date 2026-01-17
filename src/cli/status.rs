use anyhow::Result;
use clap::Args;
use std::fs;
use std::process::Command;

use crate::config::ConfigManager;
use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct StatusArgs {}

pub fn run_status(_args: StatusArgs) -> Result<()> {
    let paths = Paths::new()?;

    println!("whisper-talk System Status\n");
    println!("{}", "=".repeat(50));

    // Daemon status
    print_daemon_status(&paths);

    // Configuration status
    print_config_status(&paths);

    // Model status
    print_model_status(&paths);

    // System dependencies
    print_dependency_status();

    // Waybar module status
    print_waybar_status();

    // Audio devices
    print_audio_status();

    Ok(())
}

fn print_daemon_status(paths: &Paths) {
    println!("\n== Daemon ==");

    // Check systemd service status
    let service_status = Command::new("systemctl")
        .args(["--user", "is-active", "whisper-talk"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let status_icon = match service_status.as_str() {
        "active" => "✓",
        "inactive" => "✗",
        _ => "?",
    };

    println!("  Service: {} ({})", status_icon, service_status);

    // Check if lock file exists and is held
    if paths.lock_file.exists() {
        let lock_held = std::fs::File::open(&paths.lock_file)
            .and_then(|file| {
                use std::os::unix::io::AsRawFd;
                let fd = file.as_raw_fd();
                let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if result == 0 {
                    unsafe { libc::flock(fd, libc::LOCK_UN) };
                    Ok(false)
                } else {
                    Ok(true)
                }
            })
            .unwrap_or(false);

        if lock_held {
            println!("  Instance: ✓ running");
        } else {
            println!("  Instance: ✗ not running (stale lock)");
        }
    } else {
        println!("  Instance: ✗ not running");
    }

    // Recording status
    if let Ok(status) = fs::read_to_string(&paths.recording_status_file) {
        println!("  Recording: {}", status.trim());
    }
}

fn print_config_status(paths: &Paths) {
    println!("\n== Configuration ==");

    if paths.config_file.exists() {
        println!("  Config file: ✓ {}", paths.config_file.display());

        match ConfigManager::new(paths.clone()) {
            Ok(manager) => {
                let config = manager.get_config();
                println!("  Shortcut: {}", config.shortcuts.primary_shortcut);
                println!("  Recording mode: {:?}", config.shortcuts.recording_mode);
                println!("  Backend: {:?}", config.transcription.backend);
                println!("  Model: {}", config.transcription.model);

                if let Err(e) = manager.validate() {
                    println!("  Validation: ✗ {}", e);
                } else {
                    println!("  Validation: ✓ OK");
                }
            }
            Err(e) => {
                println!("  Config parse: ✗ {}", e);
            }
        }
    } else {
        println!("  Config file: ✗ not found");
        println!("  Run 'whisper-talk setup' to create configuration");
    }
}

fn print_model_status(paths: &Paths) {
    println!("\n== Models ==");

    let mut found_models = Vec::new();

    for dir in &paths.model_search_dirs {
        if !dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.starts_with("ggml-") && filename.ends_with(".bin") {
                        found_models.push(filename.to_string());
                    }
                }
            }
        }
    }

    if found_models.is_empty() {
        println!("  Models: ✗ none found");
        println!("  Run 'whisper-talk model download <name>' to download a model");
    } else {
        println!("  Models: ✓ {} found", found_models.len());
        for model in &found_models {
            println!("    - {}", model);
        }
    }
}

fn print_dependency_status() {
    println!("\n== Dependencies ==");

    // Check ydotool
    let ydotool_ok = Command::new("which")
        .arg("ydotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("  ydotool: {}", if ydotool_ok { "✓" } else { "✗ not found" });

    // Check pactl
    let pactl_ok = Command::new("pactl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("  pactl: {}", if pactl_ok { "✓" } else { "✗ not found" });

    // Check hyprctl (for Hyprland)
    let hyprctl_ok = Command::new("hyprctl")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("  hyprctl: {}", if hyprctl_ok { "✓" } else { "- not available" });

    // Check GPU
    let nvidia_ok = Command::new("nvidia-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let rocm_ok = Command::new("rocm-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if nvidia_ok {
        println!("  GPU: ✓ NVIDIA (CUDA)");
    } else if rocm_ok {
        println!("  GPU: ✓ AMD (ROCm)");
    } else {
        println!("  GPU: - CPU mode");
    }
}

fn print_waybar_status() {
    println!("\n== Integrations ==");

    // Check Waybar config
    let waybar_config = dirs::home_dir()
        .map(|h| h.join(".config/waybar/config"))
        .filter(|p| p.exists());

    if let Some(config_path) = waybar_config {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if content.contains("custom/whisper-talk") {
                println!("  Waybar module: ✓ installed");
            } else {
                println!("  Waybar module: ✗ not installed");
            }
        }
    } else {
        println!("  Waybar: - not configured");
    }
}

fn print_audio_status() {
    println!("\n== Audio ==");

    // Try to get default source from pactl
    if let Ok(output) = Command::new("pactl")
        .args(["get-default-source"])
        .output()
    {
        if output.status.success() {
            let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("  Default source: {}", source);
        }
    }

    // List audio devices with pactl
    if let Ok(output) = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        if output.status.success() {
            let sources = String::from_utf8_lossy(&output.stdout);
            let count = sources.lines().count();
            println!("  Audio sources: {} available", count);
        }
    }
}
