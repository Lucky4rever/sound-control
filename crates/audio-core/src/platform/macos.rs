use std::collections::HashMap;
use std::ffi::c_void;
use std::mem;
use std::ptr;

use coreaudio_sys::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectSetPropertyData, OSStatus,
    kAudioDevicePropertyVolumeScalar, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyElementMaster, kAudioObjectPropertyScopeGlobal,
    kAudioObjectSystemObject,
};

use crate::{ActivityTracker, AmplitudeBuffer, AppSession, SoundTypeBuffer};

pub struct AudioController;

impl AudioController {
    /// Отримує ID default output device (наприклад, вбудовані динаміки).
    fn get_default_output_device() -> Option<AudioObjectID> {
        let property_address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };

        let mut device_id: AudioObjectID = 0;
        let mut size = mem::size_of::<AudioObjectID>() as u32;

        let status: OSStatus = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &property_address,
                0,
                ptr::null(),
                &mut size,
                &mut device_id as *mut _ as *mut c_void,
            )
        };

        if status == 0 && device_id != 0 {
            Some(device_id)
        } else {
            None
        }
    }

    /// Читає системну гучність (0.0 – 1.0) з пристрою.
    fn get_device_volume_scalar(device_id: AudioObjectID) -> Option<f32> {
        let property_address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyVolumeScalar,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };

        // Перевіримо, чи властивість доступна на цьому пристрої
        let mut size: u32 = 0;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                device_id,
                &property_address,
                0,
                ptr::null(),
                &mut size,
            )
        };

        if status != 0 || size == 0 {
            return None;
        }

        let mut volume: f32 = 0.0;
        let mut data_size = mem::size_of::<f32>() as u32;

        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &property_address,
                0,
                ptr::null(),
                &mut data_size,
                &mut volume as *mut _ as *mut c_void,
            )
        };

        if status == 0 {
            Some(volume.clamp(0.0, 1.0))
        } else {
            None
        }
    }

    /// Встановлює системну гучність (0.0 – 1.0) на пристрої.
    fn set_device_volume_scalar(device_id: AudioObjectID, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        let property_address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyVolumeScalar,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };

        let mut value = clamped;
        unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &property_address,
                0,
                ptr::null(),
                mem::size_of::<f32>() as u32,
                &mut value as *mut _ as *mut c_void,
            );
        }
    }

    // ── Публічний API ─────────────────────────────────────────────

    pub fn get_current_volume() -> i32 {
        // Спроба через Core Audio HAL
        if let Some(device) = Self::get_default_output_device() {
            if let Some(vol) = Self::get_device_volume_scalar(device) {
                return (vol * 100.0).round() as i32;
            }
        }

        // Fallback: osascript (Bluetooth-гарнітура, AirPlay тощо)
        if let Ok(out) = std::process::Command::new("osascript")
            .args(&["-e", "output volume of (get volume settings)"])
            .output()
        {
            if let Ok(text) = String::from_utf8(out.stdout) {
                if let Ok(v) = text.trim().parse::<i32>() {
                    return v.clamp(0, 100);
                }
            }
        }

        50
    }

    pub fn set_volume(volume: i32) {
        let clamped = volume.clamp(0, 100);
        let scalar = clamped as f32 / 100.0;

        // Спроба через Core Audio HAL
        if let Some(device) = Self::get_default_output_device() {
            Self::set_device_volume_scalar(device, scalar);
            return;
        }

        // Fallback: osascript
        let _ = std::process::Command::new("osascript")
            .args(&["-e", &format!("set volume output volume {}", clamped)])
            .output();
    }

    pub fn get_app_sessions(
        _amp_histories: &mut HashMap<u32, AmplitudeBuffer>,
        _type_buffers: &mut HashMap<u32, SoundTypeBuffer>,
        _activity_tracker: &mut ActivityTracker,
        _settings: &Settings,
    ) -> Vec<AppSession> {
        vec![]
    }

    pub fn set_app_volume(_pid: u32, _volume: i32) {
        // macOS не підтримує зміну гучності per-app через публічний API.
    }
}