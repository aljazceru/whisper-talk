use crate::audio::AudioCapture;
use crate::audio::feedback::AudioFeedback;
use crate::config::ConfigManager;
use crate::device::monitor::{DeviceMonitor, extract_device_properties, DeviceProperties};
use crate::device::suspend_monitor::SuspendMonitor;
use crate::audio::pulse_monitor::PulseMonitor;
use crate::error::{GwhsprError, Result};
use crate::injection::TextInjector;
use crate::input::GlobalShortcuts;
use crate::instance_lock::InstanceLock;
use crate::paths::Paths;
use crate::transcription::WhisperBackend;
use crate::types::{Config, RecordingMode};
use crate::visualizer::daemon::MicOsdDaemon;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use anyhow;

#[derive(Clone, Debug, PartialEq)]
enum RecordingState {
    Idle,
    Recording,
    Processing,
}

#[derive(Clone)]
pub struct Application {
    config: Arc<Mutex<Config>>,
    paths: Arc<Paths>,
    audio_capture: Arc<Mutex<Option<AudioCapture>>>,
    whisper_backend: Arc<Mutex<Option<WhisperBackend>>>,
    global_shortcuts: Arc<Mutex<Option<GlobalShortcuts>>>,
    text_injector: Arc<Mutex<Option<TextInjector>>>,
    audio_feedback: Arc<Mutex<Option<AudioFeedback>>>,
    device_monitor: Arc<Mutex<Option<DeviceMonitor>>>,
    suspend_monitor: Arc<Mutex<Option<SuspendMonitor>>>,
    pulse_monitor: Arc<Mutex<Option<PulseMonitor>>>,
    mic_osd: Arc<Mutex<Option<MicOsdDaemon>>>,
    recording_state: Arc<Mutex<RecordingState>>,
    is_running: Arc<AtomicBool>,
    recovery_cooldown: Arc<Mutex<Instant>>,
    last_recovery_time: Arc<AtomicU64>,
    recovery_tx: Arc<Mutex<Option<mpsc::Sender<RecoveryEvent>>>>,
    recovery_result: Arc<Mutex<String>>,
}

#[derive(Clone, Debug)]
pub enum RecoveryEvent {
    AudioFailure,
    HotplugDevice(DeviceProperties),
    PulseAudioEvent,
    SuspendResume,
}

pub struct OwnedApplication {
    inner: Application,
    instance_lock: Option<InstanceLock>,
}

impl Application {
    pub fn new(_config: Config) -> Result<OwnedApplication> {
        let paths = Arc::new(Paths::new()?);
        let config_manager = ConfigManager::new((*paths).clone())?;
        let loaded_config = config_manager.get_config().clone();

        let (recovery_tx, mut recovery_rx) = mpsc::channel::<RecoveryEvent>(32);

        let app = Self {
            config: Arc::new(Mutex::new(loaded_config)),
            paths,
            audio_capture: Arc::new(Mutex::new(None)),
            whisper_backend: Arc::new(Mutex::new(None)),
            global_shortcuts: Arc::new(Mutex::new(None)),
            text_injector: Arc::new(Mutex::new(None)),
            audio_feedback: Arc::new(Mutex::new(None)),
            device_monitor: Arc::new(Mutex::new(None)),
            suspend_monitor: Arc::new(Mutex::new(None)),
            pulse_monitor: Arc::new(Mutex::new(None)),
            mic_osd: Arc::new(Mutex::new(None)),
            recording_state: Arc::new(Mutex::new(RecordingState::Idle)),
            is_running: Arc::new(AtomicBool::new(false)),
            recovery_cooldown: Arc::new(Mutex::new(Instant::now())),
            last_recovery_time: Arc::new(AtomicU64::new(0)),
            recovery_tx: Arc::new(Mutex::new(Some(recovery_tx))),
            recovery_result: Arc::new(Mutex::new(String::new())),
        };

        app.initialize_subsystems()?;
        app.start_recovery_task(recovery_rx)?;

        Ok(OwnedApplication {
            inner: app,
            instance_lock: None,
        })
    }

