//! Overlay windows over games: transparent, top-most, click-through windows that never
//! take the focus, one per kind (`overlay-<kind>`), each sized to its content and placed
//! where its [`Placement`] says. They load `overlay.html`, which picks its content by the
//! window label. They live apart from the main window, so they work while it is hidden
//! in the tray.
//!
//! On Wayland a window cannot place itself (KWin puts it in the middle of the screen) and may
//! take the focus from a game; there the overlays are layer-shell surfaces instead (see
//! [`layer`]), where the compositor supports them.

use std::collections::HashMap;
use std::sync::Mutex;

use dp_service::SharedLive;

use serde::Serialize;
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::settings::{Corner, ScreenRef};
use crate::state::lock;

/// Gap between an overlay and the edge of the monitor's work area, in logical pixels.
const MARGIN: f64 = 24.0;

/// Run `watch` on a thread named `name` with the app and the service's live snapshot.
pub fn spawn_watcher(
    app: &AppHandle,
    name: &str,
    watch: impl FnOnce(AppHandle, SharedLive) + Send + 'static,
) -> tauri::Result<()> {
    let live = app.state::<crate::state::AppState>().live.clone();
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
    let w = ensure(app, kind, placement)?;
    present(app, &w, kind, content);
    Some(w)
}

/// The overlay window of `kind` at `placement`, created hidden on first use. Failures are
/// logged.
pub fn ensure(app: &AppHandle, kind: OverlayKind, placement: Placement) -> Option<WebviewWindow> {
    let w = match window(app, kind) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("cannot create the {kind} overlay: {e}");
            return None;
        }
    };
    if let Err(e) = place(app, &w, placement) {
        tracing::warn!("cannot place the {kind} overlay: {e}");
    }
    Some(w)
}

/// Show `content` in `w`, the overlay window of `kind`. A page that is still loading asks for
/// it with `overlay_ready` and then shows.
fn present(app: &AppHandle, w: &WebviewWindow, kind: OverlayKind, content: &impl Serialize) {
    let Ok(value) = serde_json::to_value(content) else {
        return;
    };
    let ready = {
        let state = app.state::<Overlays>();
        let mut pages = lock(&state.pages);
        let page = pages.entry(kind).or_default();
        page.content = Some(value.clone());
        page.ready
    };
    if ready {
        // Named after the window: every page hears every event.
        let _ = app.emit_to(label(kind), &label(kind), &value);
        raise(w);
    }
}

/// Hide the overlay of `kind` and forget what it showed. The page forgets it too: the window
/// shows again before the next content reaches the page, and would show the old one meanwhile
/// (the last radial menu with its last sector lit).
pub fn dismiss(app: &AppHandle, kind: OverlayKind) {
    let ready = match lock(&app.state::<Overlays>().pages).get_mut(&kind) {
        Some(page) => {
            page.content = None;
            page.ready
        }
        None => false,
    };
    if ready {
        let _ = app.emit_to(label(kind), &label(kind), Value::Null);
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
        let mut pages = lock(&state.pages);
        let page = pages.entry(kind).or_default();
        page.ready = true;
        page.content.clone()
    };
    if content.is_some() {
        raise(&window);
    }
    content
}

/// The overlay window of `kind`, created hidden on first use.
fn window(app: &AppHandle, kind: OverlayKind) -> tauri::Result<WebviewWindow> {
    let label = label(kind);
    if let Some(w) = app.get_webview_window(&label) {
        return Ok(w);
    }
    // The new page asks for its content once it has loaded, which can be before `build`
    // returns: it must find itself not ready yet now, not after it said it is.
    lock(&app.state::<Overlays>().pages)
        .entry(kind)
        .or_default()
        .ready = false;
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
    #[cfg(target_os = "linux")]
    if layer::supported() {
        layer::init(&w);
    }
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        dp_platform::windows::overlay::prepare(hwnd.0 as isize);
    }
    #[cfg(target_os = "macos")]
    on_main(&w, |ns| {
        // SAFETY: the window's own NSWindow, on the main thread.
        unsafe { dp_platform::macos::overlay::prepare(ns) }
    });
    Ok(w)
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

/// [`monitors`] by name and place, as the settings keep a monitor.
pub fn screens(app: &AppHandle) -> Vec<ScreenRef> {
    monitors(app)
        .iter()
        .map(|m| ScreenRef {
            name: m.name().cloned().unwrap_or_default(),
            x: m.position().x,
            y: m.position().y,
        })
        .collect()
}

