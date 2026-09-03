use crate::audio::capture::AudioCapture;
use crate::cli::model;
use crate::paths::Paths;
use crate::transcription::Backend;
use crate::types::{AudioConfig, Config, TranscriptionBackend, TranscriptionConfig};
use anyhow::Result;
use clap::Args;
use dialoguer::{Confirm, Select};

#[derive(Args, Debug)]
pub struct SetupArgs {}

pub fn run_setup(_args: SetupArgs) -> Result<()> {
    let wizard = SetupWizard;
    wizard.run()
}

pub struct SetupWizard;

impl SetupWizard {
    pub fn run(&self) -> Result<()> {
        println!();
        println!("╔════════════════════════════════════════════════════════════════╗");
        println!("║                  Whisper Talk Setup                             ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!();

        let paths = Paths::new()?;
        let config = Config::default();

        // Show defaults
        println!("Default configuration:");
        println!(
            "  Shortcut:   {} (toggle recording)",
            config.shortcuts.primary_shortcut
        );
        println!("  Model:      base (good balance of speed/accuracy)");
        println!("  Microphone: System default");
        println!();

        let use_defaults = Confirm::new()
            .with_prompt("Use these defaults?")
            .default(true)
            .interact()?;

        let (backend, model, audio_config) = if use_defaults {
            (
                TranscriptionBackend::Whisper,
                "base".to_string(),
                AudioConfig::default(),
            )
        } else {
            let backend = self.select_backend()?;
            let model = self.select_model(backend)?;
            let audio = self.select_audio_device()?;
            (backend, model, audio)
        };

        // Download/verify model
        println!();
        println!("Checking model...");
        self.download_model(&model, backend, &paths)?;

        // Systemd service
        let enable_systemd = self.prompt_systemd()?;

        if enable_systemd {
            println!();
            self.install_systemd_service(&paths)?;
        }

        // Save config
        let mut final_config = config;
        final_config.transcription.backend = backend;
        final_config.transcription.model = model;
        final_config.audio = audio_config;

        self.save_config(final_config, &paths)?;

        println!();
        println!("Setup complete!");
        println!();
        println!("Start with:  whisper-talk daemon");
        println!("Or enable:   systemctl --user enable --now whisper-talk");
        println!();

        Ok(())
    }

    fn select_backend(&self) -> Result<TranscriptionBackend> {
        println!("Select Transcription Backend:");
        println!("  Whisper:   Local transcription, works on CPU or GPU (CUDA/ROCm)");
        println!("  Parakeet:  Requires ONNX Runtime 1.20.1; GPU with CUDA or ROCm optional");
        println!();

        let selection = Select::new()
            .with_prompt("Which transcription backend would you like to use?")
            .item("Whisper (recommended, works on CPU and GPU)")
            .item("Parakeet-v3 (requires ONNX Runtime)")
            .default(0)
            .interact()?;

        Ok(match selection {
            0 => TranscriptionBackend::Whisper,
            1 => TranscriptionBackend::ParakeetV3,
            _ => TranscriptionBackend::Whisper,
        })
    }

    fn select_model(&self, backend: TranscriptionBackend) -> Result<String> {
        println!();

        match backend {
            TranscriptionBackend::Whisper => {
                println!("Select Whisper Model:");
                println!("  tiny:     ~39MB, very fast, lower accuracy");
                println!("  base:     ~74MB, fast, good accuracy (recommended)");
                println!("  small:    ~244MB, moderate speed, better accuracy");
                println!("  medium:   ~769MB, slower, high accuracy");
                println!("  large-v3: ~1.5GB, slowest, highest accuracy");
                println!();

                let items = vec![
                    "tiny (39MB, very fast, lower accuracy)",
                    "base (74MB, fast, good accuracy)",
                    "small (244MB, moderate, better accuracy)",
                    "medium (769MB, slower, high accuracy)",
                    "large-v3 (1.5GB, slowest, highest accuracy)",
                ];

                let selection = Select::new()
                    .with_prompt("Which Whisper model would you like to download?")
                    .items(&items)
                    .default(1)
                    .interact()?;

                Ok(match selection {
                    0 => "tiny",
                    1 => "base",
                    2 => "small",
                    3 => "medium",
                    4 => "large-v3",
                    _ => "base",
                }
                .to_string())
            }
            TranscriptionBackend::ParakeetV3 => {
                println!("Parakeet-v3 model will be downloaded.");
                Ok("parakeet-v3".to_string())
            }
        }
    }

    fn select_audio_device(&self) -> Result<AudioConfig> {
        println!();
        println!("--- Audio Device ---");
        println!("By default, whisper-talk uses your system's default microphone.");

        let use_default = Confirm::new()
            .with_prompt("Use system default microphone?")
            .default(true)
            .interact()?;

        if use_default {
            return Ok(AudioConfig::default());
        }

        // User wants to select a specific device
        let capture = AudioCapture::new(&AudioConfig::default())?;
        let devices = capture.enumerate_devices()?;

        if devices.is_empty() {
            println!("No audio input devices detected. Using system default.");
            return Ok(AudioConfig::default());
        }

        let device_items: Vec<String> = devices
            .iter()
            .map(|d| format!("{} ({} Hz)", d.name, d.default_sample_rate))
            .collect();

        let selection = Select::new()
            .with_prompt("Select audio input device")
            .items(&device_items)
            .default(0)
            .interact()?;

        let device = &devices[selection];

        Ok(AudioConfig {
            device_id: device.id,
            device_name: Some(device.name.clone()),
            device_vendor_id: None,
            device_model_id: None,
            mute_detection: true,
            zero_volume_threshold: 5e-7,
        })
    }

    fn prompt_systemd(&self) -> Result<bool> {
        println!();
        Confirm::new()
            .with_prompt("Install systemd service (auto-start on login)?")
            .default(true)
            .interact()
            .map_err(Into::into)
    }

    #[allow(dead_code)]
    fn show_permission_commands(&self) -> Result<()> {
        println!("╔═══════════════════════════════════════════════════════════════╗");
        println!("║              System Permissions Setup Commands                 ║");
        println!("╚═══════════════════════════════════════════════════════════════╝");
        println!();
        println!("For whisper-talk to work properly, your user needs the following permissions:");
        println!();
        println!("1. ydotool group (for text injection):");
        println!("   sudo groupadd ydotool");
        println!("   sudo usermod -aG ydotool $USER");
        println!();
        println!("2. Audio group (for microphone access):");
        println!("   sudo usermod -aG audio $USER");
        println!();
        println!("3. Input group (for global hotkeys):");
        println!("   sudo usermod -aG input $USER");
        println!();
        println!("After running these commands, log out and log back in for the");
        println!("changes to take effect.");
        println!();
        println!("To check your current groups:");
        println!("   groups");
        println!();

        Ok(())
    }

    fn download_model(
        &self,
        model: &str,
        backend: TranscriptionBackend,
        paths: &Paths,
    ) -> Result<()> {
        println!("Preparing to download/verify model: {}", model);
        println!("Model will be saved to: {}", paths.models_dir.display());
        println!();

        model::run_model_download(model, false, paths)?;

        let config = TranscriptionConfig {
            backend,
            model: model.to_string(),
            ..Default::default()
        };

        let mut backend = Backend::new(&config);
        if !backend.initialize(paths)? {
            anyhow::bail!("Model '{}' could not be initialized", model);
        }

        println!("Model '{}' is ready for use.", model);
        Ok(())
    }

    fn install_systemd_service(&self, _paths: &Paths) -> Result<()> {
        let service_dir = std::path::PathBuf::from("/etc/systemd/user");
        let service_file = service_dir.join("whisper-talk.service");

        let service_content = r#"[Unit]
Description=Whisper Talk Voice Dictation
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/whisper-talk daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
        .to_string();

        println!(
            "Creating systemd service file at: {}",
            service_file.display()
        );

        if std::process::Command::new("sudo")
            .args(["mkdir", "-p", service_dir.to_str().unwrap()])
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to create service directory: {}", e))?
            .success()
        {
            let service_path = std::path::PathBuf::from("/tmp/whisper-talk.service");
            std::fs::write(&service_path, &service_content)?;

            println!();
            println!("To install the systemd service, run the following commands:");
            println!();
            println!("  sudo cp /tmp/whisper-talk.service /etc/systemd/user/whisper-talk.service");
            println!("  systemctl --user daemon-reload");
            println!("  systemctl --user enable whisper-talk");
            println!("  systemctl --user start whisper-talk");
            println!();
            println!("To check the service status:");
            println!("  systemctl --user status whisper-talk");
            println!();
            println!("Service file created at: {}", service_path.display());
        } else {
            println!("Failed to create systemd service directory. Please create manually:");
            println!();
            println!("  sudo mkdir -p {}", service_dir.display());
            println!("  sudo tee {} << 'EOF'", service_file.display());
            println!("{}", service_content);
            println!("EOF");
            println!("  systemctl --user daemon-reload");
            println!("  systemctl --user enable whisper-talk");
        }

        Ok(())
    }

    fn save_config(&self, config: Config, paths: &Paths) -> Result<()> {
        println!("Saving configuration to: {}", paths.config_file.display());

        let config_dir = paths
            .config_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid config directory"))?;

        std::fs::create_dir_all(config_dir)?;

        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;

        std::fs::write(&paths.config_file, content)
            .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;

        println!("Configuration saved successfully.");
        println!();
        println!("Configuration summary:");
        println!("  Backend:  {:?}", config.transcription.backend);
        println!("  Model:    {}", config.transcription.model);
        println!("  Threads:  {}", config.transcription.threads);
        println!(
            "  Audio:    {}",
            config.audio.device_name.as_deref().unwrap_or("default")
        );

        Ok(())
    }
}
