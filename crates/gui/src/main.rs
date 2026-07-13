slint::include_modules!();

use audio_core::{AudioController, AmplitudeBuffer, SoundTypeBuffer};
use audio_classifier::AudioClassifier;
use app_tray::SystemTray;
use tray_icon::TrayIconEvent;
use std::time::Duration;
use std::rc::Rc;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let icon_path = manifest.join("../../assets/icon.png");
    let model_path = manifest.join("../../assets/model.onnx");

    let _tray = SystemTray::init(&icon_path)?;
    let ui = AppWindow::new()?;

    let _classifier = AudioClassifier::new(model_path.to_str().unwrap())?;

    let mut last_known_volume = AudioController::get_current_volume();
    ui.set_volume(last_known_volume);

    let ui_slider_handle = ui.as_weak();
    ui.on_volume_changed(move |val| {
        if let Some(ui) = ui_slider_handle.upgrade() {
            ui.set_volume(val);
            AudioController::set_volume(val);
        }
    });

    let ui_btn_handle = ui.as_weak();
    ui.on_volume_step(move |step| {
        if let Some(ui) = ui_btn_handle.upgrade() {
            let current = ui.get_volume();
            let next = (current + step).clamp(0, 100);
            ui.set_volume(next);
            AudioController::set_volume(next);
        }
    });

    ui.on_app_volume_changed(|pid, val| {
        AudioController::set_app_volume(pid as u32, val);
    });

    ui.show()?;

    #[cfg(target_os = "windows")]
    {
        let _ = ui.window().with_winit_window(|winit_window| {
            if let Ok(window_handle) = winit_window.window_handle() {
                if let RawWindowHandle::Win32(handle) = window_handle.as_raw() {
                    let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut std::ffi::c_void);
                    unsafe {
                        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW.0 as i32);
                    }
                }
            }
        });
    }

    ui.window().set_position(slint::PhysicalPosition::new(-1000, -1000));
    let mut is_visible = false;

    let window_width = 320;
    let window_height = 420;

    let app_model = Rc::new(slint::VecModel::<AppVolume>::from(Vec::new()));
    ui.set_app_volumes(slint::ModelRc::from(app_model.clone()));

    // Буфери для кожного PID
    let mut amp_histories: HashMap<u32, AmplitudeBuffer> = HashMap::new();
    let mut type_buffers: HashMap<u32, SoundTypeBuffer> = HashMap::new();

    let ui_timer_handle = ui.as_weak();

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
        if let Some(ui) = ui_timer_handle.upgrade() {
            let system_vol = AudioController::get_current_volume();
            if system_vol != last_known_volume {
                last_known_volume = system_vol;
                ui.set_volume(system_vol);
            }

            let sessions = AudioController::get_app_sessions(&mut amp_histories, &mut type_buffers);

            while app_model.row_count() > sessions.len() {
                app_model.remove(app_model.row_count() - 1);
            }
            for (i, s) in sessions.iter().enumerate() {
                let new_item = AppVolume {
                    name: s.name.clone().into(),
                    volume: s.volume,
                    pid: s.pid as i32,
                    peak_level: s.peak_level,
                    sound_type: s.sound_type.as_str().into(),
                };
                if i < app_model.row_count() {
                    app_model.set_row_data(i, new_item);
                } else {
                    app_model.push(new_item);
                }
            }

            if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if event.click_type == tray_icon::ClickType::Left {
                    if !is_visible {
                        let rect = event.icon_rect;
                        let wx = (rect.position.x as f64 + rect.size.width as f64 / 2.0 - window_width as f64 / 2.0 - 200.0) as i32;
                        let mut wy = 40.0 as i32;
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