    fn initialize_subsystems(&self) -> Result<()> {
        let config = self.config.lock().clone();

        let audio = AudioCapture::new(&config.audio)?;
        *self.audio_capture.lock() = Some(audio);

        let whisper = WhisperBackend::new(&config.transcription);
        *self.whisper_backend.lock() = Some(whisper);

        let text_injector = TextInjector::new(&config.injection);
        *self.text_injector.lock() = Some(text_injector);

        let audio_feedback = AudioFeedback::new(&config.feedback, &self.paths)?;
        *self.audio_feedback.lock() = Some(audio_feedback);

        Ok(())
    }

    fn start_recovery_task(&self, mut rx: mpsc::Receiver<RecoveryEvent>) -> Result<()> {
        let app = self.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if app.is_running.load(Ordering::Relaxed) {
                    app.handle_recovery_event(event).await;
                }
            }
        });

        Ok(())
    }

    async fn handle_recovery_event(&self, event: RecoveryEvent) {
        let now = Instant::now();
        let cooldown_duration = Duration::from_secs(2);

        {
            let last_recovery = self.recovery_cooldown.lock();
            if now.duration_since(*last_recovery) < cooldown_duration {
                return;
            }
        }

        let result = match event {
            RecoveryEvent::AudioFailure => {
                eprintln!("Triggering recovery: Audio failure");
                self.recover_audio_capture().await
            }
            RecoveryEvent::HotplugDevice(device_props) => {
                eprintln!("Triggering recovery: Hotplug event {:?}", device_props);
                self.recover_audio_capture().await
            }
            RecoveryEvent::PulseAudioEvent => {
                eprintln!("Triggering recovery: PulseAudio event");
                self.recover_audio_capture().await
            }
            RecoveryEvent::SuspendResume => {
                eprintln!("Triggering recovery: Suspend/resume");
                self.recover_after_suspend().await
            }
        };

        *self.recovery_result.lock() = match result {
            Ok(_) => "success".to_string(),
            Err(e) => format!("failed: {}", e),
        };

        self.write_status_files();
    }

    fn start_recording(&self) {
        let state = self.recording_state.lock();
        if *state != RecordingState::Idle {
            eprintln!("Cannot start recording: not idle");
            return;
        }
        drop(state);

        println!("Starting recording...");

        if let Some(ref mut audio) = self.audio_capture.lock().as_mut() {
            if let Err(e) = audio.start_recording() {
                eprintln!("Failed to start recording: {:?}", e);
                self.handle_error(anyhow::anyhow!("Failed to start recording: {}", e), RecoveryEvent::AudioFailure);
                return;
            }
        }

        if let Some(ref feedback) = self.audio_feedback.lock().as_ref() {
            if let Err(e) = feedback.play_start() {
                eprintln!("Failed to play start sound: {:?}", e);
            }
        }

        *self.recording_state.lock() = RecordingState::Recording;

        self.write_status_files();
    }

    fn stop_recording(&self) {
        let state = self.recording_state.lock();
        if *state != RecordingState::Recording {
            return;
        }
        drop(state);

        println!("Stopping recording...");

        let audio_data = if let Some(ref mut audio) = self.audio_capture.lock().as_mut() {
            audio.stop_recording().unwrap_or_default()
        } else {
            Vec::new()
        };

        if audio_data.is_empty() {
            println!("No audio data captured");
            *self.recording_state.lock() = RecordingState::Idle;
            self.write_status_files();
            return;
        }

        println!("Processing {} audio samples...", audio_data.len());

        let config = self.config.lock();
        let mute_detection = config.audio.mute_detection;
        let zero_volume_threshold = config.audio.zero_volume_threshold;
        drop(config);

        if mute_detection {
            let is_zero_volume = if let Some(ref audio) = self.audio_capture.lock().as_ref() {
                audio.detect_zero_volume(&audio_data)
            } else {
                false
            };

            if is_zero_volume {
                println!("Microphone muted (zero volume), skipping transcription");
                if let Some(ref feedback) = self.audio_feedback.lock().as_ref() {
                    let _ = feedback.play_error();
                }
                *self.recording_state.lock() = RecordingState::Idle;
                self.write_status_files();
                let _ = std::fs::write(self.paths.mic_zero_volume_file.clone(), "1");
                return;
            }
        }

        *self.recording_state.lock() = RecordingState::Processing;
        self.write_status_files();

        let app = self.clone();
        tokio::spawn(async move {
            let result = app.process_audio(&audio_data).await;

            *app.recording_state.lock() = RecordingState::Idle;
            app.write_status_files();

            if let Some(ref feedback) = app.audio_feedback.lock().as_ref() {
                let _ = feedback.play_stop();
            }

            if let Err(e) = result {
                eprintln!("Processing error: {:?}", e);
                app.handle_error(anyhow::anyhow!("{}", e), RecoveryEvent::AudioFailure);
            }
        });
    }

    async fn process_audio(&self, audio_data: &[f32]) -> Result<String> {
        let backend = self.whisper_backend.clone();
        let audio_vec = audio_data.to_vec();

        let text = tokio::task::spawn_blocking(move || {
            let mut wb = backend.lock();
            let b = wb.as_mut().ok_or_else(|| anyhow::anyhow!("Whisper backend not loaded"))?;
            b.transcribe(&audio_vec).map_err(|e| anyhow::anyhow!("Transcription error: {}", e))
        }).await.map_err(|e| GwhsprError::Transcription(format!("Join error: {}", e)))??;

        if text.is_empty() {
            println!("No transcribed text");
            return Ok(text);
        }

        println!("Transcribed: \"{}\"", text);

        self.inject_text(&text).await?;

        Ok(text)
    }

    async fn inject_text(&self, text: &str) -> Result<()> {
        // Use block scope to ensure MutexGuard is dropped before await
        let (word_overrides, injector) = {
            let config = self.config.lock();
            (config.transcription.word_overrides.clone(), self.text_injector.clone())
        };

        let text = text.to_string();
        let overrides = word_overrides;

        let result = tokio::task::spawn_blocking(move || {
            let mut inj = injector.lock();
            let i = inj.as_mut().ok_or_else(|| anyhow::anyhow!("Text injector not loaded"))?;
            i.set_word_overrides(overrides);
            i.inject_text(&text).map_err(|e| anyhow::anyhow!("Injection error: {}", e))
        }).await.map_err(|e| GwhsprError::Injection(format!("Join error: {}", e)))??;

        Ok(result)
    }

    async fn recover_audio_capture(&self) -> Result<()> {
        let now = Instant::now();
        {
            let last_recovery = self.recovery_cooldown.lock();
            if now.duration_since(*last_recovery) < Duration::from_secs(2) {
                eprintln!("Recovery in cooldown, skipping");
                return Ok(());
            }
        }

        let audio = self.audio_capture.clone();
        // Use block_in_place instead of spawn_blocking because AudioCapture's stream is not Send
        let result = tokio::task::block_in_place(|| -> std::result::Result<(), String> {
            let mut a = audio.lock();
            let a = a.as_mut().ok_or_else(|| "Audio capture not loaded".to_string())?;
            a.recover_stream().map_err(|e| format!("Recovery failed: {}", e))
        });

        match result {
            Ok(()) => {
                eprintln!("Audio capture recovered successfully");
                *self.recovery_cooldown.lock() = now;
                self.last_recovery_time.store(now.elapsed().as_secs(), Ordering::Relaxed);
                *self.recovery_result.lock() = "success".to_string();
                self.write_status_files();
                Ok(())
            }
            Err(e) => {
                eprintln!("Audio capture recovery failed: {}", e);
                *self.recovery_result.lock() = format!("failed: {}", e);
                self.write_status_files();
                Err(GwhsprError::System(format!("Recovery failed: {}", e)))
            }
        }
    }

    async fn recover_after_suspend(&self) -> Result<()> {
        eprintln!("Starting background recovery after suspend");

        let audio = self.audio_capture.clone();
        let app = self.clone();

        tokio::spawn(async move {
            for attempt in 0..6 {
                if !app.is_running.load(Ordering::Relaxed) {
                    return;
                }

                sleep(Duration::from_secs(2)).await;

                eprintln!("Background recovery attempt {}", attempt + 1);

                // Use block_in_place instead of spawn_blocking because AudioCapture's stream is not Send
                let result = tokio::task::block_in_place(|| -> std::result::Result<(), String> {
                    let mut a = audio.lock();
                    let a = a.as_mut().ok_or_else(|| "Audio capture not loaded".to_string())?;
                    a.recover_stream().map_err(|e| format!("Recovery failed: {}", e))
                });

                match result {
                    Ok(()) => {
                        eprintln!("Background recovery succeeded on attempt {}", attempt + 1);
                        *app.recovery_result.lock() = "success".to_string();
                        app.write_status_files();
                        return;
                    }
                    Err(e) => {
                        eprintln!("Background recovery attempt {} failed: {}", attempt + 1, e);
                    }
                }
            }

            eprintln!("Background recovery failed after 6 attempts");
            *app.recovery_result.lock() = "failed: max retries".to_string();
            app.write_status_files();
        });

        Ok(())
    }

    fn handle_error(&self, error: anyhow::Error, recovery_event: RecoveryEvent) {
        eprintln!("Error: {}", error);

        if let Some(ref feedback) = self.audio_feedback.lock().as_ref() {
            let _ = feedback.play_error();
        }

        let tx = self.recovery_tx.lock().clone();
        if let Some(tx) = tx {
            tokio::spawn(async move {
                let _ = tx.send(recovery_event).await;
            });
        }
    }

    fn write_status_files(&self) {
        let state = self.recording_state.lock();
        let recording_status = match *state {
            RecordingState::Idle => "idle".to_string(),
            RecordingState::Recording => "recording".to_string(),
            RecordingState::Processing => "processing".to_string(),
        };
        drop(state);

        let _ = std::fs::write(&self.paths.recording_status_file, &recording_status);

        let audio_level = if let Some(ref audio) = self.audio_capture.lock().as_ref() {
            audio.get_audio_level()
        } else {
            0.0
        };
        let _ = std::fs::write(&self.paths.audio_level_file, format!("{}", audio_level));

        let recovery_result = self.recovery_result.lock().clone();
        if !recovery_result.is_empty() {
            let _ = std::fs::write(&self.paths.recovery_result_file, &recovery_result);
        }
    }

    fn clone_for_callback(&self) -> Self {
        Self {
            config: self.config.clone(),
            paths: self.paths.clone(),
            audio_capture: self.audio_capture.clone(),
            whisper_backend: self.whisper_backend.clone(),
            global_shortcuts: self.global_shortcuts.clone(),
            text_injector: self.text_injector.clone(),
            audio_feedback: self.audio_feedback.clone(),
            device_monitor: self.device_monitor.clone(),
            suspend_monitor: self.suspend_monitor.clone(),
            pulse_monitor: self.pulse_monitor.clone(),
            mic_osd: self.mic_osd.clone(),
            recording_state: self.recording_state.clone(),
            is_running: self.is_running.clone(),
            recovery_cooldown: self.recovery_cooldown.clone(),
            last_recovery_time: self.last_recovery_time.clone(),
            recovery_tx: self.recovery_tx.clone(),
            recovery_result: self.recovery_result.clone(),
        }
    }

    pub fn get_audio_level(&self) -> f32 {
        self.audio_capture.lock()
            .as_ref()
            .map(|a| a.get_audio_level())
            .unwrap_or(0.0)
    }

    pub fn update_config(&self, config: Config) -> Result<()> {
        let mut current = self.config.lock();
        *current = config;
        Ok(())
    }
}

