use anyhow::Result;
use clap::{Args, Subcommand};
use std::fs;

use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct BackendArgs {
    #[command(subcommand)]
    pub command: BackendCommands,
}

#[derive(Subcommand, Debug)]
pub enum BackendCommands {
    /// Repair the Whisper backend installation
    Repair,
    /// Reset backend state (removes cached state, not models)
    Reset,
}

pub fn run_backend(args: BackendArgs) -> Result<()> {
    let paths = Paths::new()?;

    match args.command {
        BackendCommands::Repair => run_backend_repair(&paths),
        BackendCommands::Reset => run_backend_reset(&paths),
    }
}

fn run_backend_repair(paths: &Paths) -> Result<()> {
    println!("Repairing Whisper backend...\n");

    // Check if models directory exists
    if !paths.models_dir.exists() {
        println!("Creating models directory: {}", paths.models_dir.display());
        fs::create_dir_all(&paths.models_dir)?;
    }

    // Check for any corrupted model files (zero-size files)
    let mut repaired = 0;
    for dir in &paths.model_search_dirs {
        if !dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.starts_with("ggml-") && filename.ends_with(".bin") {
                        if let Ok(metadata) = fs::metadata(&path) {
                            if metadata.len() == 0 {
                                println!("Removing corrupted model file: {}", path.display());
                                fs::remove_file(&path)?;
                                repaired += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // Clear any stale state files
    let state_files = [
        &paths.recovery_result_file,
        &paths.recovery_requested_file,
        &paths.mic_zero_volume_file,
    ];

    for file in &state_files {
        if file.exists() {
            println!("Clearing stale state file: {}", file.display());
            fs::remove_file(file)?;
            repaired += 1;
        }
    }

    // Test whisper-rs availability
    println!("\nChecking whisper-rs availability...");
    println!("whisper-rs: available (compiled in)");

    // Check GPU support
    let has_nvidia = std::process::Command::new("nvidia-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let has_rocm = std::process::Command::new("rocm-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_nvidia {
        println!("NVIDIA GPU: detected (CUDA support available)");
    } else if has_rocm {
        println!("AMD GPU: detected (ROCm support available)");
    } else {
        println!("GPU: not detected (using CPU mode)");
    }

    if repaired > 0 {
        println!("\nRepaired {} issue(s)", repaired);
    } else {
        println!("\nNo issues found. Backend is healthy.");
    }

    Ok(())
}

fn run_backend_reset(paths: &Paths) -> Result<()> {
    println!("Resetting backend state...\n");

    // Clear state files
    let state_files = [
        (&paths.recording_status_file, "recording status"),
        (&paths.audio_level_file, "audio level"),
        (&paths.recovery_result_file, "recovery result"),
        (&paths.recovery_requested_file, "recovery requested"),
        (&paths.mic_zero_volume_file, "mic zero volume"),
    ];

    let mut cleared = 0;
    for (file, name) in &state_files {
        if file.exists() {
            println!("Clearing {}: {}", name, file.display());
            fs::remove_file(file)?;
            cleared += 1;
        }
    }

    // Clear lock file if daemon is not running
    if paths.lock_file.exists() {
        // Try to acquire the lock to check if daemon is running
        let can_lock = std::fs::File::create(&paths.lock_file)
            .map(|file| {
                use std::os::unix::io::AsRawFd;
                let fd = file.as_raw_fd();
                let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if result == 0 {
                    unsafe { libc::flock(fd, libc::LOCK_UN) };
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if can_lock {
            println!("Clearing stale lock file: {}", paths.lock_file.display());
            fs::remove_file(&paths.lock_file)?;
            cleared += 1;
        } else {
            println!("Lock file in use (daemon may be running)");
        }
    }

    if cleared > 0 {
        println!("\nCleared {} state file(s)", cleared);
    } else {
        println!("\nNo state files to clear.");
    }

    println!(
        "\nNote: Models are preserved. Use 'whisper-talk model status' to see downloaded models."
    );

    Ok(())
}
