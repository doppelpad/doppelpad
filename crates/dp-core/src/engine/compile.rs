//! Compile a [`Profile`] into the lookup tables the runtime uses.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::recognizer::RecConfig;
use crate::control::{Control, Dir, TouchGesture};
use crate::keys::{Key, MouseButton, WheelDir};
use crate::pad::{PadButton, Stick, Trigger};
use crate::profile::{
    Action, Activator, AnalogSource, Combo, Command, JumpMode, Mapping, Mode, Press, Profile,
    RadialSelect, ShakeAxis,
};

pub(super) type MaskId = usize;
pub(super) type BindingId = usize;

/// Digital output, reference counted across activations.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Out {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}

/// Analog output contribution while held.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) enum AnalogOut {
    /// Stick deflection; `absolute` replaces the axes it names instead of adding.
    Stick {
        stick: Stick,
        x: f32,
        y: f32,
        absolute: bool,
    },
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
            AnalogOut::Stick {
                stick: Stick::Left, ..
            } => 0,
            AnalogOut::Stick {
                stick: Stick::Right,
                ..
            } => 1,
            AnalogOut::Trigger(Trigger::Left, _) => 2,
            AnalogOut::Trigger(Trigger::Right, _) => 3,
            AnalogOut::MouseVelocity(..) | AnalogOut::MouseAccel(..) => 4,
            AnalogOut::Clip(..) => 5,
        }
    }

    pub(super) fn same_slot(&self, other: &AnalogOut) -> bool {
        self.slot() == other.slot()
    }
}

/// One instruction of a compiled action sequence.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Step {
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
pub(super) enum Policy {
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
pub(super) enum StopRule {
    /// End stops at once and releases outputs (turbo pulses).
    Immediate,
    /// End lets the current run finish, then stops.
    FinishRun,
    /// End is ignored (once / repeat).
    Never,
}

/// Where a binding jumps: the target layer, an optional delay, and whether the actions wait
/// for the delayed jump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct JumpTarget {
    pub(super) layer: u8,
    pub(super) delay_ms: Option<u32>,
    pub(super) postpone: bool,
}

#[derive(Clone, Debug)]
pub(super) struct Binding {
    pub(super) mask: MaskId,
    pub(super) layer: u8,
    pub(super) activator: Activator,
    pub(super) mode: Mode,
    /// How an activation runs the programs.
    pub(super) policy: Policy,
    /// One program per segment: hold programs (ending with a hold point), runs (everything
    /// released at the end), or the one turbo pulse (press, wait half a period, release).
    pub(super) programs: Vec<Vec<Step>>,
    pub(super) jump: Option<JumpTarget>,
    pub(super) has_pad_output: bool,
    /// No actions and no jump: the trigger is consumed ("unmapped"/muted).
    pub(super) is_unmap: bool,
    /// The mapping's own press-pattern windows, where it sets them.
    pub(super) long_press_ms: Option<u32>,
    pub(super) double_tap_ms: Option<u32>,
    pub(super) triple_tap_ms: Option<u32>,
}

impl Binding {
    /// The program of one segment (sequences without break steps have one).
    pub(super) fn program(&self, segment: usize) -> &[Step] {
        &self.programs[segment]
    }

    /// Number of segments the break steps cut the sequence into.
    pub(super) fn segments(&self) -> usize {
        self.programs.len()
    }
}

