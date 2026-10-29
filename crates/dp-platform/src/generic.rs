//! Gamepads without a driver of their own, read through the operating system's HID parser.
//!
//! They report buttons, axes and hat switches in an order only the maker knows. A
//! [`GenericLayout`], recorded by the user with the identify wizard and kept per model
//! (vendor and product ID), says which of them is which pad control. Until there is one, a
//! guess in the common DirectInput order is used.

use std::sync::Arc;
use std::time::Instant;

use dp_core::pad::{Axis, PadButton, PadState, Trigger};
use serde::{Deserialize, Serialize};

use crate::PadFamily;
use crate::padio::{Identity, InitError, PadDriver, ReportIo};

/// What a generic pad reports, before any layout: buttons in HID order, axes in `-1..=1` by
/// usage (X, Y, Z, Rx, Ry, Rz, sliders, dials, then simulation controls), hat switches as a
/// direction 0–7 clockwise from up (`None` when centred). [`usages`] says how.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RawInput {
    pub buttons: Vec<bool>,
    pub axes: Vec<f32>,
    pub hats: Vec<Option<u8>>,
}

impl RawInput {
    /// Make room for this many controls of each kind; what is there stays.
    pub fn resize(&mut self, buttons: usize, axes: usize, hats: usize) {
        self.buttons.resize(buttons, false);
        self.axes.resize(axes, 0.0);
        self.hats.resize(hats, None);
    }
}

/// Turns input reports into raw input.
pub trait ReportDecoder: Send {
    /// Decode `report` (report ID first) into `out`, which holds what the earlier reports said
    /// (devices may spread their controls over several reports): `false` for a report without
    /// any of the pad's controls.
    fn decode(&mut self, report: &[u8], out: &mut RawInput) -> bool;
}

/// How the HID parsers of the OSes (`hiddesc`, and `windows::hidparse` on top of Windows' own)
/// make [`RawInput`] of a pad's inputs: which usages count, in which order, and how values
/// become axes and hat directions. A saved [`GenericLayout`] indexes these positions, so a
/// difference would map the wrong controls on the other OS.
pub(crate) mod usages {
    // Linux reads generic pads through evdev.
    #![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

    use std::ops::RangeInclusive;

    pub const PAGE_DESKTOP: u16 = 0x01;
    pub const PAGE_SIMULATION: u16 = 0x02;
    pub const PAGE_BUTTON: u16 = 0x09;
    /// X, Y, Z, Rx, Ry, Rz, slider, dial, wheel.
    const DESKTOP_AXES: RangeInclusive<u16> = 0x30..=0x38;
    const HAT_SWITCH: u16 = 0x39;
    /// More buttons than any pad has: the rest are left out.
    pub const MAX_BUTTONS: usize = 32;

    /// Whether an input of this page and usage is one of the axes.
    pub fn is_axis(page: u16, usage: u16) -> bool {
        (page == PAGE_DESKTOP && DESKTOP_AXES.contains(&usage)) || page == PAGE_SIMULATION
    }

    /// Whether an input of this page and usage is a hat switch.
    pub fn is_hat(page: u16, usage: u16) -> bool {
        page == PAGE_DESKTOP && usage == HAT_SWITCH
    }

    /// Button usages as the buttons are numbered: by usage, each once, at most
    /// [`MAX_BUTTONS`].
    pub fn order_buttons(usages: &mut Vec<u16>) {
        usages.sort_unstable();
        usages.dedup();
        usages.truncate(MAX_BUTTONS);
    }

    /// Axes as they are numbered: by page and usage (X to wheel, then the simulation
    /// controls), each once.
    pub fn order_axes<T>(axes: &mut Vec<T>, usage: impl Fn(&T) -> (u16, u16)) {
        axes.sort_by_key(&usage);
        axes.dedup_by_key(|a| usage(a));
    }

    /// A value of `bits` bits as it reads: sign-extended when its logical range is signed
    /// (`min` below 0).
    pub fn signed(raw: u32, bits: u32, min: i64) -> i64 {
        let v = i64::from(raw);
        if min < 0 && (1..=32).contains(&bits) && v & (1 << (bits - 1)) != 0 {
            v - (1 << bits)
        } else {
            v
        }
    }

    /// A value as an axis: `-1..=1` over its logical range `min..=max`.
    pub fn axis(v: i64, min: i64, max: i64) -> f32 {
        if max <= min {
            return 0.0;
        }
        (((v - min) as f32 / (max - min) as f32) * 2.0 - 1.0).clamp(-1.0, 1.0)
    }

