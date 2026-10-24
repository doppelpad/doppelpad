//! Programs known to keep controllers to themselves, so that "the controller is in use" can
//! name the likely one, and the vendor and product ids in a device path (the device itself
//! cannot be asked while another program holds it).

/// Executables (matched in any case) and the names users know them by. Tools that take
/// controllers on purpose come first; Steam comes last because it runs on many computers
/// without holding anything.
const KNOWN: &[(&str, &str)] = &[
    ("DS4Windows.exe", "DS4Windows"),
    ("BetterJoy.exe", "BetterJoy"),
    ("BetterJoyForCemu.exe", "BetterJoy"),
    ("Dolphin.exe", "Dolphin"),
    // Dolphin and Steam as macOS names them.
    ("Dolphin", "Dolphin"),
    ("steam.exe", "Steam"),
    ("steam_osx", "Steam"),
];

/// The running program that most likely holds a controller; `running` tells whether an
/// executable runs.
pub fn likely_holder(running: impl Fn(&str) -> bool) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(exe, _)| running(exe))
        .map(|(_, name)| *name)
}

/// "`holder` is using `what`", or "another program …" when no known one runs, with what to do.
pub fn busy_message(what: &str, holder: Option<&str>) -> String {
    match holder {
        Some(h) => format!("{h} is using {what}; close it or turn off its controller support"),
        None => {
            format!("another program is using {what}; close it, then look for controllers again")
        }
    }
}

/// Vendor and product id in a device path or instance id: `VID_054C&PID_0CE6` (USB) or
/// `VID&0002054C_PID&0CE6` (Bluetooth, the vendor id after a two-byte source).
pub fn ids_in_path(path: &str) -> Option<(u16, u16)> {
    let up = path.to_ascii_uppercase();
    let hex = |at: usize| {
        up.get(at..at + 4)
            .and_then(|s| u16::from_str_radix(s, 16).ok())
    };
    if let (Some(v), Some(p)) = (up.find("VID_"), up.find("PID_")) {
        return Some((hex(v + 4)?, hex(p + 4)?));
    }
    let (v, p) = (up.find("VID&")?, up.find("PID&")?);
    Some((hex(v + 8)?, hex(p + 4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_likely_holder() {
        let running = |list: &'static [&'static str]| {
            move |exe: &str| list.iter().any(|r| r.eq_ignore_ascii_case(exe))
        };
        assert_eq!(likely_holder(running(&["STEAM.EXE"])), Some("Steam"));
        assert_eq!(
            likely_holder(running(&["steam.exe", "DS4Windows.exe"])),
            Some("DS4Windows"),
            "a tool that takes controllers on purpose wins over Steam"
        );
        assert_eq!(likely_holder(running(&["notepad.exe"])), None);
        assert_eq!(
            busy_message("the DualSense", Some("Steam")),
            "Steam is using the DualSense; close it or turn off its controller support"
        );
        assert!(
            busy_message("the adapter", None).starts_with("another program is using the adapter")
        );
    }

    #[test]
    fn ids_from_usb_and_bluetooth_paths() {
        assert_eq!(
            ids_in_path(
                r"\\?\hid#vid_054c&pid_0ce6&mi_03#7&1a2b&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
            ),
            Some((0x054C, 0x0CE6))
        );
        assert_eq!(
            ids_in_path(
                r"\\?\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002057e_pid&2009#9&2&0&0000#{4d1e55b2}"
            ),
            Some((0x057E, 0x2009))
        );
        assert_eq!(
            ids_in_path(r"USB\VID_057E&PID_0337\5&1"),
            Some((0x057E, 0x0337))
        );
        assert_eq!(ids_in_path(r"\\?\hid#something"), None);
    }
}
