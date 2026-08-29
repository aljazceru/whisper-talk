use thiserror::Error;

pub type Result<T> = std::result::Result<T, WhisperTalkError>;

#[derive(Error, Debug)]
pub enum WhisperTalkError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Transcription error: {0}")]
    Transcription(String),

    #[error("Input error: {0}")]
    Input(String),

    #[error("Injection error: {0}")]
    Injection(String),

    #[error("System error: {0}")]
    System(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Another instance is already running")]
    AlreadyRunning,

    #[error("Device not found: {0}")]
    #[allow(dead_code)]
    DeviceNotFound(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<anyhow::Error> for WhisperTalkError {
    fn from(err: anyhow::Error) -> Self {
        WhisperTalkError::System(err.to_string())
    }
}

impl From<tokio::task::JoinError> for WhisperTalkError {
    fn from(err: tokio::task::JoinError) -> Self {
        WhisperTalkError::System(err.to_string())
    }
}

impl From<cpal::Error> for WhisperTalkError {
    fn from(err: cpal::Error) -> Self {
        WhisperTalkError::Audio(err.to_string())
    }
}

impl From<zbus::Error> for WhisperTalkError {
    fn from(err: zbus::Error) -> Self {
        WhisperTalkError::System(err.to_string())
    }
}
