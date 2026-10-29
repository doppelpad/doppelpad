//! Games, configs, layer presets and game bundles on disk, and where the library is.

use std::path::{Path, PathBuf};

use dp_config::Diagnostic;
use dp_config::bundle::{self, BundleLaunches, LayerPreset, PresetEntry};
use dp_config::history;
use dp_config::launches::Launch;
use dp_config::library::{self, Game, GameMeta, Library};
use dp_config::relocate;
use dp_config::templates::{self, Template};
use dp_core::profile::Profile;
use dp_service::ServiceCmd;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::{CmdError, CmdResult, ErrorCode, err};
use crate::state::{AppState, lock};

/// Games or configs changed on disk: the service re-reads them (process lists may have
/// changed), the tray relabels its slots and the mappings overlay shows a saved config anew.
fn library_changed(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.library_changed();
    let _ = state.call(ServiceCmd::Reload);
    crate::tray::refresh(app);
}

/// Refuse a path outside the library: the window only names games and configs in it, so
/// anything else came from somewhere it should not (the commands write, delete and read files).
fn in_library(app: &AppHandle, path: &Path) -> CmdResult<()> {
    if library::contains(&app.state::<AppState>().library_dir(), path) {
        Ok(())
    } else {
        Err(CmdError::with(ErrorCode::NotInLibrary, path.display()))
    }
}

/// Change the games and configs on disk; when it worked, the service and the tray are told.
fn library_change<T>(app: &AppHandle, change: impl FnOnce() -> CmdResult<T>) -> CmdResult<T> {
    let out = {
        let state = app.state::<AppState>();
        let _disk = lock(&state.disk);
        change()?
    };
    library_changed(app);
    Ok(out)
}

// ----- library -----------------------------------------------------------------------

#[tauri::command(async)]
pub fn library_scan(state: State<AppState>) -> Library {
    library::scan(&state.library_dir())
}

#[tauri::command(async)]
pub fn game_create(app: AppHandle, state: State<AppState>, name: String) -> CmdResult<Game> {
    library_change(&app, || {
        library::create_game(&state.library_dir(), &name).map_err(err)
    })
}

#[tauri::command(async)]
pub fn game_update(app: AppHandle, dir: PathBuf, meta: GameMeta) -> CmdResult<()> {
    in_library(&app, &dir)?;
    library_change(&app, || library::update_game_meta(&dir, &meta).map_err(err))
}

#[tauri::command(async)]
pub fn game_delete(app: AppHandle, dir: PathBuf) -> CmdResult<()> {
    in_library(&app, &dir)?;
    library_change(&app, || library::delete_game(&dir).map_err(err))
}

/// Copy a game with its configs and picture (the copy has no programs).
#[tauri::command(async)]
pub fn game_duplicate(app: AppHandle, dir: PathBuf) -> CmdResult<Game> {
    in_library(&app, &dir)?;
    library_change(&app, || library::duplicate_game(&dir).map_err(err))
}

/// Give a game a picture (copied into its folder), or (`None`) remove it.
#[tauri::command(async)]
pub fn game_set_picture(
    app: AppHandle,
    dir: PathBuf,
    source: Option<PathBuf>,
) -> CmdResult<GameMeta> {
    in_library(&app, &dir)?;
    library_change(&app, || {
        library::set_game_picture(&dir, source.as_deref()).map_err(err)
    })
}

/// A game's picture as a `data:` URL, if it has one.
#[tauri::command(async)]
pub fn game_picture(app: AppHandle, dir: PathBuf) -> Option<String> {
    in_library(&app, &dir).ok()?;
    library::game_picture(&dir)
}

/// A new game from a store's game: its name, the program that switches to it, and its cover.
#[tauri::command(async)]
pub fn game_create_from_store(
    app: AppHandle,
    state: State<AppState>,
    name: String,
    program: Option<String>,
    cover: Option<PathBuf>,
) -> CmdResult<Game> {
    library_change(&app, || {
        let (program, cover) = (program.as_deref(), cover.as_deref());
        library::create_game_from_store(&state.library_dir(), &name, program, cover).map_err(err)
    })
}

