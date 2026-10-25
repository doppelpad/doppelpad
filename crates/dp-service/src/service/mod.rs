//! The remapping service: owns the [`Runtime`], decides which profile is active
//! (slots, explicit one-off profiles, automatic per-application switching) and answers
//! control requests from the CLI (IPC) and the GUI.

mod command;
mod foreground;
mod handle;
mod live;
mod pause;
mod servers;
#[cfg(test)]
mod tests;

pub use self::command::{ServiceCmd, ServiceReply};

use std::path::{Path, PathBuf};

use anyhow::Result;
use dp_core::profile::Profile;
use dp_ipc::Device;
use dp_platform::Leds;
use dp_platform::groups::DevicePolicy;

use crate::dsu::DsuServer;
use crate::httpapi::HttpApi;
use crate::live::SharedLive;
use crate::net::Listener;
use crate::runtime::{DriverStatus, Runtime};
use crate::slots::{PlayerLights, SlotSet, SlotState};
use crate::timing::Every;
use crossbeam_channel::Receiver;
use dp_platform::DriverReport;
use dp_platform::scan::Rescanner;
use std::time::Duration;

/// How often the foreground application is checked by default.
const FOREGROUND_POLL_MS: u64 = 250;
/// How often to check whether the user's desktop takes input.
const DESKTOP_POLL_MS: u64 = 250;
/// How often driver availability is re-probed.
const DRIVER_POLL: Duration = Duration::from_secs(5);
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

/// Why remapping is paused: the editor has focus, the desktop is locked or a secure desktop
/// shows, the config's game is not in front.
struct PauseState {
    gui_suspended: bool,
    /// When the editor last reported getting the focus.
    gui_since: u64,
    /// The editor reports focus while another program is in front (a missed focus change).
    gui_stale: bool,
    locked: bool,
    desktop_poll: Every,
    focus_paused: bool,
}

/// The application in front.
struct Foreground {
    /// Asked at most this often.
    poll: Every,
    /// As last asked.
    now: Option<String>,
    /// Its process id, where the platform can tell.
    pid: Option<u32>,
    /// The application auto-switch last acted on (`None` while it is off).
    switched_for: Option<String>,
}

/// Driver availability, probed on the `dp-drivers` thread every `DRIVER_POLL`.
struct DriverWatch {
    status: DriverStatus,
    /// Connected devices the hiding driver keeps from Doppelpad.
    hiding_problems: Vec<String>,
    /// A report has arrived.
    known: bool,
    reports: Receiver<DriverReport>,
    _probe: Rescanner,
}

impl DriverWatch {
    fn start(probe: fn() -> DriverReport) -> DriverWatch {
        let (tx, reports) = crossbeam_channel::bounded(1);
        let probe = Rescanner::spawn("dp-drivers", DRIVER_POLL, move |_| {
            let _ = tx.try_send(probe());
        });
        DriverWatch {
            status: DriverStatus::default(),
            hiding_problems: Vec::new(),
            known: false,
            reports,
            _probe: probe,
        }
    }
}

