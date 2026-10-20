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

pub mod analog;
pub mod compile;
pub mod recognizer;
pub mod routes;

use std::collections::{BTreeMap, HashMap, HashSet};

use self::analog::{
    DIRECTION_SECTOR, directions, orient, sectors, shape_stick, shape_trigger, stick_zone,
    trigger_zone,
};
use self::compile::{
    AnalogOut, BindingId, Compiled, LayerTable, MaskId, Out, Policy, Step, StopRule, compile,
};
use self::recognizer::{RecEvent, Recognizer};
use self::routes::{RouteInput, RouteOutput, RouteState};
use crate::control::{Control, Dir, TouchRegion};
use crate::event::KbmEvent;
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{Axis, PadButton, PadState, Rumble, Stick, Trigger};
use crate::profile::{Activator, AnalogSource, Command, Mode, Profile, VirtualPadType};

/// How long one wheel notch counts as "pressed" when used as a trigger.
const WHEEL_PULSE_MS: u64 = 50;
/// Mouse speed (counts/s) above which `MouseMove:*` controls are active.
const MOUSE_DIR_THRESHOLD: f32 = 200.0;
/// Gyro rate (°/s) above which `Gyro*` controls are active.
const GYRO_DIR_THRESHOLD: f32 = 60.0;
/// Touchpad: distance from the centre (fraction of half-size) before a direction region counts.
const TOUCH_CENTER_RADIUS: f32 = 0.35;

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
    pub wheel: Vec<(WheelDir, f32)>,
    /// Rumble the mapping engine wants on the physical pad (overrides game rumble).
    pub rumble: Option<Rumble>,
    /// Commands for the host (launch app, remap off, ...).
    pub commands: Vec<Command>,
    pub layer: u8,
    /// The active layer changed this tick (mute set may differ).
    pub layer_changed: bool,
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
    analog: Vec<AnalogOut>,
    jump_at: Option<(u64, u8)>,
    /// Which segment of a sequence with break steps this activation plays.
    segment: usize,
}

/// Reference-counted output state shared by all activations.
#[derive(Debug, Default)]
struct Outputs {
    counts: BTreeMap<Out, u32>,
    events: Vec<(Out, bool)>,
    wheel: Vec<(WheelDir, f32)>,
    commands: Vec<Command>,
    rumble: Option<(Rumble, u64)>,
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

    fn is_down(&self, o: Out) -> bool {
        self.counts.contains_key(&o)
    }
}

/// Run an activation's program up to `until`.
fn step(act: &mut Activation, program: &[Step], outs: &mut Outputs, until: u64) {
    let mut guard = 0;
    while !act.finished && act.wait_until <= until {
        guard += 1;
        if guard > 100_000 {
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
                } => {
                    act.runs_done += 1;
                    let more = !act.stopping
                        && max_runs.is_none_or(|m| act.runs_done < m)
                        && !(act.ended && stop == StopRule::Immediate);
                    if !more {
                        act.finished = true;
                        break;
                    }
                    let next = match period_ms {
                        Some(p) => (act.run_start + p as u64).max(t),
                        None => t + gap_ms as u64,
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
                act.analog.retain(|x| !x.same_slot(a));
                act.analog.push(*a);
            }
            Step::AnalogUp(a) => act.analog.retain(|x| !x.same_slot(a)),
            Step::Wheel(d, n) => outs.wheel.push((*d, *n)),
            Step::Wait(ms) => {
                act.pc += 1;
                act.wait_until = t + *ms as u64;
                continue;
            }
            Step::HoldPoint => {
                if act.ended {
                    outs.release_all(&mut act.owned);
                    act.analog.clear();
                    act.finished = true;
                }
                break;
            }
            Step::Rumble { ms, low, high } => {
                outs.rumble = Some((
                    Rumble {
                        low: *low,
                        high: *high,
                    },
                    t + *ms as u64,
                ))
            }
            Step::Command(c) => outs.commands.push(c.clone()),
        }
        act.pc += 1;
    }
}

