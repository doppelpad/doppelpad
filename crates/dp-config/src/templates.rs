//! Starting points for new configs: a virtual controller that passes the physical one
//! through, the desktop driven by a controller, and keyboard and mouse as a controller.

use dp_core::profile::{Profile, VirtualPadType};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    /// The controller as a virtual Xbox 360 controller, hidden from games itself.
    VirtualXbox,
    /// The same as a virtual DualShock 4 (touchpad and motion reach the game).
    VirtualDs4,
    /// Mouse, clicks, arrow keys and scrolling on the controller; no virtual controller.
    Desktop,
    /// No controller needed: keyboard and mouse drive a virtual Xbox 360 controller.
    KeyboardMouse,
}

impl Template {
    pub const ALL: [Template; 4] = [
        Template::VirtualXbox,
        Template::VirtualDs4,
        Template::Desktop,
        Template::KeyboardMouse,
    ];
}

const DESKTOP: &str = include_str!("../../../profiles/examples/controller-desktop.toml");
const KEYBOARD_MOUSE: &str = include_str!("../../../profiles/examples/keyboard-mouse-to-xbox.toml");

/// The template's profile, named `name`.
pub fn profile(t: Template, name: &str) -> Profile {
    let mut p = match t {
        Template::VirtualXbox | Template::VirtualDs4 => {
            let mut p = Profile::default();
            if t == Template::VirtualDs4 {
                p.output.virtual_pad = VirtualPadType::DualShock4;
            }
            p
        }
        // The examples are part of the tests; they always parse.
        Template::Desktop => crate::parse_toml(DESKTOP).unwrap_or_default(),
        Template::KeyboardMouse => crate::parse_toml(KEYBOARD_MOUSE).unwrap_or_default(),
    };
    p.name = name.to_owned();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_a_working_profile() {
        for t in Template::ALL {
            let p = profile(t, "Mine");
            assert_eq!(p.name, "Mine");
            let text = crate::to_toml(&p);
            assert_eq!(crate::parse_toml(&text).unwrap(), p, "{t:?}");
        }
        assert!(!profile(Template::Desktop, "x").mappings.is_empty());
        assert_eq!(
            profile(Template::Desktop, "x").output.virtual_pad,
            VirtualPadType::None
        );
        assert!(!profile(Template::KeyboardMouse, "x").mappings.is_empty());
        assert_eq!(
            profile(Template::VirtualDs4, "x").output.virtual_pad,
            VirtualPadType::DualShock4
        );
    }
}
