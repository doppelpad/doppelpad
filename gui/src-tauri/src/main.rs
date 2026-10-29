//! Doppelpad desktop app. Hosts the remapping service (engine, slots, auto-switching,
//! IPC for the `doppelpad` CLI) and the editor window; closing the window keeps
//! remapping running in the tray.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod crosshair;
mod error;
mod logging;
mod main_window;
mod open_file;
mod osd;
mod overlay;
mod pad_overlay;
mod radial;
mod settings;
mod state;
#[cfg(test)]
mod tests;
mod tray;
mod updates;

use std::time::{Duration, Instant};

use dp_service::{HostOptions, Ipc, ServiceOptions};
use tauri::{Manager, WindowEvent};

use crate::commands::system::Notice;
use crate::main_window::MAIN_WINDOW;
use crate::settings::GuiSettings;
use crate::state::AppState;

/// How often, and at most how long, the app asks whether a service it stopped is gone.
const TAKEOVER_POLL: Duration = Duration::from_millis(50);
const TAKEOVER_WAIT: Duration = Duration::from_secs(3);

/// If a `doppelpad daemon` (or another instance) already serves remapping, ask it to stop
/// and wait until it no longer answers, so this app can take over the devices.
fn take_over_running_service(notices: &mut Vec<Notice>) {
    let answers = || dp_ipc::call(&dp_ipc::Request::Status).is_ok();
    if answers() {
        let _ = dp_ipc::call(&dp_ipc::Request::Shutdown);
        let deadline = Instant::now() + TAKEOVER_WAIT;
        while answers() && Instant::now() < deadline {
            std::thread::sleep(TAKEOVER_POLL);
        }
        notices.push(Notice::ServiceTakeover);
    }
}

/// In portable mode, the browser data of every window (cache, storage) beside the app too;
/// otherwise the default per-user folder.
pub(crate) fn webview_data_dir() -> Option<std::path::PathBuf> {
    dp_config::portable_dir().map(|dir| dir.join("webview"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            main_window::show_main(app);
            // A game bundle opened from Explorer while the app runs (or still starts).
            open_file::opened(
                app,
                open_file::bundles_in(&args, std::path::Path::new(&cwd)),
            );
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            let service = ServiceOptions::with_user_files(profiles_dir.clone());
            // `doppelpad shutdown` is refused: the app stops its service when it quits.
            let handle = dp_service::spawn(HostOptions {
                service,
                ipc: Ipc::NoShutdown,
            })?;

            let open_files = open_file::bundles_in(
                &std::env::args().collect::<Vec<_>>(),
                &std::env::current_dir().unwrap_or_default(),
            );
            app.manage(AppState::new(
                handle,
                GuiSettings::load(),
                profiles_dir,
                notices,
                open_files,
            ));
            #[cfg(target_os = "linux")]
            overlay::layer::detect();
            app.manage(overlay::Overlays::default());
            app.manage(osd::OsdState::default());
            app.manage(updates::PendingUpdate::default());
            tray::create(app.handle())?;
            overlay::spawn_watcher(app.handle(), "osd", osd::watch)?;
            overlay::spawn_watcher(app.handle(), "steam-hint", osd::watch_steam)?;
            overlay::spawn_watcher(app.handle(), "crosshair", crosshair::watch)?;
            overlay::spawn_watcher(app.handle(), "radial", radial::watch)?;
            overlay::spawn_watcher(app.handle(), "pad-overlay", pad_overlay::watch)?;

            // Started with Windows: only the tray icon shows.
            let visible = !autostart::started_hidden();
            main_window::show_in_dock(app.handle(), visible);
            let window = main_window::create_main_window(app, visible)?;
            main_window::apply_zoom(&window, &app.state::<AppState>());
            // Only after the zoom: on Windows, setting the minimum size un-maximizes the
            // window. A hidden window is fitted when it shows (`main_window::show_main`).
            if visible {
                main_window::fit_to_screen(&window);
            }
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                let state = app_handle.state::<AppState>();
                match event {
                    // Pause remapping while editing so mapped keys don't reach the editor.
                    WindowEvent::Focused(focused) => state.set_main_focused(&app_handle, *focused),
                    // Closing hides to the tray; remapping keeps running.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window(MAIN_WINDOW) {
                            let _ = w.hide();
                        }
                        main_window::show_in_dock(&app_handle, false);
                        // A hidden window has no focus, whether or not Windows reports it.
                        state.set_main_focused(&app_handle, false);
                    }
                    WindowEvent::Resized(_) => {
                        if let Some(w) = app_handle.get_webview_window(MAIN_WINDOW) {
                            main_window::apply_held_min_size(&w, &state);
                        }
                    }
                    _ => {}
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::catalog,
            commands::library::library_scan,
            commands::library::game_create,
            commands::library::game_update,
            commands::library::game_delete,
            commands::library::game_duplicate,
            commands::system::running_programs,
            commands::library::game_set_picture,
            commands::library::game_picture,
            commands::system::foreground_program,
            commands::system::installed_games,
            commands::library::game_create_from_store,
            commands::library::game_create_template,
            commands::library::config_from_template,
            commands::system::open_driver_page,
            commands::library::game_export,
            commands::library::game_launches,
            commands::library::game_import,
            commands::library::preset_list,
            commands::library::preset_save,
            commands::library::preset_load,
            commands::library::sheet_save,
            commands::library::config_load,
            commands::library::config_save,
            commands::library::config_check,
            commands::library::config_versions,
            commands::library::config_create,
            commands::library::config_duplicate,
            commands::library::config_delete,
            commands::library::config_import_launches,
            commands::library::config_import,
            commands::system::reveal,
            commands::service::apply_to_slot,
            commands::service::select_slot,
            commands::service::clear_slot,
            commands::service::own_slots,
            commands::service::set_remap,
            commands::service::set_auto_switch,
            commands::service::set_lights,
            commands::service::set_blocked,
            commands::service::set_groups,
            commands::service::set_power_off_after,
            commands::service::set_switch2_bluetooth,
            commands::service::power_off,
            commands::service::set_layout,
            commands::service::forget_layout,
            commands::service::set_dsu,
            commands::service::set_runtime_options,
            commands::service::set_http,
            commands::service::new_http_token,
            commands::service::set_preferred_device,
            commands::service::refresh_drivers,
            commands::service::rescan_devices,
            commands::service::hotkeys_get,
            commands::service::hotkeys_set,
            commands::service::hotkeys_pause,
            commands::service::test_turn,
            commands::service::gyro_calibrate,
            commands::service::gyro_auto,
            commands::service::gyro_reset,
            commands::system::rewasd_status,
            commands::system::rewasd_quit,
            commands::service::live_subscribe,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::autostart_get,
            commands::settings::autostart_set,
            commands::system::window_show,
            commands::library::library_place,
            commands::library::library_target,
            commands::library::library_move,
            commands::system::log_file,
            commands::system::take_notices,
            commands::system::take_open_files,
            updates::update_check,
            updates::update_install,
            overlay::overlay_ready,
            osd::osd_test,
            osd::osd_monitors,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| match event {
            // Keep running in the tray when the last window closes; Quit exits explicitly.
            tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
                api.prevent_exit();
                let _ = app;
            }
            // A game bundle opened from the Finder, at start-up or later.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Opened { urls } => {
                let files = urls.iter().filter_map(|u| u.to_file_path().ok()).collect();
                open_file::opened(app, files);
            }
            // The Dock icon clicked while the window is closed.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => main_window::show_main(app),
            _ => {}
        });
}
