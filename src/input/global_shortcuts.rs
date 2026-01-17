use crate::error::{WhisperTalkError, Result};
use crate::types::RecordingMode;
use evdev::{Device, KeyCode};
use parking_lot::Mutex;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct GlobalShortcuts {
    device: Option<Arc<Mutex<Device>>>,
    shortcut: Shortcut,
    on_press: Arc<dyn Fn() + Send + Sync>,
    on_release: Arc<dyn Fn() + Send + Sync>,
    is_running: Arc<Mutex<bool>>,
    grab_mode: bool,
}

impl Clone for GlobalShortcuts {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            shortcut: self.shortcut.clone(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
            is_running: self.is_running.clone(),
            grab_mode: self.grab_mode,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Shortcut {
    pub modifiers: Vec<KeyCode>,
    pub key: KeyCode,
}

impl GlobalShortcuts {
    pub fn new(
        shortcut_str: &str,
        on_press: impl Fn() + Send + Sync + 'static,
        on_release: impl Fn() + Send + Sync + 'static,
        device_path: Option<&str>,
        grab_mode: bool,
    ) -> Result<Self> {
        let shortcut = Self::parse_shortcut(shortcut_str)?;
        let on_press = Arc::new(on_press);
        let on_release = Arc::new(on_release);

        let device = if let Some(path) = device_path {
            Some(Arc::new(Mutex::new(Device::open(path)?)))
        } else {
            Self::find_keyboard_device()?.map(|d| Arc::new(Mutex::new(d)))
        };

        if let Some(ref dev) = device {
            if grab_mode {
                unsafe {
                    let fd = dev.lock().as_raw_fd();
                    libc::ioctl(fd, 0x80044590, 1);
                }
            }
        }

        println!("Global shortcuts initialized: {:?}", shortcut);

        Ok(Self {
            device,
            shortcut,
            on_press,
            on_release,
            is_running: Arc::new(Mutex::new(false)),
            grab_mode,
        })
    }

    fn find_keyboard_device() -> Result<Option<Device>> {
        let mut candidates: Vec<(std::path::PathBuf, String, usize, bool)> = Vec::new();

        let entries: Vec<_> = std::fs::read_dir("/dev/input")
            .map_err(|e| WhisperTalkError::Input(format!("Failed to read /dev/input: {}", e)))?
            .filter_map(|e| e.ok())
            .collect();

        eprintln!("Scanning {} input devices...", entries.len());

        for entry in entries {
            let entry_path = entry.path();
            let path_str = entry_path.to_string_lossy();
            if path_str.contains("event") {
                match Device::open(&entry_path) {
                    Ok(dev) => {
                        let key_count = dev.supported_keys().map_or(0, |keys| keys.into_iter().count());
                        let name = dev.name().unwrap_or("Unknown").to_string();
                        let name_lower = name.to_lowercase();

                        // Skip virtual devices (ydotool, uinput, etc.)
                        let is_virtual = name_lower.contains("ydotool")
                            || name_lower.contains("virtual")
                            || name_lower.contains("uinput");

                        eprintln!("  {}: {} ({} keys){}",
                            entry_path.display(), name, key_count,
                            if is_virtual { " [virtual, skipped]" } else { "" });

                        if key_count > 50 && !is_virtual {
                            // Prefer devices with "keyboard" in the name
                            let is_keyboard = name_lower.contains("keyboard");
                            candidates.push((entry_path.clone(), name, key_count, is_keyboard));
                        }
                    }
                    Err(e) => {
                        eprintln!("  {}: Error opening - {}", entry_path.display(), e);
                    }
                }
            }
        }

        // Sort: keyboards first, then by key count descending
        candidates.sort_by(|a, b| {
            match (a.3, b.3) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.2.cmp(&a.2),
            }
        });

        if let Some((path, name, key_count, _)) = candidates.first() {
            println!("Using keyboard device: {} ({}, {} keys)", path.display(), name, key_count);
            return Ok(Some(Device::open(path)?));
        }

        eprintln!("No suitable keyboard device found in {} candidates", candidates.len());
        Ok(None)
    }

    fn parse_shortcut(shortcut_str: &str) -> Result<Shortcut> {
        let parts: Vec<&str> = shortcut_str.split('+').collect();
        if parts.is_empty() {
            return Err(WhisperTalkError::Input("Invalid shortcut format".to_string()));
        }

        let mut modifiers = Vec::new();
        for part in &parts[..parts.len() - 1] {
            let modifier = match part.trim().to_uppercase().as_str() {
                "CTRL" | "CONTROL" | "LCTRL" => KeyCode::KEY_LEFTCTRL,
                "ALT" | "LALT" => KeyCode::KEY_LEFTALT,
                "SHIFT" | "LSHIFT" => KeyCode::KEY_LEFTSHIFT,
                "SUPER" | "META" | "LSUPER" => KeyCode::KEY_LEFTMETA,
                _ => return Err(WhisperTalkError::Input(format!("Unknown modifier: {}", part))),
            };
            modifiers.push(modifier);
        }

        let key_part = parts.last().unwrap().trim().to_uppercase();
        let key = match key_part.as_str() {
            "D" => KeyCode::KEY_D,
            "SPACE" => KeyCode::KEY_SPACE,
            "ENTER" => KeyCode::KEY_ENTER,
            "ESC" | "ESCAPE" => KeyCode::KEY_ESC,
            _ => {
                if key_part.len() == 1 {
                    Self::char_to_key(key_part.chars().next().unwrap())?
                } else {
                    return Err(WhisperTalkError::Input(format!("Unknown key: {}", key_part)));
                }
            }
        };

        Ok(Shortcut { modifiers, key })
    }

    fn char_to_key(c: char) -> Result<KeyCode> {
        match c.to_ascii_uppercase() {
            'A' => Ok(KeyCode::KEY_A),
            'B' => Ok(KeyCode::KEY_B),
            'C' => Ok(KeyCode::KEY_C),
            'D' => Ok(KeyCode::KEY_D),
            'E' => Ok(KeyCode::KEY_E),
            'F' => Ok(KeyCode::KEY_F),
            'G' => Ok(KeyCode::KEY_G),
            'H' => Ok(KeyCode::KEY_H),
            'I' => Ok(KeyCode::KEY_I),
            'J' => Ok(KeyCode::KEY_J),
            'K' => Ok(KeyCode::KEY_K),
            'L' => Ok(KeyCode::KEY_L),
            'M' => Ok(KeyCode::KEY_M),
            'N' => Ok(KeyCode::KEY_N),
            'O' => Ok(KeyCode::KEY_O),
            'P' => Ok(KeyCode::KEY_P),
            'Q' => Ok(KeyCode::KEY_Q),
            'R' => Ok(KeyCode::KEY_R),
            'S' => Ok(KeyCode::KEY_S),
            'T' => Ok(KeyCode::KEY_T),
            'U' => Ok(KeyCode::KEY_U),
            'V' => Ok(KeyCode::KEY_V),
            'W' => Ok(KeyCode::KEY_W),
            'X' => Ok(KeyCode::KEY_X),
            'Y' => Ok(KeyCode::KEY_Y),
            'Z' => Ok(KeyCode::KEY_Z),
            '0' => Ok(KeyCode::KEY_0),
            '1' => Ok(KeyCode::KEY_1),
            '2' => Ok(KeyCode::KEY_2),
            '3' => Ok(KeyCode::KEY_3),
            '4' => Ok(KeyCode::KEY_4),
            '5' => Ok(KeyCode::KEY_5),
            '6' => Ok(KeyCode::KEY_6),
            '7' => Ok(KeyCode::KEY_7),
            '8' => Ok(KeyCode::KEY_8),
            '9' => Ok(KeyCode::KEY_9),
            _ => Err(WhisperTalkError::Input(format!("Unsupported character: {}", c))),
        }
    }

    pub fn start(&self, recording_mode: RecordingMode) -> Result<()> {
        let mut is_running = self.is_running.lock();
        if *is_running {
            return Ok(());
        }
        *is_running = true;
        drop(is_running);

        let device = self.device.clone().ok_or(WhisperTalkError::Input("No device".to_string()))?;
        let shortcut = self.shortcut.clone();
        let on_press = self.on_press.clone();
        let on_release = self.on_release.clone();
        let is_running_clone = self.is_running.clone();

        std::thread::spawn(move || {
            let mut modifier_states = Vec::new();
            let key_pressed = Arc::new(Mutex::new(false));
            let press_time = Arc::new(Mutex::new(None::<Instant>));
            let release_count = Arc::new(Mutex::new(0u64));

            while *is_running_clone.lock() {
                match device.lock().fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if let evdev::EventType::KEY = event.event_type() {
                                let key_code = KeyCode::new(event.code());
                                let value = event.value();

                                match value {
                                    1 => {
                                        // Debug: show key presses
                                        if std::env::var("GWHSPR_DEBUG").is_ok() {
                                            eprintln!("Key down: {:?}, modifiers: {:?}", key_code, modifier_states);
                                        }
                                        if shortcut.modifiers.iter().any(|&k| k == key_code) {
                                            modifier_states.push(key_code);
                                        } else if key_code == shortcut.key {
                                            let all_modifiers = shortcut.modifiers.iter()
                                                .all(|m| modifier_states.contains(m));

                                            if all_modifiers {
                                                *key_pressed.lock() = true;
                                                *press_time.lock() = Some(Instant::now());

                                                match recording_mode {
                                                    RecordingMode::Toggle => {
                                                        on_press();
                                                        on_release();
                                                        *key_pressed.lock() = false;
                                                        *release_count.lock() = 0;
                                                        modifier_states.clear();
                                                    }
                                                    RecordingMode::PushToTalk => {
                                                        on_press();
                                                        modifier_states.clear();
                                                        *release_count.lock() = 0;
                                                    }
                                                    RecordingMode::Auto => {
                                                        let elapsed = press_time.lock().as_ref().map(|t| t.elapsed());
                                                        if elapsed.map_or(false, |e| e >= Duration::from_millis(400)) {
                                                            on_press();
                                                            modifier_states.clear();
                                                            *release_count.lock() = 0;
                                                            *press_time.lock() = None;
                                                        } else {
                                                            on_press();
                                                            on_release();
                                                            *key_pressed.lock() = false;
                                                            modifier_states.clear();
                                                            *release_count.lock() = 0;
                                                            *press_time.lock() = None;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    0 => {
                                        if shortcut.modifiers.iter().any(|&k| k == key_code) {
                                            modifier_states.retain(|&k| k != key_code);
                                        } else if key_code == shortcut.key {
                                            on_release();
                                            *key_pressed.lock() = false;
                                            modifier_states.clear();
                                            *release_count.lock() = 0;
                                            *press_time.lock() = None;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Event fetch error: {:?}", e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        *self.is_running.lock() = false;

        if let Some(device) = &self.device {
            if self.grab_mode {
                unsafe {
                    let fd = device.lock().as_raw_fd();
                    libc::ioctl(fd, 0x80044590, 0);
                }
            }
        }

        Ok(())
    }
}
