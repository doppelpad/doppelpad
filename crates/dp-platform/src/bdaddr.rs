//! Bluetooth device addresses, as the OSes and the controllers write them.

use std::fmt;

/// A Bluetooth device address: 48 bits, the first byte of its text form the most significant
/// (`98:B6:E9:12:34:56` is `0x98B6E9123456`), as Windows' `BTH_ADDR` holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct BdAddr(u64);

impl BdAddr {
    /// The address of this number; `None` beyond 48 bits.
    pub fn new(raw: u64) -> Option<BdAddr> {
        (raw >> 48 == 0).then_some(BdAddr(raw))
    }

    /// The address in text such as `a4:ae:12:34:56:78`, `10-18-49-75-FC-86` or `a4ae12345678`
    /// (Sony pads report theirs as their serial number): twelve hex digits, whatever separates
    /// them.
    pub fn parse(text: &str) -> Option<BdAddr> {
        let hex: String = text.chars().filter(char::is_ascii_hexdigit).collect();
        if hex.len() != 12
            || text
                .chars()
                .any(|c| c.is_ascii_alphanumeric() && !c.is_ascii_hexdigit())
        {
            return None;
        }
        u64::from_str_radix(&hex, 16).ok().and_then(BdAddr::new)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Least significant byte first, as the Linux kernel takes it (`bdaddr_t`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn le_bytes(self) -> [u8; 6] {
        let b = self.0.to_le_bytes();
        [b[0], b[1], b[2], b[3], b[4], b[5]]
    }

    /// The six bytes in text order, joined by `sep` in upper case (`98:B6:E9:12:34:56` with a
    /// colon, BlueZ's `98_B6_E9_12_34_56` with an underscore).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn upper(self, sep: char) -> String {
        let mut out = String::with_capacity(17);
        for (i, b) in self.le_bytes().iter().rev().enumerate() {
            if i > 0 {
                out.push(sep);
            }
            out.push_str(&format!("{b:02X}"));
        }
        out
    }
}

/// Twelve lower-case hex digits (`98b6e9123456`): the name a Bluetooth LE controller is listed
/// under, the same on every OS.
impl fmt::Display for BdAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:012x}", self.0)
    }
}

/// How a Bluetooth LE device's address was made: fixed by its maker, or made up by the device.
#[cfg_attr(target_os = "macos", allow(dead_code))] // CoreBluetooth does not tell
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeAddrType {
    Public,
    Random,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_in_every_spelling() {
        let a = Some(BdAddr(0xA4AE_1234_5678));
        assert_eq!(BdAddr::parse("a4:ae:12:34:56:78"), a);
        assert_eq!(BdAddr::parse("A4AE12345678"), a);
        assert_eq!(BdAddr::parse("A4_AE_12_34_56_78"), a);
        assert_eq!(
            BdAddr::parse("10-18-49-75-fc-86"),
            Some(BdAddr(0x1018_4975_FC86))
        );
        assert_eq!(BdAddr::parse("12345"), None);
        assert_eq!(BdAddr::parse("a4:ae:12:34:56:78:9a"), None);
        assert_eq!(
            BdAddr::parse("dev_A4_AE_12_34_56_78"),
            None,
            "not only digits"
        );
        assert_eq!(BdAddr::new(1 << 48), None);
    }

    #[test]
    fn addresses_written_out() {
        let a = BdAddr(0x98B6_E912_3456);
        assert_eq!(a.to_string(), "98b6e9123456");
        assert_eq!(BdAddr(0x12).to_string(), "000000000012");
        assert_eq!(a.le_bytes(), [0x56, 0x34, 0x12, 0xE9, 0xB6, 0x98]);
        assert_eq!(a.upper(':'), "98:B6:E9:12:34:56");
        assert_eq!(a.upper('_'), "98_B6_E9_12_34_56");
    }
}
