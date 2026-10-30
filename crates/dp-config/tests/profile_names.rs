//! The "All names" section of docs/PROFILES.md, written from the name tables of the code so
//! that it lists every name a profile accepts. When a name changes, rewrite the section with
//! `DOPPELPAD_BLESS=1 cargo test -p dp-config --test profile_names`.

use std::fmt::Write;
use std::str::FromStr;

use dp_core::control::{
    Control, Dir, GyroDir, Half, MOUSE_BUTTON_ALIASES, PAD_BUTTON_ALIASES, TRIGGER_NAMES,
    TouchGesture, TouchRegion, Zone,
};
use dp_core::keys::{KEY_ALIASES, Key, MouseButton, WheelDir};
use dp_core::pad::{PadButton, Stick, Trigger};
use dp_core::profile::ShakeAxis;
use serde::Serialize;

const DOC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/PROFILES.md");
const START: &str = "<!-- names:start -->";
const END: &str = "<!-- names:end -->";

/// `names` as inline code, separated by spaces.
fn code<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> String {
    let names: Vec<String> = names
        .into_iter()
        .map(|n| format!("`{}`", n.as_ref()))
        .collect();
    names.join(" ")
}

/// How a value is written in an action (`up_left`, `x1`, `+x`).
fn written(v: impl Serialize) -> String {
    toml::Value::try_from(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("a unit variant")
}

/// A table row for each of the `names` that has other names, all written after `prefix`.
fn alias_rows<T: Copy + PartialEq>(
    out: &mut String,
    prefix: &str,
    names: impl IntoIterator<Item = (&'static str, T)>,
    aliases: &[(&str, T)],
) {
    for (name, v) in names {
        let of: Vec<String> = aliases
            .iter()
            .filter(|&&(alias, a)| a == v && alias != name)
            .map(|&(alias, _)| format!("{prefix}{alias}"))
            .collect();
        if !of.is_empty() {
            writeln!(out, "| `{prefix}{name}` | {} |", code(of)).unwrap();
        }
    }
}

fn names_section() -> String {
    let dirs = code(Dir::ALL.map(Dir::name));
    let zones = code(Zone::ALL.map(Zone::name));
    let mut s = String::new();
    s += "## All names\n\n";
    s += "These lists are written from the code, so they are complete. Control names and key\n";
    s += "names may be written in any case.\n\n";

    s += "### Controls\n\n";
    s += "| Controls | Names |\n|---|---|\n";
    writeln!(
        s,
        "| Gamepad buttons | {} |",
        code(PadButton::ALL.map(PadButton::name))
    )
    .unwrap();
    for stick in Stick::ALL {
        let p = format!("{}Stick", stick.name());
        writeln!(
            s,
            "| {} stick | `{p}` + a direction (`{p}Up`), a zone (`{p}High`), or both (`{p}UpLeftMed`) |",
            stick.name()
        )
        .unwrap();
    }
    s += "| Triggers | `LT` `RT`, `LTFull` `RTFull`, or a trigger + a zone (`RTLow`) |\n";
    writeln!(
        s,
        "| Touchpad regions | {} |",
        code(TouchRegion::ALL.map(|r| format!("Touch{}", r.name())))
    )
    .unwrap();
    writeln!(
        s,
        "| Touchpad gestures | {} |",
        code(TouchGesture::ALL.map(|g| format!("Touch{}", g.name())))
    )
    .unwrap();
    writeln!(
        s,
        "| Trackpads (`[touchpad] split`) | `Trackpad` + {} + a touchpad region, `Tap` or `Click` (`TrackpadLeftUp`, `TrackpadRightClick`) |",
        code(Half::ALL.map(Half::name))
    )
    .unwrap();
    s += "| Gyro | `Gyro` + a gyro direction (`GyroUp`), a zone (`GyroLow`), or both (`GyroLeanLeftHigh`) |\n";
    s += "| Keyboard | `Key:` + a key name (`Key:Space`) |\n";
    writeln!(
        s,
        "| Mouse buttons | {} |",
        code(MouseButton::ALL.map(|b| format!("Mouse:{}", b.name())))
    )
    .unwrap();
    writeln!(
        s,
        "| Mouse wheel | {} |",
        code(WheelDir::ALL.map(|w| format!("Wheel:{}", w.name())))
    )
    .unwrap();
    s += "| Mouse movement | `MouseMove:` + a direction (`MouseMove:Up`), a zone (`MouseMove:Low`), or both (`MouseMove:UpHigh`) |\n\n";
    writeln!(s, "- Directions: {dirs}").unwrap();
    writeln!(
        s,
        "- Gyro directions: {}",
        code(GyroDir::ALL.map(GyroDir::name))
    )
    .unwrap();
    writeln!(s, "- Zones: {zones}").unwrap();
    s += "\n";

    s += "Other names that control names accept:\n\n";
    s += "| Name | Also written |\n|---|---|\n";
    alias_rows(
        &mut s,
        "",
        PadButton::ALL.map(|b| (b.name(), b)),
        PAD_BUTTON_ALIASES,
    );
    alias_rows(
        &mut s,
        "",
        [("LT", Trigger::Left), ("RT", Trigger::Right)],
        TRIGGER_NAMES,
    );
    alias_rows(
        &mut s,
        "Mouse:",
        MouseButton::ALL.map(|b| (b.name(), b)),
        MOUSE_BUTTON_ALIASES,
    );
    s += "\nA trigger's other names work in its zone controls too (`R2Full`, `ZRHigh`).\n\n";

    s += "### Keys\n\n";
    s += "Key names are positions on a US keyboard, not the characters printed on the keys.\n\n";
    let all: Vec<&str> = Key::ALL.iter().map(|k| k.name()).collect();
    let mut letters: Vec<&str> = all.iter().copied().filter(|n| n.len() == 1).collect();
    let mut digits: Vec<&str> = all
        .iter()
        .copied()
        .filter(|n| n.starts_with("Digit"))
        .collect();
    letters.sort_unstable();
    digits.sort_unstable();
    let fkeys: Vec<&str> = all
        .iter()
        .copied()
        .filter(|n| n.len() > 1 && n.starts_with('F') && n[1..].bytes().all(|b| b.is_ascii_digit()))
        .collect();
    let mut rest: Vec<&str> = all
        .iter()
        .copied()
        .filter(|n| !letters.contains(n) && !digits.contains(n) && !fkeys.contains(n))
        .collect();
    rest.sort_unstable();
    writeln!(s, "- Letters: {}", code(&letters)).unwrap();
    writeln!(s, "- Digits: {} (or simply `0`–`9`)", code(&digits)).unwrap();
    writeln!(s, "- Function keys: {}", code(&fkeys)).unwrap();
    writeln!(s, "- Other keys: {}", code(&rest)).unwrap();
    s += "\n| Key | Also written |\n|---|---|\n";
    alias_rows(
        &mut s,
        "",
        Key::ALL.iter().map(|&k| (k.name(), k)),
        KEY_ALIASES,
    );
    s += "\n";

    s += "### Action values\n\n";
    s += "Actions take these values exactly as written here.\n\n";
    s += "| Action | Values |\n|---|---|\n";
    writeln!(
        s,
        "| `pad` | {} (no other names) |",
        code(PadButton::ALL.map(written))
    )
    .unwrap();
    s += "| `key` | a key name or another name of a key, as above |\n";
    writeln!(s, "| `mouse` | {} |", code(MouseButton::ALL.map(written))).unwrap();
    writeln!(s, "| `wheel` | {} |", code(WheelDir::ALL.map(written))).unwrap();
    writeln!(s, "| `stick` | {} |", code(Stick::ALL.map(written))).unwrap();
    writeln!(s, "| `trigger` | {} |", code(Trigger::ALL.map(written))).unwrap();
    writeln!(s, "| `mouse_move` | {} |", code(Dir::ALL.map(written))).unwrap();
    writeln!(s, "| `touch` | {} |", code(TouchGesture::ALL.map(written))).unwrap();
    writeln!(s, "| `shake` | {} |", code(ShakeAxis::ALL.map(written))).unwrap();
    s
}

/// Every control name the section's patterns stand for.
fn control_names() -> Vec<String> {
    let mut names: Vec<String> = PadButton::ALL.iter().map(|b| b.name().into()).collect();
    let dir_zone = |prefix: &str, dirs: &[&str]| {
        let mut v: Vec<String> = Vec::new();
        for d in dirs.iter().copied().chain([""]) {
            if !d.is_empty() {
                v.push(format!("{prefix}{d}"));
            }
            for z in Zone::ALL {
                v.push(format!("{prefix}{d}{}", z.name()));
            }
        }
        v
    };
    let dirs: Vec<&str> = Dir::ALL.iter().map(|d| d.name()).collect();
    for stick in Stick::ALL {
        names.extend(dir_zone(&format!("{}Stick", stick.name()), &dirs));
    }
    for &(t, _) in TRIGGER_NAMES {
        names.push(t.into());
        names.push(format!("{t}Full"));
        names.extend(Zone::ALL.map(|z| format!("{t}{}", z.name())));
    }
    names.extend(TouchRegion::ALL.map(|r| format!("Touch{}", r.name())));
    names.extend(TouchGesture::ALL.map(|g| format!("Touch{}", g.name())));
    for h in Half::ALL {
        let parts = TouchRegion::ALL.map(TouchRegion::name);
        for p in parts.iter().chain(&["Tap", "Click"]) {
            names.push(format!("Trackpad{}{p}", h.name()));
        }
    }
    let gyro: Vec<&str> = GyroDir::ALL.iter().map(|d| d.name()).collect();
    names.extend(dir_zone("Gyro", &gyro));
    names.extend(Key::ALL.iter().map(|k| format!("Key:{}", k.name())));
    names.extend(KEY_ALIASES.iter().map(|(n, _)| format!("Key:{n}")));
    names.extend(MouseButton::ALL.map(|b| format!("Mouse:{}", b.name())));
    names.extend(
        MOUSE_BUTTON_ALIASES
            .iter()
            .map(|(n, _)| format!("Mouse:{n}")),
    );
    names.extend(WheelDir::ALL.map(|w| format!("Wheel:{}", w.name())));
    names.extend(dir_zone("MouseMove:", &dirs));
    names.extend(PAD_BUTTON_ALIASES.iter().map(|(n, _)| n.to_string()));
    names
}

#[test]
fn every_listed_control_name_parses() {
    for name in control_names() {
        assert!(Control::from_str(&name).is_ok(), "`{name}` does not parse");
    }
}

#[test]
fn every_listed_action_value_parses() {
    let mut actions: Vec<String> = Vec::new();
    actions.extend(PadButton::ALL.map(|b| format!("pad = \"{}\"", written(b))));
    actions.extend(Key::ALL.iter().map(|k| format!("key = \"{}\"", k.name())));
    actions.extend(KEY_ALIASES.iter().map(|(n, _)| format!("key = \"{n}\"")));
    actions.extend(MouseButton::ALL.map(|b| format!("mouse = \"{}\"", written(b))));
    actions.extend(WheelDir::ALL.map(|w| format!("wheel = \"{}\"", written(w))));
    actions.extend(Stick::ALL.map(|t| format!("stick = \"{}\", x = 0.0, y = 1.0", written(t))));
    actions.extend(Trigger::ALL.map(|t| format!("trigger = \"{}\"", written(t))));
    actions.extend(Dir::ALL.map(|d| format!("mouse_move = \"{}\"", written(d))));
    actions.extend(TouchGesture::ALL.map(|g| format!("touch = \"{}\"", written(g))));
    actions.extend(ShakeAxis::ALL.map(|a| format!("shake = \"{}\"", written(a))));
    for a in actions {
        let src = format!("[[mappings]]\ntrigger = \"A\"\nactions = [{{ {a} }}]\n");
        assert!(
            dp_config::parse_toml(&src).is_ok(),
            "`{{ {a} }}` does not parse"
        );
    }
}

#[test]
fn the_reference_lists_every_name() {
    let doc = std::fs::read_to_string(DOC).unwrap();
    let crlf = doc.contains("\r\n");
    let doc = doc.replace("\r\n", "\n");
    let start = doc.find(START).expect("the names:start marker") + START.len();
    let end = doc.find(END).expect("the names:end marker");
    let want = format!("\n{}\n", names_section());
    if doc[start..end] == want {
        return;
    }
    if std::env::var_os("DOPPELPAD_BLESS").is_some() {
        let mut new = format!("{}{want}{}", &doc[..start], &doc[end..]);
        if crlf {
            new = new.replace('\n', "\r\n");
        }
        std::fs::write(DOC, new).unwrap();
        return;
    }
    panic!(
        "the name lists in docs/PROFILES.md are out of date; rewrite them with\n\
         DOPPELPAD_BLESS=1 cargo test -p dp-config --test profile_names"
    );
}
