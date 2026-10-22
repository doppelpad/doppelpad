//! Time in the service: the loop's tick, a clock tests can move forward, and throttles.

use std::time::{Duration, Instant};

/// How long the service loop (and the CLI's foreground runner) sleeps between ticks: about
/// 1 kHz.
pub const TICK: Duration = Duration::from_millis(1);

/// Milliseconds since the runtime started.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Clock {
    start: Instant,
    /// Time tests skipped ahead.
    skipped: u64,
}

impl Clock {
    pub(crate) fn new() -> Clock {
        Clock {
            start: Instant::now(),
            skipped: 0,
        }
    }

    pub(crate) fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64 + self.skipped
    }

    /// Move the clock `ms` forward without waiting.
    #[cfg(test)]
    pub(crate) fn advance(&mut self, ms: u64) {
        self.skipped += ms;
    }
}

/// Something done at most once every `ms` milliseconds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Every {
    ms: u64,
    last: Option<u64>,
}

impl Every {
    pub(crate) const fn new(ms: u64) -> Every {
        Every { ms, last: None }
    }

    /// Whether `ms` have passed at `now` since it was last done, or it never was.
    pub(crate) fn elapsed(&self, now: u64) -> bool {
        self.last.is_none_or(|t| now >= t + self.ms)
    }

    /// Whether it is time at `now`; if so, it counts as done.
    pub(crate) fn due(&mut self, now: u64) -> bool {
        let due = self.elapsed(now);
        if due {
            self.last = Some(now);
        }
        due
    }

    /// It was done at `now`.
    pub(crate) fn done(&mut self, now: u64) {
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_throttles() {
        let mut e = Every::new(100);
        assert!(e.elapsed(5));
        assert!(e.due(5), "the first time is always due");
        assert!(!e.due(104));
        assert!(e.due(105));
        e.done(300);
        assert!(!e.elapsed(399) && e.elapsed(400));
    }

    #[test]
    fn the_clock_skips_ahead() {
        let mut c = Clock::new();
        let before = c.now_ms();
        c.advance(5000);
        assert!(c.now_ms() >= before + 5000);
    }
}
