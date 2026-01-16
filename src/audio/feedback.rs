use crate::error::{GwhsprError, Result};
use crate::paths::Paths;
use crate::types::FeedbackConfig;
use parking_lot::Mutex;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AudioFeedback {
    config: Arc<Mutex<FeedbackConfig>>,
    paths: Arc<Paths>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
}

// SAFETY: AudioFeedback is always accessed through a Mutex in Application,
// so we guarantee single-threaded access. The non-Send OutputStream is never
// actually sent across threads, it just lives in a struct that needs to be Send.
unsafe impl Send for AudioFeedback {}
unsafe impl Sync for AudioFeedback {}

impl AudioFeedback {
    pub fn new(feedback_config: &FeedbackConfig, paths: &Paths) -> Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| GwhsprError::Audio(format!("Failed to create output stream: {}", e)))?;

        Ok(Self {
            config: Arc::new(Mutex::new(feedback_config.clone())),
            paths: Arc::new(paths.clone()),
            _stream: stream,
            stream_handle,
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
            .map_err(|e| GwhsprError::Audio(format!("Failed to open sound file: {}", e)))?;

        let source = Decoder::new_vorbis(BufReader::new(file))
            .map_err(|e| GwhsprError::Audio(format!("Failed to decode audio: {}", e)))?;

        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| GwhsprError::Audio(format!("Failed to create sink: {}", e)))?;

        let config = self.config.lock();
        let master_volume = config.master_volume;
        let sound_volume = volume_getter(&config);
        drop(config);

        let final_volume = (master_volume * sound_volume).clamp(0.0, 1.0);
        sink.set_volume(final_volume as f32);

        sink.append(source);

        std::thread::spawn(move || {
            sink.sleep_until_end();
        });

        Ok(())
    }
}
