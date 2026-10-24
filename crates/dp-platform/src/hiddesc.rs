//! HID report descriptors, read without the OS's parser: whether a device numbers its
//! reports, and a decoder for the input reports of a generic pad. macOS hands descriptors over
//! as they are; Windows has a parser of its own (`windows::hidparse`).
//!
//! The rules are those of the HID specification 1.11, section 6.2.2.

use crate::generic::{RawInput, ReportDecoder};

/// One item of a report descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// 0 main, 1 global, 2 local.
    pub kind: u8,
    pub tag: u8,
    pub data: &'a [u8],
}

impl Item<'_> {
    /// The data as an unsigned number (little-endian, up to four bytes).
    pub fn unsigned(&self) -> u32 {
        self.data
            .iter()
            .rev()
            .fold(0, |v, &b| (v << 8) | u32::from(b))
    }

    /// The data as a signed number, its top bit the sign.
    pub fn signed(&self) -> i32 {
        let bits = 8 * self.data.len() as u32;
        if bits == 0 {
            return 0;
        }
        let v = self.unsigned();
        if bits < 32 && v & (1 << (bits - 1)) != 0 {
            (i64::from(v) - (1i64 << bits)) as i32
        } else {
            v as i32
        }
    }
}

const MAIN: u8 = 0;
const GLOBAL: u8 = 1;
const LOCAL: u8 = 2;

// Main items.
const INPUT: u8 = 0x8;
const COLLECTION: u8 = 0xA;
const END_COLLECTION: u8 = 0xC;
const OUTPUT: u8 = 0x9;
const FEATURE: u8 = 0xB;
// Global items.
const USAGE_PAGE: u8 = 0x0;
const LOGICAL_MIN: u8 = 0x1;
const LOGICAL_MAX: u8 = 0x2;
const REPORT_SIZE: u8 = 0x7;
const REPORT_ID: u8 = 0x8;
const REPORT_COUNT: u8 = 0x9;
const PUSH: u8 = 0xA;
const POP: u8 = 0xB;
// Local items.
const USAGE: u8 = 0x0;
const USAGE_MIN: u8 = 0x1;
const USAGE_MAX: u8 = 0x2;

// Usage pages and usages.
const PAGE_DESKTOP: u16 = 0x01;
const PAGE_SIMULATION: u16 = 0x02;
const PAGE_BUTTON: u16 = 0x09;
/// X, Y, Z, Rx, Ry, Rz, slider, dial, wheel.
const DESKTOP_AXES: std::ops::RangeInclusive<u16> = 0x30..=0x38;
const HAT_SWITCH: u16 = 0x39;
/// More buttons than any pad has: the rest are ignored.
const MAX_BUTTONS: usize = 32;

/// The short items of a descriptor, in order; long items are skipped, and a truncated item
/// ends the list.
pub fn items(desc: &[u8]) -> impl Iterator<Item = Item<'_>> {
    let mut rest = desc;
    std::iter::from_fn(move || {
        loop {
            let (&prefix, tail) = rest.split_first()?;
            if prefix == 0xFE {
                // Long item: data size, tag, data.
                let size = usize::from(*tail.first()?);
                rest = tail.get(2 + size..)?;
                continue;
            }
            let size = match prefix & 0x03 {
                3 => 4,
                n => usize::from(n),
            };
            let data = tail.get(..size)?;
            rest = &tail[size..];
            return Some(Item {
                kind: (prefix >> 2) & 0x03,
                tag: prefix >> 4,
                data,
            });
        }
    })
}

/// Whether the device numbers its reports: then each report starts with its ID.
pub fn numbered(desc: &[u8]) -> bool {
    items(desc).any(|i| (i.kind, i.tag) == (GLOBAL, REPORT_ID) && i.unsigned() != 0)
}

/// The global state of the parser, which Push and Pop save and restore.
#[derive(Clone, Copy, Debug, Default)]
struct Globals {
    page: u16,
    min: i64,
    /// The logical maximum read signed and unsigned: see [`Globals::max`].
    max_signed: i64,
    max_unsigned: i64,
    size: u32,
    count: u32,
    report: u8,
}

