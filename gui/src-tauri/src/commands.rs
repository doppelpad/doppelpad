//! Commands the web frontend invokes. Every command returns `Result<_, String>` so
//! errors arrive in JavaScript as readable messages.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_config::library::{self, Game, GameMeta, Library};
use dp_config::{Diagnostics, Severity};
use dp_core::control::{Control, mouse_button_name, pad_button_name};
use dp_core::keys::{Key, MouseButton};
use dp_core::pad::PadButton;
use dp_core::profile::{
    AnalogMapping, Mapping, Output, Profile, StickConfig, Timing, TriggerConfig,
};
use dp_ipc::Request;
use dp_service::{LiveState, ServiceCmd, ServiceHandle};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::settings::GuiSettings;

pub struct AppState {
    pub service: Mutex<ServiceHandle>,
    pub settings: Mutex<GuiSettings>,
    pub profiles_dir: PathBuf,
    /// Messages for the UI collected before it was ready (e.g. a service takeover).
    pub notices: Mutex<Vec<String>>,
    /// Incremented per live subscription; older streaming threads stop.
    pub live_generation: Arc<AtomicU64>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl AppState {
    pub fn call(&self, cmd: ServiceCmd) -> CmdResult<String> {
        self.service.lock().unwrap().call(cmd).into_result()
    }
}

/// Games or configs changed on disk: the service re-reads them (process lists may have
/// changed) and the tray relabels its slots.
fn library_changed(app: &AppHandle) {
    let _ = app
        .state::<AppState>()
        .call(ServiceCmd::Ipc(Request::Reload));
    crate::tray::refresh(app);
}

// ----- catalog -----------------------------------------------------------------------

#[derive(Serialize)]
pub struct Defaults {
    profile: Profile,
    mapping: Mapping,
    output: Output,
    timing: Timing,
    stick: StickConfig,
    trigger: TriggerConfig,
    analog: AnalogMapping,
}

#[derive(Serialize)]
pub struct Catalog {
    /// Keyboard key names (profile syntax).
    keys: Vec<&'static str>,
    /// Gamepad buttons in bit order of `PadState.buttons`.
    pad_buttons: Vec<&'static str>,
    mouse_buttons: Vec<&'static str>,
    defaults: Defaults,
}

/// Names and default values, so the frontend never duplicates them.
#[tauri::command]
pub fn catalog() -> Catalog {
    Catalog {
        keys: Key::ALL.iter().map(|k| k.name()).collect(),
        pad_buttons: PadButton::ALL.iter().map(|b| pad_button_name(*b)).collect(),
        mouse_buttons: MouseButton::ALL
            .iter()
            .map(|b| mouse_button_name(*b))
            .collect(),
        defaults: Defaults {
            profile: Profile::default(),
            mapping: Mapping::new(Control::Button(PadButton::A), Vec::new()),
            output: Output::default(),
            timing: Timing::default(),
            stick: StickConfig::default(),
            trigger: TriggerConfig::default(),
            analog: AnalogMapping::default(),
        },
    }
}

// ----- library -----------------------------------------------------------------------

#[tauri::command]
pub fn library_scan(state: State<AppState>) -> Library {
    library::scan(&state.profiles_dir)
}

#[tauri::command]
pub fn game_create(app: AppHandle, state: State<AppState>, name: String) -> CmdResult<Game> {
    let g = library::create_game(&state.profiles_dir, &name).map_err(err)?;
    library_changed(&app);
    Ok(g)
}

#[tauri::command]
pub fn game_update(app: AppHandle, dir: PathBuf, meta: GameMeta) -> CmdResult<()> {
    library::update_game_meta(&dir, &meta).map_err(err)?;
    library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn game_delete(app: AppHandle, dir: PathBuf) -> CmdResult<()> {
    library::delete_game(&dir).map_err(err)?;
    library_changed(&app);
    Ok(())
}

#[derive(Serialize)]
pub struct DiagMsg {
    severity: &'static str,
    path: String,
    message: String,
}

fn diag_msgs(d: &Diagnostics) -> Vec<DiagMsg> {
    d.iter()
        .map(|d| DiagMsg {
            severity: match d.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            path: d.path.clone(),
            message: d.message.clone(),
        })
        .collect()
}

#[derive(Serialize)]
pub struct LoadedConfig {
    profile: Profile,
    diagnostics: Vec<DiagMsg>,
    /// Imported JSON files are never written; edits must be saved as a new config.
    read_only: bool,
}

#[tauri::command]
pub fn config_load(path: PathBuf) -> CmdResult<LoadedConfig> {
    let (profile, diags) = dp_config::load(&path).map_err(err)?;
    let src = std::fs::read_to_string(&path).map_err(err)?;
    let read_only = dp_config::Format::detect(&path, &src) == dp_config::Format::Json;
    Ok(LoadedConfig {
        profile,
        diagnostics: diag_msgs(&diags),
        read_only,
    })
}

/// Validate and save; returns the validation diagnostics (errors block saving).
#[tauri::command]
pub fn config_save(app: AppHandle, path: PathBuf, profile: Profile) -> CmdResult<Vec<DiagMsg>> {
    let diags = dp_config::validate(&profile);
    if diags.has_errors() {
        return Ok(diag_msgs(&diags));
    }
    library::save_profile_atomic(&path, &profile).map_err(err)?;
    library_changed(&app);
    Ok(diag_msgs(&diags))
}

#[tauri::command]
pub fn config_create(game_dir: PathBuf, name: String) -> CmdResult<PathBuf> {
    library::create_config(&game_dir, &name).map_err(err)
}

#[tauri::command]
pub fn config_duplicate(path: PathBuf) -> CmdResult<PathBuf> {
    library::duplicate_config(&path).map_err(err)
}

#[tauri::command]
pub fn config_rename(app: AppHandle, path: PathBuf, name: String) -> CmdResult<()> {
    library::rename_config(&path, &name).map_err(err)?;
    library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn config_delete(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    library::delete_config(&path).map_err(err)?;
    library_changed(&app);
    Ok(())
}

#[derive(Serialize)]
pub struct Imported {
    path: PathBuf,
    diagnostics: Vec<DiagMsg>,
}

#[tauri::command]
pub fn config_import(game_dir: PathBuf, source: PathBuf) -> CmdResult<Imported> {
    let (path, diags) = library::import_into(&game_dir, &source).map_err(err)?;
    Ok(Imported {
        path,
        diagnostics: diag_msgs(&diags),
    })
}

/// Show a file or folder in the system file manager.
#[tauri::command]
pub fn reveal(path: PathBuf) -> CmdResult<()> {
    reveal_path(&path).map_err(err)
}

fn reveal_path(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let arg = if path.is_dir() {
            path.display().to_string()
        } else {
            format!("/select,{}", path.display())
        };
        std::process::Command::new("explorer")
            .arg(arg)
            .spawn()
            .map(drop)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(drop)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map(drop)
    }
}

// ----- service -------------------------------------------------------------------------

#[tauri::command]
pub fn apply_to_slot(state: State<AppState>, slot: usize, path: PathBuf) -> CmdResult<String> {
    state.call(ServiceCmd::ApplyToSlot { slot, path })
}

#[tauri::command]
pub fn select_slot(state: State<AppState>, slot: usize) -> CmdResult<String> {
    state.call(ServiceCmd::SelectSlot(slot))
}

#[tauri::command]
pub fn clear_slot(state: State<AppState>, slot: usize) -> CmdResult<String> {
    state.call(ServiceCmd::ClearSlot(slot))
}

#[tauri::command]
pub fn set_remap(state: State<AppState>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetRemap(on))
}

#[tauri::command]
pub fn set_auto_switch(state: State<AppState>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetAutoSwitch(on))
}

