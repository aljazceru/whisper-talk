use crate::audio::feedback::AudioFeedback;
use crate::audio::pulse_monitor::PulseMonitor;
use crate::audio::AudioCapture;
use crate::config::ConfigManager;
use crate::control::Control;
use crate::device::monitor::{extract_device_properties, DeviceMonitor, DeviceProperties};
use crate::device::suspend_monitor::SuspendMonitor;
use crate::error::{Result, WhisperTalkError};
use crate::injection::TextInjector;
use crate::input::GlobalShortcuts;
use crate::paths::Paths;
use crate::transcription::{Backend, TranscriptSegment};
use crate::types::{Config, RecordingMode};
use crate::visualizer::daemon::MicOsdDaemon;
use anyhow;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::sleep;

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
    control: Arc<Mutex<Option<Control>>>,
    applying: Arc<AtomicBool>,
    audio_capture: Arc<Mutex<Option<AudioCapture>>>,
    transcription_backend: Arc<Mutex<Option<Backend>>>,
    global_shortcuts: Arc<Mutex<Option<GlobalShortcuts>>>,
    text_injector: Arc<Mutex<Option<TextInjector>>>,
    audio_feedback: Arc<Mutex<Option<AudioFeedback>>>,
    device_monitor: Arc<Mutex<Option<DeviceMonitor>>>,
    keyboard_monitor: Arc<Mutex<Option<DeviceMonitor>>>,
    suspend_monitor: Arc<Mutex<Option<SuspendMonitor>>>,
    pulse_monitor: Arc<Mutex<Option<PulseMonitor>>>,
    mic_osd: Arc<Mutex<Option<MicOsdDaemon>>>,
    recording_state: Arc<Mutex<RecordingState>>,
    is_running: Arc<AtomicBool>,
    recovery_cooldown: Arc<Mutex<Instant>>,
    last_recovery_time: Arc<AtomicU64>,
    recovery_tx: Arc<Mutex<Option<mpsc::Sender<RecoveryEvent>>>>,
    recovery_result: Arc<Mutex<String>>,
    runtime_handle: Handle,
}

#[derive(Clone, Debug)]
pub enum RecoveryEvent {
    AudioFailure,
    HotplugDevice(DeviceProperties),
    KeyboardHotplug,
    PulseAudioEvent,
    SuspendResume,
}

pub struct OwnedApplication {
    inner: Application,
}

impl Application {
    pub fn from_config(config: Config) -> Result<OwnedApplication> {
        let paths = Arc::new(Paths::new()?);
        // Ensures config dir/file exists; the loaded value is ignored in
        // favour of the caller-supplied config.
        let _config_manager = ConfigManager::new((*paths).clone())?;

        let (recovery_tx, recovery_rx) = mpsc::channel::<RecoveryEvent>(32);

        let app = Self {
            config: Arc::new(Mutex::new(config)),
            paths,
            control: Arc::new(Mutex::new(None)),
            applying: Arc::new(AtomicBool::new(false)),
            audio_capture: Arc::new(Mutex::new(None)),
            transcription_backend: Arc::new(Mutex::new(None)),
            global_shortcuts: Arc::new(Mutex::new(None)),
            text_injector: Arc::new(Mutex::new(None)),
            audio_feedback: Arc::new(Mutex::new(None)),
            device_monitor: Arc::new(Mutex::new(None)),
            keyboard_monitor: Arc::new(Mutex::new(None)),
            suspend_monitor: Arc::new(Mutex::new(None)),
            pulse_monitor: Arc::new(Mutex::new(None)),
            mic_osd: Arc::new(Mutex::new(None)),
            recording_state: Arc::new(Mutex::new(RecordingState::Idle)),
            is_running: Arc::new(AtomicBool::new(false)),
            recovery_cooldown: Arc::new(Mutex::new(Instant::now())),
            last_recovery_time: Arc::new(AtomicU64::new(0)),
            recovery_tx: Arc::new(Mutex::new(Some(recovery_tx))),
            recovery_result: Arc::new(Mutex::new(String::new())),
            runtime_handle: Handle::current(),
        };

        app.initialize_subsystems()?;
        app.start_recovery_task(recovery_rx)?;

        Ok(OwnedApplication { inner: app })
    }

