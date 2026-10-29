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

use crate::launches::{self, Launch};
use crate::{
    Diagnostics, FileFormat, Format, LoadError, SaveError, compat, is_profile_file, load, picture,
    to_toml,
};

/// File holding a game's metadata inside its folder.
pub(crate) const GAME_FILE: &str = "game.toml";
/// The format of `game.toml`; see [`crate::PROFILE_FORMAT`].
const GAME_FORMAT: FileFormat = FileFormat::new(&[]);

/// Why a change to the library failed.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("{0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Meta(PathBuf, #[source] toml::de::Error),
    #[error("{0}: {1}")]
    Serialize(PathBuf, #[source] toml::ser::Error),
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error("{0}: JSON profiles are read-only; import them first")]
    ReadOnly(PathBuf),
    /// A game bundle that cannot be written or read.
    #[error("{0}: {1}")]
    Bundle(PathBuf, String),
    #[error("{0}: {1}")]
    Preset(PathBuf, #[source] toml::de::Error),
    #[error("cannot move {0} to the recycle bin: {1}")]
    Trash(PathBuf, #[source] trash::Error),
    #[error("{0} is not a game folder")]
    NotAGame(PathBuf),
    #[error("{0} is not a picture (PNG, JPEG, GIF, WebP, SVG or ICO)")]
    NotAPicture(PathBuf),
}

pub type Result<T, E = LibraryError> = std::result::Result<T, E>;

pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |e| LibraryError::Io(path.to_path_buf(), e)
}

/// The error for a TOML file `path` that could not be saved.
pub(crate) fn unsaved(path: &Path) -> impl FnOnce(SaveError) -> LibraryError + '_ {
    move |e| match e {
        SaveError::Serialize(e) => LibraryError::Serialize(path.to_path_buf(), e),
        SaveError::Io(e) => LibraryError::Io(path.to_path_buf(), e),
    }
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
    /// A note about the game, shown under its name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One config file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfigEntry {
    pub path: PathBuf,
    /// The profile's name, or the file name if it cannot be read.
    pub name: String,
    /// Why the file cannot be loaded, if it cannot.
    pub error: Option<String>,
    /// Executables that switch to this config (read with it, not sent to the app).
    #[serde(skip)]
    pub processes: Vec<String>,
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

/// Whether `path` lies inside the folder `root` (not `root` itself), compared as absolute paths.
/// A path that climbs with `..` never does, whatever it would land on.
pub fn contains(root: &Path, path: &Path) -> bool {
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return false;
    }
    let absolute = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (root, path) = (absolute(root), absolute(path));
    path != root && path.starts_with(&root)
}

/// Whether two program names mean the same program: case-insensitively, and with or without
/// `.exe`, so that a game listed by its Windows program is recognised where it runs natively,
/// on macOS or Linux (`Hades.exe` and `Hades`).
pub fn same_program(a: &str, b: &str) -> bool {
    fn stem(name: &str) -> &str {
        match name.len().checked_sub(4) {
            Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".exe") => {
                &name[..i]
            }
            _ => name,
        }
    }
    stem(a).eq_ignore_ascii_case(stem(b))
}

impl Library {
    /// The config to switch to for the executable `exe` (see [`same_program`]): the default
    /// config of a game that lists it, else a loose config that does.
    pub fn config_for_process(&self, exe: &str) -> Option<PathBuf> {
        let lists = |names: &[String]| names.iter().any(|n| same_program(n, exe));
        self.games
            .iter()
            .filter(|g| lists(&g.meta.processes))
            .find_map(Game::default_config)
            .or_else(|| self.unsorted.iter().find(|c| lists(&c.processes)))
            .map(|c| c.path.clone())
    }
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
            processes: p.processes,
        },
        Err(e) => ConfigEntry {
            name: file_stem(&path),
            path,
            error: Some(e.to_string()),
            processes: Vec::new(),
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
    path.file_name().is_some_and(|n| n != GAME_FILE) && !is_leftover(path) && is_profile_file(path)
}

/// A backup (`.bak`) or a temporary file (`.tmp`) the app left beside a file it saved.
fn is_leftover(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "bak" || e == "tmp")
}