#[derive(Debug, Default)]
struct WheelPulse {
    queued: u32,
    active_until: Option<u64>,
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
    keys_down: HashSet<Key>,
    mouse_down: HashSet<MouseButton>,
    taps: Vec<Control>,
    wheel: [WheelPulse; 4],
    mouse_counts: (f32, f32),
    mouse_vel: (f32, f32),
    gyro_enabled: bool,

    prev_controls: HashSet<Control>,
    cur_controls: HashSet<Control>,

    pending: Vec<(Control, u64)>,
    combo_deadline: Option<u64>,
    consumed: HashSet<Control>,
    fired: Vec<MaskId>,

    recs: HashMap<MaskId, Recognizer>,
    mask_down: HashSet<MaskId>,
    jump_owned: HashSet<MaskId>,

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
        let route_state = vec![RouteState::default(); c.routes.len()];
        Engine {
            profile,
            c,
            single_mask,
            layer: 0,
            now: 0,
            started: false,
            pad: None,
            keys_down: HashSet::new(),
            mouse_down: HashSet::new(),
            taps: Vec::new(),
            wheel: Default::default(),
            mouse_counts: (0.0, 0.0),
            mouse_vel: (0.0, 0.0),
            gyro_enabled: true,
            prev_controls: HashSet::new(),
            cur_controls: HashSet::new(),
            pending: Vec::new(),
            combo_deadline: None,
            consumed: HashSet::new(),
            fired: Vec::new(),
            recs: HashMap::new(),
            mask_down: HashSet::new(),
            jump_owned: HashSet::new(),
            acts: Vec::new(),
            next_act: 0,
            toggles: HashMap::new(),
            segment_cursor: HashMap::new(),
            outs: Outputs::default(),
            route_state,
            layer_changed: false,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn layer(&self) -> u8 {
        self.layer
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
                Control::MouseMove(_) => m.mouse_move = true,
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
                KbmEvent::Key { key, down } => {
                    let c = Control::Key(key);
                    if down {
                        self.keys_down.insert(key);
                        downs.insert(c);
                    } else {
                        self.keys_down.remove(&key);
                        if downs.remove(&c) && !self.prev_controls.contains(&c) {
                            self.taps.push(c);
                        }
                    }
                }
                KbmEvent::MouseButton { button, down } => {
                    let c = Control::Mouse(button);
                    if down {
                        self.mouse_down.insert(button);
                        downs.insert(c);
                    } else {
                        self.mouse_down.remove(&button);
                        if downs.remove(&c) && !self.prev_controls.contains(&c) {
                            self.taps.push(c);
                        }
                    }
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

    fn derive_controls(&mut self, now: u64, dt: f32) -> HashSet<Control> {
        let mut set = HashSet::new();
        if let Some(pad) = &self.pad {
            for b in PadButton::ALL {
                if pad.pressed(b) {
                    set.insert(Control::Button(b));
                }
            }
            for s in [Stick::Left, Stick::Right] {
                let cfg = self.profile.sticks.get(s);
                let v = orient(pad.stick(s), cfg);
                let Some(zone) = stick_zone(v, cfg.zone_thresholds()) else {
                    continue;
                };
                set.insert(Control::StickZone(s, None, zone));
                let d = sectors(v, DIRECTION_SECTOR);
                for dir in Dir::ALL {
                    if d[dir as usize] {
                        set.insert(Control::StickDir(s, dir));
                        set.insert(Control::StickZone(s, Some(dir), zone));
                    }
                }
            }
            for t in [Trigger::Left, Trigger::Right] {
                let cfg = self.profile.triggers.get(t);
                let v = pad.axis(t.axis());
                if let Some(zone) = trigger_zone(v, cfg.zone_thresholds()) {
                    set.insert(Control::TriggerPress(t));
                    set.insert(Control::TriggerZone(t, zone));
                }
                if v >= cfg.full_pull {
                    set.insert(Control::TriggerFull(t));
                }
            }
            if let Some(tp) = pad.touch[0] {
                let v = ((tp.x - 0.5) * 2.0, (0.5 - tp.y) * 2.0);
                let d = directions(v, TOUCH_CENTER_RADIUS, 1.0);
                let mut any = false;
                for (dir, region) in [
                    (Dir::Up, TouchRegion::Up),
                    (Dir::Down, TouchRegion::Down),
                    (Dir::Left, TouchRegion::Left),
                    (Dir::Right, TouchRegion::Right),
                ] {
                    if d[dir as usize] {
                        set.insert(Control::Touch(region));
                        any = true;
                    }
                }
                if !any {
                    set.insert(Control::Touch(TouchRegion::Center));
                }
            }
            if let Some(m) = pad.motion
                && self.gyro_enabled
            {
                let (pitch, yaw) = (m.gyro[0], m.gyro[1]);
                if pitch > GYRO_DIR_THRESHOLD {
                    set.insert(Control::Gyro(Dir::Up));
                } else if pitch < -GYRO_DIR_THRESHOLD {
                    set.insert(Control::Gyro(Dir::Down));
                }
                if yaw > GYRO_DIR_THRESHOLD {
                    set.insert(Control::Gyro(Dir::Left));
                } else if yaw < -GYRO_DIR_THRESHOLD {
                    set.insert(Control::Gyro(Dir::Right));
                }
            }
        }
        set.extend(self.keys_down.iter().map(|k| Control::Key(*k)));
        set.extend(self.mouse_down.iter().map(|b| Control::Mouse(*b)));

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
            let a = 1.0 - (-dt / 0.03).exp();
            let v = (self.mouse_counts.0 / dt, self.mouse_counts.1 / dt);
            self.mouse_vel = (
                self.mouse_vel.0 + (v.0 - self.mouse_vel.0) * a,
                self.mouse_vel.1 + (v.1 - self.mouse_vel.1) * a,
            );
        }
        let (vx, vy) = self.mouse_vel;
        if vx > MOUSE_DIR_THRESHOLD {
            set.insert(Control::MouseMove(Dir::Right));
        } else if vx < -MOUSE_DIR_THRESHOLD {
            set.insert(Control::MouseMove(Dir::Left));
        }
        if vy > MOUSE_DIR_THRESHOLD {
            set.insert(Control::MouseMove(Dir::Down));
        } else if vy < -MOUSE_DIR_THRESHOLD {
            set.insert(Control::MouseMove(Dir::Up));
        }
        set
    }

    // ----- combos ------------------------------------------------------------------

    fn combo_window(&self) -> u64 {
        self.profile.timing.combo_window_ms as u64
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
        let mut ev = Vec::new();
        self.recs.entry(m).or_default().press(t, &cfg, &mut ev);
        self.handle_events(m, ev);
    }

    fn mask_release(&mut self, m: MaskId, t: u64) {
        let was_down = self.mask_down.remove(&m);
        if self.jump_owned.remove(&m) {
            self.end_mask_activations(m, t);
            if let Some(cfg) = self.table().rec.get(&m).copied()
                && cfg.release
            {
                let rec = self.recs.entry(m).or_default();
                rec.reset();
                let mut ev = Vec::new();
                rec.release(t, &cfg, &mut ev);
                self.handle_events(m, ev);
            }
            return;
        }
        match self.table().rec.get(&m).copied() {
            Some(cfg) => {
                let mut ev = Vec::new();
                self.recs.entry(m).or_default().release(t, &cfg, &mut ev);
                self.handle_events(m, ev);
            }
            None if was_down => {
                if let Some(r) = self.recs.get_mut(&m) {
                    r.reset();
                }
                self.end_mask_activations(m, t);
            }
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
                if let Some(r) = self.recs.get_mut(&m) {
                    r.reset();
                }
                continue;
            };
            let mut ev = Vec::new();
            if let Some(r) = self.recs.get_mut(&m) {
                r.poll(now, &cfg, &mut ev);
            }
            self.handle_events(m, ev);
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
                        self.begin(b, m, kind, t);
                    }
                }
                RecEvent::End(kind, t) => {
                    self.advance(t);
                    let ids: Vec<ActId> = self
                        .acts
                        .iter()
                        .filter(|a| a.mask == m && a.kind == kind && !a.ended && !a.latched)
                        .map(|a| a.id)
                        .collect();
                    for id in ids {
                        self.end_activation(id, t);
                    }
                }
            }
        }
    }

    // ----- activations ---------------------------------------------------------------

    fn begin(&mut self, bid: BindingId, m: MaskId, kind: Activator, t: u64) {
        let b = &self.c.bindings[bid];
        let policy = b.policy();
        let mode = b.mode;
        let jump = b.jump;
        if mode == Mode::Toggle {
            if let Some(id) = self.toggles.remove(&bid)
                && self.acts.iter().any(|a| a.id == id && !a.finished)
            {
                if let Some(a) = self.acts.iter_mut().find(|a| a.id == id) {
                    a.latched = false;
                }
                self.end_activation(id, t);
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
            segment,
        };
        if let Some((target, delay)) = jump
            && delay.is_some_and(|d| d > 0)
        {
            act.jump_at = Some((t + delay.unwrap_or(0) as u64, target));
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
        if let Some((target, delay)) = jump
            && delay.is_none_or(|d| d == 0)
        {
            self.switch_layer(target, Some(m), t);
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
        match act.policy {
            Policy::Hold
            | Policy::Loop {
                stop: StopRule::Immediate,
                ..
            } => {
                self.outs.release_all(&mut act.owned);
                act.analog.clear();
                act.finished = true;
            }
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

    fn end_mask_activations(&mut self, m: MaskId, t: u64) {
        let ids: Vec<ActId> = self
            .acts
            .iter()
            .filter(|a| a.mask == m && !a.ended && !a.latched)
            .map(|a| a.id)
            .collect();
        for id in ids {
            self.end_activation(id, t);
        }
    }

    /// Run every activation's program up to `t` and fire due delayed jumps.
    fn advance(&mut self, t: u64) {
        let mut jumps = Vec::new();
        for act in &mut self.acts {
            let program = self.c.bindings[act.binding].program(act.segment);
            step(act, program, &mut self.outs, t);
            if let Some((at, target)) = act.jump_at
                && at <= t
            {
                act.jump_at = None;
                if !act.ended {
                    jumps.push((target, act.mask, at));
                }
            }
        }
        self.acts.retain(|a| !a.finished);
        for (target, mask, at) in jumps {
            self.switch_layer(target, Some(mask), at);
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
        // Toggle latches belong to the old layer.
        let latched: Vec<ActId> = self.toggles.drain().map(|(_, id)| id).collect();
        for id in latched {
            if let Some(a) = self.acts.iter_mut().find(|a| a.id == id) {
                a.latched = false;
            }
            self.end_activation(id, t);
        }
        self.layer = target;
        self.layer_changed = true;

        if let Some(m) = owner
            && self.mask_down.contains(&m)
        {
            self.jump_owned.insert(m);
            if let Some(r) = self.recs.get_mut(&m) {
                r.reset();
            }
        }
        // Everything else that is held is re-pressed under the new layer.
        let mut held: Vec<MaskId> = self
            .mask_down
            .iter()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        held.sort();
        let busy: Vec<MaskId> = self
            .recs
            .keys()
            .copied()
            .filter(|m| Some(*m) != owner && !self.jump_owned.contains(m))
            .collect();
        for m in busy {
            if let Some(r) = self.recs.get_mut(&m) {
                r.reset();
            }
            self.end_mask_activations(m, t);
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

        let controls = self.derive_controls(now, dt);
        if !self.started {
            // Inputs held when the profile is applied produce no press edges.
            self.prev_controls = controls.clone();
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
        for c in std::mem::take(&mut self.outs.commands) {
            match c {
                Command::ToggleGyro => self.gyro_enabled = !self.gyro_enabled,
                Command::GyroOn => self.gyro_enabled = true,
                Command::GyroOff => self.gyro_enabled = false,
                _ => {}
            }
            self.outs.commands.push(c);
        }

        let out = self.compose(dt, now);
        self.prev_controls = self.cur_controls.clone();
        out
    }

    fn compose(&mut self, dt: f32, now: u64) -> Output {
        let mut out = Output {
            layer: self.layer,
            layer_changed: self.layer_changed,
            ..Default::default()
        };
        self.drain_digital(&mut out);
        out.wheel = std::mem::take(&mut self.outs.wheel);
        out.commands = std::mem::take(&mut self.outs.commands);
        out.rumble = self
            .outs
            .rumble
            .filter(|(_, until)| *until > now)
            .map(|(r, _)| r);

        let table = self.c.layer(self.layer);
        let routes: Vec<usize> = table.routes.clone();
        let route_consumes = |src: AnalogSource| {
            routes
                .iter()
                .any(|&i| self.c.routes[i].source == src && self.c.routes[i].consume)
        };

        // Mirror unmapped physical controls.
        let mut pad = PadState::default();
        if self.profile.output.passthrough
            && let Some(phys) = &self.pad
        {
            let hidden = |c: &Control| {
                table.mapped.contains(c)
                    || self.consumed.contains(c)
                    || self.pending.iter().any(|(p, _)| p == c)
            };
            for b in PadButton::ALL {
                if phys.pressed(b) && !hidden(&Control::Button(b)) {
                    pad.set(b, true);
                }
            }
            for s in [Stick::Left, Stick::Right] {
                let src = if s == Stick::Left {
                    AnalogSource::LeftStick
                } else {
                    AnalogSource::RightStick
                };
                if Control::of_stick(s).iter().any(hidden) || route_consumes(src) {
                    continue;
                }
                let cfg = self.profile.sticks.get(s);
                pad.set_stick(s, shape_stick(orient(phys.stick(s), cfg), cfg));
            }
            for t in [Trigger::Left, Trigger::Right] {
                if Control::of_trigger(t).iter().any(hidden) {
                    continue;
                }
                pad.set_axis(
                    t.axis(),
                    shape_trigger(phys.axis(t.axis()), self.profile.triggers.get(t)),
                );
            }
            // A remapped touchpad or gyro no longer reaches the game, like a remapped stick.
            let touch_used = TouchRegion::ALL.iter().any(|r| hidden(&Control::Touch(*r)))
                || route_consumes(AnalogSource::Touchpad);
            if !touch_used {
                pad.touch = phys.touch;
            }
            let gyro_used = Dir::ALL.iter().any(|d| hidden(&Control::Gyro(*d)))
                || route_consumes(AnalogSource::Gyro);
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
        for act in &self.acts {
            for a in &act.analog {
                match *a {
                    AnalogOut::Stick(s, x, y) => {
                        let (cx, cy) = pad.stick(s);
                        pad.set_stick(s, ((cx + x).clamp(-1.0, 1.0), (cy + y).clamp(-1.0, 1.0)));
                    }
                    AnalogOut::Trigger(t, v) => {
                        let cur = pad.axis(t.axis());
                        pad.set_axis(t.axis(), cur.max(v.clamp(0.0, 1.0)));
                    }
                    AnalogOut::MouseVelocity(x, y) => {
                        mouse_v.0 += x;
                        mouse_v.1 += y;
                    }
                }
            }
        }
        out.mouse_move = (mouse_v.0 * dt, mouse_v.1 * dt);

        // Analog routes.
        let input = RouteInput {
            pad: self.pad.as_ref(),
            sticks: &self.profile.sticks,
            mouse: self.mouse_counts,
            dt,
            now,
            gyro_enabled: self.gyro_enabled,
        };
        for &i in &routes {
            let r = &self.c.routes[i];
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
                    pad.set_stick(s, ((cx + x).clamp(-1.0, 1.0), (cy + y).clamp(-1.0, 1.0)));
                }
            }
        }

        if self.profile.output.virtual_pad != VirtualPadType::None {
            out.pad = Some(pad);
        }
        out
    }

    /// Whether any output is currently held down (for diagnostics).
    pub fn is_output_down(&self, key: Key) -> bool {
        self.outs.is_down(Out::Key(key))
    }

    /// Axis helper used by tests and diagnostics.
    pub fn physical_axis(&self, a: Axis) -> f32 {
        self.pad.map(|p| p.axis(a)).unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests;
