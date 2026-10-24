//! Finding the installed games of Steam and Epic on macOS (their files are read in
//! `crate::stores`). Both keep them in the user's Application Support folder; a Mac game is an
//! app bundle, and its program the executable inside.

use std::path::PathBuf;

use crate::stores::{self, InstalledGame};

/// Every installed game the stores know, sorted by name.
pub fn installed_games() -> Vec<InstalledGame> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let support = PathBuf::from(home).join("Library/Application Support");
    let mut out = stores::steam_games(&support.join("Steam"), stores::app_programs_in);
    out.extend(stores::epic_games(
        &support.join("Epic/EpicGamesLauncher/Data/Manifests"),
        stores::app_programs_in,
    ));
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}
