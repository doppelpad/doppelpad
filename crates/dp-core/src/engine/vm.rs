//! Activations: the programs mappings run, step by step, and the reference-counted outputs
//! they share.

use std::collections::BTreeMap;

use super::Engine;
use super::compile::{AnalogOut, BindingId, JumpTarget, MaskId, Out, Policy, Step, StopRule};
use crate::control::TouchGesture;
use crate::keys::WheelDir;
use crate::pad::Rumble;
use crate::profile::{Activator, Command, Mode, ShakeAxis};
use crate::rng::Rng;

/// Steps one activation may run in one advance before it counts as a runaway program (a
/// loop without waits).
const MAX_STEPS_PER_ADVANCE: u32 = 100_000;

pub(super) type ActId = u64;

#[derive(Debug)]
pub(super) struct Activation {
    id: ActId,
    binding: BindingId,
    pub(super) mask: MaskId,
    pub(super) kind: Activator,
    policy: Policy,
    pc: usize,
    wait_until: u64,
    run_start: u64,
    runs_done: u32,
    /// `End` received.
    ended: bool,
    /// Finish the current run, then stop.
    stopping: bool,
    /// Toggle latch: recognizer `End` events are ignored.
    latched: bool,
    finished: bool,
    pub(super) owned: Vec<Out>,
    /// Analog outputs held by the program, with the time each one started.
    pub(super) analog: Vec<(AnalogOut, u64)>,
    jump_at: Option<DelayedJump>,
    /// The program waits for the delayed jump (postponed actions).
    deferred: bool,
    /// Which segment of a sequence with break steps this activation plays.
    segment: usize,
    /// A wheel action this held activation repeats.
    pub(super) wheel_held: Option<HeldWheel>,
}

impl Activation {
    /// Release everything it holds and stop.
    fn finish(&mut self, outs: &mut Outputs) {
        outs.release_all(&mut self.owned);
        self.analog.clear();
        self.finished = true;
    }
}

/// A layer jump some time after the activation began.
#[derive(Clone, Copy, Debug)]
struct DelayedJump {
    at: u64,
    layer: u8,
}

/// A wheel action that a held activation repeats.
#[derive(Clone, Copy, Debug)]
pub(super) struct HeldWheel {
    pub(super) dir: WheelDir,
    pub(super) notches: f32,
    pub(super) since: u64,
    /// Repeats sent so far.
    pub(super) sent: u64,
}

/// Rumble a mapping plays, until when.
#[derive(Clone, Copy, Debug)]
pub(super) struct TimedRumble {
    pub(super) rumble: Rumble,
    pub(super) until: u64,
}

/// A turn action in progress, in degrees.
#[derive(Clone, Copy, Debug)]
pub(super) struct Turn {
    pub(super) total: f32,
    pub(super) left: f32,
}

/// A gesture playing on the virtual touchpad.
#[derive(Clone, Copy, Debug)]
pub(super) struct TouchPlay {
    pub(super) gesture: TouchGesture,
    pub(super) start: u64,
    pub(super) ms: u64,
}

/// Reference-counted output state shared by all activations.
#[derive(Debug, Default)]
pub(super) struct Outputs {
    pub(super) counts: BTreeMap<Out, u32>,
    pub(super) events: Vec<(Out, bool)>,
    pub(super) wheel: Vec<(WheelDir, f32)>,
    pub(super) commands: Vec<Command>,
    pub(super) rumble: Option<TimedRumble>,
    /// Random numbers for jitter and random repeat waits.
    pub(super) rng: Rng,
    /// Accelerometer shakes and when each started.
    pub(super) shakes: Vec<(ShakeAxis, u64)>,
    /// Turn actions in progress.
    pub(super) turns: Vec<Turn>,
    pub(super) touch: Option<TouchPlay>,
    /// An absolute cursor move to send.
    pub(super) mouse_to: Option<(f32, f32)>,
}

impl Outputs {
    fn press(&mut self, owned: &mut Vec<Out>, o: Out) {
        if owned.contains(&o) {
            return;
        }
        owned.push(o);
        let c = self.counts.entry(o).or_insert(0);
        *c += 1;
        if *c == 1 {
            self.events.push((o, true));
        }
    }

