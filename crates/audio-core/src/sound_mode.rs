use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SoundMode {
    Auto,
    Voice,
    Music,
    Other,
}

impl SoundMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SoundMode::Auto => "auto",
            SoundMode::Voice => "voice",
            SoundMode::Music => "music",
            SoundMode::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(SoundMode::Auto),
            "voice" => Some(SoundMode::Voice),
            "music" => Some(SoundMode::Music),
            "other" => Some(SoundMode::Other),
            _ => None,
        }
    }

    /// Базовий числовий пріоритет. Менше = вищий. -1 = виключено (Other).
    pub fn base_priority(&self) -> i32 {
        match self {
            SoundMode::Voice => 0,
            SoundMode::Auto => 99, // перезаписується динамічно
            SoundMode::Music => 10,
            SoundMode::Other => -1,
        }
    }
}