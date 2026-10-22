//! Finding the installed games of Steam, Epic and GOG on Windows (their files are parsed in
//! `crate::stores`). Only local files and the registry are read.

use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use super::registry;
use crate::stores::{self, InstalledGame, Store};

/// Every installed game the three stores know, sorted by name.
pub fn installed_games() -> Vec<InstalledGame> {
    let mut out = steam();
    out.extend(epic());
    out.extend(gog());
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

fn steam() -> Vec<InstalledGame> {
    let Some(root) = registry::get_path(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
    else {
        return Vec::new();
    };
    let root = PathBuf::from(root.replace('/', "\\"));
    let folders =
        std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")).unwrap_or_default();
    let mut libraries = stores::steam_libraries(&folders);
    if libraries.is_empty() {
        libraries.push(root.clone());
    }
    let cache = root.join(r"appcache\librarycache");
    let mut out = Vec::new();
    for library in libraries {
        let apps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Some((id, name, dir)) = std::fs::read_to_string(e.path())
                .ok()
                .as_deref()
                .and_then(stores::steam_app)
            else {
                continue;
            };
            let dir = apps.join("common").join(dir);
            if !dir.is_dir() {
                continue;
            }
            out.push(InstalledGame {
                store: Store::Steam,
                name,
                programs: stores::programs_in(&dir),
                cover: stores::steam_cover(&cache, &id),
                dir,
            });
        }
    }
    out
}

fn epic() -> Vec<InstalledGame> {
    let Some(data) = std::env::var_os("ProgramData") else {
        return Vec::new();
    };
    let manifests = Path::new(&data).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = std::fs::read_dir(manifests) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| stores::epic_item(&text))
        .filter(|(_, dir, _)| dir.is_dir())
        .map(|(name, dir, exe)| InstalledGame {
            store: Store::Epic,
            programs: stores::programs_with(Some(exe), &dir),
            name,
            dir,
            cover: None,
        })
        .collect()
}

fn gog() -> Vec<InstalledGame> {
    const GAMES: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";
    registry::subkeys(HKEY_LOCAL_MACHINE, GAMES)
        .into_iter()
        .filter_map(|id| {
            let key = format!(r"{GAMES}\{id}");
            let name = registry::get_path(HKEY_LOCAL_MACHINE, &key, "gameName")?;
            let dir = PathBuf::from(registry::get_path(HKEY_LOCAL_MACHINE, &key, "path")?);
            let exe = registry::get_path(HKEY_LOCAL_MACHINE, &key, "exe").and_then(|e| {
                Path::new(&e)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            });
            let programs = stores::programs_with(exe, &dir);
            dir.is_dir().then_some(InstalledGame {
                store: Store::Gog,
                name,
                dir,
                programs,
                cover: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// Lists this PC's games; run by hand: `cargo test -p dp-platform installed_games -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the local game libraries"]
    fn installed_games_on_this_pc() {
        for g in super::installed_games() {
            println!(
                "{:?} {} | {} | {:?} | {:?}",
                g.store,
                g.name,
                g.dir.display(),
                g.programs.first(),
                g.cover
            );
        }
    }
}
