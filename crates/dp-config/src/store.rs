//! Writing settings and profile files so that a crash never leaves half a file, and reading
//! settings files back with their defaults.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// `path` with `suffix` added to its extension (`state.toml` → `state.toml.tmp`).
pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    path.with_extension(match path.extension().and_then(|e| e.to_str()) {
        Some(e) => format!("{e}.{suffix}"),
        None => suffix.to_owned(),
    })
}

/// Write `contents` to `path` atomically: into `<file>.tmp` next to it, then renamed over it.
/// Creates the folder if needed.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = with_suffix(path, "tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Write `value` to `path` as TOML, atomically.
pub fn save_toml<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = toml::to_string_pretty(value).map_err(io::Error::other)?;
    write_atomic(path, text)
}

/// Read `path` as TOML. A missing file gives the defaults; an unreadable or invalid one too,
/// with a warning.
pub fn load_toml_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", path.display());
            return T::default();
        }
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        tracing::warn!("ignoring {}: {e}", path.display());
        T::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Serialize, serde::Deserialize)]
    struct Settings {
        n: u32,
    }

    #[test]
    fn saves_atomically_and_loads_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("s.toml");
        assert_eq!(load_toml_or_default::<Settings>(&path), Settings::default());
        save_toml(&path, &Settings { n: 3 }).unwrap();
        assert_eq!(load_toml_or_default::<Settings>(&path), Settings { n: 3 });
        assert!(
            !with_suffix(&path, "tmp").exists(),
            "the temporary file is gone"
        );
        std::fs::write(&path, "n = \"x\"").unwrap();
        assert_eq!(load_toml_or_default::<Settings>(&path), Settings::default());
        assert_eq!(
            with_suffix(Path::new("a/state.toml"), "tmp"),
            Path::new("a/state.toml.tmp")
        );
        assert_eq!(
            with_suffix(Path::new("a/raw"), "bak"),
            Path::new("a/raw.bak")
        );
    }
}
