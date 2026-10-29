//! The app's own settings (`gui.toml`) and starting with the system.

use tauri::{AppHandle, Manager, State};

use super::{CmdResult, err};
use crate::main_window::{MAIN_WINDOW, apply_zoom};
use crate::settings::GuiSettings;
use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> GuiSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command(async)]
pub fn settings_set(
    app: AppHandle,
    state: State<AppState>,
    settings: GuiSettings,
) -> CmdResult<()> {
    let settings = settings.clamped();
    let zoom_changed = {
        let mut current = state.settings.lock().unwrap();
        settings.save().map_err(err)?;
        let changed = current.zoom != settings.zoom;
        *current = settings;
        changed
    };
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
/// cannot (not Windows). Read from Windows each time: Task Manager can change it too.
#[tauri::command]
pub fn autostart_get() -> Option<bool> {
    crate::autostart::enabled()
}

#[tauri::command]
pub fn autostart_set(on: bool) -> CmdResult<()> {
    crate::autostart::set(on).map_err(err)
}
