//! Where the library lives: the default profiles folder in the settings folder, or a folder the
//! user chose (a synced one, say). The choice is kept in a small file in the settings folder, so
//! the app, the daemon and the CLI agree on it. Moving the library copies every file, checks
//! each copy, and removes the old files only once the new folder is in use.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Serialize;

/// File in the settings folder naming the chosen profiles folder; absent for the default.
pub const LOCATION_FILE: &str = "profiles-location.txt";
/// File marking a folder as a Doppelpad library, so that a library synced from another
/// computer can be used as it is.
pub const MARKER_FILE: &str = ".doppelpad-library";
const MARKER_TEXT: &str = "Doppelpad keeps its games and configs in this folder.\n";

#[derive(Debug, thiserror::Error)]
pub enum MoveError {
    #[error("{0} is the folder in use")]
    Same(PathBuf),
    #[error("{0} is inside the folder in use, or holds it")]
    Nested(PathBuf),
    #[error("{0} holds other files; choose an empty folder or a Doppelpad library")]
    Foreign(PathBuf),
    #[error("{0}: the copy differs from the original")]
    Verify(PathBuf),
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> MoveError + '_ {
    move |e| MoveError::Io(path.to_path_buf(), e)
}

/// The chosen profiles folder written in `config_dir`, if any. A relative or empty entry is
/// ignored, so a broken file falls back to the default.
pub fn chosen_dir(config_dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(config_dir.join(LOCATION_FILE)).ok()?;
    let dir = PathBuf::from(text.trim());
    dir.is_absolute().then_some(dir)
}

/// Remember `dir` as the profiles folder; `None` goes back to the default.
pub fn save_choice(config_dir: &Path, dir: Option<&Path>) -> Result<(), MoveError> {
    let file = config_dir.join(LOCATION_FILE);
    match dir {
        Some(dir) => {
            fs::create_dir_all(config_dir).map_err(io_err(config_dir))?;
            fs::write(&file, format!("{}\n", dir.display())).map_err(io_err(&file))
        }
        None => match fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(MoveError::Io(file, e)),
            _ => Ok(()),
        },
    }
}

/// What a folder offered as the new library holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// Missing or empty: the library can move there.
    Empty,
    /// A Doppelpad library already (synced from another computer, say): it can be used as it
    /// is, and the current library stays where it is.
    Library,
    /// Other files: refused, since merging is not offered.
    Foreign,
}

/// What `dir` holds. `default_dir` (the default profiles folder) counts as a library even
/// without the marker, since only Doppelpad writes there.
pub fn target(dir: &Path, default_dir: &Path) -> Target {
    let Ok(mut entries) = fs::read_dir(dir) else {
        return Target::Empty;
    };
    if entries.next().is_none() {
        Target::Empty
    } else if dir.join(MARKER_FILE).is_file() || same_dir(dir, default_dir) {
        Target::Library
    } else {
        Target::Foreign
    }
}

/// Mark `dir` as a library (see [`MARKER_FILE`]).
pub fn mark(dir: &Path) -> Result<(), MoveError> {
    fs::create_dir_all(dir).map_err(io_err(dir))?;
    let file = dir.join(MARKER_FILE);
    if file.is_file() {
        return Ok(());
    }
    fs::write(&file, MARKER_TEXT).map_err(io_err(&file))
}

fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Whether two paths name the same folder.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    absolute(a) == absolute(b)
}

/// What `to` holds, or why the library in `from` cannot go there. `default_dir` as in
/// [`target`].
pub fn check_move(from: &Path, to: &Path, default_dir: &Path) -> Result<Target, MoveError> {
    let (a, b) = (absolute(from), absolute(to));
    if a == b {
        return Err(MoveError::Same(to.to_path_buf()));
    }
    if a.starts_with(&b) || b.starts_with(&a) {
        return Err(MoveError::Nested(to.to_path_buf()));
    }
    Ok(target(to, default_dir))
}

/// A library copied to its new folder; the originals are still in place.
#[derive(Debug)]
pub struct Copied {
    from: PathBuf,
    to: PathBuf,
    /// Every file, relative to the library folder.
    files: Vec<PathBuf>,
}

impl Copied {
    /// Remove the original files, then the folders they leave empty (the old library folder
    /// too). Returns how many files could not be removed; their copies are safe.
    pub fn remove_originals(self) -> usize {
        let failed = remove_files(&self.from, &self.files);
        remove_empty_dirs(&self.from);
        failed
    }

    /// Take the copies back (the move was given up); the originals stay as they are.
    pub fn undo(self) {
        remove_files(&self.to, &self.files);
        let _ = fs::remove_file(self.to.join(MARKER_FILE));
        remove_empty_dirs(&self.to);
    }
}

/// Copy the library in `from` into `to`, which must be missing or empty, check every copy
/// byte for byte and mark `to` as a library. On failure the copies made so far are removed.
pub fn copy_library(from: &Path, to: &Path) -> Result<Copied, MoveError> {
    if check_move(from, to, from)? != Target::Empty {
        return Err(MoveError::Foreign(to.to_path_buf()));
    }
    let mut files = Vec::new();
    if from.is_dir() {
        list_files(from, Path::new(""), &mut files).map_err(io_err(from))?;
    }
    let copied = Copied {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        files,
    };
    match copy_all(&copied) {
        Ok(()) => Ok(copied),
        Err(e) => {
            copied.undo();
            Err(e)
        }
    }
}

