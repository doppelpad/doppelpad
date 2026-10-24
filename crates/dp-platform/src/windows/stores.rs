//! Finding the installed games of Steam, Epic and GOG on Windows (their files are read in
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
    match registry::get_path(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath") {
        Some(root) => {
            stores::steam_games(&PathBuf::from(root.replace('/', "\\")), stores::programs_in)
        }
        None => Vec::new(),
    }
}

fn epic() -> Vec<InstalledGame> {
    match std::env::var_os("ProgramData") {
        Some(data) => stores::epic_games(
            &Path::new(&data).join(r"Epic\EpicGamesLauncher\Data\Manifests"),
            stores::programs_in,
        ),
        None => Vec::new(),
    }
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