    fn release(&mut self, owned: &mut Vec<Out>, o: Out) {
        let Some(i) = owned.iter().position(|x| *x == o) else {
            return;
        };
        owned.remove(i);
        if let Some(c) = self.counts.get_mut(&o) {
            *c -= 1;
            if *c == 0 {
                self.counts.remove(&o);
                self.events.push((o, false));
            }
        }
    }

    pub(super) fn release_all(&mut self, owned: &mut Vec<Out>) {
        while let Some(&o) = owned.last() {
            self.release(owned, o);
        }
    }

    #[cfg(test)]
    pub(super) fn is_down(&self, o: Out) -> bool {
        self.counts.contains_key(&o)
    }
}

/// Run an activation's program up to `until`.
fn step(act: &mut Activation, program: &[Step], outs: &mut Outputs, until: u64) {
    let mut guard = 0;
    while !act.finished && act.wait_until <= until {
        guard += 1;
        if guard > MAX_STEPS_PER_ADVANCE {
            act.finished = true; // runaway program (e.g. zero-length loop)
            break;
        }
        let t = act.wait_until;
        if act.pc >= program.len() {
            outs.release_all(&mut act.owned);
            act.analog.clear();
            match act.policy {
                Policy::Hold => {
                    act.finished = true;
                }
                Policy::Loop {
                    max_runs,
                    stop,
                    gap_ms,
                    period_ms,
                    random_ms,
                } => {
                    act.runs_done += 1;
                    let more = !act.stopping
                        && max_runs.is_none_or(|m| act.runs_done < m)
                        && !(act.ended && stop == StopRule::Immediate);
                    if !more {
                        act.finished = true;
                        break;
                    }
                    let extra = if random_ms > 0 {
                        outs.rng.upto(u64::from(random_ms))
                    } else {
                        0
                    };
                    let next = match period_ms {
                        Some(p) => (act.run_start + u64::from(p) + extra).max(t),
                        None => t + u64::from(gap_ms) + extra,
                    };
                    act.run_start = next;
                    act.pc = 0;
                    act.wait_until = next.max(t + if program.is_empty() { 1 } else { 0 });
                }
            }
            continue;
        }
        match &program[act.pc] {
            Step::Down(o) => outs.press(&mut act.owned, *o),
            Step::Up(o) => outs.release(&mut act.owned, *o),
            Step::AnalogDown(a) => {
                act.analog.retain(|(x, _)| !x.same_slot(a));
                act.analog.push((*a, t));
            }
            Step::AnalogUp(a) => act.analog.retain(|(x, _)| !x.same_slot(a)),
            Step::Wheel(d, n) => {
                outs.wheel.push((*d, *n));
                if act.policy == Policy::Hold {
                    act.wheel_held = Some(HeldWheel {
                        dir: *d,
                        notches: *n,
                        since: t,
                        sent: 0,
                    });
                }
            }
            Step::Wait(ms) => {
                act.pc += 1;
                act.wait_until = t + u64::from(*ms);
                continue;
            }
            Step::HoldPoint => {
                if act.ended {
                    act.finish(outs);
                }
                break;
            }
            Step::Rumble { ms, low, high } => {
                outs.rumble = Some(TimedRumble {
                    rumble: Rumble {
                        low: *low,
                        high: *high,
                    },
                    until: t + u64::from(*ms),
                });
            }
            Step::Command(c) => outs.commands.push(c.clone()),
            Step::Shake(axis) => {
                outs.shakes.retain(|(a, _)| a != axis);
                outs.shakes.push((*axis, t));
            }
            Step::Turn(deg) => outs.turns.push(Turn {
                total: *deg,
                left: *deg,
            }),
            Step::Touch(g, ms) => {
                outs.touch = Some(TouchPlay {
                    gesture: *g,
                    start: t,
                    ms: u64::from(*ms),
                });
            }
            Step::MouseTo(x, y) => outs.mouse_to = Some((*x, *y)),
        }
        act.pc += 1;
    }
}

