//! Decoding a generic gamepad's input reports with the Windows HID parser (`HidP_*`).

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidD_FreePreparsedData,
    HidD_GetPreparsedData, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsageValue, HidP_GetUsages,
    HidP_GetValueCaps, HidP_Input, PHIDP_PREPARSED_DATA,
};
use windows_sys::Win32::Foundation::HANDLE;

use crate::generic::{RawInput, ReportDecoder};

const PAGE_DESKTOP: u16 = 0x01;
const PAGE_SIMULATION: u16 = 0x02;
const PAGE_BUTTON: u16 = 0x09;
/// X, Y, Z, Rx, Ry, Rz, slider, dial, wheel.
const DESKTOP_AXES: std::ops::RangeInclusive<u16> = 0x30..=0x38;
const HAT_SWITCH: u16 = 0x39;
/// More buttons than any pad has: the rest are ignored.
const MAX_BUTTONS: usize = 32;

/// One value input of the device.
#[derive(Clone, Copy, Debug)]
struct Value {
    page: u16,
    usage: u16,
    link: u16,
    min: i32,
    max: i32,
    bits: u16,
    hat: bool,
}

impl Value {
    /// `-1..=1` for axes; 0–7 or `None` for hats.
    fn read(&self, raw: u32) -> (f32, Option<u8>) {
        let (min, max) = if self.max > self.min {
            (self.min as i64, self.max as i64)
        } else {
            // A full-width unsigned range that the parser read as signed (max = -1).
            (0, (1i64 << self.bits.min(32)) - 1)
        };
        let v = if min < 0 && self.bits > 0 && self.bits < 32 && raw & (1 << (self.bits - 1)) != 0 {
            raw as i64 - (1i64 << self.bits)
        } else {
            raw as i64
        };
        if self.hat {
            let steps = max - min + 1;
            let d = v - min;
            let dir = match steps {
                8 if (0..8).contains(&d) => Some(d as u8),
                4 if (0..4).contains(&d) => Some(d as u8 * 2),
                _ => None,
            };
            return (0.0, dir);
        }
        let span = (max - min) as f32;
        (
            (((v - min) as f32 / span) * 2.0 - 1.0).clamp(-1.0, 1.0),
            None,
        )
    }
}

pub struct HidDecoder {
    pp: PHIDP_PREPARSED_DATA,
    report_len: usize,
    /// Button usages in index order, and the link collections that hold buttons.
    buttons: Vec<u16>,
    button_links: Vec<u16>,
    axes: Vec<Value>,
    hats: Vec<Value>,
    state: RawInput,
}

// SAFETY: the preparsed data is a heap block owned by this value; the HID parser functions
// only read it and may be called from any thread.
unsafe impl Send for HidDecoder {}

impl Drop for HidDecoder {
    fn drop(&mut self) {
        // SAFETY: allocated by HidD_GetPreparsedData and freed once.
        unsafe { HidD_FreePreparsedData(self.pp) };
    }
}

impl HidDecoder {
    /// The device's input layout, or `None` if it has no buttons or axes.
    pub fn new(handle: HANDLE) -> Option<HidDecoder> {
        let mut pp: PHIDP_PREPARSED_DATA = 0;
        // SAFETY: valid handle; on success the data is owned by the decoder (freed in Drop).
        if !unsafe { HidD_GetPreparsedData(handle, &mut pp) } {
            return None;
        }
        let mut d = HidDecoder {
            pp,
            report_len: 0,
            buttons: Vec::new(),
            button_links: Vec::new(),
            axes: Vec::new(),
            hats: Vec::new(),
            state: RawInput::default(),
        };
        // SAFETY: zeroed plain struct filled by the parser.
        let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
        // SAFETY: `pp` is valid until the decoder drops, `caps` a valid out-pointer.
        if unsafe { HidP_GetCaps(pp, &mut caps) } != HIDP_STATUS_SUCCESS {
            return None;
        }
        d.report_len = caps.InputReportByteLength as usize;

        let mut n = caps.NumberInputButtonCaps;
        let mut bcaps = vec![HIDP_BUTTON_CAPS::default(); n as usize];
        if n > 0
            // SAFETY: the buffer holds `n` entries.
            && unsafe { HidP_GetButtonCaps(HidP_Input, bcaps.as_mut_ptr(), &mut n, pp) }
                == HIDP_STATUS_SUCCESS
        {
            for c in bcaps
                .iter()
                .take(n as usize)
                .filter(|c| c.UsagePage == PAGE_BUTTON)
            {
                // SAFETY: the union member matches IsRange.
                let (lo, hi) = unsafe {
                    if c.IsRange {
                        (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax)
                    } else {
                        (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage)
                    }
                };
                d.buttons.extend(lo..=hi);
                if !d.button_links.contains(&c.LinkCollection) {
                    d.button_links.push(c.LinkCollection);
                }
            }
        }
        d.buttons.sort_unstable();
        d.buttons.dedup();
        d.buttons.truncate(MAX_BUTTONS);

        let mut n = caps.NumberInputValueCaps;
        let mut vcaps = vec![HIDP_VALUE_CAPS::default(); n as usize];
        if n > 0
            // SAFETY: the buffer holds `n` entries.
            && unsafe { HidP_GetValueCaps(HidP_Input, vcaps.as_mut_ptr(), &mut n, pp) }
                == HIDP_STATUS_SUCCESS
        {
            for c in vcaps.iter().take(n as usize) {
                // SAFETY: the union member matches IsRange.
                let (lo, hi) = unsafe {
                    if c.IsRange {
                        (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax)
                    } else {
                        (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage)
                    }
                };
                for usage in lo..=hi {
                    let wanted = match c.UsagePage {
                        PAGE_DESKTOP => DESKTOP_AXES.contains(&usage) || usage == HAT_SWITCH,
                        PAGE_SIMULATION => true,
                        _ => false,
                    };
                    if !wanted {
                        continue;
                    }
                    let v = Value {
                        page: c.UsagePage,
                        usage,
                        link: c.LinkCollection,
                        min: c.LogicalMin,
                        max: c.LogicalMax,
                        bits: c.BitSize,
                        hat: c.UsagePage == PAGE_DESKTOP && usage == HAT_SWITCH,
                    };
                    if v.hat {
                        d.hats.push(v);
                    } else {
                        d.axes.push(v);
                    }
                }
            }
        }
        d.axes.sort_by_key(|v| (v.page, v.usage));
        d.axes.dedup_by_key(|v| (v.page, v.usage));
        if d.buttons.is_empty() && d.axes.is_empty() {
            return None;
        }
        d.state = RawInput {
            buttons: vec![false; d.buttons.len()],
            axes: vec![0.0; d.axes.len()],
            hats: vec![None; d.hats.len()],
        };
        Some(d)
    }

