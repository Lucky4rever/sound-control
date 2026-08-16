use std::collections::HashMap;
use std::time::Instant;
use crate::settings::Settings;

pub struct ActivityTracker {
    last_active: HashMap<u32, Instant>,
    timeout_ms: u32,
}

impl ActivityTracker {
    pub fn new(settings: &Settings) -> Self {
        Self {
            last_active: HashMap::new(),
            timeout_ms: settings.runtime.inactivity_timeout_ms_u32(),
        }
    }

    pub fn update_timeout(&mut self, settings: &Settings) {
        self.timeout_ms = settings.runtime.inactivity_timeout_ms_u32();
    }

    pub fn is_active(&mut self, pid: u32, peak_level: i32, threshold: i32) -> bool {
        if peak_level > threshold {
            self.last_active.insert(pid, Instant::now());
            true
        } else {
            match self.last_active.get(&pid) {
                Some(&last) => {
                    let elapsed = last.elapsed().as_millis() as u32;
                    if elapsed < self.timeout_ms {
                        true
                    } else {
                        self.last_active.remove(&pid);
                        false
                    }
                }
                None => false,
            }
        }
    }

    pub fn cleanup(&mut self, active_pids: &std::collections::HashSet<u32>) {
        self.last_active.retain(|pid, _| active_pids.contains(pid));
    }
}