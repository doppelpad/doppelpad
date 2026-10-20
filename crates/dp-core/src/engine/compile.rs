//! Compile a [`Profile`] into the lookup tables the runtime uses.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::recognizer::RecConfig;
use crate::control::{Control, TouchGesture};
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};
use crate::profile::{
    Action, Activator, AnalogMapping, AnalogSource, Command, JumpMode, Mapping, Mode, Press,
    Profile, ShakeAxis,
};

pub type MaskId = usize;
pub type BindingId = usize;

/// Digital output, reference counted across activations.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Out {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}

/// Analog output contribution while held.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AnalogOut {
    /// Stick deflection; `true` replaces the axes it names instead of adding.
    Stick(Stick, f32, f32, bool),
    Trigger(Trigger, f32),
    /// Mouse velocity in pixels per second (screen coordinates, y down).
    MouseVelocity(f32, f32),
    /// Mouse velocity that grows by this much of itself per second held (up to 20 ×).
    MouseAccel(f32, f32, f32),
    /// Cursor kept inside this rectangle (x1, y1, x2, y2) while held.
    Clip(f32, f32, f32, f32),
}

impl AnalogOut {
    /// Contributions to the same slot replace each other within one activation.
    fn slot(&self) -> u8 {
        match self {
            AnalogOut::Stick(Stick::Left, ..) => 0,
            AnalogOut::Stick(Stick::Right, ..) => 1,
            AnalogOut::Trigger(Trigger::Left, _) => 2,
            AnalogOut::Trigger(Trigger::Right, _) => 3,
            AnalogOut::MouseVelocity(..) | AnalogOut::MouseAccel(..) => 4,
            AnalogOut::Clip(..) => 5,
        }
    }

    pub fn same_slot(&self, other: &AnalogOut) -> bool {
        self.slot() == other.slot()
    }
}

/// One instruction of a compiled action sequence.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Down(Out),
    Up(Out),
    AnalogDown(AnalogOut),
    AnalogUp(AnalogOut),
    Wheel(WheelDir, f32),
    Wait(u32),
    /// Hold mode: wait here until the activation ends.
    HoldPoint,
    Rumble {
        ms: u32,
        low: f32,
        high: f32,
    },
    Command(Command),
    /// Shake the virtual accelerometer.
    Shake(ShakeAxis),
    /// Turn in the game by this many degrees with mouse motion.
    Turn(f32),
    /// Play a gesture on the virtual touchpad for this many ms.
    Touch(TouchGesture, u32),
    /// Move the cursor to a point of the screen.
    MouseTo(f32, f32),
}

impl Step {
    fn is_event(&self) -> bool {
        matches!(
            self,
            Step::Down(_) | Step::Up(_) | Step::AnalogDown(_) | Step::AnalogUp(_) | Step::Wheel(..)
        )
    }
}

