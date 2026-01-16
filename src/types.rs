use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    Toggle,
    PushToTalk,
    Auto,
}

impl Default for RecordingMode {
    fn default() -> Self {
        Self::Toggle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    CtrlShift,
    Ctrl,
    Super,
}

impl Default for PasteMode {
    fn default() -> Self {
        Self::CtrlShift
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionBackend {
    Whisper,
    ParakeetV3,
}

impl Default for TranscriptionBackend {
    fn default() -> Self {
        Self::Whisper
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct ShortcutConfig {
    #[serde(default = "default_primary_shortcut")]
    pub primary_shortcut: String,

    #[serde(default)]
    pub recording_mode: RecordingMode,

    #[serde(default)]
    pub grab_keys: bool,

    #[serde(default = "default_auto_mode_threshold_ms")]
    pub auto_mode_threshold_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct AudioConfig {
    #[serde(default)]
    pub device_id: Option<u32>,

    #[serde(default)]
    pub device_name: Option<String>,

    #[serde(default)]
    pub device_vendor_id: Option<String>,

    #[serde(default)]
    pub device_model_id: Option<String>,

    #[serde(default = "default_mute_detection")]
    pub mute_detection: bool,

    #[serde(default = "default_zero_volume_threshold")]
    pub zero_volume_threshold: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct TranscriptionConfig {
    #[serde(default)]
    pub backend: TranscriptionBackend,

    #[serde(default = "default_model")]
    pub model: String,

    #[serde(default = "default_threads")]
    pub threads: usize,

    pub language: Option<String>,

    #[serde(default)]
    pub word_overrides: HashMap<String, String>,

    #[serde(default = "default_whisper_prompt")]
    pub whisper_prompt: String,

    #[serde(default = "default_hallucination_markers")]
    pub hallucination_markers: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct InjectionConfig {
    #[serde(default)]
    pub paste_mode: PasteMode,

    #[serde(default)]
    pub auto_submit: bool,

    #[serde(default)]
    pub clipboard_behavior: bool,

    #[serde(default = "default_clipboard_clear_delay")]
    pub clipboard_clear_delay: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct FeedbackConfig {
    #[serde(default = "default_mic_osd_enabled")]
    pub mic_osd_enabled: bool,

    #[serde(default = "default_audio_feedback")]
    pub audio_feedback: bool,

    #[serde(default = "default_master_volume")]
    pub master_volume: f64,

    #[serde(default = "default_start_sound_volume")]
    pub start_sound_volume: f64,

    #[serde(default = "default_stop_sound_volume")]
    pub stop_sound_volume: f64,

    #[serde(default = "default_error_sound_volume")]
    pub error_sound_volume: f64,

    pub start_sound_path: Option<PathBuf>,
    pub stop_sound_path: Option<PathBuf>,
    pub error_sound_path: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub shortcuts: ShortcutConfig,

    #[serde(default)]
    pub audio: AudioConfig,

    #[serde(default)]
    pub transcription: TranscriptionConfig,

    #[serde(default)]
    pub injection: InjectionConfig,

    #[serde(default)]
    pub feedback: FeedbackConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcuts: ShortcutConfig::default(),
            audio: AudioConfig::default(),
            transcription: TranscriptionConfig::default(),
            injection: InjectionConfig::default(),
            feedback: FeedbackConfig::default(),
        }
    }
}

fn default_primary_shortcut() -> String {
    "SUPER+ALT+D".to_string()
}

fn default_auto_mode_threshold_ms() -> u64 {
    400
}

fn default_mute_detection() -> bool {
    true
}

fn default_zero_volume_threshold() -> f32 {
    5e-7
}

fn default_model() -> String {
    "base".to_string()
}

fn default_threads() -> usize {
    4
}

fn default_whisper_prompt() -> String {
    "Transcribe with proper capitalization, including sentence beginnings, proper nouns, titles, and standard English capitalization rules.".to_string()
}

fn default_hallucination_markers() -> Vec<String> {
    vec![
        "(blank audio)".to_string(),
        "[BLANK_AUDIO]".to_string(),
        "[ Silence ]".to_string(),
        "♪".to_string(),
    ]
}

fn default_clipboard_clear_delay() -> f64 {
    5.0
}

fn default_mic_osd_enabled() -> bool {
    true
}

fn default_audio_feedback() -> bool {
    true
}

fn default_master_volume() -> f64 {
    1.0
}

fn default_start_sound_volume() -> f64 {
    1.0
}

fn default_stop_sound_volume() -> f64 {
    1.0
}

fn default_error_sound_volume() -> f64 {
    1.0
}