impl Globals {
    /// The logical maximum. A minimum that is not negative makes it unsigned: many devices
    /// write 255 as one byte, which read signed would be -1.
    fn max(&self) -> i64 {
        if self.min >= 0 {
            self.max_unsigned
        } else {
            self.max_signed
        }
    }
}

/// One value in an input report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Field {
    report: u8,
    /// Bit offset after the report ID.
    offset: u32,
    bits: u32,
    page: u16,
    usage: u16,
    min: i64,
    max: i64,
}

impl Field {
    /// The raw value in `data` (the report without its ID), sign-extended when the range is
    /// signed; `None` when the report is too short.
    fn read(&self, data: &[u8]) -> Option<i64> {
        if self.bits == 0 || self.bits > 32 {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..self.bits {
            let bit = self.offset + i;
            let byte = *data.get((bit / 8) as usize)?;
            v |= u64::from((byte >> (bit % 8)) & 1) << i;
        }
        let v = v as i64;
        Some(if self.min < 0 && v & (1 << (self.bits - 1)) != 0 {
            v - (1 << self.bits)
        } else {
            v
        })
    }

    /// The value as an axis, `-1..=1` over the logical range.
    fn axis(&self, v: i64) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        (((v - self.min) as f32 / (self.max - self.min) as f32) * 2.0 - 1.0).clamp(-1.0, 1.0)
    }

    /// The value as a hat direction, 0–7 clockwise from up; `None` when centred.
    fn hat(&self, v: i64) -> Option<u8> {
        let d = v - self.min;
        match self.max - self.min + 1 {
            8 if (0..8).contains(&d) => Some(d as u8),
            4 if (0..4).contains(&d) => Some(d as u8 * 2),
            _ => None,
        }
    }
}

/// An array input of the button page: each slot holds the number of a pressed button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ButtonArray {
    field: Field,
    /// The button the logical minimum names.
    first: u16,
}

impl ButtonArray {
    /// The buttons a slot can name.
    fn buttons(&self) -> std::ops::RangeInclusive<u16> {
        let last = i64::from(self.first) + (self.field.max - self.field.min).clamp(0, 63);
        self.first..=u16::try_from(last).unwrap_or(u16::MAX)
    }

    /// The button a slot's value names; `None` for a value outside the range (nothing).
    fn button(&self, v: i64) -> Option<u16> {
        (self.field.min..=self.field.max)
            .contains(&v)
            .then(|| u16::try_from(i64::from(self.first) + v - self.field.min).ok())?
    }
}

/// The input fields of a descriptor that matter to a pad.
#[derive(Debug, Default)]
struct Layout {
    fields: Vec<Field>,
    button_arrays: Vec<ButtonArray>,
}

