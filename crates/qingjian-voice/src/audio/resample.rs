//! 把设备 PCM 转成 SenseVoice 需要的 16 kHz 单声道。

use rubato::{FftFixedIn, Resampler};

use crate::VoiceError;

pub(crate) fn to_mono_16k(
    samples: &[f32],
    source_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, VoiceError> {
    let mono = if channels == 1 {
        samples.to_vec()
    } else {
        let channels = channels as usize;
        samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    if source_rate == 16_000 {
        return Ok(mono);
    }
    if mono.is_empty() {
        return Ok(mono);
    }
    let chunk_size = 1024;
    let mut resampler = FftFixedIn::<f32>::new(source_rate as usize, 16_000, chunk_size, 2, 1)
        .map_err(|error| VoiceError::Resample(error.to_string()))?;
    let mut output = Vec::new();
    for chunk in mono.chunks(chunk_size) {
        let mut padded = chunk.to_vec();
        padded.resize(chunk_size, 0.0);
        let resampled = resampler
            .process(&[padded], None)
            .map_err(|error| VoiceError::Resample(error.to_string()))?;
        if let Some(channel) = resampled.first() {
            output.extend_from_slice(channel);
        }
    }
    let flushed = resampler
        .process_partial::<Vec<f32>>(None, None)
        .map_err(|error| VoiceError::Resample(error.to_string()))?;
    if let Some(channel) = flushed.first() {
        output.extend_from_slice(channel);
    }
    Ok(output)
}
