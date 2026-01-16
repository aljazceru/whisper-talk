use thiserror::Error;

pub type Result<T> = std::result::Result<T, GwhsprError>;

#[derive(Error, Debug)]
pub enum GwhsprError {
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
    DeviceNotFound(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<anyhow::Error> for GwhsprError {
    fn from(err: anyhow::Error) -> Self {
        GwhsprError::System(err.to_string())
    }
}

impl From<tokio::task::JoinError> for GwhsprError {
    fn from(err: tokio::task::JoinError) -> Self {
        GwhsprError::System(err.to_string())
    }
}

impl From<cpal::BuildStreamError> for GwhsprError {
    fn from(err: cpal::BuildStreamError) -> Self {
        GwhsprError::Audio(err.to_string())
    }
}

impl From<cpal::PlayStreamError> for GwhsprError {
    fn from(err: cpal::PlayStreamError) -> Self {
        GwhsprError::Audio(err.to_string())
    }
}

impl From<zbus::Error> for GwhsprError {
    fn from(err: zbus::Error) -> Self {
        GwhsprError::System(err.to_string())
    }
}
