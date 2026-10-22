//! The remapping service: owns the [`Runtime`], decides which profile is active
//! (slots, explicit one-off profiles, automatic per-application switching) and answers
//! control requests from the CLI (IPC) and the GUI.

use std::path::{Path, PathBuf};

use anyhow::Result;
use dp_core::hotkeys::{HotkeyAction, Hotkeys};
use dp_core::profile::Profile;
use dp_ipc::{Device, Request, Response, Status};
use dp_platform::Leds;
use dp_platform::generic::GenericLayout;
use dp_platform::groups::{DeviceGroup, DevicePolicy};

use crate::dsu::{DsuServer, DsuSettings};
use crate::httpapi::{HttpApi, HttpSettings, Job};
use crate::live::{LiveDevice, LiveState};
use crate::net::Listener;
use crate::runtime::{DriverStatus, Runtime};
use crate::slots::{PlayerLights, RuntimeOptions, SLOTS, SlotState};
use crate::timing::Every;

/// How often the foreground application is checked by default.
const FOREGROUND_POLL_MS: u64 = 250;
/// How often to check whether the user's desktop takes input.
const DESKTOP_POLL_MS: u64 = 250;
/// How often driver availability is re-probed.
const DRIVER_POLL_MS: u64 = 5000;
/// How long the editor's focus report is trusted by default although another program is in
/// front (the foreground check lags behind a focus change).
const GUI_FOCUS_GRACE_MS: u64 = 1000;

/// "on" or "off", as replies and the CLI say it.
pub fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[derive(Clone, Debug)]
pub struct ServiceOptions {
    /// Directory scanned for profiles that switch automatically by foreground application.
    pub profiles_dir: PathBuf,
    /// Where slots and switches are persisted; `None` keeps them in memory only.
    pub state_path: Option<PathBuf>,
    /// Where the controller hotkeys are persisted; `None` uses the defaults in memory.
    pub hotkeys_path: Option<PathBuf>,
    /// Where the gyro calibration is persisted; `None` keeps it in memory only.
    pub calibration_path: Option<PathBuf>,
    /// Profile to apply at start, independent of the slots.
    pub initial_profile: Option<PathBuf>,
    /// Override the persisted auto-switch setting.
    pub auto_switch: Option<bool>,
    pub foreground_poll_ms: u64,
    /// How long the editor's focus report is trusted although another program is in front.
    pub gui_focus_grace_ms: u64,
}

impl ServiceOptions {
    pub fn new(profiles_dir: PathBuf) -> Self {
        ServiceOptions {
            profiles_dir,
            state_path: None,
            hotkeys_path: None,
            calibration_path: None,
            initial_profile: None,
            auto_switch: None,
            foreground_poll_ms: FOREGROUND_POLL_MS,
            gui_focus_grace_ms: GUI_FOCUS_GRACE_MS,
        }
    }

    /// Options that keep the slots, hotkeys and calibration in the user's settings folder,
    /// where the app and the daemon share them.
    pub fn with_user_files(profiles_dir: PathBuf) -> Self {
        let dir = dp_config::config_dir();
        ServiceOptions {
            state_path: Some(dir.join(dp_config::STATE_FILE)),
            hotkeys_path: Some(dir.join(dp_config::HOTKEYS_FILE)),
            calibration_path: Some(dir.join(dp_config::CALIBRATION_FILE)),
            ..ServiceOptions::new(profiles_dir)
        }
    }
}

/// A manual gyro calibration samples this long.
const GYRO_CALIBRATION_MS: u64 = 3000;
/// An automatically updated gyro calibration is saved at most this often.
const CALIBRATION_SAVE_MS: u64 = 60_000;

/// A test turn starts this long after it is asked for (time to switch to the game) ...
const TEST_TURN_DELAY_MS: u64 = 3000;
/// ... and takes this long.
const TEST_TURN_MS: u64 = 1000;

