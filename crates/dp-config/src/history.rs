//! Earlier versions of configs: before a config is saved over, the file as it was goes into a
//! history folder of its own (`history_dir()/<game folder>/<config>/<time>.toml`), and the
//! newest few are kept. Loose configs, those directly in the library folder, keep theirs under
//! [`LOOSE`] instead of the library folder's name, so that they stay with them when the library
//! moves.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dp_core::profile::Profile;

use crate::library::{self, Result, io};

/// How many earlier versions of a config are kept.
pub const KEEP: usize = 20;

/// The history folder of the loose configs. Game folders made by the app never start with `_`
/// (see [`library::slugify`]).
const LOOSE: &str = "_loose";

/// One kept version: its file and when it was replaced (ms since 1970).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Version {
    pub path: PathBuf,
    pub time_ms: u64,
}

fn name_of(path: Option<&std::ffi::OsStr>) -> String {
    path.map_or_else(|| "_".into(), |n| n.to_string_lossy().into_owned())
}

/// The history folder of a config in the library folder `library`: by its game folder's name
/// (or [`LOOSE`]) and its file name.
fn folder(root: &Path, config: &Path, library: &Path) -> PathBuf {
    let parent = config.parent();
    let game = if parent.is_some_and(|p| crate::relocate::same_dir(p, library)) {
        LOOSE.to_owned()
    } else {
        name_of(parent.and_then(Path::file_name))
    };
    root.join(game).join(name_of(config.file_stem()))
}

/// Before loose configs had [`LOOSE`], their history was kept under the library folder's name.
/// Move it to where it belongs now, unless a game folder of that name may own it.
fn adopt_old_folder(root: &Path, config: &Path, library: &Path) {
    let new = folder(root, config, library);
    let Some(library_name) = library.file_name() else {
        return;
    };
    let in_library = config
        .parent()
        .is_some_and(|p| crate::relocate::same_dir(p, library));
    let old_game = root.join(library_name);
    let old = old_game.join(name_of(config.file_stem()));
    let a_game_has_it = library.join(library_name).join(library::GAME_FILE).exists();
    if !in_library || a_game_has_it || new.exists() || !old.is_dir() {
        return;
    }
    let moved =
        std::fs::create_dir_all(root.join(LOOSE)).and_then(|()| std::fs::rename(&old, &new));
    match moved {
        // Fails (and keeps the folder) when other configs' histories are left in it.
        Ok(()) => drop(std::fs::remove_dir(&old_game)),
        Err(e) => tracing::warn!(
            "cannot move the earlier versions of {}: {e}",
            config.display()
        ),
    }
}

/// Keep the config file `config` of the library folder `library` as it is now before it is
/// saved over: a version with the time `now_ms`, unless it equals the newest version already
/// kept. The oldest go beyond `keep`. A config that does not exist yet keeps nothing.
pub fn keep_version(
    config: &Path,
    root: &Path,
    library: &Path,
    now_ms: u64,
    keep: usize,
) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Ok(());
    };
    let dir = folder(root, config, library);
    let kept = versions(config, root, library);
    if kept
        .first()
        .is_some_and(|v| std::fs::read_to_string(&v.path).is_ok_and(|t| t == text))
    {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let path = dir.join(format!("{now_ms}.toml"));
    crate::store::write_atomic(&path, &text).map_err(io(&path))?;
    for old in versions(config, root, library).into_iter().skip(keep) {
        let _ = std::fs::remove_file(old.path);
    }
    Ok(())
}

/// The kept versions of the config `config` of the library folder `library`, the newest first.
pub fn versions(config: &Path, root: &Path, library: &Path) -> Vec<Version> {
    adopt_old_folder(root, config, library);
    let Ok(entries) = std::fs::read_dir(folder(root, config, library)) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let time_ms = path
                .file_stem()?
                .to_str()?
                .parse()
                .ok()
                .filter(|_| path.extension().is_some_and(|x| x == "toml"))?;
            Some(Version { path, time_ms })
        })
        .collect();
    out.sort_by_key(|v| std::cmp::Reverse(v.time_ms));
    out
}

