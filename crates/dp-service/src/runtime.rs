//! The runtime loop: polls devices, drives the engines, and applies their output to the
//! platform backends.
//!
//! Several profiles can run at once, one per *session*. The default session runs on the
//! controller in use (the preferred one, else the first one without a session of its
//! own); a controller can also have its own session with its own profile and virtual pad.
//! Keyboard and mouse are shared: their input goes to the default session (or the first
//! session if there is none), and every session's keyboard and mouse output goes out.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use dp_core::control::{Control, mouse_button_name, pad_button_name};
use dp_core::engine::{Engine, Input, Output};
use dp_core::hotkeys::{HotkeyAction, HotkeyDetector, Hotkeys};
use dp_core::pad::{PadButton, PadState, Rgb, Rumble, Trigger};
use dp_core::profile::{AnalogSource, Command, Profile, VirtualPadType};
use dp_core::repeat::KeyRepeater;
use dp_platform::{
    CaptureFilter, DeviceId, KbmEvent, PadSnapshot, Platform, PowerAction, VirtualIgnoreList,
    VirtualPad, VirtualPadKind,
};
use serde::Serialize;

use crate::calibration::Calibrator;

/// Availability of one optional driver.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DriverState {
    pub available: bool,
    /// Why it is unavailable, if it is.
    pub error: Option<String>,
}

impl std::fmt::Display for DriverState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.error, self.available) {
            (_, true) => f.write_str("ok"),
            (Some(e), false) => write!(f, "unavailable ({e})"),
            (None, false) => f.write_str("unavailable"),
        }
    }
}

impl DriverState {
    fn from(r: dp_platform::Result<()>) -> Self {
        match r {
            Ok(()) => DriverState {
                available: true,
                error: None,
            },
            Err(e) => DriverState {
                available: false,
                error: Some(e.to_string()),
            },
        }
    }
}

/// Drivers Doppelpad can use: ViGEmBus for virtual pads, HidHide for hiding devices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DriverStatus {
    pub virtual_pads: DriverState,
    pub device_hiding: DriverState,
}

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

/// One profile running against one controller.
struct Session {
    /// `None` for the default session.
    owner: Option<DeviceId>,
    engine: Engine,
    path: Option<PathBuf>,
    vpad: Option<Box<dyn VirtualPad>>,
    /// The controller the session reads this tick.
    device: Option<DeviceId>,
    warnings: Vec<String>,
    hide_wanted: bool,
    needs_capture: bool,
    game_rumble: Rumble,
    sent_rumble: Option<(DeviceId, Rumble)>,
    /// Light bar colour the game set on a virtual DualShock 4.
    game_lightbar: Option<Rgb>,
    /// What the controller was last told to show, and which controller.
    sent_lightbar: Option<(DeviceId, Option<Rgb>)>,
    key_repeat: KeyRepeater,
}

impl Session {
    fn info(&self) -> SessionInfo {
        SessionInfo {
            owner: self.owner.as_ref().map(|d| d.0.clone()),
            device: self.device.as_ref().map(|d| d.0.clone()),
            profile: self.engine.profile().name.clone(),
            profile_path: self.path.as_ref().map(|p| p.display().to_string()),
            layer: self.engine.layer(),
            virtual_pad: self.vpad.as_ref().map(|v| v.kind()),
        }
    }
}

/// Everything needed to run profiles against the real devices.
pub struct Runtime {
    platform: Platform,
    /// The default session (if any) comes first.
    sessions: Vec<Session>,
    filter: Arc<CaptureFilter>,
    kbm_rx: Option<Receiver<KbmEvent>>,
    kbm_buf: Vec<KbmEvent>,
    epoch: Instant,
    snapshots: Vec<PadSnapshot>,
    /// The controller of the default session.
    primary: Option<DeviceId>,
    preferred: Option<DeviceId>,
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
}

/// One full in-game turn sent as mouse motion, so the user can check `counts_per_turn`.
#[derive(Clone, Copy, Debug)]
struct TestTurn {
    counts: f32,
    start: u64,
    ms: u64,
    sent: f32,
}

/// Human-readable log of inputs and the actions they caused (for `doppelpad run`).
#[derive(Default)]
struct Echo {
    events: Vec<String>,
    pad_buttons: u32,
    triggers: [bool; 2],
    virtual_buttons: u32,
    mouse: (f32, f32),
    mouse_since: u64,
}

