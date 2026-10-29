//! GUI preferences, stored in `config_dir/gui.toml`.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::Duration;

use dp_config::FileFormat;
use serde::{Deserialize, Serialize};

/// Size of a notification, in percent.
const OSD_SCALE: RangeInclusive<u16> = 50..=200;
/// How long a notification stays, in seconds.
const OSD_SECONDS: RangeInclusive<u8> = 1..=30;
/// Opacity of the overlays, in percent.
const OPACITY: RangeInclusive<u8> = 20..=100;
/// Size of the controller overlay, in percent.
const PAD_SIZE: RangeInclusive<u8> = 30..=150;
/// Size of the window's contents, in percent.
pub const ZOOM: RangeInclusive<u16> = 80..=150;

fn clamp<T: Ord + Copy>(value: T, range: &RangeInclusive<T>) -> T {
    value.clamp(*range.start(), *range.end())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    /// UI language: `en` or `de`.
    pub language: String,
    /// Keep remapping active while the window has focus (normally it pauses so mapped
    /// keys do not reach the editor).
    pub stay_live: bool,
    /// Press a controller button to select it in the editor.
    pub hook_buttons: bool,
    /// What to do when reWASD runs as the app starts (it keeps the controllers to itself).
    pub rewasd_at_start: RewasdChoice,
    /// Ask GitHub for a newer version at start-up.
    pub check_updates: bool,
    /// On-screen notifications over games.
    pub osd: OsdSettings,
    /// The live controller overlay.
    pub pad_overlay: PadOverlaySettings,
    /// The overlay listing the current layer's mappings.
    pub mappings_overlay: MappingsOverlaySettings,
    /// The welcome guide was finished or skipped.
    pub welcome_done: bool,
    /// The editor shows the mappings' notes instead of their actions.
    pub show_notes: bool,
    /// Questions the user asked not to be asked again (their ids); they count as answered OK.
    pub silenced: Vec<String>,
    /// Size of the window's contents, in percent (80–150).
    pub zoom: u16,
    /// The controller drives the app (moves the focus, activates, goes back) instead of
    /// selecting the pressed control.
    pub pad_navigation: bool,
    /// One more tray icon for each connected controller, besides the main one.
    pub tray_per_controller: bool,
    /// The colour of each controller's tray icon (`#rrggbb`) by device id; a controller without
    /// one gets a colour from a palette.
    pub controller_colors: BTreeMap<String, String>,
}

/// `#rrggbb` as red, green and blue.
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6)?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// On-screen notifications: which events show one, and where and how.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OsdSettings {
    pub enabled: bool,
    /// Remapping turned on or off.
    pub remap: bool,
    /// Another slot became active.
    pub slot: bool,
    /// The layer changed.
    pub layer: bool,
    /// Keep the layer message up while a layer other than Main is active.
    pub keep_layer: bool,
    /// A controller connected or disconnected.
    pub controller: bool,
    /// A controller's battery runs low.
    pub battery: bool,
    /// The screenshot command saved a screenshot, or could not.
    pub screenshot: bool,
    /// Index into the monitors, the primary one first, as the window lists them; see
    /// [`OsdSettings::monitor_index`].
    pub monitor: usize,
    /// The monitor chosen, by name and place: an index points at another monitor once
    /// monitors come and go. Missing in files from before it was kept (the index then counts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenRef>,
    pub corner: Corner,
    /// Size in percent, 50–200.
    pub scale: u16,
    /// Opacity in percent, 20–100.
    pub opacity: u8,
    /// How long a message stays, in seconds (1–30).
    pub seconds: u8,
}

impl Default for OsdSettings {
    fn default() -> Self {
        OsdSettings {
            enabled: true,
            remap: true,
            slot: true,
            layer: true,
            keep_layer: false,
            controller: true,
            battery: true,
            screenshot: true,
            monitor: 0,
            screen: None,
            corner: Corner::BottomRight,
            scale: 100,
            opacity: 80,
            seconds: 5,
        }
    }
}

