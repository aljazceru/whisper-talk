use crate::error::{Result, WhisperTalkError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Wrapper to make non-Send stream types usable in Send contexts.
/// This is safe because the stream is always accessed through a mutex.
struct SendableStream(Option<Box<dyn StreamTrait>>);

// SAFETY: We ensure single-threaded access through the Mutex wrapper.
// The stream is never actually sent across threads, it's just stored
// in a struct that needs to be Send.
unsafe impl Send for SendableStream {}
unsafe impl Sync for SendableStream {}

pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    audio_level: Arc<AtomicU32>,
    device_id: Arc<Mutex<Option<u32>>>,
    device_name: Arc<Mutex<Option<String>>>,
    device_vendor_id: Arc<Mutex<Option<String>>>,
    device_model_id: Arc<Mutex<Option<String>>>,
    #[allow(dead_code)]
    sample_rate: u32,
    target_sample_rate: u32,
    target_channels: u16,
    zero_volume_threshold: f32,
    stream: Arc<Mutex<SendableStream>>,
    recovery_in_progress: Arc<AtomicBool>,
    abort_recovery: Arc<AtomicBool>,
    is_recovering: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new(config: &crate::types::AudioConfig) -> Result<Self> {
        let _host = cpal::default_host();

        Ok(Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            audio_level: Arc::new(AtomicU32::new(0)),
            device_id: Arc::new(Mutex::new(config.device_id)),
            device_name: Arc::new(Mutex::new(config.device_name.clone())),
            device_vendor_id: Arc::new(Mutex::new(config.device_vendor_id.clone())),
            device_model_id: Arc::new(Mutex::new(config.device_model_id.clone())),
            sample_rate: 16000,
            target_sample_rate: 16000,
            target_channels: 1,
            zero_volume_threshold: config.zero_volume_threshold,
            stream: Arc::new(Mutex::new(SendableStream(None))),
            recovery_in_progress: Arc::new(AtomicBool::new(false)),
            abort_recovery: Arc::new(AtomicBool::new(false)),
            is_recovering: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn enumerate_devices(&self) -> Result<Vec<DeviceInfo>> {
        let host = cpal::default_host();

        let mut devices = Vec::new();
        let devices_iter = host
            .input_devices()
            .map_err(|e| WhisperTalkError::Audio(format!("Failed to enumerate devices: {}", e)))?;

        for (index, device) in devices_iter.enumerate() {
            if let Ok(name) = device.description() {
                if let Ok(default_config) = device.default_input_config() {
                    let default_sr: u32 = default_config.sample_rate();
                    devices.push(DeviceInfo {
                        name: name.to_string(),
                        id: Some(index as u32),
                        default_sample_rate: default_sr,
                        max_channels: default_config.channels(),
                    });
                }
            }
        }
        Ok(devices)
    }

    pub fn get_audio_level(&self) -> f32 {
        let level = self.audio_level.load(Ordering::Relaxed);
        f32::from_bits(level)
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::Relaxed)
    }

    pub fn start_recording(&mut self) -> Result<()> {
        if self.is_recording.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.is_recording.store(true, Ordering::Relaxed);

        *self.audio_buffer.lock() = Vec::new();
        self.audio_level.store(0u32, Ordering::Relaxed);

        self.initialize_stream()?;

        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<Vec<f32>> {
        self.is_recording.store(false, Ordering::Relaxed);

        self.terminate_stream();

        let mut audio_data = std::mem::take(&mut *self.audio_buffer.lock());

        // Resample if necessary
        if self.sample_rate != self.target_sample_rate {
            println!(
                "Resampling from {}Hz to {}Hz",
                self.sample_rate, self.target_sample_rate
            );
            audio_data = self.resample_audio(&audio_data)?;
        }

        Ok(audio_data)
    }

    fn resample_audio(&self, input: &[f32]) -> Result<Vec<f32>> {
        crate::audio::resample::resample_to_16khz(input, self.sample_rate, 1)
            .map_err(|e| WhisperTalkError::Audio(format!("Failed to resample audio: {}", e)))
    }

    fn find_device(&self) -> Result<cpal::Device> {
        let host = cpal::default_host();
        let devices: Vec<_> = host
            .input_devices()
            .map_err(|e| WhisperTalkError::Audio(format!("Failed to enumerate devices: {}", e)))?
            .collect();

        let vendor_id = self.device_vendor_id.lock().clone();
        let model_id = self.device_model_id.lock().clone();
        let name = self.device_name.lock().clone();
        let id = *self.device_id.lock();

        if let (Some(vid), Some(mid)) = (&vendor_id, &model_id) {
            for device in &devices {
                if let Ok(device_name) = device.description() {
                    let device_str = device_name.name();
                    if device_str.contains(vid) && device_str.contains(mid) {
                        return Ok(device.clone());
                    }
                }
            }
        }

        if let Some(device_name) = &name {
            for device in &devices {
                if let Ok(dname) = device.description() {
                    if dname.name() == *device_name {
                        return Ok(device.clone());
                    }
                }
            }
        }

        if let Some(id) = id {
            if let Some(device) = devices.get(id as usize) {
                return Ok(device.clone());
            }
        }

        host.default_input_device()
            .ok_or_else(|| WhisperTalkError::Audio("No default input device found".to_string()))
    }

    fn initialize_stream(&mut self) -> Result<()> {
        let device = self.find_device()?;

        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        println!("Using audio device: {}", device_name);

        let mut supported_configs_range = device.supported_input_configs().map_err(|e| {
            WhisperTalkError::Audio(format!("Failed to get supported configs: {}", e))
        })?;

        let supported_config = supported_configs_range.next().ok_or_else(|| {
            WhisperTalkError::Audio("No supported audio config found".to_string())
        })?;

        let config: StreamConfig = supported_config.with_max_sample_rate().into();
        let input_sample_rate: u32 = config.sample_rate;
        let input_channels = config.channels;
        let sample_format = supported_config.sample_format();

        self.sample_rate = input_sample_rate; // Store actual sample rate

        let stream = match sample_format {
            SampleFormat::F32 => self.build_stream::<f32>(&device, &config, input_channels)?,
            SampleFormat::I16 => self.build_stream::<i16>(&device, &config, input_channels)?,
            SampleFormat::U16 => self.build_stream::<u16>(&device, &config, input_channels)?,
            SampleFormat::I8 => self.build_stream::<i8>(&device, &config, input_channels)?,
            SampleFormat::U8 => self.build_stream::<u8>(&device, &config, input_channels)?,
            SampleFormat::I32 => self.build_stream::<i32>(&device, &config, input_channels)?,
            SampleFormat::I64 => self.build_stream::<i64>(&device, &config, input_channels)?,
            SampleFormat::U32 => self.build_stream::<u32>(&device, &config, input_channels)?,
            SampleFormat::U64 => self.build_stream::<u64>(&device, &config, input_channels)?,
            SampleFormat::F64 => self.build_stream::<f64>(&device, &config, input_channels)?,
            _ => {
                return Err(WhisperTalkError::Audio(format!(
                    "Unsupported sample format: {:?}",
                    sample_format
                )))
            }
        };

        *self.stream.lock() = SendableStream(Some(stream));
        Ok(())
    }

    fn build_stream<T>(
        &mut self,
        device: &cpal::Device,
        config: &StreamConfig,
        input_channels: u16,
    ) -> Result<Box<dyn StreamTrait>>
    where
        T: cpal::Sample + cpal::SizedSample + Send + Sync + 'static,
        f32: cpal::FromSample<T>,
    {
        let config_clone = *config;
        let is_recording_clone = self.is_recording.clone();
        let audio_buffer_clone = self.audio_buffer.clone();
        let audio_level_clone = self.audio_level.clone();
        let _target_sample_rate = self.target_sample_rate;
        let target_channels = self.target_channels;
        let input_channels = input_channels as usize;
        let target_channels = target_channels as usize;
        let _zero_volume_threshold = self.zero_volume_threshold;

        let stream = device.build_input_stream(
            config_clone,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !is_recording_clone.load(Ordering::Relaxed) {
                    return;
                }

                let mut samples: Vec<f32> = data.iter().map(|&s| f32::from_sample(s)).collect();

                let rms = calculate_rms(&samples);
                audio_level_clone.store(rms.to_bits(), Ordering::Relaxed);

                if input_channels > 1 && target_channels == 1 {
                    samples = stereo_to_mono(&samples, input_channels);
                }

                // Removed realtime resampling
                // if let Some(resampler) = resampler_clone.lock().as_mut() { ... }

                audio_buffer_clone.lock().extend_from_slice(&samples);
            },
            move |err| {
                eprintln!("Audio stream error: {:?}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Box::new(stream))
    }

    fn terminate_stream(&self) {
        if let Some(stream) = self.stream.lock().0.take() {
            let _ = stream.pause();
        }
    }

    pub fn recover_stream(&mut self) -> Result<()> {
        // Try to atomically set recovery_in_progress from false to true
        // If it's already true, recovery is in progress
        if self
            .recovery_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(WhisperTalkError::Audio(
                "Recovery already in progress".to_string(),
            ));
        }

        self.is_recovering.store(true, Ordering::Relaxed);

        let result = self.do_recover_stream();

        self.is_recovering.store(false, Ordering::Relaxed);
        self.abort_recovery.store(false, Ordering::Relaxed);
        self.recovery_in_progress.store(false, Ordering::SeqCst);

        result
    }

    fn do_recover_stream(&mut self) -> Result<()> {
        let was_recording = self.is_recording.load(Ordering::Relaxed);
        if !was_recording {
            return Ok(());
        }

        self.terminate_stream();

        let timeouts = [10, 50, 100, 500, 1000, 2000];

        for (attempt, &timeout) in timeouts.iter().enumerate() {
            if self.abort_recovery.load(Ordering::Relaxed) {
                return Err(WhisperTalkError::Audio("Recovery aborted".to_string()));
            }

            eprintln!(
                "Recovery attempt {} of {} (timeout: {}ms)",
                attempt + 1,
                timeouts.len(),
                timeout
            );

            std::thread::sleep(Duration::from_millis(timeout));

            if let Ok(()) = self.initialize_stream() {
                eprintln!("Stream recovered successfully on attempt {}", attempt + 1);
                return Ok(());
            }
        }

        Err(WhisperTalkError::Audio(
            "Failed to recover audio stream".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub async fn background_recover_stream(&mut self) -> Result<()> {
        let was_recording = self.is_recording.load(Ordering::Relaxed);
        if !was_recording {
            return Ok(());
        }

        let recovery_interval_ms = 2000;
        let max_retries = 6;

        for attempt in 0..max_retries {
            if self.abort_recovery.load(Ordering::Relaxed) {
                return Err(WhisperTalkError::Audio("Recovery aborted".to_string()));
            }

            sleep(Duration::from_millis(recovery_interval_ms)).await;

            if let Ok(()) = self.recover_stream() {
                eprintln!("Background recovery succeeded on attempt {}", attempt + 1);
                return Ok(());
            }

            eprintln!("Background recovery attempt {} failed", attempt + 1);
        }

        Err(WhisperTalkError::Audio(format!(
            "Background recovery failed after {} attempts",
            max_retries
        )))
    }

    #[allow(dead_code)]
    pub fn abort_recovery(&self) {
        self.abort_recovery.store(true, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn is_recovering(&self) -> bool {
        self.is_recovering.load(Ordering::Relaxed)
    }

    pub fn detect_zero_volume(&self, audio: &[f32]) -> bool {
        audio
            .iter()
            .all(|&sample| sample.abs() < self.zero_volume_threshold)
    }
}

fn calculate_rms(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return 0.0;
    }
    let sum: f32 = audio.iter().map(|&x| x * x).sum();
    (sum / audio.len() as f32).sqrt()
}

fn stereo_to_mono(stereo: &[f32], channels: usize) -> Vec<f32> {
    let mut mono = Vec::with_capacity(stereo.len() / channels);
    for chunk in stereo.chunks(channels) {
        // Use the first channel (usually left/mono) instead of averaging
        // This matches the Python implementation and avoids mixing in noise from empty channels
        if let Some(&sample) = chunk.first() {
            mono.push(sample);
        }
    }
    mono
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub id: Option<u32>,
    pub default_sample_rate: u32,
    #[allow(dead_code)]
    pub max_channels: u16,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.is_recording.store(false, Ordering::Relaxed);
        self.abort_recovery.store(true, Ordering::Relaxed);
        self.terminate_stream();
    }
}
