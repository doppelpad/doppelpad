//! Sharing: a game with all its configs as one file, and layers kept as presets.
//!
//! A game bundle (`.dpgame`) is JSON: the game's metadata, every config file as text, and its
//! picture (base64) if it has one. Importing never overwrites: the game gets a folder of its
//! own. It leaves out launch commands unless asked to keep them (see [`crate::launches`]). Layer
//! presets are TOML files in the `presets` folder of the profiles directory.

use std::path::{Path, PathBuf};

use dp_core::profile::{AnalogMapping, Layer, Mapping, Profile};
use serde::{Deserialize, Serialize};

use crate::launches::{self, Launch};
use crate::library::{self, Game, GameMeta, LibraryError, Result, io};
use crate::picture::{MAX_IMAGE, base64, unbase64};
use crate::{FileFormat, Format};

/// Format version of game bundles.
const BUNDLE_VERSION: u32 = 1;
/// Folder of the layer presets inside the profiles directory.
pub(crate) const PRESETS_DIR: &str = "presets";

#[derive(Serialize, Deserialize)]
struct Bundle {
    doppelpad_game: u32,
    meta: GameMeta,
    configs: Vec<BundledFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image: Option<BundledFile>,
}

#[derive(Serialize, Deserialize)]
struct BundledFile {
    file: String,
    /// Text files as they are; the picture as base64.
    data: String,
}

fn invalid(path: &Path, why: impl Into<String>) -> LibraryError {
    LibraryError::Bundle(path.to_path_buf(), why.into())
}

/// A plain file name from a bundle: no folders, no surprises.
fn safe_name(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && name != library::GAME_FILE;
    ok.then_some(name)
}

/// Write a game folder as one bundle file.
pub fn export_game(dir: &Path, out: &Path) -> Result<()> {
    let game = library::read_game(dir)?;
    let mut configs = Vec::new();
    for c in &game.configs {
        let data = std::fs::read_to_string(&c.path).map_err(io(&c.path))?;
        let file = c
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml")
            .to_owned();
        configs.push(BundledFile { file, data });
    }
    let mut meta = game.meta.clone();
    let image = game.meta.image.as_deref().and_then(|img| {
        let path = dir.join(img);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_IMAGE);
        let bytes = small.then(|| std::fs::read(&path).ok()).flatten()?;
        let file = format!(
            "cover.{}",
            path.extension().and_then(|e| e.to_str()).unwrap_or("png")
        );
        Some(BundledFile {
            file,
            data: base64(&bytes),
        })
    });
    meta.image = image.as_ref().map(|i| i.file.clone());
    let bundle = Bundle {
        doppelpad_game: BUNDLE_VERSION,
        meta,
        configs,
        image,
    };
    let text = serde_json::to_string_pretty(&bundle).map_err(|e| invalid(out, e.to_string()))?;
    crate::store::write_atomic(out, text).map_err(io(out))
}

/// Read a bundle file. Its version is read first, so that a bundle from a newer Doppelpad,
/// whose layout this one may not know, says so.
fn read_bundle(bundle: &Path) -> Result<Bundle> {
    #[derive(Deserialize)]
    struct Head {
        doppelpad_game: u32,
    }
    let text = std::fs::read_to_string(bundle).map_err(io(bundle))?;
    let not_a_game = |e: serde_json::Error| invalid(bundle, format!("not a Doppelpad game: {e}"));
    let head: Head = serde_json::from_str(&text).map_err(not_a_game)?;
    if head.doppelpad_game > BUNDLE_VERSION {
        return Err(invalid(bundle, "made by a newer Doppelpad"));
    }
    serde_json::from_str(&text).map_err(not_a_game)
}

impl BundledFile {
    /// The config as the library will read it once unpacked, or why it cannot be read (it then
    /// runs nothing in this version, but may in a newer one).
    fn profile(&self) -> std::result::Result<(Profile, Format), String> {
        let (profile, _, format) = crate::parse_with_format(Path::new(&self.file), &self.data)
            .map_err(|e| e.to_string())?;
        Ok((profile, format))
    }
}

/// A config in a bundle that cannot be read, so its launch commands cannot be checked either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Unchecked {
    pub file: String,
    /// Why it cannot be read (for example, a newer Doppelpad made it).
    pub error: String,
}

