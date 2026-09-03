use std::io::Cursor;
use std::net::SocketAddr;

use crate::{
    app::Application,
    audio::resample::resample_to_16khz,
    error::{Result, WhisperTalkError},
};
use axum::{
    extract::{Multipart, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hound::SampleFormat;
use serde_json::json;
use std::str::FromStr;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::{get_codecs, get_probe};
use tokio::sync::oneshot;
use tracing::{error, info};

#[derive(Clone)]
struct ApiState {
    app: Application,
}

enum ResponseFormat {
    Json,
    Text,
    VerboseJson,
    Srt,
    Vtt,
}

struct TranscriptionRequest {
    file: Vec<u8>,
    model: String,
    language: Option<String>,
    prompt: Option<String>,
    response_format: ResponseFormat,
}

pub async fn spawn_api_server(
    app: Application,
    bind_addr: SocketAddr,
    shutdown: oneshot::Receiver<()>,
) -> Result<tokio::task::JoinHandle<()>> {
    let state = ApiState { app };
    let app = Router::new()
        .route("/v1/audio/transcriptions", post(transcriptions))
        .route("/v1/audio/translations", post(translations))
        .route("/v1/models", get(models))
        .route("/meetscribe/transcribe", post(meetscribe_transcribe))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!("OpenAI-compatible API listening on {}", bind_addr);

    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown.await;
        });

        if let Err(error) = server.await {
            error!("OpenAI-compatible API server failed: {}", error);
        }
    });

    Ok(task)
}

async fn models() -> Json<serde_json::Value> {
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "whisper-1",
                "object": "model",
                "created": 0,
                "owned_by": "whisper-talk"
            }
        ]
    }))
}

async fn transcriptions(State(state): State<ApiState>, multipart: Multipart) -> Response {
    handle_transcription(state, multipart, false).await
}

async fn translations(State(state): State<ApiState>, multipart: Multipart) -> Response {
    handle_transcription(state, multipart, true).await
}

async fn handle_transcription(state: ApiState, multipart: Multipart, translate: bool) -> Response {
    let request = match parse_transcription_request(multipart).await {
        Ok(value) => value,
        Err((status, message, param)) => return error_response(status, &message, param.as_deref()),
    };
    let request_language = request.language.clone();
    let request_prompt = request.prompt.clone();

    let audio_data = match decode_audio_to_16khz(&request.file) {
        Ok(value) => value,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, &message, Some("file")),
    };

    if audio_data.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "No audio samples in uploaded file",
            Some("file"),
        );
    }

    let text = match state
        .app
        .transcribe_file(audio_data, request_language, request_prompt, translate)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let status = match error {
                WhisperTalkError::UnsupportedOperation(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            return error_response(status, &format!("Transcription failed: {}", error), None);
        }
    };

    match request.response_format {
        ResponseFormat::Text => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))],
            text,
        )
            .into_response(),
        ResponseFormat::VerboseJson => {
            let payload = json!({
                "text": text,
                "duration": 0.0,
                "language": request
                    .language
                    .unwrap_or_else(|| "en".to_string()),
                "model": request.model,
                "segments": []
            });
            (StatusCode::OK, Json(payload)).into_response()
        }
        ResponseFormat::Srt => {
            let srt_text = if text.is_empty() {
                String::new()
            } else {
                format!("1\n00:00:00,000 --> 00:00:00,000\n{}", text)
            };
            (
                StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                )],
                srt_text,
            )
                .into_response()
        }
        ResponseFormat::Vtt => {
            let vtt_text = if text.is_empty() {
                "WEBVTT\n".to_string()
            } else {
                format!("WEBVTT\n\n00:00:00.000 --> 00:00:00.000\n{}", text)
            };
            (
                StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                )],
                vtt_text,
            )
                .into_response()
        }
        ResponseFormat::Json => (StatusCode::OK, Json(json!({ "text": text }))).into_response(),
    }
}

fn error_response(status: StatusCode, message: &str, param: Option<&str>) -> Response {
    let error_type = if status.is_server_error() {
        "server_error"
    } else {
        "invalid_request_error"
    };

    let error_code = if status.is_server_error() {
        serde_json::Value::Null
    } else {
        json!("invalid_request_error")
    };

    let payload = json!({
        "error": {
            "message": message,
            "type": error_type,
            "param": param,
            "code": error_code,
        },
    });
    (status, Json(payload)).into_response()
}