/// A monitor by its system name and its position (physical pixels).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenRef {
    pub name: String,
    pub x: i32,
    pub y: i32,
}

impl OsdSettings {
    /// How long a message stays.
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::from(self.seconds))
    }

    /// Where the chosen monitor is in `screens` (the monitors, the primary one first): found
    /// by name and place, else at the index it was chosen with.
    pub fn monitor_index(&self, screens: &[ScreenRef]) -> usize {
        self.screen
            .as_ref()
            .and_then(|chosen| screens.iter().position(|s| s == chosen))
            .unwrap_or(self.monitor)
    }

    /// These settings as the window sent them (it knows monitors by their index in
    /// `screens`), with the monitor kept by name and place: the one chosen before (`old`)
    /// while the index still points at it, else the one the index points at now.
    pub fn with_screen(mut self, old: &OsdSettings, screens: &[ScreenRef]) -> OsdSettings {
        let unchanged = self.monitor == old.monitor_index(screens);
        self.screen = match &old.screen {
            Some(chosen) if unchanged => Some(chosen.clone()),
            _ => screens.get(self.monitor).cloned().or(self.screen),
        };
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// The live controller overlay: where, how big, and when it hides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadOverlaySettings {
    pub corner: Corner,
    /// Size in percent of 600 logical pixels across (the window offers 50, 70 and 90).
    pub size: u8,
    /// Opacity in percent, 20–100.
    pub opacity: u8,
    /// Hide after this many seconds without input (0: stay).
    pub hide_after: u8,
}

impl Default for PadOverlaySettings {
    fn default() -> Self {
        PadOverlaySettings {
            corner: Corner::TopLeft,
            size: 70,
            opacity: 90,
            hide_after: 5,
        }
    }
}

/// The overlay listing the current layer's mappings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MappingsOverlaySettings {
    pub corner: Corner,
    /// Opacity in percent, 20–100.
    pub opacity: u8,
}

