//! Overlay windows over games: transparent, top-most, click-through windows that never
//! take the focus, one per kind (`overlay-<kind>`), each sized to its content and placed
//! where its [`Placement`] says. They load `overlay.html`, which picks its content by the
//! window label. They live apart from the main window, so they work while it is hidden
//! in the tray.

use std::collections::HashMap;
use std::sync::Mutex;

use dp_service::SharedLive;

use serde::Serialize;
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::settings::Corner;

/// Gap between an overlay and the edge of the monitor's work area, in logical pixels.
const MARGIN: f64 = 24.0;

/// Run `watch` on a thread named `name` with the app and the service's live snapshot.
pub fn spawn_watcher(
    app: &AppHandle,
    name: &str,
    watch: impl FnOnce(AppHandle, SharedLive) + Send + 'static,
) -> tauri::Result<()> {
    let live = app.state::<crate::commands::AppState>().live.clone();
    let handle = app.clone();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || watch(handle, live))?;
    Ok(())
}

/// The overlay windows, each showing a page of `overlay.html`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    Osd,
    Crosshair,
    Radial,
    Pad,
    Mappings,
}

impl OverlayKind {
    const ALL: [OverlayKind; 5] = [
        OverlayKind::Osd,
        OverlayKind::Crosshair,
        OverlayKind::Radial,
        OverlayKind::Pad,
        OverlayKind::Mappings,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            OverlayKind::Osd => "osd",
            OverlayKind::Crosshair => "crosshair",
            OverlayKind::Radial => "radial",
            OverlayKind::Pad => "pad",
            OverlayKind::Mappings => "mappings",
        }
    }

    /// The kind of the overlay window labelled `label`.
    pub fn from_label(label: &str) -> Option<OverlayKind> {
        let name = label.strip_prefix("overlay-")?;
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }
}

impl std::fmt::Display for OverlayKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn label(kind: OverlayKind) -> String {
    format!("overlay-{kind}")
}

/// Where an overlay window goes.
#[derive(Clone, Copy, Debug)]
pub enum Placement {
    /// `size` logical pixels (at the monitor's scale) in `corner` of monitor `monitor` (see
    /// [`monitors`]; the primary one if there is no such monitor).
    Corner {
        monitor: usize,
        corner: Corner,
        size: (f64, f64),
    },
    /// A rectangle in physical pixels.
    Rect {
        position: PhysicalPosition<i32>,
        size: PhysicalSize<u32>,
    },
    /// Covering the primary monitor.
    PrimaryMonitor,
}

/// What each overlay page shows, kept for a page that is still loading.
#[derive(Default)]
pub struct Overlays {
    pages: Mutex<HashMap<OverlayKind, Page>>,
}

#[derive(Default)]
struct Page {
    /// The page has loaded and listens for its events.
    ready: bool,
    content: Option<Value>,
}

/// Show `content` in the overlay window of `kind`, placed at `placement`. A page that is
/// still loading asks for it with `overlay_ready` and then shows. Returns the window, or
/// `None` when it cannot be created (logged).
pub fn show_content(
    app: &AppHandle,
    kind: OverlayKind,
    placement: Placement,
    content: &impl Serialize,
) -> Option<WebviewWindow> {
    let (w, created) = ensure(app, kind, placement)?;
    present(app, &w, created, kind, content);
    Some(w)
}

/// The overlay window of `kind` at `placement`, created hidden on first use; also whether it
/// was created now (its page is still loading). Failures are logged.
pub fn ensure(
    app: &AppHandle,
    kind: OverlayKind,
    placement: Placement,
) -> Option<(WebviewWindow, bool)> {
    let (w, created) = match window(app, kind) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("cannot create the {kind} overlay: {e}");
            return None;
        }
    };
    if let Err(e) = place(app, &w, placement) {
        tracing::warn!("cannot place the {kind} overlay: {e}");
    }
    Some((w, created))
}

/// Show `content` in `w`, the overlay window of `kind`. A page that is still loading
/// (`created`) asks for it with `overlay_ready` and then shows.
fn present(
    app: &AppHandle,
    w: &WebviewWindow,
    created: bool,
    kind: OverlayKind,
    content: &impl Serialize,
) {
    let Ok(value) = serde_json::to_value(content) else {
        return;
    };
    let ready = {
        let state = app.state::<Overlays>();
        let mut pages = state.pages.lock().unwrap();
        let page = pages.entry(kind).or_default();
        if created {
            page.ready = false;
        }
        page.content = Some(value.clone());
        page.ready
    };
    if ready {
        // Named after the window: every page hears every event.
        let _ = app.emit_to(label(kind), &label(kind), &value);
        raise(w);
    }
}

