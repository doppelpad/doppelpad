//! The live controller overlay and the overlay listing the current layer's mappings, both
//! switched by a command or a controller hotkey. The controller overlay follows the
//! controller in use and hides after a while without input; the mappings overlay shows the
//! running profile's mappings on the active layer. Both follow changes of their settings, and
//! the mappings overlay a saved config.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dp_core::profile::Profile;
use dp_platform::PadFamily;
use dp_service::{LiveDevice, LiveState, SharedLive};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::overlay::{self, OverlayKind, Placement};
use crate::settings::{Corner, PadOverlaySettings};
use crate::state::AppState;

const PAD: OverlayKind = OverlayKind::Pad;
const MAPPINGS: OverlayKind = OverlayKind::Mappings;
const WATCH_INTERVAL: Duration = Duration::from_millis(50);
/// Window sizes in logical pixels: the controller at 100 %, and three columns of mappings.
const PAD_SIZE: (f64, f64) = (600.0, 440.0);
const MAPPINGS_SIZE: (f64, f64) = (1180.0, 980.0);
/// Stick or trigger movement that counts as input for the auto-hide.
const ACTIVITY: f32 = 0.08;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct PadContent {
    device: LiveDevice,
    opacity: u8,
    corner: Corner,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct MappingsContent {
    profile: Profile,
    layer: u8,
    family: Option<PadFamily>,
    lang: String,
    opacity: u8,
    corner: Corner,
}

/// Buttons changed or an axis moved noticeably.
fn input_between(a: &LiveDevice, b: &LiveDevice) -> bool {
    a.id != b.id
        || a.state.buttons != b.state.buttons
        || a.state
            .axes
            .iter()
            .zip(b.state.axes.iter())
            .any(|(x, y)| (x - y).abs() > ACTIVITY)
}

/// `size` logical pixels in `corner` of the primary monitor.
fn in_corner(corner: Corner, size: (f64, f64)) -> Placement {
    Placement::Corner {
        monitor: 0,
        corner,
        size,
    }
}

#[derive(Default)]
struct Pad {
    /// What the overlay shows, with motion left out, and its settings; `None` while hidden.
    shown: Option<(LiveDevice, PadOverlaySettings)>,
    /// The last state seen, for the auto-hide.
    last: Option<LiveDevice>,
    last_input: Option<Instant>,
}

fn update_pad(app: &AppHandle, pad: &mut Pad, on: bool, device: Option<&LiveDevice>) {
    let Some(mut d) = device.filter(|_| on).cloned() else {
        if pad.shown.take().is_some() {
            overlay::dismiss(app, PAD);
        }
        pad.last = None;
        return;
    };
    let s = app.state::<AppState>().settings(|s| s.pad_overlay.clone());
    d.state.motion = None;
    let now = Instant::now();
    if pad.last.as_ref().is_none_or(|l| input_between(l, &d)) {
        pad.last_input = Some(now);
    }
    pad.last = Some(d.clone());
    let idle = s.hide_after > 0
        && pad
            .last_input
            .is_some_and(|t| now - t >= Duration::from_secs(u64::from(s.hide_after)));
    if idle {
        if pad.shown.take().is_some() {
            overlay::dismiss(app, PAD);
        }
        return;
    }
    let want = (d, s);
    if pad.shown.as_ref() != Some(&want) {
        let (d, s) = &want;
        let k = f64::from(s.size) / 100.0;
        let content = PadContent {
            device: d.clone(),
            opacity: s.opacity,
            corner: s.corner,
        };
        let placement = in_corner(s.corner, (PAD_SIZE.0 * k, PAD_SIZE.1 * k));
        overlay::show_content(app, PAD, placement, &content);
        pad.shown = Some(want);
    }
}

/// What the mappings overlay shows: the config and layer, and the versions of the library
/// and the settings it was made with (a saved config or other settings make it anew).
type MappingsKey = (String, u8, u64, u64);

fn update_mappings(
    app: &AppHandle,
    shown: &mut Option<MappingsKey>,
    on: bool,
    profile: Option<&str>,
    layer: u8,
    family: Option<PadFamily>,
) {
    let Some(path) = profile.filter(|_| on) else {
        if shown.take().is_some() {
            overlay::dismiss(app, MAPPINGS);
        }
        return;
    };
    let state = app.state::<AppState>();
    let key = (
        path.to_owned(),
        layer,
        state.library_version(),
        state.settings_version(),
    );
    if shown.as_ref() == Some(&key) {
        return;
    }
    let (lang, s) = state.settings(|s| (s.language.clone(), s.mappings_overlay.clone()));
    match dp_config::load(Path::new(path)) {
        Ok((profile, _)) => {
            let content = MappingsContent {
                profile,
                layer,
                family,
                lang,
                opacity: s.opacity,
                corner: s.corner,
            };
            let placement = in_corner(s.corner, MAPPINGS_SIZE);
            overlay::show_content(app, MAPPINGS, placement, &content);
        }
        Err(e) => tracing::warn!("cannot read {path} for the mappings overlay: {e:#}"),
    }
    *shown = Some(key);
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let mut pad = Pad::default();
    let mut mappings = None;
    let (mut generation, mut l) = (0, Arc::default());
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        // The live state anew when it changed; the auto-hide and saved configs or settings
        // are looked at on every pass.
        if let Some((g, state)) = live.since(generation) {
            (generation, l) = (g, state);
        }
        let l: &LiveState = &l;
        let device = l
            .primary
            .as_ref()
            .and_then(|id| l.devices.iter().find(|d| &d.id == id));
        let family = device.map(|d| d.family);
        update_pad(&app, &mut pad, l.pad_overlay, device);
        let profile = l.profile_path.as_deref();
        update_mappings(
            &app,
            &mut mappings,
            l.mappings_overlay,
            profile,
            l.layer,
            family,
        );
    }
}
