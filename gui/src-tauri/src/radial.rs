//! The radial menu over the game. A watcher follows the menu the live state names and shows
//! it in the `radial` overlay window, which covers the primary monitor while the menu is
//! open (for the tint); the page draws the ring at the centre plus the menu's offset. Image
//! icons travel inside the content, so the page needs no file access.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_core::profile::RadialMenu;
use dp_service::{LiveRadial, LiveState};
use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use crate::commands::AppState;
use crate::overlay;

const KIND: &str = "radial";
/// Pointing must feel immediate.
const WATCH_INTERVAL: Duration = Duration::from_millis(16);
/// Larger image files are not shown.
const IMAGE_MAX: u64 = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Content {
    menu: RadialMenu,
    pointed: Option<usize>,
    /// Per sector, its image file as a data URL (icons of the app's set stay names).
    images: Vec<Option<String>>,
}

fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() {
                ABC[(n >> shift & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// An image file as a data URL, if it is one of the formats a page shows and not too big.
fn image(path: &str) -> Option<String> {
    let mime = match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => return None,
    };
    if std::fs::metadata(path).ok()?.len() > IMAGE_MAX {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", base64(&data)))
}

fn content(r: &LiveRadial, cache: &mut HashMap<String, Option<String>>) -> Content {
    let images = r
        .menu
        .sectors
        .iter()
        .map(|s| {
            let looks_like_file = s.icon.contains(['/', '\\', '.']);
            looks_like_file
                .then(|| {
                    cache
                        .entry(s.icon.clone())
                        .or_insert_with(|| image(&s.icon))
                        .clone()
                })
                .flatten()
        })
        .collect();
    Content {
        menu: r.menu.clone(),
        pointed: r.pointed,
        images,
    }
}

fn show(app: &AppHandle, c: &Content) {
    if app.get_webview_window(&overlay::label(KIND)).is_none() {
        prepare(app);
    }
    match overlay::window(app, KIND) {
        Ok((w, created)) => overlay::present(app, &w, created, KIND, c),
        Err(e) => tracing::warn!("cannot create the radial menu overlay: {e}"),
    }
}

/// Create the hidden window, covering the primary monitor.
fn prepare(app: &AppHandle) {
    let monitors = overlay::monitors(app);
    let Some(m) = monitors.first() else {
        return;
    };
    match overlay::window(app, KIND) {
        Ok((w, _)) => {
            let (pos, size) = (m.position(), m.size());
            let _ = w.set_size(PhysicalSize::new(size.width, size.height));
            let _ = w.set_position(PhysicalPosition::new(pos.x, pos.y));
        }
        Err(e) => tracing::warn!("cannot create the radial menu overlay: {e}"),
    }
}

fn watch(app: AppHandle, live: Arc<Mutex<LiveState>>) {
    let mut shown: Option<(RadialMenu, Option<usize>)> = None;
    let mut cache = HashMap::new();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let (want, has_radial) = {
            let l = live.lock().unwrap();
            (l.radial.clone(), l.has_radial)
        };
        // Made ready early: a new webview takes a moment to load.
        if has_radial && app.get_webview_window(&overlay::label(KIND)).is_none() {
            prepare(&app);
        }
        let key = want.as_ref().map(|r| (r.menu.clone(), r.pointed));
        if key == shown {
            continue;
        }
        match &want {
            Some(r) => show(&app, &content(r, &mut cache)),
            None => {
                overlay::dismiss(&app, KIND);
                cache.clear();
            }
        }
        shown = key;
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
        .name("radial".into())
        .spawn(move || watch(handle, live))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
