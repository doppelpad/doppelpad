//! Four slots that each hold a config file, the active slot, and the global remap and
//! auto-switch switches. Persisted as TOML so they survive restarts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Number of slots.
pub const SLOTS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotState {
    pub slots: [Option<PathBuf>; SLOTS],
    /// Index of the active slot, `0..SLOTS`.
    pub active: usize,
    pub remap_on: bool,
    pub auto_switch: bool,
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            slots: Default::default(),
            active: 0,
            remap_on: true,
            auto_switch: true,
        }
    }
}

/// On-disk form: TOML has no null, so empty slots are empty strings.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct StateFile {
    slots: Vec<String>,
    active: usize,
    remap_on: Option<bool>,
    auto_switch: Option<bool>,
}

impl SlotState {
    /// Load from `path`. A missing or unreadable file yields the defaults.
    pub fn load(path: &Path) -> SlotState {
        let Ok(src) = std::fs::read_to_string(path) else {
            return SlotState::default();
        };
        let file: StateFile = match toml::from_str(&src) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("ignoring {}: {e}", path.display());
                return SlotState::default();
            }
        };
        let mut s = SlotState::default();
        for (slot, p) in s.slots.iter_mut().zip(file.slots) {
            *slot = (!p.is_empty()).then(|| PathBuf::from(p));
        }
        s.active = file.active.min(SLOTS - 1);
        s.remap_on = file.remap_on.unwrap_or(true);
        s.auto_switch = file.auto_switch.unwrap_or(true);
        s
    }

    /// Write to `path` atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = StateFile {
            slots: self
                .slots
                .iter()
                .map(|s| {
                    s.as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                })
                .collect(),
            active: self.active,
            remap_on: Some(self.remap_on),
            auto_switch: Some(self.auto_switch),
        };
        let text = toml::to_string_pretty(&file).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Config file of the active slot.
    pub fn active_profile(&self) -> Option<&PathBuf> {
        self.slots.get(self.active)?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        assert_eq!(
            SlotState::load(&path),
            SlotState::default(),
            "missing file = defaults"
        );

        let mut s = SlotState::default();
        s.slots[1] = Some(PathBuf::from(r"C:\games\a.toml"));
        s.slots[3] = Some(PathBuf::from("b.toml"));
        s.active = 1;
        s.remap_on = false;
        s.save(&path).unwrap();
        assert_eq!(SlotState::load(&path), s);
        assert_eq!(s.active_profile(), Some(&PathBuf::from(r"C:\games\a.toml")));

        std::fs::write(&path, "this is = = not toml").unwrap();
        assert_eq!(
            SlotState::load(&path),
            SlotState::default(),
            "corrupt file = defaults"
        );
    }
}