/// A new game whose first config (`config`) starts from a template.
#[tauri::command(async)]
pub fn game_create_template(
    app: AppHandle,
    state: State<AppState>,
    name: String,
    config: String,
    template: Template,
) -> CmdResult<Game> {
    let profile = templates::profile(template, &config);
    library_change(&app, || {
        library::create_game_with(&state.library_dir(), &name, &profile).map_err(err)
    })
}

/// A new config in a game, starting from a template.
#[tauri::command(async)]
pub fn config_from_template(
    app: AppHandle,
    game_dir: PathBuf,
    name: String,
    template: Template,
) -> CmdResult<PathBuf> {
    in_library(&app, &game_dir)?;
    let profile = templates::profile(template, &name);
    library_change(&app, || {
        library::create_config_with(&game_dir, &profile).map_err(err)
    })
}

/// Write a game and its configs as one bundle file, `out` (where the user chose).
#[tauri::command(async)]
pub fn game_export(app: AppHandle, dir: PathBuf, out: PathBuf) -> CmdResult<()> {
    in_library(&app, &dir)?;
    bundle::export_game(&dir, &out).map_err(err)
}

/// The launch commands in a game bundle's configs, and the configs that cannot be checked for
/// them, to ask about before importing it.
#[tauri::command(async)]
pub fn game_launches(file: PathBuf) -> CmdResult<BundleLaunches> {
    bundle::launches(&file).map_err(err)
}

/// Unpack a game bundle into a new game folder, without its launch commands unless
/// `keep_launches`.
#[tauri::command(async)]
pub fn game_import(
    app: AppHandle,
    state: State<AppState>,
    file: PathBuf,
    keep_launches: bool,
) -> CmdResult<Game> {
    library_change(&app, || {
        bundle::import_game(&state.library_dir(), &file, keep_launches).map_err(err)
    })
}

#[tauri::command(async)]
pub fn preset_list(state: State<AppState>) -> Vec<PresetEntry> {
    bundle::list_presets(&state.library_dir())
}

#[tauri::command(async)]
pub fn preset_save(state: State<AppState>, preset: LayerPreset) -> CmdResult<PathBuf> {
    let _disk = lock(&state.disk);
    bundle::save_preset(&state.library_dir(), &preset).map_err(err)
}

#[tauri::command(async)]
pub fn preset_load(app: AppHandle, path: PathBuf) -> CmdResult<LayerPreset> {
    in_library(&app, &path)?;
    bundle::load_preset(&path).map_err(err)
}

/// Save a bindings sheet (an HTML page the editor made) where the user chose.
#[tauri::command(async)]
pub fn sheet_save(path: PathBuf, html: String) -> CmdResult<()> {
    let html_file = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
    if !html_file {
        return Err(CmdError::new(ErrorCode::SheetNotHtml));
    }
    dp_config::write_atomic(&path, html).map_err(|e| err(format!("{}: {e}", path.display())))
}

#[derive(Serialize)]
pub struct LoadedConfig {
    profile: Profile,
    diagnostics: Vec<Diagnostic>,
    /// Imported JSON files are never written; edits must be saved as a new config.
    read_only: bool,
}

/// A config of the library, or an earlier version of one.
#[tauri::command(async)]
pub fn config_load(app: AppHandle, path: PathBuf) -> CmdResult<LoadedConfig> {
    if !library::contains(&dp_config::history_dir(), &path) {
        in_library(&app, &path)?;
    }
    let (profile, diags, format) = dp_config::load_with_format(&path).map_err(err)?;
    Ok(LoadedConfig {
        profile,
        diagnostics: diags.into_vec(),
        read_only: format == dp_config::Format::Json,
    })
}

/// The validation diagnostics of a profile being edited; nothing is written.
#[tauri::command(async)]
pub fn config_check(profile: Profile) -> Vec<Diagnostic> {
    dp_config::validate(&profile).into_vec()
}

/// Validate and save; returns the validation diagnostics (errors block saving). The file as
/// it was becomes an earlier version.
#[tauri::command(async)]
pub fn config_save(
    app: AppHandle,
    state: State<AppState>,
    path: PathBuf,
    profile: Profile,
) -> CmdResult<Vec<Diagnostic>> {
    in_library(&app, &path)?;
    let diags = dp_config::validate(&profile);
    if diags.has_errors() {
        return Ok(diags.into_vec());
    }
    let library = state.library_dir();
    library_change(&app, || {
        history::save_profile(&path, &profile, &dp_config::history_dir(), &library).map_err(err)
    })?;
    Ok(diags.into_vec())
}

