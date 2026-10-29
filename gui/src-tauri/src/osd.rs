//! On-screen notifications over games: remapping on/off, slot or config changes, layer
//! changes, controllers coming and going, and screenshots. A watcher thread follows the
//! service's live state like the tray does and shows each change for a few seconds in the
//! `osd` overlay window; the overlay page words it in the app's language. Nothing shows while
//! the main window has focus (it shows the same state) or in the first seconds after start.
//! The rules live in [`Osd::step`], apart from windows and threads; another watcher points at
//! Steam Input when a game with a virtual controller comes to the front while Steam runs.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dp_config::library;
use dp_platform::Battery;
use dp_service::{LiveScreenshot, LiveState, SharedLive};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::main_window::MAIN_WINDOW;
use crate::overlay::{self, OverlayKind, Placement};
use crate::settings::{Corner, OsdSettings};
use crate::state::{AppState, lock};

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

/// The notification overlay as the watcher, the Steam hint and the settings' sample share it.
#[derive(Default)]
pub struct OsdState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    on_screen: OnScreen,
    next_id: u64,
}

/// What the notification overlay shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OnScreen {
    #[default]
    Nothing,
    /// A message that goes away at this time.
    Until(Instant),
    /// A layer message that stays while its layer is active.
    Kept,
}

impl OnScreen {
    /// What shows after a message that stays for `keep` (`None`: kept) is shown at `now`.
    fn after(now: Instant, keep: Option<Duration>) -> OnScreen {
        keep.map_or(OnScreen::Kept, |d| OnScreen::Until(now + d))
    }
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

/// How long `e` stays on screen with the settings `s`; `None` keeps it while its layer is
/// active.
fn keep_for(e: &Event, s: &OsdSettings) -> Option<Duration> {
    match e {
        Event::Layer { layer, .. } if s.keep_layer && *layer != 0 => None,
        Event::Screenshot { .. } => Some(s.duration().min(SCREENSHOT_KEEP)),
        _ => Some(s.duration()),
    }
}

/// What the notification overlay is to do.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    /// Show a message, for a while or (`None`) while its layer is active.
    Show(Event, Option<Duration>),
    Hide,
}

/// The notification rules, apart from windows and threads: which changes of the live state
/// show a message and for how long, and when a layer message that stays comes back or goes.
struct Osd {
    /// The live state as last seen.
    seen: View,
    /// The battery warning each controller got last.
    warned: HashMap<String, Warned>,
}

impl Osd {
    fn new(seen: View) -> Osd {
        Osd {
            seen,
            warned: HashMap::new(),
        }
    }

    /// What to do at `now` about the live state `view`, with the settings `s`, `quiet` while
    /// nothing may show (notifications off, the app starting, the main window in front), and
    /// `on_screen` what the overlay shows.
    fn step(
        &mut self,
        view: &View,
        s: &OsdSettings,
        quiet: bool,
        mut on_screen: OnScreen,
        now: Instant,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let show = |out: &mut Vec<Action>, on_screen: &mut OnScreen, e, keep: Option<Duration>| {
            *on_screen = OnScreen::after(now, keep);
            out.push(Action::Show(e, keep));
        };
        if *view != self.seen {
            if !quiet {
                for e in events(&self.seen, view, s) {
                    let keep = keep_for(&e, s);
                    show(&mut out, &mut on_screen, e, keep);
                }
            }
            self.seen = view.clone();
        }
        if !quiet && s.battery {
            for e in battery_events(&self.seen.devices, &mut self.warned) {
                show(&mut out, &mut on_screen, e, Some(s.duration()));
            }
        }
        let keep_layer = !quiet && s.layer && s.keep_layer && self.seen.layer != 0;
        match on_screen {
            OnScreen::Until(end) if now >= end => {
                out.push(Action::Hide);
                on_screen = OnScreen::Nothing;
            }
            // Gone quiet, "keep layer" switched off, or back on the main layer while quiet:
            // a kept message would stay over the game, out of date.
            OnScreen::Kept if !keep_layer => {
                out.push(Action::Hide);
                on_screen = OnScreen::Nothing;
            }
            _ => {}
        }
        // Another message replaced the layer message, or the quiet is over: it comes back.
        if on_screen == OnScreen::Nothing && keep_layer {
            let e = Event::Layer {
                layer: self.seen.layer,
                name: self.seen.layer_name.clone(),
            };
            show(&mut out, &mut on_screen, e, None);
        }
        out
    }
}