/// How an activation's program is driven.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Run the hold program; it parks at its hold point until End, then releases everything.
    Hold,
    /// Repeat the run program. `max_runs: None` = until stopped.
    Loop {
        max_runs: Option<u32>,
        stop: StopRule,
        gap_ms: u32,
        period_ms: Option<u32>,
        /// Extra random wait of up to this many ms before each next run.
        random_ms: u32,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StopRule {
    /// End stops at once and releases outputs (turbo pulses).
    Immediate,
    /// End lets the current run finish, then stops.
    FinishRun,
    /// End is ignored (once / repeat).
    Never,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub mask: MaskId,
    pub layer: u8,
    pub activator: Activator,
    pub mode: Mode,
    /// Program for hold-style execution (ends with a hold point), per segment.
    pub hold_programs: Vec<Vec<Step>>,
    /// Program for one sequence run (everything released at the end), per segment.
    pub run_programs: Vec<Vec<Step>>,
    /// Program for one turbo pulse (press everything, wait half a period, release).
    pub pulse_program: Vec<Step>,
    /// Every action is a plain holdable output, so turbo/toggle can pulse/latch it.
    pub holdable: bool,
    pub jump: JumpTarget,
    pub turbo_ms: u32,
    pub repeat: u32,
    pub repeat_pause_ms: u32,
    pub random_ms: u32,
    pub has_pad_output: bool,
    /// No actions and no jump: the trigger is consumed ("unmapped"/muted).
    pub is_unmap: bool,
    pub description: String,
}

impl Binding {
    pub fn policy(&self) -> Policy {
        match self.mode {
            Mode::Hold => Policy::Hold,
            Mode::Toggle if self.holdable => Policy::Hold,
            Mode::Toggle => Policy::Loop {
                max_runs: None,
                stop: StopRule::FinishRun,
                gap_ms: self.repeat_pause_ms,
                period_ms: None,
                random_ms: self.random_ms,
            },
            Mode::Turbo if self.holdable => Policy::Loop {
                max_runs: None,
                stop: StopRule::Immediate,
                gap_ms: 0,
                period_ms: Some(self.turbo_ms),
                random_ms: self.random_ms,
            },
            Mode::Turbo => Policy::Loop {
                max_runs: None,
                stop: StopRule::FinishRun,
                gap_ms: 0,
                period_ms: Some(self.turbo_ms),
                random_ms: self.random_ms,
            },
            Mode::Once => Policy::Loop {
                max_runs: Some(1),
                stop: StopRule::Never,
                gap_ms: 0,
                period_ms: None,
                random_ms: self.random_ms,
            },
            Mode::Repeat => Policy::Loop {
                max_runs: Some(self.repeat.max(1)),
                stop: StopRule::Never,
                gap_ms: self.repeat_pause_ms,
                period_ms: None,
                random_ms: self.random_ms,
            },
            Mode::WhileHeld => Policy::Loop {
                max_runs: None,
                stop: StopRule::FinishRun,
                gap_ms: self.repeat_pause_ms,
                period_ms: None,
                random_ms: self.random_ms,
            },
        }
    }

    /// The program of one segment (sequences without break steps have one).
    pub fn program(&self, segment: usize) -> &[Step] {
        match (self.policy(), self.mode) {
            (Policy::Hold, _) => &self.hold_programs[segment],
            (_, Mode::Turbo) if self.holdable => &self.pulse_program,
            _ => &self.run_programs[segment],
        }
    }

    /// Number of segments the break steps cut the sequence into.
    pub fn segments(&self) -> usize {
        self.run_programs.len()
    }
}

/// The sequence cut at its break steps; empty segments are dropped, but there is always one.
fn segments(actions: &[Action]) -> Vec<&[Action]> {
    let segs: Vec<&[Action]> = actions
        .split(Action::is_break)
        .filter(|s| !s.is_empty())
        .collect();
    if segs.is_empty() { vec![&[]] } else { segs }
}

/// Per-layer lookup tables (after inheritance).
#[derive(Clone, Debug, Default)]
pub struct LayerTable {
    /// Effective bindings of each mask in this layer.
    pub bindings: HashMap<MaskId, Vec<BindingId>>,
    pub rec: HashMap<MaskId, RecConfig>,
    /// Single-control mask per control.
    pub plain: HashMap<Control, MaskId>,
    /// Multi-control masks with bindings, and which of them each control belongs to.
    pub shortcuts: Vec<MaskId>,
    pub member_of: HashMap<Control, Vec<MaskId>>,
    /// Controls that have a single-control mapping here (not mirrored to the virtual pad).
    pub mapped: HashSet<Control>,
    /// Keyboard/mouse controls whose physical events are swallowed in this layer.
    pub muted: HashSet<Control>,
    /// Effective analog routes.
    pub routes: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Compiled {
    pub masks: Vec<Vec<Control>>,
    pub bindings: Vec<Binding>,
    /// Indexed by layer id; missing layers fall back to layer 0.
    pub layers: BTreeMap<u8, LayerTable>,
    pub routes: Vec<AnalogMapping>,
    pub warnings: Vec<String>,
}

impl Compiled {
    pub fn layer(&self, id: u8) -> &LayerTable {
        self.layers
            .get(&id)
            .or_else(|| self.layers.get(&0))
            .expect("layer 0 always exists")
    }
}

fn press_outputs(a: &Action) -> Option<(Option<Out>, Option<AnalogOut>, Press)> {
    Some(match *a {
        Action::Key { key, action } => (Some(Out::Key(key)), None, action),
        Action::Pad { pad, action } => (Some(Out::Pad(pad)), None, action),
        Action::Mouse { mouse, action } => (Some(Out::Mouse(mouse)), None, action),
        Action::Stick {
            stick,
            x,
            y,
            absolute,
            action,
        } => (None, Some(AnalogOut::Stick(stick, x, y, absolute)), action),
        Action::Trigger {
            trigger,
            value,
            action,
        } => (None, Some(AnalogOut::Trigger(trigger, value)), action),
        Action::MouseMove {
            mouse_move,
            speed,
            acceleration,
        } => {
            let (x, y) = mouse_move.vector();
            let out = if acceleration > 0.0 {
                AnalogOut::MouseAccel(x * speed, -y * speed, acceleration)
            } else {
                AnalogOut::MouseVelocity(x * speed, -y * speed)
            };
            (None, Some(out), Press::Auto)
        }
        Action::MouseClip {
            mouse_clip: [x1, y1, x2, y2],
        } => (None, Some(AnalogOut::Clip(x1, y1, x2, y2)), Press::Auto),
        _ => return None,
    })
}

/// Insert the key delay between consecutive input events (not next to explicit waits).
fn with_key_delay(steps: Vec<Step>, key_delay: u32) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::with_capacity(steps.len() * 2);
    for s in steps {
        if key_delay > 0 && s.is_event() && out.last().is_some_and(Step::is_event) {
            out.push(Step::Wait(key_delay));
        }
        out.push(s);
    }
    out
}

fn other_steps(a: &Action) -> Option<Step> {
    Some(match a {
        Action::Wheel { wheel, clicks } => Step::Wheel(*wheel, *clicks),
        Action::Pause { pause } => Step::Wait(*pause),
        Action::Rumble { rumble, low, high } => Step::Rumble {
            ms: *rumble,
            low: *low,
            high: *high,
        },
        Action::Command { command } => Step::Command(command.clone()),
        Action::Shake { shake } => Step::Shake(*shake),
        Action::Turn { turn } => Step::Turn(*turn),
        Action::MouseTo { mouse_to: [x, y] } => Step::MouseTo(*x, *y),
        Action::Touch { touch, ms } => Step::Touch(
            *touch,
            ms.unwrap_or_else(|| super::touch::output_ms(*touch)),
        ),
        _ => return None,
    })
}

const ANALOG_TAP_MS: u32 = 20;

fn hold_program(actions: &[Action], key_delay: u32) -> Vec<Step> {
    let mut steps = Vec::new();
    for a in actions {
        if let Some((digital, analog, press)) = press_outputs(a) {
            match press {
                Press::Up => {} // hold programs drop explicit releases
                Press::Tap => {
                    if let Some(o) = digital {
                        steps.extend([Step::Down(o), Step::Wait(key_delay.max(1)), Step::Up(o)]);
                    }
                    if let Some(o) = analog {
                        steps.extend([
                            Step::AnalogDown(o),
                            Step::Wait(key_delay.max(ANALOG_TAP_MS)),
                            Step::AnalogUp(o),
                        ]);
                    }
                }
                Press::Auto | Press::Down => {
                    steps.extend(digital.map(Step::Down));
                    steps.extend(analog.map(Step::AnalogDown));
                }
            }
        } else if let Some(s) = other_steps(a) {
            steps.push(s);
        }
    }
    let mut steps = with_key_delay(steps, key_delay);
    steps.push(Step::HoldPoint);
    steps
}

fn run_program(actions: &[Action], key_delay: u32) -> Vec<Step> {
    let mut steps = Vec::new();
    for a in actions {
        if let Some((digital, analog, press)) = press_outputs(a) {
            match press {
                Press::Down => {
                    steps.extend(digital.map(Step::Down));
                    steps.extend(analog.map(Step::AnalogDown));
                }
                Press::Up => {
                    steps.extend(digital.map(Step::Up));
                    steps.extend(analog.map(Step::AnalogUp));
                }
                Press::Auto | Press::Tap => {
                    if let Some(o) = digital {
                        steps.extend([Step::Down(o), Step::Wait(key_delay.max(1)), Step::Up(o)]);
                    }
                    if let Some(o) = analog {
                        steps.extend([
                            Step::AnalogDown(o),
                            Step::Wait(key_delay.max(ANALOG_TAP_MS)),
                            Step::AnalogUp(o),
                        ]);
                    }
                }
            }
        } else if let Some(s) = other_steps(a) {
            steps.push(s);
        }
    }
    with_key_delay(steps, key_delay)
}

fn pulse_program(actions: &[Action], period: u32) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut ups = Vec::new();
    for a in actions {
        if let Some((digital, analog, _)) = press_outputs(a) {
            if let Some(o) = digital {
                steps.push(Step::Down(o));
                ups.push(Step::Up(o));
            }
            if let Some(o) = analog {
                steps.push(Step::AnalogDown(o));
                ups.push(Step::AnalogUp(o));
            }
        }
    }
    steps.push(Step::Wait((period / 2).max(1)));
    steps.extend(ups);
    steps
}

