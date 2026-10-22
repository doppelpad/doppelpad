//! Layers and what held controls do when the layer changes.

use serde::{Deserialize, Serialize};

use super::{Combo, LayerSticks, LayerTriggers, LayerVirtual, Light, is_default};
use crate::control::{Control, InputDevice};

/// A layer ("shift"). Layer 0 is the implicit base layer and is not listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layer {
    pub id: u8,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Mappings of the base layer apply here unless this layer maps the same trigger.
    pub inherit: bool,
    /// Triggers whose base-layer mapping must *not* be inherited (they do nothing here
    /// unless mapped in this layer).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_inherited: Vec<Combo>,
    /// Stick settings of this layer; whatever they leave out is the base layer's (stick
    /// and trigger settings always build on the base layer, whatever `inherit` says).
    #[serde(skip_serializing_if = "is_default")]
    pub sticks: LayerSticks,
    #[serde(skip_serializing_if = "is_default")]
    pub triggers: LayerTriggers,
    /// Virtual stick settings of this layer, on top of the base layer's.
    #[serde(rename = "virtual", skip_serializing_if = "is_default")]
    pub virtual_devices: LayerVirtual,
    /// Give the physical keyboard and/or mouse back while this layer is active: their input
    /// is not swallowed and the base layer's keyboard/mouse mappings are not inherited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock: Option<Unlock>,
    /// The light bar while this layer is active (the base layer's otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightbar: Option<Light>,
}

/// Which physical inputs an unlock layer gives back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unlock {
    All,
    Keyboard,
    Mouse,
}

impl Unlock {
    /// True if `c` is a physical input this unlock gives back.
    pub fn covers(self, c: Control) -> bool {
        match c.device() {
            InputDevice::Keyboard => self != Unlock::Mouse,
            InputDevice::Mouse => self != Unlock::Keyboard,
            InputDevice::Pad => false,
        }
    }
}

/// What held controls do when the layer changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerChange {
    /// Release held controls and press them again under the new layer. Off, they keep
    /// what they do until released and use the new layer from their next press.
    pub repress: bool,
}

impl Default for LayerChange {
    fn default() -> Self {
        LayerChange { repress: true }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            id: 1,
            name: String::new(),
            inherit: true,
            not_inherited: Vec::new(),
            sticks: LayerSticks::default(),
            triggers: LayerTriggers::default(),
            virtual_devices: LayerVirtual::default(),
            unlock: None,
            lightbar: None,
        }
    }
}
