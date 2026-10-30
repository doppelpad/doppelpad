//! Decoding a generic gamepad's input reports with the Windows HID parser (`HidP_*`), by the
//! rules every OS reads generic pads by ([`crate::generic::usages`]).

use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidD_FreePreparsedData,
    HidD_GetPreparsedData, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsageValue, HidP_GetUsages,
    HidP_GetValueCaps, HidP_Input, PHIDP_PREPARSED_DATA,
};
use windows_sys::Win32::Foundation::HANDLE;

use crate::generic::usages::{self, MAX_BUTTONS, PAGE_BUTTON};
use crate::generic::{RawInput, ReportDecoder};

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
            (i64::from(self.min), i64::from(self.max))
        } else {
            // A full-width unsigned range that the parser read as signed (max = -1).
            (0, (1i64 << self.bits.min(32)) - 1)
        };
        let v = usages::signed(raw, u32::from(self.bits), min);
        if self.hat {
            (0.0, usages::hat(v, min, max))
        } else {
            (usages::axis(v, min, max), None)
        }
    }
}

/// The parsed report descriptor of an open HID device, freed on drop.
pub(crate) struct PreparsedData(PHIDP_PREPARSED_DATA);

// SAFETY: a heap block owned by this value; the HID parser functions only read it and may
// be called from any thread.
unsafe impl Send for PreparsedData {}

impl PreparsedData {
    /// The preparsed data of the device behind `handle`.
    pub(crate) fn get(handle: HANDLE) -> Option<PreparsedData> {
        let mut pp: PHIDP_PREPARSED_DATA = 0;
        // SAFETY: valid handle and out-pointer; on success the block is ours.
        unsafe { HidD_GetPreparsedData(handle, &mut pp) }.then_some(PreparsedData(pp))
    }

    fn raw(&self) -> PHIDP_PREPARSED_DATA {
        self.0
    }

    /// The capabilities of the device's top-level collection.
    pub(crate) fn caps(&self) -> Option<HIDP_CAPS> {
        let mut caps = HIDP_CAPS::default();
        // SAFETY: valid preparsed data and out-pointer.
        (unsafe { HidP_GetCaps(self.0, &mut caps) } == HIDP_STATUS_SUCCESS).then_some(caps)
    }
}

impl Drop for PreparsedData {
    fn drop(&mut self) {
        // SAFETY: allocated by HidD_GetPreparsedData and freed once.
        unsafe { HidD_FreePreparsedData(self.0) };
    }
}

pub struct HidDecoder {
    pp: PreparsedData,
    report_len: usize,
    /// Button usages in index order, and the link collections that hold buttons.
    buttons: Vec<u16>,
    button_links: Vec<u16>,
    axes: Vec<Value>,
    hats: Vec<Value>,
    /// The report being decoded, padded to the device's input report length (kept from report
    /// to report, so that decoding does not allocate).
    buf: Vec<u8>,
}

impl HidDecoder {
    /// The device's input layout, or `None` if it has no buttons or axes.
    pub fn new(handle: HANDLE) -> Option<HidDecoder> {
        let pp = PreparsedData::get(handle)?;
        let caps = pp.caps()?;
        let raw = pp.raw();
        let mut d = HidDecoder {
            pp,
            report_len: caps.InputReportByteLength as usize,
            buttons: Vec::new(),
            button_links: Vec::new(),
            axes: Vec::new(),
            hats: Vec::new(),
            buf: Vec::new(),
        };

        let mut n = caps.NumberInputButtonCaps;
        let mut bcaps = vec![HIDP_BUTTON_CAPS::default(); n as usize];
        if n > 0
            // SAFETY: the buffer holds `n` entries.
            && unsafe { HidP_GetButtonCaps(HidP_Input, bcaps.as_mut_ptr(), &mut n, raw) }
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
        usages::order_buttons(&mut d.buttons);

        let mut n = caps.NumberInputValueCaps;
        let mut vcaps = vec![HIDP_VALUE_CAPS::default(); n as usize];
        if n > 0
            // SAFETY: the buffer holds `n` entries.
            && unsafe { HidP_GetValueCaps(HidP_Input, vcaps.as_mut_ptr(), &mut n, raw) }
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
                    let hat = usages::is_hat(c.UsagePage, usage);
                    if !hat && !usages::is_axis(c.UsagePage, usage) {
                        continue;
                    }
                    let v = Value {
                        page: c.UsagePage,
                        usage,
                        link: c.LinkCollection,
                        min: c.LogicalMin,
                        max: c.LogicalMax,
                        bits: c.BitSize,
                        hat,
                    };
                    if hat {
                        d.hats.push(v);
                    } else {
                        d.axes.push(v);
                    }
                }
            }
        }
        usages::order_axes(&mut d.axes, |v| (v.page, v.usage));
        if d.buttons.is_empty() && d.axes.is_empty() {
            return None;
        }
        Some(d)
    }
}

/// The raw value of `v` in `report`.
fn value(pp: &PreparsedData, v: &Value, report: &[u8]) -> Option<u32> {
    let mut raw = 0u32;
    // SAFETY: valid preparsed data and report buffer of the given length.
    let status = unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            v.page,
            v.link,
            v.usage,
            &mut raw,
            pp.raw(),
            report.as_ptr(),
            report.len() as u32,
        )
    };
    (status == HIDP_STATUS_SUCCESS).then_some(raw)
}

impl ReportDecoder for HidDecoder {
    fn decode(&mut self, report: &[u8], out: &mut RawInput) -> bool {
        out.resize(self.buttons.len(), self.axes.len(), self.hats.len());
        self.buf.clear();
        self.buf.extend_from_slice(report);
        self.buf.resize(report.len().max(self.report_len), 0);
        let buf = &mut self.buf;
        let mut any = false;
        // Buttons: the pressed usages of each collection that has buttons, one bit per button
        // index; a report that does not carry them leaves them as they were.
        let mut pressed = 0u32;
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
                    self.pp.raw(),
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                )
            };
            if status == HIDP_STATUS_SUCCESS {
                carried = true;
                for usage in usages.iter().take(n as usize) {
                    if let Some(i) = self.buttons.iter().position(|b| b == usage) {
                        pressed |= 1 << i;
                    }
                }
            }
        }
        const _: () = assert!(MAX_BUTTONS <= 32, "one bit each in `pressed`");
        if carried {
            any = true;
            for (i, b) in out.buttons.iter_mut().enumerate() {
                *b = pressed & (1 << i) != 0;
            }
        }
        for (v, axis) in self.axes.iter().zip(&mut out.axes) {
            if let Some(raw) = value(&self.pp, v, buf) {
                any = true;
                *axis = v.read(raw).0;
            }
        }
        for (v, hat) in self.hats.iter().zip(&mut out.hats) {
            if let Some(raw) = value(&self.pp, v, buf) {
                any = true;
                *hat = v.read(raw).1;
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(min: i32, max: i32, bits: u16, hat: bool) -> Value {
        Value {
            page: usages::PAGE_DESKTOP,
            usage: if hat { 0x39 } else { 0x30 },
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
