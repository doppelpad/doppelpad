//! System tray. The icon is greyed out while remapping is off and its tooltip names the
//! active config; middle-click toggles remapping. The menu opens the window, toggles
//! remapping and autodetect, activates a slot and quits. A watcher thread follows the
//! service's live state, so changes made in the window, over the CLI or by a mapped
//! "turn remapping off" command show here too.
//!
//! Optionally each connected controller gets an icon of its own besides the main one, in a
//! colour of its own: its tooltip names the controller, its config and battery, its menu offers
//! its slots.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use dp_platform::Battery;
use dp_service::{LiveState, SLOTS, ServiceCmd, SharedLive};
use serde::Deserialize;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{AppState, MAIN_WINDOW};
use crate::settings::parse_color;

const TRAY_ID: &str = "main";
/// How often the watcher compares the live state with what the tray shows.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Windows keeps 128 UTF-16 units of a tooltip, including the terminating zero.
const TOOLTIP_MAX: usize = 127;

/// The frontend's catalogs by language, `(language, JSON)` (listed by build.rs).
const CATALOGS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// The tray's texts: `tray.*` in the catalogs, whose parity test keeps every language complete.
#[derive(Debug, Deserialize)]
struct Texts {
    open: String,
    remap: String,
    autodetect: String,
    slot: String,
    empty: String,
    remap_off: String,
    no_config: String,
    no_controller: String,
    charging: String,
    quit: String,
    not_remapped: String,
    own_slots: String,
}

static TEXTS: LazyLock<Vec<(&str, Texts)>> = LazyLock::new(|| {
    #[derive(Deserialize)]
    struct Catalog {
        tray: Texts,
    }
    CATALOGS
        .iter()
        .map(|&(lang, json)| {
            let c: Catalog = serde_json::from_str(json).expect("a catalog with a tray section");
            (lang, c.tray)
        })
        .collect()
});

/// The texts in `lang`, English for a language without a catalog.
fn texts(lang: &str) -> &'static Texts {
    let of = |lang: &str| TEXTS.iter().find(|(l, _)| *l == lang).map(|(_, t)| t);
    of(lang).or_else(|| of("en")).expect("the English catalog")
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
    /// What the controllers' own icons show (colours left out: settings changes refresh).
    pads: Vec<PadView>,
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
            pads: pad_views(live, &BTreeMap::new()),
        }
    }
}

/// Colours for controllers' icons that have none chosen, picked by device id.
const PAD_COLORS: [[u8; 3]; 6] = [
    [0x3b, 0x82, 0xf6],
    [0xef, 0x44, 0x44],
    [0x22, 0xc5, 0x5e],
    [0xec, 0x48, 0x99],
    [0xf5, 0x9e, 0x0b],
    [0xa8, 0x55, 0xf7],
];

