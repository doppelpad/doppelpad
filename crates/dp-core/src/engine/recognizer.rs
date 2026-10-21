//! Press-pattern recognizer: turns press/release edges of one trigger into
//! `Begin`/`End` events for the Single / Long / Double / Triple / Start / Release
//! activators.
//!
//! Single, Long, Double and Triple are mutually exclusive outcomes of one press
//! pattern; Start and Release fire on every edge, in parallel.

use crate::profile::Activator;

/// Timing for one trigger.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RecConfig {
    pub single: bool,
    pub long: bool,
    pub double: bool,
    pub triple: bool,
    pub start: bool,
    pub release: bool,
    /// Hold time for Long.
    pub t_long: u64,
    /// Second press must begin within this time after the first release.
    pub t_double: u64,
    /// Third press must begin within this time after the first release.
    pub t_triple: u64,
    /// Length of activations that can only be decided after the fact.
    pub t_pulse: u64,
}

impl Default for RecConfig {
    fn default() -> Self {
        RecConfig {
            single: true,
            long: false,
            double: false,
            triple: false,
            start: false,
            release: false,
            t_long: 300,
            t_double: 150,
            t_triple: 300,
            t_pulse: 20,
        }
    }
}

/// Minimum length of a Start activation.
pub const START_MIN_MS: u64 = 20;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RecEvent {
    Begin(Activator, u64),
    End(Activator, u64),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Idle,
    /// Pressed with no pattern activator configured (only Start/Release, or nothing).
    Held,
    /// Single is the only pattern: active for the duration of the press.
    SingleActive,
    /// First press held.
    P1 {
        t0: u64,
    },
    LongActive,
    /// Released once; waiting for a second press.
    W1 {
        t_r1: u64,
    },
    DoubleActive,
    /// Second press held while Triple is still possible.
    P2 {
        t_r1: u64,
    },
    /// Triple window closed during the second press; Double active until release.
    DoubleLate,
    /// Released twice; waiting for a third press.
    W2 {
        t_r1: u64,
    },
    TripleActive,
    /// Pattern failed; wait for release.
    Dead,
}

#[derive(Clone, Debug)]
pub struct Recognizer {
    state: State,
    /// Pending ends of pulsed activations and minimum-length Start activations.
    pending_ends: Vec<(Activator, u64)>,
    /// Start activation waiting for release (or its minimum length).
    start_begin: Option<u64>,
    start_released: bool,
}

impl Default for Recognizer {
    fn default() -> Self {
        Recognizer {
            state: State::Idle,
            pending_ends: Vec::new(),
            start_begin: None,
            start_released: false,
        }
    }
}

impl Recognizer {
    pub fn is_idle(&self) -> bool {
        self.state == State::Idle && self.pending_ends.is_empty() && self.start_begin.is_none()
    }

    /// Forget everything without emitting events (caller ends activations itself).
    pub fn reset(&mut self) {
        *self = Recognizer::default();
    }

    fn pulse(&mut self, kind: Activator, t: u64, cfg: &RecConfig, out: &mut Vec<RecEvent>) {
        out.push(RecEvent::Begin(kind, t));
        self.pending_ends.push((kind, t + cfg.t_pulse));
    }

    /// Advance timers up to `now`, emitting events stamped with their deadline.
    pub fn poll(&mut self, now: u64, cfg: &RecConfig, out: &mut Vec<RecEvent>) {
        loop {
            match self.state {
                State::P1 { t0 } if cfg.long && now >= t0 + cfg.t_long => {
                    out.push(RecEvent::Begin(Activator::Long, t0 + cfg.t_long));
                    self.state = State::LongActive;
                }
                State::W1 { t_r1 } if now > t_r1 + cfg.t_double => {
                    let deadline = t_r1 + cfg.t_double;
                    self.state = State::Idle;
                    if cfg.single {
                        self.pulse(Activator::Single, deadline, cfg, out);
                    }
                }
                State::P2 { t_r1 } if now >= t_r1 + cfg.t_triple => {
                    if cfg.double {
                        out.push(RecEvent::Begin(Activator::Double, t_r1 + cfg.t_triple));
                        self.state = State::DoubleLate;
                    } else {
                        self.state = State::Dead;
                    }
                }
                State::W2 { t_r1 } if now > t_r1 + cfg.t_triple => {
                    let deadline = t_r1 + cfg.t_triple;
                    self.state = State::Idle;
                    if cfg.double {
                        self.pulse(Activator::Double, deadline, cfg, out);
                    }
                }
                _ => break,
            }
        }
        if let Some(begin) = self.start_begin
            && self.start_released
            && now >= begin + START_MIN_MS
        {
            out.push(RecEvent::End(Activator::Start, begin + START_MIN_MS));
            self.start_begin = None;
        }
        self.pending_ends.sort_by_key(|&(_, t)| t);
        while let Some(&(kind, t)) = self.pending_ends.first() {
            if t > now {
                break;
            }
            out.push(RecEvent::End(kind, t));
            self.pending_ends.remove(0);
        }
    }

