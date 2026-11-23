//! Plays the hearing test's tones. Nothing else in the app makes sound.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eq_core::hearing::{self, Tone};
use std::time::Duration;

/// Plays a tone until it ends or `Player` is dropped or replaced.
#[derive(Default)]
pub struct Player {
    stream: Option<cpal::Stream>,
}

impl Player {
    /// Returns how long the tone lasts.
    pub fn play(&mut self, tone: Tone) -> Result<Duration, String> {
        self.stream = None;
        let device = cpal::default_host().default_output_device().ok_or("No output device was found.")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config = supported.config();
        let rate = f64::from(config.sample_rate);
        let samples = hearing::render_tone(tone, rate);
        let length = Duration::from_secs_f64(samples.len() as f64 / 2.0 / rate);
        let stream = match format {
            cpal::SampleFormat::F32 => open::<f32>(&device, config, samples),
            cpal::SampleFormat::I16 => open::<i16>(&device, config, samples),
            cpal::SampleFormat::I32 => open::<i32>(&device, config, samples),
            cpal::SampleFormat::U16 => open::<u16>(&device, config, samples),
            _ => Err("This output device uses a format the test cannot play.".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(length)
    }

    pub fn stop(&mut self) {
        self.stream = None;
    }
}

fn open<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Vec<f32>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    let mut at = 0;
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let l = samples.get(at).copied().unwrap_or(0.0);
                    let r = samples.get(at + 1).copied().unwrap_or(0.0);
                    at = (at + 2).min(samples.len());
                    for (i, out) in frame.iter_mut().enumerate() {
                        let v = match (i, channels) {
                            (0, 1) => l + r,
                            (0, _) => l,
                            (1, _) => r,
                            _ => 0.0,
                        };
                        *out = T::from_sample(v);
                    }
                }
            },
            |_| {},
            None,
        )
        .map_err(|e| e.to_string())
}
