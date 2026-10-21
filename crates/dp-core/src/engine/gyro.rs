//! Gyro controls: the motion sensors as directions and zones, either by rotation rate
//! ("motion") or by tilt from a centre pose ("tilt"), and the turning that gyro routes read.

use crate::control::{Control, GyroDir, Zone};
use crate::pad::Motion;
use crate::profile::{GyroMode, GyroSettings, GyroSpace, SteeringAxis};

/// How far a tilt reaches full stick deflection, in degrees.
pub const TILT_FULL_DEG: f32 = 45.0;
/// Player-space yaw may exceed the measured yaw axis by this factor (GyroWiki's "relax").
const PLAYER_RELAX: f32 = 1.41;

/// Gyro state that outlives one tick: the tilt centre and the steering axis in use.
#[derive(Clone, Debug, Default)]
pub struct GyroState {
    /// Gravity direction (normalised accelerometer) at the centre pose.
    centre: Option<[f32; 3]>,
    /// Yaw turned since the centre pose (for yaw steering in tilt mode), degrees.
    yaw: f32,
    /// `switch_gyro_axis` swapped roll and yaw.
    swapped: bool,
}

/// Pitch (nose up +), steering (left +) and lean (left grip down +), in degrees per second
/// in motion mode or degrees from the centre in tilt mode.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct GyroAxes {
    pub pitch: f32,
    pub steer: f32,
    pub lean: f32,
}

