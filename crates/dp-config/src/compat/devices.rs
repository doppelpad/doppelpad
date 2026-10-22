//! Hardware and virtual device settings: sticks, triggers, touchpad, gyro, vibration and
//! light bars.

use dp_core::control::{Control, Half};
use dp_core::profile::{
    Action, AnalogMapping, AnalogSource, AnalogTarget, JitterPattern, KbmSource, KeyRepeat,
    LayerSticks, LayerTriggers, LayerVirtualSticks, Light, LightMode, Profile, StickConfig,
    StickPatch, Sticks, TriggerEffect, TriggerPatch, Triggers, Vibration, VirtualMotion,
    VirtualMouse, VirtualPadType, VirtualStick, VirtualStickPatch, VirtualSticks, ZoneRadius,
};
use serde_json::Value;

use super::ids::gamepad_control;
use super::json::{NULL, array, boolean, enum_str, get, int, obj, rgb};
use super::{AXIS_MAX, DeviceKind, Importer};

/// An effect like a vendor adaptive-trigger preset: `Some(None)` for presets that only
/// inherit, `None` for unknown ones.
fn adaptive_preset(name: &str) -> Option<Option<TriggerEffect>> {
    let resistance = |start, strength| Some(TriggerEffect::Resistance { start, strength });
    let vibration = |amplitude, frequency| {
        Some(TriggerEffect::Vibration {
            start: 0,
            amplitude,
            frequency,
        })
    };
    Some(match name {
        "none" => Some(TriggerEffect::Off),
        "donotinherit" | "inherited" => None,
        "fullpress" => resistance(0, 6),
        "softpress" => resistance(0, 2),
        "mediumpress" => resistance(0, 4),
        "hardpress" => resistance(0, 7),
        "halfpress" => resistance(5, 6),
        "rigidsoft" => resistance(0, 3),
        "rigidmedium" => resistance(0, 5),
        "rigidmax" => resistance(0, 8),
        "rifle" => Some(TriggerEffect::Weapon {
            start: 4,
            end: 6,
            strength: 8,
        }),
        "pulse" => vibration(4, 10),
        "choppy" => vibration(8, 5),
        "vibration" => vibration(6, 40),
        _ => return None,
    })
}

/// A light-bar entry (`mode`, `colors`, `loop`).
pub(super) fn light(v: &Value) -> Light {
    let mode = match enum_str(v, "mode").as_deref() {
        Some("shimmer") => LightMode::Shimmer,
        Some("heartbeat") => LightMode::Heartbeat,
        Some("stroboscope") => LightMode::Strobe,
        Some("rainbow") => LightMode::Rainbow,
        Some("breath") => LightMode::Breath,
        _ => LightMode::Solid,
    };
    let colors: Vec<String> = array(v, "colors").iter().filter_map(rgb).take(10).collect();
    Light {
        mode,
        colors: if colors.is_empty() {
            Light::default().colors
        } else {
            colors
        },
        cycle: boolean(v, "loop"),
        ..Default::default()
    }
}

/// Highest low-zone threshold imported, so the medium and high zones keep some room.
const MAX_LOW_ZONE: f32 = 0.95;

/// A stick's response curve (`curves.points.horizontal`), sorted by travel.
fn curve_points(s: &Value) -> Option<Vec<[f32; 2]>> {
    let points = get(s, "curves")
        .and_then(|c| get(c, "points"))
        .map(|p| array(p, "horizontal"))
        .unwrap_or(&[]);
    if points.is_empty() {
        return None;
    }
    let mut curve: Vec<[f32; 2]> = points
        .iter()
        .filter_map(|pt| {
            Some([
                int(pt, "travel")? as f32 / AXIS_MAX,
                int(pt, "value")? as f32 / AXIS_MAX,
            ])
        })
        .collect();
    curve.sort_by(|a, b| a[0].total_cmp(&b[0]));
    Some(curve)
}

/// The format's virtual stick defaults: buttons start at 7000/32768 and ramp up (initial
/// speed 8, acceleration 1).
fn vendor_virtual_stick() -> VirtualStick {
    VirtualStick {
        initial: 7000.0 / AXIS_MAX,
        speed: 8.0 * VIRTUAL_SPEED_UNIT,
        acceleration: VIRTUAL_ACCEL_UNIT,
        ..VirtualStick::default()
    }
}

/// One step of `initialSpeed`, in deflection per second (4096 / 32768).
const VIRTUAL_SPEED_UNIT: f32 = 0.125;
/// One step of `acceleration`, in deflection per second² (16384 / 32768).
const VIRTUAL_ACCEL_UNIT: f32 = 0.5;

