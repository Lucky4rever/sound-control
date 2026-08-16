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

use std::collections::HashMap;
use crate::{
    AppSession, SoundType, SoundMode, AmplitudeBuffer, SoundTypeBuffer,
    ActivityTracker, classify_tick, Settings,
};

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

    #[cfg(target_os = "windows")]
    pub fn get_app_sessions(
        amp_histories: &mut HashMap<u32, AmplitudeBuffer>,
        type_buffers: &mut HashMap<u32, SoundTypeBuffer>,
        activity_tracker: &mut ActivityTracker,
        settings: &Settings,
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

                if pid == std::process::id() {
                    continue;
                }

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
                                volume: vol_scalar, effective_volume: vol_scalar,
                                is_muted, peak_level: 0, sound_type: SoundType::None,
                                sound_mode: SoundMode::Auto,
                            });
                            continue;
                        }
                    };
                    let peak = match meter.GetPeakValue() {
                        Ok(v) => v,
                        Err(_) => 0.0,
                    };

                    let peak_percent = (peak * 100.0).round() as i32;

                    if !activity_tracker.is_active(pid, peak_percent, settings.runtime.active_peak_threshold_i32()) {
                        continue;
                    }

                    let amp_buf = amp_histories.entry(pid).or_insert_with(|| AmplitudeBuffer::new(5));
                    amp_buf.push(peak);

                    let tick_type = classify_tick(amp_buf, settings);

                    let type_buf = type_buffers.entry(pid).or_insert_with(|| SoundTypeBuffer::new(crate::PERIOD_MS, crate::TICK_MS));
                    type_buf.push(tick_type);

                    let resolved_type = type_buf.resolve();

                    (peak_percent, resolved_type)
                };

                let name = Self::get_process_name(pid);
                if name.is_empty() || name == "PID 0" {
                    continue;
                }

                sessions.push(AppSession {
                    pid, name, volume: vol_scalar, effective_volume: vol_scalar,
                    is_muted, peak_level, sound_type,
                    sound_mode: SoundMode::Auto,
                });
            }

            let active_pids: std::collections::HashSet<u32> = sessions.iter().map(|s| s.pid).collect();
            amp_histories.retain(|pid, _| active_pids.contains(pid));
            type_buffers.retain(|pid, _| active_pids.contains(pid));
            activity_tracker.cleanup(&active_pids);

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
        _amp_histories: &mut HashMap<u32, AmplitudeBuffer>,
        _type_buffers: &mut HashMap<u32, SoundTypeBuffer>,
        _activity_tracker: &mut ActivityTracker,
    ) -> Vec<AppSession> { vec![] }

    #[cfg(not(target_os = "windows"))]
    pub fn set_app_volume(_pid: u32, _volume: i32) {}
}