fn copy_all(c: &Copied) -> Result<(), MoveError> {
    fs::create_dir_all(&c.to).map_err(io_err(&c.to))?;
    for rel in &c.files {
        let (src, dst) = (c.from.join(rel), c.to.join(rel));
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        fs::copy(&src, &dst).map_err(io_err(&src))?;
    }
    for rel in &c.files {
        let (src, dst) = (c.from.join(rel), c.to.join(rel));
        let original = fs::read(&src).map_err(io_err(&src))?;
        if fs::read(&dst).map_err(io_err(&dst))? != original {
            return Err(MoveError::Verify(dst));
        }
    }
    mark(&c.to)
}

/// Every file under `dir`, relative to the library folder (`rel` is `dir`'s place in it),
/// in a stable order.
fn list_files(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = rel.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            list_files(&entry.path(), &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Remove these files under `root`; how many could not be removed.
fn remove_files(root: &Path, files: &[PathBuf]) -> usize {
    files
        .iter()
        .filter(|rel| {
            let path = root.join(rel);
            let failed = fs::remove_file(&path).is_err() && path.exists();
            if failed {
                tracing::warn!("cannot remove {}", path.display());
            }
            failed
        })
        .count()
}

/// Remove `dir` and the folders under it that are empty, deepest first.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    // Fails (and keeps the folder) when something is left in it.
    let _ = fs::remove_dir(dir);
}

/// `path` in the library moved from `from` to `to`; `None` for a file outside it.
pub fn moved_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| to.join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_choice_is_kept_and_forgotten() {
        let cfg = tempfile::tempdir().unwrap();
        assert_eq!(chosen_dir(cfg.path()), None);
        let dir = cfg.path().join("sync").join("pads");
        save_choice(cfg.path(), Some(&dir)).unwrap();
        assert_eq!(chosen_dir(cfg.path()), Some(dir));
        save_choice(cfg.path(), None).unwrap();
        assert_eq!(chosen_dir(cfg.path()), None);
        save_choice(cfg.path(), None).unwrap();
        // A relative entry is ignored.
        fs::write(cfg.path().join(LOCATION_FILE), "pads\n").unwrap();
        assert_eq!(chosen_dir(cfg.path()), None);
    }

    #[test]
    fn moves_every_file_and_removes_the_originals() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("profiles");
        let to = root.path().join("synced").join("pads");
        write(&from.join("loose.toml"), "a");
        write(&from.join("game").join("game.toml"), "b");
        write(&from.join("game").join("fight.toml"), "c");
        write(&from.join("presets").join("p.toml"), "d");

        let copied = copy_library(&from, &to).unwrap();
        assert_eq!(
            fs::read_to_string(to.join("game").join("fight.toml")).unwrap(),
            "c"
        );
        assert!(
            from.join("loose.toml").is_file(),
            "originals stay until removed"
        );
        assert_eq!(target(&to, &from), Target::Library);
        assert_eq!(copied.remove_originals(), 0);
        assert!(!from.exists(), "the empty old folder goes too");
        for f in ["loose.toml", "game/game.toml", "presets/p.toml"] {
            assert!(to.join(f).is_file(), "{f}");
        }
    }

    #[test]
    fn undo_leaves_the_target_as_it_was() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("profiles");
        let to = root.path().join("new");
        write(&from.join("game").join("a.toml"), "a");
        copy_library(&from, &to).unwrap().undo();
        assert!(!to.exists());
        assert!(from.join("game").join("a.toml").is_file());
    }

    #[test]
    fn refuses_folders_with_other_files_and_nested_ones() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("profiles");
        write(&from.join("a.toml"), "a");
        let other = root.path().join("documents");
        write(&other.join("letter.txt"), "x");
        assert!(matches!(
            copy_library(&from, &other),
            Err(MoveError::Foreign(_))
        ));
        assert_eq!(target(&other, &from), Target::Foreign);
        assert!(matches!(
            copy_library(&from, &from.join("inner")),
            Err(MoveError::Nested(_))
        ));
        assert!(matches!(
            copy_library(&from, root.path()),
            Err(MoveError::Nested(_))
        ));
        assert!(matches!(
            copy_library(&from, &from),
            Err(MoveError::Same(_))
        ));
        // An existing library is used rather than moved into.
        let lib = root.path().join("lib");
        write(&lib.join("b.toml"), "b");
        mark(&lib).unwrap();
        assert_eq!(check_move(&from, &lib, &from).unwrap(), Target::Library);
        assert!(matches!(
            copy_library(&from, &lib),
            Err(MoveError::Foreign(_))
        ));
        // The default folder is a library without the marker.
        assert_eq!(target(&from, &from), Target::Library);
        assert_eq!(target(&root.path().join("missing"), &from), Target::Empty);
    }

    #[test]
    fn paths_follow_the_library() {
        let (from, to) = (Path::new("/a/profiles"), Path::new("/sync/pads"));
        assert_eq!(
            moved_path(Path::new("/a/profiles/g/x.toml"), from, to),
            Some(PathBuf::from("/sync/pads/g/x.toml"))
        );
        assert_eq!(moved_path(Path::new("/a/profilesX/x.toml"), from, to), None);
        assert_eq!(moved_path(Path::new("/elsewhere/x.toml"), from, to), None);
    }
}