fn is_holdable(actions: &[Action]) -> bool {
    !actions.is_empty()
        && actions.iter().all(|a| {
            matches!(press_outputs(a), Some((_, _, Press::Auto | Press::Down)))
                && !matches!(a, Action::MouseMove { .. })
        })
}

fn has_pad_output(actions: &[Action]) -> bool {
    actions.iter().any(|a| {
        matches!(
            a,
            Action::Pad { .. } | Action::Stick { .. } | Action::Trigger { .. }
        )
    })
}

/// Expand jump sugar: `hold` and `toggle` jumps get a return mapping in the target layer.
/// Target layer, optional delay and postponed actions of a binding's jump.
type JumpTarget = Option<(u8, Option<u32>, bool)>;

fn expand_jumps(mappings: &[Mapping]) -> Vec<(Mapping, JumpTarget)> {
    let mut out = Vec::new();
    for m in mappings {
        let jump = m.jump.as_ref().map(|j| (j.layer, j.delay_ms, j.postpone));
        out.push((m.clone(), jump));
        let Some(j) = &m.jump else { continue };
        if j.layer == m.layer {
            continue;
        }
        let back = match j.mode {
            JumpMode::Switch => continue,
            JumpMode::Hold => Activator::Release,
            JumpMode::Toggle => m.activator,
        };
        let mut ret = Mapping::new(m.trigger.clone(), Vec::new());
        ret.layer = j.layer;
        ret.activator = back;
        ret.mode = if back == Activator::Release {
            Mode::Once
        } else {
            Mode::Hold
        };
        ret.long_press_ms = m.long_press_ms;
        ret.double_tap_ms = m.double_tap_ms;
        ret.triple_tap_ms = m.triple_tap_ms;
        ret.description = format!("return to layer {}", m.layer);
        out.push((ret, Some((m.layer, None, false))));
    }
    out
}

