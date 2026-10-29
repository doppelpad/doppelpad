//! Checks for mistakes in a profile that the engine would silently tolerate.

use dp_core::pad::Rgb;
use dp_core::profile::{
    Action, AnalogTarget, Command, Crosshair, JumpMode, Light, Magnifier, Mapping, Mode, Profile,
    RadialMenu, StickConfig, Sticks, TriggerEffect, Triggers, Vibration, VirtualMotion,
    VirtualPadType, VirtualSticks,
};

use crate::Diagnostics;

/// Check a profile for mistakes the engine would silently tolerate.
pub fn validate(p: &Profile) -> Diagnostics {
    let mut d = Diagnostics::default();
    let declared: Vec<u8> = p.layers.iter().map(|l| l.id).collect();
    check_layer_ids(&mut d, p);
    check_mappings(&mut d, p, &declared);
    check_devices(&mut d, p);
    check_routes(&mut d, p);
    check_crosshairs(&mut d, p);
    check_radial(&mut d, p);
    check_feedback(&mut d, p);
    check_layers(&mut d, p);
    check_reach(&mut d, p);
    check_outputs(&mut d, p);
    d
}

/// The diagnostic path of a mapping: its index and trigger.
fn mapping_path(i: usize, m: &Mapping) -> String {
    format!("mappings[{i}] ({})", m.trigger)
}

/// Whether a mapping on another layer jumps to layer `l`, so `l` can become active.
fn reached(p: &Profile, l: u8) -> bool {
    p.mappings
        .iter()
        .any(|m| m.layer != l && m.jump.as_ref().is_some_and(|j| j.layer == l))
}

/// Layers nothing changes to, and switches into layers that nothing leaves again.
fn check_reach(d: &mut Diagnostics, p: &Profile) {
    let mut used: Vec<u8> = p
        .layers
        .iter()
        .map(|l| l.id)
        .chain(p.mappings.iter().map(|m| m.layer))
        .filter(|&l| l != 0)
        .collect();
    used.sort_unstable();
    used.dedup();
    for l in used {
        if reached(p, l) {
            continue;
        }
        let path = match p.layers.iter().position(|x| x.id == l) {
            Some(i) => format!("layers[{i}]"),
            None => p
                .mappings
                .iter()
                .enumerate()
                .find(|(_, m)| m.layer == l)
                .map(|(i, m)| mapping_path(i, m))
                .unwrap_or_default(),
        };
        d.warn(
            path,
            format!("nothing changes to layer {l}, so its mappings never apply"),
        );
    }
    for (i, m) in p.mappings.iter().enumerate() {
        let Some(j) = &m.jump else { continue };
        // A hold or toggle jump comes back by itself; a switch to the base layer is the way back.
        if j.mode != JumpMode::Switch || j.layer == 0 || j.layer == m.layer {
            continue;
        }
        let target = j.layer;
        let leaves = p.mappings.iter().any(|o| {
            p.applies_on(o, target)
                && o.jump.as_ref().is_some_and(|oj| {
                    oj.layer != target && matches!(oj.mode, JumpMode::Switch | JumpMode::Toggle)
                })
        });
        if !leaves {
            d.warn(
                mapping_path(i, m),
                format!("after this switch to layer {target}, no mapping there leaves it again"),
            );
        }
    }
}

/// Controller output that the config's virtual controller cannot send.
fn check_outputs(d: &mut Diagnostics, p: &Profile) {
    let pad = p.output.virtual_pad;
    for (i, m) in p.mappings.iter().enumerate() {
        let to_pad = m.actions.iter().any(|a| {
            matches!(
                a,
                Action::Pad { .. }
                    | Action::Stick { .. }
                    | Action::Trigger { .. }
                    | Action::Touch { .. }
                    | Action::Shake { .. }
            )
        });
        let ds4_only = m
            .actions
            .iter()
            .any(|a| matches!(a, Action::Touch { .. } | Action::Shake { .. }));
        if pad == VirtualPadType::None && to_pad {
            d.warn(
                mapping_path(i, m),
                "the config has no virtual controller, so this mapping's controller output goes nowhere",
            );
        } else if pad == VirtualPadType::Xbox360 && ds4_only {
            d.warn(
                mapping_path(i, m),
                "touchpad gestures and shakes need a virtual DualShock 4; an Xbox 360 controller has neither",
            );
        }
    }
    if pad == VirtualPadType::None {
        for (i, r) in p.analog.iter().enumerate() {
            if matches!(r.target, AnalogTarget::LeftStick | AnalogTarget::RightStick) {
                d.warn(
                    format!("analog[{i}]"),
                    "the config has no virtual controller, so this route's stick output goes nowhere",
                );
            }
        }
    }
}

