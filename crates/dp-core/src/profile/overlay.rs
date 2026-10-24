//! What is drawn over the game: radial menus, crosshairs and the magnifier.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::Action;
use crate::pad::Stick;

/// A ring of sectors over the game. It opens when its layer becomes active; a stick, the
/// mouse or commands point at a sector, and leaving the layer (or the apply command) runs it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadialMenu {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The menu is open while this layer is active (0: only as a sub-menu).
    pub layer: u8,
    pub select: RadialSelect,
    pub apply: RadialApply,
    /// Show only after the layer has been active this long, so quick presses don't flash it.
    pub delay_ms: u32,
    /// Darken the screen behind the menu this much, 0–1.
    pub tint: f32,
    /// Size, 0.5–2 (1: 400 logical pixels across).
    pub scale: f32,
    /// Where on the primary monitor the ring sits.
    pub position: RadialPosition,
    /// Pixels from that place, right and down.
    pub offset: [i16; 2],
    pub sectors: Vec<RadialSector>,
}

impl RadialMenu {
    pub const SCALE: RangeInclusive<f32> = 0.5..=2.0;
}

impl Default for RadialMenu {
    fn default() -> Self {
        RadialMenu {
            name: String::new(),
            layer: 1,
            select: RadialSelect::RightStick,
            apply: RadialApply::Release,
            delay_ms: 0,
            tint: 0.0,
            scale: 1.0,
            position: RadialPosition::Center,
            offset: [0, 0],
            sectors: Vec::new(),
        }
    }
}

/// Where a radial menu sits on the screen: in the middle, at an edge or in a corner (the ring
/// touching it, a small margin away), or around the mouse pointer as the menu opens.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialPosition {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    /// Where the mouse pointer is when the menu opens (kept inside the screen).
    Cursor,
}

/// What points at a sector of an open radial menu.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialSelect {
    LeftStick,
    #[default]
    RightStick,
    Mouse,
    /// Only the radial commands (next, previous, up, down, ...).
    Commands,
}

impl RadialSelect {
    /// The stick that points, if a stick does.
    pub fn stick(self) -> Option<Stick> {
        match self {
            RadialSelect::LeftStick => Some(Stick::Left),
            RadialSelect::RightStick => Some(Stick::Right),
            RadialSelect::Mouse | RadialSelect::Commands => None,
        }
    }
}

/// When the pointed sector runs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialApply {
    /// When the menu's layer ends (the opening button is let go).
    #[default]
    Release,
    /// Only with the `radial_apply` command.
    Command,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadialSector {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// An icon name of the app's set, or the path of an image file.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// `#rrggbb`; empty for the accent colour.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// What the sector does, played once.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    /// Open this menu (1-based) instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submenu: Option<u8>,
    /// Close the menu after running (with the apply command).
    pub close: bool,
}

impl Default for RadialSector {
    fn default() -> Self {
        RadialSector {
            label: String::new(),
            icon: String::new(),
            color: String::new(),
            actions: Vec::new(),
            submenu: None,
            close: true,
        }
    }
}

/// A crosshair drawn over the game, shown by a command or while one of its layers is active.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Crosshair {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub shape: CrosshairShape,
    /// `#rrggbb`.
    pub color: String,
    /// 0–1.
    pub opacity: f32,
    /// Width and height in logical pixels, 10–200.
    pub size: u16,
    /// Line width in percent of the widest line (a fifth of the size), 10–100.
    pub thickness: u8,
    /// Outline width in pixels, 0 (none) to 10.
    pub outline: u8,
    pub outline_color: String,
    /// Pixels from the centre of the screen, right and down.
    pub offset: [i16; 2],
    /// Draw the crosshair; off, the entry only magnifies.
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnifier: Option<Magnifier>,
    /// Shown while one of these layers is active.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<u8>,
}

impl Crosshair {
    /// Logical pixels.
    pub const SIZE: RangeInclusive<u16> = 10..=200;
    /// Percent of the widest line.
    pub const THICKNESS: RangeInclusive<u8> = 10..=100;
    /// Pixels.
    pub const MAX_OUTLINE: u8 = 10;
}

impl Default for Crosshair {
    fn default() -> Self {
        Crosshair {
            name: String::new(),
            shape: CrosshairShape::Cross,
            color: "#00ff66".into(),
            opacity: 1.0,
            size: 30,
            thickness: 50,
            outline: 0,
            outline_color: "#000000".into(),
            offset: [0, 0],
            visible: true,
            magnifier: None,
            layers: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrosshairShape {
    #[default]
    Cross,
    /// A cross with an open centre.
    CrossGap,
    Dot,
    Circle,
    CircleDot,
    CircleCross,
    X,
    T,
    Chevron,
    Square,
    Brackets,
}

/// The screen around the crosshair, enlarged.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Magnifier {
    /// 1–16.
    pub zoom: f32,
    /// Diameter as a fraction of the screen height, 0.1–1.
    pub size: f32,
}

impl Magnifier {
    pub const ZOOM: RangeInclusive<f32> = 1.0..=16.0;
    pub const SIZE: RangeInclusive<f32> = 0.1..=1.0;
}

impl Default for Magnifier {
    fn default() -> Self {
        Magnifier {
            zoom: 2.0,
            size: 0.2,
        }
    }
}
