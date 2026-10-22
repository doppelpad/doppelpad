//! Importer for JSON profiles (`schemaVersion` 3.0).
//!
//! The importer is lenient: anything it cannot represent becomes a
//! [`Diagnostic`](crate::Diagnostic) and is skipped, instead of failing the whole file.

mod devices;
mod ids;
mod json;
mod mappings;
mod overlays;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use dp_core::profile::{
    Activator, Combo, GyroMode, JumpMode, LayerSticks, LayerTriggers, Light, Mapping, Profile,
    VirtualMouse,
};
use serde_json::Value;

use self::devices::light;
use self::json::{array, get, int, layer_id, obj, text};
use crate::diag::Diagnostics;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a supported JSON profile: {0}")]
    Format(String),
}

/// Raw axis range used throughout the format.
const AXIS_MAX: f32 = 32768.0;
/// Stick deflections in macros run 0–32767 and count as `(value + 1) / 32767`, capped at full
/// deflection; magnitudes in the settings are fractions of [`AXIS_MAX`] instead.
const MACRO_STICK_MAX: f32 = 32767.0;
/// The name the format falls back to when a profile names no game.
pub(crate) const DEFAULT_IMPORT_NAME: &str = "Game";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum DeviceKind {
    Gamepad,
    Keyboard,
    Mouse,
}

struct Importer<'a> {
    root: &'a Value,
    diags: Diagnostics,
    /// Time values are in 10 ms units unless `minTimeUnit == 1`.
    time_scale: u32,
    devices: HashMap<i64, DeviceKind>,
    masks: HashMap<i64, Combo>,
    commands: HashMap<i64, &'a Value>,
    /// Masks that zone activators turned into zone controls: (mask, zoned trigger).
    zoned: Vec<(Combo, Combo)>,
    /// Per-layer stick and trigger settings by layer id, with where they came from.
    layer_analog: HashMap<i64, ((LayerSticks, LayerTriggers), String)>,
    /// Turn calibration from a stick's mouse settings (flick values).
    turn: Option<VirtualMouse>,
    /// The gyro stick entry asks for tilt mode.
    gyro_tilt: bool,
    /// The profile uses the second trackpad, so the touchpad is split into halves.
    split_touch: bool,
    /// Touchpad regions only while the pad is clicked.
    touch_click_required: bool,
    /// Speed (px/s) and acceleration of button-driven mouse motion, from the virtual mouse.
    mouse_speed: (f32, f32),
    /// Crosshair ids in profile order, and the index of each crosshair GUID.
    crosshair_ids: Vec<i64>,
    crosshair_guids: HashMap<String, usize>,
    /// Sector id of each radial-menu sector mask, and menu and position of each sector id.
    radial_masks: HashMap<i64, i64>,
    radial_sectors: HashMap<i64, (usize, usize)>,
    /// Light bars of layers, set once the layers exist.
    layer_lights: Vec<(u8, Light)>,
}

/// Recognize the format's encoding of shift keys and fold each pair into one
/// mapping: base `[T, a] -> N` plus layer-N `[T, release] -> 0` is a hold shift, and
/// base `[T, a] -> N` plus layer-N `[T, a] -> 0` is a toggle shift.
fn fold_jump_pairs(mappings: &mut Vec<Mapping>) {
    let mut remove = Vec::new();
    for i in 0..mappings.len() {
        let m = &mappings[i];
        let Some(j) = &m.jump else { continue };
        if m.layer != 0 || j.layer == 0 || j.mode != JumpMode::Switch || remove.contains(&i) {
            continue;
        }
        let partner = mappings.iter().enumerate().position(|(k, o)| {
            k != i
                && !remove.contains(&k)
                && o.layer == j.layer
                && o.trigger.normalized() == m.trigger.normalized()
                && o.actions.is_empty()
                && o.jump
                    .as_ref()
                    .is_some_and(|oj| oj.layer == 0 && oj.mode == JumpMode::Switch)
                && (o.activator == Activator::Release || o.activator == m.activator)
        });
        if let Some(k) = partner {
            let mode = if mappings[k].activator == Activator::Release {
                JumpMode::Hold
            } else {
                JumpMode::Toggle
            };
            if let Some(j) = mappings[i].jump.as_mut() {
                j.mode = mode;
            }
            remove.push(k);
        }
    }
    remove.sort_unstable();
    for k in remove.into_iter().rev() {
        mappings.remove(k);
    }
}