    /// A value as a hat direction, 0–7 clockwise from up, over a range of eight directions
    /// or four; `None` (centred) for a value outside it.
    pub fn hat(v: i64, min: i64, max: i64) -> Option<u8> {
        let d = v - min;
        match max - min + 1 {
            8 if (0..8).contains(&d) => Some(d as u8),
            4 if (0..4).contains(&d) => Some(d as u8 * 2),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn values() {
            assert_eq!(signed(0xFF, 8, -128), -1);
            assert_eq!(signed(0xFF, 8, 0), 255, "an unsigned range");
            assert_eq!(signed(0x8000, 16, -32768), -32768);
            assert_eq!(signed(u32::MAX, 32, -1), -1, "32 bits too");
            assert_eq!(signed(u32::MAX, 32, 0), i64::from(u32::MAX));
            assert_eq!(axis(0, 0, 255), -1.0);
            assert_eq!(axis(255, 0, 255), 1.0);
            assert_eq!(axis(5, 3, 3), 0.0, "no range");
            assert_eq!(hat(2, 0, 7), Some(2));
            assert_eq!(hat(8, 0, 7), None, "centred");
            assert_eq!(hat(1, 1, 8), Some(0));
            assert_eq!(hat(3, 0, 3), Some(6), "four ways: left");
        }

        #[test]
        fn order() {
            let mut buttons: Vec<u16> = (1..=40).rev().chain([3, 3]).collect();
            order_buttons(&mut buttons);
            assert_eq!(buttons, (1..=32).collect::<Vec<u16>>());
            let mut axes = vec![
                (PAGE_SIMULATION, 0xC4),
                (PAGE_DESKTOP, 0x31),
                (PAGE_DESKTOP, 0x30),
                (PAGE_DESKTOP, 0x30),
            ];
            order_axes(&mut axes, |a| *a);
            assert_eq!(axes, [(1, 0x30), (1, 0x31), (2, 0xC4)]);
            assert!(is_axis(PAGE_DESKTOP, 0x38) && !is_axis(PAGE_DESKTOP, 0x39));
            assert!(is_hat(PAGE_DESKTOP, 0x39) && !is_hat(PAGE_BUTTON, 0x39));
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HatDir {
    Up,
    Right,
    Down,
    Left,
}

impl HatDir {
    /// Whether hat direction `d` (0–7, clockwise from up) points this way, diagonals included.
    fn covers(self, d: u8) -> bool {
        let centre = match self {
            HatDir::Up => 0,
            HatDir::Right => 2,
            HatDir::Down => 4,
            HatDir::Left => 6,
        };
        (d + 8 - centre) % 8 <= 1 || (centre + 8 - d) % 8 <= 1
    }
}

/// One raw input that presses a button or pulls a trigger.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawSource {
    Button {
        index: u8,
    },
    Hat {
        index: u8,
        dir: HatDir,
    },
    /// An axis moved from `rest` towards `full`: 0 at rest, 1 at `full`.
    Axis {
        index: u8,
        rest: f32,
        full: f32,
    },
}

impl RawSource {
    /// How far this input is pressed, 0–1.
    pub fn level(&self, raw: &RawInput) -> f32 {
        match *self {
            RawSource::Button { index } => {
                f32::from(raw.buttons.get(index as usize).copied().unwrap_or(false))
            }
            RawSource::Hat { index, dir } => f32::from(
                raw.hats
                    .get(index as usize)
                    .copied()
                    .flatten()
                    .is_some_and(|d| dir.covers(d)),
            ),
            RawSource::Axis { index, rest, full } => match raw.axes.get(index as usize) {
                Some(v) if full != rest => ((v - rest) / (full - rest)).clamp(0.0, 1.0),
                _ => 0.0,
            },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ButtonBind {
    pub button: PadButton,
    pub from: RawSource,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriggerBind {
    pub trigger: Trigger,
    pub from: RawSource,
}

/// A stick axis read from a raw axis; `invert` flips it (HID's y grows downwards).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StickBind {
    pub axis: Axis,
    pub from: u8,
    #[serde(default)]
    pub invert: bool,
}

/// Which raw input is which pad control, for one controller model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenericLayout {
    /// The model, `vvvv:pppp` in hex.
    pub device: String,
    pub buttons: Vec<ButtonBind>,
    pub triggers: Vec<TriggerBind>,
    pub sticks: Vec<StickBind>,
}

/// The layout key of a model.
fn model_key(vid: u16, pid: u16) -> String {
    format!("{vid:04x}:{pid:04x}")
}

/// Level above which a button counts as pressed.
const PRESS: f32 = 0.5;

impl GenericLayout {
    /// Why the layout cannot be stored, if it cannot: the model key must read `vvvv:pppp`
    /// and something must be mapped.
    pub fn problem(&self) -> Option<String> {
        let hex4 = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit());
        let key_ok = self
            .device
            .split_once(':')
            .is_some_and(|(v, p)| hex4(v) && hex4(p));
        if !key_ok {
            return Some(format!(
                "`{}` is not a controller model (vvvv:pppp)",
                self.device
            ));
        }
        if self.buttons.is_empty() && self.triggers.is_empty() && self.sticks.is_empty() {
            return Some("the layout maps nothing".into());
        }
        None
    }

    /// The pad state this layout makes of `raw`.
    pub fn apply(&self, raw: &RawInput) -> PadState {
        let mut s = PadState::default();
        for b in &self.buttons {
            if b.from.level(raw) > PRESS {
                s.set(b.button, true);
            }
        }
        for t in &self.triggers {
            let axis = t.trigger.axis();
            let v = t.from.level(raw).max(s.axis(axis));
            s.set_axis(axis, v);
        }
        for st in &self.sticks {
            if let Some(&v) = raw.axes.get(st.from as usize) {
                s.set_axis(st.axis, if st.invert { -v } else { v });
            }
        }
        s
    }

    /// A first guess in the order most DirectInput pads use: buttons 1–4 the face buttons,
    /// 5–8 the shoulders and triggers, then back, start, the stick clicks and home; the hat
    /// as D-pad; X/Y the left stick, the next two axes the right stick.
    pub fn guess(device: &str, raw: &RawInput) -> GenericLayout {
        use PadButton::*;
        let order = [A, B, X, Y, LB, RB];
        let mut buttons: Vec<ButtonBind> = order
            .iter()
            .zip(0..)
            .map(|(&button, index)| ButtonBind {
                button,
                from: RawSource::Button { index },
            })
            .collect();
        buttons.extend(
            [(8, Back), (9, Start), (10, LS), (11, RS), (12, Guide)].map(|(index, button)| {
                ButtonBind {
                    button,
                    from: RawSource::Button { index },
                }
            }),
        );
        if !raw.hats.is_empty() {
            for (dir, button) in [
                (HatDir::Up, DpadUp),
                (HatDir::Down, DpadDown),
                (HatDir::Left, DpadLeft),
                (HatDir::Right, DpadRight),
            ] {
                buttons.push(ButtonBind {
                    button,
                    from: RawSource::Hat { index: 0, dir },
                });
            }
        }
        buttons.retain(|b| match b.from {
            RawSource::Button { index } => (index as usize) < raw.buttons.len(),
            _ => true,
        });
        let triggers = [(6, Trigger::Left), (7, Trigger::Right)]
            .into_iter()
            .filter(|&(index, _)| index < raw.buttons.len())
            .map(|(index, trigger)| TriggerBind {
                trigger,
                from: RawSource::Button { index: index as u8 },
            })
            .collect();
        let sticks = [
            (Axis::LeftX, 0, false),
            (Axis::LeftY, 1, true),
            (Axis::RightX, 2, false),
            (Axis::RightY, 3, true),
        ]
        .into_iter()
        .filter(|&(_, from, _)| (from as usize) < raw.axes.len())
        .map(|(axis, from, invert)| StickBind { axis, from, invert })
        .collect();
        GenericLayout {
            device: device.to_owned(),
            buttons,
            triggers,
            sticks,
        }
    }
}

/// The raw input of a pad's last reports, handed out without allocating: of its three
/// buffers the engine holds at most one (from its last poll) and the device's published state
/// another, so one is free to decode the next report into.
struct RawRing {
    bufs: [Arc<RawInput>; 3],
    /// The buffer with the latest input; `None` before the first report.
    latest: Option<usize>,
}

impl RawRing {
    fn new() -> RawRing {
        RawRing {
            bufs: std::array::from_fn(|_| Arc::default()),
            latest: None,
        }
    }

    /// Decode `report` into a free buffer, on top of the latest input (a report may carry only
    /// some controls); the input then, or `None` for a report without the pad's controls.
    fn decode(&mut self, decoder: &mut dyn ReportDecoder, report: &[u8]) -> Option<&RawInput> {
        let after = self.latest.map_or(0, |l| l + 1);
        let free = (0..3)
            .map(|i| (after + i) % 3)
            .find(|&i| Some(i) != self.latest && Arc::strong_count(&self.bufs[i]) == 1)
            // All held after all: one is copied.
            .unwrap_or(after % 3);
        let latest = self.latest.map(|l| self.bufs[l].clone());
        let buf = Arc::make_mut(&mut self.bufs[free]);
        if let Some(latest) = latest {
            buf.clone_from(&latest);
        }
        if !decoder.decode(report, buf) {
            return None;
        }
        self.latest = Some(free);
        Some(buf)
    }

    fn latest(&self) -> Option<Arc<RawInput>> {
        self.latest.map(|l| self.bufs[l].clone())
    }
}

pub struct GenericDriver {
    name: String,
    device: String,
    decoder: Box<dyn ReportDecoder>,
    /// All layouts as last handed over, and the one for this model.
    layouts: Option<Arc<Vec<GenericLayout>>>,
    layout: Option<GenericLayout>,
    guess: Option<GenericLayout>,
    raw: RawRing,
    /// Whether the user's layout was in use when the notice was last reported.
    noticed: bool,
}

const NOTICE: &str = "buttons not identified yet; choose \"Identify buttons\" in its menu";

impl GenericDriver {
    pub fn new(name: String, vid: u16, pid: u16, decoder: Box<dyn ReportDecoder>) -> Self {
        GenericDriver {
            name,
            device: model_key(vid, pid),
            decoder,
            layouts: None,
            layout: None,
            guess: None,
            raw: RawRing::new(),
            noticed: false,
        }
    }
}

impl PadDriver for GenericDriver {
    fn init(&mut self, _io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        Ok(Identity {
            notice: Some(NOTICE.into()),
            ..Identity::new(self.name.clone(), PadFamily::Generic)
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        let raw = self.raw.decode(self.decoder.as_mut(), report)?;
        Some(match &self.layout {
            Some(l) => l.apply(raw),
            None => self
                .guess
                .get_or_insert_with(|| GenericLayout::guess(&self.device, raw))
                .apply(raw),
        })
    }

    fn raw(&self) -> Option<Arc<RawInput>> {
        self.raw.latest()
    }

    fn set_layouts(&mut self, layouts: &Arc<Vec<GenericLayout>>) {
        if self
            .layouts
            .as_ref()
            .is_some_and(|l| Arc::ptr_eq(l, layouts))
        {
            return;
        }
        self.layouts = Some(layouts.clone());
        self.layout = layouts.iter().find(|l| l.device == self.device).cloned();
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn take_notice_change(&mut self) -> Option<Option<String>> {
        let known = self.layout.is_some();
        if self.noticed == known {
            return None;
        }
        self.noticed = known;
        Some((!known).then(|| NOTICE.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::padio::testing::ScriptIo;

    struct Fixed(RawInput);

    impl ReportDecoder for Fixed {
        fn decode(&mut self, _report: &[u8], out: &mut RawInput) -> bool {
            out.clone_from(&self.0);
            true
        }
    }

    /// Counts its reports into the first button's place, and takes only reports `[1]`.
    struct Counter;

    impl ReportDecoder for Counter {
        fn decode(&mut self, report: &[u8], out: &mut RawInput) -> bool {
            if report != [1] {
                return false;
            }
            out.resize(1, 1, 0);
            out.axes[0] += 1.0;
            true
        }
    }

    #[test]
    fn raw_input_is_decoded_in_place() {
        let mut ring = RawRing::new();
        assert!(ring.latest().is_none());
        let mut held = Vec::new();
        for n in 1..=10 {
            assert_eq!(ring.decode(&mut Counter, &[1]).unwrap().axes, [n as f32]);
            assert!(ring.decode(&mut Counter, &[2]).is_none(), "not the pad's");
            // The engine keeps the last one it polled, the device's state the latest.
            held.push(ring.latest().unwrap());
            if held.len() > 2 {
                held.remove(0);
            }
        }
        let addresses: Vec<*const RawInput> = ring.bufs.iter().map(Arc::as_ptr).collect();
        for _ in 0..10 {
            ring.decode(&mut Counter, &[1]);
            held.push(ring.latest().unwrap());
            held.remove(0);
        }
        let again: Vec<*const RawInput> = ring.bufs.iter().map(Arc::as_ptr).collect();
        assert_eq!(addresses, again, "no buffer was made anew");
        assert_eq!(ring.latest().unwrap().axes, [20.0]);
    }

    fn raw() -> RawInput {
        let mut buttons = vec![false; 12];
        buttons[0] = true; // button 1
        buttons[7] = true; // button 8
        RawInput {
            buttons,
            axes: vec![0.0, -1.0, 0.5, 0.0, -1.0],
            hats: vec![Some(1)], // up-right
        }
    }

    #[test]
    fn hat_directions_include_diagonals() {
        assert!(HatDir::Up.covers(7) && HatDir::Up.covers(0) && HatDir::Up.covers(1));
        assert!(!HatDir::Up.covers(2) && !HatDir::Up.covers(6));
        assert!(HatDir::Left.covers(5) && HatDir::Left.covers(7) && !HatDir::Left.covers(0));
    }

    #[test]
    fn the_guess_follows_the_directinput_order() {
        let g = GenericLayout::guess("0079:0006", &raw());
        let s = g.apply(&raw());
        assert!(s.pressed(PadButton::A) && !s.pressed(PadButton::B));
        assert!(s.pressed(PadButton::DpadUp) && s.pressed(PadButton::DpadRight));
        assert_eq!(s.axis(Axis::RightTrigger), 1.0, "button 8");
        assert_eq!(s.axis(Axis::LeftY), 1.0, "HID y is inverted");
        assert_eq!(s.axis(Axis::RightX), 0.5);
        assert!(
            !g.buttons
                .iter()
                .any(|b| b.from == RawSource::Button { index: 12 }),
            "no button 13 on this pad"
        );
    }

    #[test]
    fn axis_sources_scale_from_rest_to_full() {
        let r = raw();
        let pull = |rest, full| {
            RawSource::Axis {
                index: 4,
                rest,
                full,
            }
            .level(&r)
        };
        assert_eq!(pull(-1.0, 1.0), 0.0, "at rest");
        let half = RawSource::Axis {
            index: 2,
            rest: 0.0,
            full: 1.0,
        };
        assert_eq!(half.level(&r), 0.5);
        let other_way = RawSource::Axis {
            index: 2,
            rest: 0.0,
            full: -1.0,
        };
        assert_eq!(other_way.level(&r), 0.0, "shared axis: the other trigger");
    }

    #[test]
    fn a_layout_replaces_the_guess() {
        let mut d = GenericDriver::new("Pad".into(), 0x0079, 0x0006, Box::new(Fixed(raw())));
        let mut io = ScriptIo::silent();
        assert!(d.init(&mut io).unwrap().notice.is_some());
        assert_eq!(d.take_notice_change(), None);
        assert!(d.parse(&[0], Instant::now()).unwrap().pressed(PadButton::A));
        let layout = GenericLayout {
            device: "0079:0006".into(),
            buttons: vec![ButtonBind {
                button: PadButton::Start,
                from: RawSource::Button { index: 0 },
            }],
            triggers: vec![TriggerBind {
                trigger: Trigger::Left,
                from: RawSource::Axis {
                    index: 2,
                    rest: 0.0,
                    full: 1.0,
                },
            }],
            sticks: vec![],
        };
        d.set_layouts(&Arc::new(vec![layout]));
        let s = d.parse(&[0], Instant::now()).unwrap();
        assert!(s.pressed(PadButton::Start) && !s.pressed(PadButton::A));
        assert_eq!(s.axis(Axis::LeftTrigger), 0.5);
        assert_eq!(d.take_notice_change(), Some(None), "identified");
        assert_eq!(d.raw().unwrap().buttons.len(), 12);
    }

    #[test]
    fn layouts_round_trip() {
        let g = GenericLayout::guess("0079:0006", &raw());
        let text = serde_json::to_string(&g).unwrap();
        assert!(
            text.contains(r#"{"kind":"hat","index":0,"dir":"up"}"#),
            "{text}"
        );
        let back: GenericLayout = serde_json::from_str(&text).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn layouts_need_a_model_and_a_mapping() {
        let mut l = GenericLayout {
            device: "pad".into(),
            ..Default::default()
        };
        assert!(l.problem().unwrap().contains("not a controller model"));
        l.device = "0079:0006".into();
        assert_eq!(l.problem().as_deref(), Some("the layout maps nothing"));
        l.sticks.push(StickBind {
            axis: Axis::LeftX,
            from: 0,
            invert: false,
        });
        assert_eq!(l.problem(), None);
    }
}
