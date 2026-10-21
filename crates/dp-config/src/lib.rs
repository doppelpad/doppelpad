//! Loading, saving and validating Doppelpad profiles, and importing JSON profiles
//! (schema 3.0).

pub mod bundle;
pub mod compat;
pub mod diag;
pub mod library;
pub mod templates;

use std::path::{Path, PathBuf};

pub use compat::ImportError;
pub use diag::{Diagnostic, Diagnostics, Severity};
use dp_core::profile::{
    Action, AnalogSource, AnalogTarget, Command, JumpMode, Light, Mode, Profile, Sticks,
    TriggerEffect, Triggers, VirtualSticks,
};

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Toml(PathBuf, #[source] toml::de::Error),
    #[error("{0}: {1}")]
    Import(PathBuf, #[source] ImportError),
}

/// Profile file formats.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    /// Native Doppelpad TOML.
    Toml,
    /// JSON profile (schema 3.0), imported.
    Json,
}

fn is_toml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
}

impl Format {
    /// `.toml` files are native profiles; any other file whose content is a JSON
    /// object is imported, whatever its extension.
    pub fn detect(path: &Path, content: &str) -> Format {
        let json = content
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('{');
        if json && !is_toml_path(path) {
            Format::Json
        } else {
            Format::Toml
        }
    }
}

/// Parse a native TOML profile.
pub fn parse_toml(src: &str) -> Result<Profile, toml::de::Error> {
    toml::from_str(src)
}

/// Serialize a profile as TOML. Arrays of values stay on one line (`trigger = ["LB", "RB"]`,
/// `curve = [[0.3, 0.15], [0.7, 0.55]]`) like in hand-written profiles; the "pretty" style
/// would put every element, even of a curve point, on a line of its own.
pub fn to_toml(p: &Profile) -> Result<String, toml::ser::Error> {
    let body = toml::to_string(p)?;
    Ok(format!(
        "# Doppelpad profile — https://github.com/doppelpad/doppelpad\n{body}"
    ))
}

/// Load a profile from disk, importing JSON profiles. The diagnostics include
/// import notes and [`validate`] results.
pub fn load(path: &Path) -> Result<(Profile, Diagnostics), LoadError> {
    let src = std::fs::read_to_string(path).map_err(|e| LoadError::Io(path.into(), e))?;
    let (mut profile, mut diags) = match Format::detect(path, &src) {
        Format::Toml => (
            parse_toml(&src).map_err(|e| LoadError::Toml(path.into(), e))?,
            Diagnostics::default(),
        ),
        Format::Json => compat::import(&src).map_err(|e| LoadError::Import(path.into(), e))?,
    };
    if profile.name.is_empty() {
        profile.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile")
            .to_owned();
    }
    diags.0.extend(validate(&profile).0);
    Ok((profile, diags))
}