/// Save `profile` over the config `path` of the library folder `library` (see
/// [`library::save_profile_atomic`]), keeping the file as it was as an earlier version in
/// `root` first; a version that cannot be kept is logged and does not stop the save.
pub fn save_profile(path: &Path, profile: &Profile, root: &Path, library: &Path) -> Result<()> {
    if let Err(e) = keep_version(path, root, library, now_ms(), KEEP) {
        tracing::warn!("cannot keep the earlier version of {}: {e}", path.display());
    }
    library::save_profile_atomic(path, profile)
}

/// Milliseconds since 1970, for naming a version.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_changed_versions_newest_first_and_drops_the_oldest() {
        let tmp = tempfile::tempdir().unwrap();
        let library = tmp.path().join("profiles");
        let game = library.join("halo");
        std::fs::create_dir_all(&game).unwrap();
        let config = game.join("default.toml");
        let root = tmp.path().join("history");

        keep_version(&config, &root, &library, 1, 3).unwrap();
        assert!(
            versions(&config, &root, &library).is_empty(),
            "nothing to keep yet"
        );

        for (i, text) in ["a", "b", "b", "c", "d"].iter().enumerate() {
            std::fs::write(&config, text).unwrap();
            keep_version(&config, &root, &library, 10 + i as u64, 3).unwrap();
        }
        let kept = versions(&config, &root, &library);
        // "b" twice in a row is kept once; only the newest three stay.
        assert_eq!(
            kept.iter().map(|v| v.time_ms).collect::<Vec<_>>(),
            [14, 13, 11]
        );
        assert_eq!(std::fs::read_to_string(&kept[0].path).unwrap(), "d");
        assert!(kept[0].path.starts_with(root.join("halo").join("default")));
    }

    #[test]
    fn loose_configs_keep_their_versions_when_the_library_moves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("history");
        let (old_lib, new_lib) = (tmp.path().join("profiles"), tmp.path().join("synced"));
        for lib in [&old_lib, &new_lib] {
            std::fs::create_dir_all(lib).unwrap();
        }
        let old = old_lib.join("loose.toml");
        std::fs::write(&old, "a").unwrap();
        keep_version(&old, &root, &old_lib, 1, KEEP).unwrap();
        assert!(root.join(LOOSE).join("loose").is_dir());

        let moved = new_lib.join("loose.toml");
        std::fs::write(&moved, "b").unwrap();
        assert_eq!(versions(&moved, &root, &new_lib).len(), 1);
    }

    #[test]
    fn earlier_histories_of_loose_configs_are_moved_where_they_belong() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("history");
        let library = tmp.path().join("profiles");
        std::fs::create_dir_all(&library).unwrap();
        let config = library.join("loose.toml");
        std::fs::write(&config, "now").unwrap();
        // As kept before: under the library folder's name.
        let old = root.join("profiles").join("loose");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("5.toml"), "then").unwrap();

        let kept = versions(&config, &root, &library);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].path.starts_with(root.join(LOOSE)));
        assert!(!root.join("profiles").exists(), "the empty old folder goes");

        // A game folder named like the library folder may own such a history: it stays.
        let game = library.join("profiles");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join(library::GAME_FILE), "").unwrap();
        let other = library.join("other.toml");
        let theirs = root.join("profiles").join("other");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("6.toml"), "x").unwrap();
        assert!(versions(&other, &root, &library).is_empty());
        assert!(theirs.join("6.toml").is_file());
    }

    #[test]
    fn saving_keeps_the_file_as_it_was() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("history");
        let library = tmp.path().join("profiles");
        let game = library::create_game(&library, "Halo").unwrap();
        let path = game.configs[0].path.clone();
        let before = std::fs::read_to_string(&path).unwrap();
        let profile = Profile {
            name: "Changed".into(),
            ..Profile::default()
        };
        save_profile(&path, &profile, &root, &library).unwrap();
        assert_eq!(crate::load(&path).unwrap().0.name, "Changed");
        let kept = versions(&path, &root, &library);
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read_to_string(&kept[0].path).unwrap(), before);
    }
}
