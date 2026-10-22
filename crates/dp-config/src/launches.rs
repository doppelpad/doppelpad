//! Launch commands in configs from elsewhere. A `launch` action starts any program with any
//! arguments when a button is pressed, so a shared config can work like malware: imports list
//! these commands and leave them out unless the user keeps them.

use std::fmt;

use dp_core::profile::{Action, Combo, Command, Profile};
use serde::Serialize;

/// Where a launch command sits in a profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaunchPlace {
    /// In a mapping's actions.
    Mapping {
        layer: u8,
        /// The layer's own name; empty when it has none.
        layer_name: String,
        trigger: Combo,
    },
    /// In a radial menu sector's actions; `menu` and `sector` count from 1.
    Radial {
        menu: usize,
        /// The menu's name and the sector's label; empty when they have none.
        name: String,
        sector: usize,
        label: String,
    },
}

/// One launch command: the program, its arguments and where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Launch {
    /// The config file it is in, for a bundle's configs.
    pub file: Option<String>,
    /// The name of the config it is in.
    pub config: String,
    pub place: LaunchPlace,
    pub path: String,
    pub args: String,
}

impl Launch {
    /// The command line `path args`, the path quoted when it has spaces (or is empty).
    pub fn command_line(&self) -> String {
        let program = if self.path.is_empty() || self.path.contains(char::is_whitespace) {
            format!("\"{}\"", self.path)
        } else {
            self.path.clone()
        };
        if self.args.is_empty() {
            program
        } else {
            format!("{program} {}", self.args)
        }
    }
}

/// `main layer, A`, `layer 1 "Aim", LB+B` or `radial menu 1 "Tools", sector 2 "Notes"`.
impl fmt::Display for LaunchPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let named = |f: &mut fmt::Formatter<'_>, name: &str| {
            if name.is_empty() {
                Ok(())
            } else {
                write!(f, " \"{name}\"")
            }
        };
        match self {
            LaunchPlace::Mapping {
                layer: 0, trigger, ..
            } => write!(f, "main layer, {trigger}"),
            LaunchPlace::Mapping {
                layer,
                layer_name,
                trigger,
            } => {
                write!(f, "layer {layer}")?;
                named(f, layer_name)?;
                write!(f, ", {trigger}")
            }
            LaunchPlace::Radial {
                menu,
                name,
                sector,
                label,
            } => {
                write!(f, "radial menu {menu}")?;
                named(f, name)?;
                write!(f, ", sector {sector}")?;
                named(f, label)
            }
        }
    }
}

fn launch_of(a: &Action) -> Option<(&str, &str)> {
    match a {
        Action::Command {
            command: Command::Launch { path, args },
        } => Some((path, args)),
        _ => None,
    }
}

fn is_launch(a: &Action) -> bool {
    launch_of(a).is_some()
}

/// Every launch command of `p`: the mappings' first, then the radial menus'.
pub fn find(p: &Profile) -> Vec<Launch> {
    let mut out = Vec::new();
    let mut add = |actions: &[Action], place: &dyn Fn() -> LaunchPlace| {
        for (path, args) in actions.iter().filter_map(launch_of) {
            out.push(Launch {
                file: None,
                config: p.name.clone(),
                place: place(),
                path: path.to_owned(),
                args: args.to_owned(),
            });
        }
    };
    for m in &p.mappings {
        add(&m.actions, &|| LaunchPlace::Mapping {
            layer: m.layer,
            layer_name: p.layer(m.layer).map(|l| l.name.clone()).unwrap_or_default(),
            trigger: m.trigger.clone(),
        });
    }
    for (i, menu) in p.radial_menus.iter().enumerate() {
        for (j, s) in menu.sectors.iter().enumerate() {
            add(&s.actions, &|| LaunchPlace::Radial {
                menu: i + 1,
                name: menu.name.clone(),
                sector: j + 1,
                label: s.label.clone(),
            });
        }
    }
    out
}

/// Take every launch command out of `p`; returns how many there were. A mapping that did
/// nothing else (and changes no layer) goes with it, so its trigger is free again; radial
/// sectors stay in place, only without the command.
pub fn strip(p: &mut Profile) -> usize {
    let mut n = 0;
    let mut strip = |actions: &mut Vec<Action>| {
        let before = actions.len();
        actions.retain(|a| !is_launch(a));
        n += before - actions.len();
        before > 0 && actions.is_empty()
    };
    p.mappings
        .retain_mut(|m| !(strip(&mut m.actions) && m.jump.is_none()));
    for s in p.radial_menus.iter_mut().flat_map(|m| &mut m.sectors) {
        strip(&mut s.actions);
    }
    n
}

