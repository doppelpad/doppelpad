//! Joy-Con handling shared by all backends: pairing a left and a right Joy-Con into one
//! controller, and turning a single Joy-Con sideways.
//!
//! Backends report every Joy-Con upright (as if attached to a console) with
//! [`DeviceInfo::joycon`] set. [`JoyConCombiner`] wraps such a backend: one left and one
//! right Joy-Con become one controller (first come, first paired; a pair lasts until one
//! half disconnects), and a Joy-Con without a partner is turned into a sideways mini pad.

use std::sync::Arc;

use dp_core::pad::{Axis, Motion, PadButton, PadState, Rgb, Rumble, Stick};

use crate::nintendo::NINTENDO_VID;
use crate::{
    Battery, DeviceId, DeviceInfo, GamepadBackend, JoyConSide, Leds, PadFamily, PadSnapshot,
    TriggerEffects, VirtualIgnoreList,
};

/// Product ID of joycond's combined Joy-Con device (Linux).
const JOYCOND_PAIR_PID: u16 = 0x2008;
const SWITCH2_PAIR_PID: u16 = 0x2068;

/// One controller from a left and a right Joy-Con.
pub fn combine(left: &PadState, right: &PadState) -> PadState {
    let mut s = PadState {
        buttons: left.buttons | right.buttons,
        motion: right.motion.or(left.motion),
        ..PadState::default()
    };
    s.set_stick(Stick::Left, left.stick(Stick::Left));
    s.set_stick(Stick::Right, right.stick(Stick::Right));
    s.set_axis(Axis::LeftTrigger, left.axis(Axis::LeftTrigger));
    s.set_axis(Axis::RightTrigger, right.axis(Axis::RightTrigger));
    s
}

/// A single Joy-Con held sideways with its rail on top: the left one turned a quarter
/// anticlockwise, the right one a quarter clockwise.
pub fn sideways(side: JoyConSide, up: &PadState) -> PadState {
    let mut s = PadState::default();
    let map = |s: &mut PadState, to: PadButton, from: PadButton| s.set(to, up.pressed(from));
    match side {
        JoyConSide::Left => {
            map(&mut s, PadButton::A, PadButton::DpadLeft);
            map(&mut s, PadButton::B, PadButton::DpadDown);
            map(&mut s, PadButton::X, PadButton::DpadUp);
            map(&mut s, PadButton::Y, PadButton::DpadRight);
            map(&mut s, PadButton::LB, PadButton::PaddleUL); // SL
            map(&mut s, PadButton::RB, PadButton::PaddleLL); // SR
            map(&mut s, PadButton::Start, PadButton::Back); // Minus
            map(&mut s, PadButton::Guide, PadButton::Misc); // Capture
            map(&mut s, PadButton::LS, PadButton::LS);
            map(&mut s, PadButton::PaddleUL, PadButton::LB); // L
            s.set(PadButton::PaddleLL, up.axis(Axis::LeftTrigger) > 0.5); // ZL
            let (x, y) = up.stick(Stick::Left);
            s.set_stick(Stick::Left, (-y, x));
        }
        JoyConSide::Right => {
            map(&mut s, PadButton::A, PadButton::B); // Nintendo A
            map(&mut s, PadButton::B, PadButton::Y); // Nintendo X
            map(&mut s, PadButton::X, PadButton::A); // Nintendo B
            map(&mut s, PadButton::Y, PadButton::X); // Nintendo Y
            map(&mut s, PadButton::LB, PadButton::PaddleLR); // SL
            map(&mut s, PadButton::RB, PadButton::PaddleUR); // SR
            map(&mut s, PadButton::Start, PadButton::Start); // Plus
            map(&mut s, PadButton::Guide, PadButton::Guide); // Home
            map(&mut s, PadButton::Misc2, PadButton::Misc2); // C
            map(&mut s, PadButton::LS, PadButton::RS);
            map(&mut s, PadButton::PaddleUR, PadButton::RB); // R
            s.set(PadButton::PaddleLR, up.axis(Axis::RightTrigger) > 0.5); // ZR
            let (x, y) = up.stick(Stick::Right);
            s.set_stick(Stick::Left, (y, -x));
        }
    }
    s.motion = up.motion.map(|m| rotate_motion(side, m));
    s
}

/// Motion of a sideways Joy-Con, from its upright motion.
pub fn rotate_motion(side: JoyConSide, m: Motion) -> Motion {
    let turn = |[x, y, z]: [f32; 3]| match side {
        JoyConSide::Left => [z, y, -x],
        JoyConSide::Right => [-z, y, x],
    };
    Motion {
        gyro: turn(m.gyro),
        accel: turn(m.accel),
    }
}

struct Pair {
    left: DeviceId,
    right: DeviceId,
    info: Arc<DeviceInfo>,
}

