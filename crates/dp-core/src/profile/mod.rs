//! The Doppelpad profile model: what the user configures, independent of how it is stored.
//!
//! Profiles are written in TOML (see `profiles/` for examples). JSON profiles
//! (schema 3.0) are imported into this model by `dp-config`.

use serde::{Deserialize, Serialize};

/// `target`'s value if it differs from `base`'s.
fn changed<T: PartialEq + Clone>(base: &T, target: &T) -> Option<T> {
    (base != target).then(|| target.clone())
}

/// Declares a layer's patch of a settings struct: each field optional, left out meaning the
/// base layer's value, with `apply`, `is_empty` and `between`. The fields must be those of the
/// base struct (the struct literal in `apply` fails to compile otherwise).
macro_rules! layer_patch {
    (
        $(#[$meta:meta])*
        pub struct $name:ident for $base:ident {
            $( $field:ident: $ty:ty ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            $(
                #[serde(skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )+
        }

        impl $name {
            /// The base settings with this patch's fields put over them.
            pub fn apply(&self, base: &$base) -> $base {
                $base {
                    $( $field: self.$field.clone().unwrap_or_else(|| base.$field.clone()), )+
                }
            }

            pub fn is_empty(&self) -> bool {
                *self == Self::default()
            }

            /// The patch that turns `base` into `target` (only the fields that differ).
            pub fn between(base: &$base, target: &$base) -> Self {
                $name {
                    $( $field: $crate::profile::changed(&base.$field, &target.$field), )+
                }
            }
        }
    };
}

mod analog;
mod feedback;
mod layer;
mod mapping;
mod overlay;
mod sensors;
mod sticks;
mod virtual_devices;

pub use self::analog::*;
pub use self::feedback::*;
pub use self::layer::*;
pub use self::mapping::*;
pub use self::overlay::*;
pub use self::sensors::*;
pub use self::sticks::*;
pub use self::virtual_devices::*;

/// One complete remapping profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Executable names (e.g. `game.exe`) that activate this profile automatically.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<String>,
    /// Which keyboard and mouse input the mappings react to: all of it, only the physical
    /// devices, or only what scripts and macro tools inject.
    #[serde(skip_serializing_if = "is_default")]
    pub kbm_source: KbmSource,
    #[serde(skip_serializing_if = "is_default")]
    pub output: OutputSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub timing: Timing,
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: Sticks,
    #[serde(skip_serializing_if = "is_default")]
    pub triggers: Triggers,
    /// How the motion sensors become gyro controls (`[gyro]`).
    #[serde(skip_serializing_if = "is_default")]
    pub gyro: GyroSettings,
    /// How the touchpad becomes region and gesture controls (`[touchpad]`).
    #[serde(skip_serializing_if = "is_default")]
    pub touchpad: TouchpadSettings,
    /// The physical mouse as a source (`[mouse]`).
    #[serde(skip_serializing_if = "is_default")]
    pub mouse: MouseSettings,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// What held controls do when the layer changes (`[layer_change]`).
    #[serde(skip_serializing_if = "is_default")]
    pub layer_change: LayerChange,
    pub mappings: Vec<Mapping>,
    /// Continuous analog routes (stick → mouse, mouse → stick, gyro → mouse, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub analog: Vec<AnalogMapping>,
    /// Settings of the virtual devices Doppelpad drives (`[virtual.keyboard]`, ...).
    #[serde(rename = "virtual", skip_serializing_if = "is_default")]
    pub virtual_devices: VirtualDevices,
    /// Crosshairs drawn over the game (`[[crosshairs]]`, at most ten).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub crosshairs: Vec<Crosshair>,
    /// Radial menus (`[[radial_menus]]`), each shown while its layer is active.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub radial_menus: Vec<RadialMenu>,
    /// The controller's light bar (`[lightbar]`); without it the game sets the colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightbar: Option<Light>,
    /// How the game's rumble reaches the controller (`[vibration]`).
    #[serde(skip_serializing_if = "is_default")]
    pub vibration: Vibration,
}

impl Profile {
    /// The declared layer with this id.
    pub fn layer(&self, id: u8) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Whether layer `layer` takes over the base layer's mappings on `trigger`, unless it maps
    /// the trigger itself: it inherits, does not leave the trigger out (`not_inherited`) and
    /// gives none of its controls back (`unlock`).
    pub fn inherits(&self, layer: u8, trigger: &[crate::control::Control]) -> bool {
        if layer == 0 {
            return false;
        }
        let Some(decl) = self.layer(layer) else {
            return true; // an undeclared layer inherits everything
        };
        let key = Combo(trigger.to_vec()).normalized();
        decl.inherit
            && !decl.not_inherited.iter().any(|c| c.normalized() == key)
            && !decl
                .unlock
                .is_some_and(|u| trigger.iter().any(|&c| u.covers(c)))
    }

    /// Whether `trigger` is mapped on `layer` itself, counting the way back of a hold or
    /// toggle jump into the layer (the engine maps the jump's trigger there).
    pub fn maps_on_layer(&self, layer: u8, trigger: &Combo) -> bool {
        let key = trigger.normalized();
        self.mappings.iter().any(|m| {
            let back = m.jump.as_ref().is_some_and(|j| {
                j.layer == layer && j.layer != m.layer && j.mode != JumpMode::Switch
            });
            (m.layer == layer || back) && m.trigger.normalized() == key
        })
    }

    /// Whether mapping `m` works on layer `layer`: one of the layer's own, or a base-layer
    /// mapping the layer takes over.
    pub fn applies_on(&self, m: &Mapping, layer: u8) -> bool {
        m.layer == layer
            || (m.layer == 0
                && self.inherits(layer, &m.trigger.0)
                && !self.maps_on_layer(layer, &m.trigger))
    }
}

/// Which keyboard and mouse events a profile's mappings see.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KbmSource {
    /// Physical devices and injected input alike.
    #[default]
    All,
    /// Physical devices only; what scripts inject passes through untouched.
    Physical,
    /// Only input that other programs inject (AutoHotkey, macro tools); the physical
    /// keyboard and mouse keep working as usual.
    Scripts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputSettings {
    pub virtual_pad: VirtualPadType,
    /// Hide the physical controller from games while the profile is active.
    pub hide_physical: bool,
    /// Forward physical inputs that no mapping consumes to the virtual pad.
    pub passthrough: bool,
    /// Swallow keyboard keys and mouse buttons that trigger a mapping, so only the
    /// mapping's output reaches applications. Mappings without actions always mute.
    pub mute_mapped_inputs: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        OutputSettings {
            virtual_pad: VirtualPadType::Xbox360,
            hide_physical: true,
            passthrough: true,
            mute_mapped_inputs: true,
        }
    }
}

/// Profile-wide timing defaults; individual mappings may override them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Timing {
    /// Hold time after which a press counts as a long press.
    pub long_press_ms: u32,
    /// After releasing a first tap, a second press within this time is a double press.
    pub double_tap_ms: u32,
    /// After releasing a first tap, a third press within this time is a triple press.
    pub triple_tap_ms: u32,
    /// Delay between consecutive key events inside macros, and the length of a "tap".
    pub key_delay_ms: u32,
    /// Default turbo period (one press + one release).
    pub turbo_ms: u32,
    /// Window in which the buttons of a combo must all be pressed.
    pub combo_window_ms: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            long_press_ms: 300,
            double_tap_ms: 150,
            triple_tap_ms: 300,
            key_delay_ms: 10,
            turbo_ms: 200,
            combo_window_ms: 50,
        }
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Key, WheelDir};

    #[test]
    fn toml_roundtrip() {
        let src = r#"
name = "Test"
processes = ["game.exe"]

[output]
virtual_pad = "dual_shock4"

[[layers]]
id = 1
name = "Shift"
not_inherited = ["A", ["LB", "RB"]]

[[mappings]]
trigger = "A"
actions = [{ key = "Space" }]

[[mappings]]
trigger = ["LB", "RB"]
activator = "long"
mode = "once"
actions = [{ key = "Escape", action = "tap" }, { pause = 50 }, { pad = "Start" }]

[[mappings]]
trigger = "Back"
jump = { layer = 1 }

[[mappings]]
layer = 1
trigger = "RT"
mode = "turbo"
turbo_ms = 80
actions = [{ mouse = "left" }, { stick = "left", x = 0.0, y = 1.0 }, { wheel = "up" }]

[[analog]]
source = "mouse"
target = "right_stick"
sensitivity = 2.0
"#;
        let p: Profile = toml::from_str(src).unwrap();
        assert_eq!(p.output.virtual_pad, VirtualPadType::DualShock4);
        assert_eq!(p.mappings.len(), 4);
        assert_eq!(p.mappings[1].trigger.0.len(), 2);
        assert_eq!(
            p.mappings[1].actions[0],
            Action::Key {
                key: Key::Escape,
                action: Press::Tap
            }
        );
        assert_eq!(
            p.mappings[3].actions[2],
            Action::Wheel {
                wheel: WheelDir::Up,
                clicks: 1.0
            }
        );
        assert_eq!(p.layers[0].not_inherited.len(), 2);
        assert_eq!(p.analog[0].source, AnalogSource::Mouse);
        assert!(p.output.hide_physical, "defaults fill in");

        let again: Profile = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p);
    }

    #[test]
    fn ds4_is_not_a_virtual_pad_name() {
        let src = "[output]\nvirtual_pad = \"ds4\"\n";
        assert!(toml::from_str::<Profile>(src).is_err());
    }

    #[test]
    fn unknown_field_rejected() {
        let src = "[[mappings]]\ntrigger = \"A\"\nactoins = []\n";
        assert!(toml::from_str::<Profile>(src).is_err());
    }
}
