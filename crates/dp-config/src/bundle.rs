//! Sharing: a game with all its configs as one file, and layers kept as presets.
//!
//! A game bundle (`.dpgame`) is JSON: the game's metadata, every config file as text, and its
//! picture (base64) if it has one. Importing never overwrites: the game gets a folder of its
//! own. Layer presets are TOML files in the `presets` folder of the profiles directory.

use std::path::{Path, PathBuf};

use dp_core::profile::{AnalogMapping, Layer, Mapping};
use serde::{Deserialize, Serialize};

use crate::library::{self, Game, GameMeta, LibraryError, Result};

/// Format version of game bundles.
const BUNDLE_VERSION: u32 = 1;
/// Folder of the layer presets inside the profiles directory.
pub const PRESETS_DIR: &str = "presets";
/// Pictures larger than this stay out of a bundle.
const MAX_IMAGE: u64 = 8 << 20;

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

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0);
    for c in text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        let v = B64.iter().position(|&b| b == c)? as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
        }
    }
    Some(out)
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |e| LibraryError::Io(path.to_path_buf(), e)
}

fn invalid(path: &Path, why: impl Into<String>) -> LibraryError {
    LibraryError::Io(
        path.to_path_buf(),
        std::io::Error::new(std::io::ErrorKind::InvalidData, why.into()),
    )
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
    std::fs::write(out, text).map_err(io(out))
}

/// Unpack a bundle into a new game folder under `root`.
pub fn import_game(root: &Path, bundle: &Path) -> Result<Game> {
    let text = std::fs::read_to_string(bundle).map_err(io(bundle))?;
    let b: Bundle = serde_json::from_str(&text)
        .map_err(|e| invalid(bundle, format!("not a Doppelpad game: {e}")))?;
    if b.doppelpad_game > BUNDLE_VERSION {
        return Err(invalid(bundle, "made by a newer Doppelpad"));
    }
    let created = library::create_game_empty(root, &b.meta.name)?;
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
    library::update_game_meta(&created, &meta)?;
    library::read_game(&created)
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
        .filter_map(|path| {
            let p = load_preset(&path).ok()?;
            Some(PresetEntry { name: p.name, path })
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

pub fn load_preset(path: &Path) -> Result<LayerPreset> {
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    toml::from_str(&text).map_err(|e| invalid(path, e.to_string()))
}

/// Store a preset under its name; a preset of the same name is replaced.
pub fn save_preset(profiles: &Path, preset: &LayerPreset) -> Result<PathBuf> {
    let dir = profiles.join(PRESETS_DIR);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let path = dir.join(format!("{}.toml", library::slugify(&preset.name)));
    let text = toml::to_string_pretty(preset).map_err(|e| invalid(&path, e.to_string()))?;
    std::fs::write(&path, format!("# Doppelpad layer preset\n{text}")).map_err(io(&path))?;
    Ok(path)
}

pub fn delete_preset(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(io(path))
}

#[cfg(test)]
mod tests {
    use dp_core::control::Control;
    use dp_core::keys::Key;
    use dp_core::pad::PadButton;
    use dp_core::profile::{Action, Press};

    use super::*;

    #[test]
    fn base64_round_trips() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(unbase64(&base64(&bytes)).unwrap(), bytes);
        assert_eq!(unbase64("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(unbase64("T*"), None);
    }

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
        let a = import_game(dst.path(), &file).unwrap();
        let b = import_game(dst.path(), &file).unwrap();
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
        delete_preset(&path).unwrap();
        assert!(list_presets(dir.path()).is_empty());
    }
}
