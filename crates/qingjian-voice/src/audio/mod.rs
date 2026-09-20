//! Windows 麦克风按需采集。

mod opened;
mod preview;
mod resample;

use std::sync::mpsc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::VoiceError;

pub(crate) use opened::OpenedInput;
pub(crate) use preview::LivePreview;
pub(crate) use resample::to_mono_16k;

pub(crate) fn open_input(
    preferred: Option<&str>,
    sender: mpsc::SyncSender<Vec<f32>>,
) -> Result<OpenedInput, VoiceError> {
    let host = cpal::default_host();
    let device = preferred
        .filter(|name| !name.trim().is_empty())
        .and_then(|preferred| {
            host.input_devices()
                .ok()?
                .find(|device| device.name().is_ok_and(|candidate| candidate == preferred))
        })
        .or_else(|| host.default_input_device())
        .ok_or_else(|| VoiceError::Audio("no input device found".into()))?;
    let name = device.name().unwrap_or_else(|_| "<unknown>".into());
    let supported = device
        .default_input_config()
        .map_err(|error| VoiceError::Audio(error.to_string()))?;
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let config: cpal::StreamConfig = supported.into();
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                let _ = sender.try_send(data.to_vec());
            },
            |error| tracing::error!(%error, "语音输入流出错"),
            None,
        )
        .map_err(|error| VoiceError::Audio(error.to_string()))?;
    stream
        .play()
        .map_err(|error| VoiceError::Audio(error.to_string()))?;
    Ok(OpenedInput {
        stream,
        sample_rate,
        channels,
        name,
    })
}

pub(crate) fn is_meaningful(text: &str) -> bool {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .count()
        >= 2
}

/// 把一块交错采样合成单声道后算 RMS。
pub(crate) fn rms_energy(samples: &[f32], channels: u16) -> f32 {
    let channels = usize::from(channels.max(1));
    let frames = samples.len() / channels;
    if frames == 0 {
        return 0.0;
    }
    let sum_sq = (0..frames)
        .map(|frame| {
            let mono = (0..channels)
                .map(|channel| samples[frame * channels + channel])
                .sum::<f32>()
                / channels as f32;
            mono * mono
        })
        .sum::<f32>();
    (sum_sq / frames as f32).sqrt()
}

/// RMS 映射成 UI 需要的稳定整数电平；低于环境底噪时归零。
pub(crate) fn visual_level(energy: f32) -> u16 {
    const FLOOR: f32 = 0.002;
    const CEILING: f32 = 0.18;
    (((energy - FLOOR) / (CEILING - FLOOR)).clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::{rms_energy, visual_level};

    #[test]
    fn stereo_energy_is_measured_after_downmix() {
        assert_eq!(rms_energy(&[0.5, 0.5, -0.5, -0.5], 2), 0.5);
        assert_eq!(rms_energy(&[], 2), 0.0);
    }

    #[test]
    fn visual_level_is_bounded() {
        assert_eq!(visual_level(0.0), 0);
        assert_eq!(visual_level(1.0), 1000);
    }
}
