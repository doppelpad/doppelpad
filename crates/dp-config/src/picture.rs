//! Pictures for the window: game covers and radial menu icons travel to the page as `data:`
//! URLs, so the page needs no file access; game bundles carry their cover as base64.

use std::path::Path;

/// Picture file extensions, with their media types.
pub const IMAGE_TYPES: [(&str, &str); 7] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
];

/// Game pictures larger than this stay out of bundles and the window.
pub const MAX_IMAGE: u64 = 8 << 20;

/// The extension of a picture file as [`IMAGE_TYPES`] lists it (`PNG` → `png`); `None` for a
/// file that is not a picture.
pub(crate) fn extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    IMAGE_TYPES
        .iter()
        .find(|(x, _)| x.eq_ignore_ascii_case(ext))
        .map(|(x, _)| *x)
}

/// The media type of a picture file, by its extension.
fn media_type(path: &Path) -> Option<&'static str> {
    let ext = extension(path)?;
    IMAGE_TYPES.iter().find(|(x, _)| *x == ext).map(|(_, m)| *m)
}

/// A picture file as a `data:` URL, if it is one of [`IMAGE_TYPES`] and at most `max` bytes.
pub fn data_url(path: &Path, max: u64) -> Option<String> {
    let mime = media_type(path)?;
    if std::fs::metadata(path).ok()?.len() > max {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in standard base64, with padding.
pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes of standard base64 `text` (white space and padding ignored); `None` when it holds
/// other characters.
pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0);
    for c in text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        let v = B64.iter().position(|&b| b == c)? as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(unbase64(&base64(&bytes)).unwrap(), bytes);
        assert_eq!(unbase64("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(unbase64("T*"), None);
    }

    #[test]
    fn pictures_become_data_urls_up_to_a_size() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("icon.PNG");
        std::fs::write(&png, b"\x89PNG").unwrap();
        assert_eq!(extension(&png), Some("png"));
        assert_eq!(
            data_url(&png, 4).as_deref(),
            Some("data:image/png;base64,iVBORw==")
        );
        assert_eq!(data_url(&png, 3), None, "too large");
        let svg = dir.path().join("ring.svg");
        std::fs::write(&svg, "<svg/>").unwrap();
        assert!(
            data_url(&svg, MAX_IMAGE)
                .unwrap()
                .starts_with("data:image/svg+xml;")
        );
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, "x").unwrap();
        assert_eq!((extension(&txt), data_url(&txt, MAX_IMAGE)), (None, None));
        assert_eq!(data_url(&dir.path().join("gone.png"), MAX_IMAGE), None);
    }
}