/// Show `event` now, for `keep` or, with `None`, while its layer is active.
pub fn notify(app: &AppHandle, event: Event, keep: Option<Duration>) {
    let (lang, s) = app
        .state::<AppState>()
        .settings(|s| (s.language.clone(), s.osd.clone()));
    // "Game › Config" for a slot's config file.
    let event = match event {
        Event::Slot {
            slot,
            config: Some(path),
        } => Event::Slot {
            slot,
            config: Some(library::config_label(Path::new(&path))),
        },
        e => e,
    };
    let state = app.state::<OsdState>();
    let mut inner = lock(&state.inner);
    inner.next_id += 1;
    let msg = Message {
        event,
        id: inner.next_id,
        lang,
        opacity: s.opacity,
        scale: s.scale,
        corner: s.corner,
    };
    inner.on_screen = OnScreen::after(Instant::now(), keep);
    drop(inner);
    let k = f64::from(s.scale) / 100.0;
    let placement = Placement::Corner {
        monitor: s.monitor_index(&overlay::screens(app)),
        corner: s.corner,
        size: (SIZE.0 * k, SIZE.1 * k),
    };
    overlay::show_content(app, KIND, placement, &msg);
}

/// Hide the message `shown`, unless another one replaced it meanwhile.
fn hide(app: &AppHandle, shown: OnScreen) {
    let state = app.state::<OsdState>();
    let mut inner = lock(&state.inner);
    if inner.on_screen != shown {
        return;
    }
    inner.on_screen = OnScreen::Nothing;
    drop(inner);
    overlay::dismiss(app, KIND);
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let started = Instant::now();
    let (mut generation, first) = live.since(0).unwrap_or_default();
    let mut view = View::of(&first);
    let mut osd = Osd::new(view.clone());
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        // A new view only when the live state changed; the times are checked on every pass.
        if let Some((g, state)) = live.since(generation) {
            generation = g;
            view = View::of(&state);
        }
        let app_state = app.state::<AppState>();
        let s = app_state.settings(|s| s.osd.clone());
        let quiet = !s.enabled || started.elapsed() < STARTUP_QUIET || app_state.main_focused();
        let on_screen = lock(&app.state::<OsdState>().inner).on_screen;
        for action in osd.step(&view, &s, quiet, on_screen, Instant::now()) {
            match action {
                Action::Show(e, keep) => notify(&app, e, keep),
                Action::Hide => hide(&app, on_screen),
            }
        }
    }
}

/// The id of the Steam Input hint among the questions and hints turned off (`silenced`).
pub const STEAM_HINT: &str = "steam_input";
/// Event that asks the main window to show a hint.
pub const HINT_EVENT: &str = "hint";
/// The Steam Input note stays this long at least: it needs reading.
const STEAM_HINT_MIN: Duration = Duration::from_secs(10);

/// A hint for the main window: which one, and the game it is about.
#[derive(Clone, Debug, Serialize)]
pub struct Hint {
    pub id: &'static str,
    pub game: String,
}

/// The program in front, when it is a game of the library whose config drives a virtual
/// controller (Steam Input may take that controller and remap it again).
fn game_with_virtual_pad(live: &LiveState) -> Option<&str> {
    (live.auto_active && live.virtual_pad.is_some())
        .then_some(live.foreground.as_deref())
        .flatten()
}

/// The Steam Input hint's rule: point at Steam Input once per program and run, as such a game
/// comes to the front while Steam runs.
#[derive(Default)]
struct SteamHint {
    /// The game in front when last looked.
    front: Option<String>,
    /// The programs pointed out already.
    hinted: HashSet<String>,
}

