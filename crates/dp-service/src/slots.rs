//! Four slots that each hold a config file, the active slot, and the global remap and
//! auto-switch switches; controllers can have four slots of their own. Persisted as TOML
//! so they survive restarts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dp_config::{FileFormat, Loaded};
use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;
use serde::{Deserialize, Serialize};

use crate::dsu::DsuSettings;
use crate::httpapi::HttpSettings;

/// Number of slots.
pub const SLOTS: usize = 4;

/// The format of `state.toml`.
const FORMAT: FileFormat = FileFormat::new(&[]);

/// Four slots that each may hold a config file, and the active one: the shared set, or a
/// controller's own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotSet {
    #[serde(with = "slot_paths")]
    pub slots: [Option<PathBuf>; SLOTS],
    /// Index of the active slot, `0..SLOTS`.
    pub active: usize,
}

impl SlotSet {
    /// Config file of the active slot.
    pub fn active_profile(&self) -> Option<&PathBuf> {
        self.slots.get(self.active)?.as_ref()
    }

    /// The slots' config files as the user sees them.
    pub fn display_names(&self) -> Vec<Option<String>> {
        self.slots
            .iter()
            .map(|s| s.as_ref().map(|p| p.display().to_string()))
            .collect()
    }

    /// Keep a hand-edited active slot in range.
    fn clamp(&mut self) {
        self.active = self.active.min(SLOTS - 1);
    }

    /// Point slots holding files of the library moved from `from` at their new place.
    fn library_moved(&mut self, from: &Path, to: &Path) {
        for slot in self.slots.iter_mut().flatten() {
            if let Some(p) = dp_config::relocate::moved_path(slot, from, to) {
                *slot = p;
            }
        }
    }
}

/// TOML has no null, so empty slots are empty strings on disk.
mod slot_paths {
    use std::path::PathBuf;

    use serde::{Deserialize, Deserializer, Serializer};

    use super::SLOTS;

    pub fn serialize<S: Serializer>(
        slots: &[Option<PathBuf>; SLOTS],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(slots.iter().map(|p| {
            p.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        }))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<[Option<PathBuf>; SLOTS], D::Error> {
        let mut slots: [Option<PathBuf>; SLOTS] = Default::default();
        for (slot, p) in slots.iter_mut().zip(Vec::<String>::deserialize(d)?) {
            *slot = (!p.is_empty()).then(|| PathBuf::from(p));
        }
        Ok(slots)
    }
}

/// Everything `state.toml` keeps. A missing field takes its default, so a file without the
/// two switches turns both on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotState {
    /// The shared slots, used by the controller in use unless it has slots of its own.
    #[serde(flatten)]
    pub shared: SlotSet,
    pub remap_on: bool,
    pub auto_switch: bool,
    /// Controllers with slots of their own, by device id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub devices: BTreeMap<String, SlotSet>,
    pub player_lights: PlayerLights,
    /// The DualSense microphone LED shows whether remapping is on.
    pub mic_led: bool,
    /// Devices Doppelpad ignores completely.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<String>,
    /// Controllers that act as one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<DeviceGroup>,
    /// Turn wireless controllers off after this many minutes without input; 0 never.
    pub power_off_after_min: u32,
    /// Look for Switch 2 controllers in pairing mode over Bluetooth.
    pub switch2_bluetooth: bool,
    /// How generic pad models' inputs map to pad controls.
    #[serde(skip_serializing_if = "Vec::is_empty")]
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
            shared: SlotSet::default(),
            remap_on: true,
            auto_switch: true,
            devices: BTreeMap::new(),
            player_lights: PlayerLights::default(),
            mic_led: false,
            blocked: Vec::new(),
            groups: Vec::new(),
            power_off_after_min: 0,
            switch2_bluetooth: true,
            layouts: Vec::new(),
            dsu: DsuSettings::default(),
            runtime: RuntimeOptions::default(),
            http: HttpSettings::default(),
        }
    }
}

impl SlotState {
    /// Load from `path`. A missing or unreadable file yields the defaults (with the problem).
    pub fn load(path: &Path) -> Loaded<SlotState> {
        let loaded: Loaded<SlotState> = dp_config::load_toml_or_default(path, &FORMAT);
        loaded.map(|mut state| {
            state.shared.clamp();
            state.devices.values_mut().for_each(SlotSet::clamp);
            state
        })
    }