impl Default for MappingsOverlaySettings {
    fn default() -> Self {
        MappingsOverlaySettings {
            corner: Corner::TopRight,
            opacity: 90,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewasdChoice {
    /// Offer to quit it.
    #[default]
    Ask,
    /// Quit it without asking.
    Quit,
    /// Leave it running.
    Ignore,
}

impl Default for GuiSettings {
    fn default() -> Self {
        GuiSettings {
            language: "en".into(),
            stay_live: false,
            hook_buttons: true,
            rewasd_at_start: RewasdChoice::Ask,
            check_updates: true,
            osd: OsdSettings::default(),
            pad_overlay: PadOverlaySettings::default(),
            mappings_overlay: MappingsOverlaySettings::default(),
            welcome_done: false,
            show_notes: false,
            silenced: Vec::new(),
            zoom: 100,
            pad_navigation: false,
            tray_per_controller: false,
            controller_colors: BTreeMap::new(),
        }
    }
}

fn path() -> PathBuf {
    dp_config::config_dir().join("gui.toml")
}

/// The format of `gui.toml`.
const FORMAT: FileFormat = FileFormat::new(&[]);

impl GuiSettings {
    /// The stored preferences; the defaults if `gui.toml` is missing or cannot be used (which
    /// is logged, and the file kept aside).
    pub fn load() -> GuiSettings {
        dp_config::load_toml_or_default::<GuiSettings>(&path(), &FORMAT)
            .value
            .clamped()
    }

    /// The settings with every number in its range: the window keeps to the ranges, a file
    /// edited by hand may not.
    pub fn clamped(mut self) -> GuiSettings {
        let osd = &mut self.osd;
        osd.scale = clamp(osd.scale, &OSD_SCALE);
        osd.opacity = clamp(osd.opacity, &OPACITY);
        osd.seconds = clamp(osd.seconds, &OSD_SECONDS);
        let pad = &mut self.pad_overlay;
        pad.size = clamp(pad.size, &PAD_SIZE);
        pad.opacity = clamp(pad.opacity, &OPACITY);
        let mappings = &mut self.mappings_overlay;
        mappings.opacity = clamp(mappings.opacity, &OPACITY);
        self.zoom = clamp(self.zoom, &ZOOM);
        self.silenced.sort();
        self.silenced.dedup();
        self.controller_colors
            .retain(|_, c| parse_color(c).is_some());
        self
    }

    /// Write atomically (temporary file, then rename).
    pub fn save(&self) -> std::io::Result<()> {
        dp_config::save_toml(&path(), self, &FORMAT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamped_keeps_every_number_in_its_range() {
        assert_eq!(GuiSettings::default().clamped(), GuiSettings::default());
        let mut s = GuiSettings::default();
        s.osd.scale = 500;
        s.osd.seconds = 0;
        s.osd.opacity = 5;
        s.pad_overlay.size = 10;
        s.pad_overlay.opacity = 200;
        s.mappings_overlay.opacity = 0;
        s.zoom = 300;
        let s = s.clamped();
        assert_eq!((s.osd.scale, s.osd.seconds, s.osd.opacity), (200, 1, 20));
        assert_eq!((s.pad_overlay.size, s.pad_overlay.opacity), (30, 100));
        assert_eq!(s.mappings_overlay.opacity, 20);
        assert_eq!(s.zoom, 150);
    }

    #[test]
    fn clamped_keeps_each_silenced_question_once() {
        let s = GuiSettings {
            silenced: vec!["b".into(), "a".into(), "b".into()],
            ..GuiSettings::default()
        };
        assert_eq!(s.clamped().silenced, ["a", "b"]);
    }

    #[test]
    fn notifications_stay_on_their_monitor_when_others_come_and_go() {
        let screen = |name: &str, x| ScreenRef {
            name: name.into(),
            x,
            y: 0,
        };
        let (a, b, c) = (screen("A", 0), screen("B", 1920), screen("C", -1280));
        // Chosen in the window: the second monitor.
        let window = OsdSettings {
            monitor: 1,
            ..OsdSettings::default()
        };
        let chosen = window.with_screen(&OsdSettings::default(), &[a.clone(), b.clone()]);
        assert_eq!(chosen.screen, Some(b.clone()));
        // Another monitor joins before it: the choice follows B.
        let now = [a.clone(), c.clone(), b.clone()];
        assert_eq!(chosen.monitor_index(&now), 2);
        // The window sends the settings back with the index it was shown: B stays chosen.
        let again = OsdSettings {
            monitor: 2,
            ..chosen.clone()
        };
        assert_eq!(again.with_screen(&chosen, &now).screen, Some(b.clone()));
        // B is gone: the index it was chosen with counts until it is back.
        assert_eq!(chosen.monitor_index(&[a, c.clone()]), 1);
        // A file from before monitors were kept by name: the index, until a choice is saved.
        let old = OsdSettings {
            monitor: 1,
            ..OsdSettings::default()
        };
        assert_eq!(old.monitor_index(&now), 1);
        let saved = old.clone().with_screen(&old, &now);
        assert_eq!(saved.screen, Some(c));
        // Settings without a monitor list (none reported) keep what they had.
        assert_eq!(chosen.clone().with_screen(&chosen, &[]).screen, Some(b));
    }

    #[test]
    fn controller_colours_must_be_hex() {
        assert_eq!(parse_color("#3b82f6"), Some([0x3b, 0x82, 0xf6]));
        assert_eq!(parse_color("3b82f6"), None);
        assert_eq!(parse_color("#3b82f"), None);
        assert_eq!(parse_color("#gg0000"), None);
        let s = GuiSettings {
            controller_colors: [("a", "#ff0000"), ("b", "red")]
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .into(),
            ..GuiSettings::default()
        };
        assert_eq!(
            s.clamped().controller_colors.len(),
            1,
            "a broken colour goes"
        );
    }
}