/// How often accumulated mouse motion is reported.
const ECHO_MOUSE_MS: u64 = 300;
/// Trigger pull reported as "pressed" in the echo.
const ECHO_TRIGGER: f32 = 0.1;

impl Echo {
    fn record(&mut self, pad: Option<&PadState>, out: &Output, now: u64) {
        if let Some(p) = pad {
            let changed = p.buttons ^ self.pad_buttons;
            for b in PadButton::ALL {
                if changed & b.bit() != 0 {
                    let what = if p.pressed(b) { "pressed" } else { "released" };
                    self.events
                        .push(format!("controller  {:<9} {what}", pad_button_name(b)));
                }
            }
            self.pad_buttons = p.buttons;
            for (i, (t, name)) in [(Trigger::Left, "LT"), (Trigger::Right, "RT")]
                .into_iter()
                .enumerate()
            {
                let down = p.axis(t.axis()) >= ECHO_TRIGGER;
                if down != self.triggers[i] {
                    let what = if down { "pressed" } else { "released" };
                    self.events.push(format!("controller  {name:<9} {what}"));
                    self.triggers[i] = down;
                }
            }
        }
        for (k, down) in &out.keys {
            self.events.push(format!(
                "  -> key           {k} {}",
                if *down { "down" } else { "up" }
            ));
        }
        for (b, down) in &out.mouse_buttons {
            let what = if *down { "down" } else { "up" };
            self.events.push(format!(
                "  -> mouse button  {} {what}",
                mouse_button_name(*b)
            ));
        }
        for (w, n) in &out.wheel {
            self.events.push(format!("  -> mouse wheel   {w:?} x{n}"));
        }
        if let Some(vp) = &out.pad {
            let changed = vp.buttons ^ self.virtual_buttons;
            for b in PadButton::ALL {
                if changed & b.bit() != 0 {
                    let what = if vp.pressed(b) { "down" } else { "up" };
                    self.events
                        .push(format!("  -> virtual pad   {} {what}", pad_button_name(b)));
                }
            }
            self.virtual_buttons = vp.buttons;
        }
        for c in &out.commands {
            self.events.push(format!("  -> command       {c:?}"));
        }
        if out.layer_changed {
            self.events.push(format!("layer {}", out.layer));
        }
        self.mouse.0 += out.mouse_move.0;
        self.mouse.1 += out.mouse_move.1;
        if now.saturating_sub(self.mouse_since) >= ECHO_MOUSE_MS {
            if self.mouse.0.abs() >= 1.0 || self.mouse.1.abs() >= 1.0 {
                self.events.push(format!(
                    "  -> mouse moved   {:+.0}, {:+.0} px",
                    self.mouse.0, self.mouse.1
                ));
            }
            self.mouse = (0.0, 0.0);
            self.mouse_since = now;
        }
    }
}

/// Whether a profile needs global keyboard/mouse capture.
fn needs_capture(p: &Profile) -> bool {
    let kbm = |c: &Control| !c.is_pad();
    p.mappings.iter().any(|m| m.trigger.0.iter().any(kbm))
        || p.analog
            .iter()
            .any(|r| r.source == AnalogSource::Mouse || r.while_held.iter().any(kbm))
}

/// Send a session's keyboard/mouse and virtual pad output.
fn emit(s: &mut Session, platform: &mut Platform, out: &Output, now: u64) {
    if let (Some(vpad), Some(pad)) = (&mut s.vpad, &out.pad)
        && let Err(e) = vpad.update(pad)
    {
        tracing::warn!("virtual pad update failed: {e}");
    }
    let kbm = &mut platform.kbm_out;
    for &(k, down) in &out.keys {
        kbm.key(k, down);
    }
    s.key_repeat.observe(&out.keys, now);
    if let Some(k) = s.key_repeat.due(now) {
        kbm.key(k, true);
    }
    for &(b, down) in &out.mouse_buttons {
        kbm.mouse_button(b, down);
    }
    if out.mouse_move != (0.0, 0.0) {
        kbm.mouse_move(out.mouse_move.0, out.mouse_move.1);
    }
    for &(w, n) in &out.wheel {
        kbm.wheel(w, n);
    }
}

