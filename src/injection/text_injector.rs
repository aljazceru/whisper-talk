use crate::error::{WhisperTalkError, Result};
use crate::types::PasteMode;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct HyprWindowInfo {
    class: Option<String>,
}

pub struct TextInjector {
    paste_mode: PasteMode,
    auto_submit: bool,
    clipboard_behavior: bool,
    clipboard_clear_delay: f64,
    word_overrides: HashMap<String, String>,
}

impl TextInjector {
    pub fn new(config: &crate::types::InjectionConfig) -> Self {
        Self {
            paste_mode: config.paste_mode,
            auto_submit: config.auto_submit,
            clipboard_behavior: config.clipboard_behavior,
            clipboard_clear_delay: config.clipboard_clear_delay,
            word_overrides: HashMap::new(),
        }
    }

    pub fn set_word_overrides(&mut self, overrides: HashMap<String, String>) {
        self.word_overrides = overrides;
    }

    pub fn inject_text(&self, text: &str) -> Result<()> {
        let processed = self.apply_word_overrides(text);
        let is_kitty = self.is_kitty_terminal()?;

        if let Err(e) = self.copy_to_clipboard(&processed) {
            eprintln!("Failed to copy to clipboard: {}", e);
        }
        thread::sleep(Duration::from_millis(50));

        self.clear_modifiers()?;
        thread::sleep(Duration::from_millis(20));

        if is_kitty {
            self.send_paste_keys_slow()?;
        } else {
            self.send_paste_keys_normal()?;
        }

        if self.auto_submit {
            thread::sleep(Duration::from_millis(50));
            self.send_enter_key()?;
        }

        if self.clipboard_behavior {
            thread::spawn({
                let delay_secs = self.clipboard_clear_delay;
                move || {
                    thread::sleep(Duration::from_secs_f64(delay_secs));
                    let _ = Self::clear_clipboard();
                }
            });
        }

        Ok(())
    }

    fn is_kitty_terminal(&self) -> Result<bool> {
        match Command::new("hyprctl")
            .args(&["activewindow", "-j"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    if let Ok(window_info) = serde_json::from_slice::<HyprWindowInfo>(&output.stdout) {
                        if let Some(class) = window_info.class {
                            let class_lower = class.to_lowercase();
                            return Ok(class_lower.contains("kitty")
                                || class_lower.contains("wezterm")
                                || class_lower.contains("ghostty")
                                || class_lower.contains("org.wezfurlong.wezterm"));
                        }
                    }
                }
                Ok(false)
            }
            Err(_) => Ok(false),
        }
    }

    fn clear_modifiers(&self) -> Result<()> {
        let modifiers = vec!["125:0", "126:0", "56:0", "100:0", "29:0", "97:0", "42:0", "54:0"];

        for modifier in &modifiers {
            let _ = Command::new("ydotool")
                .args(&["key", modifier])
                .output();
            thread::sleep(Duration::from_millis(5));
        }

        Ok(())
    }

    fn send_paste_keys_slow(&self) -> Result<()> {
        match self.paste_mode {
            PasteMode::CtrlShift => {
                Command::new("ydotool")
                    .args(&["key", "29:1", "42:1"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Ctrl+Shift: {}", e)))?;
                thread::sleep(Duration::from_millis(15));
                Command::new("ydotool")
                    .args(&["key", "47:1", "47:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send V: {}", e)))?;
                thread::sleep(Duration::from_millis(10));
                Command::new("ydotool")
                    .args(&["key", "42:0", "29:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to release Ctrl+Shift: {}", e)))?;
            }
            PasteMode::Ctrl => {
                Command::new("ydotool")
                    .args(&["key", "29:1"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Ctrl: {}", e)))?;
                thread::sleep(Duration::from_millis(15));
                Command::new("ydotool")
                    .args(&["key", "47:1", "47:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send V: {}", e)))?;
                thread::sleep(Duration::from_millis(10));
                Command::new("ydotool")
                    .args(&["key", "29:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to release Ctrl: {}", e)))?;
            }
            PasteMode::Super => {
                Command::new("ydotool")
                    .args(&["key", "125:1"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Super: {}", e)))?;
                thread::sleep(Duration::from_millis(15));
                Command::new("ydotool")
                    .args(&["key", "47:1", "47:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to send V: {}", e)))?;
                thread::sleep(Duration::from_millis(10));
                Command::new("ydotool")
                    .args(&["key", "125:0"])
                    .spawn()
                    .map_err(|e| WhisperTalkError::Injection(format!("Failed to release Super: {}", e)))?;
            }
        }

        Ok(())
    }

    fn send_paste_keys_normal(&self) -> Result<()> {
        let keys = match self.paste_mode {
            PasteMode::CtrlShift => vec!["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
            PasteMode::Ctrl => vec!["29:1", "47:1", "47:0", "29:0"],
            PasteMode::Super => vec!["125:1", "47:1", "47:0", "125:0"],
        };

        Command::new("ydotool")
            .args(&["key"])
            .args(&keys)
            .spawn()
            .map_err(|e| WhisperTalkError::Injection(format!("Failed to send paste keys: {}", e)))?;

        Ok(())
    }

    fn send_enter_key(&self) -> Result<()> {
        Command::new("ydotool")
            .args(&["key", "28:1", "28:0"])
            .spawn()
            .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Enter: {}", e)))?;
        Ok(())
    }

    fn clear_clipboard() -> Result<()> {
        let _ = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"");
                }
                child.wait()
            });

        Ok(())
    }

    fn apply_word_overrides(&self, text: &str) -> String {
        let mut processed = text.to_string();

        for (original, replacement) in &self.word_overrides {
            if !original.is_empty() && !replacement.is_empty() {
                let pattern = format!(r"\b{}\b", regex::escape(original));
                if let Ok(re) = regex::Regex::new(&pattern) {
                    processed = re.replace_all(&processed, replacement.as_str()).to_string();
                }
            }
        }

        processed
    }

    fn copy_to_clipboard(&self, text: &str) -> Result<()> {
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| WhisperTalkError::Injection(format!("Failed to spawn wl-copy: {}", e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to write to wl-copy stdin: {}", e)))?;
            // stdin is dropped here, signaling EOF to wl-copy
        }

        // Don't wait for wl-copy - it stays alive to serve clipboard requests
        // until the content is pasted or replaced. Waiting would block forever.

        Ok(())
    }
}
