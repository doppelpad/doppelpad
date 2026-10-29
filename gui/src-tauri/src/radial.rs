//! The radial menu over the game. A watcher follows the menu the live state names and shows
//! it in the `radial` overlay window, which covers the primary monitor while the menu is
//! open (for the tint); the page draws the ring at the menu's position plus its offset, for
//! `cursor` where the mouse pointer was as the menu opened. Image icons travel inside the
//! content, so the page needs no file access; they travel once per menu, and only the sector
//! pointed at follows the pointer.

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
pub(crate) struct Content {
    pub menu: RadialMenu,
    pub pointed: Option<usize>,
    /// Per sector, its image file as a data URL (icons of the app's set stay names).
    pub images: Vec<Option<String>>,
    /// Where the mouse pointer was as the menu opened, in the window's logical pixels.
    pub cursor: Option<[f64; 2]>,
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

/// The sector pointed at: all that changes while a menu is open, sent on its own (the
/// menu's pictures can take hundreds of kilobytes).
#[derive(Serialize)]
pub(crate) struct Pointed {
    pub pointed: Option<usize>,
}

/// What the overlay shows.
struct Shown {
    menu: RadialMenu,
    pointed: Option<usize>,
}

/// What the overlay needs for the menu the live state names (`want`), with `shown` on screen.
#[derive(Debug, PartialEq, Eq)]
enum Update {
    Nothing,
    /// Another menu, or the first one: the whole content.
    Menu,
    /// The same menu, another sector pointed at.
    Pointed,
    Hide,
}

fn update(shown: Option<&Shown>, want: Option<&LiveRadial>) -> Update {
    match (shown, want) {
        (None, None) => Update::Nothing,
        (Some(_), None) => Update::Hide,
        (Some(s), Some(w)) if s.menu == w.menu => {
            if s.pointed == w.pointed {
                Update::Nothing
            } else {
                Update::Pointed
            }
        }
        (_, Some(_)) => Update::Menu,
    }
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let mut shown: Option<Shown> = None;
    // Taken as a menu opens; its sub-menus open in the same place.
    let mut opened_at: Option<[f64; 2]> = None;
    let mut cache = HashMap::new();
    let mut generation = 0;
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let Some((g, l)) = live.since(generation) else {
            continue;
        };
        generation = g;
        // Made ready early: a new webview takes a moment to load.
        if l.has_radial && app.get_webview_window(&overlay::label(KIND)).is_none() {
            overlay::ensure(&app, KIND, Placement::PrimaryMonitor);
        }
        match (update(shown.as_ref(), l.radial.as_ref()), &l.radial) {
            (Update::Pointed, Some(r)) => {
                overlay::patch_content(&app, KIND, &Pointed { pointed: r.pointed });
                shown = Some(Shown {
                    menu: r.menu.clone(),
                    pointed: r.pointed,
                });
            }
            (Update::Menu, Some(r)) => {
                if shown.is_none() {
                    opened_at = cursor(&app);
                }
                let c = content(r, opened_at, &mut cache);
                overlay::show_content(&app, KIND, Placement::PrimaryMonitor, &c);
                shown = Some(Shown {
                    menu: r.menu.clone(),
                    pointed: r.pointed,
                });
            }
            (Update::Hide, _) => {
                overlay::dismiss(&app, KIND);
                cache.clear();
                shown = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_travels_once_and_then_only_the_sector_pointed_at() {
        let menu = |name: &str| RadialMenu {
            name: name.into(),
            ..RadialMenu::default()
        };
        let live = |name: &str, pointed| LiveRadial {
            menu: menu(name),
            pointed,
        };
        let shown = Shown {
            menu: menu("Tools"),
            pointed: None,
        };
        assert_eq!(update(None, None), Update::Nothing);
        assert_eq!(update(None, Some(&live("Tools", None))), Update::Menu);
        assert_eq!(
            update(Some(&shown), Some(&live("Tools", None))),
            Update::Nothing
        );
        assert_eq!(
            update(Some(&shown), Some(&live("Tools", Some(3)))),
            Update::Pointed
        );
        assert_eq!(
            update(Some(&shown), Some(&live("Weapons", None))),
            Update::Menu,
            "a sub-menu"
        );
        assert_eq!(update(Some(&shown), None), Update::Hide);
    }
}