/// Read a game folder's `game.toml` (not its configs); without a name the game is named after
/// its folder.
pub fn read_game_meta(dir: &Path) -> Result<GameMeta> {
    let meta_path = game_file(dir)?;
    let src = std::fs::read_to_string(&meta_path).map_err(io(&meta_path))?;
    let mut meta: GameMeta = GAME_FORMAT
        .parse(&src)
        .map_err(|e| LibraryError::Meta(meta_path.clone(), e))?;
    if meta.name.is_empty() {
        meta.name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Game")
            .to_owned();
    }
    Ok(meta)
}

/// Read one game folder.
pub fn read_game(dir: &Path) -> Result<Game> {
    let meta = read_game_meta(dir)?;
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

/// A config's name for the user: "Game › Config" in a game folder, else the config's own
/// name, which is the profile's name or, for a file that cannot be read, its file stem (as
/// in [`ConfigEntry::name`]). Reads the config and its game's `game.toml`, not the library.
pub fn config_label(path: &Path) -> String {
    let name = config_entry(path.to_path_buf()).name;
    match path.parent().and_then(|dir| read_game_meta(dir).ok()) {
        Some(meta) => format!("{} › {name}", meta.name),
        None => name,
    }
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
pub(crate) fn unique_path(dir: &Path, base: &str, ext: Option<&str>) -> PathBuf {
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
    crate::store::save_toml_with_header(&path, meta, &GAME_FORMAT, None).map_err(unsaved(&path))
}

/// Create a new game folder with one empty config named "Default".
pub fn create_game(root: &Path, name: &str) -> Result<Game> {
    create_game_with(root, name, &named("Default"))
}

/// A new game folder with metadata only (for imports that bring their own configs).
pub(crate) fn create_game_empty(root: &Path, name: &str) -> Result<PathBuf> {
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
    Ok(dir)
}

/// Replace a game's metadata.
pub fn update_game_meta(dir: &Path, meta: &GameMeta) -> Result<()> {
    game_file(dir)?;
    write_meta(dir, meta)
}

/// The metadata file of the game folder `dir`.
fn game_file(dir: &Path) -> Result<PathBuf> {
    let path = dir.join(GAME_FILE);
    if path.is_file() {
        Ok(path)
    } else {
        Err(LibraryError::NotAGame(dir.to_path_buf()))
    }
}

fn trash(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| LibraryError::Trash(path.to_path_buf(), e))
}

/// Move a game folder (with its configs) to the recycle bin.
pub fn delete_game(dir: &Path) -> Result<()> {
    game_file(dir)?;
    trash(dir)
}

/// Create an empty config in a game folder and return its path.
pub fn create_config(game_dir: &Path, name: &str) -> Result<PathBuf> {
    create_config_with(game_dir, &named(name))
}

/// An empty profile called `name`.
fn named(name: &str) -> Profile {
    Profile {
        name: name.to_owned(),
        ..Default::default()
    }
}

/// A new config file holding `profile`, named after it.
pub fn create_config_with(game_dir: &Path, profile: &Profile) -> Result<PathBuf> {
    let path = unique_path(game_dir, &slugify(&profile.name), Some("toml"));
    save_profile_atomic(&path, profile)?;
    Ok(path)
}

/// A new game whose first config is `profile`.
pub fn create_game_with(root: &Path, name: &str, profile: &Profile) -> Result<Game> {
    let dir = create_game_empty(root, name)?;
    create_config_with(&dir, profile)?;
    read_game(&dir)
}

/// A new game for one a store (Steam, Epic, GOG) installed: its name, the program that switches
/// to it, and its cover (a cover that cannot be copied leaves the game without a picture). A
/// game that cannot be set up completely goes again.
pub fn create_game_from_store(
    root: &Path,
    name: &str,
    program: Option<&str>,
    cover: Option<&Path>,
) -> Result<Game> {
    let game = create_game(root, name)?;
    let meta = GameMeta {
        processes: program.map(str::to_owned).into_iter().collect(),
        ..game.meta
    };
    let filled = update_game_meta(&game.dir, &meta).and_then(|()| {
        if let Some(cover) = cover.filter(|c| c.is_file())
            && let Err(e) = set_game_picture(&game.dir, Some(cover))
        {
            tracing::warn!("cannot copy the cover of {name}: {e}");
        }
        read_game(&game.dir)
    });
    if filled.is_err() {
        let _ = std::fs::remove_dir_all(&game.dir);
    }
    filled
}

/// Give a game a picture (copied into its folder as `cover.<ext>`), or (`None`) remove it. The
/// picture it had stays until the new one is in place.
pub fn set_game_picture(dir: &Path, source: Option<&Path>) -> Result<GameMeta> {
    let mut meta = read_game_meta(dir)?;
    let old = meta.image.take();
    if let Some(src) = source {
        let ext =
            picture::extension(src).ok_or_else(|| LibraryError::NotAPicture(src.to_path_buf()))?;
        let name = format!("cover.{ext}");
        copy_atomic(src, &dir.join(&name))?;
        meta.image = Some(name);
    }
    if let Err(e) = write_meta(dir, &meta) {
        // game.toml still names the old picture: a new one beside it goes again.
        if let Some(new) = meta.image.filter(|n| old.as_ref() != Some(n)) {
            let _ = std::fs::remove_file(dir.join(new));
        }
        return Err(e);
    }
    // An earlier cover in the folder goes, whatever its extension (a picture named by its own
    // path stays where it is).
    if let Some(old) = old.filter(|o| own_cover(o) && meta.image.as_ref() != Some(o)) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    Ok(meta)
}

/// Whether a game's `image` names a cover the game folder holds (`cover.<ext>`, no folders).
fn own_cover(image: &str) -> bool {
    image.starts_with("cover.") && !image.contains(['/', '\\'])
}

/// Copy `from` over `to` through a temporary file, so that `to` is either the old file or a whole
/// copy.
fn copy_atomic(from: &Path, to: &Path) -> Result<()> {
    let tmp = crate::store::with_suffix(to, "tmp");
    let copied = std::fs::copy(from, &tmp)
        .map_err(io(from))
        .and_then(|_| std::fs::rename(&tmp, to).map_err(io(to)));
    if copied.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    copied
}

/// A game's picture as a `data:` URL for the window, if it has one that is not too large.
pub fn game_picture(dir: &Path) -> Option<String> {
    let meta = read_game_meta(dir).ok()?;
    picture::data_url(&dir.join(meta.image?), picture::MAX_IMAGE)
}

/// Copy a game folder with its configs and picture under a new name ("<name> copy"), without
/// backups and temporary files. The copy gets no programs, so automatic switching keeps
/// choosing the original. A copy that fails halfway goes again.
pub fn duplicate_game(dir: &Path) -> Result<Game> {
    let meta = read_game_meta(dir)?;
    let name = format!("{} copy", meta.name);
    let root = dir.parent().unwrap_or(Path::new("."));
    let target = unique_path(root, &slugify(&name), None);
    std::fs::create_dir(&target).map_err(io(&target))?;
    let meta = GameMeta {
        name,
        processes: Vec::new(),
        ..meta
    };
    let copied = copy_game_files(dir, &target).and_then(|()| write_meta(&target, &meta));
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&target);
        return Err(e);
    }
    read_game(&target)
}

