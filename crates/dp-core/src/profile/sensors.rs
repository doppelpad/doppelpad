//! The physical mouse, touchpad and gyro as sources of controls.

use serde::{Deserialize, Serialize};

/// The physical mouse as a source of controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseSettings {
    /// Speeds (counts per second) where the low, medium and high `MouseMove:*` zones start;
    /// the low one is also where `MouseMove:Up` and the other directions start.
    pub zones: [f32; 3],
}

impl Default for MouseSettings {
    fn default() -> Self {
        MouseSettings {
            zones: [200.0, 1000.0, 3000.0],
        }
    }
}

/// How the touchpad becomes region controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TouchpadSettings {
    /// The left and right halves are two trackpads (`Trackpad*` controls) instead of one.
    pub split: bool,
    pub zones: TouchZones,
    /// Radial zones: radius of the centre region, as a fraction of half the pad.
    pub centre: f32,
    /// Region controls only while the pad is clicked.
    pub click_required: bool,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        TouchpadSettings {
            split: false,
            zones: TouchZones::Radial,
            centre: 0.35,
            click_required: false,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchZones {
    /// Sectors around a centre region.
    #[default]
    Radial,
    /// A 3 × 3 grid of cells.
    Grid,
}

/// How the motion sensors become gyro controls and feed gyro routes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GyroSettings {
    pub mode: GyroMode,
    /// What `GyroLeft`/`GyroRight` follow.
    pub steering: SteeringAxis,
    /// Gyro controls and routes are on when the profile starts (commands switch them).
    pub initially_on: bool,
    /// How gyro routes read turning.
    pub space: GyroSpace,
    /// Where the low, medium and high zones start in motion mode, in degrees per second.
    pub motion_zones: [f32; 3],
    /// Where the low, medium and high zones start in tilt mode, in degrees from the centre.
    pub tilt_zones: [f32; 3],
}

impl Default for GyroSettings {
    fn default() -> Self {
        GyroSettings {
            mode: GyroMode::Motion,
            steering: SteeringAxis::Roll,
            initially_on: true,
            space: GyroSpace::Local,
            motion_zones: [60.0, 250.0, 550.0],
            tilt_zones: [10.0, 22.0, 34.0],
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroMode {
    /// Gyro controls fire while the controller turns (rotation rate).
    #[default]
    Motion,
    /// The controller is a stick: controls follow how far it is tilted from its centre pose.
    Tilt,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringAxis {
    /// Rolling the controller (like a steering wheel).
    #[default]
    Roll,
    /// Turning it left or right.
    Yaw,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GyroSpace {
    /// The controller's own axes.
    #[default]
    Local,
    /// Turning around gravity, however the controller is held.
    Player,
}
