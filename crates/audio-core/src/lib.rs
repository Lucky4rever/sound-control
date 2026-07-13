#[cfg(target_os = "windows")]
use windows::{
    Win32::Media::Audio::{
        IAudioSessionManager2, IAudioSessionEnumerator, IAudioSessionControl,
        IAudioSessionControl2, ISimpleAudioVolume,
        IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
    },
    Win32::Media::Audio::Endpoints::IAudioMeterInformation,
    Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, CLSCTX_ALL, CoCreateInstance},
    Win32::System::ProcessStatus::GetModuleBaseNameW,
    Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    Win32::Foundation::CloseHandle,
};
#[cfg(target_os = "windows")]
use windows::core::Interface;

use std::collections::VecDeque;

const TICK_MS: u32 = 200;      // інтервал оновлення (мс)
const PERIOD_MS: u32 = 1000;   // період аналізу (мс)

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundType {
    None,
    Voice,
    Music,
}

impl SoundType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SoundType::None => "none",
            SoundType::Voice => "voice",
            SoundType::Music => "music",
        }
    }
}

/// Динамічний вектор типів звуку за період.
/// При кожному тіку: прибираємо першу комірку, додаємо нову останню.
/// Якщо в періоді був хоч раз Voice — весь період = Voice (пріоритет голосу).
pub struct SoundTypeBuffer {
    buffer: VecDeque<SoundType>,
    capacity: usize, // кількість тіків у періоді
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

    /// Повертає результуючий тип за період.
    /// Пріоритет: Voice > Music > None
    pub fn resolve(&self) -> SoundType {
        if self.buffer.is_empty() {
            return SoundType::None;
        }
        // Якщо хоч раз був голос — весь період = голос
        if self.buffer.iter().any(|&t| t == SoundType::Voice) {
            return SoundType::Voice;
        }
        // Якщо хоч раз була музика — музика
        if self.buffer.iter().any(|&t| t == SoundType::Music) {
            return SoundType::Music;
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
        // Обрізаємо зайві з початку, якщо новий розмір менший
        while self.buffer.len() > self.capacity {
            self.buffer.pop_front();
        }
    }
}

/// Буфер амплітуд для аналізу кореляції в рамках одного тіку
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