/// Layer 0 is the base layer; every other one is declared once.
fn check_layer_ids(d: &mut Diagnostics, p: &Profile) {
    for (i, l) in p.layers.iter().enumerate() {
        if l.id == 0 {
            d.error(
                format!("layers[{i}]"),
                "layer 0 is the base layer and must not be declared",
            );
        }
        if p.layers.iter().filter(|o| o.id == l.id).count() > 1 {
            d.error(
                format!("layers[{i}]"),
                format!("layer {} is declared twice", l.id),
            );
        }
    }
}

fn check_mappings(d: &mut Diagnostics, p: &Profile, declared: &[u8]) {
    for (i, m) in p.mappings.iter().enumerate() {
        let path = mapping_path(i, m);
        let key = m.trigger.normalized();
        if key.0.len() < m.trigger.0.len() {
            d.info(&path, "a control is listed twice; it counts once");
        }
        if let Some(j) = p.mappings[..i].iter().position(|o| {
            o.layer == m.layer && o.activator == m.activator && o.trigger.normalized() == key
        }) {
            d.warn(
                &path,
                format!("mappings[{j}] has the same trigger and activator on this layer; both run"),
            );
        }
        if m.trigger.0.len() > 4 {
            d.warn(
                &path,
                "combos with more than four inputs are hard to press reliably",
            );
        }
        if m.random_ms.is_some_and(|r| r > 10_000) {
            d.error(&path, "random_ms must be at most 10000");
        } else if m.random_ms.is_some()
            && !matches!(
                m.mode,
                Mode::Turbo | Mode::Repeat | Mode::WhileHeld | Mode::Toggle
            )
        {
            d.info(
                &path,
                "random_ms only spreads turbo and repeat cycles; this mode has none",
            );
        }
        if m.layer != 0 && !declared.contains(&m.layer) {
            d.info(
                &path,
                format!(
                    "layer {} is not declared; it inherits the base layer",
                    m.layer
                ),
            );
        }
        if let Some(j) = &m.jump
            && j.layer != 0
            && !declared.contains(&j.layer)
            && !p.mappings.iter().any(|o| o.layer == j.layer)
        {
            d.warn(
                &path,
                format!("jumps to layer {} which has no mappings", j.layer),
            );
        }
        if let Some(j) = &m.jump {
            if j.postpone && j.delay_ms.is_none_or(|d| d == 0) {
                d.info(
                    &path,
                    "postpone only matters with a jump delay; the actions start at once",
                );
            }
            if j.mode == JumpMode::Toggle && p.layer(j.layer).is_some_and(|l| l.unlock.is_some()) {
                d.warn(
                    &path,
                    "unlock layers are meant to be held; toggled on, the keyboard/mouse stay unmuted until it is toggled off",
                );
            }
        }
        for a in &m.actions {
            check_action(d, &path, a, p);
        }
        if m.actions.len() > 4096 {
            d.error(&path, "macro has more than 4096 steps");
        }
        match m.effective_mode() {
            played if played == m.mode => {}
            Mode::Hold => d.warn(
                &path,
                "a sequence with break steps cannot use turbo or toggle; it is held instead",
            ),
            _ => d.warn(
                &path,
                "a sequence with break steps runs each segment once; repeating is ignored",
            ),
        }
    }
}

/// The values of one action.
fn check_action(d: &mut Diagnostics, path: &str, a: &Action, p: &Profile) {
    match a {
        Action::Pause { pause } if *pause > 60_000 => {
            d.warn(path, "pause longer than a minute");
        }
        // The engine splits at every break step, whatever its value.
        Action::Break { r#break: false } => {
            d.error(
                path,
                "`break = false` would still split the sequence; remove it",
            );
        }
        Action::Command {
            command: Command::Unsupported(name),
        } => d.warn(
            path,
            format!("command `{name}` is not supported and does nothing"),
        ),
        Action::Stick { x, y, .. } if x.abs() > 1.0 || y.abs() > 1.0 => {
            d.warn(path, "stick deflection outside [-1, 1] is clamped");
        }
        Action::Turn { turn } if !turn.is_finite() || turn.abs() > 3600.0 => {
            d.error(path, "a turn must be within ±3600 degrees");
        }
        Action::Touch { ms: Some(ms), .. } if !(10..=10_000).contains(ms) => {
            d.error(path, "a touch gesture takes 10–10000 ms");
        }
        Action::MouseTo { mouse_to } if mouse_to.iter().any(|v| !(0.0..=1.0).contains(v)) => {
            d.error(path, "mouse_to coordinates must be in [0, 1]");
        }
        Action::MouseClip { mouse_clip } if mouse_clip.iter().any(|v| !(0.0..=1.0).contains(v)) => {
            d.error(path, "mouse_clip coordinates must be in [0, 1]");
        }
        Action::MouseMove { acceleration, .. } if !(0.0..=100.0).contains(acceleration) => {
            d.error(path, "mouse acceleration must be 0–100");
        }
        Action::Command {
            command: Command::Crosshair(n),
        } if *n == 0 || *n as usize > p.crosshairs.len() => d.warn(
            path,
            format!("there is no crosshair {n}; the command does nothing"),
        ),
        _ => {}
    }
}

