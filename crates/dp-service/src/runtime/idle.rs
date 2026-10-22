//! Idle controllers turned off, and the computer kept awake while output plays.

use std::time::Duration;

use dp_core::pad::PadState;
use dp_platform::{DeviceId, GamepadBackend};

use super::Runtime;

/// When a controller last did something.
#[derive(Clone, Copy, Debug)]
pub(super) struct Idle {
    since: u64,
    /// Its state at that moment; drifting sticks and resting triggers are no input.
    reference: PadState,
    /// Turning it off was tried already (wired controllers cannot be).
    tried: bool,
}

/// How far an axis moves before it counts as input.
const IDLE_AXIS: f32 = 0.2;

/// Whether `now` differs from `then` by more than resting noise.
fn moved(then: &PadState, now: &PadState) -> bool {
    then.buttons != now.buttons
        || then
            .axes
            .iter()
            .zip(now.axes)
            .any(|(a, b)| (a - b).abs() > IDLE_AXIS)
        || then
            .touch
            .iter()
            .zip(now.touch)
            .any(|(a, b)| a.is_some() != b.is_some())
}

impl Runtime {
    /// Turn wireless controllers off after `after` without input; `None` never does.
    pub(crate) fn set_power_off_after(&mut self, after: Option<Duration>) {
        self.power.off_after = after.map(|d| d.as_millis() as u64);
        // Count from now, not from before the timeout was set.
        self.power.idle.clear();
    }

    /// Turn a wireless controller off now; `false` if none of the backends can.
    pub(crate) fn power_off(&mut self, id: &DeviceId) -> bool {
        let done = self.pads.power_off(id);
        if done {
            tracing::info!(device = %id, "controller turned off");
        } else {
            tracing::info!(device = %id, "this controller cannot be turned off");
        }
        done
    }

    /// Keep the computer from sleeping while macros run or toggles hold output.
    pub(crate) fn set_prevent_sleep(&mut self, on: bool) {
        self.power.prevent_sleep = on;
    }

    pub(super) fn keep_awake(&mut self) {
        let busy = self.power.prevent_sleep
            && !self.suspended
            && self.sessions.iter().any(|s| s.engine.emulating());
        if busy != self.power.awake {
            self.power.awake = busy;
            self.platform.system.keep_awake(busy);
        }
    }

    /// Turn off the controllers that had no input for the idle timeout.
    pub(super) fn watch_idle(&mut self, now: u64) {
        self.power
            .idle
            .retain(|id, _| self.snapshots.iter().any(|s| &s.info.id == id));
        let Some(after) = self.power.off_after else {
            return;
        };
        let mut off = Vec::new();
        for s in &self.snapshots {
            let fresh = Idle {
                since: now,
                reference: s.state,
                tried: false,
            };
            if !self.power.idle.contains_key(&s.info.id) {
                self.power.idle.insert(s.info.id.clone(), fresh);
            }
            let e = self.power.idle.get_mut(&s.info.id).expect("inserted above");
            if moved(&e.reference, &s.state) {
                *e = fresh;
            } else if !e.tried && now.saturating_sub(e.since) >= after {
                e.tried = true;
                off.push(s.info.id.clone());
            }
        }
        for id in off {
            tracing::info!(device = %id, "no input for a while");
            self.power_off(&id);
        }
    }
}
