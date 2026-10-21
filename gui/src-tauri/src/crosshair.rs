//! The crosshair over the game. A watcher follows the crosshair the live state names and
//! shows it in the `crosshair` overlay window, centred on the primary monitor plus its
//! offset; on Windows the magnifier enlarges the screen around it, below the crosshair.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_core::profile::Crosshair;
use dp_service::LiveState;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use crate::commands::AppState;
use crate::overlay;

const KIND: &str = "crosshair";
const WATCH_INTERVAL: Duration = Duration::from_millis(50);
/// Room around the shape for anti-aliasing, in logical pixels.
const PAD: f64 = 2.0;

/// Screen centre (physical pixels) of the crosshair, and the primary monitor's height.
fn centre(app: &AppHandle, c: &Crosshair) -> Option<((i32, i32), f64, u32)> {
    let monitors = overlay::monitors(app);
    let m = monitors.first()?;
    let sf = m.scale_factor();
    let (pos, size) = (m.position(), m.size());
    let x = pos.x + size.width as i32 / 2 + (f64::from(c.offset[0]) * sf).round() as i32;
    let y = pos.y + size.height as i32 / 2 + (f64::from(c.offset[1]) * sf).round() as i32;
    Some(((x, y), sf, size.height))
}

/// The magnifier, started on first use.
#[cfg(windows)]
type Magnifier = Option<dp_platform::windows::Magnifier>;
#[cfg(not(windows))]
type Magnifier = ();

fn show(app: &AppHandle, c: &Crosshair, magnifier: &mut Magnifier) {
    let Some(((x, y), sf, height)) = centre(app, c) else {
        return;
    };
    let mut window = None;
    if c.visible {
        match overlay::window(app, KIND) {
            Ok((w, created)) => {
                // As the page draws it: the shape plus outline and line width on each side.
                let line = (f64::from(c.size) / 5.0 * f64::from(c.thickness) / 100.0).max(1.0);
                let logical = f64::from(c.size) + 2.0 * (f64::from(c.outline) + line + PAD);
                let side = (logical * sf).round() as i32;
                let _ = w.set_size(PhysicalSize::new(side as u32, side as u32));
                let _ = w.set_position(PhysicalPosition::new(x - side / 2, y - side / 2));
                overlay::present(app, &w, created, KIND, c);
                window = Some(w);
            }
            Err(e) => tracing::warn!("cannot create the crosshair overlay: {e}"),
        }
    } else {
        overlay::dismiss(app, KIND);
    }
    #[cfg(windows)]
    match c.magnifier {
        Some(m) => {
            let hwnd = window.and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
            let view = dp_platform::windows::MagnifierView {
                centre: (x, y),
                size: (f64::from(m.size) * f64::from(height)).round() as i32,
                zoom: m.zoom,
                exclude: hwnd.into_iter().collect(),
                below: hwnd,
            };
            magnifier
                .get_or_insert_with(dp_platform::windows::Magnifier::new)
                .show(view);
        }
        None => hide_magnifier(magnifier),
    }
    #[cfg(not(windows))]
    {
        let _ = (window, magnifier, height);
        if c.magnifier.is_some() {
            tracing::debug!("the magnifier needs Windows");
        }
    }
}

fn hide_magnifier(magnifier: &mut Magnifier) {
    #[cfg(windows)]
    if let Some(m) = magnifier {
        m.hide();
    }
    #[cfg(not(windows))]
    let _ = magnifier;
}

fn watch(app: AppHandle, live: Arc<Mutex<LiveState>>) {
    let mut magnifier = Magnifier::default();
    let mut shown: Option<Crosshair> = None;
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let want = live.lock().unwrap().crosshair.clone();
        if want == shown {
            continue;
        }
        match &want {
            Some(c) => show(&app, c, &mut magnifier),
            None => {
                overlay::dismiss(&app, KIND);
                hide_magnifier(&mut magnifier);
            }
        }
        shown = want;
    }
}

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let live = app
        .state::<AppState>()
        .service
        .lock()
        .unwrap()
        .live_shared();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("crosshair".into())
        .spawn(move || watch(handle, live))?;
    Ok(())
}
