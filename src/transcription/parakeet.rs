use crate::error::{Result, WhisperTalkError};
use crate::paths::Paths;
use crate::transcription::TranscriptSegment;
use crate::types::TranscriptionConfig;
#[cfg(feature = "cuda")]
use ort::ep::CUDA;
use ort::ep::{ROCm, CPU};
use parakeet_rs::{ExecutionConfig, ParakeetTDT, TimestampMode, Transcriber};
use std::path::PathBuf;

pub(crate) const ONNX_RUNTIME_HELP: &str = "Parakeet requires an ONNX Runtime 1.20.1 shared library.\n\
    \n\
    Install one of the following and make sure libonnxruntime.so.1.20.1 is on the\n\
    library search path or set ORT_DYLIB_PATH to its full path:\n\
    \n\
    - CPU runtime: https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz\n\
      (extract and point ORT_DYLIB_PATH at lib/libonnxruntime.so.1.20.1,\n\
      or extract to ~/.local/share/whisper/ort/lib/libonnxruntime.so.1.20.1)\n\
    - NVIDIA/CUDA runtime: https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-gpu-1.20.1.tgz\n\
    - AMD/ROCm: build or install a ROCm-enabled libonnxruntime.so.1.20.1\n\
      (e.g. onnxruntime-rocm in Fedora, or a custom build).";

pub struct ParakeetBackend {
    config: TranscriptionConfig,
    model: parking_lot::Mutex<Option<ParakeetTDT>>,
    gpu: GpuProvider,
}

#[derive(Clone, Copy, Debug)]
enum GpuProvider {
    None,
    Rocm,
    #[cfg(feature = "cuda")]
    Cuda,
}

impl ParakeetBackend {
    pub fn new(config: &TranscriptionConfig) -> Self {
        Self {
            config: config.clone(),
            model: parking_lot::Mutex::new(None),
            gpu: Self::detect_gpu(),
        }
    }

    fn detect_gpu() -> GpuProvider {
        #[cfg(feature = "cuda")]
        if Self::has_nvidia_gpu() {
            println!("Detected NVIDIA GPU, enabling CUDA support");
            return GpuProvider::Cuda;
        }

        if Self::has_rocm_gpu() && Self::find_rocm_dylib().is_some() {
            println!("Detected AMD GPU with ROCm ONNX Runtime, enabling ROCm support");
            return GpuProvider::Rocm;
        }

        println!("No GPU-capable ONNX Runtime found, using CPU mode");
        GpuProvider::None
    }

    #[cfg(feature = "cuda")]
    fn has_nvidia_gpu() -> bool {
        if let Ok(output) = std::process::Command::new("nvidia-smi").output() {
            output.status.success()
                && !String::from_utf8_lossy(&output.stdout).contains("not found")
        } else {
            false
        }
    }

    fn has_rocm_gpu() -> bool {
        if let Ok(output) = std::process::Command::new("rocm-smi").output() {
            output.status.success()
        } else {
            false
        }
    }

    pub fn initialize(&mut self, paths: &Paths) -> Result<bool> {
        let model_dir = self.find_model_dir(paths)?;

        let dylib = Self::find_onnxruntime_dylib();
        let dylib = dylib.ok_or_else(|| {
            WhisperTalkError::Transcription(format!(
                "No ONNX Runtime library found.\n\n{}",
                ONNX_RUNTIME_HELP
            ))
        })?;

        ort::init_from(&dylib)
            .map_err(|e| {
                WhisperTalkError::Transcription(format!("Failed to load ONNX Runtime dylib: {}", e))
            })?
            .commit();

        let mut exec_config = ExecutionConfig::new();

        let gpu = self.gpu;
        if !matches!(gpu, GpuProvider::None) {
            exec_config = exec_config.with_custom_configure(move |builder| {
                match gpu {
                    #[cfg(feature = "cuda")]
                    GpuProvider::Cuda => {
                        let cuda = CUDA::default().build().error_on_failure();
                        let cpu = CPU::default().build().error_on_failure();
                        let builder_for_cpu = builder.clone();
                        match builder.with_execution_providers([cuda, cpu]) {
                            Ok(b) => Ok(b),
                            Err(e) => {
                                eprintln!(
                                    "Warning: CUDA execution provider failed ({}), falling back to CPU.",
                                    e
                                );
                                Ok(builder_for_cpu.with_execution_providers([CPU::default().build().error_on_failure()])?)
                            }
                        }
                    }
                    GpuProvider::Rocm => {
                        let rocm = ROCm::default().build().error_on_failure();
                        let cpu = CPU::default().build().error_on_failure();
                        let builder_for_cpu = builder.clone();
                        match builder.with_execution_providers([rocm, cpu]) {
                            Ok(b) => Ok(b),
                            Err(e) => {
                                eprintln!(
                                    "Warning: ROCm execution provider failed ({}), falling back to CPU.",
                                    e
                                );
                                Ok(builder_for_cpu.with_execution_providers([CPU::default().build().error_on_failure()])?)
                            }
                        }
                    }
                    GpuProvider::None => {
                        Ok(builder.with_execution_providers([CPU::default().build().error_on_failure()])?)
                    }
                }
            });
        }

        let model = ParakeetTDT::from_pretrained(&model_dir, Some(exec_config)).map_err(|e| {
            WhisperTalkError::Transcription(format!("Failed to load Parakeet model: {}", e))
        })?;

        *self.model.lock() = Some(model);
        Ok(true)
    }

