use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::VecDeque;
use std::time::Duration;

#[cfg(target_os = "windows")]
use windows::{
    Win32::Media::Audio::{
        IAudioClient, IAudioCaptureClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        eRender, eConsole, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    },
    Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, CLSCTX_ALL, CoCreateInstance},
};

use tract_onnx::prelude::*;
use ndarray::Array2;

pub struct AudioClassifier {
    voice_prob: Arc<Mutex<f32>>,
}

impl AudioClassifier {
    pub fn new(model_path: &str) -> anyhow::Result<Self> {
        let model = tract_onnx::onnx()
            .model_for_path(model_path)?
            .into_optimized()?
            .into_runnable()?;

        let voice_prob = Arc::new(Mutex::new(0.0f32));
        let vp = voice_prob.clone();

        #[cfg(target_os = "windows")]
        thread::spawn(move || {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok(); }

            let enumerator: IMMDeviceEnumerator = unsafe {
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(e) => e,
                    Err(_) => return,
                }
            };
            let device = unsafe {
                match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    Ok(d) => d,
                    Err(_) => return,
                }
            };
            let client: IAudioClient = unsafe {
                match device.Activate(CLSCTX_ALL, None) {
                    Ok(c) => c,
                    Err(_) => return,
                }
            };

            let pwfx = unsafe {
                match client.GetMixFormat() {
                    Ok(p) => p,
                    Err(_) => return,
                }
            };
            let wfx = unsafe { &*pwfx };
            let sample_rate = wfx.nSamplesPerSec;
            let channels = wfx.nChannels;

            unsafe {
                if client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    0, 0, pwfx, None,
                ).is_err() { return; }
            }

            let capture_client: IAudioCaptureClient = unsafe {
                match client.GetService() {
                    Ok(c) => c,
                    Err(_) => return,
                }
            };
            unsafe {
                if client.Start().is_err() { return; }
            }

            let target_samples = 16000usize;
            let mut audio_buffer = VecDeque::<f32>::with_capacity(target_samples * 2);
            let downsample_factor = (sample_rate / 16000).max(1) as usize;

            loop {
                let packet_length = unsafe {
                    match capture_client.GetNextPacketSize() {
                        Ok(n) => n,
                        Err(_) => 0,
                    }
                };
                if packet_length > 0 {
                    let mut data = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    unsafe {
                        capture_client.GetBuffer(&mut data, &mut frames, &mut flags, None, None).ok();
                    }
                    if frames > 0 {
                        let slice = unsafe {
                            std::slice::from_raw_parts(data as *const f32, frames as usize * channels as usize)
                        };
                        for frame in slice.chunks(channels as usize).step_by(downsample_factor) {
                            let mono = frame.iter().sum::<f32>() / channels as f32;
                            audio_buffer.push_back(mono);
                        }
                    }
                    unsafe {
                        capture_client.ReleaseBuffer(frames).ok();
                    }
                }

                while audio_buffer.len() > target_samples {
                    audio_buffer.pop_front();
                }

                if audio_buffer.len() == target_samples {
                    let samples: Vec<f32> = audio_buffer.iter().cloned().collect();
                    let mel = compute_mel_spectrogram(&samples, 16000, 400, 160, 64);
                    let flat: Vec<f32> = mel.iter().cloned().collect();

                    if let Ok(tensor) = Tensor::from_shape(&[1, 1, 64, 100], &flat) {
                        match model.run(tvec!(tensor.into())) {
                            Ok(result) => {
                                if let Ok(view) = result[0].to_array_view::<f32>() {
                                    let speech_logit = view[[0, 0]];
                                    let music_logit = view[[0, 1]];
                                    let speech_prob = sigmoid(speech_logit - music_logit);
                                    *vp.lock().unwrap() = speech_prob;
                                }
                            }
                            Err(_) => {}
                        }
                    }
                }

                thread::sleep(Duration::from_millis(10));
            }
        });

        Ok(Self { voice_prob })
    }

    pub fn get_voice_prob(&self) -> f32 {
        *self.voice_prob.lock().unwrap()
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

fn mel_filterbank(n_mels: usize, n_fft: usize, sample_rate: usize, f_min: f32, f_max: f32) -> Array2<f32> {
    let mut filters = Array2::zeros((n_mels, n_fft / 2 + 1));
    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);
    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * (i as f32) / ((n_mels + 1) as f32))
        .collect();
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();
    let bin_points: Vec<usize> = hz_points
        .iter()
        .map(|&f| ((n_fft as f32 + 1.0) * f / sample_rate as f32).floor() as usize)
        .collect();

    for i in 0..n_mels {
        let f_m_minus = bin_points[i];
        let f_m = bin_points[i + 1];
        let f_m_plus = bin_points[i + 2];

        for j in f_m_minus..f_m.min(n_fft / 2 + 1) {
            let denom = (f_m - f_m_minus).max(1) as f32;
            filters[[i, j]] = (j - f_m_minus) as f32 / denom;
        }
        for j in f_m..f_m_plus.min(n_fft / 2 + 1) {
            let denom = (f_m_plus - f_m).max(1) as f32;
            filters[[i, j]] = (f_m_plus - j) as f32 / denom;
        }
    }
    filters
}

fn compute_mel_spectrogram(samples: &[f32], sample_rate: usize, n_fft: usize, hop_length: usize, n_mels: usize) -> Array2<f32> {
    use rustfft::{FftPlanner, num_complex::Complex};

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);

    let mut spec = Vec::new();
    let window: Vec<f32> = (0..n_fft)
        .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (n_fft - 1) as f32).cos())
        .collect();

    for frame_start in (0..samples.len().saturating_sub(n_fft)).step_by(hop_length) {
        let mut buffer: Vec<Complex<f32>> = (0..n_fft)
            .map(|i| {
                let s = if frame_start + i < samples.len() { samples[frame_start + i] } else { 0.0 };
                Complex::new(s * window[i], 0.0)
            })
            .collect();
        fft.process(&mut buffer);
        let mags: Vec<f32> = buffer.iter().take(n_fft / 2 + 1).map(|c| c.norm()).collect();
        spec.push(mags);
    }

    let n_frames = spec.len();
    let mut spec_arr = Array2::zeros((n_fft / 2 + 1, n_frames));
    for (t, frame) in spec.iter().enumerate() {
        for (f, &mag) in frame.iter().enumerate() {
            spec_arr[[f, t]] = mag;
        }
    }

    let mel_fb = mel_filterbank(n_mels, n_fft, sample_rate, 0.0, (sample_rate / 2) as f32);
    let mel_spec = mel_fb.dot(&spec_arr);

    let log_mel = mel_spec.mapv(|v| (v + 1e-10).ln());
    let max = log_mel.iter().fold(0.0f32, |a, &b| a.max(b));
    let min = log_mel.iter().fold(max, |a, &b| a.min(b));
    let range = max - min;
    if range > 0.0 {
        log_mel.mapv(|v| (v - min) / range)
    } else {
        log_mel
    }
}