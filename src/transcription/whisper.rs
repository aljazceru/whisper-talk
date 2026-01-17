use crate::error::{WhisperTalkError, Result};
use crate::paths::Paths;
use crate::types::TranscriptionConfig;
use whisper_rs::{WhisperContext, FullParams, WhisperContextParameters, SamplingStrategy};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 2000;
const DOWNLOAD_TIMEOUT_SECONDS: u64 = 300;

pub struct WhisperBackend {
    context: Option<WhisperContext>,
    config: TranscriptionConfig,
    model_name: String,
    use_gpu: bool,
}

impl WhisperBackend {
    pub fn new(config: &TranscriptionConfig) -> Self {
        Self {
            context: None,
            config: config.clone(),
            model_name: config.model.clone(),
            use_gpu: Self::detect_gpu(),
        }
    }

    fn detect_gpu() -> bool {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .output()
        {
            if output.status.success() && !String::from_utf8_lossy(&output.stdout).contains("not found") {
                println!("Detected NVIDIA GPU, enabling CUDA support");
                return true;
            }
        }

        if let Ok(output) = std::process::Command::new("rocm-smi")
            .output()
        {
            if output.status.success() {
                println!("Detected AMD GPU, enabling ROCm support");
                return true;
            }
        }

        println!("No GPU detected, using CPU mode");
        false
    }

    pub fn initialize(&mut self, paths: &Paths) -> Result<bool> {
        let model_path = self.find_model_path(paths)?;

        if !model_path.exists() {
            println!("Model not found, attempting download to: {}", model_path.display());
            self.download_model(&model_path)?;
        }

        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| WhisperTalkError::Transcription("Invalid model path".to_string()))?;

        println!("Loading Whisper model from: {} (GPU: {})", model_path_str, self.use_gpu);

        let context_params = WhisperContextParameters {
            use_gpu: self.use_gpu,
            ..Default::default()
        };

        let context = WhisperContext::new_with_params(model_path_str, context_params)
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to load model: {}", e)))?;

