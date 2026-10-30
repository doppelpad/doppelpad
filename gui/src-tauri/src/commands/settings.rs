//! The app's own settings (`gui.toml`) and starting with the system.

use tauri::{AppHandle, Manager, State};

use super::{CmdResult, err};
use crate::main_window::{MAIN_WINDOW, apply_zoom};
use crate::overlay;
use crate::settings::GuiSettings;
use crate::state::AppState;

/// The settings, with the notification monitor's index where that monitor is now.
#[tauri::command]
pub fn settings_get(app: AppHandle, state: State<AppState>) -> GuiSettings {
    let mut settings = state.settings(GuiSettings::clone);
    settings.osd.monitor = settings.osd.monitor_index(&overlay::screens(&app));
    settings
}

#[tauri::command(async)]
pub fn settings_set(
    app: AppHandle,
    state: State<AppState>,
    settings: GuiSettings,
) -> CmdResult<()> {
    let mut settings = settings.clamped();
    let old = state.settings(|s| s.osd.clone());
    settings.osd = settings.osd.with_screen(&old, &overlay::screens(&app));
    let zoom = settings.zoom;
    let zoom_changed = state.save_settings(settings).map_err(err)?.zoom != zoom;
    // "Keep remapping active" may have changed.
    state.sync_suspend()?;
    crate::tray::refresh(&app);
    // Only on a change: the zoom sets the window's minimum size, which applies its size again.
    if zoom_changed && let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        apply_zoom(&window, &state);
    }
    Ok(())
}

/// Whether the app starts hidden in the tray when the user signs in; `None` where it
/// cannot (Linux, a portable copy). Read from the system each time: Task Manager or the login
/// items can change it too.
#[tauri::command]
pub fn autostart_get(app: AppHandle) -> Option<bool> {
    crate::autostart::enabled(&app)
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, on: bool) -> CmdResult<()> {
    crate::autostart::set(&app, on)
}