/// What went wrong, for replies and warnings.
#[derive(Default)]
struct Errors {
    /// Applying the active profile.
    last: Option<String>,
    /// Saving the state during the command being handled.
    unsaved: Option<String>,
    /// Applying a controller's own profile, by device id.
    devices: std::collections::BTreeMap<String, String>,
    /// Settings files that could not be used at start-up.
    settings: Vec<String>,
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
    /// The application in front.
    fg: Foreground,
    drivers: DriverWatch,
    errors: Errors,
    /// Connected controllers when their sessions were last brought in line.
    connected: Vec<String>,
    /// The DSU motion server.
    dsu: Listener<DsuServer>,
    /// The local HTTP API.
    http: Listener<HttpApi>,
    /// The live snapshot as user interfaces and the HTTP API read it.
    shared_live: SharedLive,
    /// Why remapping is paused.
    pause: PauseState,
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
        let mut errors = Errors::default();
        let mut state = match opts.state_path.as_deref().map(SlotState::load) {
            Some(loaded) => {
                errors.settings.extend(loaded.problem);
                loaded.value
            }
            None => SlotState::default(),
        };
        if let Some(a) = opts.auto_switch {
            state.auto_switch = a;
        }
        if let Some(path) = &opts.hotkeys_path {
            let loaded = crate::hotkeys::load(path);
            errors.settings.extend(loaded.problem);
            rt.set_hotkeys(loaded.value);
        }
        if let Some(path) = &opts.calibration_path {
            let loaded = crate::calibration::Calibrator::load(path);
            errors.settings.extend(loaded.problem);
            *rt.calibration_mut() = loaded.value;
        }
        let probe = rt.driver_probe();
        let mut svc = Service {
            rt,
            library: Default::default(),
            state,
            oneoff: opts.initial_profile.clone(),
            auto_active: None,
            fg: Foreground {
                poll: Every::new(opts.foreground_poll_ms),
                now: None,
                pid: None,
                switched_for: None,
            },
            drivers: DriverWatch::start(probe),
            errors,
            connected: Vec::new(),
            dsu: Listener::default(),
            http: Listener::default(),
            shared_live: SharedLive::default(),
            pause: PauseState {
                gui_suspended: false,
                gui_since: 0,
                gui_stale: false,
                locked: false,
                desktop_poll: Every::new(DESKTOP_POLL_MS),
                focus_paused: false,
            },
            calibration_save: Every::new(CALIBRATION_SAVE_MS),
            opts,
        };
        svc.apply_policy();
        svc.apply_layouts();
        svc.apply_power_off();
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
            .map(|s| s.info.id.to_string())
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
                self.rt
                    .stop_device(&dp_platform::DeviceId::from(owner.clone()));
                self.errors.devices.remove(&owner);
            }
        }
        for (id, path) in wanted {
            let device = dp_platform::DeviceId::from(id.clone());
            if self.rt.device_profile_path(&device) == Some(&path) {
                continue;
            }
            let result =
                load(&path).and_then(|p| self.rt.apply_device(&device, p, Some(path.clone())));
            match result {
                Ok(()) => {
                    self.errors.devices.remove(&id);
                }
                Err(e) => {
                    tracing::warn!(device = %id, "cannot apply {}: {e:#}", path.display());
                    self.errors.devices.insert(id, format!("{e:#}"));
                }
            }
        }
        self.connected = connected;
    }

    /// A controller's own slots if it has them, else the shared ones.
    fn slot_set(&self, device: Option<&str>) -> &SlotSet {
        device
            .and_then(|d| self.state.devices.get(d))
            .unwrap_or(&self.state.shared)
    }

    fn slot_set_mut(&mut self, device: Option<&str>) -> &mut SlotSet {
        match device.and_then(|d| self.state.devices.get_mut(d)) {
            Some(own) => own,
            None => &mut self.state.shared,
        }
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
            self.errors.unsaved = Some(msg);
        }
    }

    fn apply_path(&mut self, path: &Path) -> Result<()> {
        let result = load(path).and_then(|p| self.rt.apply(p, Some(path.to_path_buf())));
        self.errors.last = result.as_ref().err().map(|e| format!("{e:#}"));
        result
    }

    /// Apply what should be active when no application-specific profile matches:
    /// the one-off profile, else the active slot (if remapping is on), else nothing.
    fn apply_base(&mut self) {
        let target = self.oneoff.clone().or_else(|| {
            self.state
                .remap_on
                .then(|| self.state.shared.active_profile().cloned())
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
                self.errors.last = None;
            }
        }
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

    /// Blocked devices and device groups.
    fn apply_policy(&mut self) {
        self.rt.set_device_policy(DevicePolicy {
            blocked: self.state.blocked.clone(),
            groups: self.state.groups.clone(),
        });
    }

    /// How generic pads' inputs map to pad controls.
    fn apply_layouts(&mut self) {
        self.rt.set_layouts(self.state.layouts.clone());
    }

    /// Turning idle controllers off (setting it starts their idle time anew).
    fn apply_power_off(&mut self) {
        let minutes = self.state.power_off_after_min;
        self.rt.set_power_off_after(
            (minutes > 0).then(|| std::time::Duration::from_secs(u64::from(minutes) * 60)),
        );
    }

    /// Player lights and microphone LED of every controller, from the slots and remapping.
    fn show_lights(&mut self) {
        let state = &self.state;
        self.rt.set_indicators(|id| {
            let slot = state
                .devices
                .get(id.as_str())
                .unwrap_or(&state.shared)
                .active as u8
                + 1;
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
                .any(|(s, id)| s.info.id.as_str() != id)
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
        if let Ok(report) = self.drivers.reports.try_recv() {
            self.take_driver_report(report);
        }
    }

    /// Probe the drivers now, on this thread (an explicit refresh).
    fn check_drivers(&mut self) {
        let report = (self.rt.driver_probe())();
        self.take_driver_report(report);
    }

    fn take_driver_report(&mut self, report: DriverReport) {
        let drivers = DriverStatus {
            virtual_pads: report.virtual_pads.into(),
            device_hiding: report.device_hiding.into(),
            input_access: report.input_access.into(),
            bluetooth: report.bluetooth.into(),
        };
        if !self.drivers.known || drivers != self.drivers.status {
            tracing::info!(
                "drivers: virtual pads {}, device hiding {}, keyboard and mouse {}, Bluetooth {}",
                drivers.virtual_pads,
                drivers.device_hiding,
                drivers.input_access,
                drivers.bluetooth
            );
        }
        let problems = report.hiding_problems;
        for p in problems
            .iter()
            .filter(|p| !self.drivers.hiding_problems.contains(p))
        {
            tracing::warn!("{p}");
        }
        self.drivers.status = drivers;
        self.drivers.hiding_problems = problems;
        self.drivers.known = true;
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