/// The base layer's sticks, triggers and virtual sticks, the virtual motion and mouse, and
/// the gyro, touchpad and mouse as sources.
fn check_devices(d: &mut Diagnostics, p: &Profile) {
    check_analog(d, "", &p.sticks, &p.triggers, [true; 4]);
    check_virtual(d, "", &p.virtual_devices.sticks, [true; 2]);
    let motion = &p.virtual_devices.motion;
    if !(motion.shake_g > 0.0 && motion.shake_g <= 8.0) {
        d.error("virtual.motion.shake_g", "must be in (0, 8]");
    }
    if !VirtualMotion::SHAKE_MS.contains(&motion.shake_ms) {
        d.error("virtual.motion.shake_ms", "must be 10–2000 ms");
    }
    for (field, zones) in [
        ("gyro.motion_zones", p.gyro.motion_zones),
        ("gyro.tilt_zones", p.gyro.tilt_zones),
    ] {
        if !positive_increasing(zones) {
            d.error(field, "low, med and high must be positive and increase");
        }
    }
    if !(p.touchpad.centre > 0.0 && p.touchpad.centre < 1.0) {
        d.error("touchpad.centre", "must be in (0, 1)");
    }
    let mouse = &p.virtual_devices.mouse;
    if !(mouse.counts_per_turn > 0.0 && mouse.counts_per_turn <= 1_000_000.0) {
        d.error("virtual.mouse.counts_per_turn", "must be in (0, 1000000]");
    }
    if mouse.turn_ms > 5000 {
        d.error("virtual.mouse.turn_ms", "must be at most 5000 ms");
    }
    if !positive_increasing(p.mouse.zones) {
        d.error(
            "mouse.zones",
            "low, med and high must be positive and increase",
        );
    }
}

fn check_routes(d: &mut Diagnostics, p: &Profile) {
    for (i, r) in p.analog.iter().enumerate() {
        if !(0.0..1.0).contains(&r.snap) || r.noise_filter < 0.0 || r.acceleration < 0.0 {
            d.error(
                format!("analog[{i}]"),
                "snap must be in [0, 1); noise_filter and acceleration not negative",
            );
        }
        let stick = r.source.stick().is_some();
        // Flick routes use the deadzone as their threshold and ignore outer.
        if stick && r.target != AnalogTarget::Flick && !(r.outer > r.deadzone && r.outer <= 1.0) {
            d.error(format!("analog[{i}].outer"), "must be in (deadzone, 1]");
        }
        if r.target == AnalogTarget::Flick && !stick {
            d.warn(
                format!("analog[{i}]"),
                "flick stick reads a stick; this route does nothing",
            );
        }
    }
}

/// The light bar, the vibration and the base layer's trigger effects.
fn check_feedback(d: &mut Diagnostics, p: &Profile) {
    if let Some(l) = &p.lightbar {
        check_light(d, "lightbar", l);
    }
    let v = &p.vibration;
    if !(0.0..=1.0).contains(&v.left)
        || !(0.0..=1.0).contains(&v.right)
        || !Vibration::MIN_DELAY_MS.contains(&v.min_delay_ms)
    {
        d.error(
            "vibration",
            "motor strengths must be 0–1 and min_delay_ms 10–200",
        );
    }
    for (name, t) in [
        ("triggers.left", &p.triggers.left),
        ("triggers.right", &p.triggers.right),
    ] {
        if let Some(e) = &t.effect {
            check_effect(d, name, e);
        }
    }
}

/// What the layers change: trigger effects, light bars, then stick, trigger and virtual stick
/// settings (each over every layer, in that order).
fn check_layers(d: &mut Diagnostics, p: &Profile) {
    for (i, l) in p.layers.iter().enumerate() {
        for (side, t) in [("left", &l.triggers.left), ("right", &l.triggers.right)] {
            if let Some(e) = &t.effect {
                check_effect(d, &format!("layers[{i}].triggers.{side}"), e);
            }
        }
    }
    for (i, l) in p.layers.iter().enumerate() {
        if let Some(light) = &l.lightbar {
            check_light(d, &format!("layers[{i}].lightbar"), light);
        }
    }
    for (i, l) in p.layers.iter().enumerate() {
        // Only the sides a layer changes are checked again.
        let changed = [
            !l.sticks.left.is_empty(),
            !l.sticks.right.is_empty(),
            !l.triggers.left.is_empty(),
            !l.triggers.right.is_empty(),
        ];
        if changed.contains(&true) {
            let sticks = l.sticks.apply(&p.sticks);
            let triggers = l.triggers.apply(&p.triggers);
            check_analog(d, &format!("layers[{i}]."), &sticks, &triggers, changed);
        }
        let vs = &l.virtual_devices.sticks;
        let changed = [!vs.left.is_empty(), !vs.right.is_empty()];
        if changed.contains(&true) {
            let sticks = vs.apply(&p.virtual_devices.sticks);
            check_virtual(d, &format!("layers[{i}]."), &sticks, changed);
        }
    }
}

