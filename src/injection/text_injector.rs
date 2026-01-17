use crate::error::{WhisperTalkError, Result};
use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub struct TextInjector {
    auto_submit: bool,
    word_overrides: HashMap<String, String>,
    use_wtype: bool,
    // Configuration for clipboard injection
    clipboard_behavior: bool,
    paste_mode: crate::types::PasteMode,
    clipboard_clear_delay: f64,
}

impl TextInjector {
    pub fn new(config: &crate::types::InjectionConfig) -> Self {
        // Check if wtype is available AND actually works
        // (some compositors don't support the virtual keyboard protocol)
        let use_wtype = Self::test_wtype();

        if use_wtype {
            println!("Using wtype for text injection (keyboard layout aware)");
        } else {
            println!("Using ydotool for text injection");
        }

        Self {
            auto_submit: config.auto_submit,
            word_overrides: HashMap::new(),
            use_wtype,
            clipboard_behavior: config.clipboard_behavior,
            paste_mode: config.paste_mode,
            clipboard_clear_delay: config.clipboard_clear_delay,
        }
    }

    fn test_wtype() -> bool {
        // First check if wtype is installed
        let installed = Command::new("which")
            .arg("wtype")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !installed {
            return false;
        }

        // Test if wtype actually works with this compositor
        // Use -M/-m to press and release a modifier key (no visible effect)
        let output = match Command::new("wtype")
            .args(["-M", "shift", "-m", "shift"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return false,
        };

        // Check both exit code and stderr for the error message
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || stderr.contains("does not support") {
            println!("wtype installed but compositor doesn't support virtual keyboard protocol");
            return false;
        }

        true
    }

    pub fn set_word_overrides(&mut self, overrides: HashMap<String, String>) {
        self.word_overrides = overrides;
    }

    pub fn inject_text(&self, text: &str) -> Result<()> {
        let processed = self.apply_word_overrides(text);

        if processed.is_empty() {
            return Ok(());
        }

        // Use clipboard injection if enabled
        if self.clipboard_behavior {
            return self.inject_via_clipboard(&processed);
        }

        // Clear any stuck modifier keys first (only needed for ydotool)
        if !self.use_wtype {
            self.clear_modifiers()?;
            thread::sleep(Duration::from_millis(20));
        }

        // Type the text
        self.type_text(&processed)?;

        if self.auto_submit {
            thread::sleep(Duration::from_millis(50));
            self.send_enter_key()?;
        }

        Ok(())
    }

    fn inject_via_clipboard(&self, text: &str) -> Result<()> {
        // 1. Copy to clipboard
        self.copy_to_clipboard(text)?;
        
        // 2. Clear modifiers just in case
        if !self.use_wtype {
            self.clear_modifiers()?;
            thread::sleep(Duration::from_millis(20));
        }

        // 3. Send paste shortcut
        self.send_paste_shortcut()?;

        // 4. Optionally clear clipboard after delay (spawn thread to not block)
        if self.clipboard_clear_delay > 0.0 {
            let delay = self.clipboard_clear_delay;
            thread::spawn(move || {
                thread::sleep(Duration::from_secs_f64(delay));
                // We can't easily clear without overwriting, so we leaving it for now
                // or we could overwrite with empty string?
                // For now, let's just leave it to avoid complexity
            });
        }

        Ok(())
    }

    fn copy_to_clipboard(&self, text: &str) -> Result<()> {
        // Try wl-copy first (Wayland)
        let mut child = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn();

        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return Ok(());
        }

        // Fallback to xclip (X11)
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard", "-i"])
            .stdin(std::process::Stdio::piped())
            .spawn();

        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return Ok(());
        }

        // If both fail, and we are on Linux, we might be missing tools
        Err(WhisperTalkError::Injection("Failed to copy to clipboard: neither wl-copy nor xclip found".to_string()))
    }

    fn send_paste_shortcut(&self) -> Result<()> {
        // Sleep briefly to ensure modifiers are clear
        thread::sleep(Duration::from_millis(50));

        if self.use_wtype {
            // wtype implementation of paste
             match self.paste_mode {
                crate::types::PasteMode::CtrlShift => {
                    Command::new("wtype").args(["-M", "ctrl", "-M", "shift", "-k", "v", "-m", "shift", "-m", "ctrl"]).status()
                }
                crate::types::PasteMode::Ctrl => {
                    Command::new("wtype").args(["-M", "ctrl", "-k", "v", "-m", "ctrl"]).status()
                }
                crate::types::PasteMode::Super => {
                    Command::new("wtype").args(["-M", "win", "-k", "v", "-m", "win"]).status()
                }
            }.map_err(|e| WhisperTalkError::Injection(format!("Failed to send paste with wtype: {}", e)))?;
        } else {
             // ydotool implementation
             // Codes: Ctrl=29, Shift=42, Super=125, V=47
             let args = match self.paste_mode {
                crate::types::PasteMode::CtrlShift => {
                    // key 29:1 42:1 47:1 47:0 42:0 29:0
                    vec!["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
                }
                crate::types::PasteMode::Ctrl => {
                    // key 29:1 47:1 47:0 29:0
                     vec!["key", "29:1", "47:1", "47:0", "29:0"]
                }
                crate::types::PasteMode::Super => {
                    // key 125:1 47:1 47:0 125:0
                     vec!["key", "125:1", "47:1", "47:0", "125:0"]
                }
            };
            
            Command::new("ydotool")
                .args(&args)
                .status()
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to send paste with ydotool: {}", e)))?;
        }
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<()> {
        if self.use_wtype {
            // wtype respects keyboard layout
            let status = Command::new("wtype")
                .arg("--")
                .arg(text)
                .status()
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to run wtype: {}", e)))?;

            if !status.success() {
                return Err(WhisperTalkError::Injection(format!(
                    "wtype failed with status: {}",
                    status
                )));
            }
        } else {
            // ydotool fallback (may have keyboard layout issues)
            let status = Command::new("ydotool")
                .args(["type", "--", text])
                .status()
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to run ydotool type: {}", e)))?;

            if !status.success() {
                return Err(WhisperTalkError::Injection(format!(
                    "ydotool type failed with status: {}",
                    status
                )));
            }
        }

        Ok(())
    }

    fn clear_modifiers(&self) -> Result<()> {
        // Release common modifier keys that might be stuck
        let modifiers = vec!["125:0", "126:0", "56:0", "100:0", "29:0", "97:0", "42:0", "54:0"];

        for modifier in &modifiers {
            let _ = Command::new("ydotool")
                .args(["key", modifier])
                .output();
            thread::sleep(Duration::from_millis(5));
        }

        Ok(())
    }

    fn send_enter_key(&self) -> Result<()> {
        if self.use_wtype {
            Command::new("wtype")
                .args(["-k", "Return"])
                .status()
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Enter: {}", e)))?;
        } else {
            Command::new("ydotool")
                .args(["key", "28:1", "28:0"])
                .status()
                .map_err(|e| WhisperTalkError::Injection(format!("Failed to send Enter: {}", e)))?;
        }
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
}