/// Check a profile for mistakes the engine would silently tolerate.
pub fn validate(p: &Profile) -> Diagnostics {
    let mut d = Diagnostics::default();
    let declared: Vec<u8> = p.layers.iter().map(|l| l.id).collect();
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
    for (i, m) in p.mappings.iter().enumerate() {
        let path = format!("mappings[{i}] ({})", m.trigger);
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
            if j.mode == JumpMode::Toggle
                && p.layers
                    .iter()
                    .any(|l| l.id == j.layer && l.unlock.is_some())
            {
                d.warn(
                    &path,
                    "unlock layers are meant to be held; toggled on, the keyboard/mouse stay unmuted until it is toggled off",
                );
            }
        }
        for a in &m.actions {
            match a {
                Action::Pause { pause } if *pause > 60_000 => {
                    d.warn(&path, "pause longer than a minute");
                }
                Action::Command {
                    command: Command::Unsupported(name),
                } => d.warn(
                    &path,
                    format!("command `{name}` is not supported and does nothing"),
                ),
                Action::Stick { x, y, .. } if x.abs() > 1.0 || y.abs() > 1.0 => {
                    d.warn(&path, "stick deflection outside [-1, 1] is clamped");
                }
                Action::Turn { turn } if !turn.is_finite() || turn.abs() > 3600.0 => {
                    d.error(&path, "a turn must be within ±3600 degrees");
                }
                Action::Touch { ms: Some(ms), .. } if !(10..=10_000).contains(ms) => {
                    d.error(&path, "a touch gesture takes 10–10000 ms");
                }
                Action::MouseTo { mouse_to }
                    if mouse_to.iter().any(|v| !(0.0..=1.0).contains(v)) =>
                {
                    d.error(&path, "mouse_to coordinates must be in [0, 1]");
                }
                Action::MouseClip { mouse_clip }
                    if mouse_clip.iter().any(|v| !(0.0..=1.0).contains(v)) =>
                {
                    d.error(&path, "mouse_clip coordinates must be in [0, 1]");
                }
                Action::MouseMove { acceleration, .. } if !(0.0..=100.0).contains(acceleration) => {
                    d.error(&path, "mouse acceleration must be 0–100");
                }
                Action::Command {
                    command: Command::Crosshair(n),
                } if *n == 0 || *n as usize > p.crosshairs.len() => d.warn(
                    &path,
                    format!("there is no crosshair {n}; the command does nothing"),
                ),
                _ => {}
            }
        }
        if m.actions.len() > 4096 {
            d.error(&path, "macro has more than 4096 steps");
        }
        if m.actions.iter().any(Action::is_break) {
            match m.mode {
                Mode::Turbo | Mode::Toggle => d.warn(
                    &path,
                    "a sequence with break steps cannot use turbo or toggle; it is held instead",
                ),
                Mode::Repeat | Mode::WhileHeld => d.warn(
                    &path,
                    "a sequence with break steps runs each segment once; repeating is ignored",
                ),
                Mode::Hold | Mode::Once => {}
            }
        }
    }
    check_analog(&mut d, "", &p.sticks, &p.triggers, [true; 4]);
    check_virtual(&mut d, "", &p.virtual_devices.sticks, [true; 2]);
    let motion = &p.virtual_devices.motion;
    if !(motion.shake_g > 0.0 && motion.shake_g <= 8.0) {
        d.error("virtual.motion.shake_g", "must be in (0, 8]");
    }
    if !(10..=2000).contains(&motion.shake_ms) {
        d.error("virtual.motion.shake_ms", "must be 10–2000 ms");
    }
    for (field, [low, med, high]) in [
        ("gyro.motion_zones", p.gyro.motion_zones),
        ("gyro.tilt_zones", p.gyro.tilt_zones),
    ] {
        if !(low > 0.0 && low < med && med < high) {
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
    let [low, med, high] = p.mouse.zones;
    if !(low > 0.0 && low < med && med < high) {
        d.error(
            "mouse.zones",
            "low, med and high must be positive and increase",
        );
    }
    for (i, r) in p.analog.iter().enumerate() {
        if !(0.0..1.0).contains(&r.snap) || r.noise_filter < 0.0 || r.acceleration < 0.0 {
            d.error(
                format!("analog[{i}]"),
                "snap must be in [0, 1); noise_filter and acceleration not negative",
            );
        }
        let stick = matches!(r.source, AnalogSource::LeftStick | AnalogSource::RightStick);
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
    check_crosshairs(&mut d, p);
    check_radial(&mut d, p);
    if let Some(l) = &p.lightbar {
        check_light(&mut d, "lightbar", l);
    }
    let v = &p.vibration;
    if !(0.0..=1.0).contains(&v.left)
        || !(0.0..=1.0).contains(&v.right)
        || !(10..=200).contains(&v.min_delay_ms)
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
            check_effect(&mut d, name, e);
        }
    }
    for (i, l) in p.layers.iter().enumerate() {
        for (side, t) in [("left", &l.triggers.left), ("right", &l.triggers.right)] {
            if let Some(e) = &t.effect {
                check_effect(&mut d, &format!("layers[{i}].triggers.{side}"), e);
            }
        }
    }
    for (i, l) in p.layers.iter().enumerate() {
        if let Some(light) = &l.lightbar {
            check_light(&mut d, &format!("layers[{i}].lightbar"), light);
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
            check_analog(
                &mut d,
                &format!("layers[{i}]."),
                &sticks,
                &triggers,
                changed,
            );
        }
        let vs = &l.virtual_devices.sticks;
        let changed = [!vs.left.is_empty(), !vs.right.is_empty()];
        if changed.contains(&true) {
            let sticks = vs.apply(&p.virtual_devices.sticks);
            check_virtual(&mut d, &format!("layers[{i}]."), &sticks, changed);
        }
    }
    d
}

