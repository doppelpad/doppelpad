//! Pausing remapping: the editor in front, a locked desktop, the game not in front.

use super::Service;

impl Service {
    /// Pause while the user's desktop does not take input (locked, Ctrl+Alt+Del, an
    /// elevation prompt): nothing stays pressed meanwhile.
    pub(super) fn watch_desktop(&mut self, now: u64) {
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
    pub(super) fn watch_focus(&mut self) {
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

    pub(super) fn apply_suspend(&mut self) {
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
}
