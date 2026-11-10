use crate::error::{Result, WhisperTalkError};
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

        let mut file = tempfile::NamedTempFile::new_in(&self.paths.config_dir).map_err(|e| {
            WhisperTalkError::Config(format!("Failed to create temporary config file: {}", e))
        })?;

        file.write_all(content.as_bytes())
            .map_err(|e| WhisperTalkError::Config(format!("Failed to write config file: {}", e)))?;
        file.as_file()
            .sync_all()
            .map_err(|e| WhisperTalkError::Config(format!("Failed to sync config file: {}", e)))?;
        file.persist(&self.paths.config_file).map_err(|e| {
            WhisperTalkError::Config(format!("Failed to replace config file: {}", e.error))
        })?;

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
            return Err(WhisperTalkError::InvalidConfig(
                "Shortcut cannot be empty".to_string(),
            ));
        }

        if self.config.transcription.model.is_empty() {
            return Err(WhisperTalkError::InvalidConfig(
                "Model cannot be empty".to_string(),
            ));
        }

        if self.config.transcription.threads == 0 {
            return Err(WhisperTalkError::InvalidConfig(
                "Threads must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn manager(dir: &std::path::Path) -> ConfigManager {
        ConfigManager::new(Paths {
            config_dir: dir.into(),
            config_file: dir.join("config.json"),
            data_dir: dir.into(),
            state_dir: dir.into(),
            cache_dir: dir.into(),
            models_dir: dir.into(),
            model_search_dirs: Vec::new(),
            assets_dir: dir.into(),
            lock_file: dir.join("lock"),
            recording_status_file: dir.join("recording_status"),
            audio_level_file: dir.join("audio_level"),
            recovery_result_file: dir.join("recovery_result"),
            recovery_requested_file: dir.join("recovery_requested"),
            mic_zero_volume_file: dir.join("mic_zero_volume"),
        })
        .unwrap()
    }

    #[test]
    fn save_keeps_existing_readers_on_complete_previous_config() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = manager(dir.path());
        let previous = manager.get_config().clone();
        let mut reader = fs::File::open(manager.get_config_path()).unwrap();
        manager.get_config_mut().transcription.model = "base".into();
        manager.save().unwrap();

        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&content).unwrap(), previous);
        manager.load().unwrap();
        assert_eq!(manager.get_config().transcription.model, "base");
    }

    #[test]
    fn failed_replace_leaves_destination_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = manager(dir.path());
        let destination = dir.path().join("existing-directory");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep"), "untouched").unwrap();
        manager.paths.config_file = destination.clone();

        assert!(manager.save().is_err());
        assert_eq!(
            fs::read_to_string(destination.join("keep")).unwrap(),
            "untouched"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
