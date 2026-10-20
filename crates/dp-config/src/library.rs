//! Games and their configs on disk, as shown in the GUI's profile panel.
//!
//! ```text
//! profiles_dir/<game>/game.toml      GameMeta: name, processes, image, default config, order
//! profiles_dir/<game>/<config>.toml  one Profile per file
//! profiles_dir/*.toml | *.json       loose configs, listed as "unsorted"
//! ```

use std::path::{Path, PathBuf};

use dp_core::profile::Profile;
use serde::{Deserialize, Serialize};

use crate::{Diagnostics, LoadError, compat, is_profile_file, load, to_toml};

/// File holding a game's metadata inside its folder.
pub const GAME_FILE: &str = "game.toml";
/// First line of every file written by Doppelpad (see [`to_toml`]).
const OUR_HEADER: &str = "# Doppelpad profile";

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("{0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Meta(PathBuf, #[source] toml::de::Error),
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error("{0}: {1}")]
    Import(PathBuf, #[source] compat::ImportError),
    #[error("cannot move {0} to the recycle bin: {1}")]
    Trash(PathBuf, String),
    #[error("{0} is not a game folder")]
    NotAGame(PathBuf),
}

type Result<T, E = LibraryError> = std::result::Result<T, E>;

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |e| LibraryError::Io(path.to_path_buf(), e)
}

/// Metadata of one game.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameMeta {
    pub name: String,
    /// Executables that activate the game's default config automatically.
    pub processes: Vec<String>,
    /// Background picture (path, absolute or relative to the game folder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// File name of the config used for automatic switching; defaults to the first one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<String>,
    /// Position in the list (lower first); games without one follow, sorted by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

/// One config file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfigEntry {
    pub path: PathBuf,
    /// The profile's name, or the file name if it cannot be read.
    pub name: String,
    /// Why the file cannot be loaded, if it cannot.
    pub error: Option<String>,
}

/// One game folder with its configs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Game {
    pub dir: PathBuf,
    pub meta: GameMeta,
    pub configs: Vec<ConfigEntry>,
}

impl Game {
    /// Config used for automatic switching: `default_config`, else the first one.
    pub fn default_config(&self) -> Option<&ConfigEntry> {
        self.meta
            .default_config
            .as_deref()
            .and_then(|f| {
                self.configs
                    .iter()
                    .find(|c| c.path.file_name().is_some_and(|n| n == f))
            })
            .or_else(|| self.configs.first())
    }
}

/// Everything in the profiles directory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Library {
    pub games: Vec<Game>,
    /// Config files directly in the profiles directory.
    pub unsorted: Vec<ConfigEntry>,
    /// Folders or files that could not be read.
    pub errors: Vec<String>,
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("config")
        .to_owned()
}

fn config_entry(path: PathBuf) -> ConfigEntry {
    match load(&path) {
        Ok((p, _)) => ConfigEntry {
            name: p.name,
            path,
            error: None,
        },
        Err(e) => ConfigEntry {
            name: file_stem(&path),
            path,
            error: Some(e.to_string()),
        },
    }
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn is_config_file(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n != GAME_FILE)
        && !path.extension().is_some_and(|e| e == "bak" || e == "tmp")
        && is_profile_file(path)
}

/// Read one game folder.
pub fn read_game(dir: &Path) -> Result<Game> {
    let meta_path = dir.join(GAME_FILE);
    if !meta_path.is_file() {
        return Err(LibraryError::NotAGame(dir.to_path_buf()));
    }
    let src = std::fs::read_to_string(&meta_path).map_err(io(&meta_path))?;
    let mut meta: GameMeta =
        toml::from_str(&src).map_err(|e| LibraryError::Meta(meta_path.clone(), e))?;
    if meta.name.is_empty() {
        meta.name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Game")
            .to_owned();
    }
    let configs = sorted_entries(dir)
        .into_iter()
        .filter(|p| is_config_file(p))
        .map(config_entry)
        .collect();
    Ok(Game {
        dir: dir.to_path_buf(),
        meta,
        configs,
    })
}