/// `#rrggbb`.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
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
    if l.colors.is_empty() || l.colors.len() > 10 || !l.colors.iter().all(|c| is_hex_color(c)) {
        d.error(path, "a light bar has 1–10 colours written #rrggbb");
    }
    if !(100..=10_000).contains(&l.period_ms) {
        d.error(path, "period_ms must be 100–10000");
    }
}

fn check_radial(d: &mut Diagnostics, p: &Profile) {
    let declared: Vec<u8> = p.layers.iter().map(|l| l.id).collect();
    for (i, m) in p.radial_menus.iter().enumerate() {
        let path = format!("radial_menus[{i}]");
        if m.sectors.is_empty() || m.sectors.len() > 12 {
            d.error(&path, "a radial menu has 1–12 sectors");
        }
        if !(0.0..=1.0).contains(&m.tint) || !(0.5..=2.0).contains(&m.scale) || m.delay_ms > 5000 {
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
        } else if !declared.contains(&m.layer)
            && !p
                .mappings
                .iter()
                .any(|o| o.jump.as_ref().is_some_and(|j| j.layer == m.layer))
        {
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
        if !(10..=200).contains(&c.size) || !(10..=100).contains(&c.thickness) || c.outline > 10 {
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
            && (!(1.0..=16.0).contains(&m.zoom) || !(0.1..=1.0).contains(&m.size))
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
        if !(1.0..=44.0).contains(&s.direction_width) {
            d.error(
                format!("{prefix}sticks.{name}.direction_width"),
                "must be between 1 and 44 degrees",
            );
        }
        if s.curve
            .iter()
            .any(|pt| !(0.0..=1.0).contains(&pt[0]) || !(0.0..=1.0).contains(&pt[1]))
        {
            d.error(
                format!("{prefix}sticks.{name}.curve"),
                "points must be in [0, 1]",
            );
        }
        let [low, med, high] = s.zone_thresholds();
        let problem = zone_problem([low.x, med.x, high.x]).or(zone_problem([low.y, med.y, high.y]));
        if let Some(msg) = problem {
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
        if s.curve
            .iter()
            .any(|pt| !(0.0..=1.0).contains(&pt[0]) || !(0.0..=1.0).contains(&pt[1]))
        {
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

/// What is wrong with a control's effective low, medium and high zone thresholds.
fn zone_problem([low, med, high]: [f32; 3]) -> Option<&'static str> {
    if !(low > 0.0 && high <= 1.0) {
        Some("thresholds must be in (0, 1]")
    } else if !(low < med && med < high) {
        Some("low, med and high must increase")
    } else {
        None
    }
}

/// Per-user configuration directory (`%APPDATA%\doppelpad` or `~/.config/doppelpad`).
pub fn config_dir() -> PathBuf {
    let base = if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata)
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        return PathBuf::from("doppelpad");
    };
    base.join("doppelpad")
}

/// Default directory for profiles.
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Per-user directory for log files: `%LOCALAPPDATA%\doppelpad\logs`, or
/// `$XDG_STATE_HOME/doppelpad/logs` (default `~/.local/state/doppelpad/logs`). Local
/// rather than roaming: logs describe this machine.
pub fn log_dir() -> PathBuf {
    let base = if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local)
    } else if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local").join("state")
    } else {
        return PathBuf::from("doppelpad").join("logs");
    };
    base.join("doppelpad").join("logs")
}

/// Whether a file looks like a profile: `.toml`, or content starting with a JSON object.
pub(crate) fn is_profile_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if is_toml_path(path) {
        return true;
    }
    let mut head = [0u8; 64];
    let n = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&head[..n]);
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('{')
}

