#![allow(dead_code)]
pub use evdev::KeyCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedShortcut {
    pub modifiers: Vec<Modifier>,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
    RCtl,
    RAlt,
    RShift,
    RSuper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Digit(u8),
    Special(SpecialKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialKey {
    Enter,
    Esc,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutParseError {
    pub message: String,
}

impl std::fmt::Display for ShortcutParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ShortcutParseError {}

pub fn parse_shortcut(shortcut_str: &str) -> Result<ParsedShortcut, ShortcutParseError> {
    let parts: Vec<&str> = shortcut_str.split('+').collect();

    if parts.is_empty() {
        return Err(ShortcutParseError {
            message: "Shortcut string is empty".to_string(),
        });
    }

    let mut modifiers = Vec::new();
    let mut key_part = None;

    for (i, part) in parts.iter().enumerate() {
        let trimmed = part.trim().to_uppercase();

        if is_modifier(&trimmed) {
            let modifier = parse_modifier(&trimmed)?;
            modifiers.push(modifier);
        } else if i == parts.len() - 1 {
            key_part = Some(part.trim());
        } else {
            return Err(ShortcutParseError {
                message: format!(
                    "Invalid part in shortcut: '{}'. Expected modifier or key",
                    part
                ),
            });
        }
    }

    let key_str = key_part.ok_or_else(|| ShortcutParseError {
        message: "Shortcut must contain a key".to_string(),
    })?;

    let key = parse_key(key_str)?;

    Ok(ParsedShortcut { modifiers, key })
}

pub fn parse_modifier(mod_str: &str) -> Result<Modifier, ShortcutParseError> {
    match mod_str.trim().to_uppercase().as_str() {
        "CTRL" | "CONTROL" => Ok(Modifier::Ctrl),
        "RCTRL" | "RCONTROL" => Ok(Modifier::RCtl),
        "ALT" => Ok(Modifier::Alt),
        "RALT" => Ok(Modifier::RAlt),
        "SHIFT" => Ok(Modifier::Shift),
        "RSHIFT" => Ok(Modifier::RShift),
        "SUPER" | "META" | "CMD" | "WIN" => Ok(Modifier::Super),
        "RSUPER" | "RMETA" | "RCMD" | "RWIN" => Ok(Modifier::RSuper),
        _ => Err(ShortcutParseError {
            message: format!("Unknown modifier: '{}'", mod_str),
        }),
    }
}

pub fn is_modifier(mod_str: &str) -> bool {
    matches!(
        mod_str.trim().to_uppercase().as_str(),
        "CTRL"
            | "CONTROL"
            | "RCTRL"
            | "RCONTROL"
            | "ALT"
            | "RALT"
            | "SHIFT"
            | "RSHIFT"
            | "SUPER"
            | "META"
            | "CMD"
            | "WIN"
            | "RSUPER"
            | "RMETA"
            | "RCMD"
            | "RWIN"
    )
}

pub fn is_right_modifier(mod_str: &str) -> bool {
    matches!(
        mod_str.trim().to_uppercase().as_str(),
        "RCTRL" | "RCONTROL" | "RALT" | "RSHIFT" | "RSUPER" | "RMETA" | "RCMD" | "RWIN"
    )
}

pub fn parse_key(key_str: &str) -> Result<Key, ShortcutParseError> {
    let key_str = key_str.trim();

    if key_str.len() == 1 {
        let c = key_str.chars().next().unwrap();

        if c.is_ascii_lowercase() {
            return Ok(Key::Char(c));
        }

        if c.is_ascii_uppercase() {
            return Ok(Key::Char(c.to_ascii_lowercase()));
        }

        if c.is_ascii_digit() {
            return Ok(Key::Digit(c.to_digit(10).unwrap() as u8));
        }
    }

    let upper = key_str.to_uppercase();
    match upper.as_str() {
        "ENTER" | "RETURN" => Ok(Key::Special(SpecialKey::Enter)),
        "ESC" | "ESCAPE" => Ok(Key::Special(SpecialKey::Esc)),
        "SPACE" => Ok(Key::Special(SpecialKey::Space)),
        "TAB" => Ok(Key::Special(SpecialKey::Tab)),
        "BACKSPACE" => Ok(Key::Special(SpecialKey::Backspace)),
        "DELETE" | "DEL" => Ok(Key::Special(SpecialKey::Delete)),
        "INSERT" | "INS" => Ok(Key::Special(SpecialKey::Insert)),
        "HOME" => Ok(Key::Special(SpecialKey::Home)),
        "END" => Ok(Key::Special(SpecialKey::End)),
        "PAGEUP" | "PGUP" => Ok(Key::Special(SpecialKey::PageUp)),
        "PAGEDOWN" | "PGDN" => Ok(Key::Special(SpecialKey::PageDown)),
        "UP" => Ok(Key::Special(SpecialKey::Up)),
        "DOWN" => Ok(Key::Special(SpecialKey::Down)),
        "LEFT" => Ok(Key::Special(SpecialKey::Left)),
        "RIGHT" => Ok(Key::Special(SpecialKey::Right)),
        "F1" => Ok(Key::Special(SpecialKey::F1)),
        "F2" => Ok(Key::Special(SpecialKey::F2)),
        "F3" => Ok(Key::Special(SpecialKey::F3)),
        "F4" => Ok(Key::Special(SpecialKey::F4)),
        "F5" => Ok(Key::Special(SpecialKey::F5)),
        "F6" => Ok(Key::Special(SpecialKey::F6)),
        "F7" => Ok(Key::Special(SpecialKey::F7)),
        "F8" => Ok(Key::Special(SpecialKey::F8)),
        "F9" => Ok(Key::Special(SpecialKey::F9)),
        "F10" => Ok(Key::Special(SpecialKey::F10)),
        "F11" => Ok(Key::Special(SpecialKey::F11)),
        "F12" => Ok(Key::Special(SpecialKey::F12)),
        _ => Err(ShortcutParseError {
            message: format!(
                "Unknown key: '{}'. Expected a letter (a-z), digit (0-9), or special key",
                key_str
            ),
        }),
    }
}

impl Modifier {
    pub fn to_evdev_key(self) -> KeyCode {
        match self {
            Modifier::Ctrl => KeyCode::KEY_LEFTCTRL,
            Modifier::RCtl => KeyCode::KEY_RIGHTCTRL,
            Modifier::Alt => KeyCode::KEY_LEFTALT,
            Modifier::RAlt => KeyCode::KEY_RIGHTALT,
            Modifier::Shift => KeyCode::KEY_LEFTSHIFT,
            Modifier::RShift => KeyCode::KEY_RIGHTSHIFT,
            Modifier::Super => KeyCode::KEY_LEFTMETA,
            Modifier::RSuper => KeyCode::KEY_RIGHTMETA,
        }
    }
}

impl Key {
    pub fn to_evdev_key(self) -> Option<KeyCode> {
        match self {
            Key::Char(c) => match c {
                'a' => Some(KeyCode::KEY_A),
                'b' => Some(KeyCode::KEY_B),
                'c' => Some(KeyCode::KEY_C),
                'd' => Some(KeyCode::KEY_D),
                'e' => Some(KeyCode::KEY_E),
                'f' => Some(KeyCode::KEY_F),
                'g' => Some(KeyCode::KEY_G),
                'h' => Some(KeyCode::KEY_H),
                'i' => Some(KeyCode::KEY_I),
                'j' => Some(KeyCode::KEY_J),
                'k' => Some(KeyCode::KEY_K),
                'l' => Some(KeyCode::KEY_L),
                'm' => Some(KeyCode::KEY_M),
                'n' => Some(KeyCode::KEY_N),
                'o' => Some(KeyCode::KEY_O),
                'p' => Some(KeyCode::KEY_P),
                'q' => Some(KeyCode::KEY_Q),
                'r' => Some(KeyCode::KEY_R),
                's' => Some(KeyCode::KEY_S),
                't' => Some(KeyCode::KEY_T),
                'u' => Some(KeyCode::KEY_U),
                'v' => Some(KeyCode::KEY_V),
                'w' => Some(KeyCode::KEY_W),
                'x' => Some(KeyCode::KEY_X),
                'y' => Some(KeyCode::KEY_Y),
                'z' => Some(KeyCode::KEY_Z),
                _ => None,
            },
            Key::Digit(d) => match d {
                0 => Some(KeyCode::KEY_0),
                1 => Some(KeyCode::KEY_1),
                2 => Some(KeyCode::KEY_2),
                3 => Some(KeyCode::KEY_3),
                4 => Some(KeyCode::KEY_4),
                5 => Some(KeyCode::KEY_5),
                6 => Some(KeyCode::KEY_6),
                7 => Some(KeyCode::KEY_7),
                8 => Some(KeyCode::KEY_8),
                9 => Some(KeyCode::KEY_9),
                _ => None,
            },
            Key::Special(special) => Some(special.to_evdev_key()),
        }
    }
}

impl SpecialKey {
    pub fn to_evdev_key(self) -> KeyCode {
        match self {
            SpecialKey::Enter => KeyCode::KEY_ENTER,
            SpecialKey::Esc => KeyCode::KEY_ESC,
            SpecialKey::Space => KeyCode::KEY_SPACE,
            SpecialKey::Tab => KeyCode::KEY_TAB,
            SpecialKey::Backspace => KeyCode::KEY_BACKSPACE,
            SpecialKey::Delete => KeyCode::KEY_DELETE,
            SpecialKey::Insert => KeyCode::KEY_INSERT,
            SpecialKey::Home => KeyCode::KEY_HOME,
            SpecialKey::End => KeyCode::KEY_END,
            SpecialKey::PageUp => KeyCode::KEY_PAGEUP,
            SpecialKey::PageDown => KeyCode::KEY_PAGEDOWN,
            SpecialKey::Up => KeyCode::KEY_UP,
            SpecialKey::Down => KeyCode::KEY_DOWN,
            SpecialKey::Left => KeyCode::KEY_LEFT,
            SpecialKey::Right => KeyCode::KEY_RIGHT,
            SpecialKey::F1 => KeyCode::KEY_F1,
            SpecialKey::F2 => KeyCode::KEY_F2,
            SpecialKey::F3 => KeyCode::KEY_F3,
            SpecialKey::F4 => KeyCode::KEY_F4,
            SpecialKey::F5 => KeyCode::KEY_F5,
            SpecialKey::F6 => KeyCode::KEY_F6,
            SpecialKey::F7 => KeyCode::KEY_F7,
            SpecialKey::F8 => KeyCode::KEY_F8,
            SpecialKey::F9 => KeyCode::KEY_F9,
            SpecialKey::F10 => KeyCode::KEY_F10,
            SpecialKey::F11 => KeyCode::KEY_F11,
            SpecialKey::F12 => KeyCode::KEY_F12,
        }
    }
}

impl ParsedShortcut {
    pub fn matches(&self, pressed_keys: &[KeyCode]) -> bool {
        let required_modifier_keys: Vec<KeyCode> =
            self.modifiers.iter().map(|m| m.to_evdev_key()).collect();
        let main_key = match self.key.to_evdev_key() {
            Some(k) => k,
            None => return false,
        };

        for modifier_key in &required_modifier_keys {
            if !pressed_keys.contains(modifier_key) {
                return false;
            }
        }

        if !pressed_keys.contains(&main_key) {
            return false;
        }

        true
    }

    pub fn required_keys(&self) -> Vec<KeyCode> {
        let mut keys: Vec<KeyCode> = self.modifiers.iter().map(|m| m.to_evdev_key()).collect();
        if let Some(main_key) = self.key.to_evdev_key() {
            keys.push(main_key);
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_shortcut() {
        let shortcut = parse_shortcut("d").unwrap();
        assert_eq!(shortcut.modifiers, vec![]);
        assert_eq!(shortcut.key, Key::Char('d'));
    }

    #[test]
    fn test_parse_single_modifier() {
        let shortcut = parse_shortcut("CTRL+d").unwrap();
        assert_eq!(shortcut.modifiers, vec![Modifier::Ctrl]);
        assert_eq!(shortcut.key, Key::Char('d'));
    }

    #[test]
    fn test_parse_multiple_modifiers() {
        let shortcut = parse_shortcut("CTRL+ALT+SHIFT+d").unwrap();
        assert_eq!(shortcut.modifiers.len(), 3);
        assert!(shortcut.modifiers.contains(&Modifier::Ctrl));
        assert!(shortcut.modifiers.contains(&Modifier::Alt));
        assert!(shortcut.modifiers.contains(&Modifier::Shift));
        assert_eq!(shortcut.key, Key::Char('d'));
    }

    #[test]
    fn test_parse_with_super() {
        let shortcut = parse_shortcut("SUPER+ALT+d").unwrap();
        assert_eq!(shortcut.modifiers, vec![Modifier::Super, Modifier::Alt]);
        assert_eq!(shortcut.key, Key::Char('d'));
    }

    #[test]
    fn test_parse_special_key() {
        let shortcut = parse_shortcut("CTRL+ENTER").unwrap();
        assert_eq!(shortcut.modifiers, vec![Modifier::Ctrl]);
        assert_eq!(shortcut.key, Key::Special(SpecialKey::Enter));
    }

    #[test]
    fn test_parse_f_key() {
        let shortcut = parse_shortcut("F1").unwrap();
        assert_eq!(shortcut.modifiers, vec![]);
        assert_eq!(shortcut.key, Key::Special(SpecialKey::F1));
    }

    #[test]
    fn test_parse_digit() {
        let shortcut = parse_shortcut("CTRL+5").unwrap();
        assert_eq!(shortcut.modifiers, vec![Modifier::Ctrl]);
        assert_eq!(shortcut.key, Key::Digit(5));
    }

    #[test]
    fn test_parse_right_modifier() {
        let shortcut = parse_shortcut("RCTRL+ALT+d").unwrap();
        assert_eq!(shortcut.modifiers, vec![Modifier::RCtl, Modifier::Alt]);
        assert_eq!(shortcut.key, Key::Char('d'));
    }

    #[test]
    fn test_is_right_modifier() {
        assert!(is_right_modifier("RCTRL"));
        assert!(is_right_modifier("RALT"));
        assert!(!is_right_modifier("CTRL"));
        assert!(!is_right_modifier("ALT"));
    }

    #[test]
    fn test_case_insensitive() {
        let shortcut1 = parse_shortcut("CTRL+d").unwrap();
        let shortcut2 = parse_shortcut("ctrl+D").unwrap();
        assert_eq!(shortcut1, shortcut2);
    }

    #[test]
    fn test_invalid_shortcut_empty() {
        let result = parse_shortcut("");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_shortcut_no_key() {
        let result = parse_shortcut("CTRL+ALT");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_modifier() {
        let result = parse_shortcut("FOO+d");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_key() {
        let result = parse_shortcut("CTRL+@");
        assert!(result.is_err());
    }

    #[test]
    fn test_modifier_to_evdev() {
        assert_eq!(Modifier::Ctrl.to_evdev_key(), KeyCode::KEY_LEFTCTRL);
        assert_eq!(Modifier::RCtl.to_evdev_key(), KeyCode::KEY_RIGHTCTRL);
        assert_eq!(Modifier::Alt.to_evdev_key(), KeyCode::KEY_LEFTALT);
        assert_eq!(Modifier::Super.to_evdev_key(), KeyCode::KEY_LEFTMETA);
    }

    #[test]
    fn test_key_to_evdev_char() {
        assert_eq!(Key::Char('a').to_evdev_key(), Some(KeyCode::KEY_A));
        assert_eq!(Key::Char('z').to_evdev_key(), Some(KeyCode::KEY_Z));
    }

    #[test]
    fn test_key_to_evdev_digit() {
        assert_eq!(Key::Digit(0).to_evdev_key(), Some(KeyCode::KEY_0));
        assert_eq!(Key::Digit(9).to_evdev_key(), Some(KeyCode::KEY_9));
    }

    #[test]
    fn test_key_to_evdev_special() {
        assert_eq!(
            Key::Special(SpecialKey::Enter).to_evdev_key(),
            Some(KeyCode::KEY_ENTER)
        );
        assert_eq!(
            Key::Special(SpecialKey::F1).to_evdev_key(),
            Some(KeyCode::KEY_F1)
        );
    }

    #[test]
    fn test_matches_shortcut() {
        let shortcut = parse_shortcut("CTRL+ALT+d").unwrap();
        let pressed = vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTALT, KeyCode::KEY_D];
        assert!(shortcut.matches(&pressed));
    }

    #[test]
    fn test_does_not_match_missing_modifier() {
        let shortcut = parse_shortcut("CTRL+ALT+d").unwrap();
        let pressed = vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_D];
        assert!(!shortcut.matches(&pressed));
    }

    #[test]
    fn test_required_keys() {
        let shortcut = parse_shortcut("CTRL+ALT+d").unwrap();
        let keys = shortcut.required_keys();
        assert!(keys.contains(&KeyCode::KEY_LEFTCTRL));
        assert!(keys.contains(&KeyCode::KEY_LEFTALT));
        assert!(keys.contains(&KeyCode::KEY_D));
    }
}