    fn value(&self, v: &Value, report: &mut [u8]) -> Option<u32> {
        let mut raw = 0u32;
        // SAFETY: valid preparsed data and report buffer of the given length.
        let status = unsafe {
            HidP_GetUsageValue(
                HidP_Input,
                v.page,
                v.link,
                v.usage,
                &mut raw,
                self.pp,
                report.as_ptr(),
                report.len() as u32,
            )
        };
        (status == HIDP_STATUS_SUCCESS).then_some(raw)
    }
}

impl ReportDecoder for HidDecoder {
    fn decode(&mut self, report: &[u8]) -> Option<RawInput> {
        let mut buf = report.to_vec();
        buf.resize(buf.len().max(self.report_len), 0);
        let mut any = false;
        // Buttons: the pressed usages of each collection that has buttons; a report that
        // does not carry them leaves them as they were.
        let mut pressed = Vec::new();
        let mut carried = false;
        for &link in &self.button_links {
            let mut usages = [0u16; 128];
            let mut n = usages.len() as u32;
            // SAFETY: as above; `usages` holds `n` entries.
            let status = unsafe {
                HidP_GetUsages(
                    HidP_Input,
                    PAGE_BUTTON,
                    link,
                    usages.as_mut_ptr(),
                    &mut n,
                    self.pp,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                )
            };
            if status == HIDP_STATUS_SUCCESS {
                carried = true;
                pressed.extend_from_slice(&usages[..n as usize]);
            }
        }
        if carried {
            any = true;
            for (i, usage) in self.buttons.iter().enumerate() {
                self.state.buttons[i] = pressed.contains(usage);
            }
        }
        for i in 0..self.axes.len() {
            let v = self.axes[i];
            if let Some(raw) = self.value(&v, &mut buf) {
                any = true;
                self.state.axes[i] = v.read(raw).0;
            }
        }
        for i in 0..self.hats.len() {
            let v = self.hats[i];
            if let Some(raw) = self.value(&v, &mut buf) {
                any = true;
                self.state.hats[i] = v.read(raw).1;
            }
        }
        any.then(|| self.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(min: i32, max: i32, bits: u16, hat: bool) -> Value {
        Value {
            page: PAGE_DESKTOP,
            usage: if hat { HAT_SWITCH } else { 0x30 },
            link: 0,
            min,
            max,
            bits,
            hat,
        }
    }

    #[test]
    fn axes_and_hats_from_raw_values() {
        let byte = value(0, 255, 8, false);
        assert_eq!(byte.read(0).0, -1.0);
        assert_eq!(byte.read(255).0, 1.0);
        let signed = value(-32768, 32767, 16, false);
        assert_eq!(signed.read(0x8000).0, -1.0, "sign-extended");
        assert!(signed.read(0).0.abs() < 0.001);
        let misread = value(0, -1, 16, false);
        assert_eq!(misread.read(0xFFFF).0, 1.0, "max read as -1");
        let hat8 = value(0, 7, 4, true);
        assert_eq!(hat8.read(2).1, Some(2));
        assert_eq!(hat8.read(8).1, None, "out of range: centred");
        let hat8_from_1 = value(1, 8, 4, true);
        assert_eq!(hat8_from_1.read(1).1, Some(0));
        assert_eq!(hat8_from_1.read(0).1, None);
        let hat4 = value(0, 3, 2, true);
        assert_eq!(hat4.read(3).1, Some(6), "left");
    }
}