    pub fn press(&mut self, t: u64, cfg: &RecConfig, out: &mut Vec<RecEvent>) {
        self.poll(t, cfg, out);
        if cfg.start {
            if self.start_begin.is_some() {
                // Previous Start still within its minimum length: end it now.
                out.push(RecEvent::End(Activator::Start, t));
            }
            out.push(RecEvent::Begin(Activator::Start, t));
            self.start_begin = Some(t);
            self.start_released = false;
        }
        let patterns = cfg.long || cfg.double || cfg.triple;
        self.state = match self.state {
            State::Idle if !patterns && cfg.single => {
                out.push(RecEvent::Begin(Activator::Single, t));
                State::SingleActive
            }
            State::Idle if !patterns => State::Held,
            State::Idle => State::P1 { t0: t },
            State::W1 { t_r1 } if cfg.triple => State::P2 { t_r1 },
            State::W1 { .. } => {
                out.push(RecEvent::Begin(Activator::Double, t));
                State::DoubleActive
            }
            State::W2 { .. } => {
                out.push(RecEvent::Begin(Activator::Triple, t));
                State::TripleActive
            }
            other => other,
        };
    }

    pub fn release(&mut self, t: u64, cfg: &RecConfig, out: &mut Vec<RecEvent>) {
        self.poll(t, cfg, out);
        if cfg.start
            && let Some(begin) = self.start_begin
        {
            if t >= begin + START_MIN_MS {
                out.push(RecEvent::End(Activator::Start, t));
                self.start_begin = None;
            } else {
                self.start_released = true;
            }
        }
        if cfg.release {
            self.pulse(Activator::Release, t, cfg, out);
        }
        self.state = match self.state {
            State::SingleActive => {
                out.push(RecEvent::End(Activator::Single, t));
                State::Idle
            }
            State::P1 { .. } if cfg.double || cfg.triple => State::W1 { t_r1: t },
            State::P1 { .. } => {
                if cfg.single {
                    self.pulse(Activator::Single, t, cfg, out);
                }
                State::Idle
            }
            State::LongActive => {
                out.push(RecEvent::End(Activator::Long, t));
                State::Idle
            }
            State::DoubleActive | State::DoubleLate => {
                out.push(RecEvent::End(Activator::Double, t));
                State::Idle
            }
            State::P2 { t_r1 } => State::W2 { t_r1 },
            State::TripleActive => {
                out.push(RecEvent::End(Activator::Triple, t));
                State::Idle
            }
            State::Held | State::Dead => State::Idle,
            other => other,
        };
        // Release pulses must be delivered even if nothing else is pending.
        self.poll(t, cfg, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Activator::*;
    use RecEvent::{Begin, End};

    fn cfg(single: bool, long: bool, double: bool, triple: bool) -> RecConfig {
        RecConfig {
            single,
            long,
            double,
            triple,
            ..Default::default()
        }
    }

    /// Feed a timeline of `(time, pressed)` edges, then poll to `until`.
    fn run(c: RecConfig, edges: &[(u64, bool)], until: u64) -> Vec<RecEvent> {
        let mut r = Recognizer::default();
        let mut out = Vec::new();
        for &(t, down) in edges {
            if down {
                r.press(t, &c, &mut out);
            } else {
                r.release(t, &c, &mut out);
            }
        }
        r.poll(until, &c, &mut out);
        out
    }

    // Reference timelines (T_long 300, T_dbl 150, T_tpl 300, T_pulse 20).

    #[test]
    fn single_only_follows_button() {
        assert_eq!(
            run(
                cfg(true, false, false, false),
                &[(0, true), (80, false)],
                1000
            ),
            vec![Begin(Single, 0), End(Single, 80)]
        );
    }

    #[test]
    fn single_long_short_press_is_deferred_pulse() {
        assert_eq!(
            run(
                cfg(true, true, false, false),
                &[(0, true), (120, false)],
                1000
            ),
            vec![Begin(Single, 120), End(Single, 140)]
        );
    }

    #[test]
    fn single_long_long_press() {
        assert_eq!(
            run(
                cfg(true, true, false, false),
                &[(0, true), (500, false)],
                1000
            ),
            vec![Begin(Long, 300), End(Long, 500)]
        );
    }

    #[test]
    fn single_double_double_tap_holds() {
        assert_eq!(
            run(
                cfg(true, false, true, false),
                &[(0, true), (60, false), (150, true), (230, false)],
                1000
            ),
            vec![Begin(Double, 150), End(Double, 230)]
        );
    }

    #[test]
    fn single_double_one_tap_waits_window() {
        assert_eq!(
            run(
                cfg(true, false, true, false),
                &[(0, true), (60, false)],
                1000
            ),
            vec![Begin(Single, 210), End(Single, 230)]
        );
    }

    #[test]
    fn single_double_late_second_press_starts_new_sequence() {
        let c = cfg(true, false, true, false);
        let mut r = Recognizer::default();
        let mut out = Vec::new();
        r.press(0, &c, &mut out);
        r.release(60, &c, &mut out);
        r.press(220, &c, &mut out);
        assert_eq!(out, vec![Begin(Single, 210)]);
        r.poll(231, &c, &mut out);
        assert_eq!(out, vec![Begin(Single, 210), End(Single, 230)]);
        assert!(!r.is_idle(), "the press at 220 is a new sequence");
    }

    #[test]
    fn double_triple_two_taps_defer_double() {
        assert_eq!(
            run(
                cfg(false, false, true, true),
                &[(0, true), (60, false), (150, true), (200, false)],
                1000
            ),
            vec![Begin(Double, 360), End(Double, 380)]
        );
    }

    #[test]
    fn double_triple_three_taps() {
        assert_eq!(
            run(
                cfg(false, false, true, true),
                &[
                    (0, true),
                    (60, false),
                    (150, true),
                    (200, false),
                    (300, true),
                    (350, false)
                ],
                1000
            ),
            vec![Begin(Triple, 300), End(Triple, 350)]
        );
    }

    #[test]
    fn all_patterns_long_hold() {
        assert_eq!(
            run(
                cfg(true, true, true, true),
                &[(0, true), (400, false)],
                1000
            ),
            vec![Begin(Long, 300), End(Long, 400)]
        );
    }

    #[test]
    fn start_runs_in_parallel() {
        let c = RecConfig {
            start: true,
            ..cfg(true, false, true, false)
        };
        assert_eq!(
            run(c, &[(0, true), (60, false)], 1000),
            vec![
                Begin(Start, 0),
                End(Start, 60),
                Begin(Single, 210),
                End(Single, 230)
            ]
        );
    }

    #[test]
    fn start_has_minimum_length() {
        let c = RecConfig {
            start: true,
            single: false,
            ..Default::default()
        };
        assert_eq!(
            run(c, &[(0, true), (5, false)], 1000),
            vec![Begin(Start, 0), End(Start, 20)]
        );
    }

    #[test]
    fn release_pulse() {
        let c = RecConfig {
            release: true,
            single: false,
            ..Default::default()
        };
        assert_eq!(
            run(c, &[(0, true), (60, false)], 1000),
            vec![Begin(Release, 60), End(Release, 80)]
        );
    }

    #[test]
    fn second_press_held_past_triple_window_activates_double() {
        assert_eq!(
            run(
                cfg(false, false, true, true),
                &[(0, true), (60, false), (150, true), (500, false)],
                1000
            ),
            vec![Begin(Double, 360), End(Double, 500)]
        );
    }

    #[test]
    fn two_taps_without_double_mapping_emit_nothing() {
        assert_eq!(
            run(
                cfg(true, false, false, true),
                &[(0, true), (60, false), (150, true), (200, false)],
                1000
            ),
            vec![]
        );
    }
}