        self.context = Some(context);
        Ok(true)
    }

    pub fn transcribe(&self, audio_data: &[f32]) -> Result<String> {
        self.transcribe_internal(audio_data, None)
    }

    /// Transcribe audio for streaming mode.
    /// Returns (full_transcription, new_text_to_output).
    /// The new_text_to_output is the delta from previous_text.
    pub fn transcribe_streaming(
        &self,
        audio_data: &[f32],
        previous_text: &str,
    ) -> Result<(String, String)> {
        // Do NOT pass previous_text as context_hint to transcribe_internal.
        // Doing so with the full audio history causes feedback loops and hallucinations.
        let full_text = self.transcribe_internal(audio_data, None)?;

        // Find the new text that should be output
        let new_text = self.extract_new_text(previous_text, &full_text);

        Ok((full_text, new_text))
    }

    fn transcribe_internal(&self, audio_data: &[f32], context_hint: Option<&str>) -> Result<String> {
        let context = self.context.as_ref()
            .ok_or_else(|| WhisperTalkError::Transcription("Model not loaded".to_string()))?;

        let mut state = context.create_state()
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to create state: {}", e)))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });

        params.set_n_threads(self.config.threads as i32);
        params.set_offset_ms(0);
        params.set_duration_ms(0);
        params.set_translate(false);
        params.set_no_speech_thold(0.6f32);
        params.set_temperature(0.0f32);
        params.set_max_initial_ts(1.0f32);
        params.set_max_len(224);

        if let Some(ref lang) = self.config.language {
            params.set_language(Some(lang.as_str()));
        } else {
            // Default to English if no language is specified to avoid hallucinations on short chunks
            params.set_language(Some("en"));
        }

        // Use context hint for better streaming continuity, or fall back to config prompt
        if let Some(hint) = context_hint {
            if !hint.is_empty() {
                // Use the previous transcription as context for better continuity
                let prompt = format!("{} {}", self.config.whisper_prompt, hint);
                params.set_initial_prompt(&prompt);
            } else if !self.config.whisper_prompt.is_empty() {
                params.set_initial_prompt(&self.config.whisper_prompt);
            }
        } else if !self.config.whisper_prompt.is_empty() {
            params.set_initial_prompt(&self.config.whisper_prompt);
        }

        params.set_token_timestamps(false);

        state.full(params, audio_data)
            .map_err(|e| WhisperTalkError::Transcription(format!("Transcription failed: {}", e)))?;

        let mut transcription = String::new();
        let num_segments = state.full_n_segments();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                let segment_text = segment.to_str_lossy().unwrap_or_default().to_string();
                transcription.push_str(&segment_text);
            }
        }

        self.filter_hallucinations(&mut transcription);

        Ok(transcription.trim().to_string())
    }

    /// Extract the new text that should be typed, given previous and current transcription.
    /// This handles the case where Whisper might slightly revise earlier text.
    /// Extract the new text that should be typed, given previous and current transcription.
    /// This handles the case where Whisper might slightly revise earlier text.
    fn extract_new_text(&self, previous: &str, current: &str) -> String {
        if previous.is_empty() {
            return current.to_string();
        }

        if current.is_empty() {
            return String::new();
        }

        // Normalize for robust comparison (ignore case and punctuation)
        fn normalize_word(s: &str) -> String {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        }

        // Helper to count visible characters (to find byte offset later)
        // We will just split by whitespace and reconstruct for now, 
        // effectively returning the words from current that are new.
        
        let prev_words: Vec<&str> = previous.split_whitespace().collect();
        let curr_words: Vec<&str> = current.split_whitespace().collect();

        // Find longest common prefix based on NORMALIZED words
        let mut common_count = 0;
        for (pw, cw) in prev_words.iter().zip(curr_words.iter()) {
            if normalize_word(pw) == normalize_word(cw) {
                common_count += 1;
            } else {
                break;
            }
        }

        // Return words after the common prefix
        if common_count < curr_words.len() {
            // Reconstruct the new text preserving original spacing/punctuation of the new words
            // This isn't perfect (loses spacing between the split point), but good enough for streaming
            // A better way is to find the byte offset of the Nth word.
            
            // Let's find the byte offset of the start of the (common_count)-th word in current
            // actually we want the (common_count)-th word to be SKIPPED.
            // so we want the (common_count + 1)-th word.
            
            // curr_words = ["Hello", "world.", "How", "are"]
            // common = 2 ("Hello", "world.")
            // we want "How are".
            // curr_words[2..].join(" ")
            
             curr_words[common_count..].join(" ")
        } else {
            String::new()
        }
    }

    fn filter_hallucinations(&self, text: &mut String) {
        for marker in &self.config.hallucination_markers {
            if text.contains(marker) {
                text.clear();
                break;
            }
        }
    }

    fn find_model_path(&self, paths: &Paths) -> Result<PathBuf> {
        let model_variants = if let Some(ref lang) = self.config.language {
            vec![
                format!("ggml-{}.{}.bin", self.model_name, lang),
                format!("ggml-{}.en.bin", self.model_name),
                format!("ggml-{}.bin", self.model_name),
            ]
        } else {
            vec![
                format!("ggml-{}.en.bin", self.model_name),
                format!("ggml-{}.bin", self.model_name),
            ]
        };

        for search_dir in &paths.model_search_dirs {
            if !search_dir.exists() {
                continue;
            }

            for variant in &model_variants {
                let model_path = search_dir.join(variant);
                if model_path.exists() {
                    println!("Found model at: {}", model_path.display());
                    return Ok(model_path);
                }
            }
        }

        Err(WhisperTalkError::ModelNotFound(format!("Model '{}' not found in any search path", self.model_name)))
    }

    fn download_model(&self, target_path: &PathBuf) -> Result<()> {
        let model_filename = target_path
            .file_name()
            .ok_or_else(|| WhisperTalkError::Transcription("Invalid model filename".to_string()))?
            .to_str()
            .ok_or_else(|| WhisperTalkError::Transcription("Invalid model filename encoding".to_string()))?;

        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            model_filename
        );

        println!("Downloading model from: {}", url);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
            .user_agent("gwhpr/0.1.0")
            .build()
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to create HTTP client: {}", e)))?;

        let parent_dir = target_path
            .parent()
            .ok_or_else(|| WhisperTalkError::Transcription("Invalid model path parent".to_string()))?;

        fs::create_dir_all(parent_dir)
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to create models directory: {}", e)))?;

        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            if attempt > 1 {
                println!("Retry {}/{} after {}ms delay...", attempt, MAX_RETRIES, RETRY_DELAY_MS);
                std::thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
            }

            match self.download_with_progress(&client, &url, target_path) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("Download attempt {} failed: {}", attempt, e);
                    last_error = Some(e);

                    if target_path.exists() {
                        let _ = fs::remove_file(target_path);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| WhisperTalkError::Transcription("Download failed".to_string())))
    }

    fn download_with_progress(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
        target_path: &PathBuf,
    ) -> Result<()> {
        let response = client
            .get(url)
            .send()
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to start download: {}", e)))?;

        if !response.status().is_success() {
            return Err(WhisperTalkError::Transcription(format!(
                "HTTP error downloading model: {}",
                response.status()
            )));
        }

        let total_size = response
            .content_length()
            .unwrap_or(0);

        let file = File::create(target_path)
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to create file: {}", e)))?;

        let mut writer = BufWriter::new(file);
        let mut downloaded = 0u64;
        let mut last_update = std::time::Instant::now();

        let bytes = response
            .bytes()
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to read response: {}", e)))?;

        let chunk_size = 8192;

        for chunk in bytes.chunks(chunk_size) {
            writer.write_all(chunk)
                .map_err(|e| WhisperTalkError::Transcription(format!("Failed to write chunk: {}", e)))?;

            downloaded += chunk.len() as u64;

            if total_size > 0 && last_update.elapsed().as_millis() > 500 {
                let percent = (downloaded as f64 / total_size as f64) * 100.0;
                let mb_downloaded = downloaded as f64 / (1024.0 * 1024.0);
                let mb_total = total_size as f64 / (1024.0 * 1024.0);
                println!("Download progress: {:.1}% ({:.1}/{:.1} MB)", percent, mb_downloaded, mb_total);
                last_update = std::time::Instant::now();
            }
        }

        writer.flush()
            .map_err(|e| WhisperTalkError::Transcription(format!("Failed to flush file: {}", e)))?;

        println!("Model download complete");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_model_name(&self) -> &str {
        &self.model_name
    }

    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool {
        self.context.is_some()
    }

    #[allow(dead_code)]
    pub fn is_gpu_enabled(&self) -> bool {
        self.use_gpu
    }
}
