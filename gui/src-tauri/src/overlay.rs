//! Overlay windows over games: transparent, top-most, click-through windows that never
//! take the focus, one per kind (`overlay-<kind>`), each sized to its content and placed
//! in a corner of a monitor. They load `overlay.html`, which picks its content by the
//! window label. They live apart from the main window, so they work while it is hidden
//! in the tray.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::settings::Corner;

/// Gap between an overlay and the edge of the monitor's work area, in logical pixels.
const MARGIN: f64 = 24.0;

pub fn label(kind: &str) -> String {
    format!("overlay-{kind}")
}

/// What each overlay page shows, kept for a page that is still loading.
#[derive(Default)]
pub struct Overlays {
    pages: Mutex<HashMap<String, Page>>,
}

#[derive(Default)]
struct Page {
    /// The page has loaded and listens for its events.
    ready: bool,
    content: Option<Value>,
}

/// Show `content` in `w`, the overlay window of `kind`. A page that is still loading
/// (`created`) asks for it with `overlay_ready` and then shows.
pub fn present(
    app: &AppHandle,
    w: &WebviewWindow,
    created: bool,
    kind: &str,
    content: &impl Serialize,
) {
    let Ok(value) = serde_json::to_value(content) else {
        return;
    };
    let ready = {
        let state = app.state::<Overlays>();
        let mut pages = state.pages.lock().unwrap();
        let page = pages.entry(kind.to_owned()).or_default();
        if created {
            page.ready = false;
        }
        page.content = Some(value.clone());
        page.ready
    };
    if ready {
        // Named after the window: every page hears every event.
        let _ = app.emit_to(label(kind), &label(kind), &value);
        show(w);
    }
}

/// Hide the overlay of `kind` and forget what it showed.
pub fn dismiss(app: &AppHandle, kind: &str) {
    if let Some(page) = app.state::<Overlays>().pages.lock().unwrap().get_mut(kind) {
        page.content = None;
    }
    if let Some(w) = app.get_webview_window(&label(kind)) {
        hide(&w);
    }
}

/// An overlay page has loaded: it gets what it should show, if anything, and then shows.
#[tauri::command]
pub fn overlay_ready(window: WebviewWindow, state: State<Overlays>) -> Option<Value> {
    let kind = window.label().strip_prefix("overlay-")?.to_owned();
    let content = {
        let mut pages = state.pages.lock().unwrap();
        let page = pages.entry(kind).or_default();
        page.ready = true;
        page.content.clone()
    };
    if content.is_some() {
        show(&window);
    }
    content
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