/// Commands beyond the IPC protocol, used by the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum ServiceCmd {
    /// A request of the IPC protocol (`doppelpad apply|stop|status|...`).
    Ipc(Request),
    /// Put a config into a slot and make that slot active. `device`: the slots of that
    /// controller (which gets slots of its own if it has none yet), else the shared ones.
    ApplyToSlot {
        device: Option<String>,
        slot: usize,
        path: PathBuf,
    },
    SelectSlot {
        device: Option<String>,
        slot: usize,
    },
    ClearSlot {
        device: Option<String>,
        slot: usize,
    },
    /// Give a controller slots of its own (it then runs its own profile and virtual pad),
    /// or let it use the shared slots again.
    OwnSlots {
        device: String,
        on: bool,
    },
    SetRemap(bool),
    SetSuspended(bool),
    SetPreferredDevice(Option<String>),
    SetAutoSwitch(bool),
    /// What the controllers' player lights and microphone LED show.
    SetLights {
        player: PlayerLights,
        mic: bool,
    },
    /// Ignore a device completely, or use it again.
    SetBlocked {
        id: String,
        blocked: bool,
    },
    /// Replace the device groups.
    SetGroups(Vec<DeviceGroup>),
    /// Turn wireless controllers off after this many minutes without input; 0 never.
    SetPowerOffAfter(u32),
    /// Store how a generic pad model's inputs map to pad controls (replaces its old one).
    SetLayout(GenericLayout),
    /// Go back to the guessed layout for a generic pad model (`vvvv:pppp`).
    ForgetLayout(String),
    /// Turn the DSU motion server on or off, or move it.
    SetDsu(DsuSettings),
    /// Sleep, focus and disconnect behaviour.
    SetRuntimeOptions(RuntimeOptions),
    /// Turn the local HTTP API on or off, or change where it listens.
    SetHttp(HttpSettings),
    /// Give the HTTP API a new token (the old one stops working).
    NewHttpToken,
    /// Turn a wireless controller off now.
    PowerOff(String),
    /// Re-probe ViGEmBus / HidHide now.
    RefreshDrivers,
    /// Look for controllers now, e.g. after another program released them.
    RescanDevices,
    /// The controller hotkeys in effect ([`ServiceReply::Hotkeys`]).
    GetHotkeys,
    /// Replace the controller hotkeys and persist them.
    SetHotkeys(Hotkeys),
    /// Ignore hotkeys while the app records a new one.
    PauseHotkeys(bool),
    /// Send one full in-game turn (`counts` of mouse motion) after a short delay, to check a
    /// profile's turn calibration.
    TestTurn {
        counts: f32,
    },
    /// Measure a controller's gyro bias while it lies still (`None`: the controller in use).
    CalibrateGyro {
        device: Option<String>,
    },
    /// Keep estimating a controller's gyro bias while it rests, or not.
    SetGyroAuto {
        device: Option<String>,
        on: bool,
    },
    /// Forget a controller's gyro bias.
    ResetGyro {
        device: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServiceReply {
    Ipc(Response),
    Ok(String),
    Err(String),
    Hotkeys(Hotkeys),
}

/// The IPC form of a reply.
impl From<ServiceReply> for Response {
    fn from(reply: ServiceReply) -> Response {
        match reply {
            ServiceReply::Ipc(r) => r,
            other => other.into_result().into(),
        }
    }
}

impl ServiceReply {
    pub fn into_result(self) -> Result<String, String> {
        match self {
            ServiceReply::Ok(m) | ServiceReply::Ipc(Response::Ok { message: m }) => Ok(m),
            ServiceReply::Err(m) | ServiceReply::Ipc(Response::Error { message: m }) => Err(m),
            ServiceReply::Ipc(other) => Ok(format!("{other:?}")),
            ServiceReply::Hotkeys(h) => Ok(format!("{h:?}")),
        }
    }
}

pub struct Service {
    rt: Runtime,
    opts: ServiceOptions,
    library: dp_config::library::Library,
    state: SlotState,
    /// Profile applied explicitly over IPC (`doppelpad apply`); takes precedence over slots.
    oneoff: Option<PathBuf>,
    /// Profile applied because its application is in the foreground.
    auto_active: Option<PathBuf>,
    /// The application auto-switch last acted on (`None` while it is off).
    foreground: Option<String>,
    /// The foreground application is asked at most this often.
    fg_poll: Every,
    drivers: DriverStatus,
    driver_poll: Every,
    /// [`Runtime::hiding_problems`], refreshed with the driver status.
    hiding_problems: Vec<String>,
    last_error: Option<String>,
    /// Why saving the state failed during the command being handled.
    unsaved: Option<String>,
    /// Why a controller's own profile could not be applied, by device id.
    device_errors: std::collections::BTreeMap<String, String>,
    /// Connected controllers when their sessions were last brought in line.
    connected: Vec<String>,
    /// The DSU motion server.
    dsu: Listener<DsuServer>,
    /// The local HTTP API.
    http: Listener<HttpApi>,
    /// Why remapping is paused: the editor has focus, the desktop is locked or a secure
    /// desktop shows, the config's game is not in front.
    gui_suspended: bool,
    /// When the editor last reported getting the focus.
    gui_since: u64,
    locked: bool,
    focus_paused: bool,
    desktop_poll: Every,
    /// The application in front, as last asked.
    fg_now: Option<String>,
    /// Its process id, where the platform can tell.
    fg_pid: Option<u32>,
    /// The editor reports focus while another program is in front (a missed focus change).
    gui_stale: bool,
    /// Automatic gyro calibration changes are saved at most this often.
    calibration_save: Every,
}

fn load(path: &Path) -> Result<Profile> {
    let (profile, diags) = dp_config::load(path)?;
    for d in diags.iter() {
        tracing::info!("{}: {d}", path.display());
    }
    if diags.has_errors() {
        anyhow::bail!("{} has errors", path.display());
    }
    Ok(profile)
}

/// Connected controllers in IPC form.
pub fn ipc_devices(rt: &Runtime) -> Vec<Device> {
    rt.devices()
        .iter()
        .map(|s| Device {
            id: s.info.id.to_string(),
            name: s.info.name.clone(),
            backend: s.info.backend.clone(),
            vendor_id: s.info.vendor_id,
            product_id: s.info.product_id,
            instance_paths: s.info.instance_paths.clone(),
        })
        .collect()
}

impl Service {
    /// Create the service and apply the initial profile (or the active slot).
    pub fn new(mut rt: Runtime, opts: ServiceOptions) -> Result<Service> {
        let mut state = opts
            .state_path
            .as_deref()
            .map(SlotState::load)
            .unwrap_or_default();
        if let Some(a) = opts.auto_switch {
            state.auto_switch = a;
        }
        if let Some(path) = &opts.hotkeys_path {
            rt.set_hotkeys(crate::hotkeys::load(path));
        }
        if let Some(path) = &opts.calibration_path {
            *rt.calibration_mut() = crate::calibration::Calibrator::load(path);
        }
        let mut svc = Service {
            rt,
            library: Default::default(),
            state,
            oneoff: opts.initial_profile.clone(),
            auto_active: None,
            foreground: None,
            fg_poll: Every::new(opts.foreground_poll_ms),
            drivers: DriverStatus::default(),
            driver_poll: Every::new(DRIVER_POLL_MS),
            hiding_problems: Vec::new(),
            last_error: None,
            unsaved: None,
            device_errors: Default::default(),
            connected: Vec::new(),
            dsu: Listener::default(),
            http: Listener::default(),
            gui_suspended: false,
            gui_since: 0,
            locked: false,
            focus_paused: false,
            desktop_poll: Every::new(DESKTOP_POLL_MS),
            fg_now: None,
            fg_pid: None,
            gui_stale: false,
            calibration_save: Every::new(CALIBRATION_SAVE_MS),
            opts,
        };
        svc.apply_device_policy();
        svc.apply_dsu();
        svc.apply_http();
        svc.rt.set_prevent_sleep(svc.state.runtime.prevent_sleep);
        svc.reload();
        if let Some(p) = svc.oneoff.clone() {
            svc.apply_path(&p)?;
        } else {
            svc.apply_base();
        }
        Ok(svc)
    }

    /// Start, switch or stop the sessions of controllers with slots of their own: one runs
    /// while its controller is connected, remapping is on and its active slot has a config.
    fn sync_devices(&mut self) {
        let connected: Vec<String> = self
            .rt
            .devices()
            .iter()
            .map(|s| s.info.id.0.clone())
            .collect();
        // With the option on, a disconnected controller keeps its session (and so its
        // virtual controller, idle) until it is back.
        let keep = self.state.runtime.keep_offline_pads;
        let running: Vec<String> = self
            .rt
            .sessions()
            .into_iter()
            .filter_map(|s| s.owner)
            .collect();
        let wanted: Vec<(String, PathBuf)> = if self.state.remap_on {
            self.state
                .devices
                .iter()
                .filter(|(id, _)| connected.contains(id) || (keep && running.contains(id)))
                .filter_map(|(id, d)| Some((id.clone(), d.active_profile()?.clone())))
                .collect()
        } else {
            Vec::new()
        };
        for s in self.rt.sessions() {
            if let Some(owner) = s.owner
                && !wanted.iter().any(|(id, _)| *id == owner)
            {
                self.rt.stop_device(&dp_platform::DeviceId(owner.clone()));
                self.device_errors.remove(&owner);
            }
        }
        for (id, path) in wanted {
            let device = dp_platform::DeviceId(id.clone());
            if self.rt.device_profile_path(&device) == Some(&path) {
                continue;
            }
            let result =
                load(&path).and_then(|p| self.rt.apply_device(&device, p, Some(path.clone())));
            match result {
                Ok(()) => {
                    self.device_errors.remove(&id);
                }
                Err(e) => {
                    tracing::warn!(device = %id, "cannot apply {}: {e:#}", path.display());
                    self.device_errors.insert(id, format!("{e:#}"));
                }
            }
        }
        self.connected = connected;
    }

    /// The slot set a command addresses: a controller's own, else the shared one.
    fn has_own_slots(&self, device: Option<&str>) -> Option<String> {
        device
            .filter(|d| self.state.devices.contains_key(*d))
            .map(str::to_owned)
    }

    #[cfg(test)]
    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    #[cfg(test)]
    pub fn slots(&self) -> &SlotState {
        &self.state
    }

    fn reload(&mut self) {
        self.library = dp_config::library::scan(&self.opts.profiles_dir);
        let loose_errors = self
            .library
            .unsorted
            .iter()
            .filter_map(|c| c.error.as_ref());
        for e in self.library.errors.iter().chain(loose_errors) {
            tracing::warn!("{e}");
        }
        tracing::info!(
            games = self.library.games.len(),
            loose = self.loose_profiles(),
            dir = %self.opts.profiles_dir.display(),
            "profiles loaded"
        );
    }

    /// Loose profiles (directly in the profiles directory) that load.
    fn loose_profiles(&self) -> usize {
        self.library
            .unsorted
            .iter()
            .filter(|c| c.error.is_none())
            .count()
    }

    /// Config to activate for a foreground executable: a game's default config, else a
    /// loose profile listing the executable.
    fn auto_profile_for(&self, exe: &str) -> Option<PathBuf> {
        self.library.config_for_process(exe)
    }

    /// Change the state and save it.
    fn update_state(&mut self, f: impl FnOnce(&mut SlotState)) {
        f(&mut self.state);
        self.save_state();
    }

    /// Forget the one-off profile and the one auto-switch applied.
    fn clear_overrides(&mut self) {
        self.oneoff = None;
        self.auto_active = None;
    }

    /// Remapping off and saved, with nothing left to come back to.
    fn turn_remap_off(&mut self) {
        self.clear_overrides();
        self.update_state(|s| s.remap_on = false);
    }

    fn save_state(&mut self) {
        if let Some(path) = &self.opts.state_path
            && let Err(e) = self.state.save(path)
        {
            let msg = format!("cannot save {}: {e}", path.display());
            tracing::warn!("{msg}");
            self.unsaved = Some(msg);
        }
    }

    fn apply_path(&mut self, path: &Path) -> Result<()> {
        let result = load(path).and_then(|p| self.rt.apply(p, Some(path.to_path_buf())));
        self.last_error = result.as_ref().err().map(|e| format!("{e:#}"));
        result
    }

    /// Apply what should be active when no application-specific profile matches:
    /// the one-off profile, else the active slot (if remapping is on), else nothing.
    fn apply_base(&mut self) {
        let target = self.oneoff.clone().or_else(|| {
            self.state
                .remap_on
                .then(|| self.state.active_profile().cloned())
                .flatten()
        });
        match target {
            Some(p) if self.rt.profile_path() == Some(&p) => {}
            Some(p) => {
                if let Err(e) = self.apply_path(&p) {
                    tracing::warn!("cannot apply {}: {e:#}", p.display());
                }
            }
            None => {
                self.rt.stop_default();
                self.last_error = None;
            }
        }
    }

    fn status(&self) -> Status {
        Status {
            profile: self.rt.profile().map(|p| p.name.clone()),
            profile_path: self.rt.profile_path().map(|p| p.display().to_string()),
            layer: self.rt.layer(),
            auto_switch: self.state.auto_switch,
            foreground: self.fg_now.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| k.name().to_owned()),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
            slots: self
                .state
                .slots
                .iter()
                .map(|s| s.as_ref().map(|p| p.display().to_string()))
                .collect(),
            active_slot: self.state.active + 1,
            remap_on: self.state.remap_on,
        }
    }

    /// The reply to a slot or remap command in IPC form; slots are 1–4 over IPC.
    fn ipc_reply(&mut self, cmd: ServiceCmd) -> Response {
        self.handle(cmd).into()
    }

    /// Run a slot command for a 1-based slot number from IPC.
    fn ipc_slot(&mut self, slot: usize, cmd: impl FnOnce(usize) -> ServiceCmd) -> Response {
        if !(1..=SLOTS).contains(&slot) {
            return Response::error(format!("slot {slot} does not exist (1–{SLOTS})"));
        }
        self.ipc_reply(cmd(slot - 1))
    }

    fn warnings(&self) -> Vec<String> {
        let mut w = self.rt.warnings();
        w.extend(self.hiding_problems.iter().cloned());
        w.extend(self.rt.device_problems());
        w.extend(self.last_error.clone());
        w.extend(
            self.fg_now
                .as_deref()
                .and_then(crate::anticheat::lookup)
                .map(|a| a.warning()),
        );
        w.extend(
            self.device_errors
                .iter()
                .map(|(id, e)| format!("{id}: {e}")),
        );
        w
    }

    /// Answer one IPC request. `Shutdown` only acknowledges; the host stops the loop.
    pub fn handle_ipc(&mut self, req: Request) -> Response {
        self.unsaved = None;
        match (self.answer_ipc(req), self.unsaved.take()) {
            (Response::Ok { message }, Some(e)) => Response::ok(unsaved_note(&message, &e)),
            (r, _) => r,
        }
    }

    fn answer_ipc(&mut self, req: Request) -> Response {
        match req {
            Request::Status => Response::Status(self.status()),
            Request::Apply { path } => {
                let path = PathBuf::from(path);
                match self.apply_path(&path) {
                    Ok(()) => {
                        self.oneoff = Some(path.clone());
                        self.auto_active = None;
                        self.update_state(|s| s.remap_on = true);
                        self.sync_devices();
                        Response::ok(format!("applied {}", path.display()))
                    }
                    Err(e) => Response::error(format!("{e:#}")),
                }
            }
            Request::Stop => {
                self.rt.stop();
                self.last_error = None;
                self.turn_remap_off();
                Response::ok("remapping stopped")
            }
            Request::ApplyToSlot { path, slot, device } => {
                self.ipc_slot(slot, |slot| ServiceCmd::ApplyToSlot {
                    device,
                    slot,
                    path: PathBuf::from(path),
                })
            }
            Request::SelectSlot { slot, device } => {
                self.ipc_slot(slot, |slot| ServiceCmd::SelectSlot { device, slot })
            }
            Request::ClearSlot {
                slot: Some(slot),
                device,
            } => self.ipc_slot(slot, |slot| ServiceCmd::ClearSlot { device, slot }),
            Request::ClearSlot { slot: None, device } => {
                for slot in 0..SLOTS {
                    self.run(ServiceCmd::ClearSlot {
                        device: device.clone(),
                        slot,
                    });
                }
                Response::ok("all slots cleared")
            }
            Request::Remap { on } => self.ipc_reply(ServiceCmd::SetRemap(on)),
            Request::Devices => Response::Devices {
                devices: ipc_devices(&self.rt),
            },
            Request::AutoSwitch { enabled } => {
                self.set_auto_switch(enabled);
                Response::ok(format!("automatic profile switching {}", on_off(enabled)))
            }
            Request::Reload => {
                self.reload();
                Response::ok(format!("{} profiles", self.loose_profiles()))
            }
            Request::Shutdown => Response::ok("shutting down"),
        }
    }

    fn set_auto_switch(&mut self, enabled: bool) {
        self.state.auto_switch = enabled;
        if !enabled && self.auto_active.take().is_some() {
            self.apply_base();
        }
        self.save_state();
    }

    /// `None` if the slot exists, otherwise the error reply.
    fn bad_slot(slot: usize) -> Option<ServiceReply> {
        (slot >= SLOTS).then(|| ServiceReply::Err(format!("slot {} does not exist", slot + 1)))
    }

    /// The reply after a controller's own slots changed.
    fn device_reply(&mut self, device: &str, ok: String) -> ServiceReply {
        self.save_state();
        self.sync_devices();
        match self.device_errors.get(device) {
            Some(e) => ServiceReply::Err(e.clone()),
            None => ServiceReply::Ok(ok),
        }
    }

    /// Execute one command. When a change could not be saved, the reply says so.
    pub fn handle(&mut self, cmd: ServiceCmd) -> ServiceReply {
        self.unsaved = None;
        match (self.run(cmd), self.unsaved.take()) {
            (ServiceReply::Ok(m), Some(e)) => ServiceReply::Ok(unsaved_note(&m, &e)),
            (r, _) => r,
        }
    }

    fn run(&mut self, cmd: ServiceCmd) -> ServiceReply {
        match cmd {
            ServiceCmd::Ipc(req) => ServiceReply::Ipc(self.handle_ipc(req)),
            ServiceCmd::ApplyToSlot { device, slot, path } => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                // Validate before touching the slot.
                if let Err(e) = load(&path) {
                    return ServiceReply::Err(format!("{e:#}"));
                }
                self.state.remap_on = true;
                if let Some(device) = device {
                    let own = self.state.devices.entry(device.clone()).or_default();
                    own.slots[slot] = Some(path.clone());
                    own.active = slot;
                    // Re-apply even if the same file is active: it may have been edited.
                    self.rt.stop_device(&dp_platform::DeviceId(device.clone()));
                    return self.device_reply(&device, format!("applied to slot {}", slot + 1));
                }
                self.clear_overrides();
                self.update_state(|s| {
                    s.slots[slot] = Some(path.clone());
                    s.active = slot;
                });
                self.sync_devices();
                // Re-apply even if the same file is active: it may have been edited.
                match self.apply_path(&path) {
                    Ok(()) => ServiceReply::Ok(format!("applied to slot {}", slot + 1)),
                    Err(e) => ServiceReply::Err(format!("{e:#}")),
                }
            }
            ServiceCmd::SelectSlot { device, slot } => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                self.state.remap_on = true;
                if let Some(device) = self.has_own_slots(device.as_deref()) {
                    if let Some(own) = self.state.devices.get_mut(&device) {
                        own.active = slot;
                    }
                    return self.device_reply(&device, format!("slot {} selected", slot + 1));
                }
                self.clear_overrides();
                self.update_state(|s| s.active = slot);
                self.apply_base();
                self.sync_devices();
                match &self.last_error {
                    Some(e) => ServiceReply::Err(e.clone()),
                    None => ServiceReply::Ok(format!("slot {} selected", slot + 1)),
                }
            }
            ServiceCmd::ClearSlot { device, slot } => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                if let Some(device) = self.has_own_slots(device.as_deref()) {
                    if let Some(own) = self.state.devices.get_mut(&device) {
                        own.slots[slot] = None;
                    }
                    return self.device_reply(&device, format!("slot {} cleared", slot + 1));
                }
                self.update_state(|s| s.slots[slot] = None);
                if slot == self.state.active && self.auto_active.is_none() {
                    self.apply_base();
                }
                ServiceReply::Ok(format!("slot {} cleared", slot + 1))
            }
            ServiceCmd::OwnSlots { device, on } => {
                if on {
                    self.state.devices.entry(device.clone()).or_default();
                } else {
                    self.state.devices.remove(&device);
                }
                let msg = if on {
                    "the controller has slots of its own"
                } else {
                    "the controller uses the shared slots"
                };
                self.device_reply(&device, msg.into())
            }
            ServiceCmd::SetRemap(on) => {
                if !on {
                    self.clear_overrides();
                }
                self.update_state(|s| s.remap_on = on);
                self.apply_base();
                self.sync_devices();
                match (&self.last_error, on) {
                    (Some(e), true) => ServiceReply::Err(e.clone()),
                    _ => ServiceReply::Ok(format!("remap {}", on_off(on))),
                }
            }
            ServiceCmd::SetSuspended(on) => {
                if on && !self.gui_suspended {
                    self.gui_since = self.rt.now_ms();
                }
                self.gui_suspended = on;
                self.apply_suspend();
                ServiceReply::Ok(format!(
                    "remapping {}",
                    if on { "paused" } else { "resumed" }
                ))
            }
            ServiceCmd::SetPreferredDevice(id) => {
                self.rt.set_preferred_device(id.map(dp_platform::DeviceId));
                ServiceReply::Ok("device preference set".into())
            }
            ServiceCmd::SetAutoSwitch(on) => {
                self.set_auto_switch(on);
                ServiceReply::Ok(format!("auto-switch {}", on_off(on)))
            }
            ServiceCmd::SetBlocked { id, blocked } => {
                self.state.blocked.retain(|b| *b != id);
                if blocked {
                    self.state.blocked.push(id);
                }
                self.apply_device_policy();
                self.save_state();
                ServiceReply::Ok(
                    if blocked {
                        "device ignored"
                    } else {
                        "device used again"
                    }
                    .into(),
                )
            }
            ServiceCmd::SetGroups(groups) => match group_problem(&groups) {
                Some(e) => ServiceReply::Err(e),
                None => {
                    self.state.groups = groups;
                    self.apply_device_policy();
                    self.save_state();
                    ServiceReply::Ok("device groups set".into())
                }
            },
            ServiceCmd::SetPowerOffAfter(minutes) => {
                self.state.power_off_after_min = minutes;
                self.apply_device_policy();
                self.save_state();
                ServiceReply::Ok(match minutes {
                    0 => "controllers stay on".into(),
                    m => format!("controllers turn off after {m} min without input"),
                })
            }
            ServiceCmd::SetLayout(layout) => match layout_problem(&layout) {
                Some(e) => ServiceReply::Err(e),
                None => {
                    self.state.layouts.retain(|l| l.device != layout.device);
                    self.state.layouts.push(layout);
                    self.apply_device_policy();
                    self.save_state();
                    ServiceReply::Ok("layout saved".into())
                }
            },
            ServiceCmd::SetRuntimeOptions(o) => {
                self.update_state(|s| s.runtime = o);
                self.rt.set_prevent_sleep(o.prevent_sleep);
                self.sync_devices();
                ServiceReply::Ok("runtime options set".into())
            }
            ServiceCmd::SetHttp(settings) => {
                self.state.http = HttpSettings {
                    token: self.state.http.token.clone(),
                    ..settings
                };
                self.apply_http();
                self.save_state();
                match &self.http.error {
                    Some(e) => ServiceReply::Err(e.clone()),
                    None => ServiceReply::Ok("HTTP API set".into()),
                }
            }
            ServiceCmd::NewHttpToken => {
                self.state.http.token = crate::httpapi::new_token();
                self.apply_http();
                self.save_state();
                ServiceReply::Ok("new token".into())
            }
            ServiceCmd::SetDsu(settings) => {
                self.state.dsu = settings;
                self.apply_dsu();
                self.save_state();
                match &self.dsu.error {
                    Some(e) => ServiceReply::Err(e.clone()),
                    None if settings.enabled => ServiceReply::Ok("motion server on".into()),
                    None => ServiceReply::Ok("motion server off".into()),
                }
            }
            ServiceCmd::ForgetLayout(device) => {
                self.state.layouts.retain(|l| l.device != device);
                self.apply_device_policy();
                self.save_state();
                ServiceReply::Ok("layout removed".into())
            }
            ServiceCmd::PowerOff(id) => {
                if self.rt.power_off(&dp_platform::DeviceId(id)) {
                    ServiceReply::Ok("controller turned off".into())
                } else {
                    ServiceReply::Err(
                        "this controller cannot be turned off (only wireless ones can)".into(),
                    )
                }
            }
            ServiceCmd::SetLights { player, mic } => {
                self.update_state(|s| {
                    s.player_lights = player;
                    s.mic_led = mic;
                });
                ServiceReply::Ok("lights set".into())
            }
            ServiceCmd::RefreshDrivers => {
                self.check_drivers(self.rt.now_ms());
                ServiceReply::Ok("drivers checked".into())
            }
            ServiceCmd::RescanDevices => {
                self.rt.rescan_devices();
                ServiceReply::Ok("looking for controllers".into())
            }
            ServiceCmd::GetHotkeys => ServiceReply::Hotkeys(self.rt.hotkeys().clone()),
            ServiceCmd::SetHotkeys(h) => {
                let problems = h.problems();
                if !problems.is_empty() {
                    return ServiceReply::Err(problems.join("; "));
                }
                if let Some(path) = &self.opts.hotkeys_path
                    && let Err(e) = crate::hotkeys::save(path, &h)
                {
                    return ServiceReply::Err(format!("cannot save {}: {e}", path.display()));
                }
                self.rt.set_hotkeys(h);
                ServiceReply::Ok("hotkeys saved".into())
            }
            ServiceCmd::PauseHotkeys(on) => {
                self.rt.set_hotkeys_paused(on);
                ServiceReply::Ok(format!("hotkeys {}", if on { "paused" } else { "on" }))
            }
            ServiceCmd::TestTurn { counts } => {
                self.rt.test_turn(counts, TEST_TURN_DELAY_MS, TEST_TURN_MS);
                ServiceReply::Ok(format!(
                    "turning {counts} counts in {} s",
                    TEST_TURN_DELAY_MS / 1000
                ))
            }
            ServiceCmd::CalibrateGyro { device } => self.with_gyro(device, |rt, id| {
                let now = rt.now_ms();
                rt.calibration_mut()
                    .start_manual(id, now, GYRO_CALIBRATION_MS);
                format!("calibrating the gyro of {id}")
            }),
            ServiceCmd::SetGyroAuto { device, on } => self.with_gyro(device, |rt, id| {
                rt.calibration_mut().set_auto(id, on);
                format!("automatic gyro calibration {}", on_off(on))
            }),
            ServiceCmd::ResetGyro { device } => self.with_gyro(device, |rt, id| {
                rt.calibration_mut().reset(id);
                format!("gyro calibration of {id} reset")
            }),
        }
    }

    /// Carry out the hotkeys the runtime reported (slots and the remap switch). A slot
    /// hotkey selects a slot of the controller it was pressed on if that has slots of its
    /// own, else a shared slot.
    fn run_hotkeys(&mut self) {
        for (id, action) in self.rt.take_hotkey_actions() {
            match action {
                HotkeyAction::Slot(n) => {
                    let device = self.has_own_slots(Some(&id.0));
                    let slots = match &device {
                        Some(d) => &self.state.devices[d].slots,
                        None => &self.state.slots,
                    };
                    if slots.get(n as usize).is_none_or(Option::is_none) {
                        tracing::info!("slot {} is empty", n + 1);
                        continue;
                    }
                    let cmd = ServiceCmd::SelectSlot {
                        device,
                        slot: n as usize,
                    };
                    if let ServiceReply::Err(e) = self.handle(cmd) {
                        tracing::warn!("slot hotkey: {e}");
                    }
                }
                HotkeyAction::ToggleRemap => {
                    let on = !self.state.remap_on;
                    if let ServiceReply::Err(e) = self.handle(ServiceCmd::SetRemap(on)) {
                        tracing::warn!("remap hotkey: {e}");
                    }
                }
                // The runtime handles these itself.
                HotkeyAction::StopMacros
                | HotkeyAction::EmergencyStop
                | HotkeyAction::PadOverlay
                | HotkeyAction::MappingsOverlay => {}
            }
        }
    }

    /// Run a gyro calibration change for `device`, or the controller in use.
    fn with_gyro(
        &mut self,
        device: Option<String>,
        f: impl FnOnce(&mut Runtime, &str) -> String,
    ) -> ServiceReply {
        let Some(id) = device.or_else(|| self.rt.primary_device().map(|d| d.0.clone())) else {
            return ServiceReply::Err("no controller to calibrate".into());
        };
        ServiceReply::Ok(f(&mut self.rt, &id))
    }

    /// Persist the gyro calibration: explicit changes at once, automatic ones once a minute.
    fn save_calibration(&mut self) {
        let Some(path) = &self.opts.calibration_path else {
            return;
        };
        let now = self.rt.now_ms();
        let periodic = self.calibration_save.elapsed(now);
        if self.rt.calibration().needs_save(periodic) {
            if let Err(e) = self.rt.calibration_mut().save(path) {
                tracing::warn!("cannot save {}: {e}", path.display());
            }
            self.calibration_save.done(now);
        }
    }

    /// Start, stop or move the HTTP API to match the settings.
    fn apply_http(&mut self) {
        if self.state.http.enabled && self.state.http.token.is_empty() {
            self.update_state(|s| s.http.token = crate::httpapi::new_token());
        }
        let settings = &self.state.http;
        self.http
            .restart(settings.enabled, "the HTTP API", settings.port, || {
                HttpApi::start(settings)
            });
    }

    /// Answer what the HTTP API's clients asked for since the last tick.
    fn serve_http(&mut self) {
        let Some(api) = &self.http.running else {
            return;
        };
        let jobs: Vec<Job> = api.jobs().try_iter().collect();
        for job in jobs {
            match job {
                Job::Ipc(req, reply) => {
                    let answer = self.handle_ipc(req);
                    let _ = reply.send(answer);
                }
                Job::Live(reply) => {
                    let _ = reply.send(self.live());
                }
            }
        }
    }

    /// Start, stop or move the DSU server to match the settings.
    fn apply_dsu(&mut self) {
        let want = self.state.dsu;
        self.dsu
            .restart(want.enabled, "the motion server", want.port, || {
                DsuServer::start(want)
            });
    }

    fn apply_device_policy(&mut self) {
        self.rt.set_device_policy(DevicePolicy {
            blocked: self.state.blocked.clone(),
            groups: self.state.groups.clone(),
        });
        self.rt.set_layouts(self.state.layouts.clone());
        let minutes = self.state.power_off_after_min;
        self.rt.set_power_off_after(
            (minutes > 0).then(|| std::time::Duration::from_secs(u64::from(minutes) * 60)),
        );
    }

    /// Player lights and microphone LED of every controller, from the slots and remapping.
    fn show_lights(&mut self) {
        let state = &self.state;
        self.rt.set_indicators(|id| {
            let slot = state.devices.get(&id.0).map_or(state.active, |d| d.active) as u8 + 1;
            Leds {
                player: match state.player_lights {
                    PlayerLights::Always => Some(slot),
                    PlayerLights::WhileRemapping => Some(if state.remap_on { slot } else { 0 }),
                    PlayerLights::Off => None,
                },
                mic: state.mic_led.then_some(state.remap_on),
            }
        });
    }

    /// One loop iteration: engine tick, remap-off handling, auto-switch, driver probing.
    pub fn tick(&mut self) {
        self.rt.tick();
        if let Some(dsu) = &mut self.dsu.running {
            dsu.publish(self.rt.devices());
        }
        self.serve_http();
        self.run_hotkeys();
        self.show_lights();
        self.save_calibration();
        if self.rt.take_remap_off_request() {
            self.turn_remap_off();
        }
        // Controllers with slots of their own start or stop as they come and go.
        if self.rt.devices().len() != self.connected.len()
            || self
                .rt
                .devices()
                .iter()
                .zip(&self.connected)
                .any(|(s, id)| s.info.id.0 != *id)
        {
            self.sync_devices();
        }
        let now = self.rt.now_ms();
        self.watch_desktop(now);
        if self.poll_foreground(now) {
            self.watch_focus();
            self.apply_suspend();
        }
        self.auto_switch_tick();
        if self.driver_poll.elapsed(now) {
            self.check_drivers(now);
        }
    }

    fn check_drivers(&mut self, now: u64) {
        let drivers = self.rt.drivers();
        if !self.driver_poll.ever() || drivers != self.drivers {
            tracing::info!(
                "drivers: virtual pads {}, device hiding {}",
                drivers.virtual_pads,
                drivers.device_hiding
            );
        }
        let problems = self.rt.hiding_problems();
        for p in problems
            .iter()
            .filter(|p| !self.hiding_problems.contains(p))
        {
            tracing::warn!("{p}");
        }
        self.drivers = drivers;
        self.hiding_problems = problems;
        self.driver_poll.done(now);
    }

    /// Ask which application is in front, at most every poll interval; `true` if it asked.
    fn poll_foreground(&mut self, now: u64) -> bool {
        if !self.fg_poll.due(now) {
            return false;
        }
        let fg = &mut self.rt.platform_mut().foreground;
        self.fg_now = fg.foreground_process();
        self.fg_pid = fg.foreground_pid();
        true
    }

    /// Pause while the user's desktop does not take input (locked, Ctrl+Alt+Del, an
    /// elevation prompt): nothing stays pressed meanwhile.
    fn watch_desktop(&mut self, now: u64) {
        if !self.desktop_poll.due(now) {
            return;
        }
        let locked = !self.rt.platform_mut().system.desktop_active();
        if locked != self.locked {
            tracing::info!(locked, "the user's desktop changed");
            self.locked = locked;
            self.apply_suspend();
        }
    }

    /// Executables of the game the active config belongs to (its game folder, or its own
    /// `processes`); empty for a config of no particular game.
    fn game_processes(&self) -> Vec<String> {
        let Some(path) = self.rt.profile_path() else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .library
            .games
            .iter()
            .filter(|g| g.configs.iter().any(|c| &c.path == path))
            .flat_map(|g| g.meta.processes.iter().cloned())
            .collect();
        out.extend(
            self.rt
                .profile()
                .into_iter()
                .flat_map(|p| p.processes.iter().cloned()),
        );
        out
    }

    /// With the option on, pause while the active config's game is not in front.
    fn watch_focus(&mut self) {
        let paused = self.state.runtime.pause_unfocused
            && self.state.remap_on
            && self.fg_now.as_deref().is_some_and(|fg| {
                let games = self.game_processes();
                !games.is_empty() && !games.iter().any(|g| g.eq_ignore_ascii_case(fg))
            });
        if paused != self.focus_paused {
            tracing::info!(paused, "the config's game lost or got the focus");
            self.focus_paused = paused;
            self.apply_suspend();
        }
    }

    /// The editor's pause, which holds only while a window of this process is in front
    /// (after a grace period for the foreground check to catch up): a focus change the
    /// window missed must not leave the controller dead in the game.
    fn editing(&mut self) -> bool {
        let stale = self.gui_suspended
            && self.rt.now_ms() >= self.gui_since + self.opts.gui_focus_grace_ms
            && self.fg_pid.is_some_and(|pid| pid != std::process::id());
        if stale != self.gui_stale {
            self.gui_stale = stale;
            if stale {
                tracing::warn!(
                    foreground = self.fg_now.as_deref().unwrap_or("?"),
                    "the Doppelpad window still reports focus while another program is in front; not pausing"
                );
            }
        }
        self.gui_suspended && !stale
    }

    fn apply_suspend(&mut self) {
        let reasons = [
            (self.editing(), "editing"),
            (self.locked, "desktop locked"),
            (self.focus_paused, "game not in front"),
        ];
        let on = reasons.iter().any(|(r, _)| *r);
        if on != self.rt.is_suspended() {
            if on {
                let why: Vec<&str> = reasons
                    .iter()
                    .filter(|(r, _)| *r)
                    .map(|(_, w)| *w)
                    .collect();
                tracing::info!(reason = why.join(", "), "remapping paused");
            } else {
                tracing::info!("remapping resumed");
            }
        }
        self.rt.set_suspended(on);
    }

    fn auto_switch_tick(&mut self) {
        if !self.state.auto_switch || !self.state.remap_on {
            // Look again once switching is back on, even if the same application kept
            // the focus meanwhile.
            self.foreground = None;
            return;
        }
        if self.fg_now == self.foreground {
            return;
        }
        self.foreground = self.fg_now.clone();
        let target = self
            .foreground
            .as_deref()
            .and_then(|exe| self.auto_profile_for(exe));
        match target {
            Some(path) if self.rt.profile_path() != Some(&path) => {
                tracing::info!(app = ?self.foreground, profile = %path.display(), "auto-switch");
                match self.apply_path(&path) {
                    Ok(()) => self.auto_active = Some(path),
                    Err(e) => tracing::warn!("auto-switch failed: {e:#}"),
                }
            }
            None if self.auto_active.take().is_some() => self.apply_base(),
            _ => {}
        }
    }

    /// Snapshot for user interfaces.
    pub fn live(&self) -> LiveState {
        let now = self.rt.now_ms();
        LiveState {
            devices: self
                .rt
                .devices()
                .iter()
                .map(|s| LiveDevice {
                    id: s.info.id.to_string(),
                    name: s.info.name.clone(),
                    family: s.info.family,
                    backend: s.info.backend.clone(),
                    vendor_id: s.info.vendor_id,
                    product_id: s.info.product_id,
                    state: s.state,
                    gyro_calibration: s
                        .state
                        .motion
                        .map(|_| self.rt.calibration().status(&s.info.id.0, now)),
                    battery: s.battery,
                    raw: s.raw.as_deref().cloned(),
                })
                .collect(),
            primary: self.rt.primary_device().map(|d| d.to_string()),
            profile: self.rt.profile().map(|p| p.name.clone()),
            profile_path: self.rt.profile_path().map(|p| p.display().to_string()),
            layer: self.rt.layer(),
            layer_name: self.rt.profile().and_then(|p| {
                p.layer(self.rt.layer())
                    .filter(|l| !l.name.is_empty())
                    .map(|l| l.name.clone())
            }),
            slots: self
                .state
                .slots
                .iter()
                .map(|s| s.as_ref().map(|p| p.display().to_string()))
                .collect(),
            active_slot: self.state.active,
            remap_on: self.state.remap_on,
            player_lights: self.state.player_lights,
            mic_led: self.state.mic_led,
            seen_devices: self.rt.seen_devices(),
            blocked: self.state.blocked.clone(),
            groups: self.state.groups.clone(),
            power_off_after_min: self.state.power_off_after_min,
            runtime: self.state.runtime,
            locked: self.locked,
            focus_paused: self.focus_paused,
            layouts: self
                .state
                .layouts
                .iter()
                .map(|l| l.device.clone())
                .collect(),
            http: crate::live::LiveHttp {
                settings: self.state.http.clone(),
                address: self.http.running.as_ref().map(|h| h.addr.to_string()),
                error: self.http.error.clone(),
            },
            dsu: crate::live::LiveDsu {
                settings: self.state.dsu,
                address: self.dsu.running.as_ref().map(|d| d.addr.to_string()),
                clients: self.dsu.running.as_ref().map_or(0, DsuServer::clients),
                error: self.dsu.error.clone(),
            },
            auto_switch: self.state.auto_switch,
            auto_active: self.auto_active.is_some(),
            suspended: self.rt.is_suspended(),
            foreground: self.fg_now.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| k.name().to_owned()),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
            drivers: self.drivers.clone(),
            device_slots: self
                .state
                .devices
                .iter()
                .map(|(id, d)| crate::live::LiveDeviceSlots {
                    id: id.clone(),
                    slots: d
                        .slots
                        .iter()
                        .map(|s| s.as_ref().map(|p| p.display().to_string()))
                        .collect(),
                    active_slot: d.active,
                })
                .collect(),
            sessions: self.rt.sessions(),
            crosshair: self.rt.crosshair().cloned(),
            radial: self
                .rt
                .radial()
                .map(|(menu, pointed)| crate::live::LiveRadial {
                    menu: menu.clone(),
                    pointed,
                }),
            pad_overlay: self.rt.overlays().0,
            mappings_overlay: self.rt.overlays().1,
            has_radial: self
                .rt
                .profile()
                .is_some_and(|p| !p.radial_menus.is_empty()),
        }
    }

    /// Release everything and unhide devices.
    pub fn shutdown(&mut self) {
        self.rt.stop();
    }
}