/// Collect the input fields of `desc`.
fn layout(desc: &[u8]) -> Layout {
    let mut g = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    let mut usages: Vec<(u16, u16)> = Vec::new();
    let mut usage_min: Option<(u16, u16)> = None;
    let mut usage_max: Option<(u16, u16)> = None;
    // The next free bit of each input report.
    let mut cursors: Vec<(u8, u32)> = Vec::new();
    let mut out = Layout::default();

    // A usage item's page: its own (four-byte usages) or the current one.
    let full = |item: &Item, page: u16| -> (u16, u16) {
        let v = item.unsigned();
        if item.data.len() == 4 {
            ((v >> 16) as u16, v as u16)
        } else {
            (page, v as u16)
        }
    };

    for item in items(desc) {
        match (item.kind, item.tag) {
            (GLOBAL, USAGE_PAGE) => g.page = item.unsigned() as u16,
            (GLOBAL, LOGICAL_MIN) => g.min = i64::from(item.signed()),
            (GLOBAL, LOGICAL_MAX) => {
                g.max_signed = i64::from(item.signed());
                g.max_unsigned = i64::from(item.unsigned());
            }
            (GLOBAL, REPORT_SIZE) => g.size = item.unsigned(),
            (GLOBAL, REPORT_ID) => g.report = item.unsigned() as u8,
            (GLOBAL, REPORT_COUNT) => g.count = item.unsigned(),
            (GLOBAL, PUSH) => stack.push(g),
            (GLOBAL, POP) => g = stack.pop().unwrap_or(g),
            (LOCAL, USAGE) => usages.push(full(&item, g.page)),
            (LOCAL, USAGE_MIN) => usage_min = Some(full(&item, g.page)),
            (LOCAL, USAGE_MAX) => usage_max = Some(full(&item, g.page)),
            (MAIN, INPUT) => {
                let at = match cursors.iter_mut().find(|(r, _)| *r == g.report) {
                    Some((_, c)) => c,
                    None => {
                        cursors.push((g.report, 0));
                        &mut cursors.last_mut().unwrap().1
                    }
                };
                let flags = item.unsigned();
                let constant = flags & 0x01 != 0;
                let variable = flags & 0x02 != 0;
                let field = |i: u32, (page, usage): (u16, u16)| Field {
                    report: g.report,
                    offset: *at + i * g.size,
                    bits: g.size,
                    page,
                    usage,
                    min: g.min,
                    max: g.max(),
                };
                if !constant && variable {
                    for i in 0..g.count {
                        let u = match (usage_min, usage_max) {
                            _ if (i as usize) < usages.len() => usages[i as usize],
                            (Some((p, lo)), Some((_, hi))) => {
                                (p, lo.saturating_add(i as u16).min(hi))
                            }
                            _ => match usages.last() {
                                Some(&u) => u,
                                None => continue,
                            },
                        };
                        out.fields.push(field(i, u));
                    }
                } else if !constant && let Some((PAGE_BUTTON, first)) = usage_min {
                    for i in 0..g.count {
                        out.button_arrays.push(ButtonArray {
                            field: field(i, (PAGE_BUTTON, first)),
                            first,
                        });
                    }
                }
                *at += g.size * g.count;
                usages.clear();
                usage_min = None;
                usage_max = None;
            }
            (MAIN, OUTPUT | FEATURE | COLLECTION | END_COLLECTION) => {
                usages.clear();
                usage_min = None;
                usage_max = None;
            }
            _ => {}
        }
    }
    out
}

/// Decodes a generic pad's input reports (report ID first, 0 when unnumbered) into its buttons
/// in usage order, its axes by usage (X, Y, Z, Rx, Ry, Rz, sliders, dials, then simulation
/// controls) and its hat switches, as Windows' parser does for the same pads.
pub struct Decoder {
    buttons: Vec<u16>,
    button_fields: Vec<Field>,
    button_arrays: Vec<ButtonArray>,
    axes: Vec<Field>,
    hats: Vec<Field>,
    state: RawInput,
}

impl Decoder {
    /// The decoder for a descriptor, or `None` if it has no buttons or axes.
    pub fn new(desc: &[u8]) -> Option<Decoder> {
        let layout = layout(desc);
        let mut buttons: Vec<u16> = layout
            .fields
            .iter()
            .filter(|f| f.page == PAGE_BUTTON)
            .map(|f| f.usage)
            .collect();
        for a in &layout.button_arrays {
            buttons.extend(a.buttons());
        }
        buttons.sort_unstable();
        buttons.dedup();
        buttons.truncate(MAX_BUTTONS);
        let mut axes: Vec<Field> = layout
            .fields
            .iter()
            .filter(|f| {
                (f.page == PAGE_DESKTOP && DESKTOP_AXES.contains(&f.usage))
                    || f.page == PAGE_SIMULATION
            })
            .copied()
            .collect();
        axes.sort_by_key(|f| (f.page, f.usage));
        axes.dedup_by_key(|f| (f.page, f.usage));
        let hats: Vec<Field> = layout
            .fields
            .iter()
            .filter(|f| f.page == PAGE_DESKTOP && f.usage == HAT_SWITCH)
            .copied()
            .collect();
        if buttons.is_empty() && axes.is_empty() {
            return None;
        }
        let button_fields = layout
            .fields
            .into_iter()
            .filter(|f| f.page == PAGE_BUTTON)
            .collect();
        Some(Decoder {
            state: RawInput {
                buttons: vec![false; buttons.len()],
                axes: vec![0.0; axes.len()],
                hats: vec![None; hats.len()],
            },
            buttons,
            button_fields,
            button_arrays: layout.button_arrays,
            axes,
            hats,
        })
    }
}