impl Engine {
    pub(super) fn begin(&mut self, bid: BindingId, t: u64) {
        let b = &self.c.bindings[bid];
        let (m, kind) = (b.mask, b.activator);
        let policy = b.policy;
        let mode = b.mode;
        let jump = b.jump;
        if mode == Mode::Toggle {
            if let Some(id) = self.toggles.remove(&bid)
                && self.acts.iter().any(|a| a.id == id && !a.finished)
            {
                self.end_toggle(id, t);
                return;
            }
        } else if matches!(
            policy,
            Policy::Loop {
                stop: StopRule::Never,
                ..
            }
        ) && self.acts.iter().any(|a| a.binding == bid && !a.finished)
        {
            return; // no re-entry while a one-shot sequence runs
        }
        let id = self.next_act;
        self.next_act += 1;
        let segments = self.c.bindings[bid].segments();
        let segment = if segments > 1 {
            let next = self.segment_cursor.entry(bid).or_insert(0);
            let s = *next % segments;
            *next = (s + 1) % segments;
            s
        } else {
            0
        };
        let mut act = Activation {
            id,
            binding: bid,
            mask: m,
            kind,
            policy,
            pc: 0,
            wait_until: t,
            run_start: t,
            runs_done: 0,
            ended: false,
            stopping: false,
            latched: mode == Mode::Toggle,
            finished: false,
            owned: Vec::new(),
            analog: Vec::new(),
            jump_at: None,
            deferred: false,
            segment,
            wheel_held: None,
        };
        if let Some(JumpTarget {
            layer,
            delay_ms: Some(delay),
            postpone,
        }) = jump
            && delay > 0
        {
            let at = t + u64::from(delay);
            act.jump_at = Some(DelayedJump { at, layer });
            if postpone {
                act.deferred = true;
                act.wait_until = at;
                act.run_start = at;
            }
        }
        step(
            &mut act,
            self.c.bindings[bid].program(segment),
            &mut self.outs,
            t,
        );
        self.acts.push(act);
        if mode == Mode::Toggle {
            self.toggles.insert(bid, id);
        }
        if let Some(j) = jump
            && j.delay_ms.is_none_or(|d| d == 0)
        {
            self.switch_layer(j.layer, Some(m), t);
        }
    }

    fn end_activation(&mut self, id: ActId, t: u64) {
        let Some(i) = self.acts.iter().position(|a| a.id == id) else {
            return;
        };
        let program = self.c.bindings[self.acts[i].binding].program(self.acts[i].segment);
        let act = &mut self.acts[i];
        step(act, program, &mut self.outs, t);
        act.ended = true;
        act.jump_at = None;
        if act.deferred {
            // Released before the delayed jump: neither the jump nor the actions happen.
            act.finished = true;
            return;
        }
        match act.policy {
            Policy::Hold
            | Policy::Loop {
                stop: StopRule::Immediate,
                ..
            } => act.finish(&mut self.outs),
            Policy::Loop {
                stop: StopRule::FinishRun,
                ..
            } => act.stopping = true,
            Policy::Loop {
                stop: StopRule::Never,
                ..
            } => {}
        }
    }

    /// End the running activations that `which` picks (latched toggles keep running).
    pub(super) fn end_activations_where(&mut self, t: u64, which: impl Fn(&Activation) -> bool) {
        let ids: Vec<ActId> = self
            .acts
            .iter()
            .filter(|a| !a.ended && !a.latched && which(a))
            .map(|a| a.id)
            .collect();
        for id in ids {
            self.end_activation(id, t);
        }
    }

    pub(super) fn end_mask_activations(&mut self, m: MaskId, t: u64) {
        self.end_activations_where(t, |a| a.mask == m);
    }

    /// End toggle activation `id` even though it is latched.
    pub(super) fn end_toggle(&mut self, id: ActId, t: u64) {
        if let Some(a) = self.acts.iter_mut().find(|a| a.id == id) {
            a.latched = false;
        }
        self.end_activation(id, t);
    }

    /// Run every activation's program up to `t` and fire due delayed jumps.
    pub(super) fn advance(&mut self, t: u64) {
        let mut jumps = Vec::new();
        for act in &mut self.acts {
            let program = self.c.bindings[act.binding].program(act.segment);
            step(act, program, &mut self.outs, t);
            if let Some(j) = act.jump_at
                && j.at <= t
            {
                act.jump_at = None;
                act.deferred = false;
                if !act.ended {
                    jumps.push((j, act.mask));
                }
            }
        }
        self.acts.retain(|a| !a.finished);
        for (j, mask) in jumps {
            self.switch_layer(j.layer, Some(mask), j.at);
        }
    }
}
