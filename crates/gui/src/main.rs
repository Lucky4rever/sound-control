#![windows_subsystem = "windows"]

slint::include_modules!();

use audio_core::{
    AudioController, AmplitudeBuffer, SoundTypeBuffer,
    SoundMode, Settings, PriorityDucker,
};
use audio_classifier::AudioClassifier;
use app_tray::SystemTray;
use tray_icon::TrayIconEvent;
use std::time::{Duration, Instant};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use slint::Model;

#[cfg(target_os = "windows")]
use i_slint_backend_winit::WinitWindowAccessor;
#[cfg(target_os = "windows")]
use i_slint_backend_winit::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

struct AppState {
    settings: RefCell<Settings>,
    original_volumes: RefCell<HashMap<u32, (String, i32)>>,
    pid_to_name: RefCell<HashMap<u32, String>>,
    last_save: RefCell<Option<Instant>>,
}

impl AppState {
    fn restore_and_save(&self) {
        println!("Відновлення оригінальних гучностей...");
        let orig = self.original_volumes.borrow();
        for (pid, (_name, vol)) in orig.iter() {
            AudioController::set_app_volume(*pid, *vol);
        }
        drop(orig);
        let _ = self.settings.borrow_mut().save();
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.restore_and_save();
    }
}

fn get_assets_dir() -> std::path::PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let assets = exe_dir.join("assets");
            if assets.exists() {
                return assets;
            }
        }
    }

    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets_dir = get_assets_dir();
    let icon_path = assets_dir.join("icon.png");
    let model_path = assets_dir.join("YamNet.onnx");

    let _tray = SystemTray::init(&icon_path)?;
    let ui = AppWindow::new()?;

    let classifier = AudioClassifier::new(model_path.to_str().unwrap())?;

    let state = Rc::new(AppState {
        settings: RefCell::new(Settings::load()),
        original_volumes: RefCell::new(HashMap::new()),
        pid_to_name: RefCell::new(HashMap::new()),
        last_save: RefCell::new(None),
    });

    let mut last_known_volume = AudioController::get_current_volume();
    ui.set_volume(last_known_volume);

    // --- Системна гучність ---
    let ui_slider = ui.as_weak();
    ui.on_volume_changed(move |val| {
        if let Some(ui) = ui_slider.upgrade() {
            ui.set_volume(val);
            AudioController::set_volume(val);
        }
    });

    let ui_btn = ui.as_weak();
    ui.on_volume_step(move |step| {
        if let Some(ui) = ui_btn.upgrade() {
            let next = (ui.get_volume() + step).clamp(0, 100);
            ui.set_volume(next);
            AudioController::set_volume(next);
        }
    });

    // --- Гучність додатку (слайдер) ---
    let st_vol = state.clone();
    ui.on_app_volume_changed(move |pid, val| {
        let clamped = val.clamp(0, 100);
        let pid_u = pid as u32;
        AudioController::set_app_volume(pid_u, clamped);

        let map = st_vol.pid_to_name.borrow();
        let name = map.get(&pid_u).cloned();
        drop(map);

        if let Some(name) = name {
            let mut s = st_vol.settings.borrow_mut();
            let cfg = s.get_or_default(&name);
            cfg.user_volume = clamped;
            let _ = cfg;

            s.mark_dirty();
        }
    });

    // --- Зміна режиму (ComboBox) ---
    let st_mode = state.clone();
    ui.on_app_mode_changed(move |pid, mode_index| {
        let mode = match mode_index {
            0 => SoundMode::Auto,
            1 => SoundMode::Voice,
            2 => SoundMode::Music,
            3 => SoundMode::Other,
            _ => SoundMode::Auto,
        };
        let pid_u = pid as u32;

        let map = st_mode.pid_to_name.borrow();
        let name = map.get(&pid_u).cloned();
        drop(map);

        if let Some(name) = name {
            let mut s = st_mode.settings.borrow_mut();
            let cfg = s.get_or_default(&name);
            cfg.sound_mode = mode;
            cfg.priority = mode.base_priority();
            let _ = cfg;

            s.mark_dirty();
        }
    });

    let st_settings = state.clone();
    ui.on_settings_changed(move |settings| {
        let mut s = st_settings.settings.borrow_mut();
        s.runtime.ducking_ratio = settings.ducking_ratio;
        s.runtime.recovery_ms = settings.recovery_ms;
        s.runtime.active_peak_threshold = settings.active_peak_threshold;
        s.runtime.envelope_attack = settings.envelope_attack;
        s.runtime.envelope_release = settings.envelope_release;
        s.runtime.gain_coefficient = settings.gain_coefficient;
        s.runtime.inactivity_timeout_ms = settings.inactivity_timeout_ms;
        s.runtime.noise_std_threshold = settings.noise_std_threshold;
        s.mark_dirty();
    });

    ui.show()?;

    #[cfg(target_os = "windows")]
    {
        let _ = ui.window().with_winit_window(|winit_window| {
            if let Ok(window_handle) = winit_window.window_handle() {
                if let RawWindowHandle::Win32(handle) = window_handle.as_raw() {
                    let hwnd = windows::Win32::Foundation::HWND(
                        handle.hwnd.get() as *mut std::ffi::c_void
                    );
                    unsafe {
                        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW.0 as i32);
                    }
                }
            }
        });

        let settings = state.settings.borrow();
        let rt = settings.runtime.clone();
        ui.set_runtime_settings(RuntimeSettings {
            ducking_ratio: rt.ducking_ratio,
            recovery_ms: rt.recovery_ms,
            active_peak_threshold: rt.active_peak_threshold,
            envelope_attack: rt.envelope_attack,
            envelope_release: rt.envelope_release,
            gain_coefficient: rt.gain_coefficient,
            inactivity_timeout_ms: rt.inactivity_timeout_ms,
            noise_std_threshold: rt.noise_std_threshold,
        });
    }

    ui.window().set_position(slint::PhysicalPosition::new(-1000, -1000));
    let mut is_visible = false;
    let window_width = 320;
    let _window_height = 420;

    let app_model = Rc::new(slint::VecModel::<AppVolume>::from(Vec::new()));
    ui.set_app_volumes(slint::ModelRc::from(app_model.clone()));

    let mut amp_histories: HashMap<u32, AmplitudeBuffer> = HashMap::new();
    let mut type_buffers: HashMap<u32, SoundTypeBuffer> = HashMap::new();
    let mut priority_ducker = PriorityDucker::new();
    
    let mut activity_tracker = audio_core::ActivityTracker::new(&state.settings.borrow());

    let ui_timer = ui.as_weak();
    let st_timer = state.clone();

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
        if let Some(ui) = ui_timer.upgrade() {
            // 1. Системна гучність
            let sys_vol = AudioController::get_current_volume();
            if sys_vol != last_known_volume {
                last_known_volume = sys_vol;
                ui.set_volume(sys_vol);
            }

            // 2. Отримуємо сирі сесії
            let settings_ref = st_timer.settings.borrow();
            let mut sessions = AudioController::get_app_sessions(
                &mut amp_histories,
                &mut type_buffers,
                &mut activity_tracker,
                &*settings_ref,
            );
            drop(settings_ref);

            let classification = classifier.get_result();

            // 3. Застосовуємо збережені налаштування + фіксуємо оригінальні гучності
            {
                let mut settings = st_timer.settings.borrow_mut();
                let mut orig = st_timer.original_volumes.borrow_mut();
                let mut pid_map = st_timer.pid_to_name.borrow_mut();

                for s in sessions.iter_mut() {
                    let (mode, need_mark) = {
                        let cfg = settings.get_or_default(&s.name);
                        let mode = cfg.sound_mode;
                        let need_mark = cfg.original_volume.is_none();
                        if need_mark {
                            cfg.original_volume = Some(s.volume);
                        }
                        (mode, need_mark)
                    };
                    s.sound_mode = mode;
                    if need_mark {
                        settings.mark_dirty();
                    }

                    if !orig.contains_key(&s.pid) {
                        orig.insert(s.pid, (s.name.clone(), s.volume));
                        pid_map.insert(s.pid, s.name.clone());
                    }
                }
            }

            // 4. Ducking / каскадна логіка
            let settings_ref = st_timer.settings.borrow();
            let targets = priority_ducker.process(&mut sessions, &*settings_ref);
            drop(settings_ref);

            // 5. Застосовуємо цільові гучності до Windows (крім «Інше»)
            for s in &sessions {
                if s.sound_mode == SoundMode::Other {
                    continue;
                }
                if let Some(&target) = targets.get(&s.pid) {
                    if target != s.volume {
                        AudioController::set_app_volume(s.pid, target);
                    }
                }
            }

            // 6. Сортування для UI: за пріоритетом (Other в кінці)
            let mut ui_sessions = sessions.clone();
            ui_sessions.sort_by_key(|s| {
                let p = audio_core::priority_ducker::effective_priority(s.sound_mode, s.sound_type);
                (p == -1, p)
            });

            // 7. Оновлюємо UI
            while app_model.row_count() > ui_sessions.len() {
                app_model.remove(app_model.row_count() - 1);
            }
            for (i, s) in ui_sessions.iter().enumerate() {
                let item = AppVolume {
                    name: s.name.clone().into(),
                    volume: s.volume,
                    pid: s.pid as i32,
                    peak_level: s.peak_level,
                    sound_type: s.sound_type.as_str().into(),
                    secondary_sound_type: if s.sound_mode == SoundMode::Auto {
                        classification.secondary.as_str().into()
                    } else {
                        "".into()
                    },
                    sound_mode: s.sound_mode.as_str().into(),
                    priority: audio_core::priority_ducker::effective_priority(s.sound_mode, s.sound_type),
                };
                if i < app_model.row_count() {
                    println!("sound_type: {}. secondary_sound_type: {}", s.sound_type.as_str(), classification.secondary.as_str());
                    app_model.set_row_data(i, item);
                } else {
                    app_model.push(item);
                }
            }

            // 8. Періодичне збереження
            {
                let mut last = st_timer.last_save.borrow_mut();
                let need_save = st_timer.settings.borrow().is_dirty()
                    && last.map(|t| t.elapsed().as_secs() > 5).unwrap_or(true);
                if need_save {
                    let _ = st_timer.settings.borrow_mut().save();
                    *last = Some(Instant::now());
                }
            }

            // 9. Оновлення тайм-ауту активності

            activity_tracker.update_timeout(&st_timer.settings.borrow());

            // 10. Трей
            if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if event.click_type == tray_icon::ClickType::Left {
                    if !is_visible {
                        let rect = event.icon_rect;
                        let wx = (rect.position.x as f64 + rect.size.width as f64 / 2.0
                            - window_width as f64 / 2.0 - 200.0) as i32;
                        let mut wy = 40_i32;
                        if wy < 0 {
                            wy = (rect.position.y as f64 + rect.size.height as f64 + 20.0) as i32;
                        }
                        ui.window().set_position(slint::PhysicalPosition::new(wx, wy));
                        is_visible = true;
                    } else {
                        ui.window().set_position(slint::PhysicalPosition::new(-1000, -1000));
                        is_visible = false;
                    }
                }
            }
        }
    });

    ui.run()?;
    Ok(())
}