#[cfg(test)]
mod tests {
    use dp_core::control::Control;
    use dp_core::keys::Key;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Jump, Layer, Mapping, Press, RadialMenu, RadialSector};

    use super::*;

    fn launch(path: &str, args: &str) -> Action {
        Action::Command {
            command: Command::Launch {
                path: path.into(),
                args: args.into(),
            },
        }
    }

    const KEY: Action = Action::Key {
        key: Key::W,
        action: Press::Auto,
    };

    fn profile() -> Profile {
        let mut jump = Mapping::new(Control::Button(PadButton::B), vec![launch("calc.exe", "")]);
        jump.layer = 1;
        jump.jump = Some(Jump {
            layer: 0,
            mode: Default::default(),
            delay_ms: None,
            postpone: false,
        });
        Profile {
            name: "Shared".into(),
            layers: vec![Layer {
                id: 1,
                name: "Aim".into(),
                ..Default::default()
            }],
            mappings: vec![
                Mapping::new(
                    Control::Button(PadButton::A),
                    vec![launch("powershell", "-enc ZQBjAGgAbwA=")],
                ),
                jump,
                Mapping::new(
                    Control::Button(PadButton::X),
                    vec![KEY, launch("cmd", "/c del x")],
                ),
                // Does nothing on purpose (an unmap); not ours to remove.
                Mapping::new(Control::Button(PadButton::Y), vec![]),
            ],
            radial_menus: vec![RadialMenu {
                name: "Tools".into(),
                sectors: vec![
                    RadialSector::default(),
                    RadialSector {
                        label: "Notes".into(),
                        actions: vec![launch("notepad.exe", "C:\\notes.txt")],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn finds_launches_in_mappings_and_radial_menus() {
        let found = find(&profile());
        let programs: Vec<(&str, &str)> = found
            .iter()
            .map(|l| (l.path.as_str(), l.args.as_str()))
            .collect();
        assert_eq!(
            programs,
            [
                ("powershell", "-enc ZQBjAGgAbwA="),
                ("calc.exe", ""),
                ("cmd", "/c del x"),
                ("notepad.exe", "C:\\notes.txt"),
            ]
        );
        assert!(
            found
                .iter()
                .all(|l| l.config == "Shared" && l.file.is_none())
        );
        assert_eq!(
            found[1].place,
            LaunchPlace::Mapping {
                layer: 1,
                layer_name: "Aim".into(),
                trigger: Control::Button(PadButton::B).into(),
            }
        );
        assert_eq!(
            found[3].place,
            LaunchPlace::Radial {
                menu: 1,
                name: "Tools".into(),
                sector: 2,
                label: "Notes".into(),
            }
        );
        assert!(find(&Profile::default()).is_empty());
    }

    #[test]
    fn launches_read_as_place_and_command_line() {
        let lines: Vec<String> = find(&profile())
            .iter()
            .map(|l| format!("{}: {}", l.place, l.command_line()))
            .collect();
        assert_eq!(
            lines,
            [
                "main layer, A: powershell -enc ZQBjAGgAbwA=",
                "layer 1 \"Aim\", B: calc.exe",
                "main layer, X: cmd /c del x",
                "radial menu 1 \"Tools\", sector 2 \"Notes\": notepad.exe C:\\notes.txt",
            ]
        );
        let spaced = Launch {
            path: r"C:\Program Files\x.exe".into(),
            args: String::new(),
            ..find(&profile()).remove(0)
        };
        assert_eq!(spaced.command_line(), r#""C:\Program Files\x.exe""#);
    }

    #[test]
    fn stripping_keeps_everything_else() {
        let mut p = profile();
        assert_eq!(strip(&mut p), 4);
        assert!(find(&p).is_empty());
        let triggers: Vec<String> = p.mappings.iter().map(|m| m.trigger.to_string()).collect();
        assert_eq!(triggers, ["B", "X", "Y"], "A only launched, so it goes");
        assert!(p.mappings[0].jump.is_some(), "the jump stays");
        assert_eq!(p.mappings[1].actions, [KEY]);
        assert_eq!(p.radial_menus[0].sectors.len(), 2);
        assert!(p.radial_menus[0].sectors[1].actions.is_empty());
        assert_eq!(p.radial_menus[0].sectors[1].label, "Notes");
        assert_eq!(strip(&mut p), 0);
    }
}
