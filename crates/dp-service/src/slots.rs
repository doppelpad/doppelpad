//! Four slots that each hold a config file, the active slot, and the global remap and
//! auto-switch switches; controllers can have four slots of their own. Persisted as TOML
//! so they survive restarts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;
use serde::{Deserialize, Serialize};

use crate::dsu::DsuSettings;
use crate::httpapi::HttpSettings;

/// Number of slots.
pub const SLOTS: usize = 4;

/// The slots of one controller that has its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceSlots {
    pub slots: [Option<PathBuf>; SLOTS],
    /// Index of the active slot, `0..SLOTS`.
    pub active: usize,
}

impl DeviceSlots {
    /// Config file of the active slot.
    pub fn active_profile(&self) -> Option<&PathBuf> {
        self.slots.get(self.active)?.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotState {
    /// The shared slots, used by the controller in use unless it has slots of its own.
    pub slots: [Option<PathBuf>; SLOTS],
    /// Index of the active shared slot, `0..SLOTS`.
    pub active: usize,
    pub remap_on: bool,
    pub auto_switch: bool,
    /// Controllers with slots of their own, by device id.
    pub devices: BTreeMap<String, DeviceSlots>,
    pub player_lights: PlayerLights,
    /// The DualSense microphone LED shows whether remapping is on.
    pub mic_led: bool,
    /// Devices Doppelpad ignores completely.
    pub blocked: Vec<String>,
    /// Controllers that act as one.
    pub groups: Vec<DeviceGroup>,
    /// Turn wireless controllers off after this many minutes without input; 0 never.
    pub power_off_after_min: u32,
    /// How generic pad models' inputs map to pad controls.
    pub layouts: Vec<GenericLayout>,
    /// The DSU motion server for emulators.
    pub dsu: DsuSettings,
    pub runtime: RuntimeOptions,
    /// The local HTTP API.
    pub http: HttpSettings,
}

/// How remapping fits in with the rest of the system (`[runtime]` in state.toml).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeOptions {
    /// Keep the computer awake while a mapping plays output (a macro, a toggle).
    pub prevent_sleep: bool,
    /// Pause remapping while the active config's game is not in front.
    pub pause_unfocused: bool,
    /// A controller with slots of its own keeps its virtual controller (idle) while it is
    /// disconnected.
    pub keep_offline_pads: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        RuntimeOptions {
            prevent_sleep: true,
            pause_unfocused: false,
            keep_offline_pads: false,
        }
    }
}

/// What the player lights of a controller show.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerLights {
    /// The active slot's number while remapping is on, dark while it is off.
    #[default]
    WhileRemapping,
    /// The active slot's number.
    Always,
    /// Left to the controller.
    Off,
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            slots: Default::default(),
            active: 0,
            remap_on: true,
            auto_switch: true,
            devices: BTreeMap::new(),
            player_lights: PlayerLights::default(),
            mic_led: false,
            blocked: Vec::new(),
            groups: Vec::new(),
            power_off_after_min: 0,
            layouts: Vec::new(),
            dsu: DsuSettings::default(),
            runtime: RuntimeOptions::default(),
            http: HttpSettings::default(),
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    devices: BTreeMap<String, DeviceFile>,
    player_lights: PlayerLights,
    mic_led: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocked: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    groups: Vec<DeviceGroup>,
    power_off_after_min: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    layouts: Vec<GenericLayout>,
    dsu: DsuSettings,
    runtime: RuntimeOptions,
    http: HttpSettings,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct DeviceFile {
    slots: Vec<String>,
    active: usize,
}

fn slots_from(file: Vec<String>) -> [Option<PathBuf>; SLOTS] {
    let mut slots: [Option<PathBuf>; SLOTS] = Default::default();
    for (slot, p) in slots.iter_mut().zip(file) {
        *slot = (!p.is_empty()).then(|| PathBuf::from(p));
    }
    slots
}

