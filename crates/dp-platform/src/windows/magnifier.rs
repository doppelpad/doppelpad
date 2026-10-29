//! Screen magnifier: a round, top-most, click-through window that shows the screen around a
//! point enlarged, drawn by the Windows Magnification API. Its own thread owns the windows
//! and refreshes the picture about sixty times a second while it shows.

use std::ptr::{null, null_mut};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{CreateEllipticRgn, InvalidateRect, SetWindowRgn};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Magnification::{
    MAGTRANSFORM, MW_FILTERMODE_EXCLUDE, MagInitialize, MagSetWindowFilterList, MagSetWindowSource,
    MagSetWindowTransform, MagUninitialize, WC_MAGNIFIER,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_TOPMOST, LWA_ALPHA, MSG,
    PM_REMOVE, PeekMessageW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, WS_CHILD,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    WS_VISIBLE,
};

/// How often the picture is refreshed while the magnifier shows.
const FRAME: Duration = Duration::from_millis(16);
/// How often window messages are handled while it is hidden (broadcasts must not wait long).
const IDLE: Duration = Duration::from_millis(100);

/// What to show: the circle of diameter `size` (screen pixels) centred on `centre`, with the
/// screen below it enlarged `zoom` times.
#[derive(Clone, Debug, PartialEq)]
pub struct MagnifierView {
    pub centre: (i32, i32),
    pub size: i32,
    pub zoom: f32,
    /// Windows left out of the picture, such as a crosshair drawn above it (the magnifier
    /// always leaves itself out).
    pub exclude: Vec<isize>,
    /// Stay right below this window (a crosshair); otherwise above all others.
    pub below: Option<isize>,
}

impl MagnifierView {
    /// The zoom, within the range a config may ask for: the picture and the screen square it
    /// shows must agree on it.
    fn zoom(&self) -> f32 {
        let range = dp_core::profile::Magnifier::ZOOM;
        self.zoom.clamp(*range.start(), *range.end())
    }
}

enum Msg {
    Show(MagnifierView),
    Hide,
}

/// The magnifier thread. Dropping the handle closes its window.
pub struct Magnifier {
    tx: Sender<Msg>,
}

impl Default for Magnifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Magnifier {
    pub fn new() -> Magnifier {
        let (tx, rx) = crossbeam_channel::unbounded();
        if let Err(e) = std::thread::Builder::new()
            .name("dp-magnifier".into())
            .spawn(move || run(rx))
        {
            tracing::warn!("cannot start the magnifier: {e}");
        }
        Magnifier { tx }
    }

    pub fn show(&self, view: MagnifierView) {
        let _ = self.tx.send(Msg::Show(view));
    }

    pub fn hide(&self) {
        let _ = self.tx.send(Msg::Hide);
    }
}

fn run(rx: Receiver<Msg>) {
    // SAFETY: plain call, balanced by MagUninitialize below.
    if unsafe { MagInitialize() } == 0 {
        tracing::warn!(
            "the magnifier is unavailable: {}",
            std::io::Error::last_os_error()
        );
        return;
    }
    // SAFETY: returns this program's module handle.
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = super::register_class("doppelpad-magnifier", DefWindowProcW);
    // SAFETY: a registered class; the window lives on this thread.
    let host = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        )
    };
    let mag = if host.is_null() {
        null_mut()
    } else {
        // SAFETY: our layered window.
        unsafe { SetLayeredWindowAttributes(host, 0, 255, LWA_ALPHA) };
        // SAFETY: MagInitialize registered the magnifier class; `host` is its parent.
        unsafe {
            CreateWindowExW(
                0,
                WC_MAGNIFIER,
                null(),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                1,
                1,
                host,
                null_mut(),
                hinst,
                null(),
            )
        }
    };
    if mag.is_null() {
        tracing::warn!(
            "cannot create the magnifier window: {}",
            std::io::Error::last_os_error()
        );
    } else {
        // SAFETY: both windows were made on this thread.
        unsafe { serve(&rx, host, mag) };
    }
    if !host.is_null() {
        // SAFETY: our window (with its child), destroyed once.
        unsafe { DestroyWindow(host) };
    }
    // SAFETY: balances MagInitialize.
    unsafe { MagUninitialize() };
}

/// Follow the handle's requests until it is dropped.
///
/// # Safety
/// `host` and `mag` are the magnifier's windows, created on this thread.
unsafe fn serve(rx: &Receiver<Msg>, host: HWND, mag: HWND) {
    let mut shown: Option<MagnifierView> = None;
    loop {
        match rx.recv_timeout(if shown.is_some() { FRAME } else { IDLE }) {
            Ok(Msg::Show(v)) => {
                // SAFETY: as for this function.
                unsafe { place(host, mag, &v) };
                shown = Some(v);
            }
            Ok(Msg::Hide) => {
                // SAFETY: as for this function.
                unsafe { ShowWindow(host, SW_HIDE) };
                shown = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let mut msg = MSG::default();
        // SAFETY: pumps this thread's messages into a valid MSG.
        while unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            // SAFETY: a message PeekMessageW filled in.
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if let Some(v) = &shown {
            // SAFETY: as for this function.
            unsafe {
                MagSetWindowSource(mag, source(v));
                InvalidateRect(mag, null(), 1);
            }
        }
    }
}

/// The screen square the magnifier enlarges.
fn source(v: &MagnifierView) -> RECT {
    let side = ((v.size as f32 / v.zoom()).round() as i32).max(1);
    let (left, top) = (v.centre.0 - side / 2, v.centre.1 - side / 2);
    RECT {
        left,
        top,
        right: left + side,
        bottom: top + side,
    }
}

/// Size, place, round, zoom and show the windows for `v`.
///
/// # Safety
/// `host` and `mag` are the magnifier's windows, created on this thread.
unsafe fn place(host: HWND, mag: HWND, v: &MagnifierView) {
    let side = v.size.max(2);
    let (x, y) = (v.centre.0 - side / 2, v.centre.1 - side / 2);
    let after = v.below.map_or(HWND_TOPMOST, |h| h as HWND);
    let z = v.zoom();
    let mut t = MAGTRANSFORM {
        v: [z, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 1.0],
    };
    // The magnifier leaves itself out on its own. The list is set on every show, also when
    // empty, so a window left out before (an overlay since closed) does not stay left out.
    let mut list: Vec<HWND> = v.exclude.iter().map(|&h| h as HWND).collect();
    // SAFETY: as for this function; the region belongs to the window once set.
    unsafe {
        SetWindowPos(host, after, x, y, side, side, SWP_NOACTIVATE);
        SetWindowPos(mag, null_mut(), 0, 0, side, side, SWP_NOACTIVATE);
        SetWindowRgn(host, CreateEllipticRgn(0, 0, side + 1, side + 1), 1);
        MagSetWindowTransform(mag, &mut t);
        MagSetWindowFilterList(
            mag,
            MW_FILTERMODE_EXCLUDE,
            list.len() as i32,
            list.as_mut_ptr(),
        );
        ShowWindow(host, SW_SHOWNOACTIVATE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_shrinks_with_the_zoom() {
        let v = MagnifierView {
            centre: (960, 540),
            size: 200,
            zoom: 4.0,
            exclude: Vec::new(),
            below: None,
        };
        let r = source(&v);
        assert_eq!((r.left, r.top, r.right, r.bottom), (935, 515, 985, 565));
    }
}
