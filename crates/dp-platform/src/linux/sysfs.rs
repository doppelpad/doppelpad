//! What sysfs tells about a controller's HID device besides its input: its ids in `uevent`,
//! the battery under `power_supply/` and the lights under `leds/` (hid-playstation, hid-sony,
//! hid-nintendo), and for a Bluetooth controller the adapter it is connected through, with the
//! object paths BlueZ gives such devices. Names and values are the drivers' (see
//! `docs/LINUX.md#sources`). No I/O, so it is unit-tested on every OS.

use dp_core::pad::Rgb;

use crate::Battery;
use crate::bdaddr::BdAddr;

/// A battery from its `power_supply` attributes: `capacity` in percent (hid-playstation) or
/// `capacity_level` (hid-nintendo), and `status`. Levels give the percentages the Windows
/// driver reads from the same report. `None` until the driver has had a report.
pub fn battery(
    capacity: Option<&str>,
    capacity_level: Option<&str>,
    status: Option<&str>,
) -> Option<Battery> {
    let status = status?.trim();
    if status == "Unknown" {
        return None;
    }
    let level = match capacity {
        Some(c) => c.trim().parse::<u8>().ok()?.min(100),
        None => match capacity_level?.trim() {
            "Full" => 100,
            "High" => 60,
            "Normal" => 30,
            "Low" => 10,
            "Critical" => 0,
            _ => return None,
        },
    };
    Some(Battery {
        level,
        charging: matches!(status, "Charging" | "Full"),
        wired: false,
    })
}

/// One attribute to write: the LED's directory under `leds/`, the attribute, the value.
pub type LedWrite = (String, &'static str, String);

/// What to write for a light bar colour, given the names of the device's LEDs. The
/// DualSense's light bar is one multicolour LED (`…:rgb:indicator`, red, green, blue), the
/// DualShock 4's are three (`…:red`, `…:green`, `…:blue`); all go up to 255.
pub fn lightbar_writes(leds: &[String], c: Rgb) -> Vec<LedWrite> {
    if let Some(led) = leds.iter().find(|l| l.ends_with(":rgb:indicator")) {
        return vec![
            (
                led.clone(),
                "multi_intensity",
                format!("{} {} {}", c.r, c.g, c.b),
            ),
            (led.clone(), "brightness", "255".into()),
        ];
    }
    [(":red", c.r), (":green", c.g), (":blue", c.b)]
        .into_iter()
        .filter_map(|(colour, value)| {
            let led = leds.iter().find(|l| l.ends_with(colour))?;
            Some((led.clone(), "brightness", value.to_string()))
        })
        .collect()
}

/// What to write for player `n`'s lights (0 turns them off), lit as the Windows drivers light
/// them: the DualSense's five white `…:white:player-1` … `player-5` from the centre outwards,
/// the four green ones of Switch controllers filling up from the first. (A Switch
/// controller's blue `…:blue:player-5` is its home button.)
pub fn player_writes(leds: &[String], n: u8) -> Vec<LedWrite> {
    let lights = |colour: &str| -> Vec<(u32, &String)> {
        let prefix = format!(":{colour}:player-");
        leds.iter()
            .filter_map(|l| {
                let (_, index) = l.rsplit_once(prefix.as_str())?;
                Some((index.parse().ok()?, l))
            })
            .filter(|&(i, _)| (1..=8).contains(&i))
            .collect()
    };
    let (lights, mask) = match (lights("white"), lights("green")) {
        (white, _) if !white.is_empty() => (white, crate::sony::dualsense_player_lights(n)),
        (_, green) => (green, crate::nintendo::player_mask(n)),
    };
    lights
        .into_iter()
        .map(|(i, l)| {
            let on = mask & (1 << (i - 1)) != 0;
            (l.clone(), "brightness", u8::from(on).to_string())
        })
        .collect()
}

/// What a HID device's `uevent` file says about it.
#[derive(Debug, PartialEq, Eq)]
pub struct Uevent {
    /// The bus it is on (`BUS_USB` and so on, as for input devices).
    pub bus: u16,
    pub vid: u16,
    pub pid: u16,
    pub phys: String,
}

/// `HID_ID=0003:0000057E:00002069` and `HID_PHYS=…` from a HID device's `uevent` file.
pub fn parse_uevent(text: &str) -> Option<Uevent> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
    };
    let mut ids = field("HID_ID")?.split(':');
    let mut hex = || u32::from_str_radix(ids.next()?, 16).ok();
    let (bus, vid, pid) = (hex()?, hex()?, hex()?);
    Some(Uevent {
        bus: u16::try_from(bus).ok()?,
        vid: u16::try_from(vid).ok()?,
        pid: u16::try_from(pid).ok()?,
        phys: field("HID_PHYS").unwrap_or_default().to_owned(),
    })
}