/// Copy the files of the game folder `from` into `to`, all but `game.toml`, backups and
/// temporary files.
fn copy_game_files(from: &Path, to: &Path) -> Result<()> {
    for entry in std::fs::read_dir(from).map_err(io(from))? {
        let entry = entry.map_err(io(from))?;
        let path = entry.path();
        if path.is_file() && entry.file_name() != GAME_FILE && !is_leftover(&path) {
            let target = to.join(entry.file_name());
            std::fs::copy(&path, &target).map_err(io(&path))?;
        }
    }
    Ok(())
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

/// Move a config file to the recycle bin.
pub fn delete_config(path: &Path) -> Result<()> {
    trash(path)
}

/// Save a profile as TOML: written to a temporary file and renamed over the target.
/// A file that was not written by Doppelpad (e.g. hand-written with comments, which
/// would be lost) is copied to `<file>.bak` once before its first overwrite. A JSON
/// profile is never overwritten; import it first.
pub fn save_profile_atomic(path: &Path, profile: &Profile) -> Result<()> {
    let text = to_toml(profile).map_err(|e| LibraryError::Serialize(path.to_path_buf(), e))?;
    if let Ok(existing) = std::fs::read_to_string(path) {
        if Format::detect(path, &existing) == Format::Json {
            return Err(LibraryError::ReadOnly(path.to_path_buf()));
        }
        if !existing.starts_with(crate::PROFILE_HEADER) {
            let bak = crate::store::with_suffix(path, "bak");
            if !bak.exists() {
                std::fs::copy(path, &bak).map_err(io(&bak))?;
            }
        }
    }
    crate::store::write_atomic(path, &text).map_err(io(path))
}

/// A JSON profile (schema 3.0) as [`import_into`] brings it in.
fn read_json(source: &Path) -> Result<(Profile, Diagnostics)> {
    let src = std::fs::read_to_string(source).map_err(io(source))?;
    let (mut profile, diags) =
        compat::import(&src).map_err(|e| LoadError::Import(source.to_path_buf(), e))?;
    if profile.name.is_empty() || profile.name == compat::DEFAULT_IMPORT_NAME {
        profile.name = file_stem(source);
    }
    Ok((profile, diags))
}

/// The launch commands of the JSON profile `source`, to ask about before [`import_into`].
pub fn import_launches(source: &Path) -> Result<Vec<Launch>> {
    Ok(launches::find(&read_json(source)?.0))
}

/// Import a JSON profile (schema 3.0) as a new TOML config in `game_dir`, without its launch
/// commands unless `keep_launches`. The source file is never modified or moved.
pub fn import_into(
    game_dir: &Path,
    source: &Path,
    keep_launches: bool,
) -> Result<(PathBuf, Diagnostics)> {
    let (mut profile, diags) = read_json(source)?;
    if !keep_launches {
        launches::strip(&mut profile);
    }
    let target = unique_path(game_dir, &slugify(&profile.name), Some("toml"));
    save_profile_atomic(&target, &profile)?;
    Ok((target, diags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_labels_name_the_game() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        assert_eq!(config_label(&game.configs[0].path), "Halo › Default");
        let broken = game.dir.join("broken.toml");
        std::fs::write(&broken, "[[mappings]]\ntrigger = \"Nope\"\n").unwrap();
        assert_eq!(config_label(&broken), "Halo › broken");
        // A game.toml without a name: the folder's name.
        let dir = root.path().join("unnamed");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join(GAME_FILE), "").unwrap();
        let config = create_config(&dir, "Fast").unwrap();
        assert_eq!(config_label(&config), "unnamed › Fast");
    }

    #[test]
    fn duplicate_game_copies_configs_and_picture_without_programs() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        create_config(&game.dir, "Sniper").unwrap();
        std::fs::write(game.dir.join("cover.png"), b"png").unwrap();
        let meta = GameMeta {
            processes: vec!["halo.exe".into()],
            image: Some("cover.png".into()),
            description: Some("Campaign".into()),
            ..game.meta.clone()
        };
        update_game_meta(&game.dir, &meta).unwrap();

        let copy = duplicate_game(&game.dir).unwrap();
        assert_ne!(copy.dir, game.dir);
        assert_eq!(copy.meta.name, "Halo copy");
        assert!(copy.meta.processes.is_empty());
        assert_eq!(copy.meta.image.as_deref(), Some("cover.png"));
        assert_eq!(copy.meta.description.as_deref(), Some("Campaign"));
        assert_eq!(copy.configs.len(), 2);
        assert!(copy.dir.join("cover.png").is_file());
        // A second copy gets a folder of its own.
        assert_ne!(duplicate_game(&game.dir).unwrap().dir, copy.dir);
    }

    #[test]
    fn duplicate_game_leaves_backups_and_temporary_files_behind() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        std::fs::write(game.dir.join("default.toml.bak"), "# hand-written").unwrap();
        std::fs::write(game.dir.join("default.toml.tmp"), "half").unwrap();
        let copy = duplicate_game(&game.dir).unwrap();
        assert!(copy.dir.join("default.toml").is_file());
        assert!(!copy.dir.join("default.toml.bak").exists());
        assert!(!copy.dir.join("default.toml.tmp").exists());
    }

    #[test]
    fn a_failed_duplicate_leaves_no_half_game() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        let sniper = create_config(&game.dir, "Sniper").unwrap();
        // A config no one else may read while it is open: copying it fails.
        #[cfg(windows)]
        let _held = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&sniper)
                .unwrap()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sniper, std::fs::Permissions::from_mode(0o000)).unwrap();
            if std::fs::read(&sniper).is_ok() {
                return; // root reads it anyway
            }
        }
        assert!(duplicate_game(&game.dir).is_err());
        let games: Vec<_> = std::fs::read_dir(root.path()).unwrap().flatten().collect();
        assert_eq!(games.len(), 1, "{games:?}");
    }

    #[test]
    fn game_pictures_are_copied_shown_and_removed() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        let src = root.path().join("art.PNG");
        std::fs::write(&src, b"\x89PNG").unwrap();
        let meta = set_game_picture(&game.dir, Some(&src)).unwrap();
        assert_eq!(meta.image.as_deref(), Some("cover.png"));
        assert!(game.dir.join("cover.png").is_file());
        let url = game_picture(&game.dir).unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "{url}");
        // Another picture replaces it; none removes it.
        let jpg = root.path().join("art.jpg");
        std::fs::write(&jpg, b"jpg").unwrap();
        set_game_picture(&game.dir, Some(&jpg)).unwrap();
        assert!(!game.dir.join("cover.png").exists());
        let meta = set_game_picture(&game.dir, None).unwrap();
        assert_eq!(meta.image, None);
        assert!(!game.dir.join("cover.jpg").exists());
        assert_eq!(game_picture(&game.dir), None);
        // Not a picture.
        let txt = root.path().join("notes.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(set_game_picture(&game.dir, Some(&txt)).is_err());
    }

    #[test]
    fn a_picture_that_cannot_be_used_keeps_the_one_there() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Halo").unwrap();
        let png = root.path().join("art.png");
        std::fs::write(&png, b"png").unwrap();
        set_game_picture(&game.dir, Some(&png)).unwrap();
        let txt = root.path().join("notes.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(matches!(
            set_game_picture(&game.dir, Some(&txt)),
            Err(LibraryError::NotAPicture(p)) if p == txt
        ));
        assert!(
            set_game_picture(&game.dir, Some(&root.path().join("gone.jpg"))).is_err(),
            "a copy that fails"
        );
        let meta = read_game_meta(&game.dir).unwrap();
        assert_eq!(meta.image.as_deref(), Some("cover.png"));
        assert_eq!(std::fs::read(game.dir.join("cover.png")).unwrap(), b"png");
        let names: Vec<_> = std::fs::read_dir(&game.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            names.len(),
            3,
            "game.toml, default.toml, cover.png: {names:?}"
        );
    }

    #[test]
    fn games_from_a_store_bring_their_program_and_cover() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("profiles");
        let cover = root.path().join("header.jpg");
        std::fs::write(&cover, b"jpg").unwrap();
        let g = create_game_from_store(&library, "Hades", Some("Hades.exe"), Some(&cover)).unwrap();
        assert_eq!(g.meta.name, "Hades");
        assert_eq!(g.meta.processes, ["Hades.exe"]);
        assert_eq!(g.meta.image.as_deref(), Some("cover.jpg"));
        assert_eq!(g.configs.len(), 1);
        // A cover that is not a picture, or none: a game without one.
        let odd = root.path().join("cover.txt");
        std::fs::write(&odd, b"x").unwrap();
        let g = create_game_from_store(&library, "Hades", None, Some(&odd)).unwrap();
        assert_eq!((g.meta.image, g.meta.processes.len()), (None, 0));
        let gone = root.path().join("gone.png");
        assert!(create_game_from_store(&library, "Celeste", None, Some(&gone)).is_ok());
    }

    #[test]
    fn config_labels_outside_a_game() {
        let root = tempfile::tempdir().unwrap();
        let loose = root.path().join("loose.toml");
        std::fs::write(&loose, "name = \"Loose\"\n").unwrap();
        assert_eq!(config_label(&loose), "Loose");
        assert_eq!(config_label(&root.path().join("gone.toml")), "gone");
    }

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
        let (path, _) = import_into(&game.dir, &src, false).unwrap();
        assert_eq!(std::fs::read_to_string(&src).unwrap(), JSON);
        assert_eq!(load(&path).unwrap().0.name, "Imported");
        assert!(path.extension().is_some_and(|e| e == "toml"));
    }

    /// [`JSON`] with a second mapping that runs a program.
    const LAUNCHING: &str = r#"{ "schemaVersion": 3.0, "config": { "appName": "Shared" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "commands": [ { "id": 7, "type": 1000, "name": "Run",
                      "params": { "path": "powershell.exe", "arguments": "-enc ZQBjAGgAbwA=" } } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1 } ] },
                 { "id": 2, "set": [ { "deviceId": 1, "buttonId": 2 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "keyboard": { "buttonId": 57, "description": "DIK_SPACE" } } ] },
                    { "condition": { "mask": { "id": 2, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "command": { "id": 7 } } ] } ] }"#;

    #[test]
    fn json_imports_drop_launches_unless_kept() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "Imp").unwrap();
        let src = root.path().join("shared.json");
        std::fs::write(&src, LAUNCHING).unwrap();
        let found = import_launches(&src).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].config, "Shared");
        assert_eq!(
            (found[0].path.as_str(), found[0].args.as_str()),
            ("powershell.exe", "-enc ZQBjAGgAbwA=")
        );

        let (dropped, _) = import_into(&game.dir, &src, false).unwrap();
        let p = load(&dropped).unwrap().0;
        assert!(launches::find(&p).is_empty());
        assert_eq!(p.mappings.len(), 1, "the space mapping stays");
        let (kept, _) = import_into(&game.dir, &src, true).unwrap();
        assert_eq!(launches::find(&load(&kept).unwrap().0).len(), 1);

        let plain = root.path().join("plain.json");
        std::fs::write(&plain, JSON).unwrap();
        assert!(import_launches(&plain).unwrap().is_empty());
    }

    #[test]
    fn duplicate_copies_under_a_new_name() {
        let root = tempfile::tempdir().unwrap();
        let game = create_game(root.path(), "G").unwrap();
        let path = game.configs[0].path.clone();
        let (mut profile, _) = load(&path).unwrap();
        profile.name = "Aggressive".into();
        save_profile_atomic(&path, &profile).unwrap();
        let copy = duplicate_config(&path).unwrap();
        assert_ne!(copy, path);
        assert_eq!(load(&copy).unwrap().0.name, "Aggressive copy");
    }

    #[test]
    fn imported_json_is_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("original.json");
        std::fs::write(&path, JSON).unwrap();
        let (profile, _) = load(&path).unwrap();
        let err = save_profile_atomic(&path, &profile).unwrap_err();
        let text = format!(
            "{}: JSON profiles are read-only; import them first",
            path.display()
        );
        assert_eq!(err.to_string(), text);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), JSON);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), JSON);
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "no .bak or .tmp"
        );
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
        assert_eq!(lib.config_for_process("shooter.EXE"), Some(second));
        assert!(lib.config_for_process("other.exe").is_none());
    }

    #[test]
    fn process_lookup_falls_back_to_loose_configs() {
        let root = tempfile::tempdir().unwrap();
        let loose = root.path().join("loose.toml");
        std::fs::write(&loose, "name = \"Loose\"\nprocesses = [\"Game.exe\"]\n").unwrap();
        std::fs::write(root.path().join("broken.toml"), "processes = 1\n").unwrap();
        let lib = scan(root.path());
        assert_eq!(lib.config_for_process("game.EXE"), Some(loose));
        assert!(lib.config_for_process("other.exe").is_none());
    }

    #[test]
    fn paths_inside_the_library() {
        let root = tempfile::tempdir().unwrap();
        let lib = root.path().join("profiles");
        assert!(contains(&lib, &lib.join("halo").join("default.toml")));
        assert!(contains(&lib, &lib.join("loose.toml")));
        assert!(!contains(&lib, &lib), "the library itself");
        assert!(!contains(
            &lib,
            &root.path().join("profiles-old").join("x.toml")
        ));
        assert!(!contains(&lib, &lib.join("..").join("elsewhere.toml")));
        assert!(!contains(&lib, &lib.join("halo").join("..").join("x.toml")));
        assert!(!contains(&lib, Path::new("C:/Windows/win.ini")));
    }

    #[test]
    fn programs_match_with_or_without_exe() {
        assert!(same_program("Hades.exe", "Hades"));
        assert!(same_program("hades", "HADES.EXE"));
        assert!(same_program("Game", "game"));
        assert!(!same_program("Game.exe", "Game2"));
        assert!(!same_program("exe", "Game.exe"));
        assert!(same_program("Ünïcode.exe", "Ünïcode"), "multi-byte names");
        assert!(!same_program("é", "Ée"));

        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("hades.toml"),
            "name = \"Hades\"\nprocesses = [\"Hades.exe\"]\n",
        )
        .unwrap();
        let lib = scan(root.path());
        assert!(
            lib.config_for_process("Hades").is_some(),
            "the native Mac or Linux program"
        );
    }
}
