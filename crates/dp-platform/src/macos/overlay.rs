//! Overlay windows: windows over games that never take the focus. The window itself is
//! created by the app; these calls only adjust how AppKit shows it. They must run on the main
//! thread, as AppKit wants.
//!
//! An overlay sits at the screen saver's level, above the menu bar and the Dock, and joins
//! every Space as an auxiliary of full-screen windows. That alone does not bring the app's
//! windows into another program's full-screen Space: macOS shows only a non-activating panel
//! there. So each overlay window is turned into one, changing its class to a panel class
//! (NSPanel adds nothing to NSWindow's layout, and the class keeps the one variable tao's
//! window class adds). That is checked on each window first: a window whose class is laid out
//! otherwise stays as it is.

use std::ffi::c_void;
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{ClassType, sel};
use objc2_app_kit::{
    NSPanel, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};

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
    let (class, panel) = (w.class(), panel_class());
    if same_layout(class, panel) {
        // SAFETY: a live window, whose instance variables the panel class lays out the same,
        // as checked just now.
        unsafe { objc2::ffi::object_setClass(ns_window.cast(), panel) };
        w.setStyleMask(w.styleMask() | NSWindowStyleMask::NonactivatingPanel);
    } else {
        tracing::warn!(
            class = ?class.name(),
            "the overlay's window class is not laid out as expected; it stays a plain window, \
             which does not show over full-screen games"
        );
    }
    place(w);
}

/// Whether a window of `class` can become one of `panel`: the same size, and tao's
/// `focusable` variable in the same place. tao is not pinned, so a release that lays its window
/// class out differently would otherwise turn the class change into memory corruption.
fn same_layout(class: &AnyClass, panel: &AnyClass) -> bool {
    let focusable = |c: &AnyClass| c.instance_variable(c"focusable").map(|i| i.offset());
    class.instance_size() == panel.instance_size()
        && focusable(class).is_some()
        && focusable(class) == focusable(panel)
}

/// A panel class for windows tao made: NSPanel, plus tao's `focusable` variable (its window
/// class's only one, after NSWindow's, where NSPanel adds none), never key or main.
fn panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        extern "C" fn no(_: *mut AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        let mut class = ClassBuilder::new(c"DoppelpadOverlayPanel", NSPanel::class())
            .expect("the overlay panel class is registered once");
        class.add_ivar::<Bool>(c"focusable");
        // SAFETY: the methods' signatures are those of the selectors.
        unsafe {
            class.add_method(sel!(canBecomeKeyWindow), no as extern "C" fn(_, _) -> _);
            class.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
        }
        class.register()
    })
}

const BEHAVIOR: NSWindowCollectionBehavior = NSWindowCollectionBehavior(
    NSWindowCollectionBehavior::CanJoinAllSpaces.0
        | NSWindowCollectionBehavior::FullScreenAuxiliary.0
        | NSWindowCollectionBehavior::Stationary.0
        | NSWindowCollectionBehavior::IgnoresCycle.0,
);

fn place(w: &NSWindow) {
    w.setLevel(NSScreenSaverWindowLevel);
    w.setCollectionBehavior(BEHAVIOR);
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
        place(w);
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
