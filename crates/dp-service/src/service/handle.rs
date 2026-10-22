//! Carrying out commands and IPC requests.

use std::path::PathBuf;

use dp_core::hotkeys::{HotkeyAction, Hotkeys};
use dp_ipc::{Request, Response};
use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;

use super::{
    GYRO_CALIBRATION_MS, Service, ServiceCmd, ServiceReply, TEST_TURN_DELAY_MS, TEST_TURN_MS,
    ipc_devices, load, on_off, unsaved_note,
};
use crate::dsu::DsuSettings;
use crate::httpapi::HttpSettings;
use crate::runtime::Runtime;
use crate::slots::{PlayerLights, RuntimeOptions, SLOTS};

impl Service {
    /// Answer one IPC request. `Shutdown` only acknowledges; the host stops the loop.
    pub fn handle_ipc(&mut self, req: Request) -> Response {
        self.errors.unsaved = None;
        match (self.answer_ipc(req), self.errors.unsaved.take()) {
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
        match self.errors.devices.get(device) {
            Some(e) => ServiceReply::Err(e.clone()),
            None => ServiceReply::Ok(ok),
        }
    }

    /// Execute one command. When a change could not be saved, the reply says so.
    pub fn handle(&mut self, cmd: ServiceCmd) -> ServiceReply {
        self.errors.unsaved = None;
        match (self.run(cmd), self.errors.unsaved.take()) {
            (ServiceReply::Ok(m), Some(e)) => ServiceReply::Ok(unsaved_note(&m, &e)),
            (r, _) => r,
        }
    }

    fn run(&mut self, cmd: ServiceCmd) -> ServiceReply {
        match cmd {
            ServiceCmd::GetStatus => ServiceReply::Status(self.status()),
            ServiceCmd::GetDevices => ServiceReply::Devices(ipc_devices(&self.rt)),
            ServiceCmd::Apply(path) => self.apply(path),
            ServiceCmd::Stop => self.stop(),
            ServiceCmd::Reload => {
                self.reload();
                ServiceReply::Ok(format!("{} profiles", self.loose_profiles()))
            }
            ServiceCmd::Shutdown => ServiceReply::Ok("shutting down".into()),
            ServiceCmd::ApplyToSlot { device, slot, path } => {
                self.apply_to_slot(device, slot, path)
            }
            ServiceCmd::SelectSlot { device, slot } => self.select_slot(device, slot),
            ServiceCmd::ClearSlot { device, slot } => self.clear_slot(device, slot),
            ServiceCmd::OwnSlots { device, on } => self.own_slots(device, on),
            ServiceCmd::SetRemap(on) => self.set_remap(on),
            ServiceCmd::SetSuspended(on) => self.set_suspended(on),
            ServiceCmd::SetPreferredDevice(id) => {
                self.rt
                    .set_preferred_device(id.map(dp_platform::DeviceId::from));
                ServiceReply::Ok("device preference set".into())
            }
            ServiceCmd::SetAutoSwitch(on) => {
                self.set_auto_switch(on);
                ServiceReply::Ok(format!("automatic profile switching {}", on_off(on)))
            }
            ServiceCmd::SetBlocked { id, blocked } => self.set_blocked(id, blocked),
            ServiceCmd::SetGroups(groups) => self.set_groups(groups),
            ServiceCmd::SetPowerOffAfter(minutes) => self.set_power_off_after(minutes),
            ServiceCmd::SetLayout(layout) => self.set_layout(layout),
            ServiceCmd::SetRuntimeOptions(o) => self.set_runtime_options(o),
            ServiceCmd::SetHttp(settings) => self.set_http(settings),
            ServiceCmd::NewHttpToken => self.new_http_token(),
            ServiceCmd::SetDsu(settings) => self.set_dsu(settings),
            ServiceCmd::ForgetLayout(device) => self.forget_layout(device),
            ServiceCmd::PowerOff(id) => self.power_off(id),
            ServiceCmd::SetLights { player, mic } => self.set_lights(player, mic),
            ServiceCmd::RefreshDrivers => {
                self.check_drivers(self.rt.now_ms());
                ServiceReply::Ok("drivers checked".into())
            }
            ServiceCmd::RescanDevices => {
                self.rt.rescan_devices();
                ServiceReply::Ok("looking for controllers".into())
            }
            ServiceCmd::GetHotkeys => ServiceReply::Hotkeys(self.rt.hotkeys().clone()),
            ServiceCmd::SetHotkeys(h) => self.set_hotkeys(h),
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

    /// Load and activate a config outside the slots.
    fn apply(&mut self, path: PathBuf) -> ServiceReply {
        match self.apply_path(&path) {
            Ok(()) => {
                self.oneoff = Some(path.clone());
                self.auto_active = None;
                self.update_state(|s| s.remap_on = true);
                self.sync_devices();
                ServiceReply::Ok(format!("applied {}", path.display()))
            }
            Err(e) => ServiceReply::Err(format!("{e:#}")),
        }
    }

    /// Stop remapping and forget the overrides.
    fn stop(&mut self) -> ServiceReply {
        self.rt.stop();
        self.errors.last = None;
        self.turn_remap_off();
        ServiceReply::Ok("remapping stopped".into())
    }

    /// Put a config into a slot and make that slot active.
    fn apply_to_slot(
        &mut self,
        device: Option<String>,
        slot: usize,
        path: PathBuf,
    ) -> ServiceReply {
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
            self.rt
                .stop_device(&dp_platform::DeviceId::from(device.clone()));
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

    fn select_slot(&mut self, device: Option<String>, slot: usize) -> ServiceReply {
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
        match &self.errors.last {
            Some(e) => ServiceReply::Err(e.clone()),
            None => ServiceReply::Ok(format!("slot {} selected", slot + 1)),
        }
    }

    fn clear_slot(&mut self, device: Option<String>, slot: usize) -> ServiceReply {
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

    fn own_slots(&mut self, device: String, on: bool) -> ServiceReply {
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

    fn set_remap(&mut self, on: bool) -> ServiceReply {
        if !on {
            self.clear_overrides();
        }
        self.update_state(|s| s.remap_on = on);
        self.apply_base();
        self.sync_devices();
        match (&self.errors.last, on) {
            (Some(e), true) => ServiceReply::Err(e.clone()),
            _ => ServiceReply::Ok(format!("remap {}", on_off(on))),
        }
    }

    /// The editor got or lost the focus.
    fn set_suspended(&mut self, on: bool) -> ServiceReply {
        if on && !self.pause.gui_suspended {
            self.pause.gui_since = self.rt.now_ms();
        }
        self.pause.gui_suspended = on;
        self.apply_suspend();
        ServiceReply::Ok(format!(
            "remapping {}",
            if on { "paused" } else { "resumed" }
        ))
    }

    fn set_blocked(&mut self, id: String, blocked: bool) -> ServiceReply {
        self.state.blocked.retain(|b| *b != id);
        if blocked {
            self.state.blocked.push(id);
        }
        self.apply_policy();
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

    fn set_groups(&mut self, groups: Vec<DeviceGroup>) -> ServiceReply {
        match DeviceGroup::problems(&groups) {
            Some(e) => ServiceReply::Err(e),
            None => {
                self.state.groups = groups;
                self.apply_policy();
                self.save_state();
                ServiceReply::Ok("device groups set".into())
            }
        }
    }

    fn set_power_off_after(&mut self, minutes: u32) -> ServiceReply {
        self.state.power_off_after_min = minutes;
        self.apply_power_off();
        self.save_state();
        ServiceReply::Ok(match minutes {
            0 => "controllers stay on".into(),
            m => format!("controllers turn off after {m} min without input"),
        })
    }

    fn set_layout(&mut self, layout: GenericLayout) -> ServiceReply {
        match layout.problem() {
            Some(e) => ServiceReply::Err(e),
            None => {
                self.state.layouts.retain(|l| l.device != layout.device);
                self.state.layouts.push(layout);
                self.apply_layouts();
                self.save_state();
                ServiceReply::Ok("layout saved".into())
            }
        }
    }

    fn set_runtime_options(&mut self, o: RuntimeOptions) -> ServiceReply {
        self.update_state(|s| s.runtime = o);
        self.rt.set_prevent_sleep(o.prevent_sleep);
        self.sync_devices();
        ServiceReply::Ok("runtime options set".into())
    }

    fn set_http(&mut self, settings: HttpSettings) -> ServiceReply {
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

    fn new_http_token(&mut self) -> ServiceReply {
        self.state.http.token = crate::httpapi::new_token();
        self.apply_http();
        self.save_state();
        ServiceReply::Ok("new token".into())
    }

    fn set_dsu(&mut self, settings: DsuSettings) -> ServiceReply {
        self.state.dsu = settings;
        self.apply_dsu();
        self.save_state();
        match &self.dsu.error {
            Some(e) => ServiceReply::Err(e.clone()),
            None if settings.enabled => ServiceReply::Ok("motion server on".into()),
            None => ServiceReply::Ok("motion server off".into()),
        }
    }

    fn forget_layout(&mut self, device: String) -> ServiceReply {
        self.state.layouts.retain(|l| l.device != device);
        self.apply_layouts();
        self.save_state();
        ServiceReply::Ok("layout removed".into())
    }

    /// Turn a wireless controller off now.
    fn power_off(&mut self, id: String) -> ServiceReply {
        if self.rt.power_off(&dp_platform::DeviceId::from(id)) {
            ServiceReply::Ok("controller turned off".into())
        } else {
            ServiceReply::Err(
                "this controller cannot be turned off (only wireless ones can)".into(),
            )
        }
    }

    fn set_lights(&mut self, player: PlayerLights, mic: bool) -> ServiceReply {
        self.update_state(|s| {
            s.player_lights = player;
            s.mic_led = mic;
        });
        ServiceReply::Ok("lights set".into())
    }

    fn set_hotkeys(&mut self, h: Hotkeys) -> ServiceReply {
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

    /// Carry out the hotkeys the runtime reported (slots and the remap switch). A slot
    /// hotkey selects a slot of the controller it was pressed on if that has slots of its
    /// own, else a shared slot.
    pub(super) fn run_hotkeys(&mut self) {
        for (id, action) in self.rt.take_hotkey_actions() {
            match action {
                HotkeyAction::Slot(n) => {
                    let device = self.has_own_slots(Some(id.as_str()));
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