impl SteamHint {
    /// The program to point out now, if any: one that just came to the front and was not
    /// pointed out in this run, while the hint is not turned off (`silenced`) and Steam runs
    /// (asked last: it lists the processes).
    fn step(
        &mut self,
        live: &LiveState,
        silenced: bool,
        steam_running: impl FnOnce() -> bool,
    ) -> Option<String> {
        let now = game_with_virtual_pad(live);
        let before = std::mem::replace(&mut self.front, now.map(str::to_owned));
        let exe = now.filter(|exe| {
            before.as_deref() != Some(*exe) && !silenced && !self.hinted.contains(*exe)
        })?;
        steam_running().then(|| {
            self.hinted.insert(exe.to_owned());
            exe.to_owned()
        })
    }
}

/// The game's name for the running config, else the program.
fn game_name(live: &LiveState, exe: &str) -> String {
    live.profile_path
        .as_deref()
        .and_then(|p| Path::new(p).parent())
        .and_then(|dir| library::read_game_meta(dir).ok())
        .map_or_else(|| exe.to_owned(), |meta| meta.name)
}

/// Point at Steam Input when a game that drives a virtual controller comes to the front while
/// Steam runs: as a hint in the main window, and as a notification over the game.
pub(crate) fn watch_steam(app: AppHandle, live: SharedLive) {
    let started = Instant::now();
    let mut generation = 0;
    let mut hint = SteamHint::default();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        // The window may not listen yet while the app starts: look again afterwards.
        if started.elapsed() < STARTUP_QUIET {
            continue;
        }
        let Some((g, state)) = live.since(generation) else {
            continue;
        };
        generation = g;
        let app_state = app.state::<AppState>();
        let (s, silenced) = app_state.settings(|s| {
            let silenced = s.silenced.iter().any(|id| id == STEAM_HINT);
            (s.osd.clone(), silenced)
        });
        let Some(exe) = hint.step(&state, silenced, dp_platform::steam_running) else {
            continue;
        };
        let game = game_name(&state, &exe);
        tracing::info!(%game, "Steam runs while the game uses a virtual controller");
        let _ = app.emit_to(
            MAIN_WINDOW,
            HINT_EVENT,
            Hint {
                id: STEAM_HINT,
                game: game.clone(),
            },
        );
        if s.enabled && !app_state.main_focused() {
            let keep = s.duration().max(STEAM_HINT_MIN);
            notify(&app, Event::SteamInput { game }, Some(keep));
        }
    }
}

