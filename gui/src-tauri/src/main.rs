//! Doppelpad desktop app. Hosts the remapping service (engine, slots, auto-switching,
//! IPC for the `doppelpad` CLI) and the editor window; closing the window keeps
//! remapping running in the tray.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod logging;
mod settings;
#[cfg(test)]
mod tests;
mod tray;

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_service::{HostOptions, ServiceCmd, ServiceOptions};
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::commands::AppState;
use crate::settings::GuiSettings;

/// If a `doppelpad daemon` (or another instance) already serves remapping, ask it to stop
/// so this app can take over the devices.
fn take_over_running_service(notices: &mut Vec<String>) {
    if dp_ipc::call(&dp_ipc::Request::Status).is_ok() {
        let _ = dp_ipc::call(&dp_ipc::Request::Shutdown);
        std::thread::sleep(Duration::from_millis(400));
        notices.push("service-takeover".into());
    }
}

/// Create the main window from its config entry, with the title bar drawn by the app:
/// macOS keeps its native traffic lights over the app's header, Windows and Linux get
/// no system frame and the header draws its own window buttons.
fn create_main_window(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .expect("main window config");
    let builder = WebviewWindowBuilder::from_config(app.handle(), config)?;
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        // Centres the buttons in the 52 px header (see ProfilePanel.svelte).
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 20.0));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    builder.build()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            logging::init();
            tracing::info!(
                "Doppelpad {} on {}",
                env!("CARGO_PKG_VERSION"),
                dp_platform::os_version()
            );
            let mut notices = Vec::new();
            take_over_running_service(&mut notices);

            let profiles_dir = dp_config::profiles_dir();
            let mut service = ServiceOptions::new(profiles_dir.clone());
            service.state_path = Some(dp_config::config_dir().join("state.toml"));
            let handle = dp_service::spawn(HostOptions { service, ipc: true })?;

            app.manage(AppState {
                service: Mutex::new(handle),
                settings: Mutex::new(GuiSettings::load()),
                profiles_dir,
                notices: Mutex::new(notices),
                live_generation: Arc::new(AtomicU64::new(0)),
            });
            tray::create(app.handle())?;

            let window = create_main_window(app)?;
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                let state = app_handle.state::<AppState>();
                match event {
                    // Pause remapping while editing so mapped keys don't reach the editor.
                    WindowEvent::Focused(focused) => {
                        let stay_live = state.settings.lock().unwrap().stay_live;
                        let _ = state.call(ServiceCmd::SetSuspended(*focused && !stay_live));
                    }
                    // Closing hides to the tray; remapping keeps running.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        let _ = state.call(ServiceCmd::SetSuspended(false));
                    }
                    _ => {}
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog,
            commands::library_scan,
            commands::game_create,
            commands::game_update,
            commands::game_delete,
            commands::config_load,
            commands::config_save,
            commands::config_create,
            commands::config_duplicate,
            commands::config_rename,
            commands::config_delete,
            commands::config_import,
            commands::reveal,
            commands::apply_to_slot,
            commands::select_slot,
            commands::clear_slot,
            commands::set_remap,
            commands::set_auto_switch,
            commands::set_preferred_device,
            commands::refresh_drivers,
            commands::rewasd_status,
            commands::rewasd_quit,
            commands::live_subscribe,
            commands::settings_get,
            commands::settings_set,
            commands::profiles_dir,
            commands::log_file,
            commands::take_notices,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
            {
                // Keep running in the tray when the last window closes; Quit exits explicitly.
                api.prevent_exit();
                let _ = app;
            }
        });
}
