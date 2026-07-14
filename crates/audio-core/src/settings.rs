use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

use crate::sound_mode::SoundMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub process_name: String,
    pub sound_mode: SoundMode,
    /// Бажана гучність користувачем (0–100)
    pub user_volume: i32,
    /// Пріоритет (виводиться з режиму, але можна розширити під ручне керування)
    pub priority: i32,
    /// Гучність процесу ДО запуску нашого застосунку (для відновлення при виході)
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub apps: HashMap<String, AppConfig>,
    #[serde(skip)]
    dirty: bool,
}

impl Settings {
    pub fn path() -> PathBuf {
        let mut path = dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
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

    pub fn save(&mut self) -> anyhow::Result<()> {
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

    /// Відновити оригінальні гучності для активних процесів
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