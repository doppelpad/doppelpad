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
mod updates;

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

/// In portable mode, the browser data of every window (cache, storage) beside the app too;
/// otherwise the default per-user folder.
pub(crate) fn webview_data_dir() -> Option<std::path::PathBuf> {
    dp_config::portable_dir().map(|dir| dir.join("webview"))
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
    let mut builder = WebviewWindowBuilder::from_config(app.handle(), config)?.visible(visible);
    if let Some(dir) = webview_data_dir() {
        builder = builder.data_directory(dir);
    }
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        // Centres the buttons in the 52 px header (see ProfilePanel.svelte). tao stretches
        // the title bar to button height + y, and AppKit keeps the 14 px buttons 9 px above
        // its bottom edge, so their centre lands at y - 2 (measured on macOS 26).
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 28.0));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    builder.build()
}

/// Maximize the main window if it is larger than the work area of its screen, such as a
/// 1280 × 800 handheld, where its right end with the window buttons would be off screen.
pub(crate) fn fit_to_screen(window: &WebviewWindow) {
    if window.is_maximized().unwrap_or(true) {
        return;
    }
    if let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.inner_size()) {
        let area = monitor.work_area().size;
        if size.width > area.width || size.height > area.height {
            let _ = window.maximize();
        }
    }
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

            app.manage(AppState {
                live: handle.live_shared(),
                service: handle,
                disk: Mutex::default(),
                settings: Mutex::new(GuiSettings::load()),
                profiles_dir: Mutex::new(profiles_dir),
                notices: Mutex::new(notices),
                open_files: Mutex::new(open_file::bundles_in(
                    &std::env::args().collect::<Vec<_>>(),
                    &std::env::current_dir().unwrap_or_default(),
                )),
                open_files_taken: AtomicBool::new(false),
                live_generation: Arc::new(AtomicU64::new(0)),
                main_focused: AtomicBool::new(false),
                min_size_held: AtomicBool::new(false),
            });
            #[cfg(target_os = "linux")]
            overlay::layer::detect();
            app.manage(overlay::Overlays::default());
            app.manage(osd::OsdState::default());
            app.manage(updates::PendingUpdate::default());
            tray::create(app.handle())?;
            overlay::spawn_watcher(app.handle(), "osd", osd::watch)?;
            overlay::spawn_watcher(app.handle(), "crosshair", crosshair::watch)?;
            overlay::spawn_watcher(app.handle(), "radial", radial::watch)?;
            overlay::spawn_watcher(app.handle(), "pad-overlay", pad_overlay::watch)?;

            // Started with Windows: only the tray icon shows.
            let visible = !autostart::started_hidden();
            tray::show_in_dock(app.handle(), visible);
            let window = create_main_window(app, visible)?;
            commands::apply_zoom(&window, &app.state::<AppState>());
            // Only after the zoom: on Windows, setting the minimum size un-maximizes the
            // window. A hidden window is fitted when it shows (`tray::show_main`).
            if visible {
                fit_to_screen(&window);
            }
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
                        tray::show_in_dock(&app_handle, false);
                        // A hidden window has no focus, whether or not Windows reports it.
                        state.set_main_focused(false);
                    }
                    WindowEvent::Resized(_) => {
                        if let Some(w) = app_handle.get_webview_window(MAIN_WINDOW) {
                            commands::apply_held_min_size(&w, &state);
                        }
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
            commands::game_launches,
            commands::game_import,
            commands::preset_list,
            commands::preset_save,
            commands::preset_load,
            commands::sheet_save,
            commands::config_load,
            commands::config_save,
            commands::config_check,
            commands::config_versions,
            commands::config_create,
            commands::config_duplicate,
            commands::config_delete,
            commands::config_import_launches,
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
            commands::rescan_devices,
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
            commands::library_place,
            commands::library_target,
            commands::library_move,
            commands::log_file,
            commands::take_notices,
            commands::take_open_files,
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
            } => tray::show_main(app),
            _ => {}
        });
}
