use crate::error::Result;
use serde_json::{json, Value as JsonValue};
use std::fs;
use std::path::PathBuf;

pub enum WaybarCommand {
    Install,
    Remove,
    Status,
}

pub fn handle_waybar(command: WaybarCommand) -> Result<()> {
    match command {
        WaybarCommand::Install => install_waybar_module()?,
        WaybarCommand::Remove => remove_waybar_module()?,
        WaybarCommand::Status => {
            let installed = check_status()?;
            println!("Waybar module: {}", if installed { "installed" } else { "not installed" });
        }
    }
    Ok(())
}

fn get_waybar_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        crate::error::GwhsprError::System("Home directory not found".to_string())
    })?;
    Ok(home.join(".config/waybar/config"))
}

fn get_waybar_module_script_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        crate::error::GwhsprError::System("Home directory not found".to_string())
    })?;
    
    let script_path = home.join(".local/share/gwhspr/waybar/gwhspr.py");
    
    let script_dir = script_path.parent().unwrap();
    fs::create_dir_all(script_dir)?;
    
    Ok(script_path)
}

fn install_waybar_module() -> Result<()> {
    let config_path = get_waybar_config_path()?;
    
    if !config_path.exists() {
        return Err(crate::error::GwhsprError::System(
            "Waybar config not found. Is Waybar installed?".to_string()
        ));
    }
    
    let script_path = get_waybar_module_script_path()?;
    
    let script_content = r#"#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

STATE_DIR = Path.home() / ".local/state/gwhspr"
RECORDING_STATUS_FILE = STATE_DIR / "recording_status"
AUDIO_LEVEL_FILE = STATE_DIR / "audio_level"

def read_file_content(file_path):
    try:
        with open(file_path, 'r') as f:
            return f.read().strip()
    except (FileNotFoundError, IOError):
        return None

def main():
    recording_status = read_file_content(RECORDING_STATUS_FILE)
    audio_level = read_file_content(AUDIO_LEVEL_FILE)
    
    is_recording = recording_status == "recording"
    
    if is_recording:
        text = "🎙️ REC"
        tooltip = "Recording..."
        class_name = "recording"
    else:
        text = "🎙️"
        tooltip = "Ready to record"
        class_name = "idle"
    
    try:
        level = float(audio_level) if audio_level else 0.0
    except ValueError:
        level = 0.0
    
    output = {
        "text": text,
        "tooltip": tooltip,
        "class": class_name,
        "alt": "recording" if is_recording else "idle",
    }
    
    print(json.dumps(output))

if __name__ == "__main__":
    main()
"#;
    
    fs::write(&script_path, script_content)?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }
    
    let config_content = fs::read_to_string(&config_path)?;
    let mut config: JsonValue = serde_json::from_str(&config_content)
        .map_err(|e| crate::error::GwhsprError::System(format!("Failed to parse Waybar config: {}", e)))?;
    
    if let Some(modules_array) = config.get_mut("modules") {
        if let Some(modules) = modules_array.as_array_mut() {
            let module_name = "custom/gwhspr";
            if !modules.iter().any(|m| m.as_str() == Some(module_name)) {
                modules.push(json!(module_name));
            }
        }
    }
    
    if let Some(modules_obj) = config.get_mut("modules-right") {
        if let Some(modules) = modules_obj.as_array_mut() {
            let module_name = "custom/gwhspr";
            if !modules.iter().any(|m| m.as_str() == Some(module_name)) {
                modules.push(json!(module_name));
            }
        }
    }
    
    if let Some(modules_obj) = config.get_mut("modules-center") {
        if let Some(modules) = modules_obj.as_array_mut() {
            let module_name = "custom/gwhspr";
            if !modules.iter().any(|m| m.as_str() == Some(module_name)) {
                modules.push(json!(module_name));
            }
        }
    }
    
    if let Some(modules_obj) = config.get_mut("modules-left") {
        if let Some(modules) = modules_obj.as_array_mut() {
            let module_name = "custom/gwhspr";
            if !modules.iter().any(|m| m.as_str() == Some(module_name)) {
                modules.push(json!(module_name));
            }
        }
    }
    
    let module_config = json!({
        "custom/gwhspr": {
            "exec": script_path.to_string_lossy().to_string(),
            "return-type": "json",
            "format": "🎙️ {}",
            "interval": 1
        }
    });
    
    if let Some(modules) = config.get_mut("modules") {
        if modules.is_object() {
            modules.as_object_mut().unwrap().extend(module_config.as_object().unwrap().clone());
        }
    }
    
    let pretty_config = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, pretty_config)?;
    
    println!("Waybar module installed successfully");
    println!("Restart Waybar to apply changes: systemctl restart --user waybar");
    
    Ok(())
}