/// Show a sample notification with the current settings. Async: window creation must not
/// run on the main thread, where plain commands run.
#[tauri::command(async)]
pub fn osd_test(app: AppHandle) {
    let shown = app.state::<AppState>().settings(|s| s.osd.duration());
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
        let in_front = |exe: Option<&str>| LiveState {
            foreground: exe.map(str::to_owned),
            ..live.clone()
        };
        let mut hint = SteamHint::default();
        assert_eq!(hint.step(&live, false, || false), None, "no Steam");
        let mut hint = SteamHint::default();
        assert_eq!(hint.step(&live, true, || true), None, "turned off");
        let mut hint = SteamHint::default();
        assert_eq!(
            hint.step(&live, false, || true).as_deref(),
            Some("game.exe")
        );
        assert_eq!(hint.step(&live, false, || true), None, "still in front");
        assert_eq!(hint.step(&in_front(None), false, || true), None);
        assert_eq!(hint.step(&live, false, || true), None, "once per run");
        assert_eq!(
            hint.step(&in_front(Some("other.exe")), false, || true)
                .as_deref(),
            Some("other.exe")
        );
        // Steam is asked last, and only when the rest says yes.
        let mut hint = SteamHint::default();
        hint.step(&live, false, || true);
        hint.step(&in_front(None), false, || unreachable!("nothing in front"));

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

    /// A step `ms` milliseconds after `t0`.
    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn layer(n: u8) -> View {
        View {
            layer: n,
            layer_name: (n != 0).then(|| format!("L{n}")),
            ..view()
        }
    }

    fn layer_event(n: u8) -> Event {
        Event::Layer {
            layer: n,
            name: (n != 0).then(|| format!("L{n}")),
        }
    }

    #[test]
    fn messages_go_when_their_time_is_up() {
        let s = OsdSettings::default();
        let t0 = Instant::now();
        let mut osd = Osd::new(view());
        let shown = osd.step(&layer(1), &s, false, OnScreen::Nothing, t0);
        assert_eq!(shown, [Action::Show(layer_event(1), Some(s.duration()))]);
        let until = OnScreen::after(t0, Some(s.duration()));
        assert!(
            osd.step(&layer(1), &s, false, until, at(t0, 4_999))
                .is_empty()
        );
        assert_eq!(
            osd.step(&layer(1), &s, false, until, at(t0, 5_000)),
            [Action::Hide]
        );
        // Quiet does not cut a message short, nor keep it.
        assert_eq!(
            osd.step(&layer(1), &s, true, until, at(t0, 5_000)),
            [Action::Hide]
        );
    }

    #[test]
    fn a_kept_layer_message_comes_back_after_another_one() {
        let s = OsdSettings {
            keep_layer: true,
            ..OsdSettings::default()
        };
        let t0 = Instant::now();
        let mut osd = Osd::new(view());
        assert_eq!(
            osd.step(&layer(2), &s, false, OnScreen::Nothing, t0),
            [Action::Show(layer_event(2), None)]
        );
        // A battery warning replaced it and its time is up: the layer message is back.
        let warning = OnScreen::after(t0, Some(s.duration()));
        assert_eq!(
            osd.step(&layer(2), &s, false, warning, at(t0, 5_000)),
            [Action::Hide, Action::Show(layer_event(2), None)]
        );
        // Back to the main layer: a short message, which goes.
        assert_eq!(
            osd.step(&layer(0), &s, false, OnScreen::Kept, at(t0, 6_000)),
            [Action::Show(layer_event(0), Some(s.duration()))]
        );
    }

    #[test]
    fn a_kept_layer_message_goes_when_it_must_not_stay() {
        let keep = OsdSettings {
            keep_layer: true,
            ..OsdSettings::default()
        };
        let t0 = Instant::now();
        let kept_on = |s: &OsdSettings, quiet: bool, back_to: u8| {
            let mut osd = Osd::new(layer(2));
            osd.step(&layer(back_to), s, quiet, OnScreen::Kept, t0)
        };
        assert!(kept_on(&keep, false, 2).is_empty(), "it stays");
        // The window came to the front (or notifications went off).
        assert_eq!(kept_on(&keep, true, 2), [Action::Hide]);
        // "Keep layer" was switched off, or layer messages altogether.
        let off = OsdSettings {
            keep_layer: false,
            ..keep.clone()
        };
        assert_eq!(kept_on(&off, false, 2), [Action::Hide]);
        let no_layers = OsdSettings {
            layer: false,
            ..keep.clone()
        };
        assert_eq!(kept_on(&no_layers, false, 2), [Action::Hide]);
        // Back on the main layer while quiet: no message about it, but the old one goes.
        assert_eq!(kept_on(&keep, true, 0), [Action::Hide]);
        // Once the quiet is over, the layer still active, it shows again.
        let mut osd = Osd::new(layer(2));
        assert_eq!(
            osd.step(&layer(2), &keep, false, OnScreen::Nothing, t0),
            [Action::Show(layer_event(2), None)]
        );
    }

    #[test]
    fn screenshots_go_soon_and_quiet_shows_nothing() {
        let s = OsdSettings::default();
        let t0 = Instant::now();
        let mut osd = Osd::new(view());
        let mut shot = view();
        shot.screenshot.count = 1;
        assert_eq!(
            osd.step(&shot, &s, false, OnScreen::Nothing, t0),
            [Action::Show(
                Event::Screenshot {
                    file: None,
                    error: None
                },
                Some(SCREENSHOT_KEEP)
            )]
        );
        // A change while quiet is not told later either.
        assert!(
            osd.step(&layer(1), &s, true, OnScreen::Nothing, t0)
                .is_empty()
        );
        assert!(
            osd.step(&layer(1), &s, false, OnScreen::Nothing, t0)
                .is_empty()
        );
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
