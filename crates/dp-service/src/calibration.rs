//! Gyro calibration per controller: a bias taken off the gyro rates, estimated while the
//! controller rests or measured on request, stored in `calibration.toml` next to the slot
//! state.

use std::collections::BTreeMap;
use std::path::Path;

use dp_config::{FileFormat, Loaded};
use dp_core::pad::PadState;
use serde::{Deserialize, Serialize};

/// The accelerometer must read 1 g within this to count as resting.
const REST_G_TOLERANCE: f32 = 0.1;
/// ... and change less than this between samples.
const REST_ACCEL_STEP: f32 = 0.02;
/// Every corrected rate must be under this (°/s).
const REST_RATE: f32 = 4.0;
/// Resting this long before the bias starts to follow.
const REST_MS: u64 = 1000;
/// Time constant of the automatic bias (ms).
const AUTO_TAU_MS: f32 = 2000.0;
/// A manual calibration fails when a rate strays further than this from the mean (°/s)...
const MANUAL_SPREAD: f32 = 3.0;
/// ... or the accelerometer changes more than this (g).
const MANUAL_ACCEL_STEP: f32 = 0.05;

/// One controller's calibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceCalibration {
    /// Subtracted from the gyro rates, °/s.
    pub gyro_bias: [f32; 3],
    /// Keep estimating the bias while the controller rests.
    pub auto: bool,
}

impl Default for DeviceCalibration {
    fn default() -> Self {
        DeviceCalibration {
            gyro_bias: [0.0; 3],
            auto: true,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CalibrationFile {
    devices: BTreeMap<String, DeviceCalibration>,
}

/// The format of `calibration.toml`.
const FORMAT: FileFormat = FileFormat::new(&[]);

/// What the GUI shows for one controller.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CalibrationStatus {
    pub gyro_bias: [f32; 3],
    pub auto: bool,
    /// A manual calibration is running: how far it is, 0–1.
    pub measuring: Option<f32>,
    /// How the last manual calibration ended (`true` = new bias taken).
    pub last_manual: Option<bool>,
}

#[derive(Debug)]
struct Manual {
    start: u64,
    ms: u64,
    samples: Vec<[f32; 3]>,
    accel_first: Option<[f32; 3]>,
    moved: bool,
}

#[derive(Debug, Default)]
struct DeviceState {
    cal: DeviceCalibration,
    rest_since: Option<u64>,
    last_accel: Option<[f32; 3]>,
    last_ms: Option<u64>,
    manual: Option<Manual>,
    last_manual: Option<bool>,
}

/// Calibration of every controller seen.
#[derive(Debug, Default)]
pub struct Calibrator {
    devices: BTreeMap<String, DeviceState>,
    /// Changed since the last save.
    dirty: bool,
    /// Changed by an explicit action (to be saved at once rather than periodically).
    urgent: bool,
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Calibrator {
    /// Load from `path`; a missing or unreadable file gives defaults (with the problem).
    pub fn load(path: &Path) -> Loaded<Calibrator> {
        let file: Loaded<CalibrationFile> = dp_config::load_toml_or_default(path, &FORMAT);
        file.map(|file| Calibrator {
            devices: file
                .devices
                .into_iter()
                .map(|(id, cal)| {
                    let state = DeviceState {
                        cal,
                        ..Default::default()
                    };
                    (id, state)
                })
                .collect(),
            ..Default::default()
        })
    }

    /// Write to `path` atomically.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let file = CalibrationFile {
            devices: self
                .devices
                .iter()
                .filter(|(_, d)| d.cal != DeviceCalibration::default())
                .map(|(id, d)| (id.clone(), d.cal.clone()))
                .collect(),
        };
        dp_config::save_toml(path, &file, &FORMAT)?;
        self.dirty = false;
        self.urgent = false;
        Ok(())
    }

    /// Whether to save now: after explicit changes, or (`periodic`) after any change.
    pub fn needs_save(&self, periodic: bool) -> bool {
        self.urgent || (periodic && self.dirty)
    }

    /// Take the bias off a controller's gyro rates, and learn from them.
    pub fn apply(&mut self, id: &str, state: &mut PadState, now: u64) {
        let Some(m) = state.motion.as_mut() else {
            return;
        };
        if !self.devices.contains_key(id) {
            self.devices.insert(id.to_owned(), Default::default());
        }
        let d = self.devices.get_mut(id).expect("inserted above");
        let raw = m.gyro;
        let dt = d.last_ms.map_or(0.0, |t| now.saturating_sub(t) as f32);
        d.last_ms = Some(now);

        if let Some(man) = &mut d.manual {
            let first = *man.accel_first.get_or_insert(m.accel);
            man.moved |= dist(first, m.accel) > MANUAL_ACCEL_STEP;
            man.samples.push(raw);
            if now >= man.start + man.ms {
                let n = man.samples.len() as f32;
                let mut mean = [0.0f32; 3];
                for s in &man.samples {
                    for i in 0..3 {
                        mean[i] += s[i] / n;
                    }
                }
                let spread = man
                    .samples
                    .iter()
                    .flat_map(|s| (0..3).map(move |i| (s[i] - mean[i]).abs()))
                    .fold(0.0f32, f32::max);
                let ok = !man.moved && spread <= MANUAL_SPREAD;
                if ok {
                    d.cal.gyro_bias = mean;
                    self.dirty = true;
                    self.urgent = true;
                }
                d.last_manual = Some(ok);
                d.manual = None;
            }
        } else if d.cal.auto {
            let g = (m.accel[0].powi(2) + m.accel[1].powi(2) + m.accel[2].powi(2)).sqrt();
            let still_accel = (g - 1.0).abs() <= REST_G_TOLERANCE
                && d.last_accel
                    .is_none_or(|a| dist(a, m.accel) <= REST_ACCEL_STEP);
            let still_rate = (0..3).all(|i| (raw[i] - d.cal.gyro_bias[i]).abs() < REST_RATE);
            if still_accel && still_rate {
                let since = *d.rest_since.get_or_insert(now);
                if now.saturating_sub(since) >= REST_MS && dt > 0.0 {
                    let a = (dt / AUTO_TAU_MS).min(1.0);
                    for (b, r) in d.cal.gyro_bias.iter_mut().zip(raw) {
                        *b += (r - *b) * a;
                    }
                    self.dirty = true;
                }
            } else {
                d.rest_since = None;
            }
        }
        d.last_accel = Some(m.accel);
        for ((g, r), b) in m.gyro.iter_mut().zip(raw).zip(d.cal.gyro_bias) {
            *g = r - b;
        }
    }

    /// Measure a controller's bias over `ms` from now (it should lie still).
    pub fn start_manual(&mut self, id: &str, now: u64, ms: u64) {
        let d = self.devices.entry(id.to_owned()).or_default();
        d.manual = Some(Manual {
            start: now,
            ms,
            samples: Vec::new(),
            accel_first: None,
            moved: false,
        });
        d.last_manual = None;
    }

    pub fn set_auto(&mut self, id: &str, on: bool) {
        let d = self.devices.entry(id.to_owned()).or_default();
        d.cal.auto = on;
        d.rest_since = None;
        self.dirty = true;
        self.urgent = true;
    }

    /// Forget a controller's bias.
    pub fn reset(&mut self, id: &str) {
        let d = self.devices.entry(id.to_owned()).or_default();
        d.cal.gyro_bias = [0.0; 3];
        d.rest_since = None;
        self.dirty = true;
        self.urgent = true;
    }

    pub fn status(&self, id: &str, now: u64) -> CalibrationStatus {
        let d = self.devices.get(id);
        let cal = d.map(|d| d.cal.clone()).unwrap_or_default();
        CalibrationStatus {
            gyro_bias: cal.gyro_bias,
            auto: cal.auto,
            measuring: d
                .and_then(|d| d.manual.as_ref())
                .map(|m| (now.saturating_sub(m.start) as f32 / m.ms.max(1) as f32).min(1.0)),
            last_manual: d.and_then(|d| d.last_manual),
        }
    }
}

#[cfg(test)]
mod tests {
    use dp_core::pad::Motion;

