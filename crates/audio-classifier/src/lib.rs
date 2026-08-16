use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
use windows::{
    core::ComInterface, // Замінено Interface на ComInterface
    Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    },
    Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
};

use audio_core::SoundType;
use rustfft::{num_complex::Complex, FftPlanner};

#[derive(Debug, Clone, Copy)]
pub struct ClassificationResult {
    pub primary: audio_core::SoundType,
    pub secondary: audio_core::SoundType,
    pub primary_votes: usize,
    pub secondary_votes: usize,
}

#[derive(Debug, Clone, Default)]
struct Features {
    energy: f32,
    zcr: f32,
    centroid: f32,
    flux: f32,
    rolloff: f32,
}

pub struct AudioClassifier {
    result: Arc<Mutex<ClassificationResult>>,
}

impl AudioClassifier {
    pub fn new(_model_path: &str) -> anyhow::Result<Self> {
        let result = Arc::new(Mutex::new(ClassificationResult {
            primary: SoundType::None,
            secondary: SoundType::None,
            primary_votes: 0,
            secondary_votes: 0,
        }));
        let st = result.clone();

        #[cfg(target_os = "windows")]
        thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }

            let enumerator: IMMDeviceEnumerator = match unsafe {
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            } {
                Ok(e) => e,
                Err(_) => return,
            };

            let device = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
                Ok(d) => d,
                Err(_) => return,
            };

            // Працює завдяки перебуванню `ComInterface` у скоупі
            let client: IAudioClient = match device.cast() {
                Ok(c) => c,
                Err(_) => return,
            };

            let pwfx = match unsafe { client.GetMixFormat() } {
                Ok(p) => p,
                Err(_) => return,
            };
            let wfx = unsafe { &*pwfx };
            let sample_rate = wfx.nSamplesPerSec;
            let channels = wfx.nChannels;

            unsafe {
                if client
                    .Initialize(
                        AUDCLNT_SHAREMODE_SHARED,
                        AUDCLNT_STREAMFLAGS_LOOPBACK,
                        0,
                        0,
                        pwfx,
                        None,
                    )
                    .is_err()
                {
                    return;
                }
            }

            let capture_client: IAudioCaptureClient = match unsafe { client.GetService() } {
                Ok(c) => c,
                Err(_) => return,
            };
            unsafe {
                let _ = client.Start();
            }

            let target_rate = 16000usize;
            let downsample = (sample_rate as usize / target_rate).max(1);
            let frame_size = 512;
            let hop_size = 256;

            let mut audio_buffer = VecDeque::<f32>::new();
            let mut prev_spectrum: Option<Vec<f32>> = None;
            let mut history = VecDeque::<SoundType>::with_capacity(5);
            let mut planner = FftPlanner::<f32>::new();
            let fft = planner.plan_fft_forward(frame_size);

            loop {
                let packet = unsafe {
                    match capture_client.GetNextPacketSize() {
                        Ok(n) => n,
                        Err(_) => 0,
                    }
                };

                if packet > 0 {
                    let mut data = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    unsafe {
                        capture_client
                            .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                            .ok();
                    }
                    if frames > 0 {
                        let slice = unsafe {
                            std::slice::from_raw_parts(
                                data as *const f32,
                                frames as usize * channels as usize,
                            )
                        };
                        for frame in slice.chunks(channels as usize).step_by(downsample) {
                            let mono = frame.iter().sum::<f32>() / channels as f32;
                            audio_buffer.push_back(mono);
                        }
                    }
                    unsafe {
                        capture_client.ReleaseBuffer(frames).ok();
                    }
                }

                while audio_buffer.len() >= frame_size {
                    let frame: Vec<f32> =
                        audio_buffer.iter().take(frame_size).cloned().collect();
                    for _ in 0..hop_size {
                        audio_buffer.pop_front();
                    }

                    let features =
                        extract_features(&frame, &*fft, frame_size, &mut prev_spectrum);
                    let classified = classify_frame(&features);

                    history.push_back(classified);
                    if history.len() > 5 {
                        history.pop_front();
                    }
                    let smoothed = majority_vote(&history);

                    *st.lock().unwrap() = smoothed;
                }

                thread::sleep(Duration::from_millis(5));
            }
        });

        Ok(Self { result })
    }

    pub fn get_result(&self) -> ClassificationResult {
        *self.result.lock().unwrap()
    }
}

