//! The application in front: the owner of the frontmost ordinary window in the window
//! server's list, then its process name. The list needs no permission and no run loop, so
//! this works in the daemon as in the app.

use std::time::{Duration, Instant};

use super::{process, windows};
use crate::ForegroundWatcher;

/// A process id asked for this recently is answered from the last look at the window list.
const FRESH: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct Foreground {
    /// When the window list was last read, and the process in front then.
    last: Option<(Instant, Option<u32>)>,
}

impl Foreground {
    fn front_pid(&mut self) -> Option<u32> {
        let pid = windows::front().map(|w| w.pid);
        self.last = Some((Instant::now(), pid));
        pid
    }
}

impl ForegroundWatcher for Foreground {
    fn foreground_process(&mut self) -> Option<String> {
        process::process_name(self.front_pid()?)
    }

    /// The service asks for the name first, then for the id: one look at the list serves both.
    fn foreground_pid(&mut self) -> Option<u32> {
        match self.last {
            Some((at, pid)) if at.elapsed() < FRESH => pid,
            _ => self.front_pid(),
        }
    }
}