/// What a bundle's configs would start: the launch commands of those that can be read, and
/// those that cannot be checked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BundleLaunches {
    pub launches: Vec<Launch>,
    /// Left out of an import without launch commands.
    pub unchecked: Vec<Unchecked>,
}

/// Every launch command in a bundle's configs, and the configs that cannot be checked for
/// them, to ask about before [`import_game`].
pub fn launches(bundle: &Path) -> Result<BundleLaunches> {
    let b = read_bundle(bundle)?;
    let mut out = BundleLaunches::default();
    for c in b.configs.iter().filter(|c| safe_name(&c.file).is_some()) {
        match c.profile() {
            Ok((profile, _)) => {
                out.launches
                    .extend(launches::find(&profile).into_iter().map(|l| Launch {
                        file: Some(c.file.clone()),
                        ..l
                    }));
            }
            Err(error) => out.unchecked.push(Unchecked {
                file: c.file.clone(),
                error,
            }),
        }
    }
    Ok(out)
}

/// Unpack a bundle into a new game folder under `root`. Unless `keep_launches`, it comes
/// without its launch commands and without the configs that cannot be checked for them (see
/// [`launches`]); with it, every config comes as it is.
pub fn import_game(root: &Path, bundle: &Path, keep_launches: bool) -> Result<Game> {
    let mut b = read_bundle(bundle)?;
    if !keep_launches {
        drop_launches(&mut b)?;
    }
    let created = library::create_game_empty(root, &b.meta.name)?;
    let filled = fill_game(&created, b);
    if filled.is_err() {
        // No half-written game: the folder goes again.
        let _ = std::fs::remove_dir_all(&created);
    }
    filled
}

/// Write a bundle's configs, picture and metadata into the new game folder `created`.
fn fill_game(created: &Path, b: Bundle) -> Result<Game> {
    let mut meta = b.meta;
    meta.image = None;
    for c in &b.configs {
        let Some(name) = safe_name(&c.file) else {
            tracing::warn!("skipping bundled file {:?}", c.file);
            continue;
        };
        let path = created.join(name);
        std::fs::write(&path, &c.data).map_err(io(&path))?;
    }
    if let Some(img) = &b.image
        && let (Some(name), Some(bytes)) = (safe_name(&img.file), unbase64(&img.data))
    {
        let path = created.join(name);
        std::fs::write(&path, bytes).map_err(io(&path))?;
        meta.image = Some(name.to_owned());
    }
    // The default config must be one that arrived.
    if meta
        .default_config
        .as_deref()
        .is_some_and(|d| !created.join(d).is_file())
    {
        meta.default_config = None;
    }
    library::update_game_meta(created, &meta)?;
    library::read_game(created)
}

/// Take the launch commands out of a bundle's configs. A config that has some is written
/// again as Doppelpad TOML, a JSON one under a `.toml` name (which the default config
/// follows); the others stay exactly as they came. A config that cannot be read is left out:
/// a newer Doppelpad might read it, and run what it starts.
fn drop_launches(b: &mut Bundle) -> Result<()> {
    let mut unchecked = Vec::new();
    for i in 0..b.configs.len() {
        let file = b.configs[i].file.clone();
        if safe_name(&file).is_none() {
            continue;
        }
        let (mut profile, format) = match b.configs[i].profile() {
            Ok(read) => read,
            Err(e) => {
                tracing::warn!("leaving out {file}, which cannot be checked: {e}");
                unchecked.push(i);
                continue;
            }
        };
        if launches::strip(&mut profile) == 0 {
            continue;
        }
        let text = crate::to_toml(&profile)
            .map_err(|e| LibraryError::Serialize(PathBuf::from(&file), e))?;
        if format == Format::Json {
            let stem = Path::new(&file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("config");
            let taken = |name: &str| b.configs.iter().any(|c| c.file.eq_ignore_ascii_case(name));
            let renamed = std::iter::once(format!("{stem}.toml"))
                .chain((2..).map(|n| format!("{stem}-{n}.toml")))
                .find(|f| safe_name(f).is_some() && !taken(f))
                .expect("unbounded");
            if b.meta.default_config.as_deref() == Some(file.as_str()) {
                b.meta.default_config = Some(renamed.clone());
            }
            b.configs[i].file = renamed;
        }
        b.configs[i].data = text;
    }
    for i in unchecked.into_iter().rev() {
        b.configs.remove(i);
    }
    Ok(())
}

/// One layer to reuse in other configs: its settings, mappings and routes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerPreset {
    pub name: String,
    /// The layer's own settings (inheritance, sticks, ...); none for a preset made from Main.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<Mapping>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub analog: Vec<AnalogMapping>,
}