async fn parse_transcription_request(
    mut multipart: Multipart,
) -> std::result::Result<TranscriptionRequest, (StatusCode, String, Option<String>)> {
    let mut file = None;
    let mut model = None;
    let mut language = None;
    let mut prompt = None;
    let mut response_format = ResponseFormat::Json;

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            error.to_string(),
            Some("file".to_string()),
        )
    })? {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "file" {
            file = Some(
                field
                    .bytes()
                    .await
                    .map_err(|error| {
                        (
                            StatusCode::BAD_REQUEST,
                            error.to_string(),
                            Some("file".to_string()),
                        )
                    })?
                    .to_vec(),
            );
            continue;
        }

        match field_name.as_str() {
            "model" => {
                let value = field.text().await.map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        error.to_string(),
                        Some("model".to_string()),
                    )
                })?;
                model = Some(value);
            }
            "language" => {
                let value = field.text().await.map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        error.to_string(),
                        Some("language".to_string()),
                    )
                })?;
                language = Some(value);
            }
            "prompt" => {
                let value = field.text().await.map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        error.to_string(),
                        Some("prompt".to_string()),
                    )
                })?;
                prompt = Some(value);
            }
            "response_format" => {
                let value = field.text().await.map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        error.to_string(),
                        Some("response_format".to_string()),
                    )
                })?;
                response_format = ResponseFormat::from_str(value.trim()).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Unsupported response_format: {}", value),
                        Some("response_format".to_string()),
                    )
                })?;
            }
            // OpenAI clients often send this for compatibility. We keep it for acceptance only.
            "temperature" => {
                let value = field.text().await.map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        error.to_string(),
                        Some("temperature".to_string()),
                    )
                })?;
                if let Err(error) = value.parse::<f32>() {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Invalid temperature value: {}", error),
                        Some("temperature".to_string()),
                    ));
                }
            }
            _ => {}
        }
    }

    let file = file.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing required multipart field: file".to_string(),
            Some("file".to_string()),
        )
    })?;

    let model = model.unwrap_or_else(|| "whisper-1".to_string());

    Ok(TranscriptionRequest {
        file,
        model,
        language: language.filter(|value| !value.is_empty()),
        prompt: prompt.filter(|value| !value.is_empty()),
        response_format,
    })
}

fn decode_audio_to_16khz(file: &[u8]) -> std::result::Result<Vec<f32>, String> {
    if let Ok((samples, sample_rate, channels)) = decode_wav_with_hound(file) {
        return resample_to_16khz(&samples, sample_rate, channels)
            .map_err(|error| format!("Failed resampling audio to 16 kHz: {}", error));
    }

    let (samples, sample_rate, channels) = decode_with_symphonia(file)?;
    resample_to_16khz(&samples, sample_rate, channels)
        .map_err(|error| format!("Failed resampling audio to 16 kHz: {}", error))
}

impl FromStr for ResponseFormat {
    type Err = ();

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "text" => Ok(Self::Text),
            "verbose_json" => Ok(Self::VerboseJson),
            "srt" => Ok(Self::Srt),
            "vtt" => Ok(Self::Vtt),
            _ => Err(()),
        }
    }
}

fn decode_with_symphonia(file: &[u8]) -> std::result::Result<(Vec<f32>, u32, u16), String> {
    let cursor = Cursor::new(file.to_vec());
    let source = Box::new(cursor);
    let mss = MediaSourceStream::new(source, Default::default());
    let hint = Hint::new();

    let mut format = get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("Unsupported audio format: {}", error))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "No readable audio track found in file".to_string())?;

    let track_id = track.id;
    let audio_params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(params)) => params,
        Some(_) => return Err("Track is not an audio track".to_string()),
        None => return Err("Audio codec parameters are missing".to_string()),
    };

    let sample_rate = audio_params
        .sample_rate
        .ok_or_else(|| "Audio sample rate is missing".to_string())?;
    let channels = audio_params
        .channels
        .as_ref()
        .map(|channels| channels.count() as u16)
        .unwrap_or(1);

    let registered_decoder = get_codecs()
        .get_audio_decoder(audio_params.codec)
        .ok_or_else(|| format!("Unsupported audio codec: {:?}", audio_params.codec))?;

    let mut decoder = (registered_decoder.factory)(audio_params, &AudioDecoderOptions::default())
        .map_err(|error| format!("Failed to initialize decoder: {}", error))?;

    let mut output: Vec<f32> = Vec::new();
    let output_channels = if channels > 2 { 1 } else { channels };
    loop {
        let packet = match format.next_packet() {
            // Ok(None) signals a clean end-of-stream in symphonia 0.6.
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(error) => return Err(format!("Failed to decode packets: {}", error)),
        };

        if packet.track_id != track_id {
            continue;
        }

        let packet_data = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(error) => return Err(format!("Failed to decode packet: {}", error)),
        };

        let decoded_sample_count = packet_data.frames();
        if decoded_sample_count == 0 {
            continue;
        }

        let spec = packet_data.spec();
        let spec_channels = spec.channels().count();
        // copy_to_vec_interleaved resizes (overwrites) its destination, so
        // decode into a per-packet scratch buffer first.
        let mut interleaved: Vec<f32> = Vec::with_capacity(decoded_sample_count * spec_channels);
        packet_data.copy_to_vec_interleaved(&mut interleaved);

        if channels == 1 {
            output.extend_from_slice(&interleaved);
            continue;
        }

        let channels_usize = channels as usize;
        let mut index = 0usize;
        while index + channels_usize <= interleaved.len() {
            let frame_end = index + channels_usize;
            let mut avg = 0.0f32;
            for sample in &interleaved[index..frame_end] {
                avg += *sample;
            }
            output.push(avg / channels_usize as f32);
            index = frame_end;
        }
    }

    if output.is_empty() {
        return Err("No audio samples decoded from file".to_string());
    }

    Ok((output, sample_rate, output_channels))
}