fn normalised(a: [f32; 3]) -> Option<[f32; 3]> {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (n > 0.1).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

impl GyroState {
    /// Take the current pose as the tilt centre (on the next motion sample).
    pub fn reset(&mut self) {
        self.centre = None;
        self.yaw = 0.0;
    }

    pub fn switch_axis(&mut self) {
        self.swapped = !self.swapped;
    }

    fn steering(&self, cfg: &GyroSettings) -> SteeringAxis {
        match (cfg.steering, self.swapped) {
            (s, false) => s,
            (SteeringAxis::Roll, true) => SteeringAxis::Yaw,
            (SteeringAxis::Yaw, true) => SteeringAxis::Roll,
        }
    }

    /// Tilt from the centre pose: (nose up, left grip down) in degrees.
    fn tilt(&mut self, m: &Motion) -> Option<(f32, f32)> {
        let g = normalised(m.accel)?;
        let c = *self.centre.get_or_insert(g);
        // Accelerometer axes: x right, y out of the face, z toward the player.
        let pitch = |v: [f32; 3]| (-v[2]).atan2(v[1]).to_degrees();
        let roll = |v: [f32; 3]| v[0].atan2(v[1]).to_degrees();
        Some((pitch(g) - pitch(c), roll(g) - roll(c)))
    }

    /// The gyro axes this tick (`dt` in seconds).
    pub fn axes(&mut self, cfg: &GyroSettings, m: &Motion, dt: f32) -> GyroAxes {
        let (pitch_rate, yaw_rate, roll_rate) = (m.gyro[0], m.gyro[1], m.gyro[2]);
        let steering = self.steering(cfg);
        match cfg.mode {
            GyroMode::Motion => GyroAxes {
                pitch: pitch_rate,
                steer: match steering {
                    SteeringAxis::Roll => roll_rate,
                    SteeringAxis::Yaw => yaw_rate,
                },
                lean: roll_rate,
            },
            GyroMode::Tilt => {
                let Some((pitch, roll)) = self.tilt(m) else {
                    return GyroAxes::default();
                };
                self.yaw += yaw_rate * dt;
                GyroAxes {
                    pitch,
                    steer: match steering {
                        SteeringAxis::Roll => roll,
                        SteeringAxis::Yaw => self.yaw,
                    },
                    lean: roll,
                }
            }
        }
    }
}

/// Yaw and pitch rates for gyro routes: the controller's own yaw, or in player space the
/// turning around gravity (whichever way the controller is held).
pub fn route_rates(cfg: &GyroSettings, m: &Motion) -> (f32, f32) {
    let (pitch, yaw, roll) = (m.gyro[0], m.gyro[1], m.gyro[2]);
    if cfg.space == GyroSpace::Local {
        return (yaw, pitch);
    }
    let Some(g) = normalised(m.accel) else {
        return (yaw, pitch);
    };
    let world = yaw * g[1] + roll * g[2];
    let limit = yaw.hypot(roll);
    (
        world.signum() * (world.abs() * PLAYER_RELAX).min(limit),
        pitch,
    )
}

/// Direction and zone controls for the gyro axes.
pub fn controls(cfg: &GyroSettings, a: GyroAxes, out: &mut impl Extend<Control>) {
    let t = match cfg.mode {
        GyroMode::Motion => cfg.motion_zones,
        GyroMode::Tilt => cfg.tilt_zones,
    };
    let zone = |v: f32| {
        let v = v.abs();
        if v >= t[2] {
            Some(Zone::High)
        } else if v >= t[1] {
            Some(Zone::Med)
        } else if v >= t[0] {
            Some(Zone::Low)
        } else {
            None
        }
    };
    let pairs = [
        (a.pitch, GyroDir::Up, GyroDir::Down),
        (a.steer, GyroDir::Left, GyroDir::Right),
        (a.lean, GyroDir::LeanLeft, GyroDir::LeanRight),
    ];
    // Any-direction zone: the strongest axis.
    let strongest = pairs.iter().map(|p| p.0.abs()).fold(0.0, f32::max);
    if let Some(z) = zone(strongest) {
        out.extend([Control::GyroZone(None, z)]);
    }
    for (v, pos, neg) in pairs {
        if let Some(z) = zone(v) {
            let d = if v > 0.0 { pos } else { neg };
            out.extend([Control::Gyro(d), Control::GyroZone(Some(d), z)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion(gyro: [f32; 3], accel: [f32; 3]) -> Motion {
        Motion { gyro, accel }
    }

    fn set(cfg: &GyroSettings, a: GyroAxes) -> Vec<Control> {
        let mut v = Vec::new();
        controls(cfg, a, &mut v);
        v.sort();
        v
    }

    #[test]
    fn motion_mode_uses_rates_and_zones() {
        let cfg = GyroSettings::default();
        let mut st = GyroState::default();
        let a = st.axes(&cfg, &motion([300.0, 0.0, -70.0], [0.0, 1.0, 0.0]), 0.001);
        assert_eq!(
            a,
            GyroAxes {
                pitch: 300.0,
                steer: -70.0,
                lean: -70.0
            }
        );
        let c = set(&cfg, a);
        assert!(c.contains(&Control::Gyro(GyroDir::Up)));
        assert!(c.contains(&Control::GyroZone(Some(GyroDir::Up), Zone::Med)));
        assert!(c.contains(&Control::GyroZone(Some(GyroDir::Right), Zone::Low)));
        assert!(c.contains(&Control::Gyro(GyroDir::LeanRight)));
        assert!(
            c.contains(&Control::GyroZone(None, Zone::Med)),
            "strongest axis"
        );
        st.switch_axis();
        let a = st.axes(&cfg, &motion([0.0, 80.0, 0.0], [0.0, 1.0, 0.0]), 0.001);
        assert_eq!(a.steer, 80.0, "yaw steering after the switch");
    }

    #[test]
    fn tilt_mode_measures_from_the_centre() {
        let cfg = GyroSettings {
            mode: GyroMode::Tilt,
            ..Default::default()
        };
        let mut st = GyroState::default();
        // Held tilted back 10° as the centre.
        let r = 10f32.to_radians();
        st.axes(&cfg, &motion([0.0; 3], [0.0, r.cos(), -r.sin()]), 0.001);
        // Now nose up 25° more and the left grip 15° down.
        let (p, l) = (35f32.to_radians(), 15f32.to_radians());
        let a = st.axes(&cfg, &motion([0.0; 3], [l.sin(), p.cos(), -p.sin()]), 0.001);
        assert!((a.pitch - 25.0).abs() < 1.0, "{a:?}");
        assert!(a.steer > 10.0 && a.lean == a.steer, "{a:?}");
        let c = set(&cfg, a);
        assert!(c.contains(&Control::GyroZone(Some(GyroDir::Up), Zone::Med)));
        assert!(c.contains(&Control::Gyro(GyroDir::LeanLeft)));
        st.reset();
        let a = st.axes(&cfg, &motion([0.0; 3], [l.sin(), p.cos(), -p.sin()]), 0.001);
        assert!(
            a.pitch.abs() < 0.01 && a.steer.abs() < 0.01,
            "the new centre"
        );
    }

    #[test]
    fn player_space_turns_around_gravity() {
        let cfg = GyroSettings {
            space: GyroSpace::Player,
            ..Default::default()
        };
        // Held upright (face toward the player): turning around gravity shows on roll.
        let (yaw, _) = route_rates(&cfg, &motion([0.0, 0.0, 50.0], [0.0, 0.0, 1.0]));
        assert!((yaw - 50.0).abs() < 0.01);
        let local = GyroSettings::default();
        assert_eq!(
            route_rates(&local, &motion([0.0, 0.0, 50.0], [0.0, 0.0, 1.0])).0,
            0.0
        );
    }
}