/// One `zones[]` entry as a fraction of full deflection; a missing `y` equals `x`.
fn zone_entry(zones: &[Value], name: &str) -> Option<ZoneRadius> {
    let z = zones
        .iter()
        .find(|z| enum_str(z, "name").as_deref() == Some(name))?;
    let x = int(z, "x")?;
    let y = int(z, "y").unwrap_or(x);
    let frac = |v: i64| (v as f32 / AXIS_MAX).clamp(0.001, 1.0);
    Some(ZoneRadius {
        x: frac(x),
        y: frac(y),
    })
}

impl<'a> Importer<'a> {
    pub(super) fn devices(&mut self, p: &mut Profile) {
        let Some(devices) = get(self.root, "devices") else {
            self.diags.error("devices", "missing");
            return;
        };
        // Keyboard and mouse groups of injected input, and of the physical devices.
        let (mut scripts, mut physical) = (false, false);
        for (i, hw) in array(devices, "hardware").iter().enumerate() {
            let path = format!("devices.hardware[{i}]");
            let Some(id) = int(hw, "id") else {
                self.diags.warn(&path, "device without id");
                continue;
            };
            let kind = match enum_str(hw, "name").as_deref() {
                Some("gamepad") => DeviceKind::Gamepad,
                Some("keyboard") => {
                    physical = true;
                    DeviceKind::Keyboard
                }
                Some("mouse") => {
                    physical = true;
                    DeviceKind::Mouse
                }
                Some("sendinputkeyboard") => {
                    scripts = true;
                    DeviceKind::Keyboard
                }
                Some("sendinputmouse") => {
                    scripts = true;
                    DeviceKind::Mouse
                }
                other => {
                    self.diags
                        .warn(&path, format!("unknown device kind {other:?}"));
                    continue;
                }
            };
            self.devices.insert(id, kind);
            if kind == DeviceKind::Gamepad {
                if get(hw, "hidePhysicalGamepad").is_some() {
                    p.output.hide_physical = boolean(hw, "hidePhysicalGamepad");
                }
                if let Some(d) = get(hw, "default") {
                    let (mut sticks, mut triggers) = (p.sticks.clone(), p.triggers.clone());
                    self.analog_settings(&mut sticks, &mut triggers, d, &format!("{path}.default"));
                    (p.sticks, p.triggers) = (sticks, triggers);
                    self.vibration(&mut p.vibration, d, &format!("{path}.default"));
                }
                // Per-layer blocks hold the same settings as `default`; a layer keeps only
                // what differs from the base.
                for (k, s) in array(hw, "shifts").iter().enumerate() {
                    let spath = format!("{path}.shifts[{k}]");
                    let Some(layer) = int(s, "shiftId") else {
                        self.diags.warn(&spath, "missing shiftId");
                        continue;
                    };
                    let (mut sticks, mut triggers) = (p.sticks.clone(), p.triggers.clone());
                    self.analog_settings(&mut sticks, &mut triggers, s, &spath);
                    let patch = (
                        LayerSticks {
                            left: StickPatch::between(&p.sticks.left, &sticks.left),
                            right: StickPatch::between(&p.sticks.right, &sticks.right),
                        },
                        LayerTriggers {
                            left: TriggerPatch::between(&p.triggers.left, &triggers.left),
                            right: TriggerPatch::between(&p.triggers.right, &triggers.right),
                        },
                    );
                    self.layer_analog.insert(layer, (patch, spath));
                }
            } else if kind == DeviceKind::Mouse
                && let Some(stick) = get(hw, "default").and_then(|d| get(d, "stick"))
            {
                // A mouse with stick settings is used as a stick (mouse → stick).
                let mut route = AnalogMapping {
                    source: AnalogSource::Mouse,
                    target: AnalogTarget::RightStick,
                    ..Default::default()
                };
                if let Some(t) = int(stick, "returnTime") {
                    route.decay_ms = t.clamp(0, 255) as u32;
                }
                if let Some(s) = int(stick, "smoothing") {
                    route.smoothing_ms = (s.clamp(0, 100) * 4) as u32;
                }
                // Spring mode is the format's default; the noise filter defaults to 3.
                route.spring = get(stick, "springMode")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                route.noise_filter = int(stick, "noiseFilter").unwrap_or(3).clamp(1, 20) as f32;
                if let Some(z) = int(stick, "snapZone") {
                    route.snap = (z as f32 / AXIS_MAX).clamp(0.0, 0.9);
                }
                self.diags.info(
                    format!("{path}.default.stick"),
                    "mouse-as-stick imported as a mouse → right stick route; tune its sensitivity",
                );
                p.analog.push(route);
            }
        }

        match (scripts, physical) {
            (true, false) => p.kbm_source = KbmSource::Scripts,
            (true, true) => self.diags.info(
                "devices.hardware",
                "keyboard and mouse mappings react to both the physical devices and injected input",
            ),
            _ => {}
        }

        let virt = get(devices, "virtual");
        let pads = virt.map(|v| array(v, "gamepads")).unwrap_or(&[]);
        p.output.virtual_pad = match pads.first().and_then(|g| enum_str(g, "type")).as_deref() {
            None | Some("360") => VirtualPadType::Xbox360,
            Some("ds4") => VirtualPadType::DualShock4,
            Some("keyboardandmouse") => VirtualPadType::None,
            Some(other) => {
                let fallback = if other == "ds3" {
                    VirtualPadType::DualShock4
                } else {
                    VirtualPadType::Xbox360
                };
                self.diags.warn(
                    "devices.virtual.gamepads[0].type",
                    format!("virtual `{other}` controllers need a custom driver; using {fallback:?} instead"),
                );
                fallback
            }
        };
        if pads.len() > 1 {
            self.diags.info(
                "devices.virtual.gamepads",
                "only the first virtual gamepad is used; gamepad outputs in this format do not say which one they mean. To drive a second virtual controller, give that physical controller slots of its own",
            );
        }
        // Virtual mouse: button-driven speed starts at deflection · 125 · sensitivity / 4 px/s
        // and grows by acceleration / 2 of itself per second; the wheel repeats every
        // (17 − wheelDeflection) · 20 ms while held. Absent values are the format's defaults.
        let mouse = virt
            .and_then(|v| get(v, "mouse"))
            .map_or(&NULL, |m| obj(m, "config"));
        let in_range = |key: &str, max: i64| int(mouse, key).filter(|v| (1..=max).contains(v));
        let sensitivity = in_range("sensitivity", 100).unwrap_or(4) as f32;
        let deflection = in_range("deflection", 16).unwrap_or(8) as f32;
        let acceleration = int(mouse, "acceleration")
            .filter(|v| (0..=16).contains(v))
            .unwrap_or(8);
        self.mouse_speed = (
            deflection * 125.0 * sensitivity / 4.0,
            acceleration as f32 / 2.0,
        );
        let wheel = in_range("wheelDeflection", 16).unwrap_or(13);
        p.virtual_devices.mouse.wheel_repeat_ms = ((17 - wheel) * 20) as u32;
        // Only the four combinations the editor offers mean something; others stay native.
        if let Some(cfg) = virt
            .and_then(|v| get(v, "keyboard"))
            .and_then(|k| get(k, "config"))
        {
            p.virtual_devices.keyboard.repeat = match (
                enum_str(cfg, "repeatMode").as_deref(),
                int(cfg, "repeatValue"),
            ) {
                (Some("fixed"), Some(10)) => KeyRepeat::Fast,
                (Some("fixed"), Some(50)) => KeyRepeat::Slow,
                (Some("fixed"), Some(0)) => KeyRepeat::Off,
                _ => KeyRepeat::Native,
            };
        }
    }