    /// Write to `path` atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        dp_config::save_toml(path, self, &FORMAT)
    }

    /// The library moved from `from` to `to`: every slot holding one of its files, shared or
    /// a controller's own, now holds the same file in its new place.
    pub fn library_moved(&mut self, from: &Path, to: &Path) {
        self.shared.library_moved(from, to);
        for own in self.devices.values_mut() {
            own.library_moved(from, to);
        }
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
            SlotState::load(&path).value,
            SlotState::default(),
            "missing file = defaults"
        );

        let mut s = SlotState::default();
        s.shared.slots[1] = Some(PathBuf::from(r"C:\games\a.toml"));
        s.shared.slots[3] = Some(PathBuf::from("b.toml"));
        s.shared.active = 1;
        s.remap_on = false;
        let mut own = SlotSet::default();
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
        assert_eq!(SlotState::load(&path).value, s);
        assert_eq!(
            s.shared.active_profile(),
            Some(&PathBuf::from(r"C:\games\a.toml"))
        );
        assert_eq!(
            s.devices.values().next().unwrap().active_profile(),
            Some(&PathBuf::from("c.toml"))
        );

        std::fs::write(&path, "this is = = not toml").unwrap();
        let corrupt = SlotState::load(&path);
        assert_eq!(
            corrupt.value,
            SlotState::default(),
            "corrupt file = defaults"
        );
        assert!(corrupt.problem.is_some(), "with the problem");
    }

    #[test]
    fn slots_follow_the_moved_library() {
        let (from, to) = (Path::new("/old/profiles"), Path::new("/sync/pads"));
        let mut s = SlotState::default();
        s.shared.slots[0] = Some(PathBuf::from("/old/profiles/game/a.toml"));
        s.shared.slots[2] = Some(PathBuf::from("/elsewhere/b.toml"));
        let mut own = SlotSet::default();
        own.slots[3] = Some(PathBuf::from("/old/profiles/c.toml"));
        s.devices.insert("hid:pad".into(), own);
        s.library_moved(from, to);
        assert_eq!(
            s.shared.slots[0],
            Some(PathBuf::from("/sync/pads/game/a.toml"))
        );
        assert_eq!(
            s.shared.slots[2],
            Some(PathBuf::from("/elsewhere/b.toml")),
            "a file outside the library stays"
        );
        assert_eq!(
            s.devices["hid:pad"].slots[3],
            Some(PathBuf::from("/sync/pads/c.toml"))
        );
    }

    /// The written form of `state.toml`, byte for byte, so that format changes are deliberate.
    #[test]
    fn file_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let mut s = SlotState::default();
        s.shared.slots[1] = Some(PathBuf::from(r"C:\games\a.toml"));
        s.shared.slots[3] = Some(PathBuf::from("b.toml"));
        s.shared.active = 1;
        s.auto_switch = false;
        let mut own = SlotSet::default();
        own.slots[0] = Some(PathBuf::from("c.toml"));
        own.active = 2;
        s.devices.insert("hid:pad".into(), own);
        s.devices.insert("xinput:0".into(), SlotSet::default());
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
        let head = r#"format = 1
slots = [
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
switch2_bluetooth = true

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
        assert_eq!(SlotState::load(&path).value, s);

        SlotState::default().save(&path).unwrap();
        let head = r#"format = 1
slots = [
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
switch2_bluetooth = true
"#;
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, format!("{head}{tail}"));

        // A hand-edited active slot stays in range.
        std::fs::write(&path, "active = 9\n").unwrap();
        assert_eq!(SlotState::load(&path).value.shared.active, SLOTS - 1);

        // A file without the two switches turns both on.
        std::fs::write(&path, "slots = []\nactive = 0\n").unwrap();
        assert_eq!(SlotState::load(&path).value, SlotState::default());
        std::fs::write(&path, "remap_on = false\n").unwrap();
        assert!(!SlotState::load(&path).value.remap_on);
        assert!(SlotState::load(&path).value.auto_switch);

        // A file from before the Bluetooth search could be turned off keeps it on.
        assert!(SlotState::load(&path).value.switch2_bluetooth);
        std::fs::write(&path, "switch2_bluetooth = false\n").unwrap();
        assert!(!SlotState::load(&path).value.switch2_bluetooth);
    }
}
