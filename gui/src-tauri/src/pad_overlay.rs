//! The live controller overlay and the overlay listing the current layer's mappings, both
//! switched by a command or a controller hotkey. The controller overlay follows the
//! controller in use and hides after a while without input; the mappings overlay shows the
//! running profile's mappings on the active layer.

use std::path::Path;
use std::time::{Duration, Instant};

use dp_core::profile::Profile;
use dp_platform::PadFamily;
use dp_service::{LiveDevice, SharedLive};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::overlay::{self, OverlayKind};
use crate::settings::Corner;

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

fn present<T: Serialize>(
    app: &AppHandle,
    kind: OverlayKind,
    corner: Corner,
    size: (f64, f64),
    content: &T,
) {
    match overlay::window(app, kind) {
        Ok((w, created)) => {
            if let Err(e) = overlay::place(app, &w, 0, corner, size) {
                tracing::warn!("cannot place the {kind} overlay: {e}");
            }
            overlay::present(app, &w, created, kind, content);
        }
        Err(e) => tracing::warn!("cannot create the {kind} overlay: {e}"),
    }
}

#[derive(Default)]
struct Pad {
    /// What the overlay shows, with motion left out; `None` while hidden.
    shown: Option<LiveDevice>,
    /// The last state seen, for the auto-hide.
    last: Option<LiveDevice>,
    last_input: Option<Instant>,
}

fn update_pad(app: &AppHandle, pad: &mut Pad, on: bool, device: Option<LiveDevice>) {
    let s = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .pad_overlay
        .clone();
    let Some(mut d) = device.filter(|_| on) else {
        if pad.shown.take().is_some() {
            overlay::dismiss(app, PAD);
        }
        pad.last = None;
        return;
    };
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
    if pad.shown.as_ref() != Some(&d) {
        let k = f64::from(s.size.clamp(30, 150)) / 100.0;
        let content = PadContent {
            device: d.clone(),
            opacity: s.opacity.clamp(20, 100),
            corner: s.corner,
        };
        present(
            app,
            PAD,
            s.corner,
            (PAD_SIZE.0 * k, PAD_SIZE.1 * k),
            &content,
        );
        pad.shown = Some(d);
    }
}

fn update_mappings(
    app: &AppHandle,
    shown: &mut Option<(String, u8)>,
    on: bool,
    profile: Option<String>,
    layer: u8,
    family: Option<PadFamily>,
) {
    let Some(path) = profile.filter(|_| on) else {
        if shown.take().is_some() {
            overlay::dismiss(app, MAPPINGS);
        }
        return;
    };
    let key = (path, layer);
    if shown.as_ref() == Some(&key) {
        return;
    }
    let (lang, s) = {
        let settings = app.state::<AppState>().settings.lock().unwrap().clone();
        (settings.language, settings.mappings_overlay)
    };
    match dp_config::load(Path::new(&key.0)) {
        Ok((profile, _)) => {
            let content = MappingsContent {
                profile,
                layer,
                family,
                lang,
                opacity: s.opacity.clamp(20, 100),
                corner: s.corner,
            };
            present(app, MAPPINGS, s.corner, MAPPINGS_SIZE, &content);
        }
        Err(e) => tracing::warn!("cannot read {} for the mappings overlay: {e:#}", key.0),
    }
    *shown = Some(key);
}

pub(crate) fn watch(app: AppHandle, live: SharedLive) {
    let mut pad = Pad::default();
    let mut mappings: Option<(String, u8)> = None;
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let (pad_on, mappings_on, device, profile, layer) = {
            let l = live.get();
            let device = l
                .primary
                .as_ref()
                .and_then(|id| l.devices.iter().find(|d| &d.id == id))
                .cloned();
            (
                l.pad_overlay,
                l.mappings_overlay,
                device,
                l.profile_path.clone(),
                l.layer,
            )
        };
        let family = device.as_ref().map(|d| d.family);
        update_pad(&app, &mut pad, pad_on, device);
        update_mappings(&app, &mut mappings, mappings_on, profile, layer, family);
    }
}
