//! Pure keyboard / mouse event logic for the uinput output device and the grabbing capture:
//! code translation, wheel accumulation, and the per-device passthrough filter that decides
//! which captured events are re-injected. No I/O, so it is unit-tested on every OS.

use std::collections::HashSet;

use dp_core::event::KbmEvent;
use dp_core::keys::{Key, MouseButton, WheelDir};

use super::codes::*;
use crate::CaptureFilter;

pub fn mouse_button_from_code(code: u16) -> Option<MouseButton> {
    Some(match code {
        BTN_LEFT => MouseButton::Left,
        BTN_RIGHT => MouseButton::Right,
        BTN_MIDDLE => MouseButton::Middle,
        BTN_SIDE | BTN_BACK => MouseButton::X1,
        BTN_EXTRA | BTN_FORWARD => MouseButton::X2,
        _ => return None,
    })
}

pub fn mouse_button_code(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => BTN_LEFT,
        MouseButton::Right => BTN_RIGHT,
        MouseButton::Middle => BTN_MIDDLE,
        MouseButton::X1 => BTN_SIDE,
        MouseButton::X2 => BTN_EXTRA,
    }
}

/// Direction of a wheel event, for any of the four wheel axes.
fn wheel_dir(code: u16, value: i32) -> Option<WheelDir> {
    let vertical = match code {
        REL_WHEEL | REL_WHEEL_HI_RES => true,
        REL_HWHEEL | REL_HWHEEL_HI_RES => false,
        _ => return None,
    };
    Some(match (vertical, value > 0) {
        (true, true) => WheelDir::Up,
        (true, false) => WheelDir::Down,
        (false, true) => WheelDir::Right,
        (false, false) => WheelDir::Left,
    })
}

/// Accumulates fractional wheel motion for the virtual mouse. Each call yields the
/// `(REL code, value)` pairs to emit: always the high-resolution axis, plus the classic
/// axis once a full detent has built up.
#[derive(Debug, Default)]
pub struct WheelOut {
    /// Not yet emitted high-resolution units, per [`WheelDir`].
    hi_rem: [f32; 4],
    /// High-resolution units emitted since the last full detent: vertical, horizontal.
    detent_acc: [i32; 2],
}

impl WheelOut {
    pub fn wheel(&mut self, dir: WheelDir, detents: f32) -> Vec<(u16, i32)> {
        let rem = &mut self.hi_rem[dir as usize];
        *rem += detents * HI_RES_PER_DETENT as f32;
        let amount = rem.trunc();
        if amount == 0.0 {
            return Vec::new();
        }
        *rem -= amount;
        let (hi, lo, axis, sign) = match dir {
            WheelDir::Up => (REL_WHEEL_HI_RES, REL_WHEEL, 0, 1),
            WheelDir::Down => (REL_WHEEL_HI_RES, REL_WHEEL, 0, -1),
            WheelDir::Right => (REL_HWHEEL_HI_RES, REL_HWHEEL, 1, 1),
            WheelDir::Left => (REL_HWHEEL_HI_RES, REL_HWHEEL, 1, -1),
        };
        let value = amount as i32 * sign;
        let mut out = vec![(hi, value)];
        let acc = &mut self.detent_acc[axis];
        *acc += value;
        let full = *acc / HI_RES_PER_DETENT;
        if full != 0 {
            *acc -= full * HI_RES_PER_DETENT;
            out.push((lo, full));
        }
        out
    }
}

/// State of one grabbed keyboard / mouse: turns its raw events into [`KbmEvent`]s for the
/// engine and, frame by frame, into the events re-injected through its passthrough device
/// (everything the [`CaptureFilter`] does not suppress).
#[derive(Debug, Default)]
pub struct Passthrough {
    /// The device has `REL_WHEEL_HI_RES` / `REL_HWHEEL_HI_RES`; engine wheel events then
    /// come from those (fractional detents) and the classic axes are only forwarded.
    hires: [bool; 2],
    /// Keys and buttons whose press was forwarded, so their repeat and release must be too.
    /// A release is forwarded exactly when the press was, even if the filter changed while
    /// the key was held, so nothing ever sticks on the passthrough device.
    down: HashSet<u16>,
    frame: Vec<(u16, u16, i32)>,
    motion: (i32, i32),
}

impl Passthrough {
    pub fn new(hires_vertical: bool, hires_horizontal: bool) -> Self {
        Passthrough {
            hires: [hires_vertical, hires_horizontal],
            ..Default::default()
        }
    }

