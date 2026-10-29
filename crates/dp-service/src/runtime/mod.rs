//! The runtime loop: polls devices, drives the engines, and applies their output to the
//! platform backends.
//!
//! Several profiles can run at once, one per *session*. The default session runs on the
//! controller in use (the preferred one, else the first one without a session of its
//! own, which it keeps and returns to after a disconnect); a controller can also have its
//! own session with its own profile and virtual pad.
//! Keyboard and mouse are shared: their input goes to the default session (or the first
//! session if there is none), and every session's keyboard and mouse output goes out.

mod binding;
mod commands;
pub(crate) mod confirm;
mod drivers;
mod echo;
mod idle;
mod session;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

pub use self::drivers::{DriverState, DriverStatus};

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use dp_core::control::Control;
use dp_core::engine::{Engine, Input};
use dp_core::hotkeys::{HotkeyAction, HotkeyDetector, Hotkeys};
use dp_core::keys::{Key, MouseButton};
use dp_core::pad::{PadState, Rumble};
use dp_core::profile::{AnalogSource, Crosshair, KbmSource, Profile, RadialMenu, VirtualPadType};
use dp_core::repeat::KeyRepeater;
use dp_platform::generic::GenericLayout;
use dp_platform::groups::{Grouping, SeenDevice};
use dp_platform::{
    CaptureFilter, DeviceId, DriverReport, GamepadBackend, KbmEvent, Leds, PadSnapshot, Platform,
    VirtualIgnoreList, VirtualPadKind,
};
use serde::Serialize;

use self::commands::TestTurn;
use self::confirm::Confirm;
use self::echo::Echo;
use self::idle::Idle;
use self::session::{CrosshairChoice, Session, rumble_gap_ms};
use crate::calibration::Calibrator;
use crate::timing::{Clock, Every};

/// What user interfaces show about one running profile.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SessionInfo {
    /// The controller the session belongs to; `None` for the default session.
    pub owner: Option<String>,
    /// The controller it reads right now.
    pub device: Option<String>,
    pub profile: String,
    pub profile_path: Option<String>,
    pub layer: u8,
    pub virtual_pad: Option<VirtualPadKind>,
    /// What the session sends right now, for showing it to the user.
    pub output: SessionOutput,
}

/// What a session sends right now: the virtual pad's state and the keys and mouse buttons it holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SessionOutput {
    pub pad: Option<PadState>,
    pub keys: Vec<Key>,
    pub mouse_buttons: Vec<MouseButton>,
}

impl SessionOutput {
    /// Follow one tick's output: the pad state it sends, and keys and buttons going down or up.
    pub(crate) fn observe(&mut self, out: &dp_core::engine::Output) {
        if let Some(pad) = out.pad {
            self.pad = Some(pad);
        }
        for &(k, down) in &out.keys {
            hold(&mut self.keys, k, down);
        }
        for &(b, down) in &out.mouse_buttons {
            hold(&mut self.mouse_buttons, b, down);
        }
    }
}

/// Keep `held` sorted with `x` in it while it is down.
fn hold<T: Ord>(held: &mut Vec<T>, x: T, down: bool) {
    match held.binary_search(&x) {
        Ok(i) if !down => {
            held.remove(i);
        }
        Err(i) if down => held.insert(i, x),
        _ => {}
    }
}

/// Everything needed to run profiles against the real devices.
pub struct Runtime {
    /// The platform's backends, its gamepad backends moved into `pads`.
    platform: Platform,
    /// Every gamepad backend, with the block list and the device groups applied.
    pads: Grouping,
    /// The default session (if any) comes first.
    sessions: Vec<Session>,
    kbm: KbmInput,
    clock: Clock,
    snapshots: Vec<PadSnapshot>,
    /// The ids of `snapshots`, kept to notice a changed device list without allocating.
    device_ids: Vec<DeviceId>,
    /// The pad states of `snapshots` as the engines get them (hotkey buttons withheld).
    pad_buf: Vec<PadState>,
    /// Which controller the default session reads.
    binding: DefaultBinding,
    hidden: Vec<String>,
    echo: Option<Echo>,
    suspended: bool,
    remap_off_requested: bool,
    hotkeys: HotkeyState,
    /// Rumble patterns confirming hotkeys, per controller.
    pulses: confirm::Pulses,
    /// A calibration turn: counts to send, start, length and how much was sent.
    test_turn: Option<TestTurn>,
    /// Gyro bias per controller, taken off before anything reads the rates.
    calibration: Calibrator,
    overlays: Overlays,
    /// Player lights and microphone LED last sent, per controller.
    sent_leds: HashMap<DeviceId, Leds>,
    power: PowerState,
}

