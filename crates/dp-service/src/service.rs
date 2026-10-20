//! The remapping service: owns the [`Runtime`], decides which profile is active
//! (slots, explicit one-off profiles, automatic per-application switching) and answers
//! control requests from the CLI (IPC) and the GUI.

use std::path::{Path, PathBuf};

use anyhow::Result;
use dp_core::profile::Profile;
use dp_ipc::{Device, Request, Response, Status};

use crate::live::{LiveDevice, LiveState};
use crate::runtime::{DriverStatus, Runtime};
use crate::slots::{SLOTS, SlotState};

/// How often the foreground application is checked by default.
pub const FOREGROUND_POLL_MS: u64 = 250;
/// How often driver availability is re-probed.
const DRIVER_POLL_MS: u64 = 5000;

#[derive(Clone, Debug)]
pub struct ServiceOptions {
    /// Directory scanned for profiles that switch automatically by foreground application.
    pub profiles_dir: PathBuf,
    /// Where slots and switches are persisted; `None` keeps them in memory only.
    pub state_path: Option<PathBuf>,
    /// Profile to apply at start, independent of the slots.
    pub initial_profile: Option<PathBuf>,
    /// Override the persisted auto-switch setting.
    pub auto_switch: Option<bool>,
    pub foreground_poll_ms: u64,
}

impl ServiceOptions {
    pub fn new(profiles_dir: PathBuf) -> Self {
        ServiceOptions {
            profiles_dir,
            state_path: None,
            initial_profile: None,
            auto_switch: None,
            foreground_poll_ms: FOREGROUND_POLL_MS,
        }
    }
}

/// Commands beyond the IPC protocol, used by the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum ServiceCmd {
    /// A request of the IPC protocol (`doppelpad apply|stop|status|...`).
    Ipc(Request),
    /// Put a config into a slot and make that slot active.
    ApplyToSlot {
        slot: usize,
        path: PathBuf,
    },
    SelectSlot(usize),
    ClearSlot(usize),
    SetRemap(bool),
    SetSuspended(bool),
    SetPreferredDevice(Option<String>),
    SetAutoSwitch(bool),
    /// Re-probe ViGEmBus / HidHide now.
    RefreshDrivers,
    /// Look for controllers now, e.g. after another program released them.
    RescanDevices,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServiceReply {
    Ipc(Response),
    Ok(String),
    Err(String),
}

impl ServiceReply {
    pub fn into_result(self) -> Result<String, String> {
        match self {
            ServiceReply::Ok(m) | ServiceReply::Ipc(Response::Ok { message: m }) => Ok(m),
            ServiceReply::Err(m) | ServiceReply::Ipc(Response::Error { message: m }) => Err(m),
            ServiceReply::Ipc(other) => Ok(format!("{other:?}")),
        }
    }
}