/// Pulse length of decided-after-the-fact activations.
const PULSE_PAD_MS: u64 = 20;
const PULSE_KEY_MS: u64 = 10;

#[derive(Default)]
struct MaskTable {
    masks: Vec<Vec<Control>>,
    ids: HashMap<Vec<Control>, MaskId>,
}

impl MaskTable {
    fn id(&mut self, combo: &[Control]) -> MaskId {
        let mut key = combo.to_vec();
        key.sort();
        key.dedup();
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        self.masks.push(key.clone());
        self.ids.insert(key, self.masks.len() - 1);
        self.masks.len() - 1
    }
}

pub fn compile(p: &Profile) -> Compiled {
    let mut warnings = Vec::new();
    let mut mt = MaskTable::default();

    let mut bindings = Vec::new();
    for (m, jump) in expand_jumps(&p.mappings) {
        let key_delay = m.key_delay_ms.unwrap_or(p.timing.key_delay_ms);
        let turbo_ms = m.turbo_ms.unwrap_or(p.timing.turbo_ms).max(10);
        let mut mode = m.mode;
        let mut activator = m.activator;
        // Release cannot hold/turbo/toggle; Start cannot toggle.
        if activator == Activator::Release
            && matches!(
                mode,
                Mode::Hold | Mode::Turbo | Mode::Toggle | Mode::WhileHeld
            )
        {
            mode = Mode::Once;
        }
        if activator == Activator::Start && mode == Mode::Toggle {
            mode = Mode::Hold;
        }
        if m.trigger.0.iter().any(|c| c.is_momentary()) && matches!(mode, Mode::Toggle) {
            warnings.push(format!(
                "{}: toggle on a wheel trigger behaves like hold",
                m.trigger
            ));
            activator = Activator::Single;
        }
        // Segments play one per activation, so they cannot pulse, latch or repeat.
        let segs = segments(&m.actions);
        if segs.len() > 1 || m.actions.iter().any(Action::is_break) {
            mode = match mode {
                Mode::Turbo | Mode::Toggle => Mode::Hold,
                Mode::Repeat | Mode::WhileHeld => Mode::Once,
                other => other,
            };
        }
        let mask = mt.id(&m.trigger.0);
        bindings.push(Binding {
            mask,
            layer: m.layer,
            activator,
            mode,
            hold_programs: segs.iter().map(|s| hold_program(s, key_delay)).collect(),
            run_programs: segs.iter().map(|s| run_program(s, key_delay)).collect(),
            pulse_program: pulse_program(&m.actions, turbo_ms),
            holdable: is_holdable(&m.actions),
            jump,
            turbo_ms,
            repeat: m.repeat.unwrap_or(1).clamp(1, 9999),
            repeat_pause_ms: m.repeat_pause_ms.unwrap_or(0),
            random_ms: m.random_ms.unwrap_or(0),
            has_pad_output: has_pad_output(&m.actions),
            is_unmap: m.actions.is_empty() && jump.is_none(),
            description: m.description.clone(),
        });
    }

    // Layer ids: 0 plus every declared or referenced layer.
    let mut layer_ids: Vec<u8> = vec![0];
    layer_ids.extend(p.layers.iter().map(|l| l.id));
    layer_ids.extend(bindings.iter().map(|b| b.layer));
    layer_ids.extend(bindings.iter().filter_map(|b| b.jump.map(|j| j.0)));
    layer_ids.sort();
    layer_ids.dedup();

    let mappings_expanded = expand_jumps(&p.mappings);
    let mut layers = BTreeMap::new();
    for &lid in &layer_ids {
        let decl = p.layers.iter().find(|l| l.id == lid);
        let inherit = lid != 0 && decl.is_none_or(|l| l.inherit);
        // Unlock layers give the physical keyboard/mouse back: nothing of it is swallowed and
        // the base layer's keyboard/mouse mappings stay behind.
        let unlock = decl.and_then(|l| l.unlock);
        let unlocked = |c: Control| unlock.is_some_and(|u| u.covers(c));
        let not_inherited: HashSet<MaskId> = decl
            .map(|l| l.not_inherited.iter().map(|c| mt.id(&c.0)).collect())
            .unwrap_or_default();

        let mut table = LayerTable::default();
        for (bid, b) in bindings.iter().enumerate() {
            if b.layer == lid {
                table.bindings.entry(b.mask).or_default().push(bid);
            }
        }
        if inherit {
            for (bid, b) in bindings.iter().enumerate() {
                if b.layer == 0
                    && !not_inherited.contains(&b.mask)
                    && !mt.masks[b.mask].iter().any(|&c| unlocked(c))
                    && !bindings.iter().any(|o| o.layer == lid && o.mask == b.mask)
                {
                    table.bindings.entry(b.mask).or_default().push(bid);
                }
            }
        }

        for (&mask, bids) in &table.bindings {
            let mut rec = RecConfig {
                single: false,
                t_long: p.timing.long_press_ms as u64,
                t_double: p.timing.double_tap_ms as u64,
                t_triple: p.timing.triple_tap_ms as u64,
                t_pulse: PULSE_KEY_MS,
                ..Default::default()
            };
            for &bid in bids {
                let b = &bindings[bid];
                match b.activator {
                    Activator::Single => rec.single = true,
                    Activator::Long => rec.long = true,
                    Activator::Double => rec.double = true,
                    Activator::Triple => rec.triple = true,
                    Activator::Start => rec.start = true,
                    Activator::Release => rec.release = true,
                }
                if b.has_pad_output {
                    rec.t_pulse = PULSE_PAD_MS;
                }
                let (m, _) = &mappings_expanded[bid];
                if let Some(t) = m.long_press_ms {
                    rec.t_long = t as u64;
                }
                if let Some(t) = m.double_tap_ms {
                    rec.t_double = t as u64;
                }
                if let Some(t) = m.triple_tap_ms {
                    rec.t_triple = t as u64;
                }
            }
            if rec.t_triple <= rec.t_double {
                rec.t_triple = rec.t_double + 100;
            }
            table.rec.insert(mask, rec);

            let members = &mt.masks[mask];
            if members.len() == 1 {
                let c = members[0];
                table.plain.insert(c, mask);
                table.mapped.insert(c);
                let unmap = bids.iter().any(|&b| bindings[b].is_unmap);
                if !c.is_pad() && !unlocked(c) && (unmap || p.output.mute_mapped_inputs) {
                    table.muted.insert(c);
                }
            } else {
                table.shortcuts.push(mask);
                for &c in members {
                    table.member_of.entry(c).or_default().push(mask);
                }
            }
        }
        table.shortcuts.sort();

        // Analog routes: this layer's own, plus inherited base routes for other sources.
        let own: HashSet<AnalogSource> = p
            .analog
            .iter()
            .filter(|r| r.layer == lid)
            .map(|r| r.source)
            .collect();
        for (i, r) in p.analog.iter().enumerate() {
            let mouse = r.source == AnalogSource::Mouse;
            let gone = mouse && unlocked(Control::MouseMove(crate::control::Dir::Up));
            if r.layer == lid || (inherit && r.layer == 0 && !own.contains(&r.source) && !gone) {
                table.routes.push(i);
                if mouse && r.consume && !gone {
                    table
                        .muted
                        .insert(Control::MouseMove(crate::control::Dir::Up));
                }
            }
        }
        layers.insert(lid, table);
    }

    for b in &bindings {
        if let Some((target, ..)) = b.jump
            && !layer_ids.contains(&target)
        {
            warnings.push(format!("jump to undefined layer {target}"));
        }
    }

    Compiled {
        masks: mt.masks,
        bindings,
        layers,
        routes: p.analog.clone(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Combo, Jump, Layer};

    fn btn(b: PadButton) -> Control {
        Control::Button(b)
    }

    #[test]
    fn hold_jump_creates_release_return() {
        let mut p = Profile::default();
        let mut m = Mapping::new(btn(PadButton::Back), vec![]);
        m.jump = Some(Jump {
            layer: 1,
            mode: JumpMode::Hold,
            delay_ms: None,
            postpone: false,
        });
        p.mappings.push(m);
        p.mappings.push(Mapping::new(
            btn(PadButton::A),
            vec![Action::Key {
                key: Key::Space,
                action: Press::Auto,
            }],
        ));
        let c = compile(&p);
        assert_eq!(c.bindings.len(), 3);
        let ret = &c.bindings[1];
        assert_eq!(
            (ret.layer, ret.activator, ret.jump),
            (1, Activator::Release, Some((0, None, false)))
        );
        // Layer 1 inherits A from layer 0, but Back has its own (release) binding there.
        let l1 = c.layer(1);
        let a_mask = c.layer(0).plain[&btn(PadButton::A)];
        assert_eq!(l1.bindings[&a_mask], vec![2]);
        let back_mask = l1.plain[&btn(PadButton::Back)];
        assert!(l1.rec[&back_mask].release && !l1.rec[&back_mask].single);
    }

    #[test]
    fn not_inherited_blocks_base_mapping() {
        let mut p = Profile::default();
        p.layers.push(Layer {
            id: 1,
            not_inherited: vec![Combo::single(btn(PadButton::A))],
            ..Default::default()
        });
        p.mappings.push(Mapping::new(
            btn(PadButton::A),
            vec![Action::Key {
                key: Key::Space,
                action: Press::Auto,
            }],
        ));
        let c = compile(&p);
        assert!(c.layer(1).bindings.is_empty());
        assert!(
            !c.layer(1).mapped.contains(&btn(PadButton::A)),
            "unmapped there, so it passes through"
        );
    }

    #[test]
    fn programs() {
        let actions = vec![
            Action::Key {
                key: Key::LeftCtrl,
                action: Press::Auto,
            },
            Action::Key {
                key: Key::C,
                action: Press::Auto,
            },
        ];
        assert_eq!(
            hold_program(&actions, 10),
            vec![
                Step::Down(Out::Key(Key::LeftCtrl)),
                Step::Wait(10),
                Step::Down(Out::Key(Key::C)),
                Step::HoldPoint
            ]
        );
        let run = run_program(
            &[
                Action::Key {
                    key: Key::A,
                    action: Press::Auto,
                },
                Action::Pause { pause: 50 },
            ],
            10,
        );
        assert_eq!(
            run,
            vec![
                Step::Down(Out::Key(Key::A)),
                Step::Wait(10),
                Step::Up(Out::Key(Key::A)),
                Step::Wait(50)
            ]
        );
        assert!(is_holdable(&actions));
        assert!(!is_holdable(&[Action::Pause { pause: 5 }]));
    }

    #[test]
    fn shortcuts_and_muting() {
        let mut p = Profile::default();
        p.mappings.push(Mapping::new(
            Combo(vec![btn(PadButton::LB), btn(PadButton::RB)]),
            vec![Action::Key {
                key: Key::Escape,
                action: Press::Auto,
            }],
        ));
        p.mappings.push(Mapping::new(
            Control::Key(Key::W),
            vec![Action::Stick {
                stick: Stick::Left,
                x: 0.0,
                y: 1.0,
                absolute: false,
                action: Press::Auto,
            }],
        ));
        let c = compile(&p);
        let l0 = c.layer(0);
        assert_eq!(l0.shortcuts.len(), 1);
        assert_eq!(l0.member_of[&btn(PadButton::LB)].len(), 1);
        assert!(l0.muted.contains(&Control::Key(Key::W)));
        assert!(
            !l0.mapped.contains(&btn(PadButton::LB)),
            "shortcut members still pass through"
        );
    }
}