/// Captured keyboard and mouse input.
struct KbmInput {
    /// Which inputs other applications do not see.
    filter: Arc<CaptureFilter>,
    rx: Option<Receiver<KbmEvent>>,
    /// This tick's events.
    buf: Vec<KbmEvent>,
}

/// Which controller the default session reads.
#[derive(Default)]
struct DefaultBinding {
    /// The controller in use.
    primary: Option<DeviceId>,
    /// The controller picked in the app.
    preferred: Option<DeviceId>,
    /// The controller in use until it went away (unplugged, reset, or taken by a session of
    /// its own). The session goes back to it when it is free again.
    lost: Option<DeviceId>,
}

/// The controller hotkeys.
#[derive(Default)]
struct HotkeyState {
    set: Hotkeys,
    detectors: HashMap<DeviceId, HotkeyDetector>,
    paused: bool,
    /// Fired hotkeys the service carries out.
    actions: Vec<(DeviceId, HotkeyAction)>,
}

/// What the app shows for the runtime: the live controller and mappings overlays, the
/// controller navigation and the last screenshot.
#[derive(Default)]
struct Overlays {
    pad: bool,
    mappings: bool,
    /// How often the app-navigation hotkey fired.
    navigation: u32,
    /// What the screenshot command did last.
    screenshot: crate::live::LiveScreenshot,
}

impl Overlays {
    fn toggle_pad(&mut self) {
        self.pad = !self.pad;
    }

    fn toggle_mappings(&mut self) {
        self.mappings = !self.mappings;
    }
}

/// Sleep and idle power-off.
struct PowerState {
    /// Keep the computer awake while a mapping plays output, and whether it is held awake.
    prevent_sleep: bool,
    awake: bool,
    /// Turn wireless controllers off after this long without input.
    off_after: Option<u64>,
    idle: HashMap<DeviceId, Idle>,
}

