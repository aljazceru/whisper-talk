use std::path::PathBuf;

#[derive(Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub models_dir: PathBuf,
    pub model_search_dirs: Vec<PathBuf>,
    pub assets_dir: PathBuf,
    pub lock_file: PathBuf,
    pub recording_status_file: PathBuf,
    pub audio_level_file: PathBuf,
    pub recovery_result_file: PathBuf,
    pub recovery_requested_file: PathBuf,
    pub mic_zero_volume_file: PathBuf,
}

impl Paths {
    pub fn new() -> anyhow::Result<Self> {
        let proj_dirs = directories::ProjectDirs::from("com.github.goodroot", "goodroot", "gwhspr")
            .ok_or_else(|| anyhow::anyhow!("Failed to get project directories"))?;

        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        let data_dir = proj_dirs.data_dir().to_path_buf();
        let state_dir = proj_dirs.state_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| data_dir.join("state"));
        let cache_dir = proj_dirs.cache_dir().to_path_buf();

        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(&state_dir)?;

        let models_dir = home.join(".local/share/whisper");

        let model_search_dirs = vec![
            home.join(".cache/whispercpp"),
            home.join(".local/share/whisper"),
            home.join(".local/share/pywhispercpp/models"),
            PathBuf::from("./models"),
        ];

        Ok(Self {
            config_dir: config_dir.clone(),
            config_file: config_dir.join("config.json"),
            models_dir,
            model_search_dirs,
            assets_dir: Self::find_assets_dir(),
            lock_file: state_dir.join("lock"),
            recording_status_file: state_dir.join("recording_status"),
            audio_level_file: state_dir.join("audio_level"),
            recovery_result_file: state_dir.join("recovery_result"),
            recovery_requested_file: state_dir.join("recovery_requested"),
            mic_zero_volume_file: state_dir.join("mic_zero_volume"),
            data_dir,
            state_dir,
            cache_dir,
        })
    }

    fn find_assets_dir() -> PathBuf {
        if let Ok(root) = std::env::var("GWHSPR_ROOT") {
            let dir = PathBuf::from(root).join("share").join("assets");
            if dir.exists() {
                return dir;
            }
        }

        PathBuf::from("./share/assets")
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new().expect("Failed to initialize paths")
    }
}