/// `#rrggbb`.
fn is_hex_color(s: &str) -> bool {
    Rgb::from_hex(s).is_some()
}

/// Parameter ranges of a trigger effect.
fn check_effect(d: &mut Diagnostics, path: &str, e: &TriggerEffect) {
    let ok = match *e {
        TriggerEffect::Off | TriggerEffect::Raw { .. } => true,
        TriggerEffect::Resistance { start, strength } => start <= 9 && (1..=8).contains(&strength),
        TriggerEffect::Weapon {
            start,
            end,
            strength,
        } => (2..=7).contains(&start) && end > start && end <= 8 && (1..=8).contains(&strength),
        TriggerEffect::Vibration {
            start,
            amplitude,
            frequency,
        } => start <= 9 && (1..=8).contains(&amplitude) && frequency >= 1,
    };
    if !ok {
        d.error(
            path,
            "trigger effect out of range: start 0–9 (weapon 2–7, end after start up to 8), strength and amplitude 1–8, frequency at least 1",
        );
    }
}

fn check_light(d: &mut Diagnostics, path: &str, l: &Light) {
    if l.colors.is_empty()
        || l.colors.len() > Light::MAX_COLORS
        || !l.colors.iter().all(|c| is_hex_color(c))
    {
        d.error(path, "a light bar has 1–10 colours written #rrggbb");
    }
    if !Light::PERIOD_MS.contains(&l.period_ms) {
        d.error(path, "period_ms must be 100–10000");
    }
}

fn check_radial(d: &mut Diagnostics, p: &Profile) {
    for (i, m) in p.radial_menus.iter().enumerate() {
        let path = format!("radial_menus[{i}]");
        if m.sectors.is_empty() || m.sectors.len() > 12 {
            d.error(&path, "a radial menu has 1–12 sectors");
        }
        if !(0.0..=1.0).contains(&m.tint)
            || !RadialMenu::SCALE.contains(&m.scale)
            || m.delay_ms > 5000
        {
            d.error(
                &path,
                "tint must be 0–1, scale 0.5–2 and delay_ms at most 5000",
            );
        }
        if m.layer == 0 {
            if !p
                .radial_menus
                .iter()
                .flat_map(|o| &o.sectors)
                .any(|s| s.submenu == Some(i as u8 + 1))
            {
                d.info(
                    &path,
                    "layer 0 opens no menu; this one is no one's sub-menu either",
                );
            }
        } else if !reached(p, m.layer) {
            d.warn(
                &path,
                format!(
                    "nothing activates layer {}, so the menu never opens",
                    m.layer
                ),
            );
        }
        if p.radial_menus[..i]
            .iter()
            .any(|o| o.layer == m.layer && m.layer != 0)
        {
            d.warn(
                &path,
                format!(
                    "another menu already opens with layer {}; this one never does",
                    m.layer
                ),
            );
        }
        for (k, s) in m.sectors.iter().enumerate() {
            let spath = format!("{path}.sectors[{k}]");
            if !s.color.is_empty() && !is_hex_color(&s.color) {
                d.error(&spath, "colours are written #rrggbb");
            }
            if let Some(n) = s.submenu
                && (n == 0 || n as usize > p.radial_menus.len() || n as usize == i + 1)
            {
                d.error(&spath, format!("there is no other menu {n} to open"));
            }
        }
    }
}

fn check_crosshairs(d: &mut Diagnostics, p: &Profile) {
    if p.crosshairs.len() > 10 {
        d.error("crosshairs", "at most ten crosshairs");
    }
    for (i, c) in p.crosshairs.iter().enumerate() {
        let path = format!("crosshairs[{i}]");
        if !Crosshair::SIZE.contains(&c.size)
            || !Crosshair::THICKNESS.contains(&c.thickness)
            || c.outline > Crosshair::MAX_OUTLINE
        {
            d.error(
                &path,
                "size must be 10–200, thickness 10–100 and outline 0–10",
            );
        }
        if !(0.0..=1.0).contains(&c.opacity) {
            d.error(&path, "opacity must be in [0, 1]");
        }
        if !is_hex_color(&c.color) || !is_hex_color(&c.outline_color) {
            d.error(&path, "colours are written #rrggbb");
        }
        if let Some(m) = c.magnifier
            && (!Magnifier::ZOOM.contains(&m.zoom) || !Magnifier::SIZE.contains(&m.size))
        {
            d.error(
                &path,
                "the magnifier zooms 1–16 times and takes 0.1–1 of the screen",
            );
        }
        if !c.visible && c.magnifier.is_none() {
            d.info(
                &path,
                "neither drawn nor magnifying; this crosshair shows nothing",
            );
        }
    }
}