/// Size and move `w` to `placement`; without monitors it stays as it is.
fn place(app: &AppHandle, w: &WebviewWindow, placement: Placement) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    if layer::supported() {
        let screens: Vec<Screen> = monitors(app).iter().map(Screen::of).collect();
        if let Some(anchoring) = anchoring(placement, &screens) {
            layer::place(w, anchoring);
        }
        return Ok(());
    }
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

/// A monitor, as [`anchoring`] needs it: in physical pixels, and its scale.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only for layer surfaces
#[derive(Clone, Copy, Debug)]
struct Screen {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Screen {
    fn of(m: &Monitor) -> Screen {
        Screen {
            position: *m.position(),
            size: *m.size(),
            scale: m.scale_factor(),
        }
    }

    /// Where GDK has the monitor: its position in logical pixels.
    fn origin(&self) -> (i32, i32) {
        (
            (f64::from(self.position.x) / self.scale).round() as i32,
            (f64::from(self.position.y) / self.scale).round() as i32,
        )
    }

    fn contains(&self, p: PhysicalPosition<i32>) -> bool {
        let (x, y) = (self.position.x, self.position.y);
        (x..x + self.size.width as i32).contains(&p.x)
            && (y..y + self.size.height as i32).contains(&p.y)
    }
}

/// How a layer surface takes a placement, in logical pixels: the monitor (its origin), the
/// margin to each edge it is anchored to (`None`: not anchored), and its size (`None`:
/// stretched between its anchors).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Anchoring {
    monitor: (i32, i32),
    top: Option<i32>,
    right: Option<i32>,
    bottom: Option<i32>,
    left: Option<i32>,
    size: Option<(i32, i32)>,
}

/// Where `placement` puts a layer surface on `screens` (the primary one first). A corner keeps
/// [`MARGIN`] from the monitor's edges; the compositor keeps panels free besides.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn anchoring(placement: Placement, screens: &[Screen]) -> Option<Anchoring> {
    let margin = MARGIN.round() as i32;
    match placement {
        Placement::Corner {
            monitor,
            corner,
            size,
        } => {
            let s = screens.get(monitor).or(screens.first())?;
            let (top, left) = match corner {
                Corner::TopLeft => (true, true),
                Corner::TopRight => (true, false),
                Corner::BottomLeft => (false, true),
                Corner::BottomRight => (false, false),
            };
            Some(Anchoring {
                monitor: s.origin(),
                top: top.then_some(margin),
                right: (!left).then_some(margin),
                bottom: (!top).then_some(margin),
                left: left.then_some(margin),
                size: Some((size.0.round() as i32, size.1.round() as i32)),
            })
        }
        Placement::Rect { position, size } => {
            let s = screens
                .iter()
                .find(|s| s.contains(position))
                .or(screens.first())?;
            let logical = |v: i32| (f64::from(v) / s.scale).round() as i32;
            Some(Anchoring {
                monitor: s.origin(),
                top: Some(logical(position.y - s.position.y)),
                right: None,
                bottom: None,
                left: Some(logical(position.x - s.position.x)),
                size: Some((logical(size.width as i32), logical(size.height as i32))),
            })
        }
        Placement::PrimaryMonitor => Some(Anchoring {
            monitor: screens.first()?.origin(),
            top: Some(0),
            right: Some(0),
            bottom: Some(0),
            left: Some(0),
            size: None,
        }),
    }
}

/// Overlays as layer-shell surfaces (wlr-layer-shell, which KWin and wlroots compositors
/// support), through gtk-layer-shell: on the overlay layer above every window, fullscreen
/// games included, anchored where Doppelpad says, and never with the keyboard focus.
#[cfg(target_os = "linux")]
pub mod layer {
    use std::sync::OnceLock;

    use gtk::prelude::*;
    use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
    use tauri::WebviewWindow;

    use super::Anchoring;

    static SUPPORTED: OnceLock<bool> = OnceLock::new();

    /// Find out whether the compositor takes layer surfaces (not on X11). On the main
    /// thread, once GTK runs.
    pub fn detect() {
        let supported = gtk_layer_shell::is_supported();
        tracing::info!(
            "overlays: {}",
            if supported {
                "layer-shell surfaces"
            } else {
                "windows (no layer shell)"
            }
        );
        let _ = SUPPORTED.set(supported);
    }

    pub(super) fn supported() -> bool {
        SUPPORTED.get().copied().unwrap_or(false)
    }