/// The colour of a controller's icon: the chosen one, else one from the palette by its id
/// (the same every time).
fn pad_color(id: &str, chosen: &BTreeMap<String, String>) -> [u8; 3] {
    chosen
        .get(id)
        .and_then(|c| parse_color(c))
        .unwrap_or_else(|| {
            // FNV-1a: stable across runs, unlike the standard hasher.
            let hash = id.bytes().fold(0x811c_9dc5_u32, |h, b| {
                (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
            });
            PAD_COLORS[hash as usize % PAD_COLORS.len()]
        })
}

/// What a controller's own icon shows.
#[derive(Clone, Debug, PartialEq)]
struct PadView {
    id: String,
    name: String,
    color: [u8; 3],
    battery: Option<Battery>,
    remap_on: bool,
    /// The config it runs.
    config: Option<String>,
    /// Its slots of its own, or the shared ones while it is the controller in use; `None` when
    /// it uses neither (it is not remapped).
    slots: Option<Vec<Option<String>>>,
    active_slot: usize,
}

/// Each connected controller as its icon shows it.
fn pad_views(live: &LiveState, colors: &BTreeMap<String, String>) -> Vec<PadView> {
    live.devices
        .iter()
        .map(|d| {
            let own = live.device_slots.iter().find(|s| s.id == d.id);
            let (config, slots, active_slot) = if let Some(own) = own {
                let session = live
                    .sessions
                    .iter()
                    .find(|s| s.owner.as_ref() == Some(&d.id));
                let config = session.and_then(|s| s.profile_path.clone());
                (config, Some(own.slots.clone()), own.active_slot)
            } else if live.primary.as_ref() == Some(&d.id) {
                (
                    live.profile_path.clone(),
                    Some(live.slots.clone()),
                    live.active_slot,
                )
            } else {
                (None, None, 0)
            };
            PadView {
                id: d.id.clone(),
                name: d.name.clone(),
                color: pad_color(&d.id, colors),
                battery: d.battery.filter(|b| !b.wired),
                remap_on: live.remap_on,
                config: config.filter(|_| live.remap_on),
                slots,
                active_slot,
            }
        })
        .collect()
}

/// The tooltip of a controller's icon: the controller, what it runs, its battery.
fn pad_tooltip(v: &PadView, t: &Texts) -> String {
    let line = match (&v.config, &v.slots) {
        _ if !v.remap_on => t.remap_off.clone(),
        (Some(path), _) => config_label(path),
        (None, Some(_)) => t.no_config.clone(),
        (None, None) => t.not_remapped.clone(),
    };
    let mut tip = format!("{}\n{line}", v.name);
    if let Some(b) = v.battery {
        tip = format!("{tip}\n{} %", b.level);
        if b.charging {
            tip = format!("{tip} ({})", t.charging);
        }
    }
    truncate_utf16(&tip, TOOLTIP_MAX)
}

/// The tray id of a controller's icon.
fn pad_tray_id(id: &str) -> String {
    format!("pad|{id}")
}

/// The glyph in one colour, shaded by its own brightness, for a controller's icon.
fn tinted(icon: &Image<'_>, [r, g, b]: [u8; 3]) -> Image<'static> {
    let rgba = icon
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[pr, pg, pb, a]| {
            let lum = (u32::from(pr) * 30 + u32::from(pg) * 59 + u32::from(pb) * 11) / 100;
            let k = 140 + lum * 115 / 255;
            let ch = |c: u8| (u32::from(c) * k / 255) as u8;
            [ch(r), ch(g), ch(b), a]
        })
        .collect();
    Image::new_owned(rgba, icon.width(), icon.height())
}

/// The tray's menu entries and icons, updated in place.
struct Tray {
    live: SharedLive,
    open: MenuItem<Wry>,
    status: MenuItem<Wry>,
    remap: CheckMenuItem<Wry>,
    autodetect: CheckMenuItem<Wry>,
    slots: Vec<CheckMenuItem<Wry>>,
    quit: MenuItem<Wry>,
    icon_on: Image<'static>,
    icon_off: Image<'static>,
    /// The controllers' icons shown, with what they show and in which language.
    pads: Mutex<HashMap<String, (PadView, String)>>,
}

impl Tray {
    fn view(&self) -> View {
        View::of(&self.live.get())
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

/// "Game › Config", like the slot names in the window.
fn config_label(path: &str) -> String {
    dp_config::library::config_label(Path::new(path))
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
    let settings = state.settings.lock().unwrap().clone();
    let t = texts(&settings.language);
    let pads = if settings.tray_per_controller {
        pad_views(&tray.live.get(), &settings.controller_colors)
    } else {
        Vec::new()
    };
    render_pads(app, &tray, pads, &settings.language, t);

    let active = view.profile_path.as_deref().map(config_label);
    let _ = tray.open.set_text(&t.open);
    let status = active.as_deref().unwrap_or(&t.no_config);
    let _ = tray.status.set_text(menu_text(status));
    let _ = tray.remap.set_text(&t.remap);
    let _ = tray.remap.set_checked(view.remap_on);
    let _ = tray.autodetect.set_text(&t.autodetect);
    let _ = tray.autodetect.set_checked(view.auto_switch);
    for (i, item) in tray.slots.iter().enumerate() {
        let path = view.slots.get(i).cloned().flatten();
        let text = match &path {
            Some(p) => format!("{} {}: {}", t.slot, i + 1, config_label(p)),
            None => format!("{} {} ({})", t.slot, i + 1, t.empty),
        };
        let _ = item.set_text(menu_text(&text));
        let _ = item.set_enabled(path.is_some());
        let _ = item.set_checked(path.is_some() && view.remap_on && view.active_slot == i);
    }
    let _ = tray.quit.set_text(&t.quit);

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
            None if view.remap_on => &t.no_config,
            None => &t.remap_off,
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

/// Show an icon for each controller in `pads` and none for the others; only icons whose
/// content changed are touched.
fn render_pads(app: &AppHandle, tray: &Tray, pads: Vec<PadView>, lang: &str, t: &Texts) {
    let mut shown = tray.pads.lock().unwrap();
    shown.retain(|id, _| {
        let keep = pads.iter().any(|p| &p.id == id);
        if !keep {
            let _ = app.remove_tray_by_id(&pad_tray_id(id));
        }
        keep
    });
    for v in pads {
        if shown
            .get(&v.id)
            .is_some_and(|(old, l)| old == &v && l == lang)
        {
            continue;
        }
        let image = tinted(&tray.icon_on, v.color);
        let image = if v.remap_on { image } else { dimmed(&image) };
        let tip = pad_tooltip(&v, t);
        match pad_menu(app, &v, t) {
            Ok(menu) => {
                if let Some(icon) = app.tray_by_id(&pad_tray_id(&v.id)) {
                    let _ = icon.set_icon(Some(image));
                    let _ = icon.set_tooltip(Some(tip));
                    let _ = icon.set_menu(Some(menu));
                } else if let Err(e) = TrayIconBuilder::with_id(pad_tray_id(&v.id))
                    .icon(image)
                    .tooltip(tip)
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(on_icon_event)
                    .build(app)
                {
                    tracing::warn!("cannot show the tray icon of {}: {e}", v.name);
                }
            }
            Err(e) => tracing::warn!("cannot build the tray menu of {}: {e}", v.name),
        }
        shown.insert(v.id.clone(), (v, lang.to_owned()));
    }
}

/// A controller's menu: its name and config, its slots (or a way to give it some), Open.
fn pad_menu(app: &AppHandle, v: &PadView, t: &Texts) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    let id = &v.id;
    menu.append(&MenuItem::new(
        app,
        menu_text(&v.name),
        false,
        None::<&str>,
    )?)?;
    let status = pad_tooltip(v, t);
    let line = status.lines().nth(1).unwrap_or_default();
    menu.append(&MenuItem::new(app, menu_text(line), false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    match &v.slots {
        Some(slots) => {
            for (i, path) in slots.iter().enumerate() {
                let text = match path {
                    Some(p) => format!("{} {}: {}", t.slot, i + 1, config_label(p)),
                    None => format!("{} {} ({})", t.slot, i + 1, t.empty),
                };
                let checked = path.is_some() && v.remap_on && v.active_slot == i;
                let item = CheckMenuItem::with_id(
                    app,
                    format!("{}|slot|{i}", pad_tray_id(id)),
                    menu_text(&text),
                    path.is_some(),
                    checked,
                    None::<&str>,
                )?;
                menu.append(&item)?;
            }
        }
        None => {
            let own = format!("{}|own", pad_tray_id(id));
            menu.append(&MenuItem::with_id(
                app,
                own,
                &t.own_slots,
                true,
                None::<&str>,
            )?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        &t.open,
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

/// A controller's menu entry: `pad|<device>|slot|<n>` or `pad|<device>|own`.
fn pad_command(menu_id: &str) -> Option<ServiceCmd> {
    let rest = menu_id.strip_prefix("pad|")?;
    if let Some(device) = rest.strip_suffix("|own") {
        return Some(ServiceCmd::OwnSlots {
            device: device.to_owned(),
            on: true,
        });
    }
    let (device, slot) = rest.rsplit_once("|slot|")?;
    Some(ServiceCmd::SelectSlot {
        device: Some(device.to_owned()),
        slot: slot.parse().ok()?,
    })
}

/// Clicks on an icon: left opens the window, middle switches remapping.
fn on_icon_event(tray: &tauri::tray::TrayIcon<Wry>, event: TrayIconEvent) {
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
}

/// Update the tray now (language, state or library changed). Callable from any thread.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || render(&handle));
}

/// Refresh the tray whenever the part of the live state it shows changes.
fn watch(app: AppHandle, live: SharedLive, mut shown: View) {
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let view = View::of(&live.get());
        if view != shown {
            shown = view;
            refresh(&app);
        }
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Stop the service (releasing outputs and unhiding devices), then exit.
pub fn quit(app: &AppHandle) {
    app.state::<AppState>().service.shutdown();
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
            } else if let Some(cmd) = pad_command(id) {
                command(app, cmd);
            }
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let live = app.state::<AppState>().live.clone();
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
        live,
        pads: Mutex::default(),
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
        // Called for every menu, the controllers' icons' too.
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(on_icon_event)
        .build(app)?;

    let shown = tray.view();
    app.manage(tray);
    render(app);
    crate::overlay::spawn_watcher(app, "dp-tray", move |app, live| watch(app, live, shown))
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
    fn the_texts_come_from_every_catalog() {
        let langs: Vec<&str> = CATALOGS.iter().map(|(l, _)| *l).collect();
        assert!(langs.contains(&"en") && langs.contains(&"de"), "{langs:?}");
        assert_eq!(TEXTS.len(), CATALOGS.len());
        assert_eq!(texts("de").quit, "Beenden");
        assert_eq!(texts("xx").quit, "Quit", "English for an unknown language");
    }

    fn live_with_two_pads() -> LiveState {
        use dp_service::{LiveDevice, LiveDeviceSlots, SessionInfo, SessionOutput};
        let pad = |id: &str, name: &str| LiveDevice {
            id: id.into(),
            name: name.into(),
            family: dp_platform::PadFamily::Xbox,
            backend: "hid".into(),
            vendor_id: 0,
            product_id: 0,
            state: dp_core::pad::PadState::default(),
            gyro_calibration: None,
            battery: Some(Battery {
                level: 60,
                charging: false,
                wired: false,
            }),
            raw: None,
        };
        LiveState {
            devices: vec![
                pad("hid:a", "DualSense"),
                pad("hid:b", "Pro Controller"),
                pad("xinput:0", "Xbox"),
            ],
            primary: Some("hid:a".into()),
            remap_on: true,
            profile_path: Some("C:/p/Halo/main.toml".into()),
            slots: vec![Some("C:/p/Halo/main.toml".into()), None, None, None],
            device_slots: vec![LiveDeviceSlots {
                id: "hid:b".into(),
                slots: vec![None, Some("C:/p/Kart/race.toml".into()), None, None],
                active_slot: 1,
            }],
            sessions: vec![SessionInfo {
                owner: Some("hid:b".into()),
                device: Some("hid:b".into()),
                profile: "Race".into(),
                profile_path: Some("C:/p/Kart/race.toml".into()),
                layer: 0,
                virtual_pad: None,
                output: SessionOutput::default(),
            }],
            ..LiveState::default()
        }
    }

    #[test]
    fn each_controller_shows_what_it_runs() {
        let live = live_with_two_pads();
        let colors = BTreeMap::from([("hid:b".to_owned(), "#123456".to_owned())]);
        let pads = pad_views(&live, &colors);
        let t = texts("en");
        let [a, b, x] = &pads[..] else {
            panic!("{pads:?}")
        };
        // The controller in use runs the shared slots.
        assert_eq!(a.slots.as_ref().map(Vec::len), Some(4));
        assert!(pad_tooltip(a, t).starts_with("DualSense\n"));
        assert!(pad_tooltip(a, t).ends_with("60 %"));
        // One with slots of its own runs its own config.
        assert_eq!(b.config.as_deref(), Some("C:/p/Kart/race.toml"));
        assert_eq!((b.active_slot, b.color), (1, [0x12, 0x34, 0x56]));
        // The others are not remapped.
        assert_eq!(x.slots, None);
        assert!(pad_tooltip(x, t).contains(&t.not_remapped));
        assert_eq!(
            pad_color(&x.id, &colors),
            pad_color("xinput:0", &BTreeMap::new())
        );
        // The same values as padColors.test.ts, so the settings show the icon's colour.
        assert_eq!(pad_color("xinput:0", &BTreeMap::new()), [0xa8, 0x55, 0xf7]);
        assert_eq!(pad_color("hid:a", &BTreeMap::new()), [0xec, 0x48, 0x99]);

        let off = LiveState {
            remap_on: false,
            ..live
        };
        let pads = pad_views(&off, &colors);
        assert!(pad_tooltip(&pads[1], t).contains(&t.remap_off));
    }

    #[test]
    fn controller_menus_reach_their_slots() {
        assert_eq!(
            pad_command("pad|hid:b|slot|2"),
            Some(ServiceCmd::SelectSlot {
                device: Some("hid:b".into()),
                slot: 2
            })
        );
        assert_eq!(
            pad_command("pad|xinput:0|own"),
            Some(ServiceCmd::OwnSlots {
                device: "xinput:0".into(),
                on: true
            })
        );
        assert_eq!(pad_command("slot-1"), None);
        assert_eq!(pad_command("pad|x|slot|nine"), None);
    }

    #[test]
    fn ampersands_survive_menus() {
        assert_eq!(menu_text("Ratchet & Clank"), "Ratchet && Clank");
    }
}
