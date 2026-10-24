//! The windows on screen, from the window server's list (front to back): which program is in
//! front, and which programs show a window. Window titles need the Screen Recording
//! permission; without it the program's name stands in.

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType, CGRect};
use objc2_core_graphics::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowBounds, kCGWindowLayer, kCGWindowName,
    kCGWindowOwnerName, kCGWindowOwnerPID,
};

/// One window of a program.
#[derive(Clone, Debug)]
pub struct Window {
    pub pid: u32,
    /// The program's name as the window server knows it.
    pub owner: Option<String>,
    pub title: Option<String>,
    pub bounds: CGRect,
}

/// The ordinary windows on screen (layer 0: no menu bar, Dock or overlays), front to back.
pub fn on_screen() -> Vec<Window> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, 0) else {
        return Vec::new();
    };
    // SAFETY: the window list holds dictionaries with string keys.
    let list: &objc2_core_foundation::CFArray<CFDictionary> = unsafe { list.cast_unchecked() };
    list.iter()
        .filter_map(|d| {
            // SAFETY: as above.
            let d: &CFDictionary<CFString, CFType> = unsafe { d.cast_unchecked() };
            // SAFETY: the keys are constants of CoreGraphics.
            let (layer, pid, owner, name, bounds) = unsafe {
                (
                    kCGWindowLayer,
                    kCGWindowOwnerPID,
                    kCGWindowOwnerName,
                    kCGWindowName,
                    kCGWindowBounds,
                )
            };
            let number =
                |k: &CFString| -> Option<i64> { d.get(k)?.downcast_ref::<CFNumber>()?.as_i64() };
            let string = |k: &CFString| -> Option<String> {
                let v: CFRetained<CFType> = d.get(k)?;
                let s = v.downcast_ref::<CFString>()?.to_string();
                (!s.trim().is_empty()).then_some(s)
            };
            if number(layer)? != 0 {
                return None;
            }
            Some(Window {
                pid: u32::try_from(number(pid)?).ok()?,
                owner: string(owner),
                title: string(name),
                bounds: d.get(bounds).and_then(|b| rect(&b)).unwrap_or_default(),
            })
        })
        .collect()
}

/// A window's bounds from its dictionary form (`X`, `Y`, `Width`, `Height`).
fn rect(v: &CFType) -> Option<CGRect> {
    let d = v.downcast_ref::<CFDictionary>()?;
    // SAFETY: the bounds dictionary has string keys and number values.
    let d: &CFDictionary<CFString, CFType> = unsafe { d.cast_unchecked() };
    let get = |k: &'static str| -> Option<f64> {
        d.get(&CFString::from_static_str(k))?
            .downcast_ref::<CFNumber>()?
            .as_f64()
    };
    Some(CGRect {
        origin: objc2_core_foundation::CGPoint {
            x: get("X")?,
            y: get("Y")?,
        },
        size: objc2_core_foundation::CGSize {
            width: get("Width")?,
            height: get("Height")?,
        },
    })
}

/// The frontmost ordinary window.
pub fn front() -> Option<Window> {
    on_screen().into_iter().next()
}
