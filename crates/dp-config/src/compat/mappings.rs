//! Masks, layers and mappings with their macros.

use dp_core::control::{Control, Dir, Half, TouchGesture, TrackpadPart, Zone};
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::{
    Action, Activator, Combo, Command, Jump, JumpMode, Layer, Mapping, Mode, Press, Profile,
    ShakeAxis, Unlock,
};
use serde_json::Value;

use super::ids::{
    dir_of, gamepad_control, in_zone, keyboard_key, mouse_button, press_of, wheel_of,
};
use super::json::{array, boolean, enum_str, get, int, layer_id, obj, target_layer, text};
use super::{DeviceKind, Importer, MACRO_STICK_MAX};

impl<'a> Importer<'a> {
    /// A gamepad control id; on a split touchpad the first trackpad's ids mean the left half.
    fn pad_control(&self, id: i64) -> Option<Control> {
        let c = gamepad_control(id)?;
        if !self.split_touch {
            return Some(c);
        }
        Some(match c {
            Control::Touch(r) => Control::Trackpad(Half::Left, TrackpadPart::Region(r)),
            Control::Button(PadButton::Touchpad) => {
                Control::Trackpad(Half::Left, TrackpadPart::Click)
            }
            Control::Gesture(TouchGesture::Tap) => Control::Trackpad(Half::Left, TrackpadPart::Tap),
            _ => c,
        })
    }

    /// Resolve one `{deviceId, buttonId, description}` element to a control.
    fn element(&mut self, e: &Value, path: &str) -> Option<Control> {
        let device = int(e, "deviceId").and_then(|d| self.devices.get(&d).copied());
        let id = int(e, "buttonId");
        let control =
            match (device, id) {
                (None, _) => {
                    self.diags.warn(path, "references an unknown device");
                    return None;
                }
                (_, None) => {
                    self.diags.warn(path, "missing buttonId");
                    return None;
                }
                (Some(DeviceKind::Gamepad), Some(id)) => self.pad_control(id),
                (Some(DeviceKind::Keyboard), Some(_)) => {
                    return keyboard_key(e, &mut self.diags, path).map(Control::Key);
                }
                (Some(DeviceKind::Mouse), Some(id)) => mouse_button(id)
                    .map(Control::Mouse)
                    .or_else(|| match gamepad_control(id) {
                        c @ Some(
                            Control::MouseMove(_) | Control::MouseZone(..) | Control::Wheel(_),
                        ) => c,
                        _ => None,
                    }),
            };
        if control.is_none() {
            let desc = text(e, "description").unwrap_or_default();
            self.diags.warn(
                path,
                format!("input {} ({desc}) is not supported", id.unwrap_or_default()),
            );
        }
        control
    }

    pub(super) fn masks(&mut self, p: &mut Profile) {
        for (i, m) in array(self.root, "masks").iter().enumerate() {
            let path = format!("masks[{i}]");
            let Some(id) = int(m, "id") else { continue };
            let set = array(m, "set");
            if set.is_empty() {
                // A crosshair mask shows its crosshair on a layer; mappings on it are dropped.
                for c in array(m, "crosshairSet") {
                    let index = text(c, "crosshairGuid").and_then(|g| self.crosshair_guids.get(&g));
                    match (index, int(c, "layer")) {
                        (Some(&x), Some(layer @ 0..=255)) => {
                            let layers = &mut p.crosshairs[x].layers;
                            if !layers.contains(&(layer as u8)) {
                                layers.push(layer as u8);
                            }
                        }
                        _ => self
                            .diags
                            .warn(&path, "crosshair mask of an unknown crosshair"),
                    }
                }
                // A sector mask: mappings on it are what the sector does.
                if let Some(s) = array(m, "radialMenuSet")
                    .first()
                    .and_then(|s| int(s, "sectorId"))
                {
                    self.radial_masks.insert(id, s);
                }
                continue;
            }
            let mut combo = Vec::new();
            for (j, e) in set.iter().enumerate() {
                match self.element(e, &format!("{path}.set[{j}]")) {
                    Some(c) => combo.push(c),
                    None => {
                        combo.clear();
                        break;
                    }
                }
            }
            if !combo.is_empty() {
                self.masks.insert(id, Combo(combo));
            }
        }
    }

