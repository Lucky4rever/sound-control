pub mod sound_mode;
pub mod settings;
pub mod volume_envelope;
pub mod priority_ducker;
pub mod constants;
pub mod activity_tracker;
pub mod platform;

pub use sound_mode::SoundMode;
pub use settings::{Settings, AppConfig};
pub use priority_ducker::{PriorityDucker, effective_priority};
pub use volume_envelope::VolumeEnvelope;
pub use activity_tracker::ActivityTracker;

#[cfg(target_os = "windows")]
pub use platform::windows::AudioController;
#[cfg(target_os = "linux")]
pub use platform::linux::AudioController;
#[cfg(target_os = "macos")]
pub use platform::macos::AudioController;

use std::collections::VecDeque;

pub const TICK_MS: u32 = 200;
pub const PERIOD_MS: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundType {
    None,
    Voice,
    Music,
    Noise,
}

impl SoundType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SoundType::None => "none",
            SoundType::Voice => "voice",
            SoundType::Music => "music",
            SoundType::Noise => "noise",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppSession {
    pub pid: u32,
    pub name: String,
    pub volume: i32,
    pub effective_volume: i32,
    pub is_muted: bool,
    pub peak_level: i32,
    pub sound_type: SoundType,
    pub sound_mode: SoundMode,
}

pub struct SoundTypeBuffer {
    buffer: VecDeque<SoundType>,
    capacity: usize,
    period_ms: u32,
}

impl SoundTypeBuffer {
    pub fn new(period_ms: u32, tick_ms: u32) -> Self {
        let capacity = (period_ms / tick_ms).max(1) as usize;
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            period_ms,
        }
    }

    pub fn push(&mut self, value: SoundType) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(value);
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.capacity
    }

    pub fn resolve(&self) -> SoundType {
        if self.buffer.is_empty() {
            return SoundType::None;
        }
        if self.buffer.iter().any(|&t| t == SoundType::Voice) {
            return SoundType::Voice;
        }
        if self.buffer.iter().any(|&t| t == SoundType::Music) {
            return SoundType::Music;
        }
        if self.buffer.iter().any(|&t| t == SoundType::Noise) {
            return SoundType::Noise;
        }
        SoundType::None
    }

    pub fn period_ms(&self) -> u32 {
        self.period_ms
    }

    pub fn set_period_ms(&mut self, period_ms: u32, tick_ms: u32) {
        self.period_ms = period_ms;
        let new_capacity = (period_ms / tick_ms).max(1) as usize;
        self.capacity = new_capacity;
        while self.buffer.len() > self.capacity {
            self.buffer.pop_front();
        }
    }
}

pub struct AmplitudeBuffer {
    buffer: VecDeque<f32>,
    capacity: usize,
}

impl AmplitudeBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn push(&mut self, value: f32) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(value);
    }

    pub fn correlation(&self) -> f32 {
        let vals: Vec<f32> = self.buffer.iter().cloned().collect();
        if vals.len() < 4 {
            return 0.0;
        }
        let diffs: Vec<f32> = vals.windows(2).map(|w| w[1] - w[0]).collect();
        if diffs.len() < 2 {
            return 0.0;
        }
        let n = diffs.len() - 1;
        let x: Vec<f32> = diffs[..n].to_vec();
        let y: Vec<f32> = diffs[1..].to_vec();
        let mean_x = x.iter().sum::<f32>() / n as f32;
        let mean_y = y.iter().sum::<f32>() / n as f32;
        let mut num = 0.0f32;
        let mut den_x = 0.0f32;
        let mut den_y = 0.0f32;
        for i in 0..n {
            let dx = x[i] - mean_x;
            let dy = y[i] - mean_y;
            num += dx * dy;
            den_x += dx * dx;
            den_y += dy * dy;
        }
        let den = (den_x * den_y).sqrt();
        if den < 1e-10 {
            return 0.0;
        }
        (num / den).abs()
    }

    pub fn mean(&self) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        self.buffer.iter().sum::<f32>() / self.buffer.len() as f32
    }

    pub fn std(&self) -> f32 {
        if self.buffer.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance = self.buffer.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / self.buffer.len() as f32;
        variance.sqrt()
    }
}

pub fn classify_tick(amp_buffer: &AmplitudeBuffer) -> SoundType {
    let mean_amp = amp_buffer.mean();

    if mean_amp < 0.003 {
        return SoundType::None;
    }

    if amp_buffer.len() < 3 {
        return SoundType::None;
    }

    let corr = amp_buffer.correlation();
    let std_amp = amp_buffer.std();

    if std_amp < constants::NOISE_STD_THRESHOLD && mean_amp > 0.003 {
        return SoundType::Noise;
    }

    if mean_amp < 0.01 && std_amp < 0.003 {
        return SoundType::None;
    }

    if corr > 0.5 {
        SoundType::Music
    } else if corr < 0.25 {
        SoundType::Voice
    } else {
        if std_amp < mean_amp * 0.25 {
            SoundType::Music
        } else {
            SoundType::Voice
        }
    }
}