    /// Make a new overlay window a layer surface, before it is first shown.
    pub(super) fn init(w: &WebviewWindow) {
        let window = w.clone();
        let _ = w.run_on_main_thread(move || {
            let Ok(gtk) = window.gtk_window() else {
                return;
            };
            // A window becomes a layer surface only before it is realized.
            if gtk.is_realized() {
                gtk.unrealize();
            }
            // tao gives every window on Wayland a title bar of its own.
            gtk.set_titlebar(None::<&gtk::Widget>);
            gtk.init_layer_shell();
            gtk.set_layer(Layer::Overlay);
            gtk.set_keyboard_mode(KeyboardMode::None);
            gtk.set_namespace("doppelpad-overlay");
        });
    }

    /// Anchor and size the layer surface `w` as `a` says.
    pub(super) fn place(w: &WebviewWindow, a: Anchoring) {
        let window = w.clone();
        let _ = w.run_on_main_thread(move || {
            let Ok(gtk) = window.gtk_window() else {
                return;
            };
            if let Some(display) = gtk::gdk::Display::default() {
                let monitor = (0..display.n_monitors())
                    .filter_map(|i| display.monitor(i))
                    .find(|m| {
                        let g = m.geometry();
                        (g.x(), g.y()) == a.monitor
                    });
                if let Some(m) = monitor {
                    gtk.set_monitor(&m);
                }
            }
            for (edge, margin) in [
                (Edge::Top, a.top),
                (Edge::Right, a.right),
                (Edge::Bottom, a.bottom),
                (Edge::Left, a.left),
            ] {
                gtk.set_anchor(edge, margin.is_some());
                gtk.set_layer_shell_margin(edge, margin.unwrap_or(0));
            }
            let (width, height) = a.size.unwrap_or((-1, -1));
            gtk.set_size_request(width, height);
            if let Some((width, height)) = a.size {
                gtk.resize(width, height);
            }
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A Steam Deck's screen, and a 4K monitor at 200 % to its right.
    fn screens() -> [Screen; 2] {
        [
            Screen {
                position: PhysicalPosition::new(0, 0),
                size: PhysicalSize::new(1280, 800),
                scale: 1.0,
            },
            Screen {
                position: PhysicalPosition::new(1280, 0),
                size: PhysicalSize::new(3840, 2160),
                scale: 2.0,
            },
        ]
    }

    #[test]
    fn corners_anchor_two_edges() {
        let toast = |monitor, corner| Placement::Corner {
            monitor,
            corner,
            size: (380.0, 84.0),
        };
        assert_eq!(
            anchoring(toast(0, Corner::BottomRight), &screens()),
            Some(Anchoring {
                monitor: (0, 0),
                top: None,
                right: Some(24),
                bottom: Some(24),
                left: None,
                size: Some((380, 84)),
            })
        );
        let top_left = anchoring(toast(1, Corner::TopLeft), &screens()).unwrap();
        assert_eq!(top_left.monitor, (640, 0), "GDK's logical position");
        assert_eq!((top_left.top, top_left.left), (Some(24), Some(24)));
        assert_eq!((top_left.bottom, top_left.right), (None, None));
        assert_eq!(top_left.size, Some((380, 84)), "logical, at any scale");
        assert_eq!(
            anchoring(toast(5, Corner::TopRight), &screens())
                .unwrap()
                .monitor,
            (0, 0),
            "a monitor that is gone: the primary one"
        );
        assert_eq!(anchoring(toast(0, Corner::TopLeft), &[]), None);
    }

    #[test]
    fn rectangles_go_by_their_monitor() {
        // The crosshair, centred on the second monitor.
        let rect = Placement::Rect {
            position: PhysicalPosition::new(1280 + 1880, 1040),
            size: PhysicalSize::new(80, 80),
        };
        assert_eq!(
            anchoring(rect, &screens()),
            Some(Anchoring {
                monitor: (640, 0),
                top: Some(520),
                right: None,
                bottom: None,
                left: Some(940),
                size: Some((40, 40)),
            })
        );
    }

    #[test]
    fn the_primary_monitor_is_covered() {
        let a = anchoring(Placement::PrimaryMonitor, &screens()).unwrap();
        assert_eq!(a.monitor, (0, 0));
        assert_eq!(
            [a.top, a.right, a.bottom, a.left],
            [Some(0); 4],
            "anchored to every edge"
        );
        assert_eq!(a.size, None);
    }
}