/// Range and zone-order checks for one set of stick and trigger settings; `which` picks
/// left stick, right stick, left trigger and right trigger.
fn check_analog(
    d: &mut Diagnostics,
    prefix: &str,
    sticks: &Sticks,
    triggers: &Triggers,
    which: [bool; 4],
) {
    let sides = [("left", &sticks.left), ("right", &sticks.right)];
    for ((name, s), _) in sides.into_iter().zip(which).filter(|(_, on)| *on) {
        if !(0.0..1.0).contains(&s.deadzone) {
            d.error(
                format!("{prefix}sticks.{name}.deadzone"),
                "must be in [0, 1)",
            );
        }
        if !StickConfig::DIRECTION_WIDTH.contains(&s.direction_width) {
            d.error(
                format!("{prefix}sticks.{name}.direction_width"),
                "must be between 1 and 44 degrees",
            );
        }
        if !curve_in_range(&s.curve) {
            d.error(
                format!("{prefix}sticks.{name}.curve"),
                "points must be in [0, 1]",
            );
        }
        if let Some(msg) = stick_zone_problem(s) {
            d.error(format!("{prefix}sticks.{name}.zones"), msg);
        }
    }
    let sides = [("left", &triggers.left), ("right", &triggers.right)];
    for ((name, t), _) in sides.into_iter().zip(&which[2..]).filter(|(_, on)| **on) {
        if let Some(msg) = zone_problem(t.zone_thresholds()) {
            d.error(format!("{prefix}triggers.{name}.zones"), msg);
        }
    }
}

/// Range checks for the virtual sticks (`which`: left, right).
fn check_virtual(d: &mut Diagnostics, prefix: &str, sticks: &VirtualSticks, which: [bool; 2]) {
    let sides = [("left", &sticks.left), ("right", &sticks.right)];
    for ((name, s), _) in sides.into_iter().zip(which).filter(|(_, on)| *on) {
        let path = |field: &str| format!("{prefix}virtual.sticks.{name}.{field}");
        for (field, v) in [("initial", s.initial), ("max", s.max)] {
            if !(0.0..=1.0).contains(&v) {
                d.error(path(field), "must be in [0, 1]");
            }
        }
        for (field, v) in [("speed", s.speed), ("acceleration", s.acceleration)] {
            if !(0.0..=100.0).contains(&v) {
                d.error(path(field), "must be between 0 and 100");
            }
        }
        for (field, v) in [
            ("multiplier_x", s.multiplier_x),
            ("multiplier_y", s.multiplier_y),
        ] {
            if !(0.1..=5.0).contains(&v) {
                d.error(path(field), "must be between 0.1 and 5");
            }
        }
        if !curve_in_range(&s.curve) {
            d.error(path("curve"), "points must be in [0, 1]");
        }
        let j = &s.jitter;
        if !(0.0..=0.5).contains(&j.min) || !(0.0..=0.5).contains(&j.max) || j.min > j.max {
            d.error(
                path("jitter"),
                "min and max must be in [0, 0.5], min not above max",
            );
        }
        if !(1..=1000).contains(&j.delay_min_ms)
            || !(1..=1000).contains(&j.delay_max_ms)
            || j.delay_min_ms > j.delay_max_ms
        {
            d.error(
                path("jitter"),
                "delays must be 1–1000 ms, the shortest not above the longest",
            );
        }
    }
}

/// Every curve point is in [0, 1].
fn curve_in_range(curve: &[[f32; 2]]) -> bool {
    curve
        .iter()
        .all(|pt| (0.0..=1.0).contains(&pt[0]) && (0.0..=1.0).contains(&pt[1]))
}

/// Low, medium and high thresholds above 0 that increase.
fn positive_increasing([low, med, high]: [f32; 3]) -> bool {
    low > 0.0 && low < med && med < high
}

/// What is wrong with a stick's effective zones, across or along.
pub(crate) fn stick_zone_problem(s: &StickConfig) -> Option<&'static str> {
    let [low, med, high] = s.zone_thresholds();
    zone_problem([low.x, med.x, high.x]).or(zone_problem([low.y, med.y, high.y]))
}

