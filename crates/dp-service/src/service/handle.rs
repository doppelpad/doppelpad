//! Carrying out commands and IPC requests.

use dp_core::hotkeys::HotkeyAction;
use dp_ipc::{Request, Response};

use super::{
    GYRO_CALIBRATION_MS, Service, ServiceCmd, ServiceReply, TEST_TURN_DELAY_MS, TEST_TURN_MS,
    group_problem, ipc_devices, layout_problem, load, on_off, unsaved_note,
};
use crate::httpapi::HttpSettings;
use crate::runtime::Runtime;
use crate::slots::SLOTS;

impl Service {
    /// Answer one IPC request. `Shutdown` only acknowledges; the host stops the loop.
    pub fn handle_ipc(&mut self, req: Request) -> Response {
        self.unsaved = None;
        match (self.answer_ipc(req), self.unsaved.take()) {
            (Response::Ok { message }, Some(e)) => Response::ok(unsaved_note(&message, &e)),
            (r, _) => r,
        }
    }

    /// The command for a request; slots count 1–4 over IPC.
    fn answer_ipc(&mut self, req: Request) -> Response {
        let slot = |n: usize| (1..=SLOTS).contains(&n).then(|| n - 1);
        let no_slot = |n: usize| Response::error(format!("slot {n} does not exist (1–{SLOTS})"));
        let cmd = match req {
            Request::Status => ServiceCmd::GetStatus,
            Request::Devices => ServiceCmd::GetDevices,
            Request::Apply { path } => ServiceCmd::Apply(path.into()),
            Request::Stop => ServiceCmd::Stop,
            Request::ApplyToSlot {
                path,
                slot: n,
                device,
            } => match slot(n) {
                Some(slot) => ServiceCmd::ApplyToSlot {
                    device,
                    slot,
                    path: path.into(),
                },
                None => return no_slot(n),
            },
            Request::SelectSlot { slot: n, device } => match slot(n) {
                Some(slot) => ServiceCmd::SelectSlot { device, slot },
                None => return no_slot(n),
            },
            Request::ClearSlot {
                slot: Some(n),
                device,
            } => match slot(n) {
                Some(slot) => ServiceCmd::ClearSlot { device, slot },
                None => return no_slot(n),
            },
            Request::ClearSlot { slot: None, device } => {
                for slot in 0..SLOTS {
                    self.run(ServiceCmd::ClearSlot {
                        device: device.clone(),
                        slot,
                    });
                }
                return Response::ok("all slots cleared");
            }
            Request::Remap { on } => ServiceCmd::SetRemap(on),
            Request::AutoSwitch { enabled } => ServiceCmd::SetAutoSwitch(enabled),
            Request::Reload => ServiceCmd::Reload,
            Request::Shutdown => ServiceCmd::Shutdown,
        };
        self.run(cmd).into()
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
            ServiceCmd::GetStatus => ServiceReply::Status(self.status()),
            ServiceCmd::GetDevices => ServiceReply::Devices(ipc_devices(&self.rt)),
            ServiceCmd::Apply(path) => match self.apply_path(&path) {
                Ok(()) => {
                    self.oneoff = Some(path.clone());
                    self.auto_active = None;
                    self.update_state(|s| s.remap_on = true);
                    self.sync_devices();
                    ServiceReply::Ok(format!("applied {}", path.display()))
                }
                Err(e) => ServiceReply::Err(format!("{e:#}")),
            },
            ServiceCmd::Stop => {
                self.rt.stop();
                self.last_error = None;
                self.turn_remap_off();
                ServiceReply::Ok("remapping stopped".into())
            }
            ServiceCmd::Reload => {
                self.reload();
                ServiceReply::Ok(format!("{} profiles", self.loose_profiles()))
            }
            ServiceCmd::Shutdown => ServiceReply::Ok("shutting down".into()),
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
                    s.shared.slots[slot] = Some(path.clone());
                    s.shared.active = slot;
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
                let device = self.has_own_slots(device.as_deref());
                self.slot_set_mut(device.as_deref()).active = slot;
                if let Some(device) = device {
                    return self.device_reply(&device, format!("slot {} selected", slot + 1));
                }
                self.clear_overrides();
                self.save_state();
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
                let device = self.has_own_slots(device.as_deref());
                self.slot_set_mut(device.as_deref()).slots[slot] = None;
                if let Some(device) = device {
                    return self.device_reply(&device, format!("slot {} cleared", slot + 1));
                }
                self.save_state();
                if slot == self.state.shared.active && self.auto_active.is_none() {
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
                ServiceReply::Ok(format!("automatic profile switching {}", on_off(on)))
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
    pub(super) fn run_hotkeys(&mut self) {
        for (id, action) in self.rt.take_hotkey_actions() {
            match action {
                HotkeyAction::Slot(n) => {
                    let device = self.has_own_slots(Some(&id.0));
                    let slots = &self.slot_set(device.as_deref()).slots;
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
}
