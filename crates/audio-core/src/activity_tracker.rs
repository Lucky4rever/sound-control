use std::collections::HashMap;
use std::time::Instant;
use crate::constants::INACTIVITY_TIMEOUT_MS;

pub struct ActivityTracker {
    last_active: HashMap<u32, Instant>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self { last_active: HashMap::new() }
    }

    /// Повертає true, якщо PID ще вважається активним.
    /// Якщо peak_level > threshold — оновлюємо таймер.
    /// Якщо ні — перевіряємо, чи не вийшов таймаут.
    pub fn is_active(&mut self, pid: u32, peak_level: i32, threshold: i32) -> bool {
        if peak_level > threshold {
            self.last_active.insert(pid, Instant::now());
            true
        } else {
            match self.last_active.get(&pid) {
                Some(&last) => {
                    let elapsed = last.elapsed().as_millis() as u32;
                    if elapsed < INACTIVITY_TIMEOUT_MS {
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