/// What is wrong with a control's effective low, medium and high zone thresholds.
pub(crate) fn zone_problem([low, med, high]: [f32; 3]) -> Option<&'static str> {
    if !(low > 0.0 && high <= 1.0) {
        Some("thresholds must be in (0, 1]")
    } else if !(low < med && med < high) {
        Some("low, med and high must increase")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, parse_toml, to_toml};

    #[test]
    fn validation_rejects_break_false() {
        let src = "[[mappings]]\ntrigger = \"A\"\nmode = \"once\"\nactions = [{ key = \"A\" }, { break = false }, { key = \"B\" }]\n";
        let d = validate(&parse_toml(src).unwrap());
        assert!(d.has_errors(), "{d:?}");
        assert!(
            d.iter().any(|d| d.message.contains("break = false")),
            "{d:?}"
        );
        let ok = src.replace("false", "true");
        assert!(!validate(&parse_toml(&ok).unwrap()).has_errors());
    }

    /// Diagnostics come in a fixed order: layer ids, mappings, devices, routes, crosshairs,
    /// radial menus, feedback, then what the layers change.
    #[test]
    fn validation_reports_in_a_fixed_order() {
        let src = r#"
[vibration]
left = 2.0

[virtual.motion]
shake_g = 9.0

[[layers]]
id = 0

[[layers]]
id = 1
lightbar = { colors = ["red"] }

[[mappings]]
trigger = "A"
random_ms = 20000

[[analog]]
source = "mouse"
target = "right_stick"
snap = 2.0

[[crosshairs]]
opacity = 2.0

[[radial_menus]]
layer = 0
"#;
        let d = validate(&parse_toml(src).unwrap());
        let mut paths: Vec<&str> = d.iter().map(|x| x.path.as_str()).collect();
        paths.dedup();
        assert_eq!(
            paths,
            [
                "layers[0]",
                "mappings[0] (A)",
                "virtual.motion.shake_g",
                "analog[0]",
                "crosshairs[0]",
                "radial_menus[0]",
                "vibration",
                "layers[1].lightbar",
                "layers[1]"
            ]
        );
    }

    /// The warnings of a profile, as `path: message`.
    fn warnings(src: &str) -> Vec<String> {
        validate(&parse_toml(src).unwrap())
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .map(|d| format!("{}: {}", d.path, d.message))
            .collect()
    }

    #[test]
    fn validation_flags_mappings_that_run_twice() {
        let w = warnings(
            r#"
[[mappings]]
trigger = ["LB", "RB"]
actions = [{ key = "A" }]
[[mappings]]
trigger = ["RB", "LB"]
actions = [{ key = "B" }]
[[mappings]]
trigger = ["RB", "LB"]
activator = "long"
actions = [{ key = "C" }]
[[mappings]]
layer = 1
trigger = ["LB", "RB"]
actions = [{ key = "D" }]
[[mappings]]
trigger = "X"
jump = { layer = 1 }
"#,
        );
        assert_eq!(
            w,
            [
                "mappings[1] (RB+LB): mappings[0] has the same trigger and activator on this layer; both run"
            ]
        );
        let d = validate(
            &parse_toml("[[mappings]]\ntrigger = [\"A\", \"A\"]\nactions = [{ key = \"A\" }]\n")
                .unwrap(),
        );
        assert!(
            d.iter().any(|d| d.message.contains("listed twice")),
            "{d:?}"
        );
    }

    #[test]
    fn validation_flags_layers_nothing_reaches() {
        let w = warnings(
            r#"
[[layers]]
id = 2
[[mappings]]
layer = 1
trigger = "A"
actions = [{ key = "A" }]
[[mappings]]
layer = 3
trigger = "B"
actions = [{ key = "B" }]
[[mappings]]
layer = 1
trigger = "X"
jump = { layer = 3, mode = "toggle" }
"#,
        );
        assert_eq!(
            w,
            [
                "mappings[0] (A): nothing changes to layer 1, so its mappings never apply",
                "layers[0]: nothing changes to layer 2, so its mappings never apply",
            ]
        );
    }

    #[test]
    fn the_way_back_of_a_hold_jump_shadows_base_mappings_as_in_the_engine() {
        // On layer 1 the engine maps LB itself (releasing it goes back), so the base layer's
        // double tap on LB is not taken over and is no way out of the switch to layer 1.
        let w = warnings(
            r#"
[[mappings]]
trigger = "LB"
jump = { layer = 1, mode = "hold" }
[[mappings]]
trigger = "LB"
activator = "double"
jump = { layer = 0, mode = "switch" }
[[mappings]]
trigger = "Back"
jump = { layer = 1, mode = "switch" }
[[mappings]]
layer = 1
trigger = "A"
actions = [{ key = "A" }]
"#,
        );
        assert_eq!(
            w,
            ["mappings[2] (Back): after this switch to layer 1, no mapping there leaves it again"]
        );
    }

    #[test]
    fn a_menu_on_a_declared_layer_still_needs_a_way_in() {
        let w = warnings(
            "[[layers]]\nid = 2\n[[radial_menus]]\nlayer = 2\n[[radial_menus.sectors]]\nactions = [{ key = \"A\" }]\n",
        );
        assert!(
            w.iter().any(|x| x
                == "radial_menus[0]: nothing activates layer 2, so the menu never opens"),
            "{w:?}"
        );
    }

    #[test]
    fn validation_flags_switches_without_a_way_out() {
        let stuck = r#"
[[mappings]]
trigger = "LB"
jump = { layer = 1, mode = "switch" }
[[mappings]]
layer = 1
trigger = "A"
actions = [{ key = "A" }]
"#;
        assert_eq!(
            warnings(stuck),
            ["mappings[0] (LB): after this switch to layer 1, no mapping there leaves it again"]
        );
        // A switch back on the layer, or one of the base layer's the layer takes over, is a way out.
        let back = format!(
            "{stuck}[[mappings]]\nlayer = 1\ntrigger = \"B\"\njump = {{ layer = 0, mode = \"switch\" }}\n"
        );
        assert!(warnings(&back).is_empty(), "{:?}", warnings(&back));
        let inherited = format!(
            "{stuck}[[mappings]]\ntrigger = \"Start\"\njump = {{ layer = 0, mode = \"switch\" }}\n"
        );
        assert!(
            warnings(&inherited).is_empty(),
            "{:?}",
            warnings(&inherited)
        );
        // ... unless the layer does not inherit, or maps that trigger itself.
        let blocked = format!("{inherited}[[layers]]\nid = 1\ninherit = false\n");
        assert_eq!(warnings(&blocked).len(), 1, "{:?}", warnings(&blocked));
        let shadowed = format!(
            "{inherited}[[mappings]]\nlayer = 1\ntrigger = \"Start\"\nactions = [{{ key = \"S\" }}]\n"
        );
        assert_eq!(warnings(&shadowed).len(), 1, "{:?}", warnings(&shadowed));
        // Hold and toggle jumps come back by themselves.
        let hold = stuck.replace("mode = \"switch\"", "mode = \"toggle\"");
        assert!(warnings(&hold).is_empty(), "{:?}", warnings(&hold));
    }

    #[test]
    fn validation_flags_output_the_virtual_pad_cannot_send() {
        let none = r#"
[output]
virtual_pad = "none"
[[mappings]]
trigger = "A"
actions = [{ pad = "A" }]
[[mappings]]
trigger = "B"
actions = [{ key = "B" }]
[[analog]]
source = "mouse"
target = "right_stick"
[[analog]]
source = "right_stick"
target = "mouse"
"#;
        assert_eq!(
            warnings(none),
            [
                "mappings[0] (A): the config has no virtual controller, so this mapping's controller output goes nowhere",
                "analog[0]: the config has no virtual controller, so this route's stick output goes nowhere",
            ]
        );
        let swipe = "[[mappings]]\ntrigger = \"A\"\nactions = [{ touch = \"swipe_up\" }]\n";
        assert_eq!(warnings(swipe).len(), 1, "{:?}", warnings(swipe));
        let ds4 = format!("[output]\nvirtual_pad = \"dual_shock4\"\n{swipe}");
        assert!(warnings(&ds4).is_empty(), "{:?}", warnings(&ds4));
    }

    #[test]
    fn validation_flags_bad_layers() {
        let p = parse_toml("[[layers]]\nid = 0\n").unwrap();
        assert!(validate(&p).has_errors());
    }

    #[test]
    fn validation_checks_zone_order() {
        let ok = "[sticks.left]\nzones = { low = 0.2, med = [0.4, 0.5] }\n[triggers.left]\nzones = { low = 0.1, high = 0.9 }\n";
        assert!(!validate(&parse_toml(ok).unwrap()).has_errors());
        for bad in [
            "[sticks.left]\nzones = { low = 0.5, med = 0.4 }\n",
            "[sticks.right]\nzones = { low = 0.2, high = [0.9, 0.3] }\n",
            "[triggers.right]\nzones = { low = 0.0 }\n",
            "[triggers.right]\nzones = { low = 0.1, med = 0.5, high = 1.2 }\n",
        ] {
            assert!(validate(&parse_toml(bad).unwrap()).has_errors(), "{bad}");
        }
    }

    #[test]
    fn layer_analog_settings_round_trip_and_validate() {
        let text = "[[layers]]\nid = 1\ninherit = true\n\n[layers.sticks.left]\ndeadzone = 0.3\n\n[layers.triggers.right]\nfull_pull = 0.9\n";
        let p = parse_toml(text).unwrap();
        assert_eq!(p.layers[0].sticks.left.deadzone, Some(0.3));
        assert_eq!(p.layers[0].triggers.right.full_pull, Some(0.9));
        assert!(!validate(&p).has_errors());
        let again = parse_toml(&to_toml(&p).unwrap()).unwrap();
        assert_eq!(again.layers, p.layers);
        assert!(
            !to_toml(&Profile::default())
                .unwrap()
                .contains("sticks.left"),
            "no empty patches"
        );

        let bad = parse_toml("[[layers]]\nid = 1\ninherit = true\n\n[layers.sticks.right]\nzones = { low = 0.9, med = 0.5 }\n").unwrap();
        let d = validate(&bad);
        assert!(
            d.iter().any(|x| x.path == "layers[0].sticks.right.zones"),
            "{d:?}"
        );
    }

    #[test]
    fn virtual_stick_settings_round_trip_and_validate() {
        let text = "[virtual.sticks.left]\ninitial = 0.25\nspeed = 1.0\n\n[[layers]]\nid = 1\ninherit = true\n\n[layers.virtual.sticks.right]\nmax = 0.5\n";
        let p = parse_toml(text).unwrap();
        assert_eq!(p.virtual_devices.sticks.left.initial, 0.25);
        assert_eq!(p.virtual_devices.sticks.right.initial, 1.0, "default");
        assert_eq!(p.layers[0].virtual_devices.sticks.right.max, Some(0.5));
        assert!(!validate(&p).has_errors());
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);

        let bad = parse_toml("[[layers]]\nid = 1\ninherit = true\n\n[layers.virtual.sticks.left]\nmultiplier_x = 9.0\n").unwrap();
        let d = validate(&bad);
        assert!(
            d.iter()
                .any(|x| x.path == "layers[0].virtual.sticks.left.multiplier_x"),
            "{d:?}"
        );
    }

    #[test]
    fn jitter_and_shake_settings_round_trip() {
        let text = "[virtual.motion]\nshake_g = 3.0\nshake_ms = 200\n\n[virtual.sticks.right.jitter]\nenabled = true\npattern = \"cross\"\n\n[[mappings]]\ntrigger = \"Y\"\nactions = [{ shake = \"+z\" }]\n";
        let p = parse_toml(text).unwrap();
        assert_eq!(p.virtual_devices.motion.shake_g, 3.0);
        assert!(p.virtual_devices.sticks.right.jitter.enabled);
        assert!(!validate(&p).has_errors());
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        let bad = parse_toml("[virtual.sticks.left.jitter]\nmin = 0.2\nmax = 0.1\n").unwrap();
        assert!(validate(&bad).has_errors());
    }

    #[test]
    fn flick_routes_and_turns() {
        let text = "[virtual.mouse]\ncounts_per_turn = 12000.0\n\n[[analog]]\nsource = \"right_stick\"\ntarget = \"flick\"\ndeadzone = 0.6\n\n[[mappings]]\ntrigger = \"Y\"\nactions = [{ turn = 180.0 }]\n";
        let p = parse_toml(text).unwrap();
        assert!(validate(&p).iter().next().is_none(), "{:?}", validate(&p));
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        let gyro = parse_toml("[[analog]]\nsource = \"gyro\"\ntarget = \"flick\"\n").unwrap();
        assert!(validate(&gyro).iter().any(|d| d.path == "analog[0]"));
    }

    #[test]
    fn gyro_settings_round_trip_and_validate() {
        let text = "[gyro]\nmode = \"tilt\"\nsteering = \"yaw\"\ninitially_on = false\nspace = \"player\"\ntilt_zones = [5.0, 15.0, 30.0]\n\n[[mappings]]\ntrigger = \"GyroLeanLeftHigh\"\nactions = [{ command = \"reset_tilt\" }]\n";
        let p = parse_toml(text).unwrap();
        assert!(!validate(&p).has_errors());
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        let bad = parse_toml("[gyro]\nmotion_zones = [60.0, 50.0, 500.0]\n").unwrap();
        assert!(validate(&bad).iter().any(|d| d.path == "gyro.motion_zones"));
    }

    #[test]
    fn touchpad_settings_round_trip() {
        let text = "[touchpad]\nsplit = true\nzones = \"grid\"\nclick_required = true\n\n[[mappings]]\ntrigger = \"TrackpadRightDownLeft\"\nactions = [{ key = \"A\" }]\n\n[[mappings]]\ntrigger = \"TouchZoomIn\"\nactions = [{ key = \"B\" }]\n";
        let p = parse_toml(text).unwrap();
        assert!(!validate(&p).has_errors());
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        assert!(validate(&parse_toml("[touchpad]\ncentre = 1.5\n").unwrap()).has_errors());
    }
}
