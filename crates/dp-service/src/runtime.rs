//! The runtime loop: polls devices, drives the engine, and applies its output to the
//! platform backends.

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
use dp_platform::{
    CaptureFilter, DeviceId, KbmEvent, PadSnapshot, Platform, VirtualIgnoreList, VirtualPad,
    VirtualPadKind,
};
use serde::Serialize;

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

/// Everything needed to run one profile against the real devices.
pub struct Runtime {
    platform: Platform,
    engine: Option<Engine>,
    profile_path: Option<PathBuf>,
    vpad: Option<Box<dyn VirtualPad>>,
    filter: Arc<CaptureFilter>,
    kbm_rx: Option<Receiver<KbmEvent>>,
    kbm_buf: Vec<KbmEvent>,
    epoch: Instant,
    snapshots: Vec<PadSnapshot>,
    primary: Option<DeviceId>,
    preferred: Option<DeviceId>,
    hidden: Vec<String>,
    hide_wanted: bool,
    game_rumble: Rumble,
    sent_rumble: Option<Rumble>,
    /// Light bar colour the game set on a virtual DualShock 4.
    game_lightbar: Option<Rgb>,
    /// What the controller was last told to show, and which controller.
    sent_lightbar: Option<(DeviceId, Option<Rgb>)>,
    warnings: Vec<String>,
    echo: Option<Echo>,
    suspended: bool,
    remap_off_requested: bool,
    hotkeys: Hotkeys,
    hotkey_detector: HotkeyDetector,
    hotkeys_paused: bool,
    hotkey_actions: Vec<HotkeyAction>,
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
            engine: None,
            profile_path: None,
            vpad: None,
            filter: CaptureFilter::new(),
            kbm_rx: None,
            kbm_buf: Vec::new(),
            epoch: Instant::now(),
            snapshots: Vec::new(),
            primary: None,
            preferred: None,
            hidden: Vec::new(),
            hide_wanted: false,
            game_rumble: Rumble::default(),
            sent_rumble: None,
            game_lightbar: None,
            sent_lightbar: None,
            warnings: Vec::new(),
            echo: None,
            suspended: false,
            remap_off_requested: false,
            hotkeys: Hotkeys::default(),
            hotkey_detector: HotkeyDetector::new(),
            hotkeys_paused: false,
            hotkey_actions: Vec::new(),
        }
    }

    /// True once after a profile turned remapping off with its `remap_off` command.
    pub fn take_remap_off_request(&mut self) -> bool {
        std::mem::take(&mut self.remap_off_requested)
    }

    /// Pause remapping without unloading the profile: devices keep being polled, every
    /// output is released and nothing is injected until resumed. Used while the GUI
    /// window has focus, so mapped keys do not land in the editor.
    pub fn set_suspended(&mut self, on: bool) {
        if on == self.suspended {
            return;
        }
        self.suspended = on;
        if on {
            if let Some(engine) = &mut self.engine {
                let out = engine.release_all();
                self.emit(&out);
            }
            self.filter.enabled.store(false, Ordering::Relaxed);
            self.set_physical_rumble(Rumble::default());
        } else {
            self.filter
                .enabled
                .store(self.engine.is_some(), Ordering::Relaxed);
        }
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Prefer this controller as the input device when it is connected.
    pub fn set_preferred_device(&mut self, id: Option<DeviceId>) {
        self.preferred = id;
        self.primary = None; // re-pick on the next poll
    }

    /// Controller currently used as input.
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

    fn set_physical_rumble(&mut self, r: Rumble) {
        if let Some(id) = self.primary.clone() {
            for b in &mut self.platform.gamepads {
                b.set_rumble(&id, r);
            }
        }
        self.sent_rumble = Some(r);
    }

    /// Record readable input/action events, drained with [`Runtime::take_events`].
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

    pub fn profile(&self) -> Option<&Profile> {
        self.engine.as_ref().map(Engine::profile)
    }

    pub fn profile_path(&self) -> Option<&PathBuf> {
        self.profile_path.as_ref()
    }

    pub fn layer(&self) -> u8 {
        self.engine.as_ref().map_or(0, Engine::layer)
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
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

    pub fn virtual_pad(&self) -> Option<VirtualPadKind> {
        self.vpad.as_ref().map(|v| v.kind())
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

    /// Activate a profile (replacing the current one).
    pub fn apply(&mut self, profile: Profile, path: Option<PathBuf>) -> Result<()> {
        self.stop();
        self.warnings.clear();

        let kind = match profile.output.virtual_pad {
            VirtualPadType::Xbox360 => Some(VirtualPadKind::Xbox360),
            VirtualPadType::DualShock4 => Some(VirtualPadKind::DualShock4),
            VirtualPadType::None => None,
        };
        if let Some(kind) = kind {
            let pad = self
                .platform
                .virtual_pads
                .create(kind)
                .context("cannot create the virtual controller")?;
            let ignore = VirtualIgnoreList {
                xinput_slots: pad.xinput_slot().into_iter().collect(),
            };
            for b in &mut self.platform.gamepads {
                b.ignore_virtual(&ignore);
            }
            self.vpad = Some(pad);
        }

        if needs_capture(&profile) {
            match self.platform.kbm_capture.start(self.filter.clone()) {
                Ok(rx) => self.kbm_rx = Some(rx),
                Err(e) => self
                    .warnings
                    .push(format!("keyboard/mouse capture unavailable: {e}")),
            }
        }

        let engine = Engine::new(profile);
        self.warnings.extend(engine.warnings().iter().cloned());
        self.hide_wanted = engine.profile().output.hide_physical && self.vpad.is_some();
        self.engine = Some(engine);
        self.update_filter();
        self.filter
            .enabled
            .store(!self.suspended, Ordering::Relaxed);
        self.profile_path = path;
        self.sync_hiding();
        for w in &self.warnings {
            tracing::warn!("{w}");
        }
        tracing::info!(profile = %self.profile().map(|p| p.name.as_str()).unwrap_or(""), "profile applied");
        Ok(())
    }

    /// Deactivate the current profile and restore everything.
    pub fn stop(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            let out = engine.release_all();
            self.emit(&out);
            tracing::info!(profile = %engine.profile().name, "profile stopped");
        }
        self.vpad = None;
        for b in &mut self.platform.gamepads {
            b.ignore_virtual(&VirtualIgnoreList::default());
        }
        self.platform.kbm_capture.stop();
        self.kbm_rx = None;
        self.filter.enabled.store(false, Ordering::Relaxed);
        self.filter.clear();
        self.hide_wanted = false;
        if !self.hidden.is_empty() {
            if let Err(e) = self.platform.hider.restore() {
                tracing::warn!("could not unhide devices: {e}");
            }
            self.hidden.clear();
        }
        if self.sent_rumble.take().is_some()
            && let Some(id) = self.primary.clone()
        {
            for b in &mut self.platform.gamepads {
                b.set_rumble(&id, Rumble::default());
            }
        }
        self.game_rumble = Rumble::default();
        self.game_lightbar = None;
        if let Some((id, Some(_))) = self.sent_lightbar.take() {
            for b in &mut self.platform.gamepads {
                b.set_lightbar(&id, None);
            }
        }
        self.profile_path = None;
    }

    fn update_filter(&self) {
        let Some(engine) = &self.engine else { return };
        let m = engine.mutes();
        self.filter.clear();
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

    /// Hide all connected physical pads (and physical XInput devices) while a profile with
    /// `hide_physical` drives a virtual pad.
    fn sync_hiding(&mut self) {
        if !self.hide_wanted {
            return;
        }
        let mut want: Vec<String> = self
            .snapshots
            .iter()
            .flat_map(|s| s.info.instance_paths.iter().cloned())
            .collect();
        #[cfg(windows)]
        if self.snapshots.iter().any(|s| s.info.backend == "xinput") {
            want.extend(dp_platform::windows::physical_xusb_instances());
        }
        want.sort();
        want.dedup();
        if want == self.hidden {
            return;
        }
        match self.platform.hider.hide(&want) {
            Ok(()) => self.hidden = want,
            Err(e) => {
                let msg = format!(
                    "cannot hide the physical controller (games may see double input): {e}"
                );
                tracing::warn!("{msg}");
                self.warnings.push(msg);
                self.hide_wanted = false;
            }
        }
    }

    fn poll_devices(&mut self) {
        let before: Vec<DeviceId> = self.snapshots.iter().map(|s| s.info.id.clone()).collect();
        self.snapshots.clear();
        for b in &mut self.platform.gamepads {
            b.poll(&mut self.snapshots);
        }
        let ids: Vec<DeviceId> = self.snapshots.iter().map(|s| s.info.id.clone()).collect();
        let preferred = self.preferred.as_ref().filter(|p| ids.contains(p));
        let stale = self.primary.as_ref().is_none_or(|p| !ids.contains(p));
        if stale || (preferred.is_some() && preferred != self.primary.as_ref()) {
            self.primary = preferred.cloned().or_else(|| ids.first().cloned());
            if let Some(p) = &self.primary {
                tracing::info!(device = %p, "using controller");
            }
        }
        if ids != before {
            self.sync_hiding();
        }
    }

    /// Run one engine step. Call about once per millisecond.
    pub fn tick(&mut self) {
        let now = self.now_ms();
        self.poll_devices();
        self.kbm_buf.clear();
        if let Some(rx) = &self.kbm_rx {
            self.kbm_buf.extend(rx.try_iter());
        }
        let mut pad = self
            .primary
            .as_ref()
            .and_then(|id| self.snapshots.iter().find(|s| &s.info.id == id))
            .map(|s| s.state);
        if let Some(action) = self.detect_hotkey(pad.as_mut(), now) {
            tracing::info!(?action, "hotkey");
            if let Some(echo) = &mut self.echo {
                echo.events.push(format!("hotkey      {action:?}"));
            }
            match action {
                HotkeyAction::StopMacros => {
                    if let Some(engine) = &mut self.engine {
                        engine.stop_all();
                    }
                }
                HotkeyAction::EmergencyStop => {
                    self.stop();
                    self.remap_off_requested = true;
                    return;
                }
                other => self.hotkey_actions.push(other),
            }
        }
        if self.suspended {
            return;
        }
        let Some(engine) = &mut self.engine else {
            return;
        };
        let out = engine.tick(Input {
            now_ms: now,
            pad: pad.as_ref(),
            kbm: &self.kbm_buf,
        });
        self.emit(&out);
        if out.layer_changed {
            self.update_filter();
        }
        if let Some(echo) = &mut self.echo {
            echo.record(pad.as_ref(), &out, now);
        }

        // Rumble: mapping rumble overrides what the game asks the virtual pad for.
        if let Some(r) = self.vpad.as_mut().and_then(|v| v.take_rumble()) {
            self.game_rumble = r;
        }
        let want = out.rumble.unwrap_or(self.game_rumble);
        if self.sent_rumble != Some(want)
            && let Some(id) = self.primary.clone()
        {
            for b in &mut self.platform.gamepads {
                b.set_rumble(&id, want);
            }
            self.sent_rumble = Some(want);
        }
        self.forward_lightbar();

        for c in out.commands {
            match c {
                Command::RemapOff => {
                    tracing::info!("remap-off command");
                    self.stop();
                    self.remap_off_requested = true;
                    return;
                }
                Command::Launch { path, args } => {
                    let args: Vec<&str> = args.split_whitespace().collect();
                    if let Err(e) = std::process::Command::new(&path).args(&args).spawn() {
                        tracing::warn!("cannot launch {path}: {e}");
                    }
                }
                Command::ToggleGyro => tracing::info!("gyro toggled"),
                Command::Unsupported(name) => {
                    tracing::debug!("ignoring unsupported command {name}")
                }
            }
        }
    }

    /// Run the hotkey detector on the controller in use and withhold the buttons of a
    /// complete hotkey from the engine.
    fn detect_hotkey(&mut self, pad: Option<&mut PadState>, now: u64) -> Option<HotkeyAction> {
        if self.hotkeys_paused || self.suspended {
            self.hotkey_detector.reset();
            return None;
        }
        let buttons = pad.as_ref().map_or(0, |p| p.buttons);
        let fired = self.hotkey_detector.update(&self.hotkeys, buttons, now);
        if let Some(p) = pad {
            p.buttons &= !self.hotkey_detector.withheld();
        }
        fired
    }

    /// The controller hotkeys in effect.
    pub fn hotkeys(&self) -> &Hotkeys {
        &self.hotkeys
    }

    pub fn set_hotkeys(&mut self, hotkeys: Hotkeys) {
        self.hotkeys = hotkeys;
        self.hotkey_detector.reset();
    }

    /// Ignore hotkeys for a while (the app is recording a new one).
    pub fn set_hotkeys_paused(&mut self, on: bool) {
        self.hotkeys_paused = on;
    }

    /// Hotkeys that fired and that the service has to carry out (slots, remap toggle).
    pub fn take_hotkey_actions(&mut self) -> Vec<HotkeyAction> {
        std::mem::take(&mut self.hotkey_actions)
    }

    /// Show the light bar colour the game set on the virtual pad on the controller in use.
    fn forward_lightbar(&mut self) {
        if let Some(c) = self.vpad.as_mut().and_then(|v| v.take_lightbar()) {
            self.game_lightbar = Some(c);
        }
        let Some(id) = self.primary.clone() else {
            return;
        };
        let want = (id, self.game_lightbar);
        if self.sent_lightbar.as_ref() == Some(&want) {
            return;
        }
        if let Some((old, Some(_))) = &self.sent_lightbar
            && *old != want.0
        {
            for b in &mut self.platform.gamepads {
                b.set_lightbar(old, None);
            }
        }
        for b in &mut self.platform.gamepads {
            b.set_lightbar(&want.0, want.1);
        }
        self.sent_lightbar = Some(want);
    }

    fn emit(&mut self, out: &Output) {
        if let (Some(vpad), Some(pad)) = (&mut self.vpad, &out.pad)
            && let Err(e) = vpad.update(pad)
        {
            tracing::warn!("virtual pad update failed: {e}");
        }
        let kbm = &mut self.platform.kbm_out;
        for &(k, down) in &out.keys {
            kbm.key(k, down);
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

    /// Null backends plus one fake controller backend and fake virtual pads.
    pub fn platform() -> (Platform, Shared) {
        let world = Shared::default();
        let mut p = NullPlatform::platform();
        p.gamepads.push(Box::new(FakeBackend(world.clone())));
        p.virtual_pads = Box::new(FakeFactory(world.clone()));
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