/// All loadable profiles in `dir` (TOML and importable JSON), with load errors reported
/// separately.
pub fn list_profiles(dir: &Path) -> (Vec<(PathBuf, Profile)>, Vec<LoadError>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (ok, errors);
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if !is_profile_file(&path) {
            continue;
        }
        match load(&path) {
            Ok((p, _)) => ok.push((path, p)),
            Err(e) => errors.push(e),
        }
    }
    (ok, errors)
}

/// The profile whose `processes` list contains `exe` (case-insensitive).
pub fn profile_for_process<'a>(
    profiles: &'a [(PathBuf, Profile)],
    exe: &str,
) -> Option<&'a (PathBuf, Profile)> {
    profiles
        .iter()
        .find(|(_, p)| p.processes.iter().any(|n| n.eq_ignore_ascii_case(exe)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_through_to_toml() {
        let src = "name = \"x\"\n[[mappings]]\ntrigger = [\"LB\", \"RB\"]\nactions = [{ key = \"Escape\" }]\n";
        let p = parse_toml(src).unwrap();
        let again = parse_toml(&to_toml(&p).unwrap()).unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn to_toml_keeps_value_arrays_on_one_line() {
        let src = r#"
name = "x"
processes = ["a.exe", "b.exe"]

[sticks.right]
curve = [[0.3, 0.15], [0.7, 0.55]]

[[mappings]]
trigger = ["LB", "RB"]
actions = [{ key = "Escape" }, { pause = 20 }]

[[analog]]
source = "gyro"
target = "right_stick"
while_held = ["LT", "Mouse:Right"]
"#;
        let p = parse_toml(src).unwrap();
        let text = to_toml(&p).unwrap();
        for line in [
            r#"processes = ["a.exe", "b.exe"]"#,
            "curve = [[0.3, 0.15], [0.7, 0.55]]",
            r#"trigger = ["LB", "RB"]"#,
            r#"while_held = ["LT", "Mouse:Right"]"#,
            "[[mappings]]",
            "[[mappings.actions]]",
            "[[analog]]",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing `{line}` in\n{text}"
            );
        }
        assert_eq!(parse_toml(&text).unwrap(), p);
    }

    #[test]
    fn key_repeat_names_and_milliseconds() {
        use dp_core::profile::KeyRepeat;
        let p = parse_toml("[virtual.keyboard]\nrepeat = \"fast\"\n").unwrap();
        assert_eq!(p.virtual_devices.keyboard.repeat, KeyRepeat::Fast);
        let p = parse_toml("[virtual.keyboard]\nrepeat = 40\n").unwrap();
        assert_eq!(p.virtual_devices.keyboard.repeat, KeyRepeat::Every(40));
        assert!(to_toml(&p).unwrap().contains("repeat = 40"));
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        assert!(parse_toml("[virtual.keyboard]\nrepeat = 0\n").is_err());
        assert!(parse_toml("[virtual.keyboard]\nrepeat = \"turbo\"\n").is_err());
        assert!(
            !to_toml(&Profile::default()).unwrap().contains("virtual"),
            "native is the default"
        );
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

    #[test]
    fn format_detection() {
        let json = "\u{feff}  {\"schemaVersion\": 3.0}";
        assert_eq!(Format::detect(Path::new("game.cfg"), json), Format::Json);
        assert_eq!(Format::detect(Path::new("game.json"), json), Format::Json);
        assert_eq!(
            Format::detect(Path::new("a.TOML"), "name = \"x\""),
            Format::Toml
        );
        assert_eq!(
            Format::detect(Path::new("noext"), "name = \"x\""),
            Format::Toml
        );
    }

    #[test]
    fn process_matching() {
        let mut p = Profile::default();
        p.processes.push("Game.exe".into());
        let list = vec![(PathBuf::from("g.toml"), p)];
        assert!(profile_for_process(&list, "game.EXE").is_some());
        assert!(profile_for_process(&list, "other.exe").is_none());
    }
}