/// A stored preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PresetEntry {
    pub name: String,
    pub path: PathBuf,
}

/// The presets in `profiles/presets`, by name.
pub fn list_presets(profiles: &Path) -> Vec<PresetEntry> {
    let dir = profiles.join(PRESETS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PresetEntry> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .filter_map(|path| match load_preset(&path) {
            Ok(p) => Some(PresetEntry { name: p.name, path }),
            Err(e) => {
                tracing::warn!("ignoring preset: {e}");
                None
            }
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// The format of layer presets; see [`crate::PROFILE_FORMAT`].
const PRESET_FORMAT: FileFormat = FileFormat::new(&[]);

pub fn load_preset(path: &Path) -> Result<LayerPreset> {
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    PRESET_FORMAT
        .parse(&text)
        .map_err(|e| LibraryError::Preset(path.to_path_buf(), e))
}

/// Store a preset under its name; a preset of the same name is replaced, and one whose name
/// only makes the same file name is not.
pub fn save_preset(profiles: &Path, preset: &LayerPreset) -> Result<PathBuf> {
    let dir = profiles.join(PRESETS_DIR);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let path = match list_presets(profiles)
        .into_iter()
        .find(|p| p.name == preset.name)
    {
        Some(same) => same.path,
        None => library::unique_path(&dir, &library::slugify(&preset.name), Some("toml")),
    };
    crate::store::save_toml_with_header(
        &path,
        preset,
        &PRESET_FORMAT,
        Some("# Doppelpad layer preset"),
    )
    .map_err(library::unsaved(&path))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use dp_core::control::Control;
    use dp_core::keys::Key;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Action, Press};

    use super::*;

    #[test]
    fn games_travel_as_one_file() {
        let src = tempfile::tempdir().unwrap();
        let game = library::create_game(src.path(), "Space Game").unwrap();
        std::fs::write(game.dir.join("cover.png"), [1u8, 2, 3, 255]).unwrap();
        let mut meta = game.meta.clone();
        meta.image = Some("cover.png".into());
        meta.processes = vec!["space.exe".into()];
        library::update_game_meta(&game.dir, &meta).unwrap();
        library::create_config(&game.dir, "Racing").unwrap();
        let file = src.path().join("space.dpgame");
        export_game(&game.dir, &file).unwrap();

        let dst = tempfile::tempdir().unwrap();
        let a = import_game(dst.path(), &file, false).unwrap();
        let b = import_game(dst.path(), &file, false).unwrap();
        assert_ne!(a.dir, b.dir, "a second import gets its own folder");
        assert_eq!(a.meta.name, "Space Game");
        assert_eq!(a.meta.processes, ["space.exe"]);
        assert_eq!(a.configs.len(), 2);
        assert!(a.configs.iter().all(|c| c.error.is_none()));
        assert_eq!(
            std::fs::read(a.dir.join("cover.png")).unwrap(),
            [1, 2, 3, 255]
        );
    }

    #[test]
    fn bundles_cannot_write_outside_their_folder() {
        assert_eq!(safe_name("default.toml"), Some("default.toml"));
        for bad in ["../evil.toml", r"..\evil.toml", "C:evil", "game.toml", ""] {
            assert_eq!(safe_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn presets_save_list_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let preset = LayerPreset {
            name: "Driving".into(),
            layer: Some(Layer {
                id: 0,
                name: "Driving".into(),
                inherit: false,
                ..Default::default()
            }),
            mappings: vec![Mapping::new(
                Control::Button(PadButton::A),
                vec![Action::Key {
                    key: Key::W,
                    action: Press::Auto,
                }],
            )],
            analog: vec![],
        };
        let path = save_preset(dir.path(), &preset).unwrap();
        assert_eq!(
            list_presets(dir.path()),
            [PresetEntry {
                name: "Driving".into(),
                path: path.clone()
            }]
        );
        assert_eq!(load_preset(&path).unwrap(), preset);
        assert!(
            library::scan(dir.path()).errors.is_empty(),
            "not mistaken for a game"
        );
        std::fs::remove_file(&path).unwrap();
        assert!(list_presets(dir.path()).is_empty());
    }

    #[test]
    fn presets_with_alike_names_keep_their_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let named = |name: &str| LayerPreset {
            name: name.into(),
            ..Default::default()
        };
        let a = save_preset(dir.path(), &named("Aim!")).unwrap();
        let b = save_preset(dir.path(), &named("Aim?")).unwrap();
        assert_ne!(a, b, "same slug, different presets");
        assert_eq!(
            save_preset(dir.path(), &named("Aim?")).unwrap(),
            b,
            "replaced"
        );
        let names: Vec<String> = list_presets(dir.path())
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["Aim!", "Aim?"]);
    }

    #[test]
    fn a_failed_import_leaves_no_half_game() {
        let root = tempfile::tempdir().unwrap();
        let bundle = Bundle {
            doppelpad_game: BUNDLE_VERSION,
            meta: library::GameMeta {
                name: "Broken".into(),
                ..Default::default()
            },
            configs: vec![
                BundledFile {
                    file: "ok.toml".into(),
                    data: "name = \"Ok\"\n".into(),
                },
                // A name no file system takes: writing it fails halfway through.
                BundledFile {
                    file: format!("{}.toml", "x".repeat(300)),
                    data: String::new(),
                },
            ],
            image: None,
        };
        let file = root.path().join("broken.dpgame");
        std::fs::write(&file, serde_json::to_string(&bundle).unwrap()).unwrap();
        let games = root.path().join("profiles");
        assert!(import_game(&games, &file, false).is_err());
        let left: Vec<_> = std::fs::read_dir(&games).unwrap().flatten().collect();
        assert!(left.is_empty(), "{left:?}");
    }

    const LAUNCHING_TOML: &str = r#"# shared by a stranger
name = "Shared"

[[mappings]]
trigger = "A"
actions = [{ command = { launch = { path = "powershell", args = "-enc ZQBjAGgAbwA=" } } }]

[[mappings]]
trigger = "B"
actions = [{ key = "W" }]
"#;

    const LAUNCHING_JSON: &str = r#"{ "schemaVersion": 3.0, "config": { "appName": "Json" },
      "devices": { "hardware": [ { "id": 1, "name": "gamepad" } ] },
      "commands": [ { "id": 7, "type": 1000, "params": { "path": "calc.exe" } } ],
      "masks": [ { "id": 1, "set": [ { "deviceId": 1, "buttonId": 1 } ] } ],
      "mappings": [ { "condition": { "mask": { "id": 1, "activator": { "type": "single", "mode": "hold_until_release" } } },
                      "macros": [ { "command": { "id": 7 } } ] } ] }"#;

    const PLAIN_TOML: &str = "# my notes\nname = \"Plain\"\n";

    /// A bundle with a TOML and a JSON config that launch programs, and one that does not.
    fn launching_bundle(dir: &Path) -> PathBuf {
        let config = |file: &str, data: &str| BundledFile {
            file: file.into(),
            data: data.into(),
        };
        let bundle = Bundle {
            doppelpad_game: BUNDLE_VERSION,
            meta: library::GameMeta {
                name: "Shared".into(),
                default_config: Some("shared.json".into()),
                ..Default::default()
            },
            configs: vec![
                config("shared.toml", LAUNCHING_TOML),
                config("shared.json", LAUNCHING_JSON),
                config("plain.toml", PLAIN_TOML),
            ],
            image: None,
        };
        let file = dir.join("shared.dpgame");
        std::fs::write(&file, serde_json::to_string(&bundle).unwrap()).unwrap();
        file
    }

    /// Every launch command in an unpacked game.
    fn game_launches(g: &Game) -> Vec<Launch> {
        g.configs
            .iter()
            .flat_map(|c| launches::find(&crate::load(&c.path).unwrap().0))
            .collect()
    }

    #[test]
    fn bundles_list_their_launches() {
        let dir = tempfile::tempdir().unwrap();
        let found = launches(&launching_bundle(dir.path())).unwrap();
        assert!(found.unchecked.is_empty());
        let summary: Vec<(Option<&str>, &str, &str, &str)> = found
            .launches
            .iter()
            .map(|l| {
                let file = l.file.as_deref();
                (file, l.config.as_str(), l.path.as_str(), l.args.as_str())
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    Some("shared.toml"),
                    "Shared",
                    "powershell",
                    "-enc ZQBjAGgAbwA="
                ),
                (Some("shared.json"), "Json", "calc.exe", ""),
            ]
        );
    }

    /// A config from a newer Doppelpad that this one cannot read: it may launch a program once
    /// an update reads it.
    const NEWER_TOML: &str = r#"format = 2
name = "Future"

[[mappings]]
trigger = "A"
actions = [{ command = { launch = { path = "powershell" } } }]
"#;

    #[test]
    fn configs_that_cannot_be_checked_are_named_and_left_out() {
        let src = tempfile::tempdir().unwrap();
        let bundle = Bundle {
            doppelpad_game: BUNDLE_VERSION,
            meta: library::GameMeta {
                name: "Future".into(),
                default_config: Some("future.toml".into()),
                ..Default::default()
            },
            configs: vec![
                BundledFile {
                    file: "future.toml".into(),
                    data: NEWER_TOML.into(),
                },
                BundledFile {
                    file: "plain.toml".into(),
                    data: PLAIN_TOML.into(),
                },
            ],
            image: None,
        };
        let file = src.path().join("future.dpgame");
        std::fs::write(&file, serde_json::to_string(&bundle).unwrap()).unwrap();

        let found = launches(&file).unwrap();
        assert!(found.launches.is_empty());
        let [unchecked] = &found.unchecked[..] else {
            panic!("{found:?}")
        };
        assert_eq!(unchecked.file, "future.toml");
        assert!(
            unchecked.error.contains("made by a newer Doppelpad"),
            "{}",
            unchecked.error
        );

        let dst = tempfile::tempdir().unwrap();
        let g = import_game(dst.path(), &file, false).unwrap();
        let files: Vec<_> = g
            .configs
            .iter()
            .map(|c| c.path.file_name().unwrap())
            .collect();
        assert_eq!(files, ["plain.toml"]);
        assert_eq!(g.meta.default_config, None, "it did not arrive");
        // Kept on request, as it came.
        let kept = import_game(dst.path(), &file, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(kept.dir.join("future.toml")).unwrap(),
            NEWER_TOML
        );
    }

    #[test]
    fn bundles_from_a_newer_doppelpad_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("next.dpgame");
        // A layout this version does not know.
        std::fs::write(&file, r#"{ "doppelpad_game": 2, "games": [] }"#).unwrap();
        let e = launches(&file).unwrap_err().to_string();
        assert!(e.ends_with("made by a newer Doppelpad"), "{e}");
        std::fs::write(&file, r#"{ "name": "x" }"#).unwrap();
        let e = launches(&file).unwrap_err().to_string();
        assert!(e.contains("not a Doppelpad game"), "{e}");
    }

    #[test]
    fn imports_drop_launches_unless_kept() {
        let src = tempfile::tempdir().unwrap();
        let file = launching_bundle(src.path());
        let dst = tempfile::tempdir().unwrap();

        let kept = import_game(dst.path(), &file, true).unwrap();
        assert_eq!(game_launches(&kept).len(), 2);
        let read = |g: &Game, f: &str| std::fs::read_to_string(g.dir.join(f)).unwrap();
        assert_eq!(
            read(&kept, "shared.toml"),
            LAUNCHING_TOML,
            "kept as it came"
        );

        let g = import_game(dst.path(), &file, false).unwrap();
        assert!(game_launches(&g).is_empty());
        assert!(g.configs.iter().all(|c| c.error.is_none()));
        let files: Vec<String> = g
            .configs
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["plain.toml", "shared-2.toml", "shared.toml"]);
        assert_eq!(
            g.meta.default_config.as_deref(),
            Some("shared-2.toml"),
            "follows the JSON config's new name"
        );
        assert_eq!(read(&g, "plain.toml"), PLAIN_TOML, "untouched");
        let shared = crate::load(&g.dir.join("shared.toml")).unwrap().0;
        assert_eq!(shared.name, "Shared");
        assert_eq!(shared.mappings.len(), 1, "B stays, A only launched");
    }
}
