//! The controller hotkeys on disk (`hotkeys.toml` next to the slot state).

use std::path::Path;

use dp_config::{FileFormat, Loaded};
use dp_core::hotkeys::Hotkeys;

/// The format of `hotkeys.toml`.
const FORMAT: FileFormat = FileFormat::new(&[]);

/// Load from `path`. A missing file yields the defaults; an unreadable or invalid one too,
/// with the problem.
pub fn load(path: &Path) -> Loaded<Hotkeys> {
    let loaded: Loaded<Hotkeys> = dp_config::load_toml_or_default(path, &FORMAT);
    let problems = loaded.value.problems();
    if problems.is_empty() {
        return loaded;
    }
    dp_config::set_aside(path, &problems.join("; "))
}

/// Write to `path` atomically (temporary file, then rename).
pub fn save(path: &Path, hotkeys: &Hotkeys) -> std::io::Result<()> {
    dp_config::save_toml(path, hotkeys, &FORMAT)
}

#[cfg(test)]
mod tests {
    use dp_core::pad::PadButton;

    use super::*;

    #[test]
    fn roundtrip_and_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hotkeys.toml");
        assert_eq!(
            load(&path).value,
            Hotkeys::default(),
            "missing file = defaults"
        );
        let h = Hotkeys {
            hold_ms: 800,
            remap: vec![PadButton::Guide],
            ..Default::default()
        };
        save(&path, &h).unwrap();
        assert_eq!(
            load(&path),
            Loaded {
                value: h,
                problem: None
            }
        );
        std::fs::write(&path, "hold_ms = 1").unwrap();
        let invalid = load(&path);
        assert_eq!(invalid.value, Hotkeys::default(), "invalid = defaults");
        assert!(invalid.problem.is_some());
        assert!(
            dir.path().join("hotkeys.toml.bak").exists(),
            "and kept aside"
        );
    }
}
