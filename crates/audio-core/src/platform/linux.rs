use std::collections::HashMap;
use crate::{AppSession, AmplitudeBuffer, SoundTypeBuffer, ActivityTracker};

pub struct AudioController;

impl AudioController {
    pub fn get_current_volume() -> i32 {
        // Спроба через amixer
        if let Ok(out) = std::process::Command::new("amixer")
            .args(&["-D", "pulse", "sget", "Master"])
            .output()
        {
            if let Ok(text) = String::from_utf8(out.stdout) {
                for line in text.lines() {
                    if let Some(start) = line.find('[') {
                        if let Some(end) = line[start..].find('%') {
                            let num = &line[start+1..start+end];
                            if let Ok(v) = num.parse::<i32>() {
                                return v.clamp(0, 100);
                            }
                        }
                    }
                }
            }
        }

        // Fallback через pactl
        if let Ok(out) = std::process::Command::new("pactl")
            .args(&["list", "sinks"])
            .output()
        {
            if let Ok(text) = String::from_utf8(out.stdout) {
                for line in text.lines() {
                    if line.contains("Volume:") && line.contains("front-left") {
                        if let Some(pct) = line.split('/').nth(1) {
                            let trimmed = pct.trim().trim_end_matches('%');
                            if let Ok(v) = trimmed.parse::<i32>() {
                                return v.clamp(0, 100);
                            }
                        }
                    }
                }
            }
        }

        50
    }

    pub fn set_volume(volume: i32) {
        let clamped = volume.clamp(0, 100);
        let _ = std::process::Command::new("amixer")
            .args(&["-D", "pulse", "sset", "Master", &format!("{}%", clamped)])
            .spawn();
    }

    pub fn get_app_sessions(
        _amp_histories: &mut HashMap<u32, AmplitudeBuffer>,
        _type_buffers: &mut HashMap<u32, SoundTypeBuffer>,
        _activity_tracker: &mut ActivityTracker,
    ) -> Vec<AppSession> {
        // TODO: PipeWire/PulseAudio per-app sessions not yet implemented
        vec![]
    }

    pub fn set_app_volume(_pid: u32, _volume: i32) {
        // TODO: Not implemented on Linux without PipeWire/PulseAudio direct control
    }
}