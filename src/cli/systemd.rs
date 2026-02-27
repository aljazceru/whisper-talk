use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Args, Debug)]
pub struct SystemdArgs {
    #[command(subcommand)]
    pub command: SystemdCommands,
}

#[derive(Subcommand, Debug)]
pub enum SystemdCommands {
    Install,
    Enable,
    Disable,
    Status,
    Restart,
}

const SERVICE_NAME: &str = "whisper-talk.service";
const SYSTEMD_USER_DIR: &str = ".config/systemd/user";
const LOCAL_BIN: &str = "/usr/local/bin/whisper-talk";

fn get_systemd_user_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let systemd_dir = PathBuf::from(home).join(SYSTEMD_USER_DIR);
    Ok(systemd_dir)
}

fn get_service_template_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().context("Failed to get executable path")?;
    let base_dir = exe_path
        .parent()
        .context("Failed to get parent directory")?;
    let template_path = base_dir.join("../../share/systemd/whisper-talk.service");
    Ok(template_path)
}

fn run_systemctl(args: &[&str]) -> Result<String> {
    let output = Command::new("systemctl")
        .args(["--user"])
        .args(args)
        .output()
        .context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("systemctl failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn run_systemd(args: SystemdArgs) -> Result<()> {
    match args.command {
        SystemdCommands::Install => install_service()?,
        SystemdCommands::Enable => enable_service()?,
        SystemdCommands::Disable => disable_service()?,
        SystemdCommands::Status => show_status()?,
        SystemdCommands::Restart => restart_service()?,
    }
    Ok(())
}

fn install_service() -> Result<()> {
    let systemd_dir = get_systemd_user_dir()?;

    fs::create_dir_all(&systemd_dir).context("Failed to create systemd user directory")?;

    let template_path = get_service_template_path()?;
    let service_content = fs::read_to_string(&template_path)
        .with_context(|| format!("Failed to read service template from {:?}", template_path))?;

    let exe_path = std::env::current_exe().context("Failed to get executable path")?;
    let exe_str = exe_path.to_string_lossy().to_string();

    let service_content = service_content.replace(LOCAL_BIN, &exe_str);

    let service_path = systemd_dir.join(SERVICE_NAME);
    fs::write(&service_path, service_content)
        .with_context(|| format!("Failed to write service file to {:?}", service_path))?;

    println!("Service file installed to {:?}", service_path);

    run_systemctl(&["daemon-reload"])?;
    println!("Systemd daemon reloaded");

    Ok(())
}

fn enable_service() -> Result<()> {
    run_systemctl(&["enable", SERVICE_NAME])?;
    println!("Service enabled to start on login");
    Ok(())
}

fn disable_service() -> Result<()> {
    run_systemctl(&["disable", SERVICE_NAME])?;
    println!("Service disabled from starting on login");
    Ok(())
}

fn show_status() -> Result<()> {
    run_systemctl(&["status", SERVICE_NAME])?;
    Ok(())
}

fn restart_service() -> Result<()> {
    run_systemctl(&["restart", SERVICE_NAME])?;
    println!("Service restarted");
    Ok(())
}