/// Rumble for the session's controller: mapping rumble overrides what the game asks the
/// virtual pad for. Also forwards the light bar colour the game set.
fn feedback(s: &mut Session, platform: &mut Platform, mapping_rumble: Option<Rumble>) {
    if let Some(r) = s.vpad.as_mut().and_then(|v| v.take_rumble()) {
        s.game_rumble = r;
    }
    if let Some(c) = s.vpad.as_mut().and_then(|v| v.take_lightbar()) {
        s.game_lightbar = Some(c);
    }
    let Some(id) = s.device.clone() else { return };
    let rumble = (id.clone(), mapping_rumble.unwrap_or(s.game_rumble));
    if s.sent_rumble.as_ref() != Some(&rumble) {
        for b in &mut platform.gamepads {
            b.set_rumble(&rumble.0, rumble.1);
        }
        s.sent_rumble = Some(rumble);
    }
    let light = (id, s.game_lightbar);
    if s.sent_lightbar.as_ref() == Some(&light) {
        return;
    }
    if let Some((old, Some(_))) = &s.sent_lightbar
        && *old != light.0
    {
        for b in &mut platform.gamepads {
            b.set_lightbar(old, None);
        }
    }
    for b in &mut platform.gamepads {
        b.set_lightbar(&light.0, light.1);
    }
    s.sent_lightbar = Some(light);
}

