use anyhow::Result;
use clap::{Args, Subcommand};
use std::fs;

use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct StateArgs {
    #[command(subcommand)]
    pub command: StateCommands,
}

#[derive(Subcommand, Debug)]
pub enum StateCommands {
    /// Show current state files and their contents
    Show,
    /// Validate state files
    Validate,
    /// Reset state files
    Reset {
        /// Reset all state including lock file
        #[arg(long)]
        all: bool,
    },
}

pub fn run_state(args: StateArgs) -> Result<()> {
    let paths = Paths::new()?;

    match args.command {
        StateCommands::Show => run_state_show(&paths),
        StateCommands::Validate => run_state_validate(&paths),
        StateCommands::Reset { all } => run_state_reset(all, &paths),
    }
}

fn run_state_show(paths: &Paths) -> Result<()> {
    println!("whisper-talk State Information\n");
    println!("State directory: {}", paths.state_dir.display());
    println!();

    let state_files = [
        (&paths.recording_status_file, "Recording Status"),
        (&paths.audio_level_file, "Audio Level"),
        (&paths.recovery_result_file, "Recovery Result"),
        (&paths.recovery_requested_file, "Recovery Requested"),
        (&paths.mic_zero_volume_file, "Mic Zero Volume"),
        (&paths.lock_file, "Lock File"),
    ];

    println!("{:<25} {:<15} CONTENT", "FILE", "STATUS");
    println!("{}", "-".repeat(60));

    for (path, name) in &state_files {
        let (status, content) = if path.exists() {
            let content = fs::read_to_string(path)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "<binary>".to_string());

            let content_display = if content.len() > 30 {
                format!("{}...", &content[..27])
            } else if content.is_empty() {
                "<empty>".to_string()
            } else {
                content
            };

            ("exists", content_display)
        } else {
            ("missing", "-".to_string())
        };

        println!("{:<25} {:<15} {}", name, status, content);
    }

    // Check lock file status
    println!();
    if paths.lock_file.exists() {
        let lock_held = std::fs::File::open(&paths.lock_file)
            .map(|file| {
                use std::os::unix::io::AsRawFd;
                let fd = file.as_raw_fd();
                let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if result == 0 {
                    unsafe { libc::flock(fd, libc::LOCK_UN) };
                    false
                } else {
                    true
                }
            })
            .unwrap_or(false);

        if lock_held {
            println!("Lock status: HELD (daemon is running)");
        } else {
            println!("Lock status: NOT HELD (stale lock file)");
        }
    } else {
        println!("Lock status: NO LOCK (daemon not running)");
    }

    Ok(())
}

fn run_state_validate(paths: &Paths) -> Result<()> {
    println!("Validating state files...\n");

    let mut issues = Vec::new();

    // Check state directory
    if !paths.state_dir.exists() {
        issues.push(format!(
            "State directory missing: {}",
            paths.state_dir.display()
        ));
    }

    // Check recording status file format
    if paths.recording_status_file.exists() {
        let content = fs::read_to_string(&paths.recording_status_file)?;
        let valid_states = ["idle", "recording", "processing"];
        if !valid_states.contains(&content.trim()) {
            issues.push(format!(
                "Invalid recording status: '{}' (expected: idle, recording, processing)",
                content.trim()
            ));
        }
    }

    // Check audio level file format
    if paths.audio_level_file.exists() {
        let content = fs::read_to_string(&paths.audio_level_file)?;
        if content.trim().parse::<f32>().is_err() {
            issues.push(format!(
                "Invalid audio level: '{}' (expected: float)",
                content.trim()
            ));
        }
    }

    // Check for stale lock
    if paths.lock_file.exists() {
        let lock_held = std::fs::File::open(&paths.lock_file)
            .map(|file| {
                use std::os::unix::io::AsRawFd;
                let fd = file.as_raw_fd();
                let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if result == 0 {
                    unsafe { libc::flock(fd, libc::LOCK_UN) };
                    false
                } else {
                    true
                }
            })
            .unwrap_or(false);

        if !lock_held {
            issues.push("Stale lock file detected".to_string());
        }
    }

    if issues.is_empty() {
        println!("✓ All state files are valid");
    } else {
        println!("Issues found:");
        for issue in &issues {
            println!("  ✗ {}", issue);
        }
        println!("\nRun 'whisper-talk state reset' to clear state files.");
    }

    Ok(())
}

fn run_state_reset(all: bool, paths: &Paths) -> Result<()> {
    println!("Resetting state files...\n");

    let mut cleared = 0;

    // Files to always clear
    let always_clear = [
        &paths.recording_status_file,
        &paths.audio_level_file,
        &paths.recovery_result_file,
        &paths.recovery_requested_file,
        &paths.mic_zero_volume_file,
    ];

    for path in &always_clear {
        if path.exists() {
            fs::remove_file(path)?;
            println!("  Cleared: {}", path.display());
            cleared += 1;
        }
    }

    // Only clear lock if --all and daemon is not running
    if all && paths.lock_file.exists() {
        let can_remove = std::fs::File::open(&paths.lock_file)
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
            .unwrap_or(true);

        if can_remove {
            fs::remove_file(&paths.lock_file)?;
            println!("  Cleared: {} (lock file)", paths.lock_file.display());
            cleared += 1;
        } else {
            println!(
                "  Skipped: {} (daemon is running)",
                paths.lock_file.display()
            );
        }
    }

    if cleared > 0 {
        println!("\nCleared {} state file(s)", cleared);
    } else {
        println!("No state files to clear.");
    }

    Ok(())
}