    /// Feed one event. Engine events are appended to `events`. When `SYN_REPORT` completes a
    /// frame, the events to re-inject are appended to `forward` (without the `SYN_REPORT`,
    /// which the caller's emit adds).
    pub fn event(
        &mut self,
        (ty, code, value): (u16, u16, i32),
        filter: &CaptureFilter,
        events: &mut Vec<KbmEvent>,
        forward: &mut Vec<(u16, u16, i32)>,
    ) {
        // Grabbed devices are physical; a profile that reacts only to injected input lets
        // them through untouched.
        if !filter.accepts(false) {
            if ty != EV_SYN {
                self.frame.push((ty, code, value));
            } else if code == SYN_REPORT {
                forward.append(&mut self.frame);
            }
            return;
        }
        match ty {
            EV_SYN => {
                if code != SYN_REPORT {
                    return;
                }
                if self.motion != (0, 0) {
                    let (dx, dy) = std::mem::take(&mut self.motion);
                    events.push(KbmEvent::MouseMove { dx, dy });
                }
                // A frame of nothing but MSC_SCAN (its key was suppressed) is dropped.
                if self.frame.iter().any(|e| e.0 != EV_MSC) {
                    forward.append(&mut self.frame);
                } else {
                    self.frame.clear();
                }
            }
            EV_KEY => {
                let (event, suppress) = if let Some(key) = Key::from_evdev(code) {
                    (
                        Some(KbmEvent::Key {
                            key,
                            down: value != 0,
                        }),
                        filter.key(key),
                    )
                } else if let Some(button) = mouse_button_from_code(code) {
                    (
                        Some(KbmEvent::MouseButton {
                            button,
                            down: value != 0,
                        }),
                        filter.mouse_button(button),
                    )
                } else {
                    (None, false)
                };
                let pass = match value {
                    0 => self.down.remove(&code),
                    1 if suppress => false,
                    1 => {
                        self.down.insert(code);
                        true
                    }
                    _ => self.down.contains(&code), // autorepeat
                };
                if value != 2 {
                    events.extend(event);
                }
                if pass {
                    self.frame.push((ty, code, value));
                }
            }
            EV_REL => {
                if code == REL_X || code == REL_Y {
                    if code == REL_X {
                        self.motion.0 += value;
                    } else {
                        self.motion.1 += value;
                    }
                    if !filter.suppress_mouse_move() {
                        self.frame.push((ty, code, value));
                    }
                    return;
                }
                let Some(dir) = wheel_dir(code, value) else {
                    self.frame.push((ty, code, value));
                    return;
                };
                let vertical = matches!(dir, WheelDir::Up | WheelDir::Down);
                let hires_code = code == REL_WHEEL_HI_RES || code == REL_HWHEEL_HI_RES;
                if hires_code == self.hires[!vertical as usize] {
                    let detents = if hires_code {
                        value.abs() as f32 / HI_RES_PER_DETENT as f32
                    } else {
                        value.abs() as f32
                    };
                    events.push(KbmEvent::Wheel { dir, detents });
                }
                if !filter.wheel(dir) {
                    self.frame.push((ty, code, value));
                }
            }
            _ => self.frame.push((ty, code, value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    const EV_MSC_SCAN: (u16, u16, i32) = (EV_MSC, 4, 0x70004);
    const SYN: (u16, u16, i32) = (EV_SYN, SYN_REPORT, 0);

    fn run(
        p: &mut Passthrough,
        f: &CaptureFilter,
        evs: &[(u16, u16, i32)],
    ) -> (Vec<KbmEvent>, Vec<(u16, u16, i32)>) {
        let (mut events, mut forward) = (Vec::new(), Vec::new());
        for &e in evs {
            p.event(e, f, &mut events, &mut forward);
        }
        (events, forward)
    }

    fn filter() -> std::sync::Arc<CaptureFilter> {
        let f = CaptureFilter::new();
        f.enabled.store(true, Ordering::Relaxed);
        f
    }

    #[test]
    fn unfiltered_keys_pass_through() {
        let f = filter();
        let mut p = Passthrough::default();
        let a = Key::A.evdev();
        let (events, fwd) = run(&mut p, &f, &[EV_MSC_SCAN, (EV_KEY, a, 1), SYN]);
        assert_eq!(
            events,
            vec![KbmEvent::Key {
                key: Key::A,
                down: true
            }]
        );
        assert_eq!(fwd, vec![EV_MSC_SCAN, (EV_KEY, a, 1)]);
    }

    #[test]
    fn suppressed_key_is_swallowed_with_its_scan_code() {
        let f = filter();
        f.set_key(Key::W, true);
        let mut p = Passthrough::default();
        let w = Key::W.evdev();
        let (events, fwd) = run(
            &mut p,
            &f,
            &[
                EV_MSC_SCAN,
                (EV_KEY, w, 1),
                SYN,
                (EV_KEY, w, 2),
                SYN,
                (EV_KEY, w, 0),
                SYN,
            ],
        );
        assert_eq!(
            events,
            vec![
                KbmEvent::Key {
                    key: Key::W,
                    down: true
                },
                KbmEvent::Key {
                    key: Key::W,
                    down: false
                },
            ],
            "autorepeat is not reported"
        );
        assert!(fwd.is_empty());
    }

    #[test]
    fn release_follows_press_when_filter_changes() {
        let f = filter();
        let mut p = Passthrough::default();
        let s = Key::S.evdev();
        let (_, fwd) = run(&mut p, &f, &[(EV_KEY, s, 1), SYN]);
        assert_eq!(fwd, vec![(EV_KEY, s, 1)]);
        f.set_key(Key::S, true); // profile layer changed while S was held
        let (_, fwd) = run(&mut p, &f, &[(EV_KEY, s, 2), SYN, (EV_KEY, s, 0), SYN]);
        assert_eq!(fwd, vec![(EV_KEY, s, 2), (EV_KEY, s, 0)]);
        // And the other way round: a swallowed press is never released on its own.
        let (_, fwd) = run(&mut p, &f, &[(EV_KEY, s, 1), SYN]);
        assert!(fwd.is_empty());
        f.set_key(Key::S, false);
        let (_, fwd) = run(&mut p, &f, &[(EV_KEY, s, 0), SYN]);
        assert!(fwd.is_empty());
    }

    #[test]
    fn disabled_filter_suppresses_nothing() {
        let f = CaptureFilter::new(); // enabled = false
        f.set_mouse_button(MouseButton::Left, true);
        let mut p = Passthrough::default();
        let (events, fwd) = run(&mut p, &f, &[(EV_KEY, BTN_LEFT, 1), SYN]);
        assert_eq!(
            events,
            vec![KbmEvent::MouseButton {
                button: MouseButton::Left,
                down: true
            }]
        );
        assert_eq!(fwd, vec![(EV_KEY, BTN_LEFT, 1)]);
    }

    #[test]
    fn mouse_motion_and_buttons() {
        let f = filter();
        f.mouse_move.store(true, Ordering::Relaxed);
        f.set_mouse_button(MouseButton::X1, true);
        let mut p = Passthrough::default();
        let (events, fwd) = run(
            &mut p,
            &f,
            &[
                (EV_REL, REL_X, 3),
                (EV_REL, REL_Y, -2),
                (EV_REL, REL_X, 1),
                (EV_KEY, BTN_SIDE, 1),
                (EV_KEY, BTN_RIGHT, 1),
                SYN,
            ],
        );
        assert_eq!(
            events,
            vec![
                KbmEvent::MouseButton {
                    button: MouseButton::X1,
                    down: true
                },
                KbmEvent::MouseButton {
                    button: MouseButton::Right,
                    down: true
                },
                KbmEvent::MouseMove { dx: 4, dy: -2 },
            ]
        );
        assert_eq!(fwd, vec![(EV_KEY, BTN_RIGHT, 1)]);
    }

    #[test]
    fn wheel_resolution_and_suppression() {
        let f = filter();
        f.set_wheel(WheelDir::Down, true);
        let mut p = Passthrough::new(true, false);
        let (events, fwd) = run(
            &mut p,
            &f,
            &[
                (EV_REL, REL_WHEEL, 1),
                (EV_REL, REL_WHEEL_HI_RES, 60),
                SYN,
                (EV_REL, REL_WHEEL_HI_RES, -120),
                (EV_REL, REL_WHEEL, -1),
                SYN,
                (EV_REL, REL_HWHEEL, -2),
                SYN,
            ],
        );
        assert_eq!(
            events,
            vec![
                KbmEvent::Wheel {
                    dir: WheelDir::Up,
                    detents: 0.5
                },
                KbmEvent::Wheel {
                    dir: WheelDir::Down,
                    detents: 1.0
                },
                KbmEvent::Wheel {
                    dir: WheelDir::Left,
                    detents: 2.0
                },
            ]
        );
        assert_eq!(
            fwd,
            vec![
                (EV_REL, REL_WHEEL, 1),
                (EV_REL, REL_WHEEL_HI_RES, 60),
                (EV_REL, REL_HWHEEL, -2)
            ]
        );
    }

    #[test]
    fn unknown_events_pass_through() {
        let f = filter();
        let mut p = Passthrough::default();
        let (events, fwd) = run(&mut p, &f, &[(EV_KEY, 0x2ff, 1), (EV_ABS, 0x20, 7), SYN]);
        assert!(events.is_empty());
        assert_eq!(fwd, vec![(EV_KEY, 0x2ff, 1), (EV_ABS, 0x20, 7)]);
    }

    #[test]
    fn wheel_output_accumulates() {
        let mut w = WheelOut::default();
        assert_eq!(
            w.wheel(WheelDir::Up, 1.0),
            vec![(REL_WHEEL_HI_RES, 120), (REL_WHEEL, 1)]
        );
        assert_eq!(w.wheel(WheelDir::Down, 0.5), vec![(REL_WHEEL_HI_RES, -60)]);
        assert_eq!(
            w.wheel(WheelDir::Down, 0.5),
            vec![(REL_WHEEL_HI_RES, -60), (REL_WHEEL, -1)]
        );
        assert!(w.wheel(WheelDir::Left, 0.001).is_empty());
        assert_eq!(
            w.wheel(WheelDir::Right, 2.0),
            vec![(REL_HWHEEL_HI_RES, 240), (REL_HWHEEL, 2)]
        );
    }

    #[test]
    fn mouse_button_codes_roundtrip() {
        for b in MouseButton::ALL {
            assert_eq!(mouse_button_from_code(mouse_button_code(b)), Some(b));
        }
        assert_eq!(mouse_button_from_code(BTN_BACK), Some(MouseButton::X1));
        assert_eq!(mouse_button_from_code(BTN_SOUTH), None);
    }
}
