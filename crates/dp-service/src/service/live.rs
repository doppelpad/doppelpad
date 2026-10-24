//! What user interfaces and the CLI see: the status, the warnings and the live snapshot.

use dp_ipc::Status;

use super::Service;
use crate::dsu::DsuServer;
use crate::live::{LiveDevice, LiveState, SharedLive};

impl Service {
    pub(super) fn status(&self) -> Status {
        Status {
            profile: self.rt.profile().map(|p| p.name.clone()),
            profile_path: self.rt.profile_path().map(|p| p.display().to_string()),
            layer: self.rt.layer(),
            auto_switch: self.state.auto_switch,
            foreground: self.fg.now.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| k.name().to_owned()),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
            slots: self.state.shared.display_names(),
            active_slot: self.state.shared.active + 1,
            remap_on: self.state.remap_on,
        }
    }

    fn warnings(&self) -> Vec<String> {
        let mut w = self.rt.warnings();
        w.extend(self.drivers.hiding_problems.iter().cloned());
        w.extend(self.rt.device_problems());
        w.extend(self.errors.last.clone());
        w.extend(self.errors.settings.iter().cloned());
        w.extend(
            self.fg
                .now
                .as_deref()
                .and_then(crate::anticheat::lookup)
                .map(|a| a.warning()),
        );
        w.extend(
            self.errors
                .devices
                .iter()
                .map(|(id, e)| format!("{id}: {e}")),
        );
        w
    }

    /// The snapshot user interfaces read, shared with them.
    pub fn shared_live(&self) -> SharedLive {
        self.shared_live.clone()
    }

    /// Hand the current snapshot to the readers if it changed.
    pub fn publish_live(&self) {
        self.shared_live.publish(self.live());
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
                        .map(|_| self.rt.calibration().status(s.info.id.as_str(), now)),
                    battery: s.battery,
                    raw: s.raw.as_deref().cloned(),
                    mirror: s.info.mirror,
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
            slots: self.state.shared.display_names(),
            active_slot: self.state.shared.active,
            remap_on: self.state.remap_on,
            player_lights: self.state.player_lights,
            mic_led: self.state.mic_led,
            seen_devices: self.rt.seen_devices(),
            blocked: self.state.blocked.clone(),
            groups: self.state.groups.clone(),
            power_off_after_min: self.state.power_off_after_min,
            runtime: self.state.runtime,
            locked: self.pause.locked,
            focus_paused: self.pause.focus_paused,
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
            foreground: self.fg.now.clone(),
            virtual_pad: self.rt.virtual_pad().map(|k| k.name().to_owned()),
            hidden_devices: self.rt.hidden_devices(),
            warnings: self.warnings(),
            drivers: self.drivers.status.clone(),
            device_slots: self
                .state
                .devices
                .iter()
                .map(|(id, d)| crate::live::LiveDeviceSlots {
                    id: id.clone(),
                    slots: d.display_names(),
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
            app_navigation: self.rt.navigation_presses(),
            has_radial: self
                .rt
                .profile()
                .is_some_and(|p| !p.radial_menus.is_empty()),
        }
    }
}