fn extract_features(
    frame: &[f32],
    fft: &dyn rustfft::Fft<f32>,
    frame_size: usize,
    prev_spectrum: &mut Option<Vec<f32>>,
) -> Features {
    let windowed: Vec<f32> = frame
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5
                - 0.5
                    * (2.0 * std::f32::consts::PI * i as f32 / (frame_size - 1) as f32).cos();
            s * w
        })
        .collect();

    let mut buf: Vec<Complex<f32>> = windowed.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let mags: Vec<f32> = buf.iter().take(frame_size / 2 + 1).map(|c| c.norm()).collect();
    let sum_mag: f32 = mags.iter().sum();

    let energy = (frame.iter().map(|&s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    let zcr = frame.windows(2).filter(|w| w[0] * w[1] < 0.0).count() as f32 / frame.len() as f32;

    let centroid = if sum_mag > 0.0 {
        mags.iter().enumerate().map(|(i, &m)| i as f32 * m).sum::<f32>() / sum_mag
            * 16000.0
            / frame_size as f32
    } else {
        0.0
    };

    let rolloff = if sum_mag > 0.0 {
        let thr = sum_mag * 0.85;
        let mut cs = 0.0;
        let mut idx = 0.0;
        for (i, &m) in mags.iter().enumerate() {
            cs += m;
            if cs >= thr {
                idx = i as f32;
                break;
            }
        }
        idx * 16000.0 / frame_size as f32
    } else {
        0.0
    };

    let flux = if let Some(prev) = prev_spectrum.as_ref() {
        mags.iter()
            .zip(prev.iter())
            .map(|(&c, &p)| (c - p).max(0.0).powi(2))
            .sum::<f32>()
            .sqrt()
    } else {
        0.0
    };

    *prev_spectrum = Some(mags);

    Features {
        energy,
        zcr,
        centroid,
        flux,
        rolloff,
    }
}

fn classify_frame(f: &Features) -> SoundType {
    if f.energy < 0.001 {
        return SoundType::None;
    }

    if f.zcr > 0.12 && f.rolloff > 6000.0 && f.flux > 0.5 {
        return SoundType::Noise;
    }

    if f.zcr < 0.06 && f.flux < 0.3 && f.centroid < 3000.0 && f.rolloff < 8000.0 {
        return SoundType::Music;
    }

    if f.zcr > 0.03 && f.zcr < 0.15 && f.centroid > 300.0 && f.centroid < 4000.0 && f.flux < 0.8 {
        return SoundType::Voice;
    }

    if f.energy > 0.01 {
        SoundType::Music
    } else {
        SoundType::Noise
    }
}

fn majority_vote(history: &VecDeque<SoundType>) -> ClassificationResult {
    if history.is_empty() {
        return ClassificationResult {
            primary: SoundType::None,
            secondary: SoundType::None,
            primary_votes: 0,
            secondary_votes: 0,
        };
    }

    let mut counts = [
        (SoundType::None, 0usize),
        (SoundType::Voice, 0usize),
        (SoundType::Music, 0usize),
        (SoundType::Noise, 0usize),
    ];

    for &t in history.iter() {
        match t {
            SoundType::None => counts[0].1 += 1,
            SoundType::Voice => counts[1].1 += 1,
            SoundType::Music => counts[2].1 += 1,
            SoundType::Noise => counts[3].1 += 1,
        }
    }

    counts.sort_by(|a, b| b.1.cmp(&a.1));

    ClassificationResult {
        primary: counts[0].0,
        secondary: counts[1].0,
        primary_votes: counts[0].1,
        secondary_votes: counts[1].1,
    }
}