/// Whether a profile needs global keyboard/mouse capture.
fn needs_capture(p: &Profile) -> bool {
    let kbm = |c: &Control| c.is_kbm();
    p.mappings.iter().any(|m| m.trigger.0.iter().any(kbm))
        || p.analog
            .iter()
            .any(|r| r.source == AnalogSource::Mouse || r.while_held.iter().any(kbm))
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Runtime on the backends of the current OS.
    pub fn new() -> Self {
        Self::with_platform(Platform::native())
    }

    /// Runtime on explicit backends (tests use `NullPlatform::platform()`).
    pub(crate) fn with_platform(mut platform: Platform) -> Self {
        let pads = Grouping::new(std::mem::take(&mut platform.gamepads));
        Runtime {
            platform,
            pads,
            sessions: Vec::new(),
            kbm: KbmInput {
                filter: CaptureFilter::new(),
                rx: None,
                buf: Vec::new(),
            },
            clock: Clock::new(),
            snapshots: Vec::new(),
            device_ids: Vec::new(),
            pad_buf: Vec::new(),
            binding: DefaultBinding::default(),
            hidden: Vec::new(),
            echo: None,
            suspended: false,
            remap_off_requested: false,
            hotkeys: HotkeyState::default(),
            pulses: confirm::Pulses::default(),
            test_turn: None,
            calibration: Calibrator::default(),
            overlays: Overlays::default(),
            sent_leds: HashMap::new(),
            power: PowerState {
                prevent_sleep: true,
                awake: false,
                off_after: None,
                idle: HashMap::new(),
            },
        }
    }

    /// True once after a profile turned remapping off with its `remap_off` command or the
    /// emergency-stop hotkey.
    pub(crate) fn take_remap_off_request(&mut self) -> bool {
        std::mem::take(&mut self.remap_off_requested)
    }

    /// Pause remapping without unloading the profiles: devices keep being polled, every
    /// output is released and nothing is injected until resumed. Used while the GUI
    /// window has focus, so mapped keys do not land in the editor.
    pub(crate) fn set_suspended(&mut self, on: bool) {
        if on == self.suspended {
            return;
        }
        self.suspended = on;
        if on {
            self.release_all(true);
        }
        self.sync_filter_enabled();
    }

    /// Let go of every session's output; `quiet` also stops their feedback.
    fn release_all(&mut self, quiet: bool) {
        let now = self.now_ms();
        for s in &mut self.sessions {
            let out = s.engine.release_all();
            s.emit(&mut self.platform, &out, now);
            if quiet {
                s.quiet(&mut self.pads);
            }
        }
    }

    /// Stop every profile and ask the service to turn remapping off.
    fn remap_off(&mut self) {
        self.stop();
        self.remap_off_requested = true;
    }

    /// The capture filter swallows input only while a profile runs and nothing pauses it.
    fn sync_filter_enabled(&self) {
        self.kbm
            .filter
            .set_enabled(!self.sessions.is_empty() && !self.suspended);
    }

    pub(crate) fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Record readable input/action events of the default session, drained with
    /// [`Runtime::take_events`].
    pub fn set_echo(&mut self, on: bool) {
        self.echo = on.then(Echo::default);
    }

    pub fn take_events(&mut self) -> Vec<String> {
        self.echo
            .as_mut()
            .map(|e| std::mem::take(&mut e.events))
            .unwrap_or_default()
    }

    pub(crate) fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Move the runtime's clock forward without waiting.
    #[cfg(test)]
    pub(crate) fn advance_ms(&mut self, ms: u64) {
        self.clock.advance(ms);
    }

    fn session(&self, owner: Option<&DeviceId>) -> Option<&Session> {
        self.sessions.iter().find(|s| s.owner.as_ref() == owner)
    }

    /// Profile of the default session.
    pub fn profile(&self) -> Option<&Profile> {
        self.session(None).map(|s| s.engine.profile())
    }

    pub(crate) fn profile_path(&self) -> Option<&PathBuf> {
        self.session(None).and_then(|s| s.path.as_ref())
    }

    /// File of the profile a controller's own session runs.
    pub(crate) fn device_profile_path(&self, id: &DeviceId) -> Option<&PathBuf> {
        self.session(Some(id)).and_then(|s| s.path.as_ref())
    }

    /// The library moved from `from` to `to`: sessions running one of its files name the file
    /// in its new place (the profile itself is unchanged, so nothing restarts).
    pub(crate) fn library_moved(&mut self, from: &Path, to: &Path) {
        for path in self.sessions.iter_mut().filter_map(|s| s.path.as_mut()) {
            crate::slots::follow_move(path, from, to);
        }
    }

    pub fn layer(&self) -> u8 {
        self.session(None).map_or(0, |s| s.engine.layer())
    }

    /// The crosshair the default session shows.
    pub(crate) fn crosshair(&self) -> Option<&Crosshair> {
        let s = self.session(None)?;
        s.engine.profile().crosshairs.get(s.crosshair_index()?)
    }

    /// Whether the live controller overlay and the mappings overlay are on.
    pub(crate) fn overlays(&self) -> (bool, bool) {
        (self.overlays.pad, self.overlays.mappings)
    }

    /// How often the app-navigation hotkey fired (the app toggles at each change).
    pub(crate) fn navigation_presses(&self) -> u32 {
        self.overlays.navigation
    }

    /// What the screenshot command did last (the app tells of each new one).
    pub(crate) fn screenshot(&self) -> &crate::live::LiveScreenshot {
        &self.overlays.screenshot
    }

    /// How generic pads' inputs map to pad controls, by model.
    pub(crate) fn set_layouts(&mut self, layouts: Vec<GenericLayout>) {
        self.pads.set_layouts(Arc::new(layouts));
    }

    /// Every device the backends report, blocked and grouped ones included.
    pub(crate) fn seen_devices(&self) -> Vec<SeenDevice> {
        self.pads.seen().to_vec()
    }

    /// Show `leds(controller)` on every connected controller, sending only changes.
    pub(crate) fn set_indicators(&mut self, leds: impl Fn(&DeviceId) -> Leds) {
        self.sent_leds.retain(|id, _| self.device_ids.contains(id));
        for id in &self.device_ids {
            let want = leds(id);
            match self.sent_leds.get_mut(id) {
                Some(sent) if *sent == want => continue,
                Some(sent) => *sent = want,
                None => {
                    self.sent_leds.insert(id.clone(), want);
                }
            }
            self.pads.set_leds(id, want);
        }
    }

    /// The radial menu the default session shows, and its pointed sector.
    pub(crate) fn radial(&self) -> Option<(&RadialMenu, Option<usize>)> {
        let s = self.session(None)?;
        let v = s.radial?;
        Some((s.engine.profile().radial_menus.get(v.menu)?, v.pointed))
    }

    /// Problems of every session; those of a controller's own session name the controller.
    pub(crate) fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.sessions {
            match &s.owner {
                None => out.extend(s.warnings.iter().cloned()),
                Some(id) => out.extend(s.warnings.iter().map(|w| format!("{id}: {w}"))),
            }
        }
        out
    }

    /// Every running session, the default one first.
    pub(crate) fn sessions(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(Session::info).collect()
    }

    /// Virtual pad of the default session.
    pub(crate) fn virtual_pad(&self) -> Option<VirtualPadKind> {
        self.session(None)
            .and_then(|s| s.vpad.as_ref())
            .map(|v| v.kind())
    }

    pub(crate) fn hidden_devices(&self) -> usize {
        self.hidden.len()
    }

    pub fn devices(&self) -> &[PadSnapshot] {
        &self.snapshots
    }

    /// The platform's driver probe (it can run on another thread).
    pub fn driver_probe(&self) -> fn() -> DriverReport {
        self.platform.probe_drivers
    }

    pub(crate) fn platform_mut(&mut self) -> &mut Platform {
        &mut self.platform
    }

    /// Activate a profile in the default session (replacing its current one).
    pub fn apply(&mut self, profile: Profile, path: Option<PathBuf>) -> Result<()> {
        self.start(None, profile, path)
    }

    /// Activate a profile in a controller's own session (replacing its current one).
    pub(crate) fn apply_device(
        &mut self,
        id: &DeviceId,
        profile: Profile,
        path: Option<PathBuf>,
    ) -> Result<()> {
        self.start(Some(id.clone()), profile, path)
    }

    fn start(
        &mut self,
        owner: Option<DeviceId>,
        mut profile: Profile,
        path: Option<PathBuf>,
    ) -> Result<()> {
        self.stop_session(owner.as_ref());
        // Without virtual controllers (macOS) the config runs with keyboard and mouse; the file
        // keeps the one it names for Windows and Linux.
        if !self.platform.virtual_pads.exist() {
            profile.output.virtual_pad = VirtualPadType::None;
        }
        let mut warnings = Vec::new();
        let kind = match profile.output.virtual_pad {
            VirtualPadType::Xbox360 => Some(VirtualPadKind::Xbox360),
            VirtualPadType::DualShock4 => Some(VirtualPadKind::DualShock4),
            VirtualPadType::None => None,
        };
        let vpad = match kind {
            Some(kind) => Some(
                self.platform
                    .virtual_pads
                    .create(kind)
                    .context("cannot create the virtual controller")?,
            ),
            None => None,
        };
        let key_repeat = KeyRepeater::new(profile.virtual_devices.keyboard.repeat);
        let capture = needs_capture(&profile);
        let engine = Engine::new(profile);
        let rumble_gap = Every::new(rumble_gap_ms(&engine.profile().vibration));
        warnings.extend(engine.warnings().iter().cloned());
        let session = Session {
            owner,
            hide_wanted: engine.profile().output.hide_physical
                && (vpad.is_some() || self.platform.hider.hides_without_virtual_pad()),
            needs_capture: capture,
            engine,
            path,
            vpad,
            device: None,
            warnings,
            game_rumble: Rumble::default(),
            sent_rumble: None,
            rumble_gap,
            game_lightbar: None,
            sent_lightbar: None,
            sent_triggers: None,
            key_repeat,
            clip: None,
            crosshair: CrosshairChoice::default(),
            radial: None,
            output: SessionOutput::default(),
        };
        for w in &session.warnings {
            tracing::warn!("{w}");
        }
        tracing::info!(profile = %session.engine.profile().name, device = ?session.owner, "profile applied");
        if session.owner.is_none() {
            self.sessions.insert(0, session);
        } else {
            self.sessions.push(session);
        }
        self.sessions_changed();
        Ok(())
    }

    /// Deactivate every profile and restore everything.
    pub fn stop(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        while !self.sessions.is_empty() {
            self.end_session(0);
        }
        self.sessions_changed();
    }

    /// Deactivate the default session's profile.
    pub(crate) fn stop_default(&mut self) {
        self.stop_session(None);
    }

    /// Deactivate a controller's own session.
    pub(crate) fn stop_device(&mut self, id: &DeviceId) {
        self.stop_session(Some(id));
    }

    fn stop_session(&mut self, owner: Option<&DeviceId>) {
        let Some(i) = self.sessions.iter().position(|s| s.owner.as_ref() == owner) else {
            return;
        };
        self.end_session(i);
        self.sessions_changed();
    }

    /// Release session `i`'s output, stop its feedback and drop it (unplugging its virtual
    /// pad); the caller brings the rest in line.
    fn end_session(&mut self, i: usize) {
        let mut s = self.sessions.remove(i);
        let out = s.engine.release_all();
        let now = self.now_ms();
        s.emit(&mut self.platform, &out, now);
        s.quiet(&mut self.pads);
        tracing::info!(profile = %s.engine.profile().name, device = ?s.owner, "profile stopped");
    }

    /// Bring capture, the ignore list, the capture filter and hiding in line with the sessions.
    fn sessions_changed(&mut self) {
        let ignore = VirtualIgnoreList {
            xinput_slots: self
                .sessions
                .iter()
                .filter_map(|s| s.vpad.as_ref()?.xinput_slot())
                .collect(),
        };
        self.pads.ignore_virtual(&ignore);
        let capture = self.sessions.iter().any(|s| s.needs_capture);
        if capture && self.kbm.rx.is_none() {
            match self.platform.kbm_capture.start(self.kbm.filter.clone()) {
                Ok(rx) => self.kbm.rx = Some(rx),
                Err(e) => {
                    // Retried on every change of sessions: warn once per session.
                    let warning = format!("keyboard/mouse capture unavailable: {e}");
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.needs_capture)
                        && !s.warnings.contains(&warning)
                    {
                        s.warnings.push(warning);
                    }
                }
            }
        } else if !capture && self.kbm.rx.is_some() {
            self.platform.kbm_capture.stop();
            self.kbm.rx = None;
        }
        self.update_filter();
        self.sync_filter_enabled();
        self.bind_sessions();
        self.sync_hiding();
    }

    /// What the capture filter swallows: the mutes of the session that gets keyboard and
    /// mouse input (the first: the default one, else a controller's own).
    fn update_filter(&self) {
        self.kbm.filter.clear();
        self.kbm.filter.set_source(KbmSource::All);
        let Some(s) = self.sessions.first() else {
            return;
        };
        self.kbm.filter.set_source(s.engine.profile().kbm_source);
        let m = s.engine.mutes();
        for k in m.keys {
            self.kbm.filter.set_key(k, true);
        }
        for b in m.mouse_buttons {
            self.kbm.filter.set_mouse_button(b, true);
        }
        for w in m.wheel {
            self.kbm.filter.set_wheel(w, true);
        }
        self.kbm.filter.set_mouse_move(m.mouse_move);
    }

    /// Gyro calibration of every controller seen.
    pub(crate) fn calibration(&self) -> &Calibrator {
        &self.calibration
    }

    pub(crate) fn calibration_mut(&mut self) -> &mut Calibrator {
        &mut self.calibration
    }

    /// Run one engine step for every session. Call about once per millisecond.
    pub fn tick(&mut self) {
        let now = self.now_ms();
        self.send_test_turn(now);
        self.poll_devices(now);
        self.watch_idle(now);
        self.keep_awake();
        self.kbm.buf.clear();
        if let Some(rx) = &self.kbm.rx {
            self.kbm.buf.extend(rx.try_iter());
        }
        self.pad_buf.clear();
        self.pad_buf.extend(self.snapshots.iter().map(|s| s.state));
        for (id, action) in self.detect_hotkeys(now) {
            if self.run_hotkey(id, action, now).is_break() {
                return;
            }
        }
        self.drive_pulses(now);
        if !self.suspended {
            self.tick_sessions(now);
        }
    }

    /// Carry out a hotkey the runtime handles itself, or hand it to the service (slots and
    /// the remap switch change the service's state). Breaks when remapping went off.
    fn run_hotkey(&mut self, id: DeviceId, action: HotkeyAction, now: u64) -> ControlFlow<()> {
        tracing::info!(device = %id, ?action, "hotkey");
        if let Some(echo) = &mut self.echo {
            echo.events.push(format!("hotkey      {action:?}"));
        }
        match action {
            HotkeyAction::Slot1
            | HotkeyAction::Slot2
            | HotkeyAction::Slot3
            | HotkeyAction::Slot4
            | HotkeyAction::ToggleRemap => {
                self.hotkeys.actions.push((id, action));
                return ControlFlow::Continue(());
            }
            HotkeyAction::StopMacros => {
                for s in self
                    .sessions
                    .iter_mut()
                    .filter(|s| s.device.as_ref() == Some(&id))
                {
                    s.engine.stop_all();
                }
            }
            HotkeyAction::EmergencyStop => {
                // Off first: ending the sessions quiets the motors, which would cut the
                // pattern's first pulse.
                self.remap_off();
                self.confirm_hotkey(&id, Confirm::for_action(action, false));
                self.drive_pulses(now);
                return ControlFlow::Break(());
            }
            HotkeyAction::PadOverlay => self.overlays.toggle_pad(),
            HotkeyAction::MappingsOverlay => self.overlays.toggle_mappings(),
            HotkeyAction::AppNavigation => {
                self.overlays.navigation = self.overlays.navigation.wrapping_add(1);
            }
        }
        self.confirm_hotkey(&id, Confirm::for_action(action, true));
        ControlFlow::Continue(())
    }

    /// Run every session's engine, send its output and feedback, and carry out the commands
    /// its mappings issued.
    fn tick_sessions(&mut self, now: u64) {
        let mut commands = Vec::new();
        for i in 0..self.sessions.len() {
            let s = &mut self.sessions[i];
            let pad = s.device.as_ref().and_then(|id| {
                let i = self.snapshots.iter().position(|p| &p.info.id == id)?;
                Some(self.pad_buf[i])
            });
            let kbm: &[KbmEvent] = if i == 0 { &self.kbm.buf } else { &[] };
            let out = s.engine.tick(Input {
                now_ms: now,
                pad: pad.as_ref(),
                kbm,
            });
            s.emit(&mut self.platform, &out, now);
            let pulsing = s.device.as_ref().is_some_and(|d| self.pulses.playing(d));
            s.feedback(&mut self.pads, out.rumble, pulsing, now);
            if s.owner.is_none()
                && let Some(echo) = &mut self.echo
            {
                echo.record(pad.as_ref(), &out, now);
            }
            if out.layer_changed && i == 0 {
                self.update_filter();
            }
            commands.extend(out.commands.into_iter().map(|c| (i, c)));
        }
        for (i, c) in commands {
            if !self.run_command(i, c) {
                return;
            }
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use dp_core::engine::Output;

    #[test]
    fn output_follows_presses_and_releases() {
        let mut o = SessionOutput::default();
        o.observe(&Output {
            keys: vec![(Key::W, true), (Key::A, true)],
            mouse_buttons: vec![(MouseButton::Left, true)],
            pad: Some(PadState::default()),
            ..Output::default()
        });
        assert!(o.keys.len() == 2 && o.keys.contains(&Key::A) && o.keys.contains(&Key::W));
        assert_eq!(o.mouse_buttons, [MouseButton::Left]);
        assert!(o.pad.is_some());
        // A repeated press changes nothing; releases remove.
        o.observe(&Output {
            keys: vec![(Key::W, true), (Key::A, false)],
            mouse_buttons: vec![(MouseButton::Left, false)],
            ..Output::default()
        });
        assert_eq!(o.keys, [Key::W]);
        assert!(o.mouse_buttons.is_empty());
        assert!(o.pad.is_some(), "the last pad state stays");
    }
}
