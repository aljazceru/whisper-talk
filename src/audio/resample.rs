#![allow(dead_code)]
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
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
    ResampleError(String),
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

    let resample_ratio = output_rate as f64 / input_rate as f64;

    // Use sinc interpolation for high quality resampling
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    // Calculate chunk size based on input length
    let chunk_size = input_data.len().min(1024).max(64);

    let mut resampler = SincFixedIn::<f32>::new(
        resample_ratio,
        2.0, // max relative ratio change
        params,
        chunk_size,
        1, // mono channel
    )
    .map_err(|e| ResampleError::ResampleError(e.to_string()))?;

    // Process in chunks
    let mut output = Vec::new();
    let mut pos = 0;

    while pos < input_data.len() {
        let end = (pos + chunk_size).min(input_data.len());
        let chunk = &input_data[pos..end];

        // Pad chunk if needed
        let input_chunk = if chunk.len() < chunk_size {
            let mut padded = chunk.to_vec();
            padded.resize(chunk_size, 0.0);
            padded
        } else {
            chunk.to_vec()
        };

        let waves_in = vec![input_chunk];

        match resampler.process(&waves_in, None) {
            Ok(waves_out) => {
                if !waves_out.is_empty() {
                    output.extend_from_slice(&waves_out[0]);
                }
            }
            Err(e) => return Err(ResampleError::ResampleError(e.to_string())),
        }

        pos += chunk_size;
    }

    // Trim to expected output length
    let expected_len = ((input_data.len() as f64) * resample_ratio).ceil() as usize;
    output.truncate(expected_len);

    Ok(output)
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
}