pub struct Service {
    rt: Runtime,
    opts: ServiceOptions,
    profiles: Vec<(PathBuf, Profile)>,
    library: dp_config::library::Library,
    state: SlotState,
    /// Profile applied explicitly over IPC (`doppelpad apply`); takes precedence over slots.
    oneoff: Option<PathBuf>,
    /// Profile applied because its application is in the foreground.
    auto_active: Option<PathBuf>,
    foreground: Option<String>,
    last_fg_check: Option<u64>,
    drivers: DriverStatus,
    drivers_checked: Option<u64>,
    /// [`Runtime::hiding_problems`], refreshed with the driver status.
    hiding_problems: Vec<String>,
    last_error: Option<String>,
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
    pub fn new(rt: Runtime, opts: ServiceOptions) -> Result<Service> {
        let mut state = opts
            .state_path
            .as_deref()
            .map(SlotState::load)
            .unwrap_or_default();
        if let Some(a) = opts.auto_switch {
            state.auto_switch = a;
        }
        let mut svc = Service {
            rt,
            profiles: Vec::new(),
            library: Default::default(),
            state,
            oneoff: opts.initial_profile.clone(),
            auto_active: None,
            foreground: None,
            last_fg_check: None,
            drivers: DriverStatus::default(),
            drivers_checked: None,
            hiding_problems: Vec::new(),
            last_error: None,
            opts,
        };
        svc.reload();
        if let Some(p) = svc.oneoff.clone() {
            svc.apply_path(&p)?;
        } else {
            svc.apply_base();
        }
        Ok(svc)
    }

    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.rt
    }

    pub fn slots(&self) -> &SlotState {
        &self.state
    }

    fn reload(&mut self) {
        let (profiles, errors) = dp_config::list_profiles(&self.opts.profiles_dir);
        for e in errors {
            tracing::warn!("{e}");
        }
        self.library = dp_config::library::scan(&self.opts.profiles_dir);
        for e in &self.library.errors {
            tracing::warn!("{e}");
        }
        tracing::info!(
            games = self.library.games.len(),
            loose = profiles.len(),
            dir = %self.opts.profiles_dir.display(),
            "profiles loaded"
        );
        self.profiles = profiles;
    }

    /// Re-read the profiles directory (after the GUI changed games or configs).
    pub fn reload_library(&mut self) {
        self.reload();
    }

    /// Config to activate for a foreground executable: a game's default config, else a
    /// loose profile listing the executable.
    fn auto_profile_for(&self, exe: &str) -> Option<PathBuf> {
        dp_config::library::game_for_process(&self.library, exe)
            .map(|(_, p)| p)
            .or_else(|| dp_config::profile_for_process(&self.profiles, exe).map(|(p, _)| p.clone()))
    }

    fn save_state(&self) {
        if let Some(path) = &self.opts.state_path
            && let Err(e) = self.state.save(path)
        {
            tracing::warn!("cannot save {}: {e}", path.display());
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
                self.rt.stop();
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
            foreground: self.foreground.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| format!("{k:?}")),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
        }
    }

    fn warnings(&self) -> Vec<String> {
        let mut w = self.rt.warnings().to_vec();
        w.extend(self.hiding_problems.iter().cloned());
        w.extend(self.rt.device_problems());
        w.extend(self.last_error.clone());
        w
    }

    /// Answer one IPC request. `Shutdown` only acknowledges; the host stops the loop.
    pub fn handle_ipc(&mut self, req: Request) -> Response {
        match req {
            Request::Status => Response::Status(self.status()),
            Request::Apply { path } => {
                let path = PathBuf::from(path);
                match self.apply_path(&path) {
                    Ok(()) => {
                        self.oneoff = Some(path.clone());
                        self.auto_active = None;
                        self.state.remap_on = true;
                        self.save_state();
                        Response::Ok {
                            message: format!("applied {}", path.display()),
                        }
                    }
                    Err(e) => Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }
            Request::Stop => {
                self.rt.stop();
                self.oneoff = None;
                self.auto_active = None;
                self.state.remap_on = false;
                self.save_state();
                Response::Ok {
                    message: "remapping stopped".into(),
                }
            }
            Request::Devices => Response::Devices {
                devices: ipc_devices(&self.rt),
            },
            Request::AutoSwitch { enabled } => {
                self.set_auto_switch(enabled);
                Response::Ok {
                    message: format!(
                        "automatic profile switching {}",
                        if enabled { "on" } else { "off" }
                    ),
                }
            }
            Request::Reload => {
                self.reload();
                Response::Ok {
                    message: format!("{} profiles", self.profiles.len()),
                }
            }
            Request::Shutdown => Response::Ok {
                message: "shutting down".into(),
            },
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

    /// Execute one command.
    pub fn handle(&mut self, cmd: ServiceCmd) -> ServiceReply {
        match cmd {
            ServiceCmd::Ipc(req) => ServiceReply::Ipc(self.handle_ipc(req)),
            ServiceCmd::ApplyToSlot { slot, path } => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                // Validate before touching the slot.
                if let Err(e) = load(&path) {
                    return ServiceReply::Err(format!("{e:#}"));
                }
                self.state.slots[slot] = Some(path.clone());
                self.state.active = slot;
                self.state.remap_on = true;
                self.oneoff = None;
                self.auto_active = None;
                self.save_state();
                // Re-apply even if the same file is active: it may have been edited.
                match self.apply_path(&path) {
                    Ok(()) => ServiceReply::Ok(format!("applied to slot {}", slot + 1)),
                    Err(e) => ServiceReply::Err(format!("{e:#}")),
                }
            }
            ServiceCmd::SelectSlot(slot) => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                self.state.active = slot;
                self.oneoff = None;
                self.auto_active = None;
                self.save_state();
                self.apply_base();
                match &self.last_error {
                    Some(e) => ServiceReply::Err(e.clone()),
                    None => ServiceReply::Ok(format!("slot {} selected", slot + 1)),
                }
            }
            ServiceCmd::ClearSlot(slot) => {
                if let Some(e) = Self::bad_slot(slot) {
                    return e;
                }
                self.state.slots[slot] = None;
                self.save_state();
                if slot == self.state.active && self.auto_active.is_none() {
                    self.apply_base();
                }
                ServiceReply::Ok(format!("slot {} cleared", slot + 1))
            }
            ServiceCmd::SetRemap(on) => {
                self.state.remap_on = on;
                if !on {
                    self.oneoff = None;
                    self.auto_active = None;
                }
                self.save_state();
                self.apply_base();
                match (&self.last_error, on) {
                    (Some(e), true) => ServiceReply::Err(e.clone()),
                    _ => ServiceReply::Ok(format!("remap {}", if on { "on" } else { "off" })),
                }
            }
            ServiceCmd::SetSuspended(on) => {
                self.rt.set_suspended(on);
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
                ServiceReply::Ok(format!("auto-switch {}", if on { "on" } else { "off" }))
            }
            ServiceCmd::RefreshDrivers => {
                self.check_drivers(self.rt.now_ms());
                ServiceReply::Ok("drivers checked".into())
            }
            ServiceCmd::RescanDevices => {
                self.rt.rescan_devices();
                ServiceReply::Ok("looking for controllers".into())
            }
        }
    }

    /// One loop iteration: engine tick, remap-off handling, auto-switch, driver probing.
    pub fn tick(&mut self) {
        self.rt.tick();
        if self.rt.take_remap_off_request() {
            self.state.remap_on = false;
            self.oneoff = None;
            self.auto_active = None;
            self.save_state();
        }
        let now = self.rt.now_ms();
        self.auto_switch_tick(now);
        if self
            .drivers_checked
            .is_none_or(|t| now >= t + DRIVER_POLL_MS)
        {
            self.check_drivers(now);
        }
    }

    fn check_drivers(&mut self, now: u64) {
        let drivers = self.rt.drivers();
        if self.drivers_checked.is_none() || drivers != self.drivers {
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
        self.drivers_checked = Some(now);
    }

    fn auto_switch_tick(&mut self, now: u64) {
        if !self.state.auto_switch
            || self
                .last_fg_check
                .is_some_and(|t| now < t + self.opts.foreground_poll_ms)
        {
            return;
        }
        self.last_fg_check = Some(now);
        let fg = self.rt.platform_mut().foreground.foreground_process();
        if fg == self.foreground {
            return;
        }
        self.foreground = fg;
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
                })
                .collect(),
            primary: self.rt.primary_device().map(|d| d.to_string()),
            profile: self.rt.profile().map(|p| p.name.clone()),
            profile_path: self.rt.profile_path().map(|p| p.display().to_string()),
            layer: self.rt.layer(),
            slots: self
                .state
                .slots
                .iter()
                .map(|s| s.as_ref().map(|p| p.display().to_string()))
                .collect(),
            active_slot: self.state.active,
            remap_on: self.state.remap_on,
            auto_switch: self.state.auto_switch,
            suspended: self.rt.is_suspended(),
            foreground: self.foreground.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| format!("{k:?}")),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
            drivers: self.drivers.clone(),
        }
    }

    /// Release everything and unhide devices.
    pub fn shutdown(&mut self) {
        self.rt.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use dp_platform::{ForegroundWatcher, NullPlatform};

    use super::*;

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
                slot: 0,
                path: a.clone()
            })
            .into_result()
            .is_ok()
        );
        assert!(
            s.handle(ServiceCmd::ApplyToSlot {
                slot: 2,
                path: b.clone()
            })
            .into_result()
            .is_ok()
        );
        assert_eq!(active_name(&s).as_deref(), Some("B"));

        s.handle(ServiceCmd::SelectSlot(0));
        assert_eq!(active_name(&s).as_deref(), Some("A"));

        s.handle(ServiceCmd::SetRemap(false));
        assert_eq!(active_name(&s), None, "remap off unloads but keeps slots");
        assert_eq!(s.slots().slots[2], Some(b.clone()));
        s.handle(ServiceCmd::SetRemap(true));
        assert_eq!(active_name(&s).as_deref(), Some("A"));

        s.handle(ServiceCmd::ClearSlot(0));
        assert_eq!(active_name(&s), None, "clearing the active slot unloads it");
        assert!(s.handle(ServiceCmd::SelectSlot(9)).into_result().is_err());

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
            s.handle(ServiceCmd::ApplyToSlot { slot: 1, path: bad })
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
        s.handle(ServiceCmd::ApplyToSlot { slot: 0, path: a });
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

    #[test]
    fn auto_switch_applies_and_restores() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        write_profile(&profiles, "game.toml", "Game", &["game.exe"]);
        let base = write_profile(dir.path(), "base.toml", "Base", &[]);
        let (mut s, fg) = service(dir.path(), None);
        s.handle(ServiceCmd::ApplyToSlot {
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
}
