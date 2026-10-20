//! System tray icon: open the window, toggle remapping, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;

const TRAY_ID: &str = "main";

fn texts(lang: &str) -> (&'static str, &'static str, &'static str) {
    match lang {
        "de" => ("Doppelpad öffnen", "Remapping aktiv", "Beenden"),
        _ => ("Open Doppelpad", "Remapping on", "Quit"),
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let lang = state.settings.lock().unwrap().language.clone();
    let remap_on = state.service.lock().unwrap().live().remap_on;
    let (open, remap, quit) = texts(&lang);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", open, true, None::<&str>)?,
            &CheckMenuItem::with_id(app, "remap", remap, true, remap_on, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", quit, true, None::<&str>)?,
        ],
    )
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Stop the service (releasing outputs and unhiding devices), then exit.
pub fn quit(app: &AppHandle) {
    app.state::<AppState>().service.lock().unwrap().shutdown();
    app.exit(0);
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // A 32 px glyph without the app icon's tile, drawn for tray sizes (icons/source/tray-icon.svg).
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .tooltip("Doppelpad")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "remap" => {
                let state = app.state::<AppState>();
                let on = !state.service.lock().unwrap().live().remap_on;
                let _ = state.call(dp_service::ServiceCmd::SetRemap(on));
                refresh(app);
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuild the menu (language or remap state changed).
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}
