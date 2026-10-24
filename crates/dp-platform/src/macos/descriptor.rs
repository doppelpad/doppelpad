//! Reading HID report descriptors, which IOKit hands over as they are.

/// One item of a report descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// 0 main, 1 global, 2 local.
    pub kind: u8,
    pub tag: u8,
    pub data: &'a [u8],
}

impl Item<'_> {
    /// The data as an unsigned number (little-endian, up to four bytes).
    pub fn unsigned(&self) -> u32 {
        self.data
            .iter()
            .rev()
            .fold(0, |v, &b| (v << 8) | u32::from(b))
    }
}

/// The global item that sets the report ID.
const REPORT_ID: (u8, u8) = (1, 8);

/// The short items of a descriptor, in order; long items are skipped, and a truncated item
/// ends the list.
pub fn items(desc: &[u8]) -> impl Iterator<Item = Item<'_>> {
    let mut rest = desc;
    std::iter::from_fn(move || {
        loop {
            let (&prefix, tail) = rest.split_first()?;
            if prefix == 0xFE {
                // Long item: data size, tag, data.
                let size = usize::from(*tail.first()?);
                rest = tail.get(2 + size..)?;
                continue;
            }
            let size = match prefix & 0x03 {
                3 => 4,
                n => usize::from(n),
            };
            let data = tail.get(..size)?;
            rest = &tail[size..];
            return Some(Item {
                kind: (prefix >> 2) & 0x03,
                tag: prefix >> 4,
                data,
            });
        }
    })
}

/// Whether the device numbers its reports: then each report starts with its ID.
pub fn numbered(desc: &[u8]) -> bool {
    items(desc).any(|i| (i.kind, i.tag) == REPORT_ID && i.unsigned() != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a DualSense's USB descriptor: generic desktop, game pad, report 1.
    const DUALSENSE: &[u8] = &[0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x30];
    /// A plain pad without report IDs: usage page, usage, collection, logical maximum 255.
    const PLAIN: &[u8] = &[0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x26, 0xFF, 0x00, 0xC0];

    #[test]
    fn reads_items() {
        let all: Vec<Item> = items(PLAIN).collect();
        assert_eq!(all.len(), 5);
        assert_eq!((all[0].kind, all[0].tag, all[0].unsigned()), (1, 0, 1));
        assert_eq!((all[2].kind, all[2].tag), (0, 0x0A), "collection");
        assert_eq!(all[3].unsigned(), 255, "two data bytes");
        assert!(all[4].data.is_empty(), "end collection has no data");
    }

    #[test]
    fn report_ids() {
        assert!(numbered(DUALSENSE));
        assert!(!numbered(PLAIN));
        assert!(!numbered(&[]));
    }

    #[test]
    fn long_and_truncated_items() {
        // A long item of two bytes, then a report ID.
        assert!(numbered(&[0xFE, 0x02, 0x10, 0xAA, 0xBB, 0x85, 0x03]));
        // A report ID item whose data byte is missing.
        assert!(!numbered(&[0x05, 0x01, 0x85]));
    }
}