fn pair_info(left: &DeviceInfo, right: &DeviceInfo) -> Arc<DeviceInfo> {
    let switch2 = left.name.contains("Joy-Con 2");
    let mut instance_paths = left.instance_paths.clone();
    instance_paths.extend(right.instance_paths.iter().cloned());
    Arc::new(DeviceInfo {
        id: DeviceId(format!("{}+{}", left.id, right.id)),
        name: if switch2 {
            "Joy-Con 2 (L/R)"
        } else {
            "Joy-Con (L/R)"
        }
        .to_owned(),
        family: PadFamily::JoyCon,
        vendor_id: NINTENDO_VID,
        product_id: if switch2 {
            SWITCH2_PAIR_PID
        } else {
            JOYCOND_PAIR_PID
        },
        backend: left.backend.clone(),
        instance_paths,
        joycon: None,
        has_motion: left.has_motion || right.has_motion,
        has_touchpad: false,
        has_rumble: left.has_rumble || right.has_rumble,
    })
}

/// Pairs and turns Joy-Cons reported by the wrapped backend.
pub struct JoyConCombiner<B> {
    inner: B,
    raw: Vec<PadSnapshot>,
    /// Joy-Con ids in the order they appeared, for first-come pairing.
    seen: Vec<DeviceId>,
    pairs: Vec<Pair>,
}

impl<B: GamepadBackend> JoyConCombiner<B> {
    pub fn new(inner: B) -> Self {
        JoyConCombiner {
            inner,
            raw: Vec::new(),
            seen: Vec::new(),
            pairs: Vec::new(),
        }
    }

    fn update_pairs(&mut self, joycond: bool) {
        let present = |id: &DeviceId, raw: &[PadSnapshot]| raw.iter().any(|s| &s.info.id == id);
        self.seen.retain(|id| present(id, &self.raw));
        for s in &self.raw {
            if s.info.joycon.is_some() && !self.seen.contains(&s.info.id) {
                self.seen.push(s.info.id.clone());
            }
        }
        let raw = &self.raw;
        self.pairs
            .retain(|p| present(&p.left, raw) && present(&p.right, raw));
        if joycond {
            return;
        }
        let side_of = |id: &DeviceId| {
            raw.iter()
                .find(|s| &s.info.id == id)
                .and_then(|s| s.info.joycon)
        };
        let paired =
            |id: &DeviceId, pairs: &[Pair]| pairs.iter().any(|p| &p.left == id || &p.right == id);
        loop {
            let free = |side| {
                self.seen
                    .iter()
                    .find(|id| side_of(id) == Some(side) && !paired(id, &self.pairs))
                    .cloned()
            };
            let (Some(l), Some(r)) = (free(JoyConSide::Left), free(JoyConSide::Right)) else {
                break;
            };
            let info_of =
                |id: &DeviceId| raw.iter().find(|s| &s.info.id == id).unwrap().info.clone();
            let info = pair_info(&info_of(&l), &info_of(&r));
            tracing::info!(left = %l, right = %r, "Joy-Cons paired");
            self.pairs.push(Pair {
                left: l,
                right: r,
                info,
            });
        }
    }
}

/// The emptier of two batteries: a pair runs out with its first half.
fn lower(a: Option<Battery>, b: Option<Battery>) -> Option<Battery> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b.level < a.level { b } else { a }),
        (a, b) => a.or(b),
    }
}