/// Hide the overlay of `kind` and forget what it showed.
pub fn dismiss(app: &AppHandle, kind: OverlayKind) {
    if let Some(page) = app.state::<Overlays>().pages.lock().unwrap().get_mut(&kind) {
        page.content = None;
    }
    if let Some(w) = app.get_webview_window(&label(kind)) {
        hide(&w);
    }
}

/// An overlay page has loaded: it gets what it should show, if anything, and then shows.
#[tauri::command]
pub fn overlay_ready(window: WebviewWindow, state: State<Overlays>) -> Option<Value> {
    let kind = OverlayKind::from_label(window.label())?;
    let content = {
        let mut pages = state.pages.lock().unwrap();
        let page = pages.entry(kind).or_default();
        page.ready = true;
        page.content.clone()
    };
    if content.is_some() {
        raise(&window);
    }
    content
}

/// The overlay window of `kind`, created hidden on first use. Returns whether it was created
/// now (its page is still loading).
fn window(app: &AppHandle, kind: OverlayKind) -> tauri::Result<(WebviewWindow, bool)> {
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
        .inner_size(1.0, 1.0);
    let w = match crate::webview_data_dir() {
        Some(dir) => w.data_directory(dir),
        None => w,
    }
    .build()?;
    // GTK can only do this for a window that has been shown (tao panics on one that hasn't),
    // so on Linux `raise` does it.
    #[cfg(not(target_os = "linux"))]
    w.set_ignore_cursor_events(true)?;
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::prepare(hwnd.0 as isize);
    }
    #[cfg(target_os = "macos")]
    on_main(&w, |ns| {
        // SAFETY: the window's own NSWindow, on the main thread.
        unsafe { dp_platform::macos::overlay::prepare(ns) }
    });
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

/// The primary monitor.
pub fn primary_monitor(app: &AppHandle) -> Option<Monitor> {
    monitors(app).into_iter().next()
}

/// Size and move `w` to `placement`; without monitors it stays as it is.
fn place(app: &AppHandle, w: &WebviewWindow, placement: Placement) -> tauri::Result<()> {
    let Some((position, size)) = rect(app, placement) else {
        return Ok(());
    };
    w.set_size(size)?;
    w.set_position(position)?;
    Ok(())
}

/// Where `placement` puts a window, in physical pixels.
fn rect(
    app: &AppHandle,
    placement: Placement,
) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let (index, corner, size) = match placement {
        Placement::Rect { position, size } => return Some((position, size)),
        Placement::PrimaryMonitor => {
            let m = primary_monitor(app)?;
            return Some((*m.position(), *m.size()));
        }
        Placement::Corner {
            monitor,
            corner,
            size,
        } => (monitor, corner, size),
    };
    let monitors = monitors(app);
    let m = monitors.get(index).or(monitors.first())?;
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
    Some((
        PhysicalPosition::new(x, y),
        PhysicalSize::new(width as u32, height as u32),
    ))
}

/// Run `f` with `w`'s NSWindow on the main thread, where AppKit wants it.
#[cfg(target_os = "macos")]
fn on_main(w: &WebviewWindow, f: impl FnOnce(*mut std::ffi::c_void) + Send + 'static) {
    let window = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(ns) = window.ns_window() {
            f(ns);
        }
    });
}

/// Show `w` above everything else without activating it.
fn raise(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::show(hwnd.0 as isize);
        return;
    }
    // `show` would bring Doppelpad to the front and take the focus from the game.
    #[cfg(target_os = "macos")]
    on_main(w, |ns| {
        // SAFETY: the window's own NSWindow, on the main thread.
        unsafe { dp_platform::macos::overlay::show(ns) }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = w.show();
    // See `window`; repeating it on every show does no harm.
    #[cfg(target_os = "linux")]
    let _ = w.set_ignore_cursor_events(true);
}

pub fn hide(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::hide(hwnd.0 as isize);
        return;
    }
    #[cfg(target_os = "macos")]
    on_main(w, |ns| {
        // SAFETY: the window's own NSWindow, on the main thread.
        unsafe { dp_platform::macos::overlay::hide(ns) }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = w.hide();
}
