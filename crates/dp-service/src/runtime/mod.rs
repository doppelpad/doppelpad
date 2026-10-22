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
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use dp_core::control::Control;
use dp_core::engine::{Engine, Input};
use dp_core::hotkeys::{HotkeyAction, HotkeyDetector, Hotkeys};
use dp_core::pad::{PadState, Rumble};
use dp_core::profile::{AnalogSource, Crosshair, KbmSource, Profile, RadialMenu, VirtualPadType};
use dp_core::repeat::KeyRepeater;
use dp_platform::generic::GenericLayout;
use dp_platform::groups::{Grouping, SeenDevice};
use dp_platform::{
    CaptureFilter, DeviceId, GamepadBackend, KbmEvent, Leds, PadSnapshot, Platform,
    VirtualIgnoreList, VirtualPadKind,
};
use serde::Serialize;

use self::commands::TestTurn;
use self::echo::Echo;
use self::idle::Idle;
use self::session::{CrosshairChoice, Session, emit, feedback, quiet, rumble_gap_ms};
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
}

/// Everything needed to run profiles against the real devices.
pub struct Runtime {
    /// The platform's backends, its gamepad backends moved into `pads`.
    platform: Platform,
    /// Every gamepad backend, with the block list and the device groups applied.
    pads: Grouping,
    /// The default session (if any) comes first.
    sessions: Vec<Session>,
    filter: Arc<CaptureFilter>,
    kbm_rx: Option<Receiver<KbmEvent>>,
    kbm_buf: Vec<KbmEvent>,
    clock: Clock,
    snapshots: Vec<PadSnapshot>,
    /// The ids of `snapshots`, kept to notice a changed device list without allocating.
    device_ids: Vec<DeviceId>,
    /// The pad states of `snapshots` as the engines get them (hotkey buttons withheld).
    pad_buf: Vec<PadState>,
    /// The controller of the default session.
    primary: Option<DeviceId>,
    /// The controller picked in the app for the default session.
    preferred: Option<DeviceId>,
    /// The controller the default session used until it went away (unplugged, reset, or
    /// taken by a session of its own). The session goes back to it when it is free again.
    lost: Option<DeviceId>,
    hidden: Vec<String>,
    echo: Option<Echo>,
    suspended: bool,
    remap_off_requested: bool,
    hotkeys: Hotkeys,
    detectors: HashMap<DeviceId, HotkeyDetector>,
    hotkeys_paused: bool,
    hotkey_actions: Vec<(DeviceId, HotkeyAction)>,
    /// A calibration turn: counts to send, start, length and how much was sent.
    test_turn: Option<TestTurn>,
    /// Gyro bias per controller, taken off before anything reads the rates.
    calibration: Calibrator,
    /// The live controller overlay and the mappings overlay are on.
    pad_overlay: bool,
    mappings_overlay: bool,
    /// Player lights and microphone LED last sent, per controller.
    sent_leds: HashMap<DeviceId, Leds>,
    /// Keep the computer awake while a mapping plays output, and whether it is held awake.
    prevent_sleep: bool,
    awake: bool,
    /// Turn wireless controllers off after this long without input.
    power_off_after: Option<u64>,
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
            filter: CaptureFilter::new(),
            kbm_rx: None,
            kbm_buf: Vec::new(),
            clock: Clock::new(),
            snapshots: Vec::new(),
            device_ids: Vec::new(),
            pad_buf: Vec::new(),
            primary: None,
            preferred: None,
            lost: None,
            hidden: Vec::new(),
            echo: None,
            suspended: false,
            remap_off_requested: false,
            hotkeys: Hotkeys::default(),
            detectors: HashMap::new(),
            hotkeys_paused: false,
            test_turn: None,
            calibration: Calibrator::default(),
            hotkey_actions: Vec::new(),
            pad_overlay: false,
            mappings_overlay: false,
            sent_leds: HashMap::new(),
            prevent_sleep: true,
            awake: false,
            power_off_after: None,
            idle: HashMap::new(),
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
            let now = self.now_ms();
            for s in &mut self.sessions {
                let out = s.engine.release_all();
                emit(s, &mut self.platform, &out, now);
                quiet(s, &mut self.pads);
            }
            self.filter.set_enabled(false);
        } else {
            self.filter.set_enabled(!self.sessions.is_empty());
        }
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
        (self.pad_overlay, self.mappings_overlay)
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
        profile: Profile,
        path: Option<PathBuf>,
    ) -> Result<()> {
        self.stop_session(owner.as_ref());
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
            hide_wanted: engine.profile().output.hide_physical && vpad.is_some(),
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
        let owners: Vec<Option<DeviceId>> = self.sessions.iter().map(|s| s.owner.clone()).collect();
        for o in owners {
            self.stop_session(o.as_ref());
        }
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
        let mut s = self.sessions.remove(i);
        let out = s.engine.release_all();
        let now = self.now_ms();
        emit(&mut s, &mut self.platform, &out, now);
        quiet(&mut s, &mut self.pads);
        tracing::info!(profile = %s.engine.profile().name, device = ?s.owner, "profile stopped");
        drop(s); // unplugs the virtual pad
        self.sessions_changed();
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
        if capture && self.kbm_rx.is_none() {
            match self.platform.kbm_capture.start(self.filter.clone()) {
                Ok(rx) => self.kbm_rx = Some(rx),
                Err(e) => {
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.needs_capture) {
                        s.warnings
                            .push(format!("keyboard/mouse capture unavailable: {e}"));
                    }
                }
            }
        } else if !capture && self.kbm_rx.is_some() {
            self.platform.kbm_capture.stop();
            self.kbm_rx = None;
        }
        self.update_filter();
        self.filter
            .set_enabled(!self.sessions.is_empty() && !self.suspended);
        self.bind_sessions();
        self.sync_hiding();
    }

    /// The session that gets keyboard and mouse input: the default one, else the first.
    fn kbm_session(&self) -> Option<usize> {
        (!self.sessions.is_empty()).then_some(0)
    }

    fn update_filter(&self) {
        self.filter.clear();
        self.filter.set_source(KbmSource::All);
        let Some(i) = self.kbm_session() else { return };
        self.filter
            .set_source(self.sessions[i].engine.profile().kbm_source);
        let m = self.sessions[i].engine.mutes();
        for k in m.keys {
            self.filter.set_key(k, true);
        }
        for b in m.mouse_buttons {
            self.filter.set_mouse_button(b, true);
        }
        for w in m.wheel {
            self.filter.set_wheel(w, true);
        }
        self.filter.set_mouse_move(m.mouse_move);
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
        self.kbm_buf.clear();
        if let Some(rx) = &self.kbm_rx {
            self.kbm_buf.extend(rx.try_iter());
        }
        self.pad_buf.clear();
        self.pad_buf.extend(self.snapshots.iter().map(|s| s.state));
        for (id, action) in self.detect_hotkeys(now) {
            tracing::info!(device = %id, ?action, "hotkey");
            if let Some(echo) = &mut self.echo {
                echo.events.push(format!("hotkey      {action:?}"));
            }
            match action {
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
                    self.stop();
                    self.remap_off_requested = true;
                    return;
                }
                HotkeyAction::PadOverlay => self.pad_overlay = !self.pad_overlay,
                HotkeyAction::MappingsOverlay => self.mappings_overlay = !self.mappings_overlay,
                other => self.hotkey_actions.push((id, other)),
            }
        }
        if self.suspended {
            return;
        }
        let kbm_session = self.kbm_session();
        let mut commands = Vec::new();
        for i in 0..self.sessions.len() {
            let s = &mut self.sessions[i];
            let pad = s.device.as_ref().and_then(|id| {
                let i = self.snapshots.iter().position(|p| &p.info.id == id)?;
                Some(self.pad_buf[i])
            });
            let kbm: &[KbmEvent] = if Some(i) == kbm_session {
                &self.kbm_buf
            } else {
                &[]
            };
            let out = s.engine.tick(Input {
                now_ms: now,
                pad: pad.as_ref(),
                kbm,
            });
            emit(s, &mut self.platform, &out, now);
            feedback(s, &mut self.pads, out.rumble, now);
            if s.owner.is_none()
                && let Some(echo) = &mut self.echo
            {
                echo.record(pad.as_ref(), &out, now);
            }
            if out.layer_changed && Some(i) == kbm_session {
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
