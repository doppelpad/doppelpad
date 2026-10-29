//! The editor window: created from its entry in `tauri.conf.json` with a title bar the app
//! draws, fitted to small screens, shown from the tray, and zoomed as the settings say.

use std::sync::atomic::Ordering;

use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

use crate::state::AppState;

/// The label of the editor window.
pub const MAIN_WINDOW: &str = "main";

/// Create the main window from its config entry, with the title bar drawn by the app:
/// macOS keeps its native traffic lights over the app's header, Windows and Linux get
/// no system frame and the header draws its own window buttons. A hidden window still
/// loads the frontend, so its start-up checks run.
pub fn create_main_window(app: &tauri::App, visible: bool) -> tauri::Result<WebviewWindow> {
    let config = main_config(app.config()).expect("main window config");
    let mut builder = WebviewWindowBuilder::from_config(app.handle(), config)?.visible(visible);
    if let Some(dir) = crate::webview_data_dir() {
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

/// The main window's entry in `tauri.conf.json`.
fn main_config(config: &tauri::Config) -> Option<&WindowConfig> {
    config.app.windows.iter().find(|w| w.label == MAIN_WINDOW)
}

/// Maximize the main window if it is larger than the work area of its screen, such as a
/// 1280 × 800 handheld, where its right end with the window buttons would be off screen.
pub fn fit_to_screen(window: &WebviewWindow) {
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

pub fn show_main(app: &AppHandle) {
    show_in_dock(app, true);
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        fit_to_screen(&w);
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// On macOS, have the app in the Dock while its window is open and only in the menu bar while
/// it runs on in the background, as Windows keeps it in the taskbar only with its window.
pub fn show_in_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(if show {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

fn settings_zoom(state: &AppState) -> u16 {
    state.settings(|s| s.zoom)
}

/// Show the window's contents at the zoom of the settings.
pub fn apply_zoom(window: &WebviewWindow, state: &AppState) {
    let scale = f64::from(settings_zoom(state)) / 100.0;
    if let Err(e) = window.set_zoom(scale) {
        tracing::warn!("cannot zoom the window: {e}");
    }
    // Setting the minimum size applies the window's size again, which on Windows takes a
    // maximized window out of maximized and leaves it at the maximized size. Such a window
    // gets its minimum size once it is restored (`apply_held_min_size`).
    if min_size_waits(window) {
        state.min_size_held.store(true, Ordering::Relaxed);
    } else {
        set_min_size(window, scale);
    }
}

/// Set the minimum size [`apply_zoom`] held back, once the window is restored.
pub fn apply_held_min_size(window: &WebviewWindow, state: &AppState) {
    if !min_size_waits(window) && state.min_size_held.swap(false, Ordering::Relaxed) {
        set_min_size(window, f64::from(settings_zoom(state)) / 100.0);
    }
}

/// Maximized or minimized: a minimum size set now would spoil the window's size.
fn min_size_waits(window: &WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(true) || window.is_minimized().unwrap_or(true)
}

/// Give the window its minimum size at 100 % (`minWidth`, `minHeight` in its config entry),
/// grown with the zoom so nothing is cut off.
fn set_min_size(window: &WebviewWindow, scale: f64) {
    let app = window.app_handle();
    let Some((width, height)) =
        main_config(app.config()).and_then(|c| c.min_width.zip(c.min_height))
    else {
        return;
    };
    let grow = scale.max(1.0);
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(width * grow, height * grow)));
}