impl<B: GamepadBackend> GamepadBackend for JoyConCombiner<B> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        self.raw.clear();
        self.inner.poll(&mut self.raw);
        // joycond (Linux) already pairs Joy-Cons into its own device.
        let joycond = self.raw.iter().any(|s| {
            (s.info.vendor_id, s.info.product_id) == (NINTENDO_VID, JOYCOND_PAIR_PID)
                && s.info.joycon.is_none()
        });
        self.update_pairs(joycond);
        for s in &self.raw {
            let Some(side) = s.info.joycon else {
                out.push(s.clone());
                continue;
            };
            if let Some(p) = self.pairs.iter().find(|p| p.left == s.info.id) {
                let right = self.raw.iter().find(|r| r.info.id == p.right);
                if let Some(right) = right {
                    out.push(PadSnapshot {
                        info: p.info.clone(),
                        state: combine(&s.state, &right.state),
                        battery: lower(s.battery, right.battery),
                    });
                }
            } else if self.pairs.iter().any(|p| p.right == s.info.id) {
                // Emitted with its left half.
            } else if !joycond {
                // joycond's combined device already contains the halves.
                out.push(PadSnapshot {
                    info: s.info.clone(),
                    state: sideways(side, &s.state),
                    battery: s.battery,
                });
            }
        }
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        if let Some(p) = self.pairs.iter().find(|p| &p.info.id == id) {
            let (left, right) = (p.left.clone(), p.right.clone());
            self.inner.set_rumble(
                &left,
                Rumble {
                    low: rumble.low,
                    high: 0.0,
                },
            );
            self.inner.set_rumble(
                &right,
                Rumble {
                    low: 0.0,
                    high: rumble.high,
                },
            );
        } else {
            self.inner.set_rumble(id, rumble);
        }
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        // Joy-Cons have no light bar; everything else the inner backend reads may.
        if !self.pairs.iter().any(|p| &p.info.id == id) {
            self.inner.set_lightbar(id, color);
        }
    }

    fn set_trigger_effects(&mut self, id: &DeviceId, effects: TriggerEffects) {
        if !self.pairs.iter().any(|p| &p.info.id == id) {
            self.inner.set_trigger_effects(id, effects);
        }
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        match self.pairs.iter().find(|p| &p.info.id == id) {
            Some(p) => {
                let (left, right) = (p.left.clone(), p.right.clone());
                self.inner.set_leds(&left, leds);
                self.inner.set_leds(&right, leds);
            }
            None => self.inner.set_leds(id, leds),
        }
    }

    fn ignore_virtual(&mut self, ignored: &VirtualIgnoreList) {
        self.inner.ignore_virtual(ignored);
    }

    fn problems(&self) -> Vec<String> {
        self.inner.problems()
    }

    fn rescan(&mut self) {
        self.inner.rescan();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Fake {
        pads: Vec<PadSnapshot>,
        rumble: Arc<Mutex<Vec<(DeviceId, Rumble)>>>,
    }

    impl GamepadBackend for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
            out.extend(self.pads.iter().cloned());
        }
        fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
            self.rumble.lock().unwrap().push((id.clone(), rumble));
        }
    }

    fn pad(id: &str, joycon: Option<JoyConSide>, state: PadState) -> PadSnapshot {
        PadSnapshot {
            info: Arc::new(DeviceInfo {
                id: DeviceId(id.into()),
                name: id.into(),
                family: if joycon.is_some() {
                    PadFamily::JoyCon
                } else {
                    PadFamily::Xbox
                },
                vendor_id: NINTENDO_VID,
                product_id: 0x2006,
                backend: "fake".into(),
                instance_paths: vec![format!("path-{id}")],
                joycon,
                has_motion: true,
                has_touchpad: false,
                has_rumble: true,
            }),
            state,
            battery: None,
        }
    }

    fn poll(c: &mut JoyConCombiner<Fake>) -> Vec<PadSnapshot> {
        let mut out = Vec::new();
        c.poll(&mut out);
        out
    }

    fn ids(v: &[PadSnapshot]) -> Vec<String> {
        v.iter().map(|s| s.info.id.0.clone()).collect()
    }

    const L: Option<JoyConSide> = Some(JoyConSide::Left);
    const R: Option<JoyConSide> = Some(JoyConSide::Right);

    #[test]
    fn pair_forms_and_splits() {
        let mut c = JoyConCombiner::new(Fake::default());
        c.inner.pads = vec![
            pad("x", None, PadState::default()),
            pad("l", L, PadState::default()),
        ];
        assert_eq!(
            ids(&poll(&mut c)),
            ["x", "l"],
            "lone Joy-Con passes as itself"
        );
        c.inner.pads.push(pad("r", R, PadState::default()));
        let out = poll(&mut c);
        assert_eq!(ids(&out), ["x", "l+r"]);
        let pair = &out[1].info;
        assert_eq!(pair.name, "Joy-Con (L/R)");
        assert_eq!(pair.instance_paths, ["path-l", "path-r"]);
        assert_eq!(pair.joycon, None);
        c.inner.pads.retain(|p| p.info.id.0 != "l");
        assert_eq!(ids(&poll(&mut c)), ["x", "r"]);
    }

    #[test]
    fn pairs_stay_stable() {
        let mut c = JoyConCombiner::new(Fake::default());
        c.inner.pads = vec![
            pad("l1", L, PadState::default()),
            pad("r1", R, PadState::default()),
        ];
        assert_eq!(ids(&poll(&mut c)), ["l1+r1"]);
        // A second left Joy-Con arrives first in enumeration order: the pair stays.
        c.inner.pads.insert(0, pad("l2", L, PadState::default()));
        assert_eq!(ids(&poll(&mut c)), ["l2", "l1+r1"]);
        c.inner.pads.push(pad("r2", R, PadState::default()));
        assert_eq!(ids(&poll(&mut c)), ["l2+r2", "l1+r1"]);
    }

    #[test]
    fn combined_state() {
        let mut l = PadState::default();
        l.set(PadButton::DpadUp, true);
        l.set_stick(Stick::Left, (0.5, -0.5));
        l.set_axis(Axis::LeftTrigger, 1.0);
        let mut r = PadState::default();
        r.set(PadButton::B, true);
        r.set_stick(Stick::Right, (-1.0, 0.0));
        r.motion = Some(Motion {
            gyro: [1.0, 2.0, 3.0],
            accel: [0.0, 1.0, 0.0],
        });
        let s = combine(&l, &r);
        assert!(s.pressed(PadButton::DpadUp) && s.pressed(PadButton::B));
        assert_eq!(s.stick(Stick::Left), (0.5, -0.5));
        assert_eq!(s.stick(Stick::Right), (-1.0, 0.0));
        assert_eq!(s.axis(Axis::LeftTrigger), 1.0);
        assert_eq!(s.motion.unwrap().gyro, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn left_sideways() {
        let mut up = PadState::default();
        up.set(PadButton::DpadLeft, true);
        up.set(PadButton::PaddleUL, true); // SL
        up.set(PadButton::Back, true); // Minus
        up.set_axis(Axis::LeftTrigger, 1.0); // ZL
        up.set_stick(Stick::Left, (0.0, 1.0)); // pushed toward the stick's top
        let s = sideways(JoyConSide::Left, &up);
        assert!(s.pressed(PadButton::A) && s.pressed(PadButton::LB) && s.pressed(PadButton::Start));
        assert!(s.pressed(PadButton::PaddleLL));
        assert_eq!(s.axis(Axis::LeftTrigger), 0.0);
        assert_eq!(
            s.stick(Stick::Left),
            (-1.0, 0.0),
            "upright up is sideways left"
        );
    }

    #[test]
    fn right_sideways() {
        let mut up = PadState::default();
        up.set(PadButton::B, true); // Nintendo A
        up.set(PadButton::PaddleUR, true); // SR
        up.set(PadButton::RS, true);
        up.set_stick(Stick::Right, (1.0, 0.0));
        let s = sideways(JoyConSide::Right, &up);
        assert!(s.pressed(PadButton::A), "A becomes the bottom button");
        assert!(s.pressed(PadButton::RB) && s.pressed(PadButton::LS));
        assert!(!s.pressed(PadButton::B) && !s.pressed(PadButton::RS));
        assert_eq!(s.stick(Stick::Left), (0.0, -1.0));
        assert_eq!(s.stick(Stick::Right), (0.0, 0.0));
    }

    #[test]
    fn sideways_motion() {
        let m = Motion {
            gyro: [1.0, 2.0, 3.0],
            accel: [4.0, 5.0, 6.0],
        };
        assert_eq!(rotate_motion(JoyConSide::Left, m).gyro, [3.0, 2.0, -1.0]);
        assert_eq!(rotate_motion(JoyConSide::Right, m).accel, [-6.0, 5.0, 4.0]);
    }

    #[test]
    fn rumble_routing() {
        let mut c = JoyConCombiner::new(Fake::default());
        let log = c.inner.rumble.clone();
        c.inner.pads = vec![
            pad("l", L, PadState::default()),
            pad("r", R, PadState::default()),
        ];
        poll(&mut c);
        c.set_rumble(
            &DeviceId("l+r".into()),
            Rumble {
                low: 0.8,
                high: 0.3,
            },
        );
        c.set_rumble(
            &DeviceId("other".into()),
            Rumble {
                low: 0.1,
                high: 0.1,
            },
        );
        let log = log.lock().unwrap();
        assert_eq!(
            log[0],
            (
                DeviceId("l".into()),
                Rumble {
                    low: 0.8,
                    high: 0.0
                }
            )
        );
        assert_eq!(
            log[1],
            (
                DeviceId("r".into()),
                Rumble {
                    low: 0.0,
                    high: 0.3
                }
            )
        );
        assert_eq!(log[2].0, DeviceId("other".into()));
    }

    #[test]
    fn joycond_pair_disables_pairing() {
        let mut c = JoyConCombiner::new(Fake::default());
        let mut combined = pad("combined", None, PadState::default());
        let info = Arc::make_mut(&mut combined.info);
        info.product_id = JOYCOND_PAIR_PID;
        c.inner.pads = vec![
            combined,
            pad("l", L, PadState::default()),
            pad("r", R, PadState::default()),
        ];
        assert_eq!(ids(&poll(&mut c)), ["combined"]);
    }

    #[test]
    fn a_pair_reports_its_emptier_half() {
        let b = |level| {
            Some(Battery {
                level,
                charging: false,
                wired: false,
            })
        };
        assert_eq!(lower(b(60), b(30)), b(30));
        assert_eq!(lower(None, b(60)), b(60));
        assert_eq!(lower(None, None), None);
    }
}
