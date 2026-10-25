//! On-screen notifications over games: remapping on/off, slot or config changes, layer
//! changes, controllers coming and going, and screenshots. A watcher thread follows the service's live
//! state like the tray does and shows each change for a few seconds in the `osd` overlay
//! window; the overlay page words it in the app's language. Nothing shows while the main
//! window has focus (it shows the same state) or in the first seconds after start.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dp_config::library;
use dp_platform::Battery;
use dp_service::{LiveScreenshot, LiveState, SharedLive};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{AppState, MAIN_WINDOW};
use crate::overlay::{self, OverlayKind, Placement};
use crate::settings::{Corner, OsdSettings};

const KIND: OverlayKind = OverlayKind::Osd;
/// How often the watcher compares the live state with what it last saw.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Controllers connecting while the app starts are not news.
const STARTUP_QUIET: Duration = Duration::from_secs(3);
/// Window size at 100 %, in logical pixels.
const SIZE: (f64, f64) = (380.0, 84.0);
/// A screenshot's message goes away by then at the latest, so that the next screenshot does
/// not show it.
const SCREENSHOT_KEEP: Duration = Duration::from_secs(2);

/// What a notification is about.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    RemapOn,
    RemapOff,
    /// Slot 1–4 became active or got another config (`config`: its label).
    Slot {
        slot: usize,
        config: Option<String>,
    },
    Layer {
        layer: u8,
        name: Option<String>,
    },
    Connected {
        name: String,
    },
    Disconnected {
        name: String,
    },
    BatteryLow {
        name: String,
        level: u8,
    },
    BatteryCritical {
        name: String,
        level: u8,
    },
    /// Steam runs while a game uses a virtual controller: Steam Input may remap it again.
    SteamInput {
        game: String,
    },
    /// The screenshot command saved `file`, or no screenshot (`error`: why).
    Screenshot {
        file: Option<String>,
        error: Option<String>,
    },
    /// A sample from the settings dialog.
    Test,
}

/// One notification as the overlay page receives it.
#[derive(Clone, Debug, Serialize)]
pub struct Message {
    #[serde(flatten)]
    event: Event,
    /// Changes with every message, so the page can animate each one.
    id: u64,
    lang: String,
    /// Percent.
    opacity: u8,
    /// Percent.
    scale: u16,
    corner: Corner,
}

#[cfg(test)]
impl Message {
    /// A low-battery message, for the bridge contract fixture.
    pub(crate) fn sample() -> Message {
        Message {
            event: Event::BatteryLow {
                name: "DualSense".into(),
                level: 15,
            },
            id: 3,
            lang: "en".into(),
            opacity: 90,
            scale: 100,
            corner: Corner::TopRight,
        }
    }
}

#[derive(Default)]
pub struct OsdState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// When the current message goes away; `None` keeps it.
    until: Option<Instant>,
    next_id: u64,
}

/// The part of the live state notifications follow.
#[derive(Clone, Debug, Default, PartialEq)]
struct View {
    remap_on: bool,
    active_slot: usize,
    profile_path: Option<String>,
    layer: u8,
    layer_name: Option<String>,
    devices: Vec<PadInfo>,
    screenshot: LiveScreenshot,
}

/// A controller as notifications see it.
#[derive(Clone, Debug, PartialEq)]
struct PadInfo {
    id: String,
    name: String,
    battery: Option<Battery>,
}

impl View {
    fn of(live: &LiveState) -> View {
        View {
            remap_on: live.remap_on,
            active_slot: live.active_slot,
            profile_path: live.profile_path.clone(),
            layer: live.layer,
            layer_name: live.layer_name.clone(),
            // Not another program's mirror of a controller (Steam's virtual pads): it comes and
            // goes with the controller, which is announced already.
            devices: live
                .devices
                .iter()
                .filter(|d| !d.mirror)
                .map(|d| PadInfo {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    battery: d.battery,
                })
                .collect(),
            screenshot: live.screenshot.clone(),
        }
    }
}

