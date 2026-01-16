use anyhow::Result;
use clap::{Args, Subcommand};
use std::process::Command;

use crate::config::ConfigManager;
use crate::paths::Paths;

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Initialize a new configuration file with defaults
    Init {
        /// Overwrite existing config if present
        #[arg(short, long)]
        force: bool,
    },
    /// Show the current configuration
    Show {
        /// Output format (json, toml, pretty)
        #[arg(short, long, default_value = "pretty")]
        format: String,
    },
    /// Open the configuration file in the default editor
    Edit,
    /// Get a specific configuration value
    Get {
        /// Configuration key (e.g., "shortcuts.primary_shortcut")
        key: String,
    },
    /// Set a specific configuration value
    Set {
        /// Configuration key (e.g., "shortcuts.primary_shortcut")
        key: String,
        /// New value for the key
        value: String,
    },
}

pub fn run_config(args: ConfigArgs) -> Result<()> {
    let paths = Paths::new()?;

    match args.command {
        ConfigCommands::Init { force } => run_config_init(force, &paths),
        ConfigCommands::Show { format } => run_config_show(&format, &paths),
        ConfigCommands::Edit => run_config_edit(&paths),
        ConfigCommands::Get { key } => run_config_get(&key, &paths),
        ConfigCommands::Set { key, value } => run_config_set(&key, &value, &paths),
    }
}

fn run_config_init(force: bool, paths: &Paths) -> Result<()> {
    if paths.config_file.exists() && !force {
        println!("Configuration file already exists: {}", paths.config_file.display());
        println!("Use --force to overwrite");
        return Ok(());
    }

    // Create default config
    let config = crate::types::Config::default();
    let content = serde_json::to_string_pretty(&config)?;

    // Ensure directory exists
    if let Some(parent) = paths.config_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&paths.config_file, content)?;

    println!("Configuration file created: {}", paths.config_file.display());
    println!("\nDefault settings:");
    println!("  Shortcut: {}", config.shortcuts.primary_shortcut);
    println!("  Recording mode: {:?}", config.shortcuts.recording_mode);
    println!("  Backend: {:?}", config.transcription.backend);
    println!("  Model: {}", config.transcription.model);
    println!("\nRun 'gwhspr setup' for guided configuration.");

    Ok(())
}

fn run_config_show(format: &str, paths: &Paths) -> Result<()> {
    if !paths.config_file.exists() {
        println!("No configuration file found at: {}", paths.config_file.display());
        println!("Run 'gwhspr config init' to create one.");
        return Ok(());
    }

    let config_manager = ConfigManager::new(paths.clone())?;
    let config = config_manager.get_config();

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(config)?;
            println!("{}", json);
        }
        "pretty" | _ => {
            println!("Configuration file: {}", paths.config_file.display());
            println!();
            println!("=== Shortcuts ===");
            println!("  Primary shortcut: {}", config.shortcuts.primary_shortcut);
            println!("  Recording mode: {:?}", config.shortcuts.recording_mode);
            println!("  Grab keys: {}", config.shortcuts.grab_keys);
            println!("  Auto mode threshold: {}ms", config.shortcuts.auto_mode_threshold_ms);
            println!();
            println!("=== Audio ===");
            println!("  Device ID: {:?}", config.audio.device_id);
            println!("  Device name: {:?}", config.audio.device_name);
            println!("  Vendor ID: {:?}", config.audio.device_vendor_id);
            println!("  Model ID: {:?}", config.audio.device_model_id);
            println!("  Mute detection: {}", config.audio.mute_detection);
            println!("  Zero volume threshold: {}", config.audio.zero_volume_threshold);
            println!();
            println!("=== Transcription ===");
            println!("  Backend: {:?}", config.transcription.backend);
            println!("  Model: {}", config.transcription.model);
            println!("  Threads: {}", config.transcription.threads);
            println!("  Language: {:?}", config.transcription.language);
            println!("  Word overrides: {} entries", config.transcription.word_overrides.len());
            println!();
            println!("=== Injection ===");
            println!("  Paste mode: {:?}", config.injection.paste_mode);
            println!("  Auto submit: {}", config.injection.auto_submit);
            println!("  Clipboard behavior: {}", config.injection.clipboard_behavior);
            println!("  Clipboard clear delay: {}s", config.injection.clipboard_clear_delay);
            println!();
            println!("=== Feedback ===");
            println!("  Mic OSD enabled: {}", config.feedback.mic_osd_enabled);
            println!("  Audio feedback: {}", config.feedback.audio_feedback);
            println!("  Master volume: {}", config.feedback.master_volume);
        }
    }

    Ok(())
}

fn run_config_edit(paths: &Paths) -> Result<()> {
    if !paths.config_file.exists() {
        println!("No configuration file found at: {}", paths.config_file.display());
        println!("Run 'gwhspr config init' to create one.");
        return Ok(());
    }

    // Determine editor
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            // Try to find a common editor
            for editor in &["nano", "vim", "vi", "nvim", "code", "gedit"] {
                if Command::new("which")
                    .arg(editor)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
                {
                    return editor.to_string();
                }
            }
            "nano".to_string()
        });

    println!("Opening {} with {}...", paths.config_file.display(), editor);

    let status = Command::new(&editor)
        .arg(&paths.config_file)
        .status()?;

    if status.success() {
        // Validate the config after editing
        match ConfigManager::new(paths.clone()) {
            Ok(manager) => {
                if let Err(e) = manager.validate() {
                    println!("\nWarning: Configuration validation failed: {}", e);
                    println!("The daemon may not start correctly.");
                } else {
                    println!("\nConfiguration saved and validated successfully.");
                }
            }
            Err(e) => {
                println!("\nError: Failed to load configuration: {}", e);
                println!("Please fix the JSON syntax errors.");
            }
        }
    }

    Ok(())
}