    pub(super) fn layers(&mut self, p: &mut Profile) {
        for (i, s) in array(self.root, "shifts").iter().enumerate() {
            let path = format!("shifts[{i}]");
            let Some(id) = layer_id(s, "id") else {
                self.diags.warn(&path, "invalid layer id");
                continue;
            };
            let ty = enum_str(s, "type");
            let unlock = match ty.as_deref() {
                Some("unlock") => Some(Unlock::All),
                Some("unlockkeyboardonly") => Some(Unlock::Keyboard),
                Some("unlockmouseonly") => Some(Unlock::Mouse),
                _ => None,
            };
            let toggle = ty.as_deref() == Some("toggle");
            let not_inherited = get(s, "unheritableMasks")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|m| self.masks.get(&m).cloned())
                        .collect()
                })
                .unwrap_or_default();
            let (sticks, triggers) = self
                .layer_analog
                .remove(&id)
                .map(|(patch, _)| patch)
                .unwrap_or_default();
            p.layers.push(Layer {
                id: id as u8,
                name: text(s, "description").unwrap_or_else(|| format!("Layer {id}")),
                inherit: true,
                not_inherited,
                sticks,
                triggers,
                unlock,
                ..Default::default()
            });

            // Legacy modifier (one button or several held together) → synthesized jump mapping.
            let set = array(s, "set");
            let controls: Vec<Control> = set
                .iter()
                .enumerate()
                .filter_map(|(k, e)| self.element(e, &format!("{path}.set[{k}]")))
                .collect();
            if !set.is_empty() && controls.len() == set.len() {
                let mut m = Mapping::new(Combo(controls), Vec::new());
                m.jump = Some(Jump {
                    layer: id as u8,
                    mode: if toggle {
                        JumpMode::Toggle
                    } else {
                        JumpMode::Hold
                    },
                    delay_ms: None,
                    postpone: false,
                });
                m.description = format!("layer {id} modifier");
                p.mappings.push(m);
            }
        }
        let mut orphans: Vec<_> = self.layer_analog.drain().collect();
        orphans.sort_by_key(|(id, _)| *id);
        for (id, (_, path)) in orphans {
            self.diags.warn(
                path,
                format!("settings for layer {id}, which does not exist"),
            );
        }
    }

    pub(super) fn mapping(&mut self, m: &Value, path: &str) -> Option<Mapping> {
        let cond = get(m, "condition")?;
        let layer = target_layer(cond, "shiftId");

        if let Some(hw) = get(cond, "hardware") {
            return self.hardware_mapping(hw, get(m, "hardware"), layer, path);
        }
        let mask = get(cond, "mask")?;
        let mask_id = int(mask, "id")?;
        let Some(combo) = self.masks.get(&mask_id).cloned() else {
            // The mask itself was already reported when it failed to resolve.
            return None;
        };
        let act = obj(mask, "activator");
        let params = obj(act, "params");

        let mut out = Mapping::new(combo.clone(), Vec::new());
        out.layer = layer;
        out.description = text(m, "description").unwrap_or_default();
        out.activator = match enum_str(act, "type").as_deref() {
            None | Some("single") => Activator::Single,
            Some("double") => Activator::Double,
            Some("triple") => Activator::Triple,
            Some("long") => Activator::Long,
            Some("start") => Activator::Start,
            Some("release") => Activator::Release,
            Some(z @ ("zonelow" | "zonemed" | "zonehigh")) => {
                // The zone applies to the mask's stick directions and triggers, which
                // become their zone controls (`RT` → `RTHigh`).
                let zone = match z {
                    "zonelow" => Zone::Low,
                    "zonemed" => Zone::Med,
                    _ => Zone::High,
                };
                let mut zoned = false;
                for c in &mut out.trigger.0 {
                    if let Some(zc) = in_zone(*c, zone) {
                        *c = zc;
                        zoned = true;
                    }
                }
                if zoned {
                    self.zoned.push((combo, out.trigger.clone()));
                } else {
                    self.diags.warn(
                        path,
                        format!(
                            "zone activator `{z}` on {} is not supported; imported as an ordinary press",
                            out.trigger
                        ),
                    );
                }
                Activator::Single
            }
            Some(other) => {
                self.diags
                    .warn(path, format!("unknown activator `{other}`"));
                return None;
            }
        };
        out.mode = match enum_str(act, "mode").as_deref() {
            None | Some("hold_until_release") => Mode::Hold,
            Some("turbo") => Mode::Turbo,
            Some("toggle") => Mode::Toggle,
            Some("onetime") => Mode::Once,
            Some("repeat") => Mode::Repeat,
            Some(other) => {
                self.diags
                    .warn(path, format!("unknown activation mode `{other}`"));
                Mode::Hold
            }
        };
        if let Some(t) = int(params, "pause") {
            match out.mode {
                Mode::Turbo => out.turbo_ms = Some(self.time(t.min(60_000)).max(10)),
                Mode::Repeat => out.repeat_pause_ms = Some(self.time(t.min(2550))),
                _ => {}
            }
        }
        if out.mode == Mode::Repeat {
            out.repeat = Some(int(params, "times").unwrap_or(1).clamp(1, 9999) as u32);
        }
        // Key delay is in time units like pauses.
        if let Some(d) = int(params, "delay")
            .map(|d| self.time(d))
            .filter(|d| *d <= 500)
        {
            out.key_delay_ms = Some(d);
        }
        // Wait times are plain milliseconds. `singlewaittime` is the double-press window
        // and `doublewaittime` the triple-press window.
        if let Some(t) = int(params, "longwaittime").filter(|t| (1..=2500).contains(t)) {
            out.long_press_ms = Some(t as u32);
        }
        let double = int(params, "singlewaittime").filter(|t| (1..=900).contains(t));
        let triple = int(params, "doublewaittime").filter(|t| (1..=1000).contains(t));
        if let Some(d) = double {
            out.double_tap_ms = Some(d as u32);
        }
        if let Some(t) = triple.filter(|t| double.is_none_or(|d| *t > d)) {
            out.triple_tap_ms = Some(t as u32);
        }

        let mut break_seen = false;
        for (j, item) in array(m, "macros").iter().enumerate() {
            let ipath = format!("{path}.macros[{j}]");
            if get(item, "break").is_some() {
                break_seen = true;
                if !out.actions.last().is_some_and(Action::is_break) {
                    out.actions.push(Action::BREAK);
                }
                continue;
            }
            if let Some(a) = self.macro_item(item, &ipath) {
                // Consecutive pauses merge.
                if let (Action::Pause { pause: new }, Some(Action::Pause { pause })) =
                    (&a, out.actions.last_mut())
                {
                    *pause += new;
                    continue;
                }
                out.actions.push(a);
            }
        }
        if break_seen {
            // Segments play one per press: no turbo, toggle or repetition.
            out.mode = match out.mode {
                Mode::Turbo | Mode::Toggle => Mode::Hold,
                Mode::Repeat => Mode::Once,
                other => other,
            };
            out.turbo_ms = None;
            out.repeat = None;
            out.repeat_pause_ms = None;
        }
        // A one-shot "press K, maybe wait, release K" is a plain remap of K,
        // and so is a single item without an explicit action.
        if out.mode == Mode::Once && !boolean(params, "macro") {
            let plain = match out.actions.as_slice() {
                [
                    Action::Key {
                        key: a,
                        action: Press::Down,
                    },
                    rest @ ..,
                ] => match rest {
                    [
                        Action::Key {
                            key: b,
                            action: Press::Up,
                        },
                    ]
                    | [
                        Action::Pause { .. },
                        Action::Key {
                            key: b,
                            action: Press::Up,
                        },
                    ] if a == b => Some(Action::Key {
                        key: *a,
                        action: Press::Auto,
                    }),
                    _ => None,
                },
                [single]
                    if matches!(
                        single,
                        Action::Key {
                            action: Press::Auto,
                            ..
                        } | Action::Pad {
                            action: Press::Auto,
                            ..
                        } | Action::Mouse {
                            action: Press::Auto,
                            ..
                        }
                    ) =>
                {
                    Some(single.clone())
                }
                _ => None,
            };
            if let Some(a) = plain {
                out.mode = Mode::Hold;
                out.actions = vec![a];
            }
        }

        if let Some(j) = get(m, "jumpToLayer") {
            // Jumps in this format are absolute "go to layer N"; hold/toggle shifts are pairs of
            // jumps, which `fold_jump_pairs` turns back into our hold/toggle form.
            out.jump = Some(Jump {
                layer: target_layer(j, "layer"),
                mode: JumpMode::Switch,
                delay_ms: int(j, "delay")
                    .filter(|d| *d > 0)
                    .map(|d| d.min(5000) as u32),
                postpone: boolean(j, "postpone"),
            });
            if out.mode == Mode::Toggle && out.actions.is_empty() {
                out.mode = Mode::Hold;
            }
        }
        if out.actions.is_empty() && out.jump.is_none() {
            // A mapping without outputs still consumes its trigger ("unmap").
            out.description = if out.description.is_empty() {
                "unmapped".into()
            } else {
                out.description
            };
        }
        Some(out)
    }

    pub(super) fn macro_item(&mut self, item: &Value, path: &str) -> Option<Action> {
        if let Some(k) = get(item, "keyboard") {
            let key = keyboard_key(k, &mut self.diags, path)?;
            return Some(Action::Key {
                key,
                action: press_of(k),
            });
        }
        if let Some(ms) = get(item, "mouse") {
            if let Some(b) = int(ms, "buttonId") {
                let Some(mouse) = mouse_button(b) else {
                    self.diags.warn(path, format!("unknown mouse button {b}"));
                    return None;
                };
                return Some(Action::Mouse {
                    mouse,
                    action: press_of(ms),
                });
            }
            if let Some(d) = enum_str(ms, "direction").as_deref().and_then(dir_of) {
                // Vertical directions in this format are inverted relative to the screen.
                let screen = match d {
                    Dir::Up => Dir::Down,
                    Dir::Down => Dir::Up,
                    other => other,
                };
                // A flick item turns a quarter to that side.
                if boolean(ms, "flick") && matches!(d, Dir::Left | Dir::Right) {
                    let turn = if d == Dir::Left { -90.0 } else { 90.0 };
                    return Some(Action::Turn { turn });
                }
                let (speed, acceleration) = self.mouse_speed;
                return Some(Action::MouseMove {
                    mouse_move: screen,
                    speed,
                    acceleration,
                });
            }
            if let Some(w) = enum_str(ms, "wheel").as_deref().and_then(wheel_of) {
                return Some(Action::Wheel {
                    wheel: w,
                    clicks: 1.0,
                });
            }
            self.diags.warn(path, "unrecognized mouse item");
            return None;
        }
        if let Some(g) = get(item, "gamepad") {
            return self.gamepad_item(g, path);
        }
        if let Some(pz) = get(item, "pause") {
            return Some(Action::Pause {
                pause: self.time(int(pz, "value").unwrap_or(50)),
            });
        }
        if let Some(r) = get(item, "rumble") {
            let motor = get(r, "amplitude").and_then(|a| get(a, "motor"));
            let amp = |side: &str| {
                motor.and_then(|m| int(m, side)).unwrap_or(80).clamp(0, 100) as f32 / 100.0
            };
            let ms = self.time(int(r, "duration").unwrap_or(15)).clamp(100, 2550);
            return Some(Action::Rumble {
                rumble: ms,
                low: amp("left"),
                high: amp("right"),
            });
        }
        if let Some(c) = get(item, "command") {
            let id = int(c, "id")?;
            let Some(cmd) = self.commands.get(&id).copied() else {
                self.diags
                    .warn(path, format!("command {id} is not defined"));
                return None;
            };
            let ty = int(cmd, "type").unwrap_or(0);
            let command = match ty {
                1 => Command::RemapOff,
                2 => Command::PowerOff,
                4 => Command::SwitchGyroAxis,
                5 => Command::ResetTilt,
                6 => Command::ToggleGyro,
                7 => Command::LogOff,
                8 => Command::Shutdown,
                9 => Command::Sleep,
                11 => Command::Screenshot,
                12 => Command::CloseWindow,
                13 => Command::Hibernate,
                15 => Command::GyroOn,
                16 => Command::GyroOff,
                17 => Command::Restart,
                101..=110 => {
                    // By crosshair id; the position counts in the profile.
                    let id = ty - 100;
                    let n = self
                        .crosshair_ids
                        .iter()
                        .position(|&c| c == id)
                        .map_or(id, |i| i as i64 + 1);
                    Command::Crosshair(n as u8)
                }
                120 => Command::ToggleCrosshair,
                130 => Command::RadialPrev,
                131 => Command::RadialNext,
                132 => Command::RadialApply,
                133 => Command::RadialBack,
                134 => Command::RadialUp,
                135 => Command::RadialDown,
                136 => Command::RadialRight,
                137 => Command::RadialLeft,
                1000..=1999 => {
                    let params = obj(cmd, "params");
                    Command::Launch {
                        path: text(params, "path").unwrap_or_default(),
                        args: text(params, "arguments").unwrap_or_default(),
                    }
                }
                _ => {
                    let name = text(cmd, "name").unwrap_or_else(|| format!("command type {ty}"));
                    self.diags
                        .warn(path, format!("command `{name}` is not supported"));
                    Command::Unsupported(name)
                }
            };
            return Some(Action::Command { command });
        }
        self.diags
            .warn(path, format!("unrecognized macro item {item}"));
        None
    }

    /// Virtual-gamepad output item.
    fn gamepad_item(&mut self, g: &Value, path: &str) -> Option<Action> {
        let id = int(g, "buttonId")?;
        let action = press_of(g);
        let value = int(g, "value");
        let deflection = |v: Option<i64>| {
            v.map(|v| ((v + 1) as f32 / MACRO_STICK_MAX).clamp(0.0, 1.0))
                .unwrap_or(1.0)
        };
        let stick = |s: Stick, d: Dir| {
            let (x, y) = d.vector();
            let m = deflection(value);
            Some(Action::Stick {
                stick: s,
                x: x * m,
                y: y * m,
                absolute: boolean(g, "absolute"),
                action,
            })
        };
        match id {
            40 => stick(Stick::Left, Dir::Up),
            41 => stick(Stick::Left, Dir::Down),
            42 => stick(Stick::Left, Dir::Left),
            43 => stick(Stick::Left, Dir::Right),
            47 => stick(Stick::Right, Dir::Up),
            48 => stick(Stick::Right, Dir::Down),
            49 => stick(Stick::Right, Dir::Left),
            50 => stick(Stick::Right, Dir::Right),
            51 | 59 => Some(Action::Trigger {
                trigger: Trigger::Left,
                value: deflection(value),
                action,
            }),
            55 | 60 => Some(Action::Trigger {
                trigger: Trigger::Right,
                value: deflection(value),
                action,
            }),
            // Gestures on the virtual touchpad; `value` is the length.
            77..=90 => match gamepad_control(id) {
                Some(Control::Gesture(touch)) => Some(Action::Touch {
                    touch,
                    ms: value.map(|v| self.time(v)),
                }),
                _ => None,
            },
            // Accelerometer shakes: x+, x-, y+, y-, z+, z-.
            248..=253 => Some(Action::Shake {
                shake: ShakeAxis::ALL[(id - 248) as usize],
            }),
            _ => match gamepad_control(id) {
                Some(Control::Button(pad)) => Some(Action::Pad { pad, action }),
                _ => {
                    let desc = text(g, "description").unwrap_or_default();
                    self.diags.warn(
                        path,
                        format!("virtual output {id} ({desc}) is not supported"),
                    );
                    None
                }
            },
        }
    }

    /// Low-level remaps.
    fn hardware_mapping(
        &mut self,
        cond: &Value,
        target: Option<&Value>,
        layer: u8,
        path: &str,
    ) -> Option<Mapping> {
        let target = target?;
        if get(cond, "stick").is_some() {
            self.diags.warn(
                path,
                "stick axis remaps are not supported; use the stick invert options instead",
            );
            return None;
        }
        if get(cond, "trigger").is_some() {
            self.diags.warn(path, "trigger swaps are not supported");
            return None;
        }
        let source = self.element(cond, path)?;
        let mut m = Mapping::new(source, Vec::new());
        m.layer = layer;
        if boolean(target, "unmap") {
            m.description = "unmapped".into();
            return Some(m);
        }
        let device = int(cond, "deviceId").and_then(|d| self.devices.get(&d).copied());
        let to = int(target, "buttonId")?;
        let action = match device {
            Some(DeviceKind::Gamepad) => match gamepad_control(to) {
                Some(Control::Button(pad)) => Action::Pad {
                    pad,
                    action: Press::Auto,
                },
                _ => {
                    self.diags
                        .warn(path, format!("cannot remap to gamepad input {to}"));
                    return None;
                }
            },
            Some(DeviceKind::Keyboard) => Action::Key {
                key: keyboard_key(target, &mut self.diags, path)?,
                action: Press::Auto,
            },
            Some(DeviceKind::Mouse) => Action::Mouse {
                mouse: mouse_button(to)?,
                action: Press::Auto,
            },
            None => return None,
        };
        m.actions.push(action);
        m.description = "hardware remap".into();
        Some(m)
    }
}