/// Scan the profiles directory one level deep.
pub fn scan(dir: &Path) -> Library {
    let mut lib = Library::default();
    for path in sorted_entries(dir) {
        if path.is_dir() {
            if !path.join(GAME_FILE).is_file() {
                continue;
            }
            match read_game(&path) {
                Ok(g) => lib.games.push(g),
                Err(e) => lib.errors.push(e.to_string()),
            }
        } else if is_config_file(&path) {
            lib.unsorted.push(config_entry(path));
        }
    }
    lib.games.sort_by(|a, b| {
        let key = |g: &Game| (g.meta.order.unwrap_or(i32::MAX), g.meta.name.to_lowercase());
        key(a).cmp(&key(b))
    });
    lib
}

/// File-name-safe form of a name: ASCII lowercase letters, digits and dashes.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = match c {
            'ä' | 'Ä' => Some("ae"),
            'ö' | 'Ö' => Some("oe"),
            'ü' | 'Ü' => Some("ue"),
            'ß' => Some("ss"),
            _ => None,
        }
        .map(str::to_owned)
        .unwrap_or_else(|| c.to_ascii_lowercase().to_string());
        for ch in c.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
            } else if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

/// `base`, or `base-2`, `base-3`, … whichever does not exist yet in `dir`.
fn unique_path(dir: &Path, base: &str, ext: Option<&str>) -> PathBuf {
    let make = |stem: &str| match ext {
        Some(e) => dir.join(format!("{stem}.{e}")),
        None => dir.join(stem),
    };
    let first = make(base);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| make(&format!("{base}-{n}")))
        .find(|p| !p.exists())
        .expect("unbounded")
}

fn write_meta(dir: &Path, meta: &GameMeta) -> Result<()> {
    let path = dir.join(GAME_FILE);
    let text = toml::to_string_pretty(meta)
        .map_err(|e| LibraryError::Io(path.clone(), std::io::Error::other(e)))?;
    write_atomic(&path, &text)
}

/// Create a new game folder with one empty config named "Default".
pub fn create_game(root: &Path, name: &str) -> Result<Game> {
    std::fs::create_dir_all(root).map_err(io(root))?;
    let dir = unique_path(root, &slugify(name), None);
    std::fs::create_dir(&dir).map_err(io(&dir))?;
    write_meta(
        &dir,
        &GameMeta {
            name: name.to_owned(),
            ..Default::default()
        },
    )?;
    create_config(&dir, "Default")?;
    read_game(&dir)
}

/// Replace a game's metadata.
pub fn update_game_meta(dir: &Path, meta: &GameMeta) -> Result<()> {
    if !dir.join(GAME_FILE).is_file() {
        return Err(LibraryError::NotAGame(dir.to_path_buf()));
    }
    write_meta(dir, meta)
}

fn trash(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| LibraryError::Trash(path.to_path_buf(), e.to_string()))
}

/// Move a game folder (with its configs) to the recycle bin.
pub fn delete_game(dir: &Path) -> Result<()> {
    if !dir.join(GAME_FILE).is_file() {
        return Err(LibraryError::NotAGame(dir.to_path_buf()));
    }
    trash(dir)
}

/// Create an empty config in a game folder and return its path.
pub fn create_config(game_dir: &Path, name: &str) -> Result<PathBuf> {
    let path = unique_path(game_dir, &slugify(name), Some("toml"));
    let profile = Profile {
        name: name.to_owned(),
        ..Default::default()
    };
    save_profile_atomic(&path, &profile)?;
    Ok(path)
}

