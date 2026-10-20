//! GUI preferences, stored in `config_dir/gui.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// On-screen notifications over games.
    pub osd: OsdSettings,
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
    /// The settings with every number in its range.
    pub fn clamped(mut self) -> OsdSettings {
        self.scale = self.scale.clamp(50, 200);
        self.opacity = self.opacity.clamp(20, 100);
        self.seconds = self.seconds.clamp(1, 30);
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
            osd: OsdSettings::default(),
        }
    }
}

fn path() -> PathBuf {
    dp_config::config_dir().join("gui.toml")
}

impl GuiSettings {
    pub fn load() -> GuiSettings {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(p, text)
    }
}