    fn initialize_subsystems(&self) -> Result<()> {
        let config = self.config.lock().clone();

        let audio = AudioCapture::new(&config.audio)?;
        *self.audio_capture.lock() = Some(audio);

        let backend = Backend::new(&config.transcription);
        *self.transcription_backend.lock() = Some(backend);

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
            RecoveryEvent::KeyboardHotplug => {
                eprintln!("Triggering recovery: Keyboard hotplug");
                self.recover_shortcuts()
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

        *self.recovery_result.lock() = match &result {
            Ok(_) => "success".to_string(),
            Err(e) => {
                self.report_error(format!("Recovery failed: {}", e));
                format!("failed: {}", e)
            }
        };

        self.write_status_files();
    }

    fn start_recording(&self) {
        {
            let mut state = self.recording_state.lock();
            if *state != RecordingState::Idle || self.applying.load(Ordering::Relaxed) {
                eprintln!("Cannot start recording: not idle");
                return;
            }
            // Reserve Recording while still holding the state lock so a
            // concurrent config apply can't slip between the check and the
            // audio stream opening.
            *state = RecordingState::Recording;
        }

        println!("Starting recording...");

        let start_result = if let Some(ref mut audio) = self.audio_capture.lock().as_mut() {
            audio.start_recording()
        } else {
            Ok(())
        };

        if let Err(e) = start_result {
            eprintln!("Failed to start recording: {:?}", e);
            *self.recording_state.lock() = RecordingState::Idle;
            self.handle_error(
                anyhow::anyhow!("Failed to start recording: {}", e),
                RecoveryEvent::AudioFailure,
            );
            return;
        }

        if let Some(feedback) = self.audio_feedback.lock().as_ref() {
            if let Err(e) = feedback.play_start() {
                eprintln!("Failed to play start sound: {:?}", e);
            }
        }

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
            match audio.stop_recording() {
                Ok(data) => data,
                Err(e) => {
                    self.report_error(format!("stop recording failed: {}", e));
                    Vec::new()
                }
            }
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
        let _zero_volume_threshold = config.audio.zero_volume_threshold;
        drop(config);

        if mute_detection {
            let is_zero_volume = if let Some(audio) = self.audio_capture.lock().as_ref() {
                audio.detect_zero_volume(&audio_data)
            } else {
                false
            };

            if is_zero_volume {
                println!("Microphone muted (zero volume), skipping transcription");
                if let Some(feedback) = self.audio_feedback.lock().as_ref() {
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
        self.runtime_handle.spawn(async move {
            let result = app.process_audio(&audio_data).await;

            *app.recording_state.lock() = RecordingState::Idle;
            app.write_status_files();

            if let Some(feedback) = app.audio_feedback.lock().as_ref() {
                let _ = feedback.play_stop();
            }

            if let Err(e) = result {
                // Transcription/injection failures are not audio stream
                // failures: report but do not trigger audio recovery.
                eprintln!("Processing error: {:?}", e);
                app.report_error(format!("Processing error: {}", e));
                if let Some(feedback) = app.audio_feedback.lock().as_ref() {
                    let _ = feedback.play_error();
                }
            }
        });
    }

    async fn process_audio(&self, audio_data: &[f32]) -> Result<String> {
        let text = self.transcribe_audio(audio_data, None, None, false).await?;

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
            (
                config.transcription.word_overrides.clone(),
                self.text_injector.clone(),
            )
        };

        let text = text.to_string();
        let overrides = word_overrides;

        tokio::task::spawn_blocking(move || {
            let mut inj = injector.lock();
            let i = inj
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Text injector not loaded"))?;
            i.set_word_overrides(overrides);
            i.inject_text(&text)
                .map_err(|e| anyhow::anyhow!("Injection error: {}", e))
        })
        .await
        .map_err(|e| WhisperTalkError::Injection(format!("Join error: {}", e)))??;

        Ok(())
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
        // Blocked ALSA recovery runs inline on a worker thread via block_in_place.
        let result = tokio::task::block_in_place(|| -> std::result::Result<(), String> {
            let mut a = audio.lock();
            let a = a
                .as_mut()
                .ok_or_else(|| "Audio capture not loaded".to_string())?;
            a.recover_stream()
                .map_err(|e| format!("Recovery failed: {}", e))
        });

        match result {
            Ok(()) => {
                eprintln!("Audio capture recovered successfully");
                *self.recovery_cooldown.lock() = now;
                self.last_recovery_time
                    .store(now.elapsed().as_secs(), Ordering::Relaxed);
                *self.recovery_result.lock() = "success".to_string();
                self.write_status_files();
                Ok(())
            }
            Err(e) => {
                eprintln!("Audio capture recovery failed: {}", e);
                *self.recovery_result.lock() = format!("failed: {}", e);
                self.write_status_files();
                Err(WhisperTalkError::System(format!("Recovery failed: {}", e)))
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

                // Blocked ALSA recovery runs inline on a worker thread via block_in_place.
                let result = tokio::task::block_in_place(|| -> std::result::Result<(), String> {
                    let mut a = audio.lock();
                    let a = a
                        .as_mut()
                        .ok_or_else(|| "Audio capture not loaded".to_string())?;
                    a.recover_stream()
                        .map_err(|e| format!("Recovery failed: {}", e))
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
            app.report_error("audio recovery failed after 6 attempts".to_string());
            *app.recovery_result.lock() = "failed: max retries".to_string();
        });

        Ok(())
    }

    fn recover_shortcuts(&self) -> Result<()> {
        let config = self.config.lock().clone();
        let recording_mode = config.shortcuts.recording_mode;

        if let Some(shortcuts) = self.global_shortcuts.lock().as_mut() {
            shortcuts.stop()?;
        }

        let shortcuts = self.build_shortcuts(&config.shortcuts)?;
        let shortcuts_clone = shortcuts.clone();
        *self.global_shortcuts.lock() = Some(shortcuts);
        shortcuts_clone.start(recording_mode)?;

        Ok(())
    }

    async fn transcribe_audio(
        &self,
        audio_data: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
        translate: bool,
    ) -> Result<String> {
        if self.applying.load(Ordering::Relaxed) {
            return Err(WhisperTalkError::Transcription(
                "settings are being applied".to_string(),
            ));
        }
        let backend = self.transcription_backend.clone();
        let audio_vec = audio_data.to_vec();
        let language = language.map(str::to_owned);
        let prompt = prompt.map(str::to_owned);

        let result = tokio::task::spawn_blocking(move || {
            let mut wb = backend.lock();
            let b = wb
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Transcription backend not loaded"))?;
            b.transcribe_with_options(
                &audio_vec,
                language.as_deref(),
                prompt.as_deref(),
                translate,
            )
        })
        .await
        .map_err(|e| WhisperTalkError::Transcription(format!("Join error: {}", e)))
        .and_then(|r| r);

        result
    }

    pub async fn transcribe_file(
        &self,
        audio_data: Vec<f32>,
        language: Option<String>,
        prompt: Option<String>,
        translate: bool,
    ) -> Result<String> {
        let result = self
            .transcribe_audio(
                &audio_data,
                language.as_deref(),
                prompt.as_deref(),
                translate,
            )
            .await;
        if let Err(e) = &result {
            self.report_error(format!("Transcription failed: {}", e));
        }
        result
    }

    pub async fn transcribe_file_segments(
        &self,
        audio_data: Vec<f32>,
        language: Option<String>,
        prompt: Option<String>,
    ) -> Result<Vec<TranscriptSegment>> {
        let backend = self.transcription_backend.clone();

        let result = tokio::task::spawn_blocking(move || {
            let mut wb = backend.lock();
            let b = wb
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Transcription backend not loaded"))?;
            b.transcribe_with_segments(&audio_data, language.as_deref(), prompt.as_deref(), false)
        })
        .await
        .map_err(|e| WhisperTalkError::Transcription(format!("Join error: {}", e)))
        .and_then(|r| r);

        if let Err(e) = &result {
            self.report_error(format!("Transcription failed: {}", e));
        }
        result
    }

    /// Push an error to the shared control state so the GUI can display it.
    /// Also mirrored to stderr by callers that already log there.
    pub fn report_error(&self, error: String) {
        if let Some(control) = self.control.lock().as_ref() {
            control.push_error(error);
        }
    }

    fn set_control_status(&self, status: &str) {
        if let Some(control) = self.control.lock().as_ref() {
            control.set_status(status);
        }
    }

    fn handle_error(&self, error: anyhow::Error, recovery_event: RecoveryEvent) {
        eprintln!("Error: {}", error);
        self.report_error(format!("{}", error));

        if let Some(feedback) = self.audio_feedback.lock().as_ref() {
            let _ = feedback.play_error();
        }

        // Use try_send to avoid requiring async context (can be called from input thread)
        let tx = self.recovery_tx.lock().clone();
        if let Some(tx) = tx {
            if let Err(e) = tx.try_send(recovery_event) {
                eprintln!("Failed to send recovery event: {}", e);
            }
        }
    }

    pub(crate) fn write_status_files(&self) {
        let state = self.recording_state.lock();
        let recording_status = match *state {
            RecordingState::Idle => "idle",
            RecordingState::Recording => "recording",
            RecordingState::Processing => "processing",
        };
        let busy = *state != RecordingState::Idle || self.applying.load(Ordering::Relaxed);
        let status_string = recording_status.to_string();
        drop(state);

        self.set_control_status(&status_string);
        if let Some(control) = self.control.lock().as_ref() {
            control.set_busy(busy);
        }

        let _ = std::fs::write(&self.paths.recording_status_file, &status_string);

        let audio_level = if let Some(audio) = self.audio_capture.lock().as_ref() {
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

    #[allow(dead_code)]
    pub fn get_audio_level(&self) -> f32 {
        self.audio_capture
            .lock()
            .as_ref()
            .map(|a| a.get_audio_level())
            .unwrap_or(0.0)
    }

    /// Attach shared control state so status/errors reach the GUI.
    pub fn set_control(&self, control: Control) {
        *self.control.lock() = Some(control);
    }

    /// Apply a new configuration live.
    ///
    /// Refuses while recording/processing or while another apply runs.
    /// Replacement subsystems are built (and the model initialized) before
    /// anything is committed; when `persist` is set the atomic
    /// ConfigManager::save runs before the swap too, so a failed apply leaves
    /// the old working runtime and on-disk config untouched.
    pub fn update_config(&self, config: Config, persist: bool) -> Result<()> {
        let old = self.config.lock().clone();

        if self.applying.swap(true, Ordering::SeqCst) {
            return Err(WhisperTalkError::InvalidConfig(
                "settings apply already in progress".to_string(),
            ));
        }
        struct ApplyingGuard(Arc<AtomicBool>);
        impl Drop for ApplyingGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _guard = ApplyingGuard(self.applying.clone());

        if *self.recording_state.lock() != RecordingState::Idle {
            return Err(WhisperTalkError::InvalidConfig(
                "cannot apply settings while recording or processing".to_string(),
            ));
        }

        // Unsupported live changes: the API listener binds once at startup,
        // and shortcut/injection/feedback subsystems aren't transactionally
        // swappable — require a restart instead of half-applying them.
        let mut restart_required: Vec<&str> = Vec::new();
        if config.api_bind != old.api_bind {
            restart_required.push("api_bind");
        }
        if config.shortcuts != old.shortcuts {
            restart_required.push("shortcuts");
        }
        if config.injection != old.injection {
            restart_required.push("injection");
        }
        if config.feedback != old.feedback {
            restart_required.push("feedback");
        }
        if !restart_required.is_empty() {
            return Err(WhisperTalkError::InvalidConfig(format!(
                "{} changes require a daemon restart",
                restart_required.join(", ")
            )));
        }

        // Validate before building anything, on SIGHUP reloads too.
        {
            let mut manager = ConfigManager::new((*self.paths).clone())?;
            *manager.get_config_mut() = config.clone();
            manager.validate()?;
        }

        // Hold the backend lock for the whole apply so in-flight and new
        // API/hotkey transcriptions serialize against the swap; a spawned
        // transcription holds it only inside spawn_blocking.
        let mut backend_slot = self.transcription_backend.lock();

        // Build replacements before committing anything.
        let new_backend = if config.transcription != old.transcription {
            let mut backend = Backend::new(&config.transcription);
            match backend.initialize(&self.paths) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(WhisperTalkError::Transcription(
                        "model not found; settings not applied".to_string(),
                    ))
                }
                Err(e) => return Err(e),
            }
            Some(backend)
        } else {
            None
        };

        let new_audio = if config.audio != old.audio {
            let audio = AudioCapture::new(&config.audio)?;
            // Reject configured mic selectors that match no input device,
            // instead of silently falling back to the default device later.
            if audio_selector_configured(&config.audio) {
                let devices = audio.enumerate_devices()?;
                if !audio_selector_matches(&config.audio, &devices) {
                    return Err(WhisperTalkError::Audio(
                        "configured microphone not found; settings not applied".to_string(),
                    ));
                }
            }
            Some(audio)
        } else {
            None
        };

        // Persist (atomic save) before swapping the live runtime.
        if persist {
            let mut manager = ConfigManager::new((*self.paths).clone())?;
            *manager.get_config_mut() = config.clone();
            manager.validate()?;
            manager.save()?;
        }

        // Commit: swap config and rebuilt subsystems.
        let audio_changed = new_audio.is_some();
        *self.config.lock() = config.clone();
        if let Some(control) = self.control.lock().as_ref() {
            control.0.lock().config = config.clone();
        }
        if let Some(backend) = new_backend {
            *backend_slot = Some(backend);
        }
        if let Some(audio) = new_audio {
            *self.audio_capture.lock() = Some(audio);
        }

        if audio_changed && self.is_running.load(Ordering::Relaxed) {
            // Monitors depend on the audio config: hotplug matching keys off
            // the configured device and the PulseAudio monitor only runs when
            // no specific device is selected.
            self.stop_monitors();
            if let Err(e) = self.start_monitors() {
                self.report_error(format!(
                    "audio settings applied but monitor restart failed: {}",
                    e
                ));
            }
        }

        self.write_status_files();
        Ok(())
    }

    fn build_shortcuts(&self, shortcuts: &crate::types::ShortcutConfig) -> Result<GlobalShortcuts> {
        let app_press = self.clone();
        let app_release = self.clone();
        GlobalShortcuts::new(
            &shortcuts.primary_shortcut,
            move || app_press.on_shortcut_press(),
            move || app_release.on_shortcut_release(),
            None,
            shortcuts.grab_keys,
        )
    }

    pub async fn run_event_loop(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        while self.is_running.load(Ordering::Relaxed) {
            interval.tick().await;

            // Surface asynchronous audio stream errors reported by the cpal
            // callback; release the audio mutex before handle_error.
            let stream_error = self
                .audio_capture
                .lock()
                .as_ref()
                .and_then(|a| a.take_stream_error());
            if let Some(message) = stream_error {
                self.handle_error(anyhow::anyhow!(message), RecoveryEvent::AudioFailure);
            }

            self.write_status_files();
        }

        Ok(())
    }
}

impl OwnedApplication {
    pub fn set_control(&self, control: Control) {
        self.inner.set_control(control);
    }

    pub async fn start(&mut self) -> Result<()> {
        self.inner.is_running.store(true, Ordering::Relaxed);

        let config = self.inner.config.lock().clone();
        let shortcut_str = config.shortcuts.primary_shortcut.clone();
        let recording_mode = config.shortcuts.recording_mode;

        let shortcuts = self.inner.build_shortcuts(&config.shortcuts)?;

        let shortcuts_clone = shortcuts.clone();
        *self.inner.global_shortcuts.lock() = Some(shortcuts);

        println!("Starting transcription backend...");
        let mut backend = self.inner.transcription_backend.lock();
        if let Some(wb) = backend.as_mut() {
            if !wb.initialize(&self.inner.paths)? {
                return Err(WhisperTalkError::Transcription(
                    "transcription model not found".to_string(),
                ));
            }
        }
        drop(backend);

        shortcuts_clone.start(recording_mode)?;

        self.inner.start_monitors()?;

        println!(
            "whisper-talk is running. Press {} to toggle recording",
            shortcut_str
        );

        self.inner.write_status_files();

        Ok(())
    }

    /// Best-effort shutdown: reports stop errors but always completes so the
    /// daemon can exit cleanly.
    pub async fn stop(&mut self) -> Result<()> {
        self.inner.is_running.store(false, Ordering::Relaxed);

        if let Some(shortcuts) = self.inner.global_shortcuts.lock().as_mut() {
            if let Err(e) = shortcuts.stop() {
                self.inner
                    .report_error(format!("failed to stop shortcuts: {}", e));
            }
        }

        self.inner.stop_monitors();

        if let Some(audio) = self.inner.audio_capture.lock().as_mut() {
            if audio.is_recording() {
                if let Err(e) = audio.stop_recording() {
                    self.inner
                        .report_error(format!("failed to stop recording: {}", e));
                }
            }
        }

        println!("whisper-talk stopped");

        Ok(())
    }

    pub fn api_application(&self) -> Application {
        self.inner.clone()
    }
}

impl Application {
    fn start_monitors(&self) -> Result<()> {
        use tracing::{debug, info, warn};

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
                    if let Some(feedback) = app.audio_feedback.lock().as_ref() {
                        let _ = feedback.play_start();
                    }
                    let tx = app.recovery_tx.lock().clone();
                    if let Some(tx) = tx {
                        app.runtime_handle.spawn(async move {
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
            if let Some(feedback) = app.audio_feedback.lock().as_ref() {
                let _ = feedback.play_error();
            }
        };

        let mut device_monitor = DeviceMonitor::new("sound", on_add, on_remove)?;
        device_monitor.start()?;
        *self.device_monitor.lock() = Some(device_monitor);
        info!("Device hotplug monitor started");

        let app = self.clone();
        let on_add = move |device: &udev::Device| {
            if !is_keyboard_input_device(device) {
                return;
            }

            info!("Keyboard device added, triggering shortcut recovery");
            let tx = app.recovery_tx.lock().clone();
            if let Some(tx) = tx {
                app.runtime_handle.spawn(async move {
                    let _ = tx.send(RecoveryEvent::KeyboardHotplug).await;
                });
            }
        };

        let app = self.clone();
        let on_remove = move |device: &udev::Device| {
            if !is_keyboard_input_device(device) {
                return;
            }

            info!("Keyboard device removed, triggering shortcut recovery");
            let tx = app.recovery_tx.lock().clone();
            if let Some(tx) = tx {
                app.runtime_handle.spawn(async move {
                    let _ = tx.send(RecoveryEvent::KeyboardHotplug).await;
                });
            }
        };

        let mut keyboard_monitor = DeviceMonitor::new("input", on_add, on_remove)?;
        keyboard_monitor.start()?;
        *self.keyboard_monitor.lock() = Some(keyboard_monitor);
        info!("Keyboard hotplug monitor started");

        // Suspend/resume monitor
        let app = self.clone();
        let on_resume = move || {
            info!("System resumed, triggering recovery");
            let tx = app.recovery_tx.lock().clone();
            if let Some(tx) = tx {
                app.runtime_handle.spawn(async move {
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
                        app.runtime_handle.spawn(async move {
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
                        app.runtime_handle.spawn(async move {
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
        if let Some(ref mut monitor) = self.keyboard_monitor.lock().take() {
            monitor.stop();
            info!("Keyboard monitor stopped");
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
        let is_recording = self
            .audio_capture
            .lock()
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
        let mode = self.config.lock().shortcuts.recording_mode;
        if mode == RecordingMode::PushToTalk {
            self.stop_recording();
        }
    }
}

fn is_keyboard_input_device(device: &udev::Device) -> bool {
    device
        .property_value("ID_INPUT_KEYBOARD")
        .and_then(|value| value.to_str())
        == Some("1")
}

impl Drop for OwnedApplication {
    fn drop(&mut self) {
        self.inner.is_running.store(false, Ordering::Relaxed);
        self.inner.stop_monitors();
        // Stop the input thread and close the recovery channel so the spawned
        // recovery task exits and drops its Application clone (and the model
        // backend it holds) instead of leaking on failed-start retries.
        if let Some(shortcuts) = self.inner.global_shortcuts.lock().as_mut() {
            let _ = shortcuts.stop();
        }
        let _ = self.inner.recovery_tx.lock().take();
    }
}

fn audio_selector_configured(audio: &crate::types::AudioConfig) -> bool {
    audio.device_name.is_some()
        || audio.device_id.is_some()
        || (audio.device_vendor_id.is_some() && audio.device_model_id.is_some())
}

/// Mirrors AudioCapture::find_device priority: vendor+model id, then exact
/// device name, then enumeration index.
fn audio_selector_matches(
    audio: &crate::types::AudioConfig,
    devices: &[crate::audio::capture::DeviceInfo],
) -> bool {
    devices.iter().any(|d| {
        audio
            .device_vendor_id
            .as_ref()
            .zip(audio.device_model_id.as_ref())
            .is_some_and(|(vid, mid)| {
                d.name.contains(vid.as_str()) && d.name.contains(mid.as_str())
            })
            || audio.device_name.as_ref().is_some_and(|n| d.name == *n)
            || audio.device_id == d.id
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::capture::DeviceInfo;
    use crate::types::AudioConfig;

    fn device(name: &str, id: u32) -> DeviceInfo {
        DeviceInfo {
            name: name.to_string(),
            id: Some(id),
            default_sample_rate: 48000,
            max_channels: 2,
        }
    }

    #[test]
    fn selector_not_configured_allows_default() {
        // No selector: the check is skipped entirely, device falls back to default.
        assert!(!audio_selector_configured(&AudioConfig::default()));
    }

    #[test]
    fn selector_matches_by_name_and_id() {
        let devices = vec![device("USB Mic", 0), device("Built-in", 1)];

        let mut cfg = AudioConfig {
            device_name: Some("Built-in".to_string()),
            ..Default::default()
        };
        assert!(audio_selector_configured(&cfg));
        assert!(audio_selector_matches(&cfg, &devices));

        cfg.device_name = Some("Missing Mic".to_string());
        assert!(!audio_selector_matches(&cfg, &devices));

        let cfg = AudioConfig {
            device_id: Some(0),
            ..Default::default()
        };
        assert!(audio_selector_matches(&cfg, &devices));

        let cfg = AudioConfig {
            device_id: Some(9),
            ..Default::default()
        };
        assert!(!audio_selector_matches(&cfg, &devices));
    }

    #[test]
    fn selector_matches_by_vendor_model() {
        let devices = vec![device("USB Audio [1234:abcd]", 0)];
        let cfg = AudioConfig {
            device_vendor_id: Some("1234".to_string()),
            device_model_id: Some("abcd".to_string()),
            ..Default::default()
        };
        assert!(audio_selector_matches(&cfg, &devices));

        let cfg = AudioConfig {
            device_vendor_id: Some("ffff".to_string()),
            device_model_id: Some("abcd".to_string()),
            ..Default::default()
        };
        assert!(!audio_selector_matches(&cfg, &devices));
    }
}