/// Copy a config next to itself under a new name ("<name> copy").
pub fn duplicate_config(path: &Path) -> Result<PathBuf> {
    let (mut profile, _) = load(path)?;
    profile.name = format!("{} copy", profile.name);
    let dir = path.parent().unwrap_or(Path::new("."));
    let target = unique_path(dir, &slugify(&profile.name), Some("toml"));
    save_profile_atomic(&target, &profile)?;
    Ok(target)
}

/// Rename a config. Only the profile's name changes, never the file name, so slots and
/// other references to the path stay valid.
pub fn rename_config(path: &Path, name: &str) -> Result<()> {
    let (mut profile, _) = load(path)?;
    profile.name = name.to_owned();
    if crate::Format::detect(path, &std::fs::read_to_string(path).map_err(io(path))?)
        == crate::Format::Json
    {
        // Never rewrite an imported original; the caller should import it first.
        return Err(LibraryError::Io(
            path.to_path_buf(),
            std::io::Error::other("JSON profiles are read-only; import them first"),
        ));
    }
    save_profile_atomic(path, &profile)
}

/// Move a config file to the recycle bin.
pub fn delete_config(path: &Path) -> Result<()> {
    trash(path)
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension(match path.extension().and_then(|e| e.to_str()) {
        Some(e) => format!("{e}.tmp"),
        None => "tmp".into(),
    });
    std::fs::write(&tmp, text).map_err(io(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

/// Save a profile as TOML: written to a temporary file and renamed over the target.
/// A file that was not written by Doppelpad (e.g. hand-written with comments, which
/// would be lost) is copied to `<file>.bak` once before its first overwrite.
pub fn save_profile_atomic(path: &Path, profile: &Profile) -> Result<()> {
    if let Ok(existing) = std::fs::read_to_string(path)
        && !existing.starts_with(OUR_HEADER)
    {
        let bak = path.with_extension(match path.extension().and_then(|e| e.to_str()) {
            Some(e) => format!("{e}.bak"),
            None => "bak".into(),
        });
        if !bak.exists() {
            std::fs::copy(path, &bak).map_err(io(&bak))?;
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    write_atomic(path, &to_toml(profile))
}

/// Import a JSON profile (schema 3.0) as a new TOML config in `game_dir`. The source
/// file is never modified or moved.
pub fn import_into(game_dir: &Path, source: &Path) -> Result<(PathBuf, Diagnostics)> {
    let src = std::fs::read_to_string(source).map_err(io(source))?;
    let (mut profile, diags) =
        compat::import(&src).map_err(|e| LibraryError::Import(source.to_path_buf(), e))?;
    if profile.name.is_empty() || profile.name == "Game" {
        profile.name = file_stem(source);
    }
    let target = unique_path(game_dir, &slugify(&profile.name), Some("toml"));
    save_profile_atomic(&target, &profile)?;
    Ok((target, diags))
}

/// The game whose `processes` contain `exe` (case-insensitive), with the config to use.
pub fn game_for_process<'a>(lib: &'a Library, exe: &str) -> Option<(&'a Game, PathBuf)> {
    lib.games.iter().find_map(|g| {
        if !g.meta.processes.iter().any(|p| p.eq_ignore_ascii_case(exe)) {
            return None;
        }
        g.default_config().map(|c| (g, c.path.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{ "schemaVersion": 3.0, "appVersion": "9.2.0",
      "config": { "appName": "Imported", "minTimeUnit": 1 },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "keyboard": { "buttonId": 57, "description": "DIK_SPACE" } } ] } ] }"#;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Elden Ring: Nightreign"), "elden-ring-nightreign");
        assert_eq!(slugify("Über Spiel!"), "ueber-spiel");
        assert_eq!(slugify("***"), "untitled");
    }

    #[test]
    fn create_scan_and_collisions() {
        let root = tempfile::tempdir().unwrap();
        let a = create_game(root.path(), "My Game").unwrap();
        let b = create_game(root.path(), "My Game").unwrap();
        assert_ne!(a.dir, b.dir, "slug collision gets a suffix");
        assert!(b.dir.ends_with("my-game-2"));
        assert_eq!(a.configs.len(), 1);
        assert_eq!(a.configs[0].name, "Default");

        // A loose config, a broken config and a folder without game.toml.
        std::fs::write(root.path().join("loose.toml"), "name = \"Loose\"\n").unwrap();
        std::fs::write(
            a.dir.join("broken.toml"),
            "[[mappings]]\ntrigger = \"Nope\"\n",
        )
        .unwrap();
        std::fs::create_dir(root.path().join("not-a-game")).unwrap();

        let mut meta = a.meta.clone();
        meta.order = Some(1);
        meta.name = "Zebra".into();
        update_game_meta(&a.dir, &meta).unwrap();

        let lib = scan(root.path());
        assert_eq!(lib.games.len(), 2);
        assert_eq!(lib.games[0].meta.name, "Zebra", "order wins over name");
        assert_eq!(lib.unsorted.len(), 1);
        assert_eq!(lib.unsorted[0].name, "Loose");
        let broken = lib.games[0]
            .configs
            .iter()
            .find(|c| c.path.ends_with("broken.toml"))
            .unwrap();
        assert!(broken.error.is_some());
        assert_eq!(broken.name, "broken");
    }

    #[test]
    fn atomic_save_makes_one_backup_of_foreign_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hand.toml");
        std::fs::write(&path, "# my notes\nname = \"Hand\"\n").unwrap();
        let (mut p, _) = load(&path).unwrap();
        p.name = "Changed".into();
        save_profile_atomic(&path, &p).unwrap();
        let bak = dir.path().join("hand.toml.bak");
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "# my notes\nname = \"Hand\"\n"
        );

        p.name = "Again".into();
        save_profile_atomic(&path, &p).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "# my notes\nname = \"Hand\"\n",
            "backup kept"
        );
        assert_eq!(load(&path).unwrap().0.name, "Again");
        assert!(!dir.path().join("hand.toml.tmp").exists());
        // Our own files never get a backup.
        let own = dir.path().join("own.toml");
        save_profile_atomic(&own, &p).unwrap();
        save_profile_atomic(&own, &p).unwrap();
        assert!(!dir.path().join("own.toml.bak").exists());
        // Backups are not listed as configs.
        assert!(!is_config_file(&bak));
    }

    #[test]
    fn import_leaves_original_alone() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Imp").unwrap();
        let src = root.path().join("orig.rwcfg");
        std::fs::write(&src, JSON).unwrap();
        let (path, _) = import_into(&game.dir, &src).unwrap();
        assert_eq!(std::fs::read_to_string(&src).unwrap(), JSON);
        assert_eq!(load(&path).unwrap().0.name, "Imported");
        assert!(path.extension().is_some_and(|e| e == "toml"));
    }

    #[test]
    fn rename_keeps_path_and_duplicate_copies() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "G").unwrap();
        let path = game.configs[0].path.clone();
        rename_config(&path, "Aggressive").unwrap();
        assert_eq!(load(&path).unwrap().0.name, "Aggressive");
        let copy = duplicate_config(&path).unwrap();
        assert_ne!(copy, path);
        assert_eq!(load(&copy).unwrap().0.name, "Aggressive copy");
    }

    #[test]
    fn process_lookup_uses_default_config() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Shooter").unwrap();
        let second = create_config(&game.dir, "Sniper").unwrap();
        let meta = GameMeta {
            processes: vec!["Shooter.exe".into()],
            default_config: Some(second.file_name().unwrap().to_string_lossy().into_owned()),
            ..game.meta.clone()
        };
        update_game_meta(&game.dir, &meta).unwrap();
        let lib = scan(root.path());
        let (g, cfg) = game_for_process(&lib, "shooter.EXE").unwrap();
        assert_eq!(g.meta.name, "Shooter");
        assert_eq!(cfg, second);
        assert!(game_for_process(&lib, "other.exe").is_none());
    }
}
