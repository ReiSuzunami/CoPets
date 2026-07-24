mod control;
mod file_tail;
mod local_trust;
mod observer;
mod pet;

use std::{ffi::c_void, time::Duration};

use observer::RuntimeHandle;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

const WINDOW_HOVER_INTERVAL_MS: u64 = 100;
const TRAY_ID: &str = "copets-status";
const SETTINGS_WINDOW_LABEL: &str = "settings";

#[cfg(target_os = "macos")]
#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DragPointerSnapshot {
    pressed: bool,
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *const c_void) -> CGPoint;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

#[tauri::command]
fn get_drag_pointer_snapshot() -> Result<DragPointerSnapshot, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return Err("global pointer position unavailable".to_string());
        }
        let location = CGEventGetLocation(event);
        CFRelease(event);
        Ok(DragPointerSnapshot {
            pressed: CGEventSourceButtonState(0, 0),
            x: location.x,
            y: location.y,
        })
    }

    #[cfg(not(target_os = "macos"))]
    Ok(DragPointerSnapshot {
        pressed: false,
        x: 0.0,
        y: 0.0,
    })
}

fn point_in_window_bounds(
    pointer_x: f64,
    pointer_y: f64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> bool {
    let left = f64::from(x);
    let top = f64::from(y);
    pointer_x >= left
        && pointer_x < left + f64::from(width)
        && pointer_y >= top
        && pointer_y < top + f64::from(height)
}

fn window_hovered(window: &WebviewWindow) -> bool {
    if !window.is_visible().unwrap_or(false) {
        return false;
    }
    let Ok(pointer) = window.cursor_position() else {
        return false;
    };
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    point_in_window_bounds(
        pointer.x,
        pointer.y,
        position.x,
        position.y,
        size.width,
        size.height,
    )
}

#[tauri::command]
fn get_window_hover_state(window: WebviewWindow) -> bool {
    window_hovered(&window)
}

fn start_window_hover_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut previous = None;
        let mut interval = tokio::time::interval(Duration::from_millis(WINDOW_HOVER_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(window) = app.get_webview_window("pet") else {
                break;
            };
            let hovered = window_hovered(&window);
            if previous != Some(hovered) {
                let _ = window.emit("pet-window-hover", hovered);
                previous = Some(hovered);
            }
        }
    });
}

#[tauri::command]
fn close_settings_window(app: AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.destroy()?;
    }
    Ok(())
}

fn show_settings_window(app: &AppHandle) -> tauri::Result<()> {
    let _ = app.emit_to("pet", "close-inline-settings", ());
    let window = if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            SETTINGS_WINDOW_LABEL,
            WebviewUrl::App("index.html".into()),
        )
        .title("CoPets Settings")
        .inner_size(320.0, 520.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(true)
        .focused(true)
        .center()
        .build()?
    };
    window.center()?;
    window.show()?;
    window.set_focus()?;
    app.emit_to(SETTINGS_WINDOW_LABEL, "refresh-settings", ())?;
    Ok(())
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let settings_item = MenuItem::with_id(app, "settings", "Open Settings…", true, None::<&str>)?;
    let visibility_item = MenuItem::with_id(app, "visibility", "Hide pet", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&settings_item, &visibility_item, &separator, &quit_item],
    )?;
    let visibility_item_for_event = visibility_item.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .title("●")
        .tooltip("CoPets")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "settings" => {
                let _ = show_settings_window(app);
            }
            "visibility" => {
                if let Some(window) = app.get_webview_window("pet") {
                    let visible = window.is_visible().unwrap_or(true);
                    if visible {
                        let _ = window.hide();
                        let _ = visibility_item_for_event.set_text("Show pet");
                    } else {
                        let _ = window.show();
                        let _ = visibility_item_for_event.set_text("Hide pet");
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime = RuntimeHandle::default();
    tauri::Builder::default()
        .manage(runtime.clone())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            pet::list_pets,
            pet::load_pet,
            pet::preview_pet_import,
            pet::install_pet,
            pet::remove_pet,
            pet::open_pets_folder,
            observer::commands::get_runtime_state,
            observer::commands::get_control_state,
            observer::commands::perform_control_action,
            observer::commands::dismiss_control_notification,
            observer::commands::send_follow_up,
            observer::commands::stop_current_task,
            get_drag_pointer_snapshot,
            get_window_hover_state,
            close_settings_window,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup_tray(app)?;
            start_window_hover_monitor(app.handle().clone());
            observer::start(app.handle().clone(), runtime.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running CoPets");
}

#[cfg(test)]
mod tests {
    use super::point_in_window_bounds;

    #[test]
    fn hover_bounds_include_top_left_and_exclude_bottom_right() {
        assert!(point_in_window_bounds(10.0, 20.0, 10, 20, 100, 80));
        assert!(point_in_window_bounds(109.9, 99.9, 10, 20, 100, 80));
        assert!(!point_in_window_bounds(110.0, 50.0, 10, 20, 100, 80));
        assert!(!point_in_window_bounds(50.0, 100.0, 10, 20, 100, 80));
        assert!(!point_in_window_bounds(9.9, 50.0, 10, 20, 100, 80));
    }
}
