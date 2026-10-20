//! Gamepads without a driver of their own, read through the operating system's HID parser.
//!
//! They report buttons, axes and hat switches in an order only the maker knows. A
//! [`GenericLayout`], recorded by the user with the identify wizard and kept per model
//! (vendor and product ID), says which of them is which pad control. Until there is one, a
//! guess in the common DirectInput order is used.

use std::sync::Arc;
use std::time::Instant;

use dp_core::pad::{Axis, PadButton, PadState, Rumble, Trigger};
use serde::{Deserialize, Serialize};

use crate::PadFamily;
use crate::padio::{Identity, InitError, PadDriver, ReportIo};

/// What a generic pad reports, before any layout: buttons in HID order, axes in `-1..=1` by
/// usage (X, Y, Z, Rx, Ry, Rz, sliders, dials, then simulation controls), hat switches as a
/// direction 0–7 clockwise from up (`None` when centred).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RawInput {
    pub buttons: Vec<bool>,
    pub axes: Vec<f32>,
    pub hats: Vec<Option<u8>>,
}

/// Turns input reports into raw input; keeps what earlier reports said for devices that
/// spread their controls over several reports.
pub trait ReportDecoder: Send {
    fn decode(&mut self, report: &[u8]) -> Option<RawInput>;
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
pub fn model_key(vid: u16, pid: u16) -> String {
    format!("{vid:04x}:{pid:04x}")
}

/// Level above which a button counts as pressed.
const PRESS: f32 = 0.5;

impl GenericLayout {
    /// The pad state this layout makes of `raw`.
    pub fn apply(&self, raw: &RawInput) -> PadState {
        let mut s = PadState::default();
        for b in &self.buttons {
            if b.from.level(raw) > PRESS {
                s.set(b.button, true);
            }
        }
        for t in &self.triggers {
            let axis = match t.trigger {
                Trigger::Left => Axis::LeftTrigger,
                Trigger::Right => Axis::RightTrigger,
            };
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

pub struct GenericDriver {
    name: String,
    device: String,
    decoder: Box<dyn ReportDecoder>,
    /// All layouts as last handed over, and the one for this model.
    layouts: Option<Arc<Vec<GenericLayout>>>,
    layout: Option<GenericLayout>,
    guess: Option<GenericLayout>,
    raw: Option<RawInput>,
    /// Whether the user's layout was in use when the notice was last reported.
    noticed: Option<bool>,
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
            raw: None,
            noticed: Some(false),
        }
    }
}

impl PadDriver for GenericDriver {
    fn init(&mut self, _io: &mut dyn ReportIo) -> Result<Identity, InitError> {
        Ok(Identity {
            name: self.name.clone(),
            family: PadFamily::Generic,
            joycon: None,
            has_motion: false,
            has_touchpad: false,
            has_rumble: false,
            notice: Some(NOTICE.into()),
        })
    }

    fn parse(&mut self, report: &[u8], _now: Instant) -> Option<PadState> {
        let raw = self.decoder.decode(report)?;
        let state = match &self.layout {
            Some(l) => l.apply(&raw),
            None => self
                .guess
                .get_or_insert_with(|| GenericLayout::guess(&self.device, &raw))
                .apply(&raw),
        };
        self.raw = Some(raw);
        Some(state)
    }

    fn raw(&self) -> Option<RawInput> {
        self.raw.clone()
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

    fn service(&mut self, _io: &mut dyn ReportIo, _rumble: Rumble) -> std::io::Result<()> {
        Ok(())
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn take_notice_change(&mut self) -> Option<Option<String>> {
        let known = self.layout.is_some();
        if self.noticed == Some(known) {
            return None;
        }
        self.noticed = Some(known);
        Some((!known).then(|| NOTICE.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::padio::testing::ScriptIo;

    struct Fixed(RawInput);

    impl ReportDecoder for Fixed {
        fn decode(&mut self, _report: &[u8]) -> Option<RawInput> {
            Some(self.0.clone())
        }
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
}