/// The notifications `s` asks for between two views, the most important last (it is the
/// one that stays on screen).
fn events(old: &View, new: &View, s: &OsdSettings) -> Vec<Event> {
    let mut out = Vec::new();
    if s.controller {
        for d in &old.devices {
            if !new.devices.iter().any(|n| n.id == d.id) {
                out.push(Event::Disconnected {
                    name: d.name.clone(),
                });
            }
        }
        for d in &new.devices {
            if !old.devices.iter().any(|o| o.id == d.id) {
                out.push(Event::Connected {
                    name: d.name.clone(),
                });
            }
        }
    }
    let remap = old.remap_on != new.remap_on;
    let config = old.active_slot != new.active_slot || old.profile_path != new.profile_path;
    // A new config or remapping going off resets the layer; that is not a layer change.
    if s.layer && !remap && !config && (old.layer, &old.layer_name) != (new.layer, &new.layer_name)
    {
        out.push(Event::Layer {
            layer: new.layer,
            name: new.layer_name.clone(),
        });
    }
    if s.slot && config && new.remap_on && !remap {
        out.push(Event::Slot {
            slot: new.active_slot + 1,
            config: new.profile_path.clone(),
        });
    }
    if s.remap && remap {
        out.push(if new.remap_on {
            Event::RemapOn
        } else {
            Event::RemapOff
        });
    }
    if s.screenshot && new.screenshot.count != old.screenshot.count {
        out.push(Event::Screenshot {
            file: new.screenshot.file.clone(),
            error: new.screenshot.error.clone(),
        });
    }
    out
}

/// Battery warnings below these levels (percent).
const LOW: u8 = 40;
const CRITICAL: u8 = 20;
/// A battery charging or back at this level (percent) may warn again.
const REARM_ABOVE: u8 = LOW + 10;

/// The battery warning a controller got last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Warned {
    #[default]
    None,
    Low,
    Critical,
}

/// Warnings for batteries that dropped below a threshold since `warned` was last updated.
/// A warning repeats only after charging or climbing back to [`REARM_ABOVE`].
fn battery_events(devices: &[PadInfo], warned: &mut HashMap<String, Warned>) -> Vec<Event> {
    warned.retain(|id, _| devices.iter().any(|d| &d.id == id));
    let mut out = Vec::new();
    for d in devices {
        let Some(b) = d.battery else { continue };
        let w = warned.entry(d.id.clone()).or_default();
        if !b.draining() || b.level >= REARM_ABOVE {
            *w = Warned::None;
            continue;
        }
        let now = match b.level {
            l if l < CRITICAL => Warned::Critical,
            l if l < LOW => Warned::Low,
            _ => Warned::None,
        };
        if now > *w {
            *w = now;
            let (name, level) = (d.name.clone(), b.level);
            out.push(if now == Warned::Critical {
                Event::BatteryCritical { name, level }
            } else {
                Event::BatteryLow { name, level }
            });
        }
    }
    out
}

/// Show `event` now, for `keep` or, with `None`, until the next message.
pub fn notify(app: &AppHandle, event: Event, keep: Option<Duration>) {
    let (lang, s) = {
        let settings = app.state::<AppState>().settings.lock().unwrap().clone();
        (settings.language, settings.osd)
    };
    let state = app.state::<OsdState>();
    let mut inner = state.inner.lock().unwrap();
    inner.next_id += 1;
    let msg = Message {
        event,
        id: inner.next_id,
        lang,
        opacity: s.opacity,
        scale: s.scale,
        corner: s.corner,
    };
    inner.until = keep.map(|d| Instant::now() + d);
    drop(inner);
    let k = f64::from(s.scale) / 100.0;
    let placement = Placement::Corner {
        monitor: s.monitor,
        corner: s.corner,
        size: (SIZE.0 * k, SIZE.1 * k),
    };
    overlay::show_content(app, KIND, placement, &msg);
}

