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