    use super::*;

    fn pad(gyro: [f32; 3], accel: [f32; 3]) -> PadState {
        PadState {
            motion: Some(Motion { gyro, accel }),
            ..Default::default()
        }
    }

    #[test]
    fn auto_bias_follows_a_resting_drift() {
        let mut c = Calibrator::default();
        let mut out = pad([0.0; 3], [0.0; 3]);
        for t in 0..10_000 {
            out = pad([1.5, -0.5, 0.2], [0.0, 1.0, 0.0]);
            c.apply("pad", &mut out, t);
        }
        let g = out.motion.unwrap().gyro;
        assert!(g.iter().all(|v| v.abs() < 0.1), "drift removed: {g:?}");
        // A real turn is left alone and does not move the bias.
        let mut turn = pad([90.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        c.apply("pad", &mut turn, 10_001);
        assert!((turn.motion.unwrap().gyro[0] - 88.5).abs() < 0.2);
        assert!(c.status("pad", 10_001).gyro_bias[0] > 1.4);
    }

    #[test]
    fn manual_calibration_needs_stillness() {
        let mut c = Calibrator::default();
        c.set_auto("pad", false);
        c.start_manual("pad", 0, 100);
        for t in 0..=100 {
            c.apply("pad", &mut pad([2.0, 1.0, -1.0], [0.0, 1.0, 0.0]), t);
        }
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
        let s = c.status("pad", 101);
        assert_eq!(s.last_manual, Some(true));
        assert!(near(s.gyro_bias, [2.0, 1.0, -1.0]), "{:?}", s.gyro_bias);
        // Moving during the measurement keeps the old bias.
        c.start_manual("pad", 200, 100);
        for t in 200..=300 {
            let a = if t == 250 {
                [0.3, 0.9, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            c.apply("pad", &mut pad([5.0, 0.0, 0.0], a), t);
        }
        let s = c.status("pad", 301);
        assert_eq!(s.last_manual, Some(false));
        assert!(near(s.gyro_bias, [2.0, 1.0, -1.0]));
    }

    #[test]
    fn saved_and_loaded_per_device() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("calibration.toml");
        let mut c = Calibrator::default();
        c.set_auto("a", false);
        c.reset("b");
        assert!(c.needs_save(false));
        c.save(&path).unwrap();
        assert!(!c.needs_save(true));
        let c = Calibrator::load(&path).value;
        assert!(!c.status("a", 0).auto);
        assert!(c.status("b", 0).auto, "default entries are not written");
    }
}
