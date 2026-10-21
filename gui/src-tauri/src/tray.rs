//! System tray. The icon is greyed out while remapping is off and its tooltip names the
//! active config; middle-click toggles remapping. The menu opens the window, toggles
//! remapping and autodetect, activates a slot and quits. A watcher thread follows the
//! service's live state, so changes made in the window, over the CLI or by a mapped
//! "turn remapping off" command show here too.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_config::library::Library;
use dp_platform::Battery;
use dp_service::{LiveState, SLOTS, ServiceCmd};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::commands::AppState;

const TRAY_ID: &str = "main";
/// How often the watcher compares the live state with what the tray shows.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Windows keeps 128 UTF-16 units of a tooltip, including the terminating zero.
const TOOLTIP_MAX: usize = 127;

struct Texts {
    open: &'static str,
    remap: &'static str,
    autodetect: &'static str,
    slot: &'static str,
    empty: &'static str,
    remap_off: &'static str,
    no_config: &'static str,
    no_controller: &'static str,
    charging: &'static str,
    quit: &'static str,
}

fn texts(lang: &str) -> Texts {
    match lang {
        "de" => Texts {
            open: "Doppelpad öffnen",
            remap: "Remapping aktiv",
            autodetect: "Spiele automatisch erkennen",
            slot: "Slot",
            empty: "leer",
            remap_off: "Remapping aus",
            no_config: "Keine Config aktiv",
            no_controller: "Kein Controller verbunden",
            charging: "lädt",
            quit: "Beenden",
        },
        _ => Texts {
            open: "Open Doppelpad",
            remap: "Remapping on",
            autodetect: "Autodetect games",
            slot: "Slot",
            empty: "empty",
            remap_off: "Remapping off",
            no_config: "No config active",
            no_controller: "No controller connected",
            charging: "charging",
            quit: "Quit",
        },
    }
}

/// The part of the live state the tray shows.
#[derive(Clone, Debug, PartialEq)]
struct View {
    remap_on: bool,
    auto_switch: bool,
    active_slot: usize,
    slots: Vec<Option<String>>,
    profile_path: Option<String>,
    has_controller: bool,
    /// Name and battery of each controller that reports one.
    batteries: Vec<(String, Battery)>,
}

impl View {
    fn of(live: &LiveState) -> View {
        View {
            remap_on: live.remap_on,
            auto_switch: live.auto_switch,
            active_slot: live.active_slot,
            slots: live.slots.clone(),
            profile_path: live.profile_path.clone(),
            has_controller: !live.devices.is_empty(),
            batteries: live
                .devices
                .iter()
                .filter_map(|d| Some((d.name.clone(), d.battery.filter(|b| !b.wired)?)))
                .collect(),
        }
    }
}

/// The tray's menu entries and icons, updated in place.
struct Tray {
    live: Arc<Mutex<LiveState>>,
    open: MenuItem<Wry>,
    status: MenuItem<Wry>,
    remap: CheckMenuItem<Wry>,
    autodetect: CheckMenuItem<Wry>,
    slots: Vec<CheckMenuItem<Wry>>,
    quit: MenuItem<Wry>,
    icon_on: Image<'static>,
    icon_off: Image<'static>,
}

impl Tray {
    fn view(&self) -> View {
        View::of(&self.live.lock().unwrap())
    }
}

/// The tray glyph in grey and fainter, shown while remapping is off.
fn dimmed(icon: &Image<'_>) -> Image<'static> {
    let rgba = icon
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| {
            let grey = ((u32::from(r) * 30 + u32::from(g) * 59 + u32::from(b) * 11) / 100) as u8;
            [grey, grey, grey, (u32::from(a) * 3 / 5) as u8]
        })
        .collect();
    Image::new_owned(rgba, icon.width(), icon.height())
}

/// "Game › Config", like the slot tooltips in the window.
pub(crate) fn config_label(lib: &Library, path: &str) -> String {
    let path = Path::new(path);
    lib.games
        .iter()
        .find_map(|g| {
            g.configs
                .iter()
                .find(|c| c.path == path)
                .map(|c| format!("{} › {}", g.meta.name, c.name))
        })
        .or_else(|| {
            lib.unsorted
                .iter()
                .find(|c| c.path == path)
                .map(|c| c.name.clone())
        })
        .unwrap_or_else(|| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            )
        })
}

/// Menus take `&` as the mnemonic marker; a doubled one shows as itself.
fn menu_text(s: &str) -> String {
    s.replace('&', "&&")
}