/// How a binding in `mode` runs; `holdable` when every action is a plain output to hold, so
/// turbo pulses it and toggle latches it.
fn policy_for(mode: Mode, holdable: bool, m: &Mapping, turbo_ms: u32) -> Policy {
    let random_ms = m.random_ms.unwrap_or(0);
    let gap_ms = m.repeat_pause_ms.unwrap_or(0);
    match mode {
        Mode::Hold => Policy::Hold,
        Mode::Toggle if holdable => Policy::Hold,
        // A toggled sequence loops until the next activation, like a held one.
        Mode::Toggle | Mode::WhileHeld => Policy::Loop {
            max_runs: None,
            stop: StopRule::FinishRun,
            gap_ms,
            period_ms: None,
            random_ms,
        },
        Mode::Turbo => Policy::Loop {
            max_runs: None,
            stop: if holdable {
                StopRule::Immediate
            } else {
                StopRule::FinishRun
            },
            gap_ms: 0,
            period_ms: Some(turbo_ms),
            random_ms,
        },
        Mode::Once => Policy::Loop {
            max_runs: Some(1),
            stop: StopRule::Never,
            gap_ms: 0,
            period_ms: None,
            random_ms,
        },
        Mode::Repeat => Policy::Loop {
            max_runs: Some(m.repeat.unwrap_or(1).clamp(1, MAX_REPEATS)),
            stop: StopRule::Never,
            gap_ms,
            period_ms: None,
            random_ms,
        },
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
pub(super) struct LayerTable {
    /// Effective bindings of each mask in this layer.
    pub(super) bindings: HashMap<MaskId, Vec<BindingId>>,
    pub(super) rec: HashMap<MaskId, RecConfig>,
    /// Single-control mask per control.
    pub(super) plain: HashMap<Control, MaskId>,
    /// Multi-control masks with bindings, and which of them each control belongs to.
    pub(super) shortcuts: Vec<MaskId>,
    pub(super) member_of: HashMap<Control, Vec<MaskId>>,
    /// Controls that have a single-control mapping here (not mirrored to the virtual pad).
    pub(super) mapped: HashSet<Control>,
    /// Keyboard/mouse controls whose physical events are swallowed in this layer.
    pub(super) muted: HashSet<Control>,
    /// Effective analog routes (indices into the profile's).
    pub(super) routes: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct Compiled {
    pub(super) masks: Vec<Vec<Control>>,
    pub(super) bindings: Vec<Binding>,
    /// Indexed by layer id; missing layers fall back to layer 0.
    pub(super) layers: BTreeMap<u8, LayerTable>,
    pub(super) warnings: Vec<String>,
    /// The binding of every radial menu sector, per menu.
    pub(super) radial: Vec<Vec<BindingId>>,
}

impl Compiled {
    pub(super) fn layer(&self, id: u8) -> &LayerTable {
        self.layers
            .get(&id)
            .or_else(|| self.layers.get(&0))
            .expect("layer 0 always exists")
    }
}

/// What a key, button or analog action holds while it is down.
#[derive(Copy, Clone)]
enum Held {
    Digital(Out),
    Analog(AnalogOut),
}

impl Held {
    fn down(self) -> Step {
        match self {
            Held::Digital(o) => Step::Down(o),
            Held::Analog(o) => Step::AnalogDown(o),
        }
    }

    fn up(self) -> Step {
        match self {
            Held::Digital(o) => Step::Up(o),
            Held::Analog(o) => Step::AnalogUp(o),
        }
    }

    /// Down, a short wait (analog outputs a little longer, so games read them), up.
    fn tap(self, key_delay: u32) -> [Step; 3] {
        let wait = match self {
            Held::Digital(_) => key_delay.max(1),
            Held::Analog(_) => key_delay.max(ANALOG_TAP_MS),
        };
        [self.down(), Step::Wait(wait), self.up()]
    }
}

fn press_outputs(a: &Action) -> Option<(Held, Press)> {
    Some(match *a {
        Action::Key { key, action } => (Held::Digital(Out::Key(key)), action),
        Action::Pad { pad, action } => (Held::Digital(Out::Pad(pad)), action),
        Action::Mouse { mouse, action } => (Held::Digital(Out::Mouse(mouse)), action),
        Action::Stick {
            stick,
            x,
            y,
            absolute,
            action,
        } => (
            Held::Analog(AnalogOut::Stick {
                stick,
                x,
                y,
                absolute,
            }),
            action,
        ),
        Action::Trigger {
            trigger,
            value,
            action,
        } => (Held::Analog(AnalogOut::Trigger(trigger, value)), action),
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
            (Held::Analog(out), Press::Auto)
        }
        Action::MouseClip {
            mouse_clip: [x1, y1, x2, y2],
        } => (Held::Analog(AnalogOut::Clip(x1, y1, x2, y2)), Press::Auto),
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
        if let Some((held, press)) = press_outputs(a) {
            match press {
                Press::Up => {} // hold programs drop explicit releases
                Press::Tap => steps.extend(held.tap(key_delay)),
                Press::Auto | Press::Down => steps.push(held.down()),
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
        if let Some((held, press)) = press_outputs(a) {
            match press {
                Press::Down => steps.push(held.down()),
                Press::Up => steps.push(held.up()),
                Press::Auto | Press::Tap => steps.extend(held.tap(key_delay)),
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
        if let Some((held, _)) = press_outputs(a) {
            steps.push(held.down());
            ups.push(held.up());
        }
    }
    steps.push(Step::Wait((period / 2).max(1)));
    steps.extend(ups);
    steps
}

fn is_holdable(actions: &[Action]) -> bool {
    !actions.is_empty()
        && actions.iter().all(|a| {
            matches!(press_outputs(a), Some((_, Press::Auto | Press::Down)))
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
fn expand_jumps(mappings: &[Mapping]) -> Vec<(Mapping, Option<JumpTarget>)> {
    let mut out = Vec::new();
    for m in mappings {
        let jump = m.jump.as_ref().map(|j| JumpTarget {
            layer: j.layer,
            delay_ms: j.delay_ms,
            postpone: j.postpone,
        });
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
        let back = JumpTarget {
            layer: m.layer,
            delay_ms: None,
            postpone: false,
        };
        out.push((ret, Some(back)));
    }
    out
}

/// Stands for all mouse motion in the muted set and the unlock checks.
const MOUSE_MOTION: Control = Control::MouseMove(Dir::Up);

/// Pulse length of decided-after-the-fact activations.
pub(super) const PULSE_PAD_MS: u64 = 20;
const PULSE_KEY_MS: u64 = 10;
/// Shortest turbo period.
const MIN_TURBO_MS: u32 = 10;
/// Most runs a `repeat` mapping makes.
const MAX_REPEATS: u32 = 9999;
/// The triple-press window ends at least this long after the double-press window.
const MIN_TRIPLE_GAP_MS: u64 = 100;

#[derive(Default)]
struct MaskTable {
    masks: Vec<Vec<Control>>,
    ids: HashMap<Vec<Control>, MaskId>,
}

impl MaskTable {
    fn id(&mut self, combo: &[Control]) -> MaskId {
        let key = Combo(combo.to_vec()).normalized().0;
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        self.masks.push(key.clone());
        self.ids.insert(key, self.masks.len() - 1);
        self.masks.len() - 1
    }
}

/// The binding a mapping compiles to.
fn make_binding(
    p: &Profile,
    m: &Mapping,
    jump: Option<JumpTarget>,
    mt: &mut MaskTable,
    warnings: &mut Vec<String>,
) -> Binding {
    let key_delay = m.key_delay_ms.unwrap_or(p.timing.key_delay_ms);
    let turbo_ms = m.turbo_ms.unwrap_or(p.timing.turbo_ms).max(MIN_TURBO_MS);
    let mut mode = m.effective_mode();
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
            "{}: toggle on a wheel trigger switches on with one notch and off with the next, as single presses",
            m.trigger
        ));
        activator = Activator::Single;
    }
    let segs = segments(&m.actions);
    let holdable = is_holdable(&m.actions);
    let policy = policy_for(mode, holdable, m, turbo_ms);
    let programs = match policy {
        Policy::Hold => segs.iter().map(|s| hold_program(s, key_delay)).collect(),
        Policy::Loop { .. } if mode == Mode::Turbo && holdable => {
            vec![pulse_program(&m.actions, turbo_ms)]
        }
        Policy::Loop { .. } => segs.iter().map(|s| run_program(s, key_delay)).collect(),
    };
    Binding {
        mask: mt.id(&m.trigger.0),
        layer: m.layer,
        activator,
        mode,
        policy,
        programs,
        jump,
        has_pad_output: has_pad_output(&m.actions),
        is_unmap: m.actions.is_empty() && jump.is_none(),
        long_press_ms: m.long_press_ms,
        double_tap_ms: m.double_tap_ms,
        triple_tap_ms: m.triple_tap_ms,
    }
}

pub(super) fn compile(p: &Profile) -> Compiled {
    let mut warnings = Vec::new();
    let mut mt = MaskTable::default();

    let mut bindings = Vec::new();
    for (m, jump) in expand_jumps(&p.mappings) {
        bindings.push(make_binding(p, &m, jump, &mut mt, &mut warnings));
    }

    // Layer ids: 0 plus every declared or referenced layer.
    let mut layer_ids: Vec<u8> = vec![0];
    layer_ids.extend(p.layers.iter().map(|l| l.id));
    layer_ids.extend(bindings.iter().map(|b| b.layer));
    layer_ids.extend(bindings.iter().filter_map(|b| b.jump.map(|j| j.layer)));
    layer_ids.sort();
    layer_ids.dedup();

    let mut layers = BTreeMap::new();
    for &lid in &layer_ids {
        let decl = p.layer(lid);
        let inherit = lid != 0 && decl.is_none_or(|l| l.inherit);
        // Unlock layers give the physical keyboard/mouse back: nothing of it is swallowed and
        // the base layer's keyboard/mouse mappings stay behind.
        let unlock = decl.and_then(|l| l.unlock);
        let unlocked = |c: Control| unlock.is_some_and(|u| u.covers(c));

        let mut table = LayerTable::default();
        for (bid, b) in bindings.iter().enumerate() {
            if b.layer == lid {
                table.bindings.entry(b.mask).or_default().push(bid);
            }
        }
        // The base layer's bindings the layer takes over (the rule dp-config checks with
        // `Profile::applies_on`; the bindings include the ways back of jumps).
        let own: HashSet<MaskId> = table.bindings.keys().copied().collect();
        for (bid, b) in bindings.iter().enumerate() {
            if b.layer == 0 && !own.contains(&b.mask) && p.inherits(lid, &mt.masks[b.mask]) {
                table.bindings.entry(b.mask).or_default().push(bid);
            }
        }

        for (&mask, bids) in &table.bindings {
            let mut rec = RecConfig {
                single: false,
                t_long: u64::from(p.timing.long_press_ms),
                t_double: u64::from(p.timing.double_tap_ms),
                t_triple: u64::from(p.timing.triple_tap_ms),
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
                if let Some(t) = b.long_press_ms {
                    rec.t_long = u64::from(t);
                }
                if let Some(t) = b.double_tap_ms {
                    rec.t_double = u64::from(t);
                }
                if let Some(t) = b.triple_tap_ms {
                    rec.t_triple = u64::from(t);
                }
            }
            if rec.t_triple <= rec.t_double {
                rec.t_triple = rec.t_double + MIN_TRIPLE_GAP_MS;
            }
            table.rec.insert(mask, rec);

            let members = &mt.masks[mask];
            if members.len() == 1 {
                let c = members[0];
                table.plain.insert(c, mask);
                table.mapped.insert(c);
                let unmap = bids.iter().any(|&b| bindings[b].is_unmap);
                if c.is_kbm() && !unlocked(c) && (unmap || p.output.mute_mapped_inputs) {
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
            let gone = mouse && unlocked(MOUSE_MOTION);
            if r.layer == lid || (inherit && r.layer == 0 && !own.contains(&r.source) && !gone) {
                table.routes.push(i);
                if mouse && r.consume && !gone {
                    table.muted.insert(MOUSE_MOTION);
                }
            }
        }
        layers.insert(lid, table);
    }

    // Radial sectors play like one-shot mappings that no input triggers; a menu pointed by
    // the mouse keeps the cursor still while it is open.
    let mut radial = Vec::new();
    for menu in &p.radial_menus {
        let mut ids = Vec::new();
        for s in &menu.sectors {
            let mut m = Mapping::new(Combo(Vec::new()), s.actions.clone());
            m.mode = Mode::Once;
            ids.push(bindings.len());
            bindings.push(make_binding(p, &m, None, &mut mt, &mut warnings));
        }
        radial.push(ids);
        if menu.select == RadialSelect::Mouse
            && let Some(t) = layers.get_mut(&menu.layer)
        {
            t.muted.insert(MOUSE_MOTION);
        }
    }

    Compiled {
        masks: mt.masks,
        bindings,
        layers,
        warnings,
        radial,
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
    fn inheritance_agrees_with_profile_applies_on() {
        use crate::profile::Unlock;
        let key = |k| {
            vec![Action::Key {
                key: k,
                action: Press::Auto,
            }]
        };
        let jump = |layer, mode| Jump {
            layer,
            mode,
            delay_ms: None,
            postpone: false,
        };
        let mut p = Profile::default();
        let mut m = Mapping::new(btn(PadButton::LB), vec![]);
        m.jump = Some(jump(1, JumpMode::Hold));
        p.mappings.push(m);
        let mut m = Mapping::new(btn(PadButton::RB), vec![]);
        m.jump = Some(jump(2, JumpMode::Toggle));
        p.mappings.push(m);
        let mut m = Mapping::new(btn(PadButton::Back), vec![]);
        m.jump = Some(jump(3, JumpMode::Switch));
        p.mappings.push(m);
        p.mappings
            .push(Mapping::new(btn(PadButton::A), key(Key::A)));
        let mut m = Mapping::new(btn(PadButton::A), key(Key::B));
        m.activator = Activator::Long;
        p.mappings.push(m);
        p.mappings
            .push(Mapping::new(btn(PadButton::B), key(Key::C)));
        p.mappings
            .push(Mapping::new(Control::Key(Key::Q), key(Key::D)));
        // A base mapping on the hold jump's trigger: the way back takes it over on layer 1.
        let mut m = Mapping::new(btn(PadButton::LB), key(Key::E));
        m.activator = Activator::Double;
        p.mappings.push(m);
        let mut m = Mapping::new(btn(PadButton::X), key(Key::F));
        m.layer = 1;
        p.mappings.push(m);
        p.mappings
            .push(Mapping::new(btn(PadButton::X), key(Key::G)));
        p.layers = vec![
            Layer {
                id: 1,
                not_inherited: vec![Combo::single(btn(PadButton::B))],
                ..Layer::default()
            },
            Layer {
                id: 2,
                inherit: false,
                ..Layer::default()
            },
            Layer {
                id: 3,
                unlock: Some(Unlock::Keyboard),
                ..Layer::default()
            },
        ];
        let c = compile(&p);
        // The binding of each mapping (expand_jumps adds a way back after hold and toggle
        // jumps).
        let mut bid = 0;
        let mut of_mapping = Vec::new();
        for m in &p.mappings {
            of_mapping.push(bid);
            bid += 1;
            if m.jump.as_ref().is_some_and(|j| j.mode != JumpMode::Switch) {
                bid += 1;
            }
        }
        for l in 1..=3u8 {
            let table = &c.layers[&l];
            for (i, m) in p.mappings.iter().enumerate() {
                let compiled = table
                    .bindings
                    .values()
                    .any(|bids| bids.contains(&of_mapping[i]));
                assert_eq!(compiled, p.applies_on(m, l), "mapping {i} on layer {l}");
            }
        }
        assert!(!p.applies_on(&p.mappings[7], 1), "shadowed by the way back");
        assert!(p.applies_on(&p.mappings[3], 1) && !p.applies_on(&p.mappings[5], 1));
        assert!(
            !p.applies_on(&p.mappings[6], 3),
            "the keyboard is given back"
        );
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
            (
                1,
                Activator::Release,
                Some(JumpTarget {
                    layer: 0,
                    delay_ms: None,
                    postpone: false
                })
            )
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