/// The id of the Steam Input hint among the questions and hints turned off (`silenced`).
pub const STEAM_HINT: &str = "steam_input";
/// Event that asks the main window to show a hint.
const HINT_EVENT: &str = "hint";
/// The Steam Input note stays this long at least: it needs reading.
const STEAM_HINT_MIN: Duration = Duration::from_secs(10);

/// A hint for the main window: which one, and the game it is about.
#[derive(Clone, Debug, Serialize)]
struct Hint {
    id: &'static str,
    game: String,
}

/// The program in front, when it is a game of the library whose config drives a virtual
/// controller (Steam Input may take that controller and remap it again).
fn game_with_virtual_pad(live: &LiveState) -> Option<&str> {
    (live.auto_active && live.virtual_pad.is_some())
        .then_some(live.foreground.as_deref())
        .flatten()
}

/// Whether to point at Steam Input for `front`: it just came to the front, it was not pointed
/// out for this program in this run, the hint is not turned off, and Steam runs (asked last:
/// it lists the processes).
fn steam_hint_due(
    front: Option<&str>,
    before: Option<&str>,
    hinted: &HashSet<String>,
    silenced: bool,
    steam_running: impl FnOnce() -> bool,
) -> bool {
    match front {
        Some(exe) if before != Some(exe) && !silenced && !hinted.contains(exe) => steam_running(),
        _ => false,
    }
}

/// The game's name for the running config, else the program.
fn game_name(live: &LiveState, exe: &str) -> String {
    live.profile_path
        .as_deref()
        .and_then(|p| Path::new(p).parent())
        .and_then(|dir| library::read_game(dir).ok())
        .map_or_else(|| exe.to_owned(), |g| g.meta.name)
}

/// Hide the message whose time is up. Returns whether one went away.
fn expire(app: &AppHandle) -> bool {
    let state = app.state::<OsdState>();
    let mut inner = state.inner.lock().unwrap();
    if inner.until.is_none_or(|u| Instant::now() < u) {
        return false;
    }
    inner.until = None;
    drop(inner);
    overlay::dismiss(app, KIND);
    true
}

/// "Game › Config" for a config path.
fn config_label(path: &str) -> String {
    library::config_label(Path::new(path))
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let started = Instant::now();
    let mut seen = View::of(&live.get());
    let mut warned = HashMap::new();
    let (mut front, mut hinted) = (None::<String>, HashSet::new());
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let state = live.get();
        let view = View::of(&state);
        let app_state = app.state::<AppState>();
        let (s, steam_silenced) = {
            let settings = app_state.settings.lock().unwrap();
            (
                settings.osd.clone(),
                settings.silenced.iter().any(|id| id == STEAM_HINT),
            )
        };
        let starting = started.elapsed() < STARTUP_QUIET;
        let quiet = !s.enabled || starting || app_state.main_focused();
        // The window may not listen yet while the app starts: look again afterwards.
        if !starting {
            let now = game_with_virtual_pad(&state);
            if steam_hint_due(
                now,
                front.as_deref(),
                &hinted,
                steam_silenced,
                dp_platform::steam_running,
            ) && let Some(exe) = now
            {
                hinted.insert(exe.to_owned());
                let game = game_name(&state, exe);
                tracing::info!(%game, "Steam runs while the game uses a virtual controller");
                let _ = app.emit_to(
                    MAIN_WINDOW,
                    HINT_EVENT,
                    Hint {
                        id: STEAM_HINT,
                        game: game.clone(),
                    },
                );
                if !quiet {
                    notify(
                        &app,
                        Event::SteamInput { game },
                        Some(s.duration().max(STEAM_HINT_MIN)),
                    );
                }
            }
            front = now.map(str::to_owned);
        }
        if view != seen {
            if !quiet {
                for mut e in events(&seen, &view, &s) {
                    if let Event::Slot {
                        config: Some(path), ..
                    } = &mut e
                    {
                        *path = config_label(path);
                    }
                    let stays =
                        s.keep_layer && matches!(e, Event::Layer { layer, .. } if layer != 0);
                    let keep = match e {
                        Event::Screenshot { .. } => s.duration().min(SCREENSHOT_KEEP),
                        _ => s.duration(),
                    };
                    notify(&app, e, (!stays).then_some(keep));
                }
            }
            seen = view;
        }
        if !quiet && s.battery {
            for e in battery_events(&seen.devices, &mut warned) {
                notify(&app, e, Some(s.duration()));
            }
        }
        // Another message replaced a layer message that should stay: bring it back.
        if expire(&app) && !quiet && s.layer && s.keep_layer && seen.layer != 0 {
            let e = Event::Layer {
                layer: seen.layer,
                name: seen.layer_name.clone(),
            };
            notify(&app, e, None);
        }
    }
}

