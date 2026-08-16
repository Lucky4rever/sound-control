use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

use crate::sound_mode::SoundMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub process_name: String,
    pub sound_mode: SoundMode,
    pub user_volume: i32,
    pub priority: i32,
    pub original_volume: Option<i32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            process_name: String::new(),
            sound_mode: SoundMode::Auto,
            user_volume: 100,
            priority: 99,
            original_volume: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub ducking_ratio: i32,         // 0-100 (відсотки)
    pub recovery_ms: i32,           // 100-5000 (мс)
    pub active_peak_threshold: i32, // 0-20
    pub envelope_attack: i32,       // 1-100 (×0.01)
    pub envelope_release: i32,      // 1-100 (×0.01)
    pub gain_coefficient: i32,      // 0-100 (×0.01)
    pub inactivity_timeout_ms: i32, // 1000-10000 (мс)
    pub noise_std_threshold: i32,   // 1-50 (×0.001)
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            ducking_ratio: 20,
            recovery_ms: 1000,
            active_peak_threshold: 3,
            envelope_attack: 60,
            envelope_release: 15,
            gain_coefficient: 25,
            inactivity_timeout_ms: 3000,
            noise_std_threshold: 5,
        }
    }
}

impl RuntimeSettings {
    pub fn ducking_ratio_f32(&self) -> f32 {
        self.ducking_ratio as f32 / 100.0
    }
    pub fn recovery_ms_u128(&self) -> u128 {
        self.recovery_ms as u128
    }
    pub fn active_peak_threshold_i32(&self) -> i32 {
        self.active_peak_threshold
    }
    pub fn envelope_attack_f32(&self) -> f32 {
        self.envelope_attack as f32 / 100.0
    }
    pub fn envelope_release_f32(&self) -> f32 {
        self.envelope_release as f32 / 100.0
    }
    pub fn gain_coefficient_f32(&self) -> f32 {
        self.gain_coefficient as f32 / 100.0
    }
    pub fn inactivity_timeout_ms_u32(&self) -> u32 {
        self.inactivity_timeout_ms as u32
    }
    pub fn noise_std_threshold_f32(&self) -> f32 {
        self.noise_std_threshold as f32 / 1000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub apps: HashMap<String, AppConfig>,
    pub runtime: RuntimeSettings,
    #[serde(skip)]
    dirty: bool,
}

impl Settings {
    pub fn path() -> PathBuf {
        let mut path = dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        println!("{}", path.display());
        path.push("sound-control");
        let _ = std::fs::create_dir_all(&path);
        path.push("settings.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str(&data) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    pub fn save(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::path();
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data)?;
        self.dirty = false;
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn get_or_default(&mut self, name: &str) -> &mut AppConfig {
        if !self.apps.contains_key(name) {
            self.apps.insert(name.to_string(), AppConfig {
                process_name: name.to_string(),
                ..Default::default()
            });
        }
        self.apps.get_mut(name).unwrap()
    }

    pub fn restore_all(&self, active_pids: &[(u32, String)]) {
        for (pid, name) in active_pids {
            if let Some(cfg) = self.apps.get(name) {
                if let Some(orig) = cfg.original_volume {
                    crate::AudioController::set_app_volume(*pid, orig);
                }
            }
        }
    }
}