fn run_config_get(key: &str, paths: &Paths) -> Result<()> {
    if !paths.config_file.exists() {
        anyhow::bail!("No configuration file found at: {}", paths.config_file.display());
    }

    let config_manager = ConfigManager::new(paths.clone())?;
    let config = config_manager.get_config();

    // Parse the key path and get the value
    let value = get_config_value(config, key)?;
    println!("{}", value);

    Ok(())
}

fn run_config_set(key: &str, value: &str, paths: &Paths) -> Result<()> {
    if !paths.config_file.exists() {
        anyhow::bail!("No configuration file found. Run 'gwhspr config init' first.");
    }

    let mut config_manager = ConfigManager::new(paths.clone())?;

    set_config_value(config_manager.get_config_mut(), key, value)?;
    config_manager.save()?;

    println!("Set {} = {}", key, value);
    Ok(())
}

fn get_config_value(config: &crate::types::Config, key: &str) -> Result<String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["shortcuts", "primary_shortcut"] => Ok(config.shortcuts.primary_shortcut.clone()),
        ["shortcuts", "recording_mode"] => Ok(format!("{:?}", config.shortcuts.recording_mode)),
        ["shortcuts", "grab_keys"] => Ok(config.shortcuts.grab_keys.to_string()),
        ["shortcuts", "auto_mode_threshold_ms"] => Ok(config.shortcuts.auto_mode_threshold_ms.to_string()),

        ["audio", "device_id"] => Ok(format!("{:?}", config.audio.device_id)),
        ["audio", "device_name"] => Ok(format!("{:?}", config.audio.device_name)),
        ["audio", "mute_detection"] => Ok(config.audio.mute_detection.to_string()),

        ["transcription", "backend"] => Ok(format!("{:?}", config.transcription.backend)),
        ["transcription", "model"] => Ok(config.transcription.model.clone()),
        ["transcription", "threads"] => Ok(config.transcription.threads.to_string()),
        ["transcription", "language"] => Ok(format!("{:?}", config.transcription.language)),

        ["injection", "paste_mode"] => Ok(format!("{:?}", config.injection.paste_mode)),
        ["injection", "auto_submit"] => Ok(config.injection.auto_submit.to_string()),

        ["feedback", "mic_osd_enabled"] => Ok(config.feedback.mic_osd_enabled.to_string()),
        ["feedback", "audio_feedback"] => Ok(config.feedback.audio_feedback.to_string()),
        ["feedback", "master_volume"] => Ok(config.feedback.master_volume.to_string()),

        _ => anyhow::bail!("Unknown config key: {}", key),
    }
}

fn set_config_value(config: &mut crate::types::Config, key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["shortcuts", "primary_shortcut"] => {
            config.shortcuts.primary_shortcut = value.to_string();
        }
        ["shortcuts", "recording_mode"] => {
            config.shortcuts.recording_mode = match value.to_lowercase().as_str() {
                "toggle" => crate::types::RecordingMode::Toggle,
                "push_to_talk" | "pushtotalk" => crate::types::RecordingMode::PushToTalk,
                "auto" => crate::types::RecordingMode::Auto,
                _ => anyhow::bail!("Invalid recording mode: {}", value),
            };
        }
        ["shortcuts", "grab_keys"] => {
            config.shortcuts.grab_keys = value.parse()?;
        }
        ["shortcuts", "auto_mode_threshold_ms"] => {
            config.shortcuts.auto_mode_threshold_ms = value.parse()?;
        }

        ["audio", "mute_detection"] => {
            config.audio.mute_detection = value.parse()?;
        }
        ["audio", "device_name"] => {
            config.audio.device_name = if value.is_empty() || value == "null" {
                None
            } else {
                Some(value.to_string())
            };
        }

        ["transcription", "backend"] => {
            config.transcription.backend = match value.to_lowercase().as_str() {
                "whisper" => crate::types::TranscriptionBackend::Whisper,
                "parakeet" | "parakeet_v3" | "parakeetv3" => crate::types::TranscriptionBackend::ParakeetV3,
                _ => anyhow::bail!("Invalid backend: {}", value),
            };
        }
        ["transcription", "model"] => {
            config.transcription.model = value.to_string();
        }
        ["transcription", "threads"] => {
            config.transcription.threads = value.parse()?;
        }
        ["transcription", "language"] => {
            config.transcription.language = if value.is_empty() || value == "null" || value == "auto" {
                None
            } else {
                Some(value.to_string())
            };
        }

        ["injection", "paste_mode"] => {
            config.injection.paste_mode = match value.to_lowercase().as_str() {
                "ctrl_shift" | "ctrlshift" => crate::types::PasteMode::CtrlShift,
                "ctrl" => crate::types::PasteMode::Ctrl,
                "super" => crate::types::PasteMode::Super,
                _ => anyhow::bail!("Invalid paste mode: {}", value),
            };
        }
        ["injection", "auto_submit"] => {
            config.injection.auto_submit = value.parse()?;
        }

        ["feedback", "mic_osd_enabled"] => {
            config.feedback.mic_osd_enabled = value.parse()?;
        }
        ["feedback", "audio_feedback"] => {
            config.feedback.audio_feedback = value.parse()?;
        }
        ["feedback", "master_volume"] => {
            config.feedback.master_volume = value.parse()?;
        }

        _ => anyhow::bail!("Unknown or read-only config key: {}", key),
    }

    Ok(())
}
