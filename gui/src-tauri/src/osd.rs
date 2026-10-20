//! On-screen notifications over games: remapping on/off, slot or config changes, layer
//! changes and controllers coming and going. A watcher thread follows the service's live
//! state like the tray does and shows each change for a few seconds in the `osd` overlay
//! window; the overlay page words it in the app's language. Nothing shows while the main
//! window has focus (it shows the same state) or in the first seconds after start.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dp_config::library;
use dp_platform::Battery;
use dp_service::LiveState;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::overlay;
use crate::settings::{Corner, OsdSettings};

const KIND: &str = "osd";
/// How often the watcher compares the live state with what it last saw.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Controllers connecting while the app starts are not news.
const STARTUP_QUIET: Duration = Duration::from_secs(3);
/// Window size at 100 %, in logical pixels.
const SIZE: (f64, f64) = (380.0, 84.0);

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
    /// Id, name and battery of each controller.
    devices: Vec<(String, String, Option<Battery>)>,
}

impl View {
    fn of(live: &LiveState) -> View {
        View {
            remap_on: live.remap_on,
            active_slot: live.active_slot,
            profile_path: live.profile_path.clone(),
            layer: live.layer,
            layer_name: live.layer_name.clone(),
            devices: live
                .devices
                .iter()
                .map(|d| (d.id.clone(), d.name.clone(), d.battery))
                .collect(),
        }
    }
}

/// The notifications `s` asks for between two views, the most important last (it is the
/// one that stays on screen).
fn events(old: &View, new: &View, s: &OsdSettings) -> Vec<Event> {
    let mut out = Vec::new();
    if s.controller {
        for (id, name, _) in &old.devices {
            if !new.devices.iter().any(|(i, ..)| i == id) {
                out.push(Event::Disconnected { name: name.clone() });
            }
        }
        for (id, name, _) in &new.devices {
            if !old.devices.iter().any(|(i, ..)| i == id) {
                out.push(Event::Connected { name: name.clone() });
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
    out
}

/// Battery warnings below these levels (percent).
const LOW: u8 = 40;
const CRITICAL: u8 = 20;

/// Warnings for batteries that dropped below a threshold since `warned` (per controller: 0
/// none, 1 low, 2 critical) was last updated. A warning repeats only after charging or
/// climbing well above the low threshold again.
fn battery_events(
    devices: &[(String, String, Option<Battery>)],
    warned: &mut HashMap<String, u8>,
) -> Vec<Event> {
    warned.retain(|id, _| devices.iter().any(|(i, ..)| i == id));
    let mut out = Vec::new();
    for (id, name, battery) in devices {
        let Some(b) = battery else { continue };
        let w = warned.entry(id.clone()).or_insert(0);
        if !b.draining() || b.level >= LOW + 10 {
            *w = 0;
            continue;
        }
        let now = match b.level {
            l if l < CRITICAL => 2,
            l if l < LOW => 1,
            _ => 0,
        };
        if now > *w {
            *w = now;
            let (name, level) = (name.clone(), b.level);
            out.push(if now == 2 {
                Event::BatteryCritical { name, level }
            } else {
                Event::BatteryLow { name, level }
            });
        }
    }
    out
}

/// Show `event` now; `keep` leaves it up until the next one.
pub fn notify(app: &AppHandle, event: Event, keep: bool) {
    let (lang, s) = {
        let settings = app.state::<AppState>().settings.lock().unwrap().clone();
        (settings.language, settings.osd.clamped())
    };
    let (w, created) = match overlay::window(app, KIND) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("cannot create the notification overlay: {e}");
            return;
        }
    };
    let k = s.scale as f64 / 100.0;
    if let Err(e) = overlay::place(app, &w, s.monitor, s.corner, (SIZE.0 * k, SIZE.1 * k)) {
        tracing::warn!("cannot place the notification overlay: {e}");
    }
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
    inner.until = (!keep).then(|| Instant::now() + Duration::from_secs(s.seconds as u64));
    drop(inner);
    overlay::present(app, &w, created, KIND, &msg);
}

/// Hide the message whose time is up. Returns whether one went away.
fn expire(app: &AppHandle) -> bool {
    let state = app.state::<OsdState>();
    let mut inner = state.inner.lock().unwrap();
    if !inner.until.is_some_and(|u| Instant::now() >= u) {
        return false;
    }
    inner.until = None;
    drop(inner);
    overlay::dismiss(app, KIND);
    true
}

fn main_focused(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .main_focused
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// "Game › Config" for a config path.
fn config_label(app: &AppHandle, path: &str) -> String {
    let dir = app.state::<AppState>().profiles_dir.clone();
    crate::tray::config_label(&library::scan(&dir), path)
}

fn watch(app: AppHandle, live: Arc<Mutex<LiveState>>) {
    let started = Instant::now();
    let mut seen = View::of(&live.lock().unwrap());
    let mut warned = HashMap::new();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let view = View::of(&live.lock().unwrap());
        let s = app.state::<AppState>().settings.lock().unwrap().osd.clone();
        let quiet = !s.enabled || started.elapsed() < STARTUP_QUIET || main_focused(&app);
        if view != seen {
            if !quiet {
                for mut e in events(&seen, &view, &s) {
                    if let Event::Slot {
                        config: Some(path), ..
                    } = &mut e
                    {
                        *path = config_label(&app, path);
                    }
                    let keep =
                        s.keep_layer && matches!(e, Event::Layer { layer, .. } if layer != 0);
                    notify(&app, e, keep);
                }
            }
            seen = view;
        }
        if !quiet && s.battery {
            for e in battery_events(&seen.devices, &mut warned) {
                notify(&app, e, false);
            }
        }
        // Another message replaced a layer message that should stay: bring it back.
        if expire(&app) && !quiet && s.layer && s.keep_layer && seen.layer != 0 {
            let e = Event::Layer {
                layer: seen.layer,
                name: seen.layer_name.clone(),
            };
            notify(&app, e, true);
        }
    }
}

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let live = app
        .state::<AppState>()
        .service
        .lock()
        .unwrap()
        .live_shared();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("osd".into())
        .spawn(move || watch(handle, live))?;
    Ok(())
}

/// Show a sample notification with the current settings.
#[tauri::command]
pub fn osd_test(app: AppHandle) {
    // Window creation must not run on the main thread, where sync commands run.
    std::thread::spawn(move || notify(&app, Event::Test, false));
}

/// Names of the monitors notifications can use, the primary one first.
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

    fn view() -> View {
        View {
            remap_on: true,
            active_slot: 0,
            profile_path: Some("a.toml".into()),
            layer: 0,
            layer_name: None,
            devices: vec![("pad1".into(), "DualSense".into(), None)],
        }
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
        new.devices = vec![("pad2".into(), "Pro Controller".into(), None)];
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
    fn batteries_warn_once_per_drop() {
        let pad = |level, charging| {
            vec![(
                "pad1".to_string(),
                "DualSense".to_string(),
                Some(Battery {
                    level,
                    charging,
                    wired: false,
                }),
            )]
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