    pub fn transcribe_with_options(
        &self,
        audio_data: &[f32],
        _language: Option<&str>,
        _prompt: Option<&str>,
        translate: bool,
    ) -> Result<String> {
        if translate {
            return Err(WhisperTalkError::UnsupportedOperation(
                "Translation is not supported by the Parakeet backend".to_string(),
            ));
        }

        let mut model_guard = self.model.lock();
        let model = model_guard.as_mut().ok_or_else(|| {
            WhisperTalkError::Transcription("Parakeet model not loaded".to_string())
        })?;

        let result = model
            .transcribe_samples(
                audio_data.to_vec(),
                16000,
                1,
                Some(TimestampMode::Sentences),
            )
            .map_err(|e| WhisperTalkError::Transcription(format!("Transcription failed: {}", e)))?;

        let mut text = result.text;
        self.filter_hallucinations(&mut text);
        Ok(text.trim().to_string())
    }

    pub fn transcribe_with_segments(
        &self,
        audio_data: &[f32],
        _language: Option<&str>,
        _prompt: Option<&str>,
        translate: bool,
    ) -> Result<Vec<TranscriptSegment>> {
        if translate {
            return Err(WhisperTalkError::UnsupportedOperation(
                "Translation is not supported by the Parakeet backend".to_string(),
            ));
        }

        let mut model_guard = self.model.lock();
        let model = model_guard.as_mut().ok_or_else(|| {
            WhisperTalkError::Transcription("Parakeet model not loaded".to_string())
        })?;

        let result = model
            .transcribe_samples(
                audio_data.to_vec(),
                16000,
                1,
                Some(TimestampMode::Sentences),
            )
            .map_err(|e| WhisperTalkError::Transcription(format!("Transcription failed: {}", e)))?;

        let segments: Vec<TranscriptSegment> = result
            .tokens
            .into_iter()
            .filter_map(|token| {
                let mut text = token.text;
                self.filter_hallucinations(&mut text);
                if text.trim().is_empty() {
                    None
                } else {
                    Some(TranscriptSegment {
                        start: token.start as f64,
                        end: token.end as f64,
                        text,
                    })
                }
            })
            .collect();

        Ok(segments)
    }

    fn filter_hallucinations(&self, text: &mut String) {
        for marker in &self.config.hallucination_markers {
            if text.contains(marker) {
                text.clear();
                break;
            }
        }
    }

    fn find_model_dir(&self, paths: &Paths) -> Result<PathBuf> {
        let model_name = &self.config.model;
        for search_dir in &paths.model_search_dirs {
            let candidate = search_dir.join(model_name);
            if candidate.is_dir() && candidate.join("vocab.txt").exists() {
                return Ok(candidate);
            }
        }

        Err(WhisperTalkError::ModelNotFound(format!(
            "Parakeet model '{}' not found in any search path",
            model_name
        )))
    }

    pub(crate) fn find_onnxruntime_dylib() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("ORT_DYLIB_PATH") {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        Self::find_rocm_dylib()
            .or_else(Self::find_user_dylib)
            .or_else(Self::find_cpu_dylib)
    }

    fn find_rocm_dylib() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("/usr/lib64/rocm/lib/libonnxruntime.so.1.20.1"),
            PathBuf::from("/lib64/rocm/lib/libonnxruntime.so.1.20.1"),
            dirs::home_dir()
                .map(|h| {
                    h.join(
                        ".local/share/whisper/ort-rocm/usr/lib64/rocm/lib/libonnxruntime.so.1.20.1",
                    )
                })
                .unwrap_or_default(),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
        None
    }

    fn find_user_dylib() -> Option<PathBuf> {
        let candidates = [
            dirs::home_dir()
                .map(|h| h.join(".local/share/whisper/ort/lib/libonnxruntime.so.1.20.1"))
                .unwrap_or_default(),
            dirs::home_dir()
                .map(|h| h.join(".local/share/whisper/ort-cpu/lib/libonnxruntime.so.1.20.1"))
                .unwrap_or_default(),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
        None
    }

    fn find_cpu_dylib() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("/lib64/libonnxruntime.so.1.20.1"),
            PathBuf::from("/usr/lib64/libonnxruntime.so.1.20.1"),
            PathBuf::from("/lib/x86_64-linux-gnu/libonnxruntime.so.1.20.1"),
            dirs::home_dir()
                .map(|h| h.join(".local/lib/libonnxruntime.so.1.20.1"))
                .unwrap_or_default(),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TranscriptionBackend;

    #[test]
    fn test_parakeet_rejects_translation() {
        let config = TranscriptionConfig {
            backend: TranscriptionBackend::ParakeetV3,
            model: "parakeet-v3".to_string(),
            ..Default::default()
        };
        let backend = ParakeetBackend::new(&config);
        let audio = vec![0.0; 16000];

        let err = backend
            .transcribe_with_options(&audio, None, None, true)
            .unwrap_err();
        assert!(
            err.to_string().contains("Translation is not supported"),
            "unexpected error: {}",
            err
        );

        let err = backend
            .transcribe_with_segments(&audio, None, None, true)
            .unwrap_err();
        assert!(
            err.to_string().contains("Translation is not supported"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    #[ignore = "requires parakeet-v3 model files"]
    fn test_parakeet_load_and_transcribe() {
        let _ = tracing_subscriber::fmt::try_init();

        let config = TranscriptionConfig {
            backend: TranscriptionBackend::ParakeetV3,
            model: "parakeet-v3".to_string(),
            ..Default::default()
        };

        let paths = Paths::new().expect("failed to initialize paths");
        let mut backend = ParakeetBackend::new(&config);
        backend
            .initialize(&paths)
            .expect("failed to initialize Parakeet backend");

        use std::f32::consts::PI;
        let audio: Vec<f32> = (0..16000)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / 16000.0).sin())
            .collect();
        let text = backend
            .transcribe_with_options(&audio, None, None, false)
            .expect("transcription failed");
        println!("Parakeet transcription: {}", text);
    }
}
