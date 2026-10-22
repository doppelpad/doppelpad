//! Profile commands, controller hotkeys and the calibration test turn.

use dp_core::hotkeys::{HotkeyAction, HotkeyDetector, Hotkeys};
use dp_core::profile::Command;
use dp_platform::{DeviceId, PowerAction};

use super::Runtime;

/// One full in-game turn sent as mouse motion, so the user can check `counts_per_turn`.
#[derive(Clone, Copy, Debug)]
pub(super) struct TestTurn {
    counts: f32,
    start: u64,
    ms: u64,
    sent: f32,
}

impl Runtime {
    /// Send `counts` of horizontal mouse motion over `ms`, starting after `delay_ms` (time to
    /// switch to the game): one full turn when `counts` matches the game.
    pub(crate) fn test_turn(&mut self, counts: f32, delay_ms: u64, ms: u64) {
        self.test_turn = Some(TestTurn {
            counts,
            start: self.now_ms() + delay_ms,
            ms: ms.max(1),
            sent: 0.0,
        });
    }

    pub(super) fn send_test_turn(&mut self, now: u64) {
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

    /// Carry out a host command of session `i`; `false` once remapping was turned off.
    pub(super) fn run_command(&mut self, i: usize, c: Command) -> bool {
        match c {
            Command::RemapOff => {
                tracing::info!("remap-off command");
                self.remap_off();
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
                tracing::info!(command = ?c, "gyro switched");
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
                self.release_all(false);
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
            // The engine runs radial menus itself.
            Command::RadialNext
            | Command::RadialPrev
            | Command::RadialUp
            | Command::RadialDown
            | Command::RadialLeft
            | Command::RadialRight
            | Command::RadialApply
            | Command::RadialBack => {}
            Command::PadOverlay => self.overlays.toggle_pad(),
            Command::MappingsOverlay => self.overlays.toggle_mappings(),
            Command::PowerOff => {
                if let Some(id) = self.sessions.get(i).and_then(|s| s.device.clone()) {
                    self.power_off(&id);
                }
            }
            Command::Crosshair(_) | Command::ToggleCrosshair => {
                if let Some(s) = self.sessions.get_mut(i) {
                    s.crosshair_command(&c);
                }
            }
            Command::Unsupported(name) => {
                tracing::debug!(session = i, "ignoring unsupported command {name}");
            }
        }
        true
    }

    /// Run the hotkey detectors on every controller and withhold the buttons of complete
    /// hotkeys from the engines (in `pad_buf`). While the app has the focus (suspended) only
    /// the app-navigation hotkey counts: the app reads the controller then.
    pub(super) fn detect_hotkeys(&mut self, now: u64) -> Vec<(DeviceId, HotkeyAction)> {
        if self.hotkeys.paused {
            self.hotkeys.detectors.clear();
            return Vec::new();
        }
        let mut fired = Vec::new();
        for (s, pad) in self.snapshots.iter().zip(&mut self.pad_buf) {
            let id = &s.info.id;
            if !self.hotkeys.detectors.contains_key(id) {
                self.hotkeys
                    .detectors
                    .insert(id.clone(), HotkeyDetector::default());
            }
            let d = self.hotkeys.detectors.get_mut(id).expect("inserted above");
            if let Some(a) = d.update(&self.hotkeys.set, pad.buttons, now)
                && (!self.suspended || a == HotkeyAction::AppNavigation)
            {
                fired.push((id.clone(), a));
            }
            pad.buttons &= !d.withheld();
        }
        fired
    }

    /// The controller hotkeys in effect.
    pub(crate) fn hotkeys(&self) -> &Hotkeys {
        &self.hotkeys.set
    }

    pub(crate) fn set_hotkeys(&mut self, hotkeys: Hotkeys) {
        self.hotkeys.set = hotkeys;
        self.hotkeys.detectors.clear();
    }

    /// Confirm a hotkey with a rumble pattern on the controller it was pressed on, if the
    /// hotkey settings ask for it.
    pub(crate) fn confirm_hotkey(&mut self, id: &DeviceId, c: super::confirm::Confirm) {
        if self.hotkeys.set.rumble {
            let now = self.now_ms();
            self.pulses.start(id, c, now);
        }
    }

    /// Play the confirmation patterns; a controller whose pattern ended gets its sessions'
    /// rumble again.
    pub(super) fn drive_pulses(&mut self, now: u64) {
        for id in self.pulses.drive(&mut self.pads, now) {
            for s in self
                .sessions
                .iter_mut()
                .filter(|s| s.device.as_ref() == Some(&id))
            {
                s.sent_rumble = None;
            }
        }
    }

    /// Ignore hotkeys for a while (the app is recording a new one).
    pub(crate) fn set_hotkeys_paused(&mut self, on: bool) {
        self.hotkeys.paused = on;
    }

    /// Hotkeys that fired, with the controller they fired on, that the service has to carry
    /// out (slots, remap toggle).
    pub(crate) fn take_hotkey_actions(&mut self) -> Vec<(DeviceId, HotkeyAction)> {
        std::mem::take(&mut self.hotkeys.actions)
    }
}