impl OwnedApplication {
    pub fn acquire_instance_lock(&mut self) -> Result<()> {
        let lock = InstanceLock::acquire(&self.inner.paths.lock_file)?;
        self.instance_lock = Some(lock);
        Ok(())
    }

    pub fn update_config(&self, config: Config) -> Result<()> {
        self.inner.update_config(config)
    }

    pub async fn start(&mut self) -> Result<()> {
        self.inner.is_running.store(true, Ordering::Relaxed);

        let config = self.inner.config.lock().clone();
        let shortcut_str = config.shortcuts.primary_shortcut.clone();
        let recording_mode = config.shortcuts.recording_mode.clone();

        let app_press = self.inner.clone();
        let app_release = self.inner.clone();

        let on_press = {
            Arc::new(move || app_press.on_shortcut_press())
        };

        let on_release = {
            Arc::new(move || app_release.on_shortcut_release())
        };

        let device_path = None;
        let grab_mode = config.shortcuts.grab_keys;

        let shortcuts = GlobalShortcuts::new(
            &shortcut_str,
            move || on_press(),
            move || on_release(),
            device_path,
            grab_mode,
        )?;

        let shortcuts_clone = shortcuts.clone();
        *self.inner.global_shortcuts.lock() = Some(shortcuts);

        println!("Starting Whisper backend...");
        let mut backend = self.inner.whisper_backend.lock();
        if let Some(ref mut wb) = backend.as_mut() {
            if !wb.initialize(&self.inner.paths)? {
                println!("WARNING: Whisper backend not initialized, model may not exist");
            }
        }
        drop(backend);

        shortcuts_clone.start(recording_mode)?;

        self.inner.start_monitors()?;

        println!("gwhspr-rs is running. Press {} to toggle recording", shortcut_str);

        self.inner.write_status_files();

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.inner.is_running.store(false, Ordering::Relaxed);

        if let Some(ref mut shortcuts) = self.inner.global_shortcuts.lock().as_mut() {
            shortcuts.stop()?;
        }

        self.inner.stop_monitors();

        if let Some(ref mut audio) = self.inner.audio_capture.lock().as_mut() {
            if audio.is_recording() {
                audio.stop_recording()?;
            }
        }

        println!("gwhspr-rs stopped");

        Ok(())
    }