    /// Коефіцієнт кореляції між сусідніми змінами амплітуд
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

#[derive(Debug, Clone)]
pub struct AppSession {
    pub pid: u32,
    pub name: String,
    pub volume: i32,
    pub is_muted: bool,
    pub peak_level: i32,
    pub sound_type: SoundType,
}

pub struct AudioController;

impl AudioController {
    #[cfg(target_os = "windows")]
    fn get_volume_interface() -> Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let volume: windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
            Some(volume)
        }
    }

    pub fn get_current_volume() -> i32 {
        #[cfg(target_os = "windows")]
        {
            if let Some(volume_api) = Self::get_volume_interface() {
                unsafe {
                    if let Ok(vol) = volume_api.GetMasterVolumeLevelScalar() {
                        return (vol * 100.0f32).round() as i32;
                    }
                }
            }
        }
        50
    }

    pub fn set_volume(volume: i32) {
        let clamped = volume.clamp(0, 100);
        let scalar = clamped as f32 / 100.0;
        #[cfg(target_os = "windows")]
        {
            if let Some(volume_api) = Self::get_volume_interface() {
                unsafe {
                    let _ = volume_api.SetMasterVolumeLevelScalar(scalar, std::ptr::null());
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("amixer")
                .args(&["-D", "pulse", "sset", "Master", &format!("{}%", clamped)])
                .spawn()
                .ok();
        }
    }

    #[cfg(target_os = "windows")]
    fn get_device() -> Option<windows::Win32::Media::Audio::IMMDevice> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            Some(device)
        }
    }

    #[cfg(target_os = "windows")]
    fn get_process_name(pid: u32) -> String {
        unsafe {
            let h_process = match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                Ok(h) => h,
                Err(_) => return format!("PID {}", pid),
            };
            let mut buf = [0u16; 260];
            let len = GetModuleBaseNameW(h_process, None, &mut buf);
            let _ = CloseHandle(h_process);
            if len == 0 {
                return format!("PID {}", pid);
            }
            String::from_utf16_lossy(&buf[..len as usize])
        }
    }

    /// Класифікація звуку на основі кореляції амплітуд у рамках одного тіку
    fn classify_tick(amp_buffer: &AmplitudeBuffer) -> SoundType {
        let mean_amp = amp_buffer.mean();

        // Тиша: дуже тихий сигнал
        if mean_amp < 0.003 {
            return SoundType::None;
        }

        if amp_buffer.len() < 3 {
            // Недостатньо даних — вважаємо тишею
            return SoundType::None;
        }

        let corr = amp_buffer.correlation();
        let std_amp = amp_buffer.std();

        // Дуже тихий стабільний сигнал
        if mean_amp < 0.01 && std_amp < 0.003 {
            return SoundType::None;
        }

        // Класифікація за кореляцією
        if corr > 0.5 {
            // Висока кореляція = узгоджені зміни = музика
            SoundType::Music
        } else if corr < 0.25 {
            // Низька кореляція = хаотичні зміни = голос
            SoundType::Voice
        } else {
            // Середня кореляція — дивимось на стабільність
            if std_amp < mean_amp * 0.25 {
                SoundType::Music
            } else {
                SoundType::Voice
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub fn get_app_sessions(
        amp_histories: &mut std::collections::HashMap<u32, AmplitudeBuffer>,
        type_buffers: &mut std::collections::HashMap<u32, SoundTypeBuffer>,
    ) -> Vec<AppSession> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let Some(device) = Self::get_device() else { return vec![] };

            let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                Ok(sm) => sm,
                Err(_) => return vec![],
            };

            let enumerator: IAudioSessionEnumerator = match session_manager.GetSessionEnumerator() {
                Ok(e) => e,
                Err(_) => return vec![],
            };

            let count = match enumerator.GetCount() {
                Ok(c) => c,
                Err(_) => return vec![],
            };

            let mut sessions = Vec::new();

            for i in 0..count {
                let session_control: IAudioSessionControl = match enumerator.GetSession(i) {
                    Ok(sc) => sc,
                    Err(_) => continue,
                };

                let session_control2: IAudioSessionControl2 = match session_control.cast() {
                    Ok(sc2) => sc2,
                    Err(_) => continue,
                };

                let pid = match session_control2.GetProcessId() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let simple_volume: ISimpleAudioVolume = match session_control.cast() {
                    Ok(sv) => sv,
                    Err(_) => continue,
                };

                let vol_scalar = match simple_volume.GetMasterVolume() {
                    Ok(v) => (v * 100.0).round() as i32,
                    Err(_) => continue,
                };

                let is_muted = match simple_volume.GetMute() {
                    Ok(m) => m.as_bool(),
                    Err(_) => false,
                };

                let (peak_level, sound_type) = {
                    let meter: IAudioMeterInformation = match session_control.cast() {
                        Ok(m) => m,
                        Err(_) => {
                            sessions.push(AppSession {
                                pid, name: Self::get_process_name(pid),
                                volume: vol_scalar, is_muted,
                                peak_level: 0, sound_type: SoundType::None,
                            });
                            continue;
                        }
                    };
                    let peak = match meter.GetPeakValue() {
                        Ok(v) => v,
                        Err(_) => 0.0,
                    };

                    let peak_percent = (peak * 100.0).round() as i32;

                    // Оновлюємо буфер амплітуд (для аналізу в рамках тіку)
                    let amp_buf = amp_histories.entry(pid).or_insert_with(|| AmplitudeBuffer::new(5));
                    amp_buf.push(peak);

                    // Класифікуємо поточний тік
                    let tick_type = Self::classify_tick(amp_buf);

                    // Оновлюємо періодний буфер (для згладжування)
                    let type_buf = type_buffers.entry(pid).or_insert_with(|| SoundTypeBuffer::new(PERIOD_MS, TICK_MS));
                    type_buf.push(tick_type);

                    // Результуючий тип за період
                    let resolved_type = type_buf.resolve();

                    (peak_percent, resolved_type)
                };

                let name = Self::get_process_name(pid);
                if name.is_empty() || name == "PID 0" {
                    continue;
                }

                sessions.push(AppSession {
                    pid, name, volume: vol_scalar, is_muted, peak_level, sound_type,
                });
            }

            // Очищаємо історії для неактивних PID
            let active_pids: std::collections::HashSet<u32> = sessions.iter().map(|s| s.pid).collect();
            amp_histories.retain(|pid, _| active_pids.contains(pid));
            type_buffers.retain(|pid, _| active_pids.contains(pid));

            sessions
        }
    }

    #[cfg(target_os = "windows")]
    pub fn set_app_volume(pid: u32, volume: i32) {
        let clamped = volume.clamp(0, 100);
        let scalar = clamped as f32 / 100.0;
        unsafe {
            let Some(device) = Self::get_device() else { return };
            let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                Ok(sm) => sm,
                Err(_) => return,
            };
            let enumerator: IAudioSessionEnumerator = match session_manager.GetSessionEnumerator() {
                Ok(e) => e,
                Err(_) => return,
            };
            let count = match enumerator.GetCount() {
                Ok(c) => c,
                Err(_) => return,
            };

            for i in 0..count {
                let session_control: IAudioSessionControl = match enumerator.GetSession(i) {
                    Ok(sc) => sc,
                    Err(_) => continue,
                };
                let session_control2: IAudioSessionControl2 = match session_control.cast() {
                    Ok(sc2) => sc2,
                    Err(_) => continue,
                };
                let session_pid = match session_control2.GetProcessId() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                if session_pid == pid {
                    let simple_volume: ISimpleAudioVolume = match session_control.cast() {
                        Ok(sv) => sv,
                        Err(_) => continue,
                    };
                    let _ = simple_volume.SetMasterVolume(scalar, std::ptr::null());
                    break;
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_app_sessions(
        _amp_histories: &mut std::collections::HashMap<u32, AmplitudeBuffer>,
        _type_buffers: &mut std::collections::HashMap<u32, SoundTypeBuffer>,
    ) -> Vec<AppSession> { vec![] }

    #[cfg(not(target_os = "windows"))]
    pub fn set_app_volume(_pid: u32, _volume: i32) {}
}