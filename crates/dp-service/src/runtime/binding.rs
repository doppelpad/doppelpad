//! Which controller each session reads, and hiding the ones behind a virtual pad.

use dp_platform::groups::DevicePolicy;
use dp_platform::{DeviceId, GamepadBackend};

use super::Runtime;

impl Runtime {
    /// Prefer this controller for the default session when it is connected.
    pub(crate) fn set_preferred_device(&mut self, id: Option<DeviceId>) {
        self.binding.preferred = id;
        self.binding.lost = None;
        self.binding.primary = None; // re-pick on the next poll
    }

    /// Controller of the default session.
    pub(crate) fn primary_device(&self) -> Option<&DeviceId> {
        self.binding.primary.as_ref()
    }

    /// Ignore the blocked devices and play each group as one controller.
    pub(crate) fn set_device_policy(&mut self, policy: DevicePolicy) {
        self.pads.set_policy(policy);
    }

    /// Look for controllers now and retry the ones that could not be opened.
    pub(crate) fn rescan_devices(&mut self) {
        self.pads.rescan();
    }

    /// Hide the controllers of sessions whose profile hides the physical controller behind
    /// a virtual one (and what the hider must hide with them).
    pub(super) fn sync_hiding(&mut self) {
        let mut want: Vec<String> = Vec::new();
        for s in self.sessions.iter().filter(|s| s.hide_wanted) {
            let Some(snap) = s
                .device
                .as_ref()
                .and_then(|id| self.snapshots.iter().find(|x| &x.info.id == id))
            else {
                continue;
            };
            self.platform.hider.expand(&snap.info, &mut want);
        }
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

    /// Which controller each session reads: its own, or for the default session one
    /// without a session of its own: the preferred one, else the one it lost, else the
    /// current one, else the first. Returns whether any session changed controllers.
    pub(super) fn bind_sessions(&mut self) -> bool {
        let owned = |id: &DeviceId| self.sessions.iter().any(|s| s.owner.as_ref() == Some(id));
        let is_free = |p: &&DeviceId| self.device_ids.contains(p) && !owned(p);
        let first_free = || self.device_ids.iter().find(|id| !owned(id));
        let (primary, reason) = if let Some(p) = self.binding.preferred.as_ref().filter(is_free) {
            (Some(p), "picked in the app")
        } else if let Some(p) = self.binding.lost.as_ref().filter(is_free) {
            (Some(p), "the controller in use before is back")
        } else if let Some(p) = self.binding.primary.as_ref().filter(is_free) {
            (Some(p), "still connected")
        } else if self.binding.primary.is_some() {
            let reason = "the controller in use went away or has its own profile";
            (first_free(), reason)
        } else {
            (first_free(), "first free controller")
        };
        let primary = primary.cloned();
        if primary != self.binding.primary {
            match &primary {
                Some(p) => tracing::info!(device = %p, reason, "using controller"),
                None => tracing::info!(reason, "no controller in use"),
            }
            if self.binding.lost.is_none()
                && let Some(old) = &self.binding.primary
                && !is_free(&old)
            {
                self.binding.lost = Some(old.clone());
            }
            if primary.is_some() && primary == self.binding.lost {
                self.binding.lost = None;
            }
            self.binding.primary = primary;
        }
        let mut changed = false;
        for s in &mut self.sessions {
            let device = match &s.owner {
                Some(id) => self.device_ids.contains(id).then_some(id),
                None => self.binding.primary.as_ref(),
            };
            if device != s.device.as_ref() {
                s.device = device.cloned();
                changed = true;
            }
        }
        changed
    }

    pub(super) fn poll_devices(&mut self, now: u64) {
        self.snapshots.clear();
        self.pads.poll(&mut self.snapshots);
        for s in &mut self.snapshots {
            self.calibration
                .apply(s.info.id.as_str(), &mut s.state, now);
        }
        let devices_changed = !self
            .snapshots
            .iter()
            .map(|s| &s.info.id)
            .eq(&self.device_ids);
        if devices_changed {
            self.device_ids.clear();
            self.device_ids
                .extend(self.snapshots.iter().map(|s| s.info.id.clone()));
        }
        let ids = &self.device_ids;
        let primary_lost = self
            .binding
            .primary
            .as_ref()
            .is_some_and(|p| !ids.contains(p));
        let preferred_back =
            self.binding.preferred.is_some() && self.binding.preferred != self.binding.primary;
        let rebound =
            (devices_changed || primary_lost || preferred_back || self.binding.primary.is_none())
                && self.bind_sessions();
        if devices_changed {
            self.hotkeys
                .detectors
                .retain(|id, _| self.device_ids.contains(id));
        }
        // A session on another controller must hide that one instead.
        if devices_changed || rebound {
            self.sync_hiding();
        }
    }
}