async fn meetscribe_transcribe(State(state): State<ApiState>, multipart: Multipart) -> Response {
    // Parse: accepts same multipart as /v1/audio/transcriptions (file, language, prompt)
    let request = match parse_transcription_request(multipart).await {
        Ok(value) => value,
        Err((status, message, param)) => return error_response(status, &message, param.as_deref()),
    };

    let audio_data = match decode_audio_to_16khz(&request.file) {
        Ok(value) => value,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, &message, Some("file")),
    };

    if audio_data.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "No audio samples in uploaded file",
            Some("file"),
        );
    }

    let duration_secs = audio_data.len() as f64 / 16000.0;
    let language = request.language.clone();
    let prompt = request.prompt.clone();

    let segments = match state
        .app
        .transcribe_file_segments(audio_data, language.clone(), prompt)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Transcription failed: {}", error),
                None,
            )
        }
    };

    let payload = json!({
        "segments": segments,
        "language": language.unwrap_or_else(|| "en".to_string()),
        "duration": duration_secs,
    });

    (StatusCode::OK, Json(payload)).into_response()
}

fn decode_wav_with_hound(file: &[u8]) -> std::result::Result<(Vec<f32>, u32, u16), String> {
    let mut reader = hound::WavReader::new(Cursor::new(file))
        .map_err(|error| format!("Invalid WAV input: {}", error))?;
    let spec = reader.spec();

    let samples = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed reading WAV samples: {}", error))?,
        (SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|sample| {
                sample.map(|value| {
                    // Convert i16 range [-32768, 32767] to f32 [-1.0, 1.0).
                    value as f32 / (i16::MAX as f32)
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed reading WAV samples: {}", error))?,
        (SampleFormat::Int, bits @ (24 | 32)) => reader
            .samples::<i32>()
            .map(|sample| {
                sample.map(|value| {
                    let denominator = (1i64 << (bits as u32 - 1)) as f32;
                    value as f32 / denominator
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed reading WAV samples: {}", error))?,
        _ => {
            return Err(format!(
            "Unsupported WAV format: {:?}, {} bits. Expected WAV with Int24/32 or Float32 samples",
            spec.sample_format, spec.bits_per_sample
        ))
        }
    };

    let input_channels = if spec.channels > 2 { 1 } else { spec.channels };
    let samples = if spec.channels > 2 {
        let channels = spec.channels as usize;
        samples
            .chunks_exact(channels)
            .map(|frame| {
                let sum: f32 = frame.iter().sum();
                sum / channels as f32
            })
            .collect::<Vec<_>>()
    } else {
        samples
    };

    Ok((samples, spec.sample_rate, input_channels))
}

#[cfg(test)]
mod tests {
    use super::{decode_wav_with_hound, decode_with_symphonia};

    #[test]
    fn decode_symphonia_tiny_wav_is_nonempty() {
        let sample_rate = 16_000u32;
        let duration_samples = sample_rate as usize;
        let mut file = std::io::Cursor::new(Vec::<u8>::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        {
            let mut writer = hound::WavWriter::new(&mut file, spec).unwrap();
            for i in 0..duration_samples {
                let value = ((i as f32 / sample_rate as f32 * 440.0 * std::f32::consts::TAU).sin()
                    * i16::MAX as f32) as i16;
                writer.write_sample(value).unwrap();
            }
            writer.finalize().unwrap();
        }

        let encoded = file.into_inner();
        let (sym_samples, sym_rate, sym_channels) = decode_with_symphonia(&encoded).unwrap();

        assert_eq!(sym_rate, sample_rate);
        assert_eq!(sym_channels, 1);
        assert!(!sym_samples.is_empty());

        let (wav_samples, wav_rate, wav_channels) = decode_wav_with_hound(&encoded).unwrap();
        assert_eq!(sym_rate, wav_rate);
        assert_eq!(sym_channels, wav_channels);
        assert!(!wav_samples.is_empty());
    }
}