    pub async fn run_event_loop(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        while self.inner.is_running.load(Ordering::Relaxed) {
            interval.tick().await;
            self.inner.write_status_files();
        }

        Ok(())
    }
}

impl Application {
    fn start_monitors(&self) -> Result<()> {
        use tracing::{info, warn, debug};

        let config = self.config.lock().clone();
        let device_name = config.audio.device_name.clone();
        let has_specific_device = device_name.is_some();
        drop(config);

        // Device hotplug monitor
        let app = self.clone();
        let device_name_clone = device_name.clone();
        let on_add = move |device: &udev::Device| {
            let props = extract_device_properties(device);
            if let Some(ref name) = device_name_clone {
                if props.matches(name) {
                    info!("Audio device added: {:?}", props);
                    if let Some(ref feedback) = app.audio_feedback.lock().as_ref() {
                        let _ = feedback.play_start();
                    }
                    let tx = app.recovery_tx.lock().clone();
                    if let Some(tx) = tx {
                        tokio::spawn(async move {
                            let _ = tx.send(RecoveryEvent::HotplugDevice(props)).await;
                        });
                    }
                }
            }
        };

        let app = self.clone();
        let on_remove = move |device: &udev::Device| {
            let props = extract_device_properties(device);
            info!("Audio device removed: {:?}", props);
            if let Some(ref feedback) = app.audio_feedback.lock().as_ref() {
                let _ = feedback.play_error();
            }
        };

        let mut device_monitor = DeviceMonitor::new(on_add, on_remove)?;
        device_monitor.start()?;
        *self.device_monitor.lock() = Some(device_monitor);
        info!("Device hotplug monitor started");

        // Suspend/resume monitor
        let app = self.clone();
        let on_resume = move || {
            info!("System resumed, triggering recovery");
            let tx = app.recovery_tx.lock().clone();
            if let Some(tx) = tx {
                tokio::spawn(async move {
                    let _ = tx.send(RecoveryEvent::SuspendResume).await;
                });
            }
        };
        let on_suspend = move || {
            info!("System suspending");
        };

        let mut suspend_monitor = SuspendMonitor::new(on_suspend, on_resume)?;
        suspend_monitor.start()?;
        *self.suspend_monitor.lock() = Some(suspend_monitor);
        info!("Suspend/resume monitor started");

        // PulseAudio/PipeWire monitor (only if no specific device is configured)
        if !has_specific_device {
            let app = self.clone();
            let mut pulse_monitor = PulseMonitor::new()?;

            // Set callback for default source changes
            pulse_monitor.set_on_default_change({
                let app = app.clone();
                move |new_source| {
                    info!("PulseAudio default source changed to: {}", new_source);
                    let tx = app.recovery_tx.lock().clone();
                    if let Some(tx) = tx {
                        tokio::spawn(async move {
                            let _ = tx.send(RecoveryEvent::PulseAudioEvent).await;
                        });
                    }
                }
            });

            // Set callback for server restarts
            pulse_monitor.set_on_server_restart({
                let app = app.clone();
                move || {
                    info!("PulseAudio server restarted, triggering recovery");
                    let tx = app.recovery_tx.lock().clone();
                    if let Some(tx) = tx {
                        tokio::spawn(async move {
                            let _ = tx.send(RecoveryEvent::PulseAudioEvent).await;
                        });
                    }
                }
            });

            match pulse_monitor.start() {
                Ok(true) => {
                    *self.pulse_monitor.lock() = Some(pulse_monitor);
                    info!("PulseAudio/PipeWire monitor started");
                }
                Ok(false) => {
                    warn!("PulseAudio monitoring not available (pactl not found)");
                }
                Err(e) => {
                    warn!("Failed to start PulseAudio monitor: {}", e);
                }
            }
        } else {
            debug!("Specific audio device configured, skipping PulseAudio monitoring");
        }

        // Mic OSD (optional)
        if let Ok(osd) = MicOsdDaemon::new() {
            *self.mic_osd.lock() = Some(osd);
            debug!("Mic OSD daemon initialized");
        }

        Ok(())
    }

    fn stop_monitors(&self) {
        use tracing::info;

        if let Some(ref mut monitor) = self.device_monitor.lock().take() {
            monitor.stop();
            info!("Device monitor stopped");
        }
        if let Some(ref mut monitor) = self.suspend_monitor.lock().take() {
            monitor.stop();
            info!("Suspend monitor stopped");
        }
        if let Some(ref mut monitor) = self.pulse_monitor.lock().take() {
            monitor.stop();
            info!("PulseAudio monitor stopped");
        }
    }

    fn on_shortcut_press(&self) {
        let is_recording = self.audio_capture.lock()
            .as_ref()
            .map(|a| a.is_recording())
            .unwrap_or(false);

        if is_recording {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn on_shortcut_release(&self) {
        let config = self.config.lock();
        if config.shortcuts.recording_mode == RecordingMode::PushToTalk {
            self.stop_recording();
        }
    }
}

impl Drop for OwnedApplication {
    fn drop(&mut self) {
        self.inner.is_running.store(false, Ordering::Relaxed);
        self.inner.stop_monitors();
    }
}