impl ReportDecoder for Decoder {
    fn decode(&mut self, report: &[u8]) -> Option<RawInput> {
        let (&id, data) = report.split_first()?;
        let mut any = false;
        // Buttons a report carries are set from it; the others stay as they were.
        for f in self.button_fields.iter().filter(|f| f.report == id) {
            if let (Some(v), Some(i)) = (f.read(data), self.button_index(f.usage)) {
                self.state.buttons[i] = v != 0;
                any = true;
            }
        }
        let arrays: Vec<ButtonArray> = self
            .button_arrays
            .iter()
            .filter(|a| a.field.report == id)
            .copied()
            .collect();
        let mut pressed = Vec::new();
        for a in &arrays {
            if let Some(v) = a.field.read(data) {
                any = true;
                pressed.extend(a.button(v));
            }
        }
        for (i, usage) in self.buttons.iter().enumerate() {
            if arrays.iter().any(|a| a.buttons().contains(usage)) {
                self.state.buttons[i] = pressed.contains(usage);
            }
        }
        for (i, f) in self.axes.iter().enumerate() {
            if f.report == id
                && let Some(v) = f.read(data)
            {
                self.state.axes[i] = f.axis(v);
                any = true;
            }
        }
        for (i, f) in self.hats.iter().enumerate() {
            if f.report == id
                && let Some(v) = f.read(data)
            {
                self.state.hats[i] = f.hat(v);
                any = true;
            }
        }
        any.then(|| self.state.clone())
    }
}