/// Show a sample notification with the current settings. Async: window creation must not
/// run on the main thread, where plain commands run.
#[tauri::command(async)]
pub fn osd_test(app: AppHandle) {
    let shown = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .osd
        .duration();
    notify(&app, Event::Test, Some(shown));
}

/// The monitors notifications can use, named by their size, the primary one first.
#[tauri::command]
pub fn osd_monitors(app: AppHandle) -> Vec<String> {
    overlay::monitors(&app)
        .iter()
        .map(|m| {
            let size = m.size();
            format!("{} × {}", size.width, size.height)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_hint_once_per_game_with_a_virtual_pad() {
        let mut live = LiveState {
            auto_active: true,
            virtual_pad: Some("xbox360".into()),
            foreground: Some("game.exe".into()),
            ..LiveState::default()
        };
        assert_eq!(game_with_virtual_pad(&live), Some("game.exe"));
        let mut hinted = HashSet::new();
        let due = |front, before, hinted: &HashSet<String>, silenced, steam| {
            steam_hint_due(front, before, hinted, silenced, || steam)
        };
        assert!(due(Some("game.exe"), None, &hinted, false, true));
        assert!(
            !due(Some("game.exe"), None, &hinted, false, false),
            "no Steam"
        );
        assert!(
            !due(Some("game.exe"), None, &hinted, true, true),
            "turned off"
        );
        assert!(
            !due(Some("game.exe"), Some("game.exe"), &hinted, false, true),
            "still in front"
        );
        hinted.insert("game.exe".to_owned());
        assert!(!due(Some("game.exe"), None, &hinted, false, true), "once");
        assert!(due(Some("other.exe"), None, &hinted, false, true));
        assert!(!due(None, Some("game.exe"), &hinted, false, true));

        live.virtual_pad = None;
        assert_eq!(
            game_with_virtual_pad(&live),
            None,
            "keyboard and mouse only"
        );
        live.virtual_pad = Some("ds4".into());
        live.auto_active = false;
        assert_eq!(game_with_virtual_pad(&live), None, "not a library game");
    }

    fn view() -> View {
        View {
            remap_on: true,
            active_slot: 0,
            profile_path: Some("a.toml".into()),
            layer: 0,
            layer_name: None,
            devices: vec![PadInfo {
                id: "pad1".into(),
                name: "DualSense".into(),
                battery: None,
            }],
            screenshot: LiveScreenshot::default(),
        }
    }

    #[test]
    fn screenshots_are_told_of() {
        let mut s = OsdSettings::default();
        let old = view();
        let mut new = view();
        new.screenshot = LiveScreenshot {
            count: 1,
            file: Some("Game 2026-10-25 17-16-20.png".into()),
            error: None,
        };
        assert_eq!(
            events(&old, &new, &s),
            vec![Event::Screenshot {
                file: Some("Game 2026-10-25 17-16-20.png".into()),
                error: None
            }]
        );
        let mut failed = new.clone();
        failed.screenshot = LiveScreenshot {
            count: 2,
            file: None,
            error: Some("no permission".into()),
        };
        assert_eq!(
            events(&new, &failed, &s),
            vec![Event::Screenshot {
                file: None,
                error: Some("no permission".into())
            }],
            "the same outcome again is news too"
        );
        s.screenshot = false;
        assert!(events(&old, &new, &s).is_empty());
    }

    #[test]
    fn layer_changes_but_not_the_reset_with_a_new_config() {
        let s = OsdSettings::default();
        let old = view();
        let mut new = view();
        new.layer = 1;
        new.layer_name = Some("Aim".into());
        assert_eq!(
            events(&old, &new, &s),
            vec![Event::Layer {
                layer: 1,
                name: Some("Aim".into())
            }]
        );
        let mut other = view();
        other.active_slot = 1;
        other.profile_path = Some("b.toml".into());
        assert_eq!(
            events(&new, &other, &s),
            vec![Event::Slot {
                slot: 2,
                config: Some("b.toml".into())
            }]
        );
    }

    #[test]
    fn remap_and_controllers_follow_their_switches() {
        let mut s = OsdSettings::default();
        let old = view();
        let mut new = view();
        new.remap_on = false;
        new.devices = vec![PadInfo {
            id: "pad2".into(),
            name: "Pro Controller".into(),
            battery: None,
        }];
        assert_eq!(
            events(&old, &new, &s),
            vec![
                Event::Disconnected {
                    name: "DualSense".into()
                },
                Event::Connected {
                    name: "Pro Controller".into()
                },
                Event::RemapOff,
            ]
        );
        s.controller = false;
        s.remap = false;
        assert!(events(&old, &new, &s).is_empty());
    }

    #[test]
    fn mirrors_are_not_announced() {
        let pad = dp_service::LiveDevice {
            id: "evdev:054c:0ce6:usb-1".into(),
            name: "DualSense Wireless Controller".into(),
            family: dp_platform::PadFamily::DualSense,
            backend: "evdev".into(),
            vendor_id: 0x054C,
            product_id: 0x0CE6,
            state: dp_core::pad::PadState::default(),
            gyro_calibration: None,
            battery: None,
            raw: None,
            mirror: false,
        };
        let steam = dp_service::LiveDevice {
            id: "evdev:28de:11ff:/dev/input/event20".into(),
            name: "Microsoft X-Box 360 pad 0".into(),
            mirror: true,
            ..pad.clone()
        };
        let live = LiveState {
            devices: vec![pad, steam],
            ..LiveState::default()
        };
        let names: Vec<String> = View::of(&live)
            .devices
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, ["DualSense Wireless Controller"]);
    }

    #[test]
    fn batteries_warn_once_per_drop() {
        let pad = |level, charging| {
            vec![PadInfo {
                id: "pad1".into(),
                name: "DualSense".into(),
                battery: Some(Battery {
                    level,
                    charging,
                    wired: false,
                }),
            }]
        };
        let mut warned = HashMap::new();
        assert!(battery_events(&pad(55, false), &mut warned).is_empty());
        let low = battery_events(&pad(35, false), &mut warned);
        assert_eq!(
            low,
            vec![Event::BatteryLow {
                name: "DualSense".into(),
                level: 35
            }]
        );
        // Wobbling around the threshold does not repeat it.
        assert!(battery_events(&pad(45, false), &mut warned).is_empty());
        assert!(battery_events(&pad(35, false), &mut warned).is_empty());
        assert!(matches!(
            battery_events(&pad(15, false), &mut warned)[..],
            [Event::BatteryCritical { level: 15, .. }]
        ));
        // Charging clears it; draining below the threshold again warns again.
        assert!(battery_events(&pad(15, true), &mut warned).is_empty());
        assert_eq!(battery_events(&pad(15, false), &mut warned).len(), 1);
    }

    #[test]
    fn messages_carry_the_event_kind_flat() {
        let m = Message {
            event: Event::Slot {
                slot: 2,
                config: None,
            },
            id: 7,
            lang: "en".into(),
            opacity: 80,
            scale: 100,
            corner: Corner::BottomRight,
        };
        let v = serde_json::to_value(m).unwrap();
        assert_eq!(v["kind"], "slot");
        assert_eq!(v["slot"], 2);
        assert_eq!(v["corner"], "bottom_right");
    }
}
