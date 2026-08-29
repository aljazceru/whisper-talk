use crate::error::{Result, WhisperTalkError};
use crate::paths::Paths;
use crate::types::FeedbackConfig;
use parking_lot::Mutex;
use rodio::mixer::Mixer;
use rodio::{Decoder, DeviceSinkBuilder, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AudioFeedback {
    config: Arc<Mutex<FeedbackConfig>>,
    paths: Arc<Paths>,
    mixer: Mixer,
    // Kept alive for as long as AudioFeedback exists; dropping it stops all
    // playback through `mixer`.
    _sink: std::sync::Mutex<rodio::MixerDeviceSink>,
}

impl AudioFeedback {
    pub fn new(feedback_config: &FeedbackConfig, paths: &Paths) -> Result<Self> {
        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| {
            WhisperTalkError::Audio(format!("Failed to create output stream: {}", e))
        })?;
        sink.log_on_drop(false);
        let mixer = sink.mixer().clone();

        Ok(Self {
            config: Arc::new(Mutex::new(feedback_config.clone())),
            paths: Arc::new(paths.clone()),
            mixer,
            _sink: std::sync::Mutex::new(sink),
        })
    }

    pub fn play_start(&self) -> Result<()> {
        let config = self.config.lock();
        if !config.audio_feedback {
            return Ok(());
        }
        drop(config);

        let sound_path = self.get_start_sound_path()?;
        self.play_sound(&sound_path, |config| config.start_sound_volume)
    }

    pub fn play_stop(&self) -> Result<()> {
        let config = self.config.lock();
        if !config.audio_feedback {
            return Ok(());
        }
        drop(config);

        let sound_path = self.get_stop_sound_path()?;
        self.play_sound(&sound_path, |config| config.stop_sound_volume)
    }

    pub fn play_error(&self) -> Result<()> {
        let config = self.config.lock();
        if !config.audio_feedback {
            return Ok(());
        }
        drop(config);

        let sound_path = self.get_error_sound_path()?;
        self.play_sound(&sound_path, |config| config.error_sound_volume)
    }

    #[allow(dead_code)]
    pub fn set_volume(&self, volume: f64) -> Result<()> {
        let mut config = self.config.lock();
        config.master_volume = volume.clamp(0.0, 1.0);
        Ok(())
    }

    fn get_start_sound_path(&self) -> Result<PathBuf> {
        let config = self.config.lock();
        if let Some(ref path) = config.start_sound_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        drop(config);
        Ok(self.paths.assets_dir.join("ping-up.ogg"))
    }

    fn get_stop_sound_path(&self) -> Result<PathBuf> {
        let config = self.config.lock();
        if let Some(ref path) = config.stop_sound_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        drop(config);
        Ok(self.paths.assets_dir.join("ping-down.ogg"))
    }

    fn get_error_sound_path(&self) -> Result<PathBuf> {
        let config = self.config.lock();
        if let Some(ref path) = config.error_sound_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        drop(config);
        Ok(self.paths.assets_dir.join("ping-error.ogg"))
    }

    fn play_sound<F>(&self, path: &PathBuf, volume_getter: F) -> Result<()>
    where
        F: FnOnce(&FeedbackConfig) -> f64,
    {
        if !path.exists() {
            tracing::warn!("Sound file not found: {:?}", path);
            return Ok(());
        }

        let file = File::open(path)
            .map_err(|e| WhisperTalkError::Audio(format!("Failed to open sound file: {}", e)))?;

        let source = Decoder::new_vorbis(BufReader::new(file))
            .map_err(|e| WhisperTalkError::Audio(format!("Failed to decode audio: {}", e)))?;

        let config = self.config.lock();
        let master_volume = config.master_volume;
        let sound_volume = volume_getter(&config);
        drop(config);

        let final_volume = (master_volume * sound_volume).clamp(0.0, 1.0);

        // The mixer plays the source to completion on its own thread; no
        // per-sound sink or sleep_until_end bookkeeping is needed.
        self.mixer.add(source.amplify(final_volume as f32));

        Ok(())
    }
}