fn slots_to(slots: &[Option<PathBuf>; SLOTS]) -> Vec<String> {
    slots
        .iter()
        .map(|s| {
            s.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        })
        .collect()
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
        SlotState {
            slots: slots_from(file.slots),
            active: file.active.min(SLOTS - 1),
            remap_on: file.remap_on.unwrap_or(true),
            auto_switch: file.auto_switch.unwrap_or(true),
            devices: file
                .devices
                .into_iter()
                .map(|(id, d)| {
                    let slots = DeviceSlots {
                        slots: slots_from(d.slots),
                        active: d.active.min(SLOTS - 1),
                    };
                    (id, slots)
                })
                .collect(),
            player_lights: file.player_lights,
            mic_led: file.mic_led,
            blocked: file.blocked,
            groups: file.groups,
            power_off_after_min: file.power_off_after_min,
            layouts: file.layouts,
            dsu: file.dsu,
            runtime: file.runtime,
            http: file.http,
        }
    }

    /// Write to `path` atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = StateFile {
            slots: slots_to(&self.slots),
            active: self.active,
            remap_on: Some(self.remap_on),
            auto_switch: Some(self.auto_switch),
            devices: self
                .devices
                .iter()
                .map(|(id, d)| {
                    let file = DeviceFile {
                        slots: slots_to(&d.slots),
                        active: d.active,
                    };
                    (id.clone(), file)
                })
                .collect(),
            player_lights: self.player_lights,
            mic_led: self.mic_led,
            blocked: self.blocked.clone(),
            groups: self.groups.clone(),
            power_off_after_min: self.power_off_after_min,
            layouts: self.layouts.clone(),
            dsu: self.dsu,
            runtime: self.runtime,
            http: self.http.clone(),
        };
        let text = toml::to_string_pretty(&file).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Config file of the active shared slot.
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
        let mut own = DeviceSlots::default();
        own.slots[2] = Some(PathBuf::from("c.toml"));
        own.active = 2;
        s.devices
            .insert(r"hid:HID\VID_054C&PID_0CE6\7&1".into(), own);
        let raw = dp_platform::generic::RawInput {
            buttons: vec![false; 10],
            axes: vec![0.0; 4],
            hats: vec![None],
        };
        s.layouts.push(GenericLayout::guess("0079:0006", &raw));
        s.save(&path).unwrap();
        assert_eq!(SlotState::load(&path), s);
        assert_eq!(s.active_profile(), Some(&PathBuf::from(r"C:\games\a.toml")));
        assert_eq!(
            s.devices.values().next().unwrap().active_profile(),
            Some(&PathBuf::from("c.toml"))
        );

        std::fs::write(&path, "this is = = not toml").unwrap();
        assert_eq!(
            SlotState::load(&path),
            SlotState::default(),
            "corrupt file = defaults"
        );
    }

    /// The written form of `state.toml`, byte for byte, so that format changes are deliberate.
    #[test]
    fn file_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let mut s = SlotState::default();
        s.slots[1] = Some(PathBuf::from(r"C:\games\a.toml"));
        s.slots[3] = Some(PathBuf::from("b.toml"));
        s.active = 1;
        s.auto_switch = false;
        let mut own = DeviceSlots::default();
        own.slots[0] = Some(PathBuf::from("c.toml"));
        own.active = 2;
        s.devices.insert("hid:pad".into(), own);
        s.devices.insert("xinput:0".into(), DeviceSlots::default());
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let tail = r#"
[dsu]
enabled = false
lan = false
port = 26760

[runtime]
prevent_sleep = true
pause_unfocused = false
keep_offline_pads = false

[http]
enabled = false
lan = false
port = 26761
token = ""
"#;
        let head = r#"slots = [
    "",
    'C:\games\a.toml',
    "",
    "b.toml",
]
active = 1
remap_on = true
auto_switch = false
player_lights = "while_remapping"
mic_led = false
power_off_after_min = 0

[devices."hid:pad"]
slots = [
    "c.toml",
    "",
    "",
    "",
]
active = 2

[devices."xinput:0"]
slots = [
    "",
    "",
    "",
    "",
]
active = 0
"#;
        assert_eq!(text, format!("{head}{tail}"));
        assert_eq!(SlotState::load(&path), s);

        SlotState::default().save(&path).unwrap();
        let head = r#"slots = [
    "",
    "",
    "",
    "",
]
active = 0
remap_on = true
auto_switch = true
player_lights = "while_remapping"
mic_led = false
power_off_after_min = 0
"#;
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, format!("{head}{tail}"));

        // A file without the two switches turns both on.
        std::fs::write(&path, "slots = []\nactive = 0\n").unwrap();
        assert_eq!(SlotState::load(&path), SlotState::default());
        std::fs::write(&path, "remap_on = false\n").unwrap();
        assert!(!SlotState::load(&path).remap_on);
        assert!(SlotState::load(&path).auto_switch);
    }
}
