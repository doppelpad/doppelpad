//! Overlay windows: windows over games that never take the focus. The window itself is
//! created by the app; these calls only adjust how AppKit shows it. They must run on the main
//! thread, as AppKit wants.
//!
//! An overlay sits at the screen saver's level, above the menu bar and the Dock, and joins
//! every Space as an auxiliary of full-screen windows, so it shows over a game in full screen
//! too.

use std::ffi::c_void;

use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};

/// The window behind an `NSWindow` pointer, if there is one.
///
/// # Safety
/// `ns_window` is null or points to a live `NSWindow`, and the caller is on the main thread.
unsafe fn window<'a>(ns_window: *mut c_void) -> Option<&'a NSWindow> {
    // SAFETY: as the caller guarantees.
    unsafe { ns_window.cast::<NSWindow>().as_ref() }
}

/// Put the window above everything, on every Space, out of the window cycle, and keep it
/// when Doppelpad is hidden or in the background.
///
/// # Safety
/// As for [`window`].
pub unsafe fn prepare(ns_window: *mut c_void) {
    // SAFETY: as the caller guarantees.
    let Some(w) = (unsafe { window(ns_window) }) else {
        return;
    };
    w.setLevel(NSScreenSaverWindowLevel);
    w.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    w.setIgnoresMouseEvents(true);
    w.setHidesOnDeactivate(false);
    w.setCanHide(false);
}

/// Show the window in front without activating Doppelpad, so the game keeps the focus.
///
/// # Safety
/// As for [`window`].
pub unsafe fn show(ns_window: *mut c_void) {
    // SAFETY: as the caller guarantees.
    if let Some(w) = unsafe { window(ns_window) } {
        w.orderFrontRegardless();
    }
}

/// # Safety
/// As for [`window`].
pub unsafe fn hide(ns_window: *mut c_void) {
    // SAFETY: as the caller guarantees.
    if let Some(w) = unsafe { window(ns_window) } {
        w.orderOut(None);
    }
}
