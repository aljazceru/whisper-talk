#![allow(dead_code)]
use crate::error::{Result, WhisperTalkError};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

static LAYOUT_CACHE: Lazy<Mutex<HashMap<String, KeyboardLayout>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Deserialize)]
struct HyprlandDevice {
    #[serde(rename = "activeKeyboardKeymap")]
    active_keyboard_keymap: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HyprlandDevices {
    keyboards: Option<Vec<HyprlandDevice>>,
}

#[derive(Clone)]
pub struct KeyboardLayout {
    pub layout: String,
    pub variant: Option<String>,
    pub char_to_keycode: HashMap<char, u16>,
}

impl KeyboardLayout {
    pub fn from_hyprland() -> Result<Self> {
        let output = Command::new("hyprctl")
            .args(["devices", "-j"])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    WhisperTalkError::Input(
                        "Hyprland not running or hyprctl not available".to_string(),
                    )
                } else {
                    WhisperTalkError::Input(format!("Failed to execute hyprctl: {}", e))
                }
            })?;

        if !output.status.success() {
            return Err(WhisperTalkError::Input(format!(
                "hyprctl failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let devices: HyprlandDevices = serde_json::from_slice(&output.stdout).map_err(|e| {
            WhisperTalkError::Input(format!("Failed to parse hyprctl output: {}", e))
        })?;

        let keymap = devices
            .keyboards
            .and_then(|kb| kb.into_iter().next())
            .and_then(|k| k.active_keyboard_keymap)
            .unwrap_or_else(|| "us".to_string());

        Self::parse_keymap(&keymap)
    }

    pub fn new(layout: &str, variant: Option<&str>) -> Result<Self> {
        let cache_key = if let Some(v) = variant {
            format!("{}:{}", layout, v)
        } else {
            layout.to_string()
        };

        {
            let cache = LAYOUT_CACHE.lock();
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(Self {
                    layout: cached.layout.clone(),
                    variant: cached.variant.clone(),
                    char_to_keycode: cached.char_to_keycode.clone(),
                });
            }
        }

        let layout_obj = if layout == "us" && variant.is_none() {
            Self {
                layout: layout.to_string(),
                variant: variant.map(|v| v.to_string()),
                char_to_keycode: HashMap::new(),
            }
        } else {
            let char_to_keycode = Self::compile_keymap(layout, variant)?;
            Self {
                layout: layout.to_string(),
                variant: variant.map(|v| v.to_string()),
                char_to_keycode,
            }
        };

        {
            let mut cache = LAYOUT_CACHE.lock();
            cache.insert(cache_key, layout_obj.clone());
        }

        Ok(layout_obj)
    }

    pub fn get_keycode(&self, ch: char) -> Option<u16> {
        self.char_to_keycode.get(&ch).copied()
    }

    fn parse_keymap(keymap: &str) -> Result<Self> {
        let parts: Vec<&str> = keymap.split(',').collect();
        let layout = parts.first().unwrap_or(&"us").to_string();
        let variant = if parts.len() > 1 && !parts[1].is_empty() {
            Some(parts[1].to_string())
        } else {
            None
        };

        Self::new(&layout, variant.as_deref())
    }

    fn compile_keymap(layout: &str, variant: Option<&str>) -> Result<HashMap<char, u16>> {
        let mut cmd = Command::new("xkbcli");
        cmd.args(["compile-keymap", "--format=verbose"]);

        if let Some(v) = variant {
            cmd.arg(format!("layout({})", layout));
            cmd.arg(format!("variant({})", v));
        } else {
            cmd.arg(format!("layout({})", layout));
        }

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                WhisperTalkError::Input("xkbcli not available".to_string())
            } else {
                WhisperTalkError::Input(format!("Failed to execute xkbcli: {}", e))
            }
        })?;

        if !output.status.success() {
            return Err(WhisperTalkError::Input(format!(
                "xkbcli failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Self::parse_xkb_output(&String::from_utf8_lossy(&output.stdout))
    }

    fn parse_xkb_output(output: &str) -> Result<HashMap<char, u16>> {
        let mut char_to_keycode = HashMap::new();

        for line in output.lines() {
            if let Some(mapping) = Self::parse_keycode_line(line) {
                char_to_keycode.extend(mapping);
            }
        }

        Ok(char_to_keycode)
    }

    fn parse_keycode_line(line: &str) -> Option<Vec<(char, u16)>> {
        let line = line.trim();
        if !line.starts_with("keycode") {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

        let keycode_str = parts[1].strip_prefix('=')?;
        let keycode: u16 = keycode_str.parse().ok()?;

        let mut mappings = Vec::new();

        for part in &parts[2..] {
            if let Some(symbols) = Self::parse_symbols(part) {
                mappings.extend(symbols.into_iter().map(|ch| (ch, keycode)));
            }
        }

        Some(mappings)
    }

    fn parse_symbols(symbols: &str) -> Option<Vec<char>> {
        let symbols = symbols.trim_matches('{').trim_matches('}');
        let parts: Vec<&str> = symbols.split(',').collect();

        let mut result = Vec::new();
        for part in &parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(ch) = Self::parse_symbol(part) {
                result.push(ch);
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    fn parse_symbol(symbol: &str) -> Option<char> {
        let symbol = symbol.trim();

        if let Some(code) = symbol
            .strip_prefix("\\u")
            .or_else(|| symbol.strip_prefix("\\x"))
        {
            let num = u32::from_str_radix(code, 16).ok()?;
            Some(char::from_u32(num)?)
        } else if symbol.starts_with('\\') && symbol.len() > 1 {
            let code = &symbol[1..];
            let num = u32::from_str_radix(code, 16).ok()?;
            Some(char::from_u32(num)?)
        } else if symbol == "space" {
            Some(' ')
        } else if symbol == "Tab" {
            Some('\t')
        } else if symbol == "Return" {
            Some('\n')
        } else if symbol == "BackSpace" {
            Some('\x08')
        } else if symbol == "Escape" {
            Some('\x1b')
        } else if symbol == "exclam" {
            Some('!')
        } else if symbol == "at" {
            Some('@')
        } else if symbol == "numbersign" {
            Some('#')
        } else if symbol == "dollar" {
            Some('$')
        } else if symbol == "percent" {
            Some('%')
        } else if symbol == "asciicircum" {
            Some('^')
        } else if symbol == "ampersand" {
            Some('&')
        } else if symbol == "asterisk" {
            Some('*')
        } else if symbol == "parenleft" {
            Some('(')
        } else if symbol == "parenright" {
            Some(')')
        } else if symbol == "minus" {
            Some('-')
        } else if symbol == "underscore" {
            Some('_')
        } else if symbol == "equal" {
            Some('=')
        } else if symbol == "plus" {
            Some('+')
        } else if symbol == "bracketleft" {
            Some('[')
        } else if symbol == "bracketright" {
            Some(']')
        } else if symbol == "braceleft" {
            Some('{')
        } else if symbol == "braceright" {
            Some('}')
        } else if symbol == "semicolon" {
            Some(';')
        } else if symbol == "colon" {
            Some(':')
        } else if symbol == "apostrophe" {
            Some('\'')
        } else if symbol == "quotedbl" {
            Some('"')
        } else if symbol == "comma" {
            Some(',')
        } else if symbol == "period" {
            Some('.')
        } else if symbol == "slash" {
            Some('/')
        } else if symbol == "backslash" {
            Some('\\')
        } else if symbol == "bar" {
            Some('|')
        } else if symbol == "grave" {
            Some('`')
        } else if symbol == "asciitilde" {
            Some('~')
        } else if symbol == "less" {
            Some('<')
        } else if symbol == "greater" {
            Some('>')
        } else if symbol == "question" {
            Some('?')
        } else if symbol.len() == 1 {
            Some(symbol.chars().next()?)
        } else if symbol.len() == 3 && symbol.starts_with('\'') && symbol.ends_with('\'') {
            Some(symbol.chars().nth(1)?)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_symbol() {
        assert_eq!(KeyboardLayout::parse_symbol("a"), Some('a'));
        assert_eq!(KeyboardLayout::parse_symbol("space"), Some(' '));
        assert_eq!(KeyboardLayout::parse_symbol("exclam"), Some('!'));
        assert_eq!(KeyboardLayout::parse_symbol("\\u0041"), Some('A'));
        assert_eq!(KeyboardLayout::parse_symbol("\\x41"), Some('A'));
    }

    #[test]
    fn test_parse_symbols() {
        let symbols = "{a,b,c}";
        let result = KeyboardLayout::parse_symbols(symbols);
        assert_eq!(result, Some(vec!['a', 'b', 'c']));
    }

    #[test]
    fn test_qwerty_uses_empty_map() {
        let layout = KeyboardLayout::new("us", None).unwrap();
        assert!(layout.char_to_keycode.is_empty());
    }

    #[test]
    fn test_layout_from_hyprland() {
        let hyprctl_available = std::process::Command::new("hyprctl")
            .args(["devices", "-j"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !hyprctl_available {
            return;
        }

        let result = KeyboardLayout::from_hyprland();
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            assert!(result.is_ok());
        }
    }
}