/// Cut `s` to at most `max` UTF-16 units, marking the cut.
fn truncate_utf16(s: &str, max: usize) -> String {
    if s.encode_utf16().count() <= max {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        units += c.len_utf16();
        if units > max - 1 {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Bring every entry, the icon and the tooltip in line with the live state and language.
/// Runs on the main thread.
fn render(app: &AppHandle) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    let state = app.state::<AppState>();
    let view = tray.view();
    let t = texts(&state.settings.lock().unwrap().language);
    let lib = dp_config::library::scan(&state.profiles_dir);

    let active = view.profile_path.as_deref().map(|p| config_label(&lib, p));
    let _ = tray.open.set_text(t.open);
    let status = active.as_deref().unwrap_or(t.no_config);
    let _ = tray.status.set_text(menu_text(status));
    let _ = tray.remap.set_text(t.remap);
    let _ = tray.remap.set_checked(view.remap_on);
    let _ = tray.autodetect.set_text(t.autodetect);
    let _ = tray.autodetect.set_checked(view.auto_switch);
    for (i, item) in tray.slots.iter().enumerate() {
        let path = view.slots.get(i).cloned().flatten();
        let text = match &path {
            Some(p) => format!("{} {}: {}", t.slot, i + 1, config_label(&lib, p)),
            None => format!("{} {} ({})", t.slot, i + 1, t.empty),
        };
        let _ = item.set_text(menu_text(&text));
        let _ = item.set_enabled(path.is_some());
        let _ = item.set_checked(path.is_some() && view.remap_on && view.active_slot == i);
    }
    let _ = tray.quit.set_text(t.quit);

    if let Some(icon) = app.tray_by_id(TRAY_ID) {
        let image = if view.remap_on {
            &tray.icon_on
        } else {
            &tray.icon_off
        };
        let _ = icon.set_icon(Some(image.clone()));
        // The menu has the switch; the tooltip says it in words.
        let line = match &active {
            Some(label) => label,
            None if view.remap_on => t.no_config,
            None => t.remap_off,
        };
        let mut tip = format!("Doppelpad\n{line}");
        if !view.has_controller {
            tip = format!("{tip}\n{}", t.no_controller);
        }
        for (name, b) in &view.batteries {
            tip = format!("{tip}\n{name}: {} %", b.level);
            if b.charging {
                tip = format!("{tip} ({})", t.charging);
            }
        }
        let _ = icon.set_tooltip(Some(truncate_utf16(&tip, TOOLTIP_MAX)));
    }
}

/// Update the tray now (language, state or library changed). Callable from any thread.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || render(&handle));
}

/// Refresh the tray whenever the part of the live state it shows changes.
fn watch(app: AppHandle, live: Arc<Mutex<LiveState>>, mut shown: View) {
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let view = View::of(&live.lock().unwrap());
        if view != shown {
            shown = view;
            refresh(&app);
        }
    }
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

/// Send a command and show the outcome; the tray has no place for error messages, so
/// they go to the log.
fn command(app: &AppHandle, cmd: ServiceCmd) {
    if let Err(e) = app.state::<AppState>().call(cmd.clone()) {
        tracing::warn!("tray {cmd:?}: {e}");
    }
    // Clicked check items have toggled themselves; show the actual state.
    render(app);
}

fn toggle_remap(app: &AppHandle) {
    let on = !app.state::<Tray>().view().remap_on;
    command(app, ServiceCmd::SetRemap(on));
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main(app),
        "quit" => quit(app),
        "remap" => toggle_remap(app),
        "autodetect" => {
            let on = !app.state::<Tray>().view().auto_switch;
            command(app, ServiceCmd::SetAutoSwitch(on));
        }
        _ => {
            if let Some(slot) = id.strip_prefix("slot-").and_then(|n| n.parse().ok()) {
                command(app, ServiceCmd::SelectSlot { device: None, slot });
            }
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let live = app
        .state::<AppState>()
        .service
        .lock()
        .unwrap()
        .live_shared();
    let check = |id: &str| CheckMenuItem::with_id(app, id, "", true, false, None::<&str>);
    // A 32 px glyph without the app icon's tile, drawn for tray sizes (icons/source/tray-icon.svg).
    let icon_on = tauri::include_image!("icons/tray.png");
    let tray = Tray {
        open: MenuItem::with_id(app, "open", "", true, None::<&str>)?,
        status: MenuItem::with_id(app, "status", "", false, None::<&str>)?,
        remap: check("remap")?,
        autodetect: check("autodetect")?,
        slots: (0..SLOTS)
            .map(|i| check(&format!("slot-{i}")))
            .collect::<tauri::Result<_>>()?,
        quit: MenuItem::with_id(app, "quit", "", true, None::<&str>)?,
        icon_off: dimmed(&icon_on),
        icon_on,
        live: live.clone(),
    };

    let separators = [
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
    ];
    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![
        &tray.open,
        &separators[0],
        &tray.status,
        &tray.remap,
        &tray.autodetect,
        &separators[1],
    ];
    items.extend(tray.slots.iter().map(|s| s as &dyn IsMenuItem<Wry>));
    items.extend([&separators[2] as &dyn IsMenuItem<Wry>, &tray.quit]);
    let menu = Menu::with_items(app, &items)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray.icon_on.clone())
        .tooltip("Doppelpad")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                match button {
                    MouseButton::Left => show_main(app),
                    MouseButton::Middle => toggle_remap(app),
                    _ => {}
                }
            }
        })
        .build(app)?;

    let shown = tray.view();
    app.manage(tray);
    render(app);
    let handle = app.clone();
    std::thread::Builder::new()
        .name("dp-tray".into())
        .spawn(move || watch(handle, live, shown))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_are_cut_to_the_windows_limit() {
        assert_eq!(truncate_utf16("short", 127), "short");
        let long = "é".repeat(200);
        let cut = truncate_utf16(&long, 127);
        assert_eq!(cut.encode_utf16().count(), 127);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn ampersands_survive_menus() {
        assert_eq!(menu_text("Ratchet & Clank"), "Ratchet && Clank");
    }
}