/// A reply to a change that took effect but could not be saved.
fn unsaved_note(reply: &str, error: &str) -> String {
    format!("{reply}; {error}, so the change is lost when Doppelpad restarts")
}

/// Why a generic pad layout cannot be stored, if it cannot.
fn layout_problem(l: &GenericLayout) -> Option<String> {
    let hex4 = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit());
    let key_ok = l
        .device
        .split_once(':')
        .is_some_and(|(v, p)| hex4(v) && hex4(p));
    if !key_ok {
        return Some(format!(
            "`{}` is not a controller model (vvvv:pppp)",
            l.device
        ));
    }
    if l.buttons.is_empty() && l.triggers.is_empty() && l.sticks.is_empty() {
        return Some("the layout maps nothing".into());
    }
    None
}

/// What is wrong with a set of device groups, if anything.
fn group_problem(groups: &[DeviceGroup]) -> Option<String> {
    for (i, g) in groups.iter().enumerate() {
        if g.name.trim().is_empty() {
            return Some("a device group needs a name".into());
        }
        if groups[..i].iter().any(|o| o.name == g.name) {
            return Some(format!("there are two device groups named {}", g.name));
        }
        if g.members.len() < 2 {
            return Some(format!(
                "device group {} needs at least two controllers",
                g.name
            ));
        }
        if let Some(m) = g
            .members
            .iter()
            .find(|m| groups[..i].iter().any(|o| o.members.contains(m)))
        {
            return Some(format!("{m} is in two device groups"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use dp_platform::{ForegroundWatcher, NullPlatform};

    use super::*;
    #[test]
    fn device_groups_need_names_and_distinct_members() {
        let g = |name: &str, members: &[&str]| DeviceGroup {
            name: name.into(),
            members: members.iter().map(|m| m.to_string()).collect(),
        };
        assert_eq!(group_problem(&[g("Pair", &["a", "b"])]), None);
        assert!(group_problem(&[g("", &["a", "b"])]).is_some());
        assert!(group_problem(&[g("Solo", &["a"])]).is_some());
        assert!(group_problem(&[g("P", &["a", "b"]), g("Q", &["b", "c"])]).is_some());
    }

    struct FakeForeground(Arc<Mutex<Option<String>>>);

    impl ForegroundWatcher for FakeForeground {
        fn foreground_process(&mut self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
    }

    fn write_profile(dir: &Path, file: &str, name: &str, processes: &[&str]) -> PathBuf {
        let path = dir.join(file);
        let procs: Vec<String> = processes.iter().map(|p| format!("\"{p}\"")).collect();
        std::fs::write(
            &path,
            format!(
                "name = \"{name}\"\nprocesses = [{}]\n[output]\nvirtual_pad = \"none\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{{ key = \"Space\" }}]\n",
                procs.join(", ")
            ),
        )
        .unwrap();
        path
    }

    fn service(dir: &Path, state: Option<PathBuf>) -> (Service, Arc<Mutex<Option<String>>>) {
        let fg = Arc::new(Mutex::new(None));
        let mut platform = NullPlatform::platform();
        platform.foreground = Box::new(FakeForeground(fg.clone()));
        let mut opts = ServiceOptions::new(dir.join("profiles"));
        opts.state_path = state;
        opts.foreground_poll_ms = 0;
        (
            Service::new(Runtime::with_platform(platform), opts).unwrap(),
            fg,
        )
    }

    fn active_name(s: &Service) -> Option<String> {
        s.runtime().profile().map(|p| p.name.clone())
    }

    struct Desktop(Arc<std::sync::atomic::AtomicBool>);

    impl dp_platform::SystemControl for Desktop {
        fn power(&mut self, _: dp_platform::PowerAction) -> dp_platform::Result<()> {
            Ok(())
        }
        fn screenshot(&mut self) -> dp_platform::Result<PathBuf> {
            Ok(PathBuf::new())
        }
        fn close_foreground_window(&mut self) -> dp_platform::Result<()> {
            Ok(())
        }
        fn desktop_active(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[test]
    fn answers_http_requests_on_its_tick() {
        use std::io::{Read, Write};
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _) = service(dir.path(), None);
        let reply = s.handle(ServiceCmd::SetHttp(HttpSettings {
            enabled: true,
            port: 0,
            ..Default::default()
        }));
        assert!(reply.into_result().is_ok());
        let live = s.live().http;
        assert_eq!(
            live.settings.token.len(),
            32,
            "a token is made on first use"
        );
        let addr: std::net::SocketAddr = live.address.unwrap().parse().unwrap();
        let client = std::thread::spawn(move || {
            let mut c = std::net::TcpStream::connect(addr).unwrap();
            write!(
                c,
                "GET /api/status HTTP/1.1
Host: 127.0.0.1:{}

",
                addr.port()
            )
            .unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).unwrap();
            out
        });
        while !client.is_finished() {
            s.tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let out = client.join().unwrap();
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(out.contains(r#""type":"status""#), "{out}");
    }

    #[test]
    fn pauses_while_locked_or_the_game_is_not_in_front() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let game = write_profile(dir.path(), "game.toml", "Game", &["game.exe"]);
        let fg = Arc::new(Mutex::new(Some("game.exe".to_string())));
        let active = Arc::new(AtomicBool::new(true));
        let mut platform = NullPlatform::platform();
        platform.foreground = Box::new(FakeForeground(fg.clone()));
        platform.system = Box::new(Desktop(active.clone()));
        let mut opts = ServiceOptions::new(dir.path().join("profiles"));
        opts.foreground_poll_ms = 0;
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: game,
        });
        s.handle(ServiceCmd::SetRuntimeOptions(RuntimeOptions {
            pause_unfocused: true,
            ..Default::default()
        }));
        s.tick();
        assert!(!s.live().suspended);
        *fg.lock().unwrap() = Some("notepad.exe".into());
        s.tick();
        assert!(
            s.live().focus_paused && s.live().suspended,
            "the game is not in front"
        );
        *fg.lock().unwrap() = Some("GAME.EXE".into());
        s.tick();
        assert!(!s.live().suspended);

        active.store(false, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(DESKTOP_POLL_MS + 20));
        s.tick();
        assert!(s.live().locked && s.live().suspended, "locked");
        active.store(true, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(DESKTOP_POLL_MS + 20));
        s.tick();
        assert!(!s.live().suspended);
    }

    #[test]
    fn slots_apply_select_clear_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_profile(dir.path(), "a.toml", "A", &[]);
        let b = write_profile(dir.path(), "b.toml", "B", &[]);
        let state = dir.path().join("state.toml");
        let (mut s, _) = service(dir.path(), Some(state.clone()));
        assert_eq!(active_name(&s), None);

        assert!(
            s.handle(ServiceCmd::ApplyToSlot {
                device: None,
                slot: 0,
                path: a
            })
            .into_result()
            .is_ok()
        );
        assert!(
            s.handle(ServiceCmd::ApplyToSlot {
                device: None,
                slot: 2,
                path: b.clone()
            })
            .into_result()
            .is_ok()
        );
        assert_eq!(active_name(&s).as_deref(), Some("B"));

        s.handle(ServiceCmd::SelectSlot {
            device: None,
            slot: 0,
        });
        assert_eq!(active_name(&s).as_deref(), Some("A"));

        s.handle(ServiceCmd::SetRemap(false));
        assert_eq!(active_name(&s), None, "remap off unloads but keeps slots");
        assert_eq!(s.slots().slots[2], Some(b.clone()));
        s.handle(ServiceCmd::SetRemap(true));
        assert_eq!(active_name(&s).as_deref(), Some("A"));

        s.handle(ServiceCmd::SetRemap(false));
        s.handle(ServiceCmd::SelectSlot {
            device: None,
            slot: 2,
        });
        assert_eq!(
            active_name(&s).as_deref(),
            Some("B"),
            "selecting a slot turns remapping on"
        );
        assert!(s.live().remap_on);
        s.handle(ServiceCmd::SelectSlot {
            device: None,
            slot: 0,
        });

        s.handle(ServiceCmd::ClearSlot {
            device: None,
            slot: 0,
        });
        assert_eq!(active_name(&s), None, "clearing the active slot unloads it");
        assert!(
            s.handle(ServiceCmd::SelectSlot {
                device: None,
                slot: 9
            })
            .into_result()
            .is_err()
        );

        // A new service picks the persisted state up again.
        drop(s);
        let (s2, _) = service(dir.path(), Some(state));
        assert_eq!(s2.slots().slots[2], Some(b));
        assert_eq!(s2.slots().active, 0);
    }

    #[test]
    fn apply_to_slot_rejects_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.toml");
        std::fs::write(&bad, "[[mappings]]\ntrigger = \"NoSuchButton\"\n").unwrap();
        let (mut s, _) = service(dir.path(), None);
        assert!(
            s.handle(ServiceCmd::ApplyToSlot {
                device: None,
                slot: 1,
                path: bad
            })
            .into_result()
            .is_err()
        );
        assert_eq!(s.slots().slots[1], None, "slot untouched");
    }

    #[test]
    fn suspend_is_reported_and_reversible() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_profile(dir.path(), "a.toml", "A", &[]);
        let (mut s, _) = service(dir.path(), None);
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: a,
        });
        s.handle(ServiceCmd::SetSuspended(true));
        s.tick();
        assert!(s.live().suspended);
        assert_eq!(
            active_name(&s).as_deref(),
            Some("A"),
            "profile stays loaded while paused"
        );
        s.handle(ServiceCmd::SetSuspended(false));
        assert!(!s.live().suspended);
    }

    /// Reports only which process is in front.
    struct PidForeground(Arc<Mutex<Option<u32>>>);

    impl ForegroundWatcher for PidForeground {
        fn foreground_process(&mut self) -> Option<String> {
            None
        }
        fn foreground_pid(&mut self) -> Option<u32> {
            *self.0.lock().unwrap()
        }
    }

    #[test]
    fn editor_pause_holds_only_while_doppelpad_is_in_front() {
        let dir = tempfile::tempdir().unwrap();
        let pid = Arc::new(Mutex::new(Some(std::process::id())));
        let mut platform = NullPlatform::platform();
        platform.foreground = Box::new(PidForeground(pid.clone()));
        let mut opts = ServiceOptions::new(dir.path().join("profiles"));
        opts.foreground_poll_ms = 0;
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();

        s.handle(ServiceCmd::SetSuspended(true));
        s.tick();
        assert!(s.live().suspended, "the editor in front pauses");

        *pid.lock().unwrap() = Some(std::process::id() + 1);
        s.tick();
        assert!(
            s.live().suspended,
            "trusted while the foreground check catches up"
        );

        s.opts.gui_focus_grace_ms = 0;
        s.tick();
        assert!(
            !s.live().suspended,
            "a missed focus change does not keep the pause"
        );

        *pid.lock().unwrap() = Some(std::process::id());
        s.tick();
        assert!(s.live().suspended, "back in the editor");

        s.handle(ServiceCmd::SetSuspended(false));
        assert!(!s.live().suspended);
    }

    #[test]
    fn auto_switch_applies_and_restores() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        write_profile(&profiles, "shooter.toml", "Game", &["game.exe"]);
        let base = write_profile(dir.path(), "base.toml", "Base", &[]);
        let (mut s, fg) = service(dir.path(), None);
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: base,
        });

        *fg.lock().unwrap() = Some("Game.EXE".into());
        s.tick();
        assert_eq!(active_name(&s).as_deref(), Some("Game"));
        assert_eq!(s.live().foreground.as_deref(), Some("Game.EXE"));

        *fg.lock().unwrap() = Some("explorer.exe".into());
        s.tick();
        assert_eq!(
            active_name(&s).as_deref(),
            Some("Base"),
            "slot profile restored"
        );

        s.handle(ServiceCmd::SetAutoSwitch(false));
        *fg.lock().unwrap() = Some("game.exe".into());
        s.tick();
        assert_eq!(active_name(&s).as_deref(), Some("Base"), "auto-switch off");
    }

    #[test]
    fn foreground_is_reported_without_auto_switch() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, fg) = service(dir.path(), None);
        s.handle(ServiceCmd::SetAutoSwitch(false));
        s.handle(ServiceCmd::SetRemap(false));
        *fg.lock().unwrap() = Some("notepad.exe".into());
        s.tick();
        assert_eq!(s.live().foreground.as_deref(), Some("notepad.exe"));
        assert_eq!(s.status().foreground.as_deref(), Some("notepad.exe"));
    }

    #[test]
    fn auto_switch_waits_while_remapping_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        write_profile(&profiles, "shooter.toml", "Game", &["game.exe"]);
        let (mut s, fg) = service(dir.path(), None);
        s.handle(ServiceCmd::SetRemap(false));
        *fg.lock().unwrap() = Some("game.exe".into());
        s.tick();
        assert_eq!(active_name(&s), None, "remapping is off");

        s.handle(ServiceCmd::SetRemap(true));
        s.tick();
        assert_eq!(
            active_name(&s).as_deref(),
            Some("Game"),
            "the game in the foreground takes over once remapping is on again"
        );
    }

    #[test]
    fn auto_switch_uses_game_folders() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        let game = dp_config::library::create_game(&profiles, "Racer").unwrap();
        let meta = dp_config::library::GameMeta {
            processes: vec!["racer.exe".into()],
            ..game.meta.clone()
        };
        dp_config::library::update_game_meta(&game.dir, &meta).unwrap();
        let (mut s, fg) = service(dir.path(), None);
        *fg.lock().unwrap() = Some("racer.exe".into());
        s.tick();
        assert_eq!(active_name(&s).as_deref(), Some("Default"));
    }

    #[test]
    fn gyro_calibration_commands_reach_the_controller_and_disk() {
        use dp_platform::PadFamily;

        let dir = tempfile::tempdir().unwrap();
        let (platform, world) = crate::runtime::testing::platform();
        let mut opts = ServiceOptions::new(dir.path().join("profiles"));
        let path = dir.path().join("calibration.toml");
        opts.calibration_path = Some(path.clone());
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();
        assert!(matches!(
            s.handle(ServiceCmd::CalibrateGyro { device: None }),
            ServiceReply::Err(_)
        ));
        world
            .lock()
            .unwrap()
            .pads
            .push(crate::runtime::testing::pad("pad", PadFamily::DualSense));
        s.tick();
        assert!(matches!(
            s.handle(ServiceCmd::SetGyroAuto {
                device: None,
                on: false
            }),
            ServiceReply::Ok(_)
        ));
        s.tick();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.contains("[devices.pad]") && saved.contains("auto = false"),
            "{saved}"
        );
    }

    #[test]
    fn hotkeys_select_slots_and_switch_remapping() {
        use dp_core::pad::PadButton::{self, *};
        use dp_platform::PadFamily;

        let dir = tempfile::tempdir().unwrap();
        let a = write_profile(dir.path(), "a.toml", "A", &[]);
        let b = write_profile(dir.path(), "b.toml", "B", &[]);
        let (platform, world) = crate::runtime::testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(crate::runtime::testing::pad("pad", PadFamily::Xbox));
        let mut opts = ServiceOptions::new(dir.path().join("profiles"));
        opts.hotkeys_path = Some(dir.path().join("hotkeys.toml"));
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: a,
        });
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 1,
            path: b,
        });
        assert_eq!(active_name(&s).as_deref(), Some("B"));
        let hk = Hotkeys {
            hold_ms: 50,
            remap: vec![Guide],
            ..Default::default()
        };
        assert!(
            s.handle(ServiceCmd::SetHotkeys(hk.clone()))
                .into_result()
                .is_ok()
        );
        assert_eq!(s.handle(ServiceCmd::GetHotkeys), ServiceReply::Hotkeys(hk));

        let hold = |s: &mut Service, buttons: &[PadButton]| {
            world.lock().unwrap().pads[0].state.buttons = PadButton::mask(buttons);
            s.tick();
            std::thread::sleep(std::time::Duration::from_millis(70));
            s.tick();
            world.lock().unwrap().pads[0].state.buttons = 0;
            s.tick();
        };
        hold(&mut s, &[LB, RB, X]);
        assert_eq!(active_name(&s).as_deref(), Some("A"), "slot 1 hotkey");
        hold(&mut s, &[LB, RB, B]);
        assert_eq!(active_name(&s).as_deref(), Some("A"), "slot 3 is empty");
        hold(&mut s, &[Guide]);
        assert!(!s.live().remap_on);
        assert_eq!(active_name(&s), None, "remap hotkey turns remapping off");
        hold(&mut s, &[Guide]);
        assert_eq!(active_name(&s).as_deref(), Some("A"), "and on again");
        hold(&mut s, &[Back, Start, LB, RB]);
        assert!(!s.live().remap_on, "emergency stop");

        s.handle(ServiceCmd::PauseHotkeys(true));
        hold(&mut s, &[Guide]);
        assert!(!s.live().remap_on, "paused while recording");
        assert!(
            s.handle(ServiceCmd::SetHotkeys(Hotkeys {
                hold_ms: 5,
                ..Default::default()
            }))
            .into_result()
            .is_err()
        );
        let saved = crate::hotkeys::load(&dir.path().join("hotkeys.toml"));
        assert_eq!(saved.remap, vec![Guide], "settings persisted");
    }

    #[test]
    fn ipc_slots_and_remap() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_profile(dir.path(), "a.toml", "A", &[]);
        let b = write_profile(dir.path(), "b.toml", "B", &[]);
        let (mut s, _) = service(dir.path(), None);
        let ok = |r: Response| matches!(r, Response::Ok { .. });
        assert!(ok(s.handle_ipc(Request::ApplyToSlot {
            path: a.display().to_string(),
            slot: 1,
            device: None
        })));
        assert!(ok(s.handle_ipc(Request::ApplyToSlot {
            path: b.display().to_string(),
            slot: 3,
            device: None
        })));
        assert_eq!(active_name(&s).as_deref(), Some("B"));
        assert!(ok(s.handle_ipc(Request::SelectSlot {
            slot: 1,
            device: None
        })));
        assert_eq!(active_name(&s).as_deref(), Some("A"));
        let Response::Status(st) = s.handle_ipc(Request::Status) else {
            panic!()
        };
        assert_eq!((st.active_slot, st.remap_on), (1, true));
        assert_eq!(
            st.slots[2].as_deref(),
            Some(b.display().to_string().as_str())
        );

        assert!(ok(s.handle_ipc(Request::Remap { on: false })));
        assert_eq!(active_name(&s), None);
        assert!(ok(s.handle_ipc(Request::Remap { on: true })));
        assert_eq!(active_name(&s).as_deref(), Some("A"));
        assert!(
            !ok(s.handle_ipc(Request::SelectSlot {
                slot: 0,
                device: None
            })),
            "slots are 1–4"
        );
        assert!(!ok(s.handle_ipc(Request::SelectSlot {
            slot: 5,
            device: None
        })));
        assert!(ok(s.handle_ipc(Request::ClearSlot {
            slot: Some(3),
            device: None
        })));
        assert_eq!(s.slots().slots[2], None);
        assert!(ok(s.handle_ipc(Request::ClearSlot {
            slot: None,
            device: None
        })));
        assert!(s.slots().slots.iter().all(Option::is_none));
        assert_eq!(active_name(&s), None);
    }

    #[test]
    fn two_controllers_with_their_own_profiles() {
        use dp_core::keys::Key;
        use dp_core::pad::PadButton;
        use dp_platform::PadFamily;

        let dir = tempfile::tempdir().unwrap();
        let profile = |name: &str, key: &str| {
            let path = dir.path().join(format!("{name}.toml"));
            std::fs::write(
                &path,
                format!("name = \"{name}\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{{ key = \"{key}\" }}]\n"),
            )
            .unwrap();
            path
        };
        let one = profile("One", "Q");
        let two = profile("Two", "E");
        let (platform, world) = crate::runtime::testing::platform();
        {
            let mut w = world.lock().unwrap();
            w.pads
                .push(crate::runtime::testing::pad("p1", PadFamily::Xbox));
            w.pads
                .push(crate::runtime::testing::pad("p2", PadFamily::DualSense));
        }
        let opts = ServiceOptions::new(dir.path().join("profiles"));
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();
        s.tick();
        let ok = |r: ServiceReply| r.into_result().unwrap();
        ok(s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: one,
        }));
        ok(s.handle(ServiceCmd::ApplyToSlot {
            device: Some("p2".into()),
            slot: 1,
            path: two,
        }));
        let sessions = s.runtime().sessions();
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            (sessions[0].owner.as_deref(), sessions[0].device.as_deref()),
            (None, Some("p1")),
            "the shared slots run on the controller without its own"
        );
        assert_eq!(
            (sessions[1].owner.as_deref(), sessions[1].device.as_deref()),
            (Some("p2"), Some("p2"))
        );
        assert_eq!(
            world.lock().unwrap().virtual_pads.len(),
            2,
            "one virtual pad each"
        );
        assert_eq!(s.live().device_slots[0].active_slot, 1);
        s.tick(); // the engines start (inputs held at start are no presses)

        let press = |s: &mut Service, pad: usize| {
            world.lock().unwrap().pads[pad]
                .state
                .set(PadButton::A, true);
            s.tick();
            world.lock().unwrap().pads[pad]
                .state
                .set(PadButton::A, false);
            s.tick();
        };
        press(&mut s, 1);
        press(&mut s, 0);
        let downs: Vec<Key> = world
            .lock()
            .unwrap()
            .keys
            .iter()
            .filter(|k| k.1)
            .map(|k| k.0)
            .collect();
        assert_eq!(
            downs,
            [Key::E, Key::Q],
            "each controller drives its own profile"
        );

        // Its session ends when the controller goes away, and comes back with it.
        let p2 = world.lock().unwrap().pads.remove(1);
        s.tick();
        assert_eq!(s.runtime().sessions().len(), 1);
        world.lock().unwrap().pads.push(p2);
        s.tick();
        assert_eq!(s.runtime().sessions().len(), 2);

        ok(s.handle(ServiceCmd::OwnSlots {
            device: "p2".into(),
            on: false,
        }));
        assert_eq!(s.runtime().sessions().len(), 1);
        assert!(s.live().device_slots.is_empty());
        ok(s.handle(ServiceCmd::SetRemap(false)));
        assert!(s.runtime().sessions().is_empty());
    }

    #[test]
    fn ipc_apply_and_stop() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_profile(dir.path(), "a.toml", "A", &[]);
        let (mut s, _) = service(dir.path(), None);
        let r = s.handle_ipc(Request::Apply {
            path: a.display().to_string(),
        });
        assert!(matches!(r, Response::Ok { .. }), "{r:?}");
        assert!(
            matches!(s.handle_ipc(Request::Status), Response::Status(st) if st.profile.as_deref() == Some("A"))
        );
        s.handle_ipc(Request::Stop);
        assert_eq!(active_name(&s), None);
        assert!(!s.live().remap_on);
    }

    #[test]
    fn virtual_pad_kind_is_spelled_like_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pad.toml");
        std::fs::write(&path, "name = \"Pad\"\n").unwrap();
        let (platform, world) = crate::runtime::testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .push(crate::runtime::testing::pad(
                "p1",
                dp_platform::PadFamily::Xbox,
            ));
        let opts = ServiceOptions::new(dir.path().join("profiles"));
        let mut s = Service::new(Runtime::with_platform(platform), opts).unwrap();
        s.tick();
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path,
        })
        .into_result()
        .unwrap();
        assert_eq!(s.status().virtual_pad.as_deref(), Some("xbox360"));
        assert_eq!(s.live().virtual_pad.as_deref(), Some("xbox360"));
    }

    #[test]
    fn replies_say_when_the_state_cannot_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the state's folder should be: saving always fails.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "").unwrap();
        let (mut s, _) = service(dir.path(), Some(blocker.join("state.toml")));
        let reply = s.handle(ServiceCmd::SetLights {
            player: PlayerLights::Always,
            mic: false,
        });
        let msg = reply.into_result().unwrap();
        assert!(
            msg.starts_with("lights set") && msg.contains("cannot save"),
            "{msg}"
        );
        match s.handle_ipc(Request::Remap { on: false }) {
            Response::Ok { message } => assert!(message.contains("cannot save"), "{message}"),
            r => panic!("{r:?}"),
        }
        assert!(
            matches!(s.handle_ipc(Request::Status), Response::Status(_)),
            "only replies to changes carry it"
        );
    }

    #[test]
    fn stop_forgets_a_failed_apply() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _) = service(dir.path(), None);
        let missing = dir.path().join("missing.toml");
        let r = s.handle_ipc(Request::Apply {
            path: missing.display().to_string(),
        });
        assert!(matches!(r, Response::Error { .. }), "{r:?}");
        let mentions = |s: &Service| s.status().warnings.iter().any(|w| w.contains("missing"));
        assert!(mentions(&s), "the failure is reported");
        s.handle_ipc(Request::Stop);
        assert!(!mentions(&s), "and forgotten once stopped");
    }
}
