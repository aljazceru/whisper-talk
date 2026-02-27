use anyhow::Result;
use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::paths::Paths;

/// Available Whisper model sizes with their approximate download sizes
const MODELS: &[(&str, &str, &str)] = &[
    ("tiny", "ggml-tiny.bin", "~77 MB"),
    ("tiny.en", "ggml-tiny.en.bin", "~77 MB"),
    ("base", "ggml-base.bin", "~148 MB"),
    ("base.en", "ggml-base.en.bin", "~148 MB"),
    ("small", "ggml-small.bin", "~488 MB"),
    ("small.en", "ggml-small.en.bin", "~488 MB"),
    ("medium", "ggml-medium.bin", "~1.5 GB"),
    ("medium.en", "ggml-medium.en.bin", "~1.5 GB"),
    ("large-v1", "ggml-large-v1.bin", "~3.1 GB"),
    ("large-v2", "ggml-large-v2.bin", "~3.1 GB"),
    ("large-v3", "ggml-large-v3.bin", "~3.1 GB"),
    ("large-v3-turbo", "ggml-large-v3-turbo.bin", "~1.6 GB"),
];

const HUGGINGFACE_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const DOWNLOAD_TIMEOUT_SECONDS: u64 = 600;
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 2000;

#[derive(Args, Debug)]
pub struct ModelArgs {
    #[command(subcommand)]
    pub command: ModelCommands,
}

#[derive(Subcommand, Debug)]
pub enum ModelCommands {
    /// Download a Whisper model
    Download {
        /// Model name (tiny, base, small, medium, large-v3, etc.)
        name: String,
        /// Force re-download even if model exists
        #[arg(short, long)]
        force: bool,
    },
    /// List available models
    List,
    /// Show status of downloaded models
    Status,
}

pub fn run_model(args: ModelArgs) -> Result<()> {
    let paths = Paths::new()?;

    match args.command {
        ModelCommands::Download { name, force } => run_model_download(&name, force, &paths),
        ModelCommands::List => run_model_list(),
        ModelCommands::Status => run_model_status(&paths),
    }
}

fn run_model_download(name: &str, force: bool, paths: &Paths) -> Result<()> {
    // Find model info
    let model_info = MODELS.iter().find(|(n, _, _)| *n == name).or_else(|| {
        // Try exact filename match
        MODELS.iter().find(|(_, f, _)| f.contains(name))
    });

    let (model_name, filename, size) = match model_info {
        Some(info) => *info,
        None => {
            println!("Unknown model: {}", name);
            println!("\nAvailable models:");
            for (name, _, size) in MODELS {
                println!("  {} ({})", name, size);
            }
            return Ok(());
        }
    };

    // Check if model already exists
    let existing_path = find_existing_model(paths, filename);
    if let Some(path) = &existing_path {
        if !force {
            println!(
                "Model '{}' already exists at: {}",
                model_name,
                path.display()
            );
            println!("Use --force to re-download");
            return Ok(());
        } else {
            println!("Force flag set, will re-download model");
        }
    }

    // Determine target path
    let target_dir = &paths.models_dir;
    fs::create_dir_all(target_dir)?;
    let target_path = target_dir.join(filename);

    println!("Downloading Whisper model: {} ({})", model_name, size);
    println!("Target: {}", target_path.display());

    let url = format!("{}/{}", HUGGINGFACE_BASE_URL, filename);
    download_with_progress(&url, &target_path)?;

    println!("\nModel '{}' downloaded successfully!", model_name);
    println!(
        "You can now use it with: whisper-talk config set transcription.model {}",
        model_name
    );

    Ok(())
}

fn run_model_list() -> Result<()> {
    println!("Available Whisper models:\n");
    println!("{:<18} {:<30} SIZE", "NAME", "FILENAME");
    println!("{}", "-".repeat(60));

    for (name, filename, size) in MODELS {
        println!("{:<18} {:<30} {}", name, filename, size);
    }

    println!("\nModel variants:");
    println!("  .en models: English-only, slightly better for English");
    println!("  Regular models: Multilingual support");
    println!();
    println!("Recommended models:");
    println!("  - tiny/base: Fast, good for testing");
    println!("  - small: Good balance of speed and accuracy");
    println!("  - medium: High accuracy");
    println!("  - large-v3-turbo: Best accuracy with reasonable speed");
    println!("  - large-v3: Maximum accuracy (slower)");

    Ok(())
}

fn run_model_status(paths: &Paths) -> Result<()> {
    println!("Model status:\n");
    println!("Search directories:");
    for dir in &paths.model_search_dirs {
        let exists = if dir.exists() { "✓" } else { "✗" };
        println!("  {} {}", exists, dir.display());
    }
    println!();

    // Check for downloaded models
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
                        let size = fs::metadata(&path)
                            .map(|m| format_size(m.len()))
                            .unwrap_or_else(|_| "?".to_string());

                        // Extract model name from filename
                        let model_name = filename
                            .strip_prefix("ggml-")
                            .and_then(|s| s.strip_suffix(".bin"))
                            .unwrap_or(filename);

                        found_models.push((model_name.to_string(), path, size));
                    }
                }
            }
        }
    }

    if found_models.is_empty() {
        println!("No models found.");
        println!("\nRun 'whisper-talk model download <name>' to download a model.");
        println!("Run 'whisper-talk model list' to see available models.");
    } else {
        println!("Downloaded models:\n");
        println!("{:<20} {:<10} PATH", "MODEL", "SIZE");
        println!("{}", "-".repeat(70));

        for (name, path, size) in found_models {
            println!("{:<20} {:<10} {}", name, size, path.display());
        }
    }

    Ok(())
}

fn find_existing_model(paths: &Paths, filename: &str) -> Option<PathBuf> {
    for dir in &paths.model_search_dirs {
        let path = dir.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn download_with_progress(url: &str, target_path: &PathBuf) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
        .user_agent("whisper-talk/0.1.0")
        .build()?;

    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        if attempt > 1 {
            println!("Retry {}/{} after delay...", attempt, MAX_RETRIES);
            std::thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
        }

        match try_download(&client, url, target_path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("Download attempt {} failed: {}", attempt, e);
                last_error = Some(e);

                // Clean up partial download
                if target_path.exists() {
                    let _ = fs::remove_file(target_path);
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("Download failed after {} attempts", MAX_RETRIES)))
}

fn try_download(
    client: &reqwest::blocking::Client,
    url: &str,
    target_path: &PathBuf,
) -> Result<()> {
    let response = client.get(url).send()?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP error: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);

    // Set up progress bar
    let pb = if total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"));
        pb
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner().template("{spinner:.green} {bytes} downloaded")?,
        );
        pb
    };

    let file = File::create(target_path)?;
    let mut writer = BufWriter::new(file);

    let bytes = response.bytes()?;
    let chunk_size = 8192;

    for chunk in bytes.chunks(chunk_size) {
        writer.write_all(chunk)?;
        pb.inc(chunk.len() as u64);
    }

    writer.flush()?;
    pb.finish_with_message("Download complete");

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