impl Decoder {
    fn button_index(&self, usage: u16) -> Option<usize> {
        self.buttons.iter().position(|&u| u == usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a DualSense's USB descriptor: generic desktop, game pad, report 1.
    const DUALSENSE: &[u8] = &[0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x30];
    /// A plain pad without report IDs: usage page, usage, collection, logical maximum 255.
    const PLAIN: &[u8] = &[0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x26, 0xFF, 0x00, 0xC0];

    /// A typical cheap USB pad: four 8-bit axes (X, Y, Z, Rz), twelve buttons, four bits of
    /// padding, a 4-bit hat (0–7, centred outside) and four bits of padding. No report IDs.
    const PAD: &[u8] = &[
        0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, // generic desktop, game pad, collection
        0x15, 0x00, 0x26, 0xFF, 0x00, // logical 0..255
        0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35, // X, Y, Z, Rz
        0x75, 0x08, 0x95, 0x04, 0x81, 0x02, // 4 × 8 bits, input variable
        0x05, 0x09, 0x19, 0x01, 0x29, 0x0C, // buttons 1–12
        0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x0C, 0x81, 0x02, // 12 × 1 bit
        0x75, 0x01, 0x95, 0x04, 0x81, 0x03, // padding
        0x05, 0x01, 0x09, 0x39, 0x15, 0x00, 0x25, 0x07, // hat 0–7
        0x75, 0x04, 0x95, 0x01, 0x81, 0x42, // 4 bits, null state
        0x75, 0x04, 0x95, 0x01, 0x81, 0x03, // padding
        0xC0,
    ];

    #[test]
    fn reads_items() {
        let all: Vec<Item> = items(PLAIN).collect();
        assert_eq!(all.len(), 5);
        assert_eq!((all[0].kind, all[0].tag, all[0].unsigned()), (1, 0, 1));
        assert_eq!((all[2].kind, all[2].tag), (0, 0x0A), "collection");
        assert_eq!(all[3].unsigned(), 255, "two data bytes");
        assert!(all[4].data.is_empty(), "end collection has no data");
        let neg = Item {
            kind: 1,
            tag: 1,
            data: &[0x81],
        };
        assert_eq!(neg.signed(), -127);
    }

    #[test]
    fn report_ids() {
        assert!(numbered(DUALSENSE));
        assert!(!numbered(PLAIN));
        assert!(!numbered(&[]));
    }

    #[test]
    fn long_and_truncated_items() {
        // A long item of two bytes, then a report ID.
        assert!(numbered(&[0xFE, 0x02, 0x10, 0xAA, 0xBB, 0x85, 0x03]));
        // A report ID item whose data byte is missing.
        assert!(!numbered(&[0x05, 0x01, 0x85]));
    }

    #[test]
    fn decodes_a_plain_pad() {
        let mut d = Decoder::new(PAD).unwrap();
        // ID 0, X 0 (left), Y 255 (down), Z 128, Rz 128, buttons 1 and 10, hat 2 (right).
        let raw = d
            .decode(&[0x00, 0x00, 0xFF, 0x80, 0x80, 0x01, 0x02, 0x02])
            .unwrap();
        assert_eq!(raw.buttons.len(), 12);
        assert!(raw.buttons[0] && raw.buttons[9]);
        assert_eq!(raw.buttons.iter().filter(|&&b| b).count(), 2);
        assert_eq!(raw.axes.len(), 4);
        assert_eq!((raw.axes[0], raw.axes[1]), (-1.0, 1.0));
        assert!(raw.axes[2].abs() < 0.01);
        assert_eq!(raw.hats, [Some(2)]);
        let centred = d.decode(&[0, 128, 128, 128, 128, 0, 0, 0x08]).unwrap();
        assert_eq!(centred.hats, [None], "8 is outside 0–7");
        assert!(d.decode(&[]).is_none());
    }

    #[test]
    fn numbered_reports_only_change_their_own_fields() {
        // Report 1: X and Y as signed 16-bit; report 2: eight buttons.
        let desc = [
            0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, //
            0x85, 0x01, 0x16, 0x00, 0x80, 0x26, 0xFF, 0x7F, // report 1, -32768..32767
            0x09, 0x30, 0x09, 0x31, 0x75, 0x10, 0x95, 0x02, 0x81, 0x02, //
            0x85, 0x02, 0x05, 0x09, 0x19, 0x01, 0x29, 0x08, // report 2, buttons 1–8
            0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, //
            0xC0,
        ];
        let mut d = Decoder::new(&desc).unwrap();
        let a = d.decode(&[0x01, 0x00, 0x80, 0xFF, 0x7F]).unwrap();
        assert_eq!((a.axes[0], a.axes[1]), (-1.0, 1.0));
        assert!(a.buttons.iter().all(|&b| !b));
        let b = d.decode(&[0x02, 0b1000_0001]).unwrap();
        assert!(b.buttons[0] && b.buttons[7]);
        assert_eq!(b.axes[0], -1.0, "kept from report 1");
        assert!(d.decode(&[0x03, 0x00]).is_none(), "unknown report");
    }

    #[test]
    fn one_byte_maximum_of_255_is_unsigned() {
        // Logical 0..255 written as 0x25 0xFF, which read signed would be -1.
        let desc = [
            0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x15, 0x00, 0x25, 0xFF, 0x09, 0x30, 0x75, 0x08,
            0x95, 0x01, 0x81, 0x02, 0xC0,
        ];
        let mut d = Decoder::new(&desc).unwrap();
        assert_eq!(d.decode(&[0, 255]).unwrap().axes, [1.0]);
        assert_eq!(d.decode(&[0, 0]).unwrap().axes, [-1.0]);
    }

    #[test]
    fn simulation_controls_come_after_the_desktop_axes() {
        // Accelerator and brake (simulation page), then X.
        let desc = [
            0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95,
            0x02, 0x05, 0x02, 0x09, 0xC4, 0x09, 0xC5, 0x81, 0x02, 0x05, 0x01, 0x09, 0x30, 0x95,
            0x01, 0x81, 0x02, 0xC0,
        ];
        let mut d = Decoder::new(&desc).unwrap();
        let raw = d.decode(&[0, 255, 0, 128]).unwrap();
        assert_eq!(raw.axes.len(), 3);
        assert!(raw.axes[0].abs() < 0.01, "X first");
        assert_eq!(
            (raw.axes[1], raw.axes[2]),
            (1.0, -1.0),
            "accelerator, brake"
        );
    }

    #[test]
    fn button_arrays_name_the_pressed_buttons() {
        // Two array slots of buttons 1–16 (0 = none).
        let desc = [
            0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x01,
            0x25, 0x10, 0x75, 0x08, 0x95, 0x02, 0x81, 0x00, 0xC0,
        ];
        let mut d = Decoder::new(&desc).unwrap();
        let raw = d.decode(&[0, 3, 0]).unwrap();
        assert_eq!(raw.buttons.len(), 16);
        assert!(raw.buttons[2]);
        assert_eq!(raw.buttons.iter().filter(|&&b| b).count(), 1);
        assert!(!d.decode(&[0, 0, 0]).unwrap().buttons.iter().any(|&b| b));
    }
}
