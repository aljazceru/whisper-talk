pub mod parakeet;
pub mod whisper;

pub use parakeet::ParakeetBackend;
pub use whisper::{TranscriptSegment, WhisperBackend};

use crate::error::Result;
use crate::paths::Paths;
use crate::types::TranscriptionConfig;

pub enum Backend {
    Whisper(WhisperBackend),
    Parakeet(Box<ParakeetBackend>),
}

impl Backend {
    pub fn new(config: &TranscriptionConfig) -> Self {
        match config.backend {
            crate::types::TranscriptionBackend::Whisper => {
                Self::Whisper(WhisperBackend::new(config))
            }
            crate::types::TranscriptionBackend::ParakeetV3 => {
                Self::Parakeet(Box::new(ParakeetBackend::new(config)))
            }
        }
    }

    pub fn initialize(&mut self, paths: &Paths) -> Result<bool> {
        match self {
            Self::Whisper(b) => b.initialize(paths),
            Self::Parakeet(b) => b.initialize(paths),
        }
    }

    pub fn transcribe_with_options(
        &self,
        audio_data: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
        translate: bool,
    ) -> Result<String> {
        match self {
            Self::Whisper(b) => b.transcribe_with_options(audio_data, language, prompt, translate),
            Self::Parakeet(b) => b.transcribe_with_options(audio_data, language, prompt, translate),
        }
    }

    pub fn transcribe_with_segments(
        &self,
        audio_data: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
        translate: bool,
    ) -> Result<Vec<TranscriptSegment>> {
        match self {
            Self::Whisper(b) => b.transcribe_with_segments(audio_data, language, prompt, translate),
            Self::Parakeet(b) => {
                b.transcribe_with_segments(audio_data, language, prompt, translate)
            }
        }
    }
}
