use std::collections::VecDeque;
use crate::settings::Settings;

#[derive(Debug)]
pub struct VolumeEnvelope {
    history: VecDeque<f32>,
    capacity: usize,
    current: f32,
    attack: f32,
    release: f32,
}

impl VolumeEnvelope {
    pub fn new(capacity: usize, settings: &Settings) -> Self {
        Self {
            history: VecDeque::with_capacity(capacity),
            capacity,
            current: 1.0,
            attack: settings.runtime.envelope_attack_f32(),
            release: settings.runtime.envelope_release_f32(),
        }
    }

    pub fn update_rates(&mut self, settings: &Settings) {
        self.attack = settings.runtime.envelope_attack_f32();
        self.release = settings.runtime.envelope_release_f32();
    }

    pub fn push_target(&mut self, target: f32) {
        if self.history.len() >= self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(target);

        let alpha = if self.is_stable() { self.attack } else { self.release };
        self.current += (target - self.current) * alpha;
        self.current = self.current.clamp(0.0, 1.0);
    }

    fn is_stable(&self) -> bool {
        if self.history.len() < 4 { return true; }
        let vals: Vec<f32> = self.history.iter().cloned().collect();
        let diffs: Vec<f32> = vals.windows(2).map(|w| w[1] - w[0]).collect();
        Self::correlation(&diffs) > 0.25
    }

    fn correlation(diffs: &[f32]) -> f32 {
        if diffs.len() < 2 { return 0.0; }
        let n = diffs.len() - 1;
        let x: Vec<f32> = diffs[..n].to_vec();
        let y: Vec<f32> = diffs[1..].to_vec();
        let mx = x.iter().sum::<f32>() / n as f32;
        let my = y.iter().sum::<f32>() / n as f32;
        let mut num = 0.0f32;
        let mut dx2 = 0.0f32;
        let mut dy2 = 0.0f32;
        for i in 0..n {
            let dxi = x[i] - mx;
            let dyi = y[i] - my;
            num += dxi * dyi;
            dx2 += dxi * dxi;
            dy2 += dyi * dyi;
        }
        let den = (dx2 * dy2).sqrt();
        if den < 1e-10 { return 0.0; }
        (num / den).abs()
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn reset(&mut self) {
        self.current = 1.0;
        self.history.clear();
    }
}