/// BlueZ's object path for the device with Bluetooth address `mac` whose HID device is
/// `hid_device` (`/sys/devices/…/bluetooth/hci0/hci0:256/0005:054C:0CE6.0003`): the adapter
/// comes from that path, else it is `hci0`.
pub fn bluez_device_path(hid_device: &str, mac: &str) -> Option<String> {
    let address = BdAddr::parse(mac)?;
    let mut parts = hid_device.split('/');
    let adapter = parts
        .find(|&p| p == "bluetooth")
        .and_then(|_| parts.next())
        .filter(|a| a.starts_with("hci"))
        .unwrap_or("hci0");
    Some(format!("/org/bluez/{adapter}/dev_{}", address.upper('_')))
}

/// The address of the device at a BlueZ object path (`…/dev_98_B6_E9_12_34_56`).
pub fn bluez_address(path: &str) -> Option<BdAddr> {
    let name = path.rsplit('/').next()?.strip_prefix("dev_")?;
    let well_formed = name.split('_').count() == 6 && name.split('_').all(|b| b.len() == 2);
    well_formed.then(|| BdAddr::parse(name))?
}

/// The adapter a device's BlueZ object path is under (`/org/bluez/hci0`).
pub fn bluez_adapter(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(adapter, _)| adapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batteries() {
        let b = |level, charging| {
            Some(Battery {
                level,
                charging,
                wired: false,
            })
        };
        // hid-playstation
        assert_eq!(
            battery(Some("55\n"), None, Some("Discharging\n")),
            b(55, false)
        );
        assert_eq!(battery(Some("100"), None, Some("Full")), b(100, true));
        assert_eq!(battery(Some("40"), None, Some("Charging")), b(40, true));
        assert_eq!(
            battery(Some("40"), None, Some("Not charging")),
            b(40, false)
        );
        assert_eq!(
            battery(Some("0"), None, Some("Unknown")),
            None,
            "no report yet"
        );
        // hid-nintendo
        assert_eq!(
            battery(None, Some("High"), Some("Discharging")),
            b(60, false)
        );
        assert_eq!(
            battery(None, Some("Critical"), Some("Charging")),
            b(0, true)
        );
        assert_eq!(battery(None, Some("Unknown"), Some("Discharging")), None);
        assert_eq!(battery(None, None, Some("Discharging")), None);
        assert_eq!(battery(Some("55"), None, None), None);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter()
            .map(|n| format!("0005:054C:0CE6.0003:{n}"))
            .collect()
    }

    #[test]
    fn light_bars() {
        let c = Rgb {
            r: 255,
            g: 0,
            b: 64,
        };
        let dualsense = names(&["rgb:indicator", "white:player-1", "white:player-2"]);
        assert_eq!(
            lightbar_writes(&dualsense, c),
            [
                (dualsense[0].clone(), "multi_intensity", "255 0 64".into()),
                (dualsense[0].clone(), "brightness", "255".into()),
            ]
        );
        let ds4 = names(&["red", "green", "blue", "global"]);
        assert_eq!(
            lightbar_writes(&ds4, c),
            [
                (ds4[0].clone(), "brightness", "255".into()),
                (ds4[1].clone(), "brightness", "0".into()),
                (ds4[2].clone(), "brightness", "64".into()),
            ]
        );
        let switch = names(&["green:player-1", "blue:player-5"]);
        assert!(lightbar_writes(&switch, c).is_empty(), "no light bar");
    }

    fn lit(writes: &[LedWrite]) -> String {
        writes.iter().map(|w| w.2.as_str()).collect()
    }

    #[test]
    fn player_lights() {
        let dualsense = names(&[
            "rgb:indicator",
            "white:player-1",
            "white:player-2",
            "white:player-3",
            "white:player-4",
            "white:player-5",
        ]);
        assert_eq!(lit(&player_writes(&dualsense, 1)), "00100");
        assert_eq!(lit(&player_writes(&dualsense, 2)), "01010");
        assert_eq!(lit(&player_writes(&dualsense, 4)), "11011");
        assert_eq!(lit(&player_writes(&dualsense, 0)), "00000");
        assert!(
            player_writes(&dualsense, 1)
                .iter()
                .all(|w| w.1 == "brightness")
        );

        // A Pro Controller: four player lights and the home button's light.
        let pro = names(&[
            "green:player-1",
            "green:player-2",
            "green:player-3",
            "green:player-4",
            "blue:player-5",
        ]);
        let p3 = player_writes(&pro, 3);
        assert_eq!(lit(&p3), "1110");
        assert!(p3.iter().all(|w| !w.0.ends_with("blue:player-5")));
        assert_eq!(lit(&player_writes(&pro, 1)), "1000");

        assert!(player_writes(&names(&["red", "green", "blue"]), 1).is_empty());
    }

    #[test]
    fn uevents() {
        // A Pro Controller 2 as the Steam Deck's kernel lists it.
        let pro = "DRIVER=hid-generic\nHID_ID=0003:0000057E:00002069\n\
                   HID_NAME=Nintendo Switch 2 Pro Controller\n\
                   HID_PHYS=usb-0000:04:00.3-1.3/input0\nHID_UNIQ=\n\
                   MODALIAS=hid:b0003g0001v0000057Ep00002069\n";
        assert_eq!(
            parse_uevent(pro),
            Some(Uevent {
                bus: super::super::codes::BUS_USB,
                vid: 0x057E,
                pid: 0x2069,
                phys: "usb-0000:04:00.3-1.3/input0".into(),
            })
        );
        // The touchscreen on I2C.
        let i2c = "HID_ID=0018:00002808:00001015\nHID_PHYS=i2c-FTS3528:00\n";
        assert_eq!(parse_uevent(i2c).unwrap().bus, 0x18);
        assert_eq!(parse_uevent("HID_NAME=no ids\n"), None);
        assert_eq!(parse_uevent("HID_ID=0003:0000057E\n"), None, "cut short");
        assert_eq!(
            parse_uevent("HID_ID=0003:0001057E:00002069\n"),
            None,
            "no u16"
        );
    }

    #[test]
    fn bluez_paths() {
        let hid = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-1/1-1:1.0/bluetooth/hci1/hci1:256/0005:054C:0CE6.0003";
        assert_eq!(
            bluez_device_path(hid, "a0:b1:c2:d3:e4:f5").as_deref(),
            Some("/org/bluez/hci1/dev_A0_B1_C2_D3_E4_F5")
        );
        // Bluetooth LE through uhid: no adapter in the path.
        assert_eq!(
            bluez_device_path(
                "/sys/devices/virtual/misc/uhid/0005:045E:0B13.0004",
                "a0:b1:c2:d3:e4:f5"
            )
            .as_deref(),
            Some("/org/bluez/hci0/dev_A0_B1_C2_D3_E4_F5")
        );
        assert_eq!(bluez_device_path(hid, ""), None);
        assert_eq!(bluez_device_path(hid, "usb-0000:00:14.0-1/input0"), None);

        let path = "/org/bluez/hci0/dev_98_B6_E9_12_34_56";
        assert_eq!(bluez_address(path), BdAddr::new(0x98B6_E912_3456));
        assert_eq!(bluez_address("/org/bluez/hci0"), None);
        assert_eq!(bluez_address(&format!("{path}/service0010")), None);
        assert_eq!(bluez_address("/org/bluez/hci0/dev_98_B6"), None);
        assert_eq!(bluez_address("/org/bluez/hci0/dev_98B6E9123456"), None);
        assert_eq!(bluez_adapter(path), "/org/bluez/hci0");
    }
}