/// Import a JSON profile document.
pub fn import(json: &str) -> Result<(Profile, Diagnostics), ImportError> {
    let json = json.trim_start_matches('\u{feff}').trim_start();
    if !json.starts_with('{') {
        return Err(ImportError::Format("expected a JSON object".into()));
    }
    let root: Value = serde_json::from_str(json)?;
    match root.get("schemaVersion").and_then(Value::as_f64) {
        Some(3.0) => {}
        Some(v) => {
            return Err(ImportError::Format(format!(
                "unsupported schemaVersion {v} (only 3.0 is supported)"
            )));
        }
        None => return Err(ImportError::Format("missing schemaVersion".into())),
    }
    let mut imp = Importer {
        root: &root,
        diags: Diagnostics::default(),
        time_scale: 10,
        devices: HashMap::new(),
        masks: HashMap::new(),
        commands: HashMap::new(),
        zoned: Vec::new(),
        layer_analog: HashMap::new(),
        turn: None,
        gyro_tilt: false,
        split_touch: false,
        touch_click_required: false,
        mouse_speed: (1000.0, 4.0),
        crosshair_ids: Vec::new(),
        crosshair_guids: HashMap::new(),
        radial_masks: HashMap::new(),
        radial_sectors: HashMap::new(),
        layer_lights: Vec::new(),
    };
    let profile = imp.run();
    Ok((profile, imp.diags))
}

impl<'a> Importer<'a> {
    fn run(&mut self) -> Profile {
        let mut p = Profile::default();
        // The format never mutes a mapped key implicitly; explicit "unmap" entries do.
        p.output.mute_mapped_inputs = false;
        let config = obj(self.root, "config");
        if int(config, "minTimeUnit") == Some(1) {
            self.time_scale = 1;
        }
        p.name = text(config, "appName")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                text(config, "comment")
                    .and_then(|c| {
                        c.strip_prefix("Custom configuration to play ")
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| DEFAULT_IMPORT_NAME.into())
            });
        p.description = text(config, "description").unwrap_or_default();
        p.processes = array(config, "processNames")
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(Into::into)
            .collect();
        if let Some(led) = get(config, "led") {
            p.lightbar = get(led, "default").map(light);
            for s in array(led, "shifts") {
                if let Some(id) = layer_id(s, "shiftId") {
                    self.layer_lights.push((id as u8, light(s)));
                }
            }
        }

        self.devices(&mut p);
        for c in array(self.root, "commands") {
            if let Some(id) = int(c, "id") {
                self.commands.insert(id, c);
            }
        }
        self.split_touch = self.uses_second_trackpad();
        p.touchpad.split = self.split_touch;
        self.crosshairs(&mut p);
        self.radial_menus(&mut p);
        self.masks(&mut p);
        self.layers(&mut p);
        for (id, light) in std::mem::take(&mut self.layer_lights) {
            match p.layers.iter_mut().find(|l| l.id == id) {
                Some(l) => l.lightbar = Some(light),
                None => self.diags.warn(
                    "config.led",
                    format!("light bar of layer {id}, which does not exist"),
                ),
            }
        }
        let mappings = array(self.root, "mappings");
        for (i, m) in mappings.iter().enumerate() {
            let path = format!("mappings[{i}]");
            if self.radial_sector_mapping(m, &mut p, &path) {
                continue;
            }
            if let Some(mapping) = self.mapping(m, &path) {
                p.mappings.push(mapping);
            }
        }
        fold_jump_pairs(&mut p.mappings);
        self.virtual_sticks(&mut p);
        if let Some(turn) = self.turn.take() {
            p.virtual_devices.mouse.counts_per_turn = turn.counts_per_turn;
            p.virtual_devices.mouse.turn_ms = turn.turn_ms;
        }
        if self.gyro_tilt {
            p.gyro.mode = GyroMode::Tilt;
        }
        p.touchpad.click_required = self.touch_click_required;
        // A mask a layer does not inherit stays uninherited in its zone form too.
        for l in &mut p.layers {
            for (mask, zoned) in &self.zoned {
                if l.not_inherited.contains(mask) && !l.not_inherited.contains(zoned) {
                    l.not_inherited.push(zoned.clone());
                }
            }
        }
        Self::finish_radial(&mut p);
        p
    }

    fn time(&self, v: i64) -> u32 {
        (v.max(0) as u32).saturating_mul(self.time_scale)
    }
}
