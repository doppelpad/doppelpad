//! The remapping engine.
//!
//! [`Engine::tick`] is called at a fixed rate (the daemon uses ~1 kHz) with the
//! current physical gamepad state and the keyboard/mouse events since the last tick.
//! It returns what to send to the virtual pad, keyboard and mouse. The engine is
//! deterministic and does no I/O, so every behaviour is unit-testable with a fake clock.
//!
//! Pipeline per tick:
//! 1. derive digital controls from analog inputs (stick directions, trigger pulls, ...)
//! 2. resolve combos (shortcut masks) with a short pending window
//! 3. run press-pattern recognizers → activator `Begin`/`End` events
//! 4. run mode executors / macro programs → output events
//! 5. mirror unmapped physical controls to the virtual pad and merge outputs

mod analog;
mod compile;
mod gyro;
mod radial;
mod recognizer;
mod routes;
mod touch;

/// The stick and trigger shaping and the zone rule, which the editor mirrors to draw them.
pub use self::analog::{apply_curve, shape_stick, shape_trigger, stick_zone, trigger_zone};
pub use self::radial::RadialView;

use std::collections::{BTreeMap, HashMap, HashSet};

use self::analog::{
    DIRECTION_SECTOR, directions, ema_alpha, mix, orient, secs_since, sectors, take_toward_zero,
    zone_at,
};
use self::compile::{
    AnalogOut, BindingId, Compiled, JumpTarget, LayerTable, MaskId, Out, Policy, Step, StopRule,
    compile,
};
use self::gyro::{GyroAxes, GyroState};
use self::recognizer::{RecEvent, Recognizer};
use self::routes::{RouteInput, RouteOutput, RouteState};
use self::touch::GestureTracker;
use crate::control::{Control, Dir, Half, TouchGesture, TrackpadPart};
use crate::event::KbmEvent;
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{Motion, PadButton, PadState, Rumble, Stick, Trigger};
use crate::profile::{
    Activator, AnalogSource, Command, GyroMode, Mode, Profile, ShakeAxis, Sticks, TriggerEffect,
    Triggers, VirtualPadType, VirtualStick, VirtualSticks,
};

/// Button-driven mouse motion speeds up to at most this many times its speed.
const MOUSE_ACCEL_MAX: f32 = 20.0;
/// Steps one activation may run in one advance before it counts as a runaway program (a
/// loop without waits).
const MAX_STEPS_PER_ADVANCE: u32 = 100_000;
/// Commands one tick runs at most; more (commands that keep queueing commands) are dropped.
const MAX_COMMANDS_PER_TICK: usize = 256;
/// Time constant of the smoothed mouse speed that mouse-motion controls read, in seconds.
const MOUSE_VELOCITY_TAU_S: f32 = 0.03;
/// A shake flips direction this often.
const SHAKE_FLIP_MS: u64 = 25;

/// Where a virtual stick's jitter offset is and when it moves on.
#[derive(Clone, Copy, Debug, Default)]
struct JitterState {
    active: bool,
    point: usize,
    offset: (f32, f32),
    next_at: u64,
}

/// Small deterministic random generator (xorshift64*), so runs and tests repeat.
#[derive(Debug)]
struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Rng(0x9e37_79b9_7f4a_7c15)
    }
}

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `0..=n`.
    fn upto(&mut self, n: u64) -> u64 {
        self.next() % (n + 1)
    }
}

/// How long one wheel notch counts as "pressed" when used as a trigger.
const WHEEL_PULSE_MS: u64 = 50;

/// One engine tick's input.
pub struct Input<'a> {
    /// Monotonic time in milliseconds.
    pub now_ms: u64,
    /// Merged state of the physical gamepad(s); `None` if none is connected.
    pub pad: Option<&'a PadState>,
    /// Keyboard/mouse events since the previous tick, in order.
    pub kbm: &'a [KbmEvent],
}

/// One engine tick's output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// State for the virtual gamepad (`None` when the profile has no virtual pad).
    pub pad: Option<PadState>,
    /// Key transitions to inject, in order.
    pub keys: Vec<(Key, bool)>,
    pub mouse_buttons: Vec<(MouseButton, bool)>,
    /// Relative mouse motion in pixels (y down).
    pub mouse_move: (f32, f32),
    /// Move the cursor to this point of the primary screen (0–1) first.
    pub mouse_to: Option<(f32, f32)>,
    /// Keep the cursor inside this rectangle of the primary screen (0–1).
    pub mouse_clip: Option<[f32; 4]>,
    pub wheel: Vec<(WheelDir, f32)>,
    /// Rumble the mapping engine wants on the physical pad (overrides game rumble).
    pub rumble: Option<Rumble>,
    /// Commands for the host (launch app, remap off, ...).
    pub commands: Vec<Command>,
    pub layer: u8,
    /// The active layer changed this tick (mute set may differ).
    pub layer_changed: bool,
    /// The radial menu to draw.
    pub radial: Option<radial::RadialView>,
}

/// Keyboard/mouse inputs to swallow in the current layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mutes {
    pub keys: Vec<Key>,
    pub mouse_buttons: Vec<MouseButton>,
    pub wheel: Vec<WheelDir>,
    pub mouse_move: bool,
}

type ActId = u64;

#[derive(Debug)]
struct Activation {
    id: ActId,
    binding: BindingId,
    mask: MaskId,
    kind: Activator,
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
    owned: Vec<Out>,
    /// Analog outputs held by the program, with the time each one started.
    analog: Vec<(AnalogOut, u64)>,
    jump_at: Option<DelayedJump>,
    /// The program waits for the delayed jump (postponed actions).
    deferred: bool,
    /// Which segment of a sequence with break steps this activation plays.
    segment: usize,
    /// A wheel action this held activation repeats.
    wheel_held: Option<HeldWheel>,
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
struct HeldWheel {
    dir: WheelDir,
    notches: f32,
    since: u64,
    /// Repeats sent so far.
    sent: u64,
}

/// Rumble a mapping plays, until when.
#[derive(Clone, Copy, Debug)]
struct TimedRumble {
    rumble: Rumble,
    until: u64,
}

/// A turn action in progress, in degrees.
#[derive(Clone, Copy, Debug)]
struct Turn {
    total: f32,
    left: f32,
}

/// A gesture playing on the virtual touchpad.
#[derive(Clone, Copy, Debug)]
struct TouchPlay {
    gesture: TouchGesture,
    start: u64,
    ms: u64,
}

/// Reference-counted output state shared by all activations.
#[derive(Debug, Default)]
struct Outputs {
    counts: BTreeMap<Out, u32>,
    events: Vec<(Out, bool)>,
    wheel: Vec<(WheelDir, f32)>,
    commands: Vec<Command>,
    rumble: Option<TimedRumble>,
    /// Random numbers for jitter and random repeat waits.
    rng: Rng,
    /// Accelerometer shakes and when each started.
    shakes: Vec<(ShakeAxis, u64)>,
    /// Turn actions in progress.
    turns: Vec<Turn>,
    touch: Option<TouchPlay>,
    /// An absolute cursor move to send.
    mouse_to: Option<(f32, f32)>,
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

