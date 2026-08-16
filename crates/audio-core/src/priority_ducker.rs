use std::collections::HashMap;
use std::time::Instant;
use crate::{AppSession, SoundType, SoundMode, Settings};
use crate::volume_envelope::VolumeEnvelope;

#[derive(Debug)]
struct DuckState {
    envelope: VolumeEnvelope,
    last_duck: Instant,
    is_ducked: bool,
}

pub struct PriorityDucker {
    states: HashMap<u32, DuckState>,
}

impl PriorityDucker {
    pub fn new() -> Self {
        Self { states: HashMap::new() }
    }

    pub fn process(&mut self, sessions: &mut [AppSession], settings: &Settings) -> HashMap<u32, i32> {
        let mut indexed: Vec<(usize, i32)> = sessions.iter().enumerate()
            .map(|(i, s)| (i, effective_priority(s.sound_mode, s.sound_type)))
            .filter(|(_, p)| *p >= 0)
            .collect();

        indexed.sort_by_key(|(_, p)| *p);

        let mut targets = HashMap::new();
        if indexed.is_empty() {
            self.states.retain(|_, _| false);
            return targets;
        }

        let voice_info = indexed.iter().find(|(idx, _)| {
            let s = &sessions[*idx];
            (s.sound_type == SoundType::Voice || s.sound_mode == SoundMode::Voice) && s.peak_level > 0
        }).map(|&(idx, _)| {
            let s = &sessions[idx];
            (s.volume, s.peak_level.max(1))
        });

        let voice_active = voice_info.is_some();
        let (voice_vol, voice_peak) = voice_info.unwrap_or((0, 1));

        for (rank, (idx, _)) in indexed.iter().enumerate() {
            let session = &sessions[*idx];
            let pid = session.pid;

            let base_vol = settings.apps.get(&session.name)
                .map(|cfg| cfg.user_volume.clamp(0, 100) as f32 / 100.0)
                .unwrap_or(session.volume.clamp(0, 100) as f32 / 100.0);

            let st = self.states.entry(pid).or_insert_with(|| DuckState {
                envelope: VolumeEnvelope::new(8, settings),
                last_duck: Instant::now(),
                is_ducked: false,
            });

            // Оновлюємо параметри envelope якщо налаштування змінились
            st.envelope.update_rates(settings);

            let target_ratio: f32;

            if rank == 0 {
                target_ratio = 1.0;
            } else if voice_active {
                st.is_ducked = true;
                st.last_duck = Instant::now();

                let self_peak = session.peak_level.max(1) as f32;
                let v_vol = voice_vol as f32;
                let v_peak = voice_peak as f32;

                let raw_percent = v_vol * v_peak / self_peak * settings.runtime.gain_coefficient_f32();
                target_ratio = (raw_percent / 100.0).clamp(0.0, 1.0);
            } else {
                if st.last_duck.elapsed().as_millis() > settings.runtime.recovery_ms_u128() {
                    st.is_ducked = false;
                    target_ratio = 1.0;
                } else {
                    let current = st.envelope.current();
                    let t = (st.last_duck.elapsed().as_millis() as f32 / settings.runtime.recovery_ms_u128() as f32).min(1.0);
                    target_ratio = current + (1.0 - current) * t;
                }
            }

            let abs_target = base_vol * target_ratio;
            st.envelope.push_target(abs_target);
            let eff = st.envelope.current();
            let eff_i = (eff * 100.0).round() as i32;

            targets.insert(pid, eff_i);
            sessions[*idx].effective_volume = eff_i;
        }

        let active_pids: std::collections::HashSet<u32> = sessions.iter().map(|s| s.pid).collect();
        self.states.retain(|pid, _| active_pids.contains(pid));

        targets
    }
}

pub fn effective_priority(mode: SoundMode, resolved: SoundType) -> i32 {
    match mode {
        SoundMode::Other => -1,
        SoundMode::Voice => 0,
        SoundMode::Music => 10,
        SoundMode::Auto => match resolved {
            SoundType::Voice => 1,
            SoundType::Music => 10,
            SoundType::Noise => 15,
            SoundType::None => 20,
        },
    }
}