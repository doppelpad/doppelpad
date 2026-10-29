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
mod combo;
mod compile;
mod compose;
mod derive;
mod gyro;
mod layers;
mod radial;
mod recognizer;
mod routes;
mod touch;
mod vm;

/// The stick and trigger shaping and the zone rule, which the editor mirrors to draw them.
pub use self::analog::{apply_curve, shape_stick, shape_trigger, stick_zone, trigger_zone};
pub use self::radial::RadialView;

use std::collections::{HashMap, HashSet};

use self::compile::{BindingId, Compiled, LayerTable, MaskId, Out, compile};
use self::compose::JitterState;
use self::derive::WheelPulse;
use self::gyro::{GyroAxes, GyroState};
use self::layers::LayerSettings;
use self::recognizer::{RecEvent, Recognizer};
use self::routes::RouteState;
use self::touch::GestureTracker;
use self::vm::{ActId, Activation, Outputs};
use crate::control::{Control, TrackpadPart};
use crate::event::KbmEvent;
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadState, Rumble, Stick};
use crate::profile::{
    Command, Profile, Sticks, TriggerEffect, Triggers, VirtualPadType, VirtualStick,
};

/// Commands one tick runs at most; more (commands that keep queueing commands) are dropped.
const MAX_COMMANDS_PER_TICK: usize = 256;

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

/// Whether any mapping or route condition of the profile uses a control matching `f`.
fn uses(p: &Profile, f: impl Fn(&Control) -> bool) -> bool {
    p.mappings.iter().any(|m| m.trigger.0.iter().any(&f))
        || p.layers
            .iter()
            .any(|l| l.not_inherited.iter().any(|c| c.0.iter().any(&f)))
        || p.analog.iter().any(|r| r.while_held.iter().any(&f))
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
            if done.len() == MAX_COMMANDS_PER_TICK {
                break;
            }
            match c {
                Command::ToggleGyro => self.gyro_enabled = !self.gyro_enabled,
                Command::GyroOn => self.gyro_enabled = true,
                Command::GyroOff => self.gyro_enabled = false,
                Command::ResetTilt => self.gyro.reset(),
                Command::SwitchGyroAxis => self.gyro.switch_axis(),
                _ => self.radial_command(&c, now),
            }
            done.push(c);
            queue.extend(std::mem::take(&mut self.outs.commands));
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

    /// Whether a mapping holds `key` down right now.
    #[cfg(test)]
    pub(crate) fn is_output_down(&self, key: Key) -> bool {
        self.outs.is_down(Out::Key(key))
    }
}

#[cfg(test)]
mod tests;