#[tauri::command]
pub fn set_preferred_device(state: State<AppState>, id: Option<String>) -> CmdResult<String> {
    state.call(ServiceCmd::SetPreferredDevice(id))
}

#[tauri::command]
pub fn refresh_drivers(state: State<AppState>) -> CmdResult<String> {
    state.call(ServiceCmd::RefreshDrivers)
}

// ----- reWASD ------------------------------------------------------------------------

/// What of reWASD is running. reWASD keeps the controllers to itself while it runs.
#[derive(Serialize)]
pub struct RewasdStatus {
    /// Display name of its service while that runs (quitting it needs administrator rights).
    service: Option<String>,
    /// Its other running programs (file names).
    programs: Vec<String>,
}

/// `None` when reWASD is not installed.
#[tauri::command]
pub fn rewasd_status() -> Option<RewasdStatus> {
    #[cfg(windows)]
    {
        dp_platform::windows::rewasd::detect().map(|r| RewasdStatus {
            service: r.service_running.then_some(r.service_name),
            programs: r.processes.into_iter().map(|p| p.name).collect(),
        })
    }
    #[cfg(not(windows))]
    None
}

/// Quit reWASD, then look for controllers again. `false` when the user declined the
/// administrator prompt, which leaves reWASD untouched.
#[tauri::command]
pub async fn rewasd_quit(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> CmdResult<bool> {
    #[cfg(windows)]
    {
        use dp_platform::windows::rewasd::{self, StopError};
        // The administrator prompt belongs to this window.
        let parent = window.hwnd().map_or(0, |h| h.0 as isize);
        let result = tauri::async_runtime::spawn_blocking(move || {
            match rewasd::detect().filter(|r| r.running()) {
                Some(found) => rewasd::stop(&found, parent),
                None => Ok(()),
            }
        })
        .await
        .map_err(err)?;
        if matches!(result, Err(StopError::Declined)) {
            return Ok(false);
        }
        // Also after a partial failure: whatever reWASD released is usable now.
        let _ = state.call(ServiceCmd::RescanDevices);
        result.map(|()| true).map_err(err)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, state);
        Ok(false)
    }
}

/// Stream live snapshots (devices, slots, layer, ...) to the frontend while they change.
#[tauri::command]
pub fn live_subscribe(state: State<AppState>, on_event: Channel<LiveState>) {
    let live = state.service.lock().unwrap().live_shared();
    let generation = state.live_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let current = state.live_generation.clone();
    std::thread::Builder::new()
        .name("dp-live".into())
        .spawn(move || {
            let mut last: Option<LiveState> = None;
            while current.load(Ordering::SeqCst) == generation {
                let snap = live.lock().unwrap().clone();
                if last.as_ref() != Some(&snap) {
                    if on_event.send(snap.clone()).is_err() {
                        break;
                    }
                    last = Some(snap);
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        })
        .ok();
}

// ----- settings ------------------------------------------------------------------------

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> GuiSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    state: State<AppState>,
    settings: GuiSettings,
) -> CmdResult<()> {
    settings.save().map_err(err)?;
    let stay_live = settings.stay_live;
    *state.settings.lock().unwrap() = settings;
    // The settings dialog is only reachable while the window has focus.
    state.call(ServiceCmd::SetSuspended(!stay_live))?;
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn profiles_dir(state: State<AppState>) -> PathBuf {
    state.profiles_dir.clone()
}

/// The app's log file, to attach to bug reports.
#[tauri::command]
pub fn log_file() -> PathBuf {
    crate::logging::path()
}

#[tauri::command]
pub fn take_notices(state: State<AppState>) -> Vec<String> {
    std::mem::take(&mut *state.notices.lock().unwrap())
}
