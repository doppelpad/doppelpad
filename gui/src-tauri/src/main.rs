//! Doppelpad desktop app. Hosts the remapping service (engine, slots, auto-switching,
//! IPC for the `doppelpad` CLI) and the editor window; closing the window keeps
//! remapping running in the tray.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod crosshair;
mod logging;
mod open_file;
mod osd;
mod overlay;
mod pad_overlay;
mod radial;
mod settings;
#[cfg(test)]
mod tests;
mod tray;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dp_service::{HostOptions, Ipc, ServiceOptions};
use tauri::{Emitter, Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::commands::{AppState, MAIN_WINDOW, Notice};
use crate::settings::GuiSettings;

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

/// Create the main window from its config entry, with the title bar drawn by the app:
/// macOS keeps its native traffic lights over the app's header, Windows and Linux get
/// no system frame and the header draws its own window buttons. A hidden window still
/// loads the frontend, so its start-up checks run.
fn create_main_window(app: &tauri::App, visible: bool) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_WINDOW)
        .expect("main window config");
    let builder = WebviewWindowBuilder::from_config(app.handle(), config)?.visible(visible);
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
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            tray::show_main(app);
            // A game bundle opened from Explorer while the app runs.
            let files = open_file::bundles_in(&args, std::path::Path::new(&cwd));
            if !files.is_empty() {
                let _ = app.emit_to(MAIN_WINDOW, open_file::OPEN_EVENT, files);
            }
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
            let service = ServiceOptions::with_user_files(profiles_dir.clone());
            // `doppelpad shutdown` is refused: the app stops its service when it quits.
            let handle = dp_service::spawn(HostOptions {
                service,
                ipc: Ipc::NoShutdown,
            })?;

            app.manage(AppState {
                live: handle.live_shared(),
                service: handle,
                disk: Mutex::default(),
                settings: Mutex::new(GuiSettings::load()),
                profiles_dir,
                notices: Mutex::new(notices),
                open_files: Mutex::new(open_file::bundles_in(
                    &std::env::args().collect::<Vec<_>>(),
                    &std::env::current_dir().unwrap_or_default(),
                )),
                live_generation: Arc::new(AtomicU64::new(0)),
                main_focused: AtomicBool::new(false),
            });
            app.manage(overlay::Overlays::default());
            app.manage(osd::OsdState::default());
            tray::create(app.handle())?;
            overlay::spawn_watcher(app.handle(), "osd", osd::watch)?;
            overlay::spawn_watcher(app.handle(), "crosshair", crosshair::watch)?;
            overlay::spawn_watcher(app.handle(), "radial", radial::watch)?;
            overlay::spawn_watcher(app.handle(), "pad-overlay", pad_overlay::watch)?;

            // Started with Windows: only the tray icon shows.
            let window = create_main_window(app, !autostart::started_hidden())?;
            commands::apply_zoom(
                &window,
                app.state::<AppState>().settings.lock().unwrap().zoom,
            );
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                let state = app_handle.state::<AppState>();
                match event {
                    // Pause remapping while editing so mapped keys don't reach the editor.
                    WindowEvent::Focused(focused) => state.set_main_focused(*focused),
                    // Closing hides to the tray; remapping keeps running.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window(MAIN_WINDOW) {
                            let _ = w.hide();
                        }
                        // A hidden window has no focus, whether or not Windows reports it.
                        state.set_main_focused(false);
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
            commands::game_duplicate,
            commands::running_programs,
            commands::game_set_picture,
            commands::game_picture,
            commands::foreground_program,
            commands::installed_games,
            commands::game_create_from_store,
            commands::game_create_template,
            commands::config_from_template,
            commands::open_driver_page,
            commands::game_export,
            commands::game_import,
            commands::preset_list,
            commands::preset_save,
            commands::preset_load,
            commands::sheet_save,
            commands::config_load,
            commands::config_save,
            commands::config_check,
            commands::config_create,
            commands::config_duplicate,
            commands::config_delete,
            commands::config_import,
            commands::reveal,
            commands::apply_to_slot,
            commands::select_slot,
            commands::clear_slot,
            commands::own_slots,
            commands::set_remap,
            commands::set_auto_switch,
            commands::set_lights,
            commands::set_blocked,
            commands::set_groups,
            commands::set_power_off_after,
            commands::power_off,
            commands::set_layout,
            commands::forget_layout,
            commands::set_dsu,
            commands::set_runtime_options,
            commands::set_http,
            commands::new_http_token,
            commands::set_preferred_device,
            commands::refresh_drivers,
            commands::hotkeys_get,
            commands::hotkeys_set,
            commands::hotkeys_pause,
            commands::test_turn,
            commands::gyro_calibrate,
            commands::gyro_auto,
            commands::gyro_reset,
            commands::rewasd_status,
            commands::rewasd_quit,
            commands::live_subscribe,
            commands::settings_get,
            commands::settings_set,
            commands::autostart_get,
            commands::autostart_set,
            commands::window_show,
            commands::profiles_dir,
            commands::log_file,
            commands::take_notices,
            commands::take_open_files,
            overlay::overlay_ready,
            osd::osd_test,
            osd::osd_monitors,
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
