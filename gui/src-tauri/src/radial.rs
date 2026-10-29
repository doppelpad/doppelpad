//! The radial menu over the game. A watcher follows the menu the live state names and shows
//! it in the `radial` overlay window, which covers the primary monitor while the menu is
//! open (for the tint); the page draws the ring at the menu's position plus its offset, for
//! `cursor` where the mouse pointer was as the menu opened. Image icons travel inside the
//! content, so the page needs no file access.

use std::collections::HashMap;
use std::time::Duration;

use dp_core::profile::RadialMenu;
use dp_service::{LiveRadial, SharedLive};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::overlay::{self, OverlayKind, Placement};

const KIND: OverlayKind = OverlayKind::Radial;
/// Pointing must feel immediate.
const WATCH_INTERVAL: Duration = dp_service::LIVE_INTERVAL;
/// Larger image files are not shown.
const IMAGE_MAX: u64 = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Content {
    menu: RadialMenu,
    pointed: Option<usize>,
    /// Per sector, its image file as a data URL (icons of the app's set stay names).
    images: Vec<Option<String>>,
    /// Where the mouse pointer was as the menu opened, in the window's logical pixels.
    cursor: Option<[f64; 2]>,
}

/// The mouse pointer in the logical pixels of the window over the primary monitor.
fn cursor(app: &AppHandle) -> Option<[f64; 2]> {
    let at = app.cursor_position().ok()?;
    let m = overlay::primary_monitor(app)?;
    let sf = m.scale_factor();
    let origin = m.position();
    Some([
        (at.x - f64::from(origin.x)) / sf,
        (at.y - f64::from(origin.y)) / sf,
    ])
}

/// An image file as a data URL, if it is one of the formats a page shows and not too big.
fn image(path: &str) -> Option<String> {
    dp_config::picture::data_url(std::path::Path::new(path), IMAGE_MAX)
}

fn content(
    r: &LiveRadial,
    cursor: Option<[f64; 2]>,
    cache: &mut HashMap<String, Option<String>>,
) -> Content {
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
        cursor,
    }
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let mut shown: Option<(RadialMenu, Option<usize>)> = None;
    // Taken as a menu opens; its sub-menus open in the same place.
    let mut opened_at: Option<[f64; 2]> = None;
    let mut cache = HashMap::new();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let (want, has_radial) = {
            let l = live.get();
            (l.radial.clone(), l.has_radial)
        };
        // Made ready early: a new webview takes a moment to load.
        if has_radial && app.get_webview_window(&overlay::label(KIND)).is_none() {
            overlay::ensure(&app, KIND, Placement::PrimaryMonitor);
        }
        let key = want.as_ref().map(|r| (r.menu.clone(), r.pointed));
        if key == shown {
            continue;
        }
        match &want {
            Some(r) => {
                if shown.is_none() {
                    opened_at = cursor(&app);
                }
                let c = content(r, opened_at, &mut cache);
                overlay::show_content(&app, KIND, Placement::PrimaryMonitor, &c);
            }
            None => {
                overlay::dismiss(&app, KIND);
                cache.clear();
            }
        }
        shown = key;
    }
}
