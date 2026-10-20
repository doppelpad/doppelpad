//! The controller hotkeys on disk (`hotkeys.toml` next to the slot state).

use std::path::Path;

use dp_core::hotkeys::Hotkeys;

/// Load from `path`. A missing file yields the defaults; an unreadable or invalid one too,
/// with a warning.
pub fn load(path: &Path) -> Hotkeys {
    let Ok(src) = std::fs::read_to_string(path) else {
        return Hotkeys::default();
    };
    match toml::from_str::<Hotkeys>(&src) {
        Ok(h) if h.problems().is_empty() => h,
        Ok(h) => {
            tracing::warn!("ignoring {}: {}", path.display(), h.problems().join("; "));
            Hotkeys::default()
        }
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", path.display());
            Hotkeys::default()
        }
    }
}

/// Write to `path` atomically (temporary file, then rename).
pub fn save(path: &Path, hotkeys: &Hotkeys) -> std::io::Result<()> {
    let text = toml::to_string_pretty(hotkeys).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use dp_core::pad::PadButton;

    use super::*;

    #[test]
    fn roundtrip_and_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hotkeys.toml");
        assert_eq!(load(&path), Hotkeys::default(), "missing file = defaults");
        let h = Hotkeys {
            hold_ms: 800,
            remap: vec![PadButton::Guide],
            ..Default::default()
        };
        save(&path, &h).unwrap();
        assert_eq!(load(&path), h);
        std::fs::write(&path, "hold_ms = 1").unwrap();
        assert_eq!(load(&path), Hotkeys::default(), "invalid = defaults");
    }
}
