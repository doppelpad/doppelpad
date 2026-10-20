//! Overlay windows over games: transparent, top-most, click-through windows that never
//! take the focus, one per kind (`overlay-<kind>`), each sized to its content and placed
//! in a corner of a monitor. They load `overlay.html`, which picks its content by the
//! window label. They live apart from the main window, so they work while it is hidden
//! in the tray.

use tauri::{
    AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::settings::Corner;

/// Gap between an overlay and the edge of the monitor's work area, in logical pixels.
const MARGIN: f64 = 24.0;

pub fn label(kind: &str) -> String {
    format!("overlay-{kind}")
}

/// The overlay window of `kind`, created hidden on first use. Returns whether it was created
/// now (its page is still loading).
pub fn window(app: &AppHandle, kind: &str) -> tauri::Result<(WebviewWindow, bool)> {
    let label = label(kind);
    if let Some(w) = app.get_webview_window(&label) {
        return Ok((w, false));
    }
    let w = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay.html".into()))
        .title("Doppelpad overlay")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .inner_size(1.0, 1.0)
        .build()?;
    w.set_ignore_cursor_events(true)?;
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::prepare(hwnd.0 as isize);
    }
    Ok((w, true))
}

/// Monitors with the primary one first.
pub fn monitors(app: &AppHandle) -> Vec<Monitor> {
    let mut all = app.available_monitors().unwrap_or_default();
    if let Ok(Some(primary)) = app.primary_monitor()
        && let Some(i) = all
            .iter()
            .position(|m| m.name() == primary.name() && m.position() == primary.position())
    {
        let p = all.remove(i);
        all.insert(0, p);
    }
    all
}

/// Size `w` to `size` logical pixels (at the monitor's scale) and put it in `corner` of
/// monitor `index` (the primary one if there is no such monitor).
pub fn place(
    app: &AppHandle,
    w: &WebviewWindow,
    index: usize,
    corner: Corner,
    size: (f64, f64),
) -> tauri::Result<()> {
    let monitors = monitors(app);
    let Some(m) = monitors.get(index).or(monitors.first()) else {
        return Ok(());
    };
    let sf = m.scale_factor();
    let (width, height) = ((size.0 * sf).round() as i32, (size.1 * sf).round() as i32);
    let margin = (MARGIN * sf).round() as i32;
    let area = m.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => ax + margin,
        Corner::TopRight | Corner::BottomRight => ax + aw - width - margin,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => ay + margin,
        Corner::BottomLeft | Corner::BottomRight => ay + ah - height - margin,
    };
    w.set_size(PhysicalSize::new(width as u32, height as u32))?;
    w.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

/// Show `w` above everything else without activating it.
pub fn show(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::show(hwnd.0 as isize);
        return;
    }
    let _ = w.show();
}

pub fn hide(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::hide(hwnd.0 as isize);
        return;
    }
    let _ = w.hide();
}