    fn release_all(&mut self, owned: &mut Vec<Out>) {
        while let Some(&o) = owned.last() {
            self.release(owned, o);
        }
    }

    #[cfg(test)]
    fn is_down(&self, o: Out) -> bool {
        self.counts.contains_key(&o)
    }
}

/// Whether any mapping or route condition of the profile uses a control matching `f`.
fn uses(p: &Profile, f: impl Fn(&Control) -> bool) -> bool {
    p.mappings.iter().any(|m| m.trigger.0.iter().any(&f))
        || p.layers
            .iter()
            .any(|l| l.not_inherited.iter().any(|c| c.0.iter().any(&f)))
        || p.analog.iter().any(|r| r.while_held.iter().any(&f))
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

#[derive(Debug, Default)]
struct WheelPulse {
    queued: u32,
    active_until: Option<u64>,
}

/// The stick, trigger and virtual stick settings of the layers that change them.
struct LayerSettings {
    analog: HashMap<u8, (Sticks, Triggers)>,
    virtual_sticks: HashMap<u8, VirtualSticks>,
}

impl LayerSettings {
    fn new(p: &Profile) -> LayerSettings {
        let analog = p
            .layers
            .iter()
            .filter(|l| l.sticks != Default::default() || l.triggers != Default::default())
            .map(|l| {
                let sticks = l.sticks.apply(&p.sticks);
                (l.id, (sticks, l.triggers.apply(&p.triggers)))
            })
            .collect();
        let virtual_sticks = p
            .layers
            .iter()
            .filter(|l| l.virtual_devices.sticks != Default::default())
            .map(|l| {
                (
                    l.id,
                    l.virtual_devices.sticks.apply(&p.virtual_devices.sticks),
                )
            })
            .collect();
        LayerSettings {
            analog,
            virtual_sticks,
        }
    }

    /// Whether `layer` has stick or trigger settings of its own.
    fn changes_analog(&self, layer: u8) -> bool {
        self.analog.contains_key(&layer)
    }

    /// Stick settings of `layer`.
    fn sticks<'a>(&'a self, base: &'a Profile, layer: u8) -> &'a Sticks {
        self.analog.get(&layer).map_or(&base.sticks, |(s, _)| s)
    }

    /// Trigger settings of `layer`.
    fn triggers<'a>(&'a self, base: &'a Profile, layer: u8) -> &'a Triggers {
        self.analog.get(&layer).map_or(&base.triggers, |(_, t)| t)
    }

    /// Virtual stick settings of `layer`.
    fn virtual_stick<'a>(&'a self, base: &'a Profile, layer: u8, s: Stick) -> &'a VirtualStick {
        self.virtual_sticks
            .get(&layer)
            .unwrap_or(&base.virtual_devices.sticks)
            .get(s)
    }
}

/// The remapping engine for one profile.
pub struct Engine {
    profile: Profile,
    c: Compiled,
    single_mask: HashMap<Control, MaskId>,
    layer: u8,
    now: u64,
    started: bool,

    pad: Option<PadState>,
    /// Keys and mouse buttons held down.
    kbm_down: HashSet<Control>,
    taps: Vec<Control>,
    wheel: [WheelPulse; 4],
    mouse_counts: (f32, f32),
    mouse_vel: (f32, f32),
    gyro_enabled: bool,
    gyro: GyroState,
    /// The gyro axes of this tick (tilt routes read them).
    gyro_axes: GyroAxes,
    /// The profile maps diagonal touchpad regions or mouse directions, so those sources
    /// tell eight directions apart.
    touch_diagonals: bool,
    /// Touchpad gesture recognition, and the gestures pressed until when.
    gestures: GestureTracker,
    gesture_until: Vec<(Control, u64)>,
    mouse_diagonals: bool,
    layer_settings: LayerSettings,
    /// Jitter offset of each virtual stick.
    jitter: [JitterState; 2],

    prev_controls: HashSet<Control>,
    cur_controls: HashSet<Control>,

    pending: Vec<(Control, u64)>,
    combo_deadline: Option<u64>,
    consumed: HashSet<Control>,
    fired: Vec<MaskId>,

    recs: HashMap<MaskId, Recognizer>,
    mask_down: HashSet<MaskId>,
    jump_owned: HashSet<MaskId>,
    /// Held across a layer change without re-press: they keep their old activations until
    /// released.
    stale: HashSet<MaskId>,
    radial: Option<radial::RadialState>,

    acts: Vec<Activation>,
    next_act: ActId,
    toggles: HashMap<BindingId, ActId>,
    /// Next segment to play, per binding whose sequence has break steps.
    segment_cursor: HashMap<BindingId, usize>,
    outs: Outputs,
    route_state: Vec<RouteState>,
    layer_changed: bool,
}

impl Engine {
    pub fn new(profile: Profile) -> Engine {
        let c = compile(&profile);
        for w in &c.warnings {
            tracing::warn!("{w}");
        }
        let single_mask = c
            .masks
            .iter()
            .enumerate()
            .filter(|(_, m)| m.len() == 1)
            .map(|(i, m)| (m[0], i))
            .collect();
        let route_state = vec![RouteState::default(); profile.analog.len()];
        let layer_settings = LayerSettings::new(&profile);
        let gyro_enabled = profile.gyro.initially_on;
        let touch_diagonals = uses(
            &profile,
            |c| matches!(c, Control::Touch(r) | Control::Trackpad(_, TrackpadPart::Region(r)) if r.is_diagonal()),
        );
        let mouse_diagonals = uses(
            &profile,
            |c| matches!(c, Control::MouseMove(d) | Control::MouseZone(Some(d), _) if d.is_diagonal()),
        );
        Engine {
            profile,
            c,
            single_mask,
            layer: 0,
            now: 0,
            started: false,
            pad: None,
            kbm_down: HashSet::new(),
            taps: Vec::new(),
            wheel: Default::default(),
            mouse_counts: (0.0, 0.0),
            mouse_vel: (0.0, 0.0),
            gyro_enabled,
            gyro: GyroState::default(),
            gyro_axes: GyroAxes::default(),
            touch_diagonals,
            gestures: GestureTracker::default(),
            gesture_until: Vec::new(),
            mouse_diagonals,
            layer_settings,
            jitter: Default::default(),
            prev_controls: HashSet::new(),
            cur_controls: HashSet::new(),
            pending: Vec::new(),
            combo_deadline: None,
            consumed: HashSet::new(),
            fired: Vec::new(),
            recs: HashMap::new(),
            mask_down: HashSet::new(),
            jump_owned: HashSet::new(),
            stale: HashSet::new(),
            radial: None,
            acts: Vec::new(),
            next_act: 0,
            toggles: HashMap::new(),
            segment_cursor: HashMap::new(),
            outs: Outputs::default(),
            route_state,
            layer_changed: false,
        }
    }

    /// Stick settings of the active layer.
    fn sticks(&self) -> &Sticks {
        self.layer_settings.sticks(&self.profile, self.layer)
    }

    /// Virtual stick settings of the active layer.
    fn virtual_stick(&self, s: Stick) -> &VirtualStick {
        self.layer_settings
            .virtual_stick(&self.profile, self.layer, s)
    }

    /// Trigger settings of the active layer.
    fn triggers(&self) -> &Triggers {
        self.layer_settings.triggers(&self.profile, self.layer)
    }

    /// The trigger effects of the active layer, left then right.
    pub fn trigger_effects(&self) -> [Option<TriggerEffect>; 2] {
        let t = self.triggers();
        [t.left.effect, t.right.effect]
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn layer(&self) -> u8 {
        self.layer
    }

    /// Whether a mapping is playing output right now (a macro runs, a toggle or a held
    /// mapping keeps something pressed).
    pub fn emulating(&self) -> bool {
        !self.acts.is_empty() || !self.toggles.is_empty()
    }

    pub fn warnings(&self) -> &[String] {
        &self.c.warnings
    }

    fn table(&self) -> &LayerTable {
        self.c.layer(self.layer)
    }

    /// Keyboard/mouse inputs that must not reach other applications in the current layer.
    pub fn mutes(&self) -> Mutes {
        let t = self.table();
        let mut m = Mutes::default();
        for c in &t.muted {
            match *c {
                Control::Key(k) => m.keys.push(k),
                Control::Mouse(b) => m.mouse_buttons.push(b),
                Control::Wheel(w) => m.wheel.push(w),
                Control::MouseMove(_) | Control::MouseZone(..) => m.mouse_move = true,
                _ => {}
            }
        }
        m.keys.sort();
        m.mouse_buttons.sort();
        m.wheel.sort();
        m
    }

    // ----- input bookkeeping -------------------------------------------------------

    fn apply_kbm(&mut self, events: &[KbmEvent]) {
        // Controls that went down during this tick; if they also go up again before the
        // tick ends, they become a press + release "tap" instead of being lost.
        let mut downs: HashSet<Control> = HashSet::new();
        for e in events {
            match *e {
                KbmEvent::Key { key, down } => self.kbm_edge(Control::Key(key), down, &mut downs),
                KbmEvent::MouseButton { button, down } => {
                    self.kbm_edge(Control::Mouse(button), down, &mut downs);
                }
                KbmEvent::MouseMove { dx, dy } => {
                    self.mouse_counts.0 += dx as f32;
                    self.mouse_counts.1 += dy as f32;
                }
                KbmEvent::Wheel { dir, detents } => {
                    self.wheel[dir as usize].queued += detents.round().max(1.0) as u32;
                }
            }
        }
        self.taps.sort();
        self.taps.dedup();
    }

    /// A key or mouse button went down or up; `downs` went down during this tick.
    fn kbm_edge(&mut self, c: Control, down: bool, downs: &mut HashSet<Control>) {
        if down {
            self.kbm_down.insert(c);
            downs.insert(c);
        } else {
            self.kbm_down.remove(&c);
            if downs.remove(&c) && !self.prev_controls.contains(&c) {
                self.taps.push(c);
            }
        }
    }

    /// Stick and trigger controls under the current layer's settings.
    fn analog_controls(&self, pad: &PadState, out: &mut impl Extend<Control>) {
        for s in Stick::ALL {
            let cfg = self.sticks().get(s);
            let v = orient(pad.stick(s), cfg);
            let Some(zone) = stick_zone(v, cfg.zone_thresholds()) else {
                continue;
            };
            out.extend([Control::StickZone(s, None, zone)]);
            let d = sectors(v, cfg.direction_ratio(), cfg.diagonals);
            for dir in Dir::ALL {
                if d[dir as usize] {
                    out.extend([
                        Control::StickDir(s, dir),
                        Control::StickZone(s, Some(dir), zone),
                    ]);
                }
            }
        }
        for t in Trigger::ALL {
            let cfg = self.triggers().get(t);
            let v = pad.axis(t.axis());
            if let Some(zone) = trigger_zone(v, cfg.zone_thresholds()) {
                out.extend([Control::TriggerPress(t), Control::TriggerZone(t, zone)]);
            }
            if v >= cfg.full_pull {
                out.extend([Control::TriggerFull(t)]);
            }
        }
    }

    /// This tick's controls, added to `set`.
    fn derive_controls(&mut self, now: u64, dt: f32, set: &mut HashSet<Control>) {
        if let Some(pad) = &self.pad {
            for b in PadButton::ALL {
                if pad.pressed(b) {
                    set.insert(Control::Button(b));
                }
            }
            self.analog_controls(pad, set);
            let cfg = &self.profile.touchpad;
            let clicked = pad.pressed(PadButton::Touchpad);
            let regions_on = !cfg.click_required || clicked;
            let mut regions = Vec::new();
            if cfg.split {
                for p in pad.touch.into_iter().flatten() {
                    let (half, at) = touch::half_of(p);
                    if regions_on {
                        regions.clear();
                        touch::regions(at, cfg, self.touch_diagonals, &mut regions);
                        set.extend(
                            regions
                                .iter()
                                .map(|r| Control::Trackpad(half, TrackpadPart::Region(*r))),
                        );
                    }
                    if clicked {
                        set.insert(Control::Trackpad(half, TrackpadPart::Click));
                    }
                }
            } else if let Some(p) = pad.touch[0]
                && regions_on
            {
                touch::regions((p.x, p.y), cfg, self.touch_diagonals, &mut regions);
                set.extend(regions.into_iter().map(Control::Touch));
            }
        }
        // Gestures: short presses after the tracker recognises them.
        if let Some(pad) = self.pad {
            let split = self.profile.touchpad.split;
            for c in self.gestures.update(pad.touch, now, split) {
                self.gesture_until.retain(|(g, _)| *g != c);
                self.gesture_until.push((c, now + touch::GESTURE_PRESS_MS));
            }
        }
        self.gesture_until.retain(|(_, until)| now < *until);
        set.extend(self.gesture_until.iter().map(|(c, _)| *c));
        self.gyro_axes = Default::default();
        if let Some(m) = self.pad.and_then(|p| p.motion) {
            // The tilt centre is taken even while the gyro is off.
            let axes = self.gyro.axes(&self.profile.gyro, &m, dt);
            self.gyro_axes = axes;
            if self.gyro_enabled {
                gyro::controls(&self.profile.gyro, axes, set);
            }
        }
        set.extend(self.kbm_down.iter().copied());

        for dir in [
            WheelDir::Up,
            WheelDir::Down,
            WheelDir::Left,
            WheelDir::Right,
        ] {
            let w = &mut self.wheel[dir as usize];
            match w.active_until {
                Some(t) if now < t => {
                    set.insert(Control::Wheel(dir));
                }
                Some(_) => w.active_until = None, // one tick released between notches
                None if w.queued > 0 => {
                    w.queued -= 1;
                    w.active_until = Some(now + WHEEL_PULSE_MS);
                    set.insert(Control::Wheel(dir));
                }
                None => {}
            }
        }

        if dt > 0.0 {
            let v = (self.mouse_counts.0 / dt, self.mouse_counts.1 / dt);
            self.mouse_vel = mix(self.mouse_vel, v, ema_alpha(dt, MOUSE_VELOCITY_TAU_S));
        }
        let (vx, vy) = self.mouse_vel;
        let [low, med, high] = self.profile.mouse.zones;
        let mut dirs = Vec::new();
        if self.mouse_diagonals {
            // Screen y grows downwards; directions are y up.
            let d = directions((vx, -vy), low, DIRECTION_SECTOR, true);
            dirs.extend(Dir::ALL.into_iter().filter(|d8| d[*d8 as usize]));
        } else {
            if vx > low {
                dirs.push(Dir::Right);
            } else if vx < -low {
                dirs.push(Dir::Left);
            }
            if vy > low {
                dirs.push(Dir::Down);
            } else if vy < -low {
                dirs.push(Dir::Up);
            }
        }
        // Zones by speed, in any direction and in each direction it moves.
        let speed = vx.hypot(vy);
        if let Some(z) = zone_at(speed, [low, med, high]) {
            set.insert(Control::MouseZone(None, z));
            set.extend(dirs.iter().map(|d| Control::MouseZone(Some(*d), z)));
        }
        set.extend(dirs.into_iter().map(Control::MouseMove));
    }

    // ----- combos ------------------------------------------------------------------

    fn combo_window(&self) -> u64 {
        u64::from(self.profile.timing.combo_window_ms)
    }

    fn on_press(&mut self, c: Control, t: u64) {
        if !self.consumed.contains(&c) && self.table().member_of.contains_key(&c) {
            self.pending.push((c, t));
            let window = self.combo_window();
            self.combo_deadline.get_or_insert(t + window);
            self.try_fire(t, false);
        } else {
            self.plain_press(c, t);
        }
    }

    fn on_release(&mut self, c: Control, t: u64) {
        if let Some(i) = self.pending.iter().position(|(p, _)| *p == c) {
            let (_, t0) = self.pending.remove(i);
            if self.pending.is_empty() {
                self.combo_deadline = None;
            }
            self.plain_press(c, t0);
            self.plain_release(c, t);
        } else if self.consumed.remove(&c) {
            let masks = &self.c.masks;
            let released: Vec<MaskId> = self
                .fired
                .iter()
                .copied()
                .filter(|s| masks[*s].contains(&c))
                .collect();
            self.fired.retain(|s| !released.contains(s));
            for s in released {
                self.mask_release(s, t);
            }
        } else {
            self.plain_release(c, t);
        }
    }

    fn try_fire(&mut self, now: u64, window_closed: bool) {
        loop {
            let table = self.c.layer(self.layer);
            let masks = &self.c.masks;
            let ready = |s: MaskId| {
                masks[s]
                    .iter()
                    .all(|c| self.cur_controls.contains(c) && !self.consumed.contains(c))
                    && masks[s]
                        .iter()
                        .any(|c| self.pending.iter().any(|(p, _)| p == c))
            };
            let best = table
                .shortcuts
                .iter()
                .copied()
                .filter(|&s| ready(s))
                .max_by(|&a, &b| masks[a].len().cmp(&masks[b].len()).then(b.cmp(&a)));
            let Some(s) = best else { break };
            if !window_closed && self.combo_deadline.is_some_and(|d| now <= d) {
                let members = &masks[s];
                let superset_possible = table.shortcuts.iter().any(|&o| {
                    masks[o].len() > members.len()
                        && members.iter().all(|c| masks[o].contains(c))
                        && masks[o].iter().any(|c| !self.cur_controls.contains(c))
                });
                if superset_possible {
                    break;
                }
            }
            let members = masks[s].clone();
            for c in members {
                if let Some(i) = self.pending.iter().position(|(p, _)| *p == c) {
                    self.pending.remove(i);
                } else {
                    // Already delivered as a plain press: end that first.
                    self.plain_release(c, now);
                }
                self.consumed.insert(c);
            }
            self.fired.push(s);
            self.mask_press(s, now);
        }
        if self.pending.is_empty() {
            self.combo_deadline = None;
        }
    }

    fn expire_combo(&mut self, now: u64) {
        let Some(deadline) = self.combo_deadline else {
            return;
        };
        if now <= deadline {
            return;
        }
        self.try_fire(deadline, true);
        for (c, t0) in std::mem::take(&mut self.pending) {
            self.plain_press(c, t0);
        }
        self.combo_deadline = None;
    }

    // ----- masks and recognizers -----------------------------------------------------

    /// Forget mask `m`'s press pattern without events.
    fn reset_rec(&mut self, m: MaskId) {
        if let Some(r) = self.recs.get_mut(&m) {
            r.reset();
        }
    }

    /// Forget mask `m`'s press pattern and end what it activated.
    fn drop_mask(&mut self, m: MaskId, t: u64) {
        self.reset_rec(m);
        self.end_mask_activations(m, t);
    }

    /// Feed mask `m`'s recognizer with `f` and handle the events it emits.
    fn feed_rec(&mut self, m: MaskId, f: impl FnOnce(&mut Recognizer, &mut Vec<RecEvent>)) {
        let mut ev = Vec::new();
        f(self.recs.entry(m).or_default(), &mut ev);
        self.handle_events(m, ev);
    }

    fn plain_press(&mut self, c: Control, t: u64) {
        if let Some(&m) = self.table().plain.get(&c) {
            self.mask_press(m, t);
        }
    }

    fn plain_release(&mut self, c: Control, t: u64) {
        if let Some(&m) = self.single_mask.get(&c) {
            self.mask_release(m, t);
        }
    }

    fn mask_press(&mut self, m: MaskId, t: u64) {
        self.mask_down.insert(m);
        if self.jump_owned.contains(&m) {
            return;
        }
        let Some(cfg) = self.table().rec.get(&m).copied() else {
            return;
        };
        self.feed_rec(m, |r, ev| r.press(t, &cfg, ev));
    }

    fn mask_release(&mut self, m: MaskId, t: u64) {
        let was_down = self.mask_down.remove(&m);
        if self.jump_owned.remove(&m) {
            self.end_mask_activations(m, t);
            if let Some(cfg) = self.table().rec.get(&m).copied()
                && cfg.release
            {
                self.feed_rec(m, |r, ev| {
                    r.reset();
                    r.release(t, &cfg, ev);
                });
            }
            return;
        }
        if self.stale.remove(&m) {
            self.drop_mask(m, t);
            return;
        }
        match self.table().rec.get(&m).copied() {
            Some(cfg) => self.feed_rec(m, |r, ev| r.release(t, &cfg, ev)),
            None if was_down => self.drop_mask(m, t),
            None => {}
        }
    }

    fn poll_recognizers(&mut self, now: u64) {
        let mut ids: Vec<MaskId> = self
            .recs
            .iter()
            .filter(|(_, r)| !r.is_idle())
            .map(|(m, _)| *m)
            .collect();
        ids.sort();
        for m in ids {
            let Some(cfg) = self.table().rec.get(&m).copied() else {
                self.reset_rec(m);
                continue;
            };
            self.feed_rec(m, |r, ev| r.poll(now, &cfg, ev));
        }
    }

    fn handle_events(&mut self, m: MaskId, mut events: Vec<RecEvent>) {
        events.sort_by_key(|e| match e {
            RecEvent::Begin(_, t) | RecEvent::End(_, t) => *t,
        });
        for e in events {
            match e {
                RecEvent::Begin(kind, t) => {
                    self.advance(t);
                    let bids: Vec<BindingId> = self
                        .table()
                        .bindings
                        .get(&m)
                        .map(|v| {
                            v.iter()
                                .copied()
                                .filter(|&b| self.c.bindings[b].activator == kind)
                                .collect()
                        })
                        .unwrap_or_default();
                    for b in bids {
                        self.begin(b, t);
                    }
                }
                RecEvent::End(kind, t) => {
                    self.advance(t);
                    self.end_activations_where(t, |a| a.mask == m && a.kind == kind);
                }
            }
        }
    }

    // ----- activations ---------------------------------------------------------------

    fn begin(&mut self, bid: BindingId, t: u64) {
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
    fn end_activations_where(&mut self, t: u64, which: impl Fn(&Activation) -> bool) {
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

    fn end_mask_activations(&mut self, m: MaskId, t: u64) {
        self.end_activations_where(t, |a| a.mask == m);
    }

    /// End toggle activation `id` even though it is latched.
    fn end_toggle(&mut self, id: ActId, t: u64) {
        if let Some(a) = self.acts.iter_mut().find(|a| a.id == id) {
            a.latched = false;
        }
        self.end_activation(id, t);
    }

    /// Run every activation's program up to `t` and fire due delayed jumps.
    fn advance(&mut self, t: u64) {
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

    // ----- layers --------------------------------------------------------------------

    fn switch_layer(&mut self, target: u8, owner: Option<MaskId>, t: u64) {
        if target == self.layer {
            return;
        }
        tracing::debug!(from = self.layer, to = target, "layer switch");
        // Segmented sequences start again from their first segment.
        self.segment_cursor.clear();
        // Toggle latches belong to the old layer; they end in the order they began.
        let mut latched: Vec<ActId> = self.toggles.drain().map(|(_, id)| id).collect();
        latched.sort_unstable();
        for id in latched {
            self.end_toggle(id, t);
        }
        let analog_differs = self.layer_settings.changes_analog(self.layer)
            || self.layer_settings.changes_analog(target);
        self.layer = target;
        self.layer_changed = true;
        // Stick and trigger controls follow the new layer's thresholds at once: what the
        // new layer does not report is released before anything is re-pressed (what it
        // newly reports is pressed on the next tick).
        if analog_differs && let Some(pad) = self.pad {
            let mut now_on = HashSet::new();
            self.analog_controls(&pad, &mut now_on);
            let mut off: Vec<Control> = self
                .cur_controls
                .iter()
                .copied()
                .filter(|c| {
                    matches!(
                        c,
                        Control::StickDir(..)
                            | Control::StickZone(..)
                            | Control::TriggerPress(_)
                            | Control::TriggerZone(..)
                            | Control::TriggerFull(_)
                    ) && !now_on.contains(c)
                })
                .collect();
            off.sort();
            for c in off {
                self.cur_controls.remove(&c);
                self.on_release(c, t);
            }
        }

        if let Some(m) = owner
            && self.mask_down.contains(&m)
        {
            self.jump_owned.insert(m);
            self.reset_rec(m);
        }
        // Everything else that is held is re-pressed under the new layer.
        let mut held: Vec<MaskId> = self
            .mask_down
            .iter()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        held.sort();
        let mut busy: Vec<MaskId> = self
            .recs
            .keys()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        busy.sort_unstable();
        let repress = self.profile.layer_change.repress;
        for m in busy {
            if !repress && held.contains(&m) {
                continue;
            }
            self.drop_mask(m, t);
        }
        if !repress {
            self.stale.extend(held);
            return;
        }
        for m in held {
            if self.table().rec.contains_key(&m) {
                self.mask_down.remove(&m);
                self.mask_press(m, t);
            }
        }
    }

    // ----- lifecycle -----------------------------------------------------------------

    /// Stop every macro, turbo and toggle and release every output (stop-macros and
    /// emergency-stop hotkeys, unapply).
    pub fn stop_all(&mut self) {
        for act in &mut self.acts {
            self.outs.release_all(&mut act.owned);
        }
        self.acts.clear();
        self.toggles.clear();
        self.segment_cursor.clear();
        for r in self.recs.values_mut() {
            r.reset();
        }
        self.pending.clear();
        self.combo_deadline = None;
        self.outs.rumble = None;
        self.outs.shakes.clear();
        self.outs.turns.clear();
        self.outs.touch = None;
    }

    /// Release everything and return the final output (call before unloading the profile).
    pub fn release_all(&mut self) -> Output {
        self.stop_all();
        let mut out = Output {
            layer: self.layer,
            ..Default::default()
        };
        self.drain_digital(&mut out);
        if self.profile.output.virtual_pad != VirtualPadType::None {
            out.pad = Some(PadState::default());
        }
        out
    }

    fn drain_digital(&mut self, out: &mut Output) {
        for (o, down) in self.outs.events.drain(..) {
            match o {
                Out::Key(k) => out.keys.push((k, down)),
                Out::Mouse(b) => out.mouse_buttons.push((b, down)),
                Out::Pad(_) => {}
            }
        }
    }

    /// Advance the engine to `input.now_ms`.
    pub fn tick(&mut self, input: Input) -> Output {
        let now = input.now_ms.max(self.now);
        let dt = if self.started {
            (now - self.now) as f32 / 1000.0
        } else {
            0.0
        };
        self.now = now;
        self.layer_changed = false;
        self.pad = input.pad.copied();
        self.mouse_counts = (0.0, 0.0);
        self.apply_kbm(input.kbm);

        // The set of two ticks ago, refilled.
        let mut controls = std::mem::take(&mut self.cur_controls);
        controls.clear();
        self.derive_controls(now, dt, &mut controls);
        if !self.started {
            // Inputs held when the profile is applied produce no press edges.
            self.prev_controls.clone_from(&controls);
            self.started = true;
        }
        self.cur_controls = controls;

        let mut released: Vec<Control> = self
            .prev_controls
            .difference(&self.cur_controls)
            .copied()
            .collect();
        let mut pressed: Vec<Control> = self
            .cur_controls
            .difference(&self.prev_controls)
            .copied()
            .collect();
        released.sort();
        pressed.sort();
        for c in released {
            self.on_release(c, now);
        }
        for c in pressed {
            self.on_press(c, now);
        }
        for c in std::mem::take(&mut self.taps) {
            if !self.cur_controls.contains(&c) {
                self.on_press(c, now);
                self.on_release(c, now);
            }
        }
        self.expire_combo(now);
        self.poll_recognizers(now);
        self.advance(now);
        self.update_radial(now);
        // Radial commands may play sectors whose own commands join the queue.
        let mut queue: std::collections::VecDeque<Command> =
            std::mem::take(&mut self.outs.commands).into();
        let mut done = Vec::new();
        while let Some(c) = queue.pop_front() {
            match c {
                Command::ToggleGyro => self.gyro_enabled = !self.gyro_enabled,
                Command::GyroOn => self.gyro_enabled = true,
                Command::GyroOff => self.gyro_enabled = false,
                Command::ResetTilt => self.gyro.reset(),
                Command::SwitchGyroAxis => self.gyro.switch_axis(),
                _ => self.radial_command(&c, now),
            }
            done.push(c);
            if done.len() < MAX_COMMANDS_PER_TICK {
                queue.extend(std::mem::take(&mut self.outs.commands));
            }
        }
        self.outs.commands = done;

        let mut out = self.compose(dt, now);
        out.radial = self
            .radial
            .as_ref()
            .and_then(|r| r.view(&self.profile.radial_menus, now));
        // The older set is refilled on the next tick.
        std::mem::swap(&mut self.prev_controls, &mut self.cur_controls);
        out
    }

    // ----- radial menus --------------------------------------------------------------

    /// Open the menu of the active layer, close the one whose layer ended (a release menu
    /// plays its pointed sector) and point with the menu's stick or the mouse.
    fn update_radial(&mut self, now: u64) {
        let layer = self.layer;
        let mut run = None;
        if let Some(r) = &self.radial
            && self
                .profile
                .radial_menus
                .get(r.root())
                .is_none_or(|m| m.layer != layer)
        {
            run = r.leave(&self.profile.radial_menus);
            self.radial = None;
        }
        if self.radial.is_none()
            && layer != 0
            && let Some(i) = self
                .profile
                .radial_menus
                .iter()
                .position(|m| m.layer == layer)
        {
            self.radial = Some(radial::RadialState::open(i, now));
        }
        let stick = self.radial_stick().and_then(|s| {
            let p = self.pad?;
            Some(analog::orient(p.stick(s), self.sticks().get(s)))
        });
        if let Some(r) = &mut self.radial {
            r.point(&self.profile.radial_menus, stick, self.mouse_counts);
        }
        if let Some(run) = run {
            self.run_radial(run, now);
        }
    }

    /// The stick an open menu points with; it does not reach the game meanwhile.
    fn radial_stick(&self) -> Option<Stick> {
        let r = self.radial.as_ref().filter(|r| !r.closed)?;
        self.profile.radial_menus.get(r.root())?.select.stick()
    }

    /// Run a radial menu command (others do nothing here).
    fn radial_command(&mut self, c: &Command, now: u64) {
        let Some(r) = &mut self.radial else {
            return;
        };
        let menus = &self.profile.radial_menus;
        let run = match c {
            Command::RadialNext => {
                r.step(menus, 1);
                None
            }
            Command::RadialPrev => {
                r.step(menus, -1);
                None
            }
            Command::RadialUp => {
                r.point_dir(menus, 0.0, 1.0);
                None
            }
            Command::RadialDown => {
                r.point_dir(menus, 0.0, -1.0);
                None
            }
            Command::RadialLeft => {
                r.point_dir(menus, -1.0, 0.0);
                None
            }
            Command::RadialRight => {
                r.point_dir(menus, 1.0, 0.0);
                None
            }
            Command::RadialApply => r.apply(menus),
            Command::RadialBack => {
                r.back();
                None
            }
            _ => None,
        };
        if let Some(run) = run {
            self.run_radial(run, now);
        }
    }

    /// Play a sector's actions once.
    fn run_radial(&mut self, run: radial::RadialRun, now: u64) {
        let Some(&bid) = self
            .c
            .radial
            .get(run.menu)
            .and_then(|ids| ids.get(run.sector))
        else {
            return;
        };
        self.begin(bid, now);
    }

    fn compose(&mut self, dt: f32, now: u64) -> Output {
        let mut out = Output {
            layer: self.layer,
            layer_changed: self.layer_changed,
            ..Default::default()
        };
        self.drain_digital(&mut out);
        // Held wheel actions repeat at the virtual mouse's interval.
        let every = u64::from(self.profile.virtual_devices.mouse.wheel_repeat_ms);
        for act in &mut self.acts {
            if let Some(w) = &mut act.wheel_held
                && let Some(due) = now.saturating_sub(w.since).checked_div(every)
            {
                while w.sent < due {
                    self.outs.wheel.push((w.dir, w.notches));
                    w.sent += 1;
                }
            }
        }
        out.wheel = std::mem::take(&mut self.outs.wheel);
        out.mouse_to = self.outs.mouse_to.take();
        out.commands = std::mem::take(&mut self.outs.commands);
        out.rumble = self.outs.rumble.filter(|r| r.until > now).map(|r| r.rumble);

        let table = self.c.layer(self.layer);
        let routes = &table.routes;
        let radial_stick = self.radial_stick();
        let route_consumes = |src: AnalogSource| {
            routes
                .iter()
                .any(|&i| self.profile.analog[i].source == src && self.profile.analog[i].consume)
        };

        // Mirror unmapped physical controls.
        let mut pad = PadState::default();
        if self.profile.output.passthrough
            && let Some(phys) = &self.pad
        {
            let hidden = |c: Control| {
                table.mapped.contains(&c)
                    || self.consumed.contains(&c)
                    || self.pending.iter().any(|(p, _)| *p == c)
            };
            // A split pad's clicks come from the touchpad button.
            let click_used = Half::ALL
                .iter()
                .any(|h| hidden(Control::Trackpad(*h, TrackpadPart::Click)));
            for b in PadButton::ALL {
                let used = hidden(Control::Button(b)) || (b == PadButton::Touchpad && click_used);
                if phys.pressed(b) && !used {
                    pad.set(b, true);
                }
            }
            for s in Stick::ALL {
                if Control::of_stick(s).any(hidden)
                    || route_consumes(AnalogSource::from(s))
                    || radial_stick == Some(s)
                {
                    continue;
                }
                let cfg = self.sticks().get(s);
                pad.set_stick(s, shape_stick(orient(phys.stick(s), cfg), cfg));
            }
            for t in Trigger::ALL {
                if Control::of_trigger(t).any(hidden) {
                    continue;
                }
                pad.set_axis(
                    t.axis(),
                    shape_trigger(phys.axis(t.axis()), self.triggers().get(t)),
                );
            }
            // A remapped touchpad or gyro no longer reaches the game, like a remapped stick.
            let touch_used =
                Control::of_touchpad().any(hidden) || route_consumes(AnalogSource::Touchpad);
            if !touch_used {
                pad.touch = phys.touch;
            }
            let gyro_used = Control::of_gyro().any(hidden) || route_consumes(AnalogSource::Gyro);
            if !gyro_used {
                pad.motion = phys.motion;
            }
        }

        // Mapping outputs.
        for o in self.outs.counts.keys() {
            if let Out::Pad(b) = o {
                pad.set(*b, true);
            }
        }
        let mut mouse_v = (0.0f32, 0.0f32);
        // Per stick and axis, the largest push in each direction (opposite pushes cancel).
        let mut pushes = [[[0.0f32; 2]; 2]; 2];
        // Axes that an absolute push sets: neither the physical stick nor routes reach them.
        let mut absolute = [[false; 2]; 2];
        for act in &self.acts {
            for &(a, since) in &act.analog {
                match a {
                    AnalogOut::Stick {
                        stick: s,
                        x,
                        y,
                        absolute: abs,
                    } => {
                        if abs {
                            let centre = x == 0.0 && y == 0.0;
                            absolute[s as usize][0] |= centre || x != 0.0;
                            absolute[s as usize][1] |= centre || y != 0.0;
                        }
                        let vs = self.virtual_stick(s);
                        let held = secs_since(now, since);
                        for (axis, v, m) in [(0, x, vs.multiplier_x), (1, y, vs.multiplier_y)] {
                            let reach = v.abs().min(vs.ramp(held, m));
                            let dir = usize::from(v < 0.0);
                            let p = &mut pushes[s as usize][axis][dir];
                            *p = p.max(reach);
                        }
                    }
                    AnalogOut::Trigger(t, v) => {
                        let cur = pad.axis(t.axis());
                        pad.set_axis(t.axis(), cur.max(v.clamp(0.0, 1.0)));
                    }
                    AnalogOut::MouseVelocity(x, y) => {
                        mouse_v.0 += x;
                        mouse_v.1 += y;
                    }
                    AnalogOut::Clip(x1, y1, x2, y2) => out.mouse_clip = Some([x1, y1, x2, y2]),
                    AnalogOut::MouseAccel(x, y, accel) => {
                        let held = secs_since(now, since);
                        let k = (1.0 + accel * held).min(MOUSE_ACCEL_MAX);
                        mouse_v.0 += x * k;
                        mouse_v.1 += y * k;
                    }
                }
            }
        }
        out.mouse_move = (mouse_v.0 * dt, mouse_v.1 * dt);
        // Turn actions: horizontal mouse motion spread over the turn time.
        let turn = &self.profile.virtual_devices.mouse;
        let secs = turn.turn_ms as f32 / 1000.0;
        for t in &mut self.outs.turns {
            let max_step = if secs > 0.0 {
                t.total.abs() * dt / secs
            } else {
                f32::INFINITY
            };
            let step = take_toward_zero(&mut t.left, max_step);
            out.mouse_move.0 += step * turn.counts_per_turn / 360.0;
        }
        self.outs.turns.retain(|t| t.left.abs() > 1e-4);
        for s in Stick::ALL {
            let axis = |[pos, neg]: [f32; 2]| {
                if pos > 0.0 && neg > 0.0 {
                    0.0
                } else {
                    pos - neg
                }
            };
            let [px, py] = pushes[s as usize];
            let [ax, ay] = absolute[s as usize];
            let (cx, cy) = pad.stick(s);
            let base = |c: f32, abs: bool| if abs { 0.0 } else { c };
            pad.set_stick(
                s,
                (
                    (base(cx, ax) + axis(px)).clamp(-1.0, 1.0),
                    (base(cy, ay) + axis(py)).clamp(-1.0, 1.0),
                ),
            );
        }

        // Analog routes.
        let input = RouteInput {
            pad: self.pad.as_ref(),
            sticks: self.layer_settings.sticks(&self.profile, self.layer),
            mouse: self.mouse_counts,
            dt,
            now,
            gyro_enabled: self.gyro_enabled,
            turn: &self.profile.virtual_devices.mouse,
            gyro_rates: self
                .pad
                .and_then(|p| p.motion)
                .map(|m| gyro::route_rates(&self.profile.gyro, &m)),
            tilt: (self.profile.gyro.mode == GyroMode::Tilt)
                .then(|| self.gyro_axes.tilt_deflection()),
        };
        for &i in routes {
            let r = &self.profile.analog[i];
            let pointing = radial_stick.is_some() && r.source.stick() == radial_stick;
            if pointing {
                self.route_state[i] = RouteState::default();
                continue;
            }
            if !r.while_held.iter().all(|c| self.cur_controls.contains(c)) {
                self.route_state[i] = RouteState::default();
                continue;
            }
            match routes::run(r, &mut self.route_state[i], &input) {
                RouteOutput::None => {}
                RouteOutput::Mouse(x, y) => {
                    out.mouse_move.0 += x;
                    out.mouse_move.1 += y;
                }
                RouteOutput::Stick(s, x, y) => {
                    let (cx, cy) = pad.stick(s);
                    let [ax, ay] = absolute[s as usize];
                    let add =
                        |c: f32, v: f32, abs: bool| if abs { c } else { (c + v).clamp(-1.0, 1.0) };
                    pad.set_stick(s, (add(cx, x, ax), add(cy, y, ay)));
                }
            }
        }
        // The virtual stick's own output curve, over every source.
        for s in Stick::ALL {
            let curve = &self.virtual_stick(s).curve;
            let (x, y) = pad.stick(s);
            let mag = x.hypot(y).min(1.0);
            if !curve.is_empty() && mag > 0.0 {
                let k = apply_curve(curve, mag) / mag;
                pad.set_stick(s, ((x * k).clamp(-1.0, 1.0), (y * k).clamp(-1.0, 1.0)));
            }
        }
        // Jitter: an offset that steps through its pattern after random delays.
        for s in Stick::ALL {
            let j = self.virtual_stick(s).jitter.clone();
            let st = &mut self.jitter[s as usize];
            if !j.enabled {
                *st = JitterState::default();
                continue;
            }
            if !st.active || now >= st.next_at {
                let points = j.pattern.points();
                st.point = if st.active {
                    (st.point + 1) % points.len()
                } else {
                    0
                };
                let size = j.min + (j.max - j.min).max(0.0) * self.outs.rng.unit();
                let (px, py) = points[st.point];
                st.offset = (px * size, py * size);
                let spread = u64::from(j.delay_max_ms.saturating_sub(j.delay_min_ms));
                st.next_at = now + u64::from(j.delay_min_ms.max(1)) + self.outs.rng.upto(spread);
                st.active = true;
            }
            let (x, y) = pad.stick(s);
            pad.set_stick(
                s,
                (
                    (x + st.offset.0).clamp(-1.0, 1.0),
                    (y + st.offset.1).clamp(-1.0, 1.0),
                ),
            );
        }
        // A touch action plays its fingers on the virtual touchpad.
        if let Some(p) = self.outs.touch {
            if now < p.start + p.ms {
                let t = now.saturating_sub(p.start) as f32 / p.ms.max(1) as f32;
                pad.touch = touch::output_fingers(p.gesture, t);
            } else {
                self.outs.touch = None;
            }
        }
        // Shakes: a square wave on one accelerometer axis, starting in its direction.
        let motion = &self.profile.virtual_devices.motion;
        let shake_ms = u64::from(motion.shake_ms);
        self.outs.shakes.retain(|(_, t0)| now < t0 + shake_ms);
        if !self.outs.shakes.is_empty() {
            let mut m = pad.motion.unwrap_or(Motion {
                gyro: [0.0; 3],
                accel: [0.0, 1.0, 0.0],
            });
            for &(axis, t0) in &self.outs.shakes {
                let (i, sign) = axis.axis();
                let flipped = (now - t0) / SHAKE_FLIP_MS % 2 == 1;
                m.accel[i] += if flipped { -sign } else { sign } * motion.shake_g;
            }
            pad.motion = Some(m);
        }

        if self.profile.output.virtual_pad != VirtualPadType::None {
            out.pad = Some(pad);
        }
        out
    }

    /// Whether a mapping holds `key` down right now.
    #[cfg(test)]
    pub(crate) fn is_output_down(&self, key: Key) -> bool {
        self.outs.is_down(Out::Key(key))
    }
}

#[cfg(test)]
mod tests;