/// Stop a session's feedback on its controller.
fn quiet(s: &mut Session, platform: &mut Platform) {
    if let Some((id, r)) = s.sent_rumble.take()
        && r != Rumble::default()
    {
        for b in &mut platform.gamepads {
            b.set_rumble(&id, Rumble::default());
        }
    }
    if let Some((id, Some(_))) = s.sent_lightbar.take() {
        for b in &mut platform.gamepads {
            b.set_lightbar(&id, None);
        }
    }
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
    pub fn with_platform(platform: Platform) -> Self {
        Runtime {
            platform,
            sessions: Vec::new(),
            filter: CaptureFilter::new(),
            kbm_rx: None,
            kbm_buf: Vec::new(),
            epoch: Instant::now(),
            snapshots: Vec::new(),
            primary: None,
            preferred: None,
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
        }
    }

    /// True once after a profile turned remapping off with its `remap_off` command or the
    /// emergency-stop hotkey.
    pub fn take_remap_off_request(&mut self) -> bool {
        std::mem::take(&mut self.remap_off_requested)
    }

    /// Pause remapping without unloading the profiles: devices keep being polled, every
    /// output is released and nothing is injected until resumed. Used while the GUI
    /// window has focus, so mapped keys do not land in the editor.
    pub fn set_suspended(&mut self, on: bool) {
        if on == self.suspended {
            return;
        }
        self.suspended = on;
        if on {
            let now = self.now_ms();
            for s in &mut self.sessions {
                let out = s.engine.release_all();
                emit(s, &mut self.platform, &out, now);
                quiet(s, &mut self.platform);
            }
            self.filter.enabled.store(false, Ordering::Relaxed);
        } else {
            self.filter
                .enabled
                .store(!self.sessions.is_empty(), Ordering::Relaxed);
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Prefer this controller for the default session when it is connected.
    pub fn set_preferred_device(&mut self, id: Option<DeviceId>) {
        self.preferred = id;
        self.primary = None; // re-pick on the next poll
    }

    /// Controller of the default session.
    pub fn primary_device(&self) -> Option<&DeviceId> {
        self.primary.as_ref()
    }

    /// Check which optional drivers are usable (this probes the drivers; not for every tick).
    pub fn drivers(&self) -> DriverStatus {
        DriverStatus {
            virtual_pads: DriverState::from(self.platform.virtual_pads.available()),
            device_hiding: DriverState::from(self.platform.hider.available()),
        }
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

    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn session(&self, owner: Option<&DeviceId>) -> Option<&Session> {
        self.sessions.iter().find(|s| s.owner.as_ref() == owner)
    }

    /// Profile of the default session.
    pub fn profile(&self) -> Option<&Profile> {
        self.session(None).map(|s| s.engine.profile())
    }

    pub fn profile_path(&self) -> Option<&PathBuf> {
        self.session(None).and_then(|s| s.path.as_ref())
    }

    /// File of the profile a controller's own session runs.
    pub fn device_profile_path(&self, id: &DeviceId) -> Option<&PathBuf> {
        self.session(Some(id)).and_then(|s| s.path.as_ref())
    }

    /// Whether a controller has a session of its own.
    pub fn has_device_session(&self, id: &DeviceId) -> bool {
        self.session(Some(id)).is_some()
    }

    pub fn layer(&self) -> u8 {
        self.session(None).map_or(0, |s| s.engine.layer())
    }

    /// Problems of every session; those of a controller's own session name the controller.
    pub fn warnings(&self) -> Vec<String> {
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
    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(Session::info).collect()
    }

    /// Controllers the backends found but cannot use, explained for the user.
    pub fn device_problems(&self) -> Vec<String> {
        self.platform
            .gamepads
            .iter()
            .flat_map(|b| b.problems())
            .collect()
    }

    /// Look for controllers now and retry the ones that could not be opened.
    pub fn rescan_devices(&mut self) {
        for b in &mut self.platform.gamepads {
            b.rescan();
        }
    }

    /// Connected controllers the hiding driver hides from Doppelpad itself, explained for
    /// the user. Asks the driver, so call it now and then rather than every tick.
    pub fn hiding_problems(&self) -> Vec<String> {
        self.platform.hider.problems()
    }

    /// Virtual pad of the default session.
    pub fn virtual_pad(&self) -> Option<VirtualPadKind> {
        self.session(None)
            .and_then(|s| s.vpad.as_ref())
            .map(|v| v.kind())
    }

    pub fn hidden_devices(&self) -> usize {
        self.hidden.len()
    }

    pub fn devices(&self) -> &[PadSnapshot] {
        &self.snapshots
    }

    pub fn platform_mut(&mut self) -> &mut Platform {
        &mut self.platform
    }

    /// Activate a profile in the default session (replacing its current one).
    pub fn apply(&mut self, profile: Profile, path: Option<PathBuf>) -> Result<()> {
        self.start(None, profile, path)
    }

    /// Activate a profile in a controller's own session (replacing its current one).
    pub fn apply_device(
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
            game_lightbar: None,
            sent_lightbar: None,
            key_repeat,
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
    pub fn stop_default(&mut self) {
        self.stop_session(None);
    }

    /// Deactivate a controller's own session.
    pub fn stop_device(&mut self, id: &DeviceId) {
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
        quiet(&mut s, &mut self.platform);
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
        for b in &mut self.platform.gamepads {
            b.ignore_virtual(&ignore);
        }
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
        self.filter.enabled.store(
            !self.sessions.is_empty() && !self.suspended,
            Ordering::Relaxed,
        );
        self.bind_sessions();
        self.sync_hiding();
    }

    /// The session that gets keyboard and mouse input: the default one, else the first.
    fn kbm_session(&self) -> Option<usize> {
        (!self.sessions.is_empty()).then_some(0)
    }

    fn update_filter(&self) {
        self.filter.clear();
        let Some(i) = self.kbm_session() else { return };
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
        self.filter
            .mouse_move
            .store(m.mouse_move, Ordering::Relaxed);
    }

    /// Hide the controllers of sessions whose profile hides the physical controller behind
    /// a virtual one (with every physical XInput device if one of them is an XInput pad).
    fn sync_hiding(&mut self) {
        let mut want: Vec<String> = Vec::new();
        let mut xinput = false;
        for s in self.sessions.iter().filter(|s| s.hide_wanted) {
            let Some(snap) = s
                .device
                .as_ref()
                .and_then(|id| self.snapshots.iter().find(|x| &x.info.id == id))
            else {
                continue;
            };
            want.extend(snap.info.instance_paths.iter().cloned());
            xinput |= snap.info.backend == "xinput";
        }
        #[cfg(windows)]
        if xinput {
            want.extend(dp_platform::windows::physical_xusb_instances());
        }
        #[cfg(not(windows))]
        let _ = xinput;
        want.sort();
        want.dedup();
        if want == self.hidden {
            return;
        }
        let result = if want.is_empty() {
            self.platform.hider.restore()
        } else {
            self.platform.hider.hide(&want)
        };
        match result {
            Ok(()) => self.hidden = want,
            Err(e) => {
                let msg = format!(
                    "cannot hide the physical controller (games may see double input): {e}"
                );
                tracing::warn!("{msg}");
                for s in self.sessions.iter_mut().filter(|s| s.hide_wanted) {
                    s.warnings.push(msg.clone());
                    s.hide_wanted = false;
                }
            }
        }
    }

    /// Which controller each session reads: its own, or for the default session the
    /// preferred (else first) controller that has no session of its own.
    fn bind_sessions(&mut self) {
        let ids: Vec<DeviceId> = self.snapshots.iter().map(|s| s.info.id.clone()).collect();
        let owned: Vec<DeviceId> = self
            .sessions
            .iter()
            .filter_map(|s| s.owner.clone())
            .collect();
        let free: Vec<&DeviceId> = ids.iter().filter(|id| !owned.contains(id)).collect();
        let preferred = self.preferred.as_ref().filter(|p| free.contains(p));
        let current = self.primary.as_ref().filter(|p| free.contains(p));
        let primary = preferred.or(current).or(free.first().copied()).cloned();
        if primary != self.primary {
            if let Some(p) = &primary {
                tracing::info!(device = %p, "using controller");
            }
            self.primary = primary;
        }
        for s in &mut self.sessions {
            s.device = match &s.owner {
                Some(id) => ids.contains(id).then(|| id.clone()),
                None => self.primary.clone(),
            };
        }
    }

    fn poll_devices(&mut self, now: u64) {
        let before: Vec<DeviceId> = self.snapshots.iter().map(|s| s.info.id.clone()).collect();
        self.snapshots.clear();
        for b in &mut self.platform.gamepads {
            b.poll(&mut self.snapshots);
        }
        for s in &mut self.snapshots {
            self.calibration.apply(&s.info.id.0, &mut s.state, now);
        }
        let ids: Vec<DeviceId> = self.snapshots.iter().map(|s| s.info.id.clone()).collect();
        let devices_changed = ids != before;
        let primary_lost = self.primary.as_ref().is_some_and(|p| !ids.contains(p));
        let preferred_back = self.preferred.is_some() && self.preferred != self.primary;
        if devices_changed || primary_lost || preferred_back || self.primary.is_none() {
            self.bind_sessions();
        }
        if devices_changed {
            self.detectors.retain(|id, _| ids.contains(id));
            self.sync_hiding();
        }
    }

    /// Gyro calibration of every controller seen.
    pub fn calibration(&self) -> &Calibrator {
        &self.calibration
    }

    pub fn calibration_mut(&mut self) -> &mut Calibrator {
        &mut self.calibration
    }

    /// Send `counts` of horizontal mouse motion over `ms`, starting after `delay_ms` (time to
    /// switch to the game): one full turn when `counts` matches the game.
    pub fn test_turn(&mut self, counts: f32, delay_ms: u64, ms: u64) {
        self.test_turn = Some(TestTurn {
            counts,
            start: self.now_ms() + delay_ms,
            ms: ms.max(1),
            sent: 0.0,
        });
    }

    fn send_test_turn(&mut self, now: u64) {
        let Some(t) = &mut self.test_turn else {
            return;
        };
        if now < t.start {
            return;
        }
        let done = ((now - t.start) as f32 / t.ms as f32).min(1.0);
        let step = t.counts * done - t.sent;
        if step != 0.0 {
            self.platform.kbm_out.mouse_move(step, 0.0);
            t.sent += step;
        }
        if done >= 1.0 {
            self.test_turn = None;
        }
    }

    /// Run one engine step for every session. Call about once per millisecond.
    pub fn tick(&mut self) {
        let now = self.now_ms();
        self.send_test_turn(now);
        self.poll_devices(now);
        self.kbm_buf.clear();
        if let Some(rx) = &self.kbm_rx {
            self.kbm_buf.extend(rx.try_iter());
        }
        let mut pads: HashMap<DeviceId, PadState> = self
            .snapshots
            .iter()
            .map(|s| (s.info.id.clone(), s.state))
            .collect();
        for (id, action) in self.detect_hotkeys(&mut pads, now) {
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
            let pad = s.device.as_ref().and_then(|id| pads.get(id)).copied();
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
            feedback(s, &mut self.platform, out.rumble);
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

    /// Carry out a host command of session `i`; `false` once remapping was turned off.
    fn run_command(&mut self, i: usize, c: Command) -> bool {
        match c {
            Command::RemapOff => {
                tracing::info!("remap-off command");
                self.stop();
                self.remap_off_requested = true;
                return false;
            }
            Command::Launch { path, args } => {
                let args: Vec<&str> = args.split_whitespace().collect();
                if let Err(e) = std::process::Command::new(&path).args(&args).spawn() {
                    tracing::warn!("cannot launch {path}: {e}");
                }
            }
            Command::ToggleGyro
            | Command::GyroOn
            | Command::GyroOff
            | Command::ResetTilt
            | Command::SwitchGyroAxis => {
                tracing::info!(command = ?c, "gyro switched")
            }
            Command::Shutdown
            | Command::Restart
            | Command::Sleep
            | Command::Hibernate
            | Command::LogOff => {
                let action = match c {
                    Command::Shutdown => PowerAction::Shutdown,
                    Command::Restart => PowerAction::Restart,
                    Command::Sleep => PowerAction::Sleep,
                    Command::Hibernate => PowerAction::Hibernate,
                    _ => PowerAction::LogOff,
                };
                tracing::info!(?action, "power command");
                // Let go of everything before the session ends or the PC sleeps.
                let now = self.now_ms();
                for s in &mut self.sessions {
                    let out = s.engine.release_all();
                    emit(s, &mut self.platform, &out, now);
                }
                if let Err(e) = self.platform.system.power(action) {
                    tracing::warn!("{action:?} failed: {e}");
                }
            }
            Command::Screenshot => match self.platform.system.screenshot() {
                Ok(path) => tracing::info!(path = %path.display(), "screenshot saved"),
                Err(e) => tracing::warn!("screenshot failed: {e}"),
            },
            Command::CloseWindow => {
                if let Err(e) = self.platform.system.close_foreground_window() {
                    tracing::warn!("cannot close the window: {e}");
                }
            }
            Command::Unsupported(name) => {
                tracing::debug!(session = i, "ignoring unsupported command {name}")
            }
        }
        true
    }

    /// Run the hotkey detectors on every controller and withhold the buttons of complete
    /// hotkeys from the engines.
    fn detect_hotkeys(
        &mut self,
        pads: &mut HashMap<DeviceId, PadState>,
        now: u64,
    ) -> Vec<(DeviceId, HotkeyAction)> {
        if self.hotkeys_paused || self.suspended {
            self.detectors.clear();
            return Vec::new();
        }
        let mut fired = Vec::new();
        for (id, pad) in pads.iter_mut() {
            let d = self.detectors.entry(id.clone()).or_default();
            if let Some(a) = d.update(&self.hotkeys, pad.buttons, now) {
                fired.push((id.clone(), a));
            }
            pad.buttons &= !d.withheld();
        }
        fired
    }

    /// The controller hotkeys in effect.
    pub fn hotkeys(&self) -> &Hotkeys {
        &self.hotkeys
    }

    pub fn set_hotkeys(&mut self, hotkeys: Hotkeys) {
        self.hotkeys = hotkeys;
        self.detectors.clear();
    }

    /// Ignore hotkeys for a while (the app is recording a new one).
    pub fn set_hotkeys_paused(&mut self, on: bool) {
        self.hotkeys_paused = on;
    }

    /// Hotkeys that fired, with the controller they fired on, that the service has to carry
    /// out (slots, remap toggle).
    pub fn take_hotkey_actions(&mut self) -> Vec<(DeviceId, HotkeyAction)> {
        std::mem::take(&mut self.hotkey_actions)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Raise the OS timer resolution for the lifetime of the guard so 1 ms sleeps are accurate.
pub struct TimerResolution;

impl TimerResolution {
    pub fn high() -> Self {
        #[cfg(windows)]
        // SAFETY: plain winmm call, undone in Drop.
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        TimerResolution
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: matches timeBeginPeriod in `high`.
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Fake controllers and virtual pads for runtime tests.

    use std::sync::{Arc, Mutex};

    use dp_core::pad::{PadState, Rgb, Rumble};
    use dp_platform::{
        DeviceId, DeviceInfo, GamepadBackend, NullPlatform, PadFamily, PadSnapshot, Platform,
        VirtualPad, VirtualPadFactory, VirtualPadKind,
    };

    /// Everything the fakes saw, shared with the test.
    #[derive(Default)]
    pub struct World {
        pub pads: Vec<PadSnapshot>,
        pub rumble: Vec<(DeviceId, Rumble)>,
        pub lightbar: Vec<(DeviceId, Option<Rgb>)>,
        /// Virtual pads created so far, with the last state each was sent.
        pub virtual_pads: Vec<(VirtualPadKind, Option<PadState>)>,
        /// What the game "sends" to the next virtual pad poll.
        pub game_rumble: Option<Rumble>,
        pub game_lightbar: Option<Rgb>,
        /// Keys sent to the virtual keyboard, in order.
        pub keys: Vec<(dp_core::keys::Key, bool)>,
        /// System commands carried out, by name.
        pub system: Vec<String>,
    }

    pub type Shared = Arc<Mutex<World>>;

    pub fn pad(id: &str, family: PadFamily) -> PadSnapshot {
        PadSnapshot {
            info: Arc::new(DeviceInfo {
                id: DeviceId(id.into()),
                name: id.into(),
                family,
                vendor_id: 0,
                product_id: 0,
                backend: "fake".into(),
                instance_paths: vec![format!("path-{id}")],
                joycon: None,
                has_motion: true,
                has_touchpad: true,
                has_rumble: true,
            }),
            state: PadState::default(),
        }
    }

    struct FakeBackend(Shared);

    impl GamepadBackend for FakeBackend {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
            out.extend(self.0.lock().unwrap().pads.iter().cloned());
        }
        fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
            self.0.lock().unwrap().rumble.push((id.clone(), rumble));
        }
        fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
            self.0.lock().unwrap().lightbar.push((id.clone(), color));
        }
    }

    struct FakeFactory(Shared);

    impl VirtualPadFactory for FakeFactory {
        fn available(&self) -> dp_platform::Result<()> {
            Ok(())
        }
        fn create(&mut self, kind: VirtualPadKind) -> dp_platform::Result<Box<dyn VirtualPad>> {
            let mut w = self.0.lock().unwrap();
            w.virtual_pads.push((kind, None));
            Ok(Box::new(FakePad {
                world: self.0.clone(),
                index: w.virtual_pads.len() - 1,
                kind,
            }))
        }
    }

    struct FakePad {
        world: Shared,
        index: usize,
        kind: VirtualPadKind,
    }

    impl VirtualPad for FakePad {
        fn kind(&self) -> VirtualPadKind {
            self.kind
        }
        fn update(&mut self, state: &PadState) -> dp_platform::Result<()> {
            self.world.lock().unwrap().virtual_pads[self.index].1 = Some(*state);
            Ok(())
        }
        fn take_rumble(&mut self) -> Option<Rumble> {
            self.world.lock().unwrap().game_rumble.take()
        }
        fn take_lightbar(&mut self) -> Option<Rgb> {
            self.world.lock().unwrap().game_lightbar.take()
        }
    }

    struct FakeSystem(Shared);

    impl dp_platform::SystemControl for FakeSystem {
        fn power(&mut self, action: dp_platform::PowerAction) -> dp_platform::Result<()> {
            self.0.lock().unwrap().system.push(format!("{action:?}"));
            Ok(())
        }
        fn screenshot(&mut self) -> dp_platform::Result<std::path::PathBuf> {
            self.0.lock().unwrap().system.push("Screenshot".into());
            Ok("shot.png".into())
        }
        fn close_foreground_window(&mut self) -> dp_platform::Result<()> {
            self.0.lock().unwrap().system.push("CloseWindow".into());
            Ok(())
        }
    }

    struct FakeKbm(Shared);

    impl dp_platform::KbmOutput for FakeKbm {
        fn key(&mut self, key: dp_core::keys::Key, down: bool) {
            self.0.lock().unwrap().keys.push((key, down));
        }
        fn mouse_button(&mut self, _: dp_core::keys::MouseButton, _: bool) {}
        fn mouse_move(&mut self, _: f32, _: f32) {}
        fn wheel(&mut self, _: dp_core::keys::WheelDir, _: f32) {}
    }

    /// Null backends plus fake controller, virtual pad and keyboard backends.
    pub fn platform() -> (Platform, Shared) {
        let world = Shared::default();
        let mut p = NullPlatform::platform();
        p.gamepads.push(Box::new(FakeBackend(world.clone())));
        p.virtual_pads = Box::new(FakeFactory(world.clone()));
        p.kbm_out = Box::new(FakeKbm(world.clone()));
        p.system = Box::new(FakeSystem(world.clone()));
        (p, world)
    }
}

#[cfg(test)]
mod tests {
    use dp_core::hotkeys::Hotkeys;
    use dp_core::profile::VirtualPadType;
    use dp_platform::PadFamily;

    use super::testing::{self, pad};
    use super::*;

    fn ds4_profile() -> Profile {
        let mut p = Profile::default();
        p.output.virtual_pad = VirtualPadType::DualShock4;
        p
    }

    #[test]
    fn hotkey_buttons_do_not_reach_the_game() {
        use dp_core::pad::PadButton::*;
        let (platform, world) = testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(pad("pad1", PadFamily::Xbox));
        let mut rt = Runtime::with_platform(platform);
        rt.set_hotkeys(Hotkeys {
            hold_ms: 50,
            ..Default::default()
        });
        rt.apply(Profile::default(), None).unwrap();
        let mut press = |buttons: u32| {
            world.lock().unwrap().pads[0].state.buttons = buttons;
            rt.tick();
            world.lock().unwrap().virtual_pads[0].1.unwrap().buttons
        };
        assert_eq!(press(LB.bit()), LB.bit(), "a partial press passes through");
        let combo = LB.bit() | RB.bit() | X.bit();
        assert_eq!(press(combo | DpadUp.bit()), DpadUp.bit(), "hotkey withheld");
        assert_eq!(press(LB.bit() | RB.bit()), 0, "until released");
        assert_eq!(press(0), 0);
        assert_eq!(press(X.bit()), X.bit(), "free again");
    }

    #[test]
    fn held_keys_repeat_on_the_virtual_keyboard() {
        use dp_core::keys::Key;
        use dp_core::pad::PadButton;
        use dp_core::profile::{Action, KeyRepeat, Mapping, Press};
        let (platform, world) = testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(pad("pad1", PadFamily::Xbox));
        let mut rt = Runtime::with_platform(platform);
        let mut p = Profile::default();
        p.output.virtual_pad = VirtualPadType::None;
        p.virtual_devices.keyboard.repeat = KeyRepeat::Every(5);
        p.mappings.push(Mapping::new(
            dp_core::control::Control::Button(PadButton::A),
            vec![Action::Key {
                key: Key::Space,
                action: Press::Auto,
            }],
        ));
        rt.apply(p, None).unwrap();
        rt.tick();
        world.lock().unwrap().pads[0].state.set(PadButton::A, true);
        rt.tick();
        std::thread::sleep(std::time::Duration::from_millis(30));
        rt.tick();
        world.lock().unwrap().pads[0].state.set(PadButton::A, false);
        rt.tick();
        std::thread::sleep(std::time::Duration::from_millis(30));
        rt.tick();
        let keys = world.lock().unwrap().keys.clone();
        assert_eq!(keys.first(), Some(&(Key::Space, true)));
        assert_eq!(keys.last(), Some(&(Key::Space, false)), "{keys:?}");
        assert!(
            keys.iter().filter(|k| k.1).count() >= 2,
            "the held key repeated: {keys:?}"
        );
    }

    #[test]
    fn system_commands_reach_the_platform() {
        use dp_core::control::Control;
        use dp_core::pad::PadButton;
        use dp_core::profile::{Action, Mapping, Mode};
        let (platform, world) = testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(pad("pad1", PadFamily::Xbox));
        let mut rt = Runtime::with_platform(platform);
        let mut p = Profile::default();
        let mut m = Mapping::new(
            Control::Button(PadButton::A),
            [Command::Screenshot, Command::CloseWindow, Command::Sleep]
                .into_iter()
                .map(|command| Action::Command { command })
                .collect(),
        );
        m.mode = Mode::Once;
        p.mappings.push(m);
        rt.apply(p, None).unwrap();
        rt.tick();
        world.lock().unwrap().pads[0].state.set(PadButton::A, true);
        for _ in 0..3 {
            rt.tick();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        assert_eq!(
            world.lock().unwrap().system,
            ["Screenshot", "CloseWindow", "Sleep"]
        );
    }

    #[test]
    fn game_feedback_reaches_the_controller() {
        let (platform, world) = testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(pad("pad1", PadFamily::DualSense));
        let mut rt = Runtime::with_platform(platform);
        rt.apply(ds4_profile(), None).unwrap();
        rt.tick();

        let red = Rgb { r: 255, g: 0, b: 0 };
        let buzz = Rumble {
            low: 1.0,
            high: 0.5,
        };
        {
            let mut w = world.lock().unwrap();
            w.game_rumble = Some(buzz);
            w.game_lightbar = Some(red);
        }
        rt.tick();
        rt.tick();
        {
            let w = world.lock().unwrap();
            let id = DeviceId("pad1".into());
            assert_eq!(w.rumble.last(), Some(&(id.clone(), buzz)));
            assert_eq!(w.lightbar.last(), Some(&(id.clone(), Some(red))));
            assert_eq!(
                w.lightbar.iter().filter(|(_, c)| *c == Some(red)).count(),
                1,
                "sent once"
            );
            assert_eq!(w.virtual_pads[0].0, VirtualPadKind::DualShock4);
            assert!(
                w.virtual_pads[0].1.is_some(),
                "the virtual pad gets reports"
            );
        }

        rt.stop();
        let w = world.lock().unwrap();
        let id = DeviceId("pad1".into());
        assert_eq!(w.rumble.last(), Some(&(id.clone(), Rumble::default())));
        assert_eq!(w.lightbar.last(), Some(&(id, None)), "light bar released");
    }
}