/// The earlier versions kept of a config, the newest first.
#[tauri::command(async)]
pub fn config_versions(state: State<AppState>, path: PathBuf) -> Vec<history::Version> {
    history::versions(&path, &dp_config::history_dir(), &state.library_dir())
}

#[tauri::command(async)]
pub fn config_create(app: AppHandle, game_dir: PathBuf, name: String) -> CmdResult<PathBuf> {
    in_library(&app, &game_dir)?;
    library_change(&app, || {
        library::create_config(&game_dir, &name).map_err(err)
    })
}

#[tauri::command(async)]
pub fn config_duplicate(app: AppHandle, path: PathBuf) -> CmdResult<PathBuf> {
    in_library(&app, &path)?;
    library_change(&app, || library::duplicate_config(&path).map_err(err))
}

#[tauri::command(async)]
pub fn config_delete(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    in_library(&app, &path)?;
    library_change(&app, || library::delete_config(&path).map_err(err))
}

#[derive(Serialize)]
pub struct Imported {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
}

/// The launch commands in a JSON profile, to ask about before importing it.
#[tauri::command(async)]
pub fn config_import_launches(source: PathBuf) -> CmdResult<Vec<Launch>> {
    library::import_launches(&source).map_err(err)
}

/// Import a JSON profile as a new config, without its launch commands unless `keep_launches`.
#[tauri::command(async)]
pub fn config_import(
    app: AppHandle,
    game_dir: PathBuf,
    source: PathBuf,
    keep_launches: bool,
) -> CmdResult<Imported> {
    in_library(&app, &game_dir)?;
    let (path, diags) = library_change(&app, || {
        library::import_into(&game_dir, &source, keep_launches).map_err(err)
    })?;
    Ok(Imported {
        path,
        diagnostics: diags.into_vec(),
    })
}

/// Where the library is, and whether that is the default folder.
#[derive(Serialize)]
pub struct LibraryPlace {
    dir: PathBuf,
    default: PathBuf,
    is_default: bool,
    /// In portable mode, the folder beside the app that holds everything.
    portable: Option<PathBuf>,
}

#[tauri::command]
pub fn library_place(state: State<AppState>) -> LibraryPlace {
    let dir = state.library_dir();
    let default = dp_config::default_profiles_dir();
    LibraryPlace {
        is_default: relocate::same_dir(&dir, &default),
        dir,
        default,
        portable: dp_config::portable_dir().map(Path::to_path_buf),
    }
}

/// What a folder offered for the library holds (`empty`: the library moves there; `library`:
/// it is used as it is; `foreign`: refused), or why the library cannot go there.
#[tauri::command(async)]
pub fn library_target(state: State<AppState>, dir: PathBuf) -> CmdResult<relocate::Target> {
    relocate::check_move(
        &state.library_dir(),
        &dir,
        &dp_config::default_profiles_dir(),
    )
    .map_err(err)
}

/// Move the library to `dir` (missing or empty), or use the Doppelpad library already there;
/// `None` goes back to the default folder. Files are copied and checked, the new place is
/// remembered and the service told (slots follow the files), and only once it switched do the
/// old files go.
#[tauri::command(async)]
pub fn library_move(
    app: AppHandle,
    state: State<AppState>,
    dir: Option<PathBuf>,
) -> CmdResult<relocate::Moved> {
    let default = dp_config::default_profiles_dir();
    let to = dir.unwrap_or_else(|| default.clone());
    let disk = lock(&state.disk);
    let from = state.library_dir();
    let switch = |dir: &Path, moved_from: Option<&Path>| {
        let told = state.call(ServiceCmd::UseLibrary {
            dir: dir.to_path_buf(),
            moved_from: moved_from.map(Path::to_path_buf),
        });
        if let Err(e) = &told {
            tracing::warn!("the service did not switch the library: {}", e.describe());
        }
        told.is_ok()
    };
    let moved = relocate::move_library(&from, &to, &default, &dp_config::config_dir(), switch)
        .map_err(err)?;
    *lock(&state.profiles_dir) = to.clone();
    drop(disk);
    tracing::info!(from = %from.display(), to = %to.display(), "library moved");
    crate::tray::refresh(&app);
    Ok(moved)
}
