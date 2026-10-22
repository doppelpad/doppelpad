//! The application in front, and switching profiles with it.

use super::Service;

impl Service {
    /// Ask which application is in front, at most every poll interval; `true` if it asked.
    pub(super) fn poll_foreground(&mut self, now: u64) -> bool {
        if !self.fg.poll.due(now) {
            return false;
        }
        let fg = &mut self.rt.platform_mut().foreground;
        self.fg.now = fg.foreground_process();
        self.fg.pid = fg.foreground_pid();
        true
    }

    pub(super) fn auto_switch_tick(&mut self) {
        if !self.state.auto_switch || !self.state.remap_on {
            // Look again once switching is back on, even if the same application kept
            // the focus meanwhile.
            self.fg.switched_for = None;
            return;
        }
        if self.fg.now == self.fg.switched_for {
            return;
        }
        self.fg.switched_for = self.fg.now.clone();
        let target = self
            .fg
            .switched_for
            .as_deref()
            .and_then(|exe| self.auto_profile_for(exe));
        match target {
            Some(path) if self.rt.profile_path() != Some(&path) => {
                tracing::info!(app = ?self.fg.switched_for, profile = %path.display(), "auto-switch");
                match self.apply_path(&path) {
                    Ok(()) => self.auto_active = Some(path),
                    Err(e) => tracing::warn!("auto-switch failed: {e:#}"),
                }
            }
            None if self.auto_active.take().is_some() => self.apply_base(),
            _ => {}
        }
    }
}
