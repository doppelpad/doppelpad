//! GUI preferences, stored in `config_dir/gui.toml`.

use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Size of a notification, in percent.
const OSD_SCALE: RangeInclusive<u16> = 50..=200;
/// How long a notification stays, in seconds.
const OSD_SECONDS: RangeInclusive<u8> = 1..=30;
/// Opacity of the overlays, in percent.
const OPACITY: RangeInclusive<u8> = 20..=100;
/// Size of the controller overlay, in percent.
const PAD_SIZE: RangeInclusive<u8> = 30..=150;

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
    /// Index into the monitors, the primary one first.
    pub monitor: usize,
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
            monitor: 0,
            corner: Corner::BottomRight,
            scale: 100,
            opacity: 80,
            seconds: 5,
        }
    }
}

impl OsdSettings {
    /// How long a message stays.
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::from(self.seconds))
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
        }
    }
}

fn path() -> PathBuf {
    dp_config::config_dir().join("gui.toml")
}

impl GuiSettings {
    pub fn load() -> GuiSettings {
        dp_config::load_toml_or_default::<GuiSettings>(&path()).clamped()
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
        self
    }

    /// Write atomically (temporary file, then rename).
    pub fn save(&self) -> std::io::Result<()> {
        dp_config::save_toml(&path(), self)
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
        let s = s.clamped();
        assert_eq!((s.osd.scale, s.osd.seconds, s.osd.opacity), (200, 1, 20));
        assert_eq!((s.pad_overlay.size, s.pad_overlay.opacity), (30, 100));
        assert_eq!(s.mappings_overlay.opacity, 20);
    }
}