    /// How mappings push the virtual sticks (`devices.virtual.gamepads[0]`), on the base
    /// layer and per layer. Sticks that buttons push follow the format's ramp even without
    /// settings; the others keep ours.
    pub(super) fn virtual_sticks(&mut self, p: &mut Profile) {
        let path = "devices.virtual.gamepads[0]";
        let pad = get(self.root, "devices")
            .and_then(|d| get(d, "virtual"))
            .and_then(|v| array(v, "gamepads").first());
        let mut base = VirtualSticks {
            left: vendor_virtual_stick(),
            right: vendor_virtual_stick(),
        };
        let mut used = [false; 2];
        if let Some(d) = pad.and_then(|g| get(g, "default")) {
            used = self.virtual_stick_settings(
                &mut base,
                &mut p.virtual_devices.motion,
                d,
                &format!("{path}.default"),
            );
        }
        let mut layers = Vec::new();
        for (k, s) in pad
            .map(|g| array(g, "shifts"))
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let spath = format!("{path}.shifts[{k}]");
            let Some(id) = int(s, "shiftId") else {
                self.diags.warn(&spath, "missing shiftId");
                continue;
            };
            let mut sticks = base.clone();
            let seen =
                self.virtual_stick_settings(&mut sticks, &mut p.virtual_devices.motion, s, &spath);
            used = [used[0] || seen[0], used[1] || seen[1]];
            layers.push((id, sticks, spath));
        }
        for a in p.mappings.iter().flat_map(|m| &m.actions) {
            if let Action::Stick { stick, .. } = a {
                used[*stick as usize] = true;
            }
        }
        let pick = |v: &VirtualStick, i: usize| {
            if used[i] {
                v.clone()
            } else {
                VirtualStick::default()
            }
        };
        let main = VirtualSticks {
            left: pick(&base.left, 0),
            right: pick(&base.right, 1),
        };
        for (id, sticks, spath) in layers {
            let patch = LayerVirtualSticks {
                left: VirtualStickPatch::between(&main.left, &pick(&sticks.left, 0)),
                right: VirtualStickPatch::between(&main.right, &pick(&sticks.right, 1)),
            };
            match p.layers.iter_mut().find(|l| i64::from(l.id) == id) {
                Some(l) => l.virtual_devices.sticks = patch,
                None => self.diags.warn(
                    spath,
                    format!("settings for layer {id}, which does not exist"),
                ),
            }
        }
        p.virtual_devices.sticks = main;
    }

    /// One block of virtual stick settings; returns which sticks (left, right) it set.
    fn virtual_stick_settings(
        &mut self,
        sticks: &mut VirtualSticks,
        motion: &mut VirtualMotion,
        d: &Value,
        path: &str,
    ) -> [bool; 2] {
        let mut seen = [false; 2];
        for s in array(d, "sticks") {
            // Shake settings belong to the virtual accelerometer, whichever entry holds them.
            if let Some(v) = int(s, "shakingDuration").filter(|v| *v > 0) {
                motion.shake_ms = v.clamp(50, 300) as u32;
            }
            if let Some(v) = int(s, "shakingAmplitude").filter(|v| *v > 0) {
                motion.shake_g = v.clamp(1, 8) as f32;
            }
            let (cfg, i) = match enum_str(s, "name").as_deref() {
                Some("left") => (&mut sticks.left, 0),
                Some("right") => (&mut sticks.right, 1),
                Some("gyro") => {
                    self.diags.info(
                        format!("{path}.sticks"),
                        "virtual gyro stick settings are not supported",
                    );
                    continue;
                }
                _ => continue,
            };
            seen[i] = true;
            let fraction = |v: i64| (v as f32 / AXIS_MAX).clamp(0.0, 1.0);
            if let Some(v) = int(s, "initialDeflection") {
                cfg.initial = fraction(v);
            }
            if let Some(v) = int(s, "maxDeflection") {
                cfg.max = fraction(v);
            }
            if let Some(v) = int(s, "initialSpeed") {
                cfg.speed = v.clamp(0, 16) as f32 * VIRTUAL_SPEED_UNIT;
            }
            if let Some(v) = int(s, "acceleration") {
                cfg.acceleration = v.clamp(0, 4) as f32 * VIRTUAL_ACCEL_UNIT;
            }
            // A multiplier of 0 means 1.
            if let Some(v) = int(s, "multiplierX") {
                cfg.multiplier_x = v.clamp(1, 5) as f32;
            }
            if let Some(v) = int(s, "multiplierY") {
                cfg.multiplier_y = v.clamp(1, 5) as f32;
            }
            if let Some(curve) = curve_points(s) {
                cfg.curve = curve;
            }
            let vertical: Vec<&str> = [
                "initialDeflectionY",
                "maxDeflectionY",
                "initialSpeedY",
                "accelerationY",
            ]
            .into_iter()
            .filter(|k| get(s, k).is_some())
            .collect();
            if !vertical.is_empty() {
                self.diags.info(
                    format!("{path}.sticks"),
                    format!(
                        "separate vertical values ({}) are not supported; the horizontal ones apply",
                        vertical.join(", ")
                    ),
                );
            }
            // Jitter sizes are 1–100 for 0.1–10 % of full deflection.
            let j = &mut cfg.jitter;
            j.enabled = boolean(s, "jitter");
            j.pattern = match enum_str(s, "jitterType").as_deref() {
                Some("square") => JitterPattern::Square,
                Some("cross") => JitterPattern::Cross,
                _ => JitterPattern::Circle,
            };
            let size = |v: i64| v.clamp(1, 100) as f32 / 1000.0;
            if let Some(v) = int(s, "jitterDeflectionMin") {
                j.min = size(v);
            }
            if let Some(v) = int(s, "jitterDeflectionMax") {
                j.max = size(v);
            }
            if let Some(v) = int(s, "jitterDelayMin") {
                j.delay_min_ms = v.clamp(10, 100) as u32;
            }
            if let Some(v) = int(s, "jitterDelayMax") {
                j.delay_max_ms = v.clamp(10, 100) as u32;
            }
            (j.min, j.max) = (j.min.min(j.max), j.min.max(j.max));
            (j.delay_min_ms, j.delay_max_ms) = (
                j.delay_min_ms.min(j.delay_max_ms),
                j.delay_min_ms.max(j.delay_max_ms),
            );
            if j.enabled && int(s, "jitterOptionalParameter").is_some_and(|v| v != 1) {
                self.diags.info(
                    format!("{path}.sticks"),
                    "the jitter's optional parameter is not supported",
                );
            }
        }
        seen
    }

    /// Flick values in a stick's mouse settings: the turn calibration (sensitivity is
    /// counts per degree × 10) and the flick length.
    fn flick_settings(&mut self, mc: &Value, path: &str) {
        let sens = int(mc, "flickSensitivity");
        let ms = int(mc, "flickDuration");
        if sens.is_none() && ms.is_none() {
            return;
        }
        let turn = self.turn.get_or_insert_with(VirtualMouse::default);
        if let Some(v) = sens {
            turn.counts_per_turn = 36.0 * v.clamp(1, 2000) as f32;
        }
        if let Some(v) = ms {
            turn.turn_ms = v.clamp(0, 1500) as u32;
        }
        if get(mc, "flickThreshold").is_some() || get(mc, "flickSmoothing").is_some() {
            self.diags.info(
                path,
                "flick threshold and smoothing are set on a flick route; add one to use them",
            );
        }
    }

    /// Motor strengths, a motor swap and the command gap from a device's `default` block.
    fn vibration(&mut self, v: &mut Vibration, d: &Value, path: &str) {
        let mut swapped = [false; 2];
        for item in array(d, "vibration") {
            if get(item, "trigger").is_some() {
                self.diags.info(
                    format!("{path}.vibration"),
                    "trigger rumble settings are not supported",
                );
                continue;
            }
            let Some(m) = get(item, "motor") else {
                continue;
            };
            let (strength, other, i) = match enum_str(m, "name").as_deref() {
                Some("left") => (&mut v.left, "insteadMotorRight", 0),
                Some("right") => (&mut v.right, "insteadMotorLeft", 1),
                _ => continue,
            };
            if let Some(a) = get(m, "amplitude").and_then(Value::as_f64) {
                *strength = (a / 100.0).clamp(0.0, 1.0) as f32;
            }
            if boolean(m, "turnedOff") {
                *strength = 0.0;
            }
            swapped[i] = boolean(m, other);
        }
        v.swap = swapped == [true, true];
        if swapped[0] != swapped[1] {
            self.diags.info(
                format!("{path}.vibration"),
                "one motor redirected onto the other is not supported; the motors stay apart",
            );
        }
        if let Some(gap) = int(d, "vibrationRumbleDelay") {
            v.min_delay_ms = gap.clamp(10, 200) as u32;
        }
        if get(d, "vibrationRumbleMode").is_some() {
            self.diags.info(
                format!("{path}.vibrationRumbleMode"),
                "the rumble mode is not imported",
            );
        }
    }

    /// Gamepad stick/trigger/touchpad settings.
    fn analog_settings(
        &mut self,
        sticks: &mut Sticks,
        triggers: &mut Triggers,
        d: &Value,
        path: &str,
    ) {
        for s in array(d, "sticks") {
            let cfg = match enum_str(s, "name").as_deref() {
                Some("left") => &mut sticks.left,
                Some("right") => &mut sticks.right,
                Some("gyro") => {
                    if boolean(s, "tilt") {
                        self.gyro_tilt = true;
                    }
                    self.diags.info(
                        format!("{path}.sticks"),
                        "gyro sensitivity and curves are not imported; tune the gyro route",
                    );
                    continue;
                }
                _ => continue,
            };
            // The low zone is the deadzone of digital mappings; an output deadzone, if
            // any, is part of the curve.
            let zones = array(s, "zones");
            if let Some(low) = zone_entry(zones, "low") {
                cfg.zones.low = ZoneRadius {
                    x: low.x.min(MAX_LOW_ZONE),
                    y: low.y.min(MAX_LOW_ZONE),
                };
            }
            cfg.zones.med = zone_entry(zones, "med");
            cfg.zones.high = zone_entry(zones, "high");
            let [low, med, high] = cfg.zone_thresholds();
            if crate::zone_problem([low.x, med.x, high.x])
                .or(crate::zone_problem([low.y, med.y, high.y]))
                .is_some()
            {
                self.diags.warn(
                    format!("{path}.sticks"),
                    "stick zones are out of order; the medium and high zones use their defaults",
                );
                cfg.zones.med = None;
                cfg.zones.high = None;
            }
            cfg.diagonals = boolean(s, "diagonalZones");
            // The direction sensitivity is the tangent of the cardinal sector's half width,
            // in 1/32768 units.
            if let Some(sens) = get(s, "sensitivity") {
                let parts: Vec<f32> = [int(sens, "x"), int(sens, "y")]
                    .into_iter()
                    .flatten()
                    .map(|v| v.clamp(1, 32768) as f32 / AXIS_MAX)
                    .collect();
                if !parts.is_empty() {
                    let ratio = parts.iter().sum::<f32>() / parts.len() as f32;
                    let [min, max] = [
                        StickConfig::DIRECTION_WIDTH.start(),
                        StickConfig::DIRECTION_WIDTH.end(),
                    ];
                    cfg.direction_width = ratio.atan().to_degrees().clamp(*min, *max);
                }
            }
            if let Some(mc) = get(s, "mouse") {
                self.flick_settings(mc, &format!("{path}.sticks"));
            }
            if let Some(r) = int(s, "rotation") {
                cfg.rotation = r.clamp(-90, 90) as f32;
            }
            if let Some(curve) = curve_points(s) {
                cfg.curve = curve;
            }
        }
        for t in array(d, "triggers") {
            let cfg = match enum_str(t, "name").as_deref() {
                Some("left") => &mut triggers.left,
                Some("right") => &mut triggers.right,
                _ => continue,
            };
            let zones = array(t, "zones");
            if let Some(low) = zone_entry(zones, "low") {
                cfg.zones.low = low.x.min(MAX_LOW_ZONE);
            }
            cfg.zones.med = zone_entry(zones, "med").map(|r| r.x);
            cfg.zones.high = zone_entry(zones, "high").map(|r| r.x);
            // Zones shape the trigger's output only when they apply at hardware level.
            if boolean(t, "affectHardware") {
                cfg.deadzone = cfg.zones.low;
            }
            if let Some(max) = int(t, "highZoneMax") {
                cfg.full_pull = (max as f32 / AXIS_MAX).clamp(cfg.zones.low + 0.01, 1.0);
            }
            if crate::zone_problem(cfg.zone_thresholds()).is_some() {
                self.diags.warn(
                    format!("{path}.triggers"),
                    "trigger zones are out of order; the medium and high zones use their defaults",
                );
                cfg.zones.med = None;
                cfg.zones.high = None;
            }
            if let Some(preset) = enum_str(t, "adaptiveDescription") {
                match adaptive_preset(&preset) {
                    Some(effect) => {
                        cfg.effect = effect;
                        self.diags.info(
                            format!("{path}.triggers"),
                            format!(
                                "adaptive trigger preset `{preset}` is played as a similar effect"
                            ),
                        );
                    }
                    None => self.diags.info(
                        format!("{path}.triggers"),
                        format!("adaptive trigger preset `{preset}` is not supported"),
                    ),
                }
            }
        }
        for tp in array(d, "touchpads") {
            if boolean(tp, "zoneClickRequired") {
                self.touch_click_required = true;
            }
        }
        if !array(d, "touchpads").is_empty() {
            self.diags.info(
                format!("{path}.touchpads"),
                "touchpad tuning is imported as region and gesture buttons and the click switch only",
            );
        }
    }

    /// Whether masks use the second trackpad (right half), which means the touchpad is
    /// split into two trackpads.
    pub(super) fn uses_second_trackpad(&self) -> bool {
        array(self.root, "masks")
            .iter()
            .flat_map(|m| array(m, "set"))
            .any(|e| {
                let gamepad = int(e, "deviceId")
                    .and_then(|d| self.devices.get(&d))
                    .is_some_and(|k| *k == DeviceKind::Gamepad);
                gamepad
                    && int(e, "buttonId")
                        .and_then(gamepad_control)
                        .is_some_and(|c| matches!(c, Control::Trackpad(Half::Right, _)))
            })
    }
}