fn remove_waybar_module() -> Result<()> {
    let config_path = get_waybar_config_path()?;
    
    if !config_path.exists() {
        return Err(crate::error::GwhsprError::System(
            "Waybar config not found".to_string()
        ));
    }
    
    let config_content = fs::read_to_string(&config_path)?;
    let mut config: JsonValue = serde_json::from_str(&config_content)
        .map_err(|e| crate::error::GwhsprError::System(format!("Failed to parse Waybar config: {}", e)))?;
    
    let module_name = "custom/gwhspr";
    
    if let Some(modules_array) = config.get_mut("modules") {
        if let Some(modules) = modules_array.as_array_mut() {
            modules.retain(|m| m.as_str() != Some(module_name));
        }
    }
    
    if let Some(modules) = config.get_mut("modules-right") {
        if let Some(modules) = modules.as_array_mut() {
            modules.retain(|m| m.as_str() != Some(module_name));
        }
    }
    
    if let Some(modules) = config.get_mut("modules-center") {
        if let Some(modules) = modules.as_array_mut() {
            modules.retain(|m| m.as_str() != Some(module_name));
        }
    }
    
    if let Some(modules) = config.get_mut("modules-left") {
        if let Some(modules) = modules.as_array_mut() {
            modules.retain(|m| m.as_str() != Some(module_name));
        }
    }
    
    if let Some(modules) = config.get_mut("modules") {
        if modules.is_object() {
            modules.as_object_mut().unwrap().remove(module_name);
        }
    }
    
    let pretty_config = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, pretty_config)?;
    
    println!("Waybar module removed successfully");
    println!("Restart Waybar to apply changes: systemctl restart --user waybar");
    
    Ok(())
}

fn check_status() -> Result<bool> {
    let config_path = get_waybar_config_path()?;
    
    if !config_path.exists() {
        return Ok(false);
    }
    
    let config_content = fs::read_to_string(&config_path)?;
    let config: JsonValue = serde_json::from_str(&config_content)
        .map_err(|e| crate::error::GwhsprError::System(format!("Failed to parse Waybar config: {}", e)))?;
    
    let module_name = "custom/gwhspr";
    
    let in_modules = config.get("modules")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().any(|item| item.as_str() == Some(module_name)))
        .unwrap_or(false);
    
    let in_modules_left = config.get("modules-left")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().any(|item| item.as_str() == Some(module_name)))
        .unwrap_or(false);
    
    let in_modules_center = config.get("modules-center")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().any(|item| item.as_str() == Some(module_name)))
        .unwrap_or(false);
    
    let in_modules_right = config.get("modules-right")
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().any(|item| item.as_str() == Some(module_name)))
        .unwrap_or(false);
    
    let in_modules_obj = config.get("modules")
        .and_then(|m| m.as_object())
        .map(|obj| obj.contains_key(module_name))
        .unwrap_or(false);
    
    Ok(in_modules || in_modules_left || in_modules_center || in_modules_right || in_modules_obj)
}

pub fn write_recording_status(state_dir: &std::path::Path, is_recording: bool) -> Result<()> {
    let status_path = state_dir.join("recording_status");
    let status = if is_recording { "recording" } else { "idle" };
    fs::write(&status_path, status)?;
    Ok(())
}

pub fn write_audio_level(state_dir: &std::path::Path, level: f32) -> Result<()> {
    let level_path = state_dir.join("audio_level");
    fs::write(&level_path, level.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_check_status_no_config() {
        let result = check_status();
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
    
    #[test]
    fn test_write_recording_status() {
        let dir = tempdir().unwrap();
        write_recording_status(dir.path(), true).unwrap();
        
        let content = fs::read_to_string(dir.path().join("recording_status")).unwrap();
        assert_eq!(content, "recording");
    }
    
    #[test]
    fn test_write_audio_level() {
        let dir = tempdir().unwrap();
        write_audio_level(dir.path(), 0.75).unwrap();
        
        let content = fs::read_to_string(dir.path().join("audio_level")).unwrap();
        assert_eq!(content, "0.75");
    }
}
