use crate::error::{WhisperTalkError, Result};
use crate::paths::Paths;
use crate::types::Config;
use std::fs;
use std::io::Write;

pub struct ConfigManager {
    paths: Paths,
    config: Config,
}

impl ConfigManager {
    pub fn new(paths: Paths) -> Result<Self> {
        let mut manager = Self {
            paths,
            config: Config::default(),
        };

        if manager.paths.config_file.exists() {
            manager.load()?;
        } else {
            manager.save()?;
        }

        Ok(manager)
    }

    pub fn load(&mut self) -> Result<()> {
        let content = fs::read_to_string(&self.paths.config_file)
            .map_err(|e| WhisperTalkError::Config(format!("Failed to read config file: {}", e)))?;

        let loaded_config: Config = serde_json::from_str(&content)
            .map_err(|e| WhisperTalkError::Config(format!("Failed to parse config: {}", e)))?;

        self.migrate(&loaded_config)?;
        self.config = loaded_config;

        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.config)
            .map_err(|e| WhisperTalkError::Config(format!("Failed to serialize config: {}", e)))?;

        let mut file = fs::File::create(&self.paths.config_file)
            .map_err(|e| WhisperTalkError::Config(format!("Failed to create config file: {}", e)))?;

        file.write_all(content.as_bytes())
            .map_err(|e| WhisperTalkError::Config(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn get_config_path(&self) -> &std::path::Path {
        &self.paths.config_file
    }

    fn migrate(&mut self, _config: &Config) -> Result<()> {
        // Perform any necessary config migrations here
        // (e.g., updating old config formats to new ones)
        // Validation is handled separately by the validate() function
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.config.shortcuts.primary_shortcut.is_empty() {
            return Err(WhisperTalkError::InvalidConfig("Shortcut cannot be empty".to_string()));
        }

        if self.config.transcription.model.is_empty() {
            return Err(WhisperTalkError::InvalidConfig("Model cannot be empty".to_string()));
        }

        if self.config.transcription.threads == 0 {
            return Err(WhisperTalkError::InvalidConfig("Threads must be > 0".to_string()));
        }

        Ok(())
    }
}
