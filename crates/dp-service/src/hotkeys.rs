//! The controller hotkeys on disk (`hotkeys.toml` next to the slot state).

use std::path::Path;

use dp_core::hotkeys::Hotkeys;

/// Load from `path`. A missing file yields the defaults; an unreadable or invalid one too,
/// with a warning.
pub fn load(path: &Path) -> Hotkeys {
    let h: Hotkeys = dp_config::load_toml_or_default(path);
    let problems = h.problems();
    if problems.is_empty() {
        return h;
    }
    tracing::warn!("ignoring {}: {}", path.display(), problems.join("; "));
    Hotkeys::default()
}

/// Write to `path` atomically (temporary file, then rename).
pub fn save(path: &Path, hotkeys: &Hotkeys) -> std::io::Result<()> {
    dp_config::save_toml(path, hotkeys)
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
