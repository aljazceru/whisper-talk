use anyhow::Result;
use clap::Args;
use std::fs;
use std::process::Command;

use crate::config::ConfigManager;
use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Fix issues automatically where possible
    #[arg(long)]
    pub fix: bool,
}

pub fn run_validate(args: ValidateArgs) -> Result<()> {
    let paths = Paths::new()?;
    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    println!("Validating whisper-talk installation...\n");

    // Check configuration file
    if !paths.config_file.exists() {
        issues.push("Configuration file not found".to_string());
        if args.fix {
            println!("Creating default configuration...");
            let config = crate::types::Config::default();
            let content = serde_json::to_string_pretty(&config)?;
            if let Some(parent) = paths.config_file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&paths.config_file, content)?;
            println!("  ✓ Created {}", paths.config_file.display());
        }
    } else {
        match ConfigManager::new(paths.clone()) {
            Ok(manager) => {
                if let Err(e) = manager.validate() {
                    issues.push(format!("Configuration validation failed: {}", e));
                } else {
                    println!("✓ Configuration file valid");
                }

                // Check if configured model exists
                let config = manager.get_config();
                let model_variants = vec![
                    format!("ggml-{}.bin", config.transcription.model),
                    format!("ggml-{}.en.bin", config.transcription.model),
                ];

                let model_found = paths.model_search_dirs.iter().any(|dir| {
                    model_variants.iter().any(|variant| dir.join(variant).exists())
                });

                if !model_found {
                    issues.push(format!(
                        "Configured model '{}' not found",
                        config.transcription.model
                    ));
                } else {
                    println!("✓ Configured model found");
                }
            }
            Err(e) => {
                issues.push(format!("Failed to parse configuration: {}", e));
            }
        }
    }

    // Check required directories
    let required_dirs = [
        (&paths.config_dir, "config"),
        (&paths.state_dir, "state"),
        (&paths.data_dir, "data"),
    ];

    for (dir, name) in &required_dirs {
        if !dir.exists() {
            if args.fix {
                fs::create_dir_all(dir)?;
                println!("  ✓ Created {} directory: {}", name, dir.display());
            } else {
                warnings.push(format!("{} directory missing: {}", name, dir.display()));
            }
        }
    }

    // Check required binaries
    let required_binaries = [
        ("ydotool", "text injection"),
        ("pactl", "audio monitoring"),
    ];

    for (binary, purpose) in &required_binaries {
        let available = Command::new("which")
            .arg(binary)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !available {
            issues.push(format!("{} not found (required for {})", binary, purpose));
        } else {
            println!("✓ {} available", binary);
        }
    }

    // Check optional binaries
    let optional_binaries = [
        ("hyprctl", "Hyprland integration"),
        ("notify-send", "desktop notifications"),
    ];

    for (binary, purpose) in &optional_binaries {
        let available = Command::new("which")
            .arg(binary)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !available {
            warnings.push(format!("{} not found (optional: {})", binary, purpose));
        } else {
            println!("✓ {} available (optional)", binary);
        }
    }

    // Check user groups (for input device access)
    if let Ok(output) = Command::new("groups").output() {
        let groups = String::from_utf8_lossy(&output.stdout);
        if !groups.contains("input") {
            warnings.push("User not in 'input' group (may need for keyboard shortcuts)".to_string());
        } else {
            println!("✓ User in 'input' group");
        }
    }

    // Check systemd service
    let service_path = dirs::home_dir()
        .map(|h| h.join(".config/systemd/user/whisper-talk.service"));

    if let Some(path) = service_path {
        if path.exists() {
            println!("✓ Systemd service installed");
        } else {
            warnings.push("Systemd service not installed (run 'whisper-talk systemd install')".to_string());
        }
    }

    // Print summary
    println!("\n{}", "=".repeat(50));

    if issues.is_empty() && warnings.is_empty() {
        println!("\n✓ All validations passed!");
    } else {
        if !issues.is_empty() {
            println!("\n✗ Issues ({}):", issues.len());
            for issue in &issues {
                println!("  - {}", issue);
            }
        }

        if !warnings.is_empty() {
            println!("\n⚠ Warnings ({}):", warnings.len());
            for warning in &warnings {
                println!("  - {}", warning);
            }
        }

        if !args.fix && !issues.is_empty() {
            println!("\nRun 'whisper-talk validate --fix' to attempt automatic fixes.");
        }
    }

    if !issues.is_empty() {
        anyhow::bail!("Validation failed with {} issue(s)", issues.len());
    }

    Ok(())
}
