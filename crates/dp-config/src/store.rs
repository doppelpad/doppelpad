//! Writing settings and profile files so that a crash never leaves half a file, and reading
//! settings files back with their defaults.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::format::{FileFormat, one_line};

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

/// Write `value` to `path` as TOML in `format`, atomically.
pub fn save_toml<T: Serialize>(path: &Path, value: &T, format: &FileFormat) -> io::Result<()> {
    let text = toml::to_string_pretty(value).map_err(io::Error::other)?;
    write_atomic(path, format.stamp(&text))
}

/// A settings file as read.
#[derive(Debug, Default, PartialEq)]
pub struct Loaded<T> {
    pub value: T,
    /// Why the file could not be used, for the user. The value is then the defaults, and the
    /// file was copied aside (see [`set_aside`]).
    pub problem: Option<String>,
}

impl<T> Loaded<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Loaded<U> {
        Loaded {
            value: f(self.value),
            problem: self.problem,
        }
    }
}

/// Read `path` as TOML in `format`, bringing an older file up to date. A missing file gives
/// the defaults. So does an unreadable or invalid one, or one from a newer Doppelpad, after it
/// was copied aside.
pub fn load_toml_or_default<T: DeserializeOwned + Default>(
    path: &Path,
    format: &FileFormat,
) -> Loaded<T> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::default(),
        Err(e) => return set_aside(path, &e.to_string()),
    };
    match format.parse(&text) {
        Ok(value) => Loaded {
            value,
            problem: None,
        },
        Err(e) => set_aside(path, &one_line(&e, &text)),
    }
}

/// Give up on the settings file `path` because of `why`: copy it to `<file>.bak` beside it
/// (replacing an older copy), so the defaults saved over it later do not lose it, and use the
/// defaults.
pub fn set_aside<T: Default>(path: &Path, why: &str) -> Loaded<T> {
    let bak = with_suffix(path, "bak");
    let kept = match std::fs::copy(path, &bak) {
        Ok(_) => format!("the file is kept as {}", bak.display()),
        Err(e) => format!("keeping a copy failed: {e}"),
    };
    let problem = format!("{}: {why}. Using the defaults; {kept}.", path.display());
    tracing::warn!("{problem}");
    Loaded {
        value: T::default(),
        problem: Some(problem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Serialize, serde::Deserialize)]
    struct Settings {
        n: u32,
    }

    const FORMAT: FileFormat = FileFormat::new(&[]);

    fn load(path: &Path) -> Loaded<Settings> {
        load_toml_or_default(path, &FORMAT)
    }

    #[test]
    fn saves_atomically_and_loads_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("s.toml");
        assert_eq!(load(&path), Loaded::default());
        save_toml(&path, &Settings { n: 3 }, &FORMAT).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "format = 1\nn = 3\n"
        );
        assert_eq!(load(&path).value, Settings { n: 3 });
        assert!(load(&path).problem.is_none());
        assert!(
            !with_suffix(&path, "tmp").exists(),
            "the temporary file is gone"
        );
        assert_eq!(
            with_suffix(Path::new("a/state.toml"), "tmp"),
            Path::new("a/state.toml.tmp")
        );
        assert_eq!(
            with_suffix(Path::new("a/raw"), "bak"),
            Path::new("a/raw.bak")
        );
    }

    #[test]
    fn a_file_that_cannot_be_used_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.toml");
        let bak = dir.path().join("s.toml.bak");
        std::fs::write(&path, "n = \"x\"").unwrap();
        let invalid = load(&path);
        assert_eq!(invalid.value, Settings::default());
        let problem = invalid.problem.unwrap();
        assert!(problem.contains("line 1: invalid type"), "{problem}");
        assert!(problem.contains("s.toml.bak"), "{problem}");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "n = \"x\"");

        std::fs::write(&path, "format = 2\nn = 4\n").unwrap();
        let newer = load(&path).problem.unwrap();
        assert!(newer.contains("made by a newer Doppelpad"), "{newer}");
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "format = 2\nn = 4\n",
            "the newest copy replaces an older one"
        );
    }
}
