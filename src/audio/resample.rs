#![allow(dead_code)]
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Fft, FixedSync, Resampler};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResampleError {
    #[error("Input sample rate must be greater than 0")]
    InvalidSampleRate,
    #[error("Input channels must be 1 or 2")]
    InvalidChannels,
    #[error("Input audio data is empty")]
    EmptyAudio,
    #[error("Resampling error: {0}")]
    Other(String),
}

pub fn resample_to_16khz(
    audio_data: &[f32],
    input_sample_rate: u32,
    input_channels: u16,
) -> Result<Vec<f32>, ResampleError> {
    if audio_data.is_empty() {
        return Ok(Vec::new());
    }

    if input_sample_rate == 0 {
        return Err(ResampleError::InvalidSampleRate);
    }

    if !(1..=2).contains(&input_channels) {
        return Err(ResampleError::InvalidChannels);
    }

    let output_sample_rate = 16000;

    let mono_data = match input_channels {
        1 => {
            if input_sample_rate == output_sample_rate {
                return Ok(audio_data.to_vec());
            }
            audio_data.to_vec()
        }
        2 => stereo_to_mono(audio_data),
        _ => unreachable!(),
    };

    if input_sample_rate == output_sample_rate {
        return Ok(mono_data);
    }

    resample_with_rubato(&mono_data, input_sample_rate, output_sample_rate)
}

fn stereo_to_mono(stereo_data: &[f32]) -> Vec<f32> {
    let samples = stereo_data.len() / 2;
    let mut mono = Vec::with_capacity(samples);

    for i in 0..samples {
        let left = stereo_data[i * 2];
        let right = stereo_data[i * 2 + 1];
        mono.push((left + right) * 0.5);
    }

    mono
}

fn resample_with_rubato(
    input_data: &[f32],
    input_rate: u32,
    output_rate: u32,
) -> Result<Vec<f32>, ResampleError> {
    if input_data.is_empty() {
        return Ok(Vec::new());
    }

    let frames = input_data.len(); // mono input
    let chunk_size = frames.clamp(64, 1024);

    // FFT-based synchronous resampler. Both sides fixed: chunk_size is a hint,
    // actual block sizes are rounded to fit the exact rate ratio.
    let mut resampler = Fft::<f32>::new(
        input_rate as usize,
        output_rate as usize,
        chunk_size,
        1, // mono
        FixedSync::Both,
    )
    .map_err(|e| ResampleError::Other(e.to_string()))?;

    let input = InterleavedSlice::<&[f32]>::new(input_data, 1, frames)
        .map_err(|e| ResampleError::Other(e.to_string()))?;

    // process_all runs the chunk loop, trims the resampler startup delay, and
    // returns exactly the resampled frames.
    let output: InterleavedOwned<f32> = resampler
        .process_all(&input, frames, None)
        .map_err(|e| ResampleError::Other(e.to_string()))?;

    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_audio() {
        let result = resample_to_16khz(&[], 48000, 2);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_mono_16khz_no_resample() {
        let audio = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let result = resample_to_16khz(&audio, 16000, 1).unwrap();
        assert_eq!(result, audio);
    }

    #[test]
    fn test_invalid_sample_rate() {
        let audio = vec![0.1, 0.2, 0.3];
        let result = resample_to_16khz(&audio, 0, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_channels() {
        let audio = vec![0.1, 0.2, 0.3];
        let result = resample_to_16khz(&audio, 48000, 0);
        assert!(result.is_err());

        let result = resample_to_16khz(&audio, 48000, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_stereo_to_mono() {
        let stereo = vec![1.0, 0.5, 0.4, 0.6, 0.8, 0.2];
        let result = resample_to_16khz(&stereo, 16000, 2).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0.75);
        assert_eq!(result[1], 0.5);
        assert_eq!(result[2], 0.5);
    }

    #[test]
    fn test_resample_48k_to_16k_length_and_content() {
        // 1 second of 48 kHz DC-offset audio resamples to ~1 second of 16 kHz.
        let input: Vec<f32> = vec![0.5; 48000];
        let result = resample_to_16khz(&input, 48000, 1).unwrap();
        assert_eq!(result.len(), 16000);
        // The first few samples carry the anti-aliasing filter's startup
        // transient; the bulk must preserve the DC level.
        let bulk = &result[32..];
        let peak_deviation = bulk
            .iter()
            .map(|s| (s - 0.5).abs())
            .fold(0.0f32, f32::max);
        assert!(
            peak_deviation < 0.01,
            "DC level drifted by {} after resampling",
            peak_deviation
        );
    }

    #[test]
    fn test_resample_44100_to_16k_odd_ratio() {
        // 44.1 kHz -> 16 kHz is a 160/441 ratio; exercises non-integer ratios.
        let input: Vec<f32> = vec![0.25; 44100];
        let result = resample_to_16khz(&input, 44100, 1).unwrap();
        assert_eq!(result.len(), 16000);
    }
}
