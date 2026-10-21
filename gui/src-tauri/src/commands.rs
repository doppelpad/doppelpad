//! Commands the web frontend invokes. Every command returns `Result<_, String>` so
//! errors arrive in JavaScript as readable messages.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dp_config::Diagnostic;
use dp_config::bundle::{self, LayerPreset, PresetEntry};
use dp_config::library::{self, Game, GameMeta, Library};
use dp_config::templates::{self, Template};
use dp_core::control::{Control, mouse_button_name, pad_button_name};
use dp_core::hotkeys::Hotkeys;
use dp_core::keys::{Key, MouseButton};
use dp_core::pad::PadButton;
use dp_core::profile::{
    AnalogMapping, Crosshair, GyroSettings, Light, Magnifier, Mapping, Output, Profile, RadialMenu,
    RadialSector, StickConfig, Timing, TouchpadSettings, TriggerConfig, Vibration, VirtualStick,
};
use dp_ipc::Request;
use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;
use dp_service::dsu::DsuSettings;
use dp_service::httpapi::HttpSettings;
use dp_service::slots::RuntimeOptions;
use dp_service::{LiveState, PlayerLights, ServiceCmd, ServiceHandle, ServiceReply};
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
    /// The main window has the focus (window events; the webview inside holds the keyboard
    /// focus, so the window itself does not report it).
    pub main_focused: std::sync::atomic::AtomicBool,
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

/// The outcome of a change to the games and configs on disk; when it worked, the service
/// and the tray are told.
fn library_change<T, E: std::fmt::Display>(app: &AppHandle, result: Result<T, E>) -> CmdResult<T> {
    let out = result.map_err(err)?;
    library_changed(app);
    Ok(out)
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
    virtual_stick: VirtualStick,
    gyro: GyroSettings,
    touchpad: TouchpadSettings,
    crosshair: Crosshair,
    magnifier: Magnifier,
    radial_menu: RadialMenu,
    radial_sector: RadialSector,
    light: Light,
    vibration: Vibration,
}

#[derive(Serialize)]
pub struct Catalog {
    /// Keyboard key names (profile syntax).
    keys: Vec<&'static str>,
    /// Gamepad buttons in bit order of `PadState.buttons`.
    pad_buttons: Vec<&'static str>,
    mouse_buttons: Vec<&'static str>,
    pub(crate) defaults: Defaults,
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
            virtual_stick: VirtualStick::default(),
            gyro: GyroSettings::default(),
            touchpad: TouchpadSettings::default(),
            crosshair: Crosshair::default(),
            magnifier: Magnifier::default(),
            radial_menu: RadialMenu::default(),
            radial_sector: RadialSector::default(),
            light: Light::default(),
            vibration: Vibration::default(),
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
    library_change(&app, library::create_game(&state.profiles_dir, &name))
}

#[tauri::command]
pub fn game_update(app: AppHandle, dir: PathBuf, meta: GameMeta) -> CmdResult<()> {
    library_change(&app, library::update_game_meta(&dir, &meta))
}

#[tauri::command]
pub fn game_delete(app: AppHandle, dir: PathBuf) -> CmdResult<()> {
    library_change(&app, library::delete_game(&dir))
}

/// A new game whose first config (`config`) starts from a template.
#[tauri::command]
pub fn game_create_template(
    app: AppHandle,
    state: State<AppState>,
    name: String,
    config: String,
    template: Template,
) -> CmdResult<Game> {
    let profile = templates::profile(template, &config);
    library_change(
        &app,
        library::create_game_with(&state.profiles_dir, &name, &profile),
    )
}

/// A new config in a game, starting from a template.
#[tauri::command]
pub fn config_from_template(
    app: AppHandle,
    game_dir: PathBuf,
    name: String,
    template: Template,
) -> CmdResult<PathBuf> {
    let profile = templates::profile(template, &name);
    library_change(&app, library::create_config_with(&game_dir, &profile))
}

/// Open the download page of a driver Doppelpad uses.
#[tauri::command]
pub fn open_driver_page(driver: String) -> CmdResult<()> {
    let url = match driver.as_str() {
        "virtual_pads" => "https://github.com/nefarius/ViGEmBus/releases",
        "device_hiding" => "https://github.com/nefarius/HidHide/releases",
        _ => return Err(format!("unknown driver {driver}")),
    };
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let opened = std::process::Command::new("xdg-open").arg(url).spawn();
    opened.map(drop).map_err(err)
}

/// Write a game and its configs as one bundle file.
#[tauri::command]
pub fn game_export(dir: PathBuf, out: PathBuf) -> CmdResult<()> {
    bundle::export_game(&dir, &out).map_err(err)
}

/// Unpack a game bundle into a new game folder.
#[tauri::command]
pub fn game_import(app: AppHandle, state: State<AppState>, file: PathBuf) -> CmdResult<Game> {
    library_change(&app, bundle::import_game(&state.profiles_dir, &file))
}

#[tauri::command]
pub fn preset_list(state: State<AppState>) -> Vec<PresetEntry> {
    bundle::list_presets(&state.profiles_dir)
}

#[tauri::command]
pub fn preset_save(state: State<AppState>, preset: LayerPreset) -> CmdResult<PathBuf> {
    bundle::save_preset(&state.profiles_dir, &preset).map_err(err)
}

#[tauri::command]
pub fn preset_load(path: PathBuf) -> CmdResult<LayerPreset> {
    bundle::load_preset(&path).map_err(err)
}

#[tauri::command]
pub fn preset_delete(path: PathBuf) -> CmdResult<()> {
    bundle::delete_preset(&path).map_err(err)
}

/// Save a bindings sheet (an HTML page the editor made).
#[tauri::command]
pub fn sheet_save(path: PathBuf, html: String) -> CmdResult<()> {
    let html_file = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
    if !html_file {
        return Err("a bindings sheet is saved as .html".into());
    }
    std::fs::write(&path, html).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Serialize)]
pub struct LoadedConfig {
    profile: Profile,
    diagnostics: Vec<Diagnostic>,
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
        diagnostics: diags.0,
        read_only,
    })
}

/// Validate and save; returns the validation diagnostics (errors block saving).
#[tauri::command]
pub fn config_save(app: AppHandle, path: PathBuf, profile: Profile) -> CmdResult<Vec<Diagnostic>> {
    let diags = dp_config::validate(&profile);
    if diags.has_errors() {
        return Ok(diags.0);
    }
    library_change(&app, library::save_profile_atomic(&path, &profile))?;
    Ok(diags.0)
}

#[tauri::command]
pub fn config_create(app: AppHandle, game_dir: PathBuf, name: String) -> CmdResult<PathBuf> {
    library_change(&app, library::create_config(&game_dir, &name))
}

#[tauri::command]
pub fn config_duplicate(app: AppHandle, path: PathBuf) -> CmdResult<PathBuf> {
    library_change(&app, library::duplicate_config(&path))
}

#[tauri::command]
pub fn config_rename(app: AppHandle, path: PathBuf, name: String) -> CmdResult<()> {
    library_change(&app, library::rename_config(&path, &name))
}

#[tauri::command]
pub fn config_delete(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    library_change(&app, library::delete_config(&path))
}

#[derive(Serialize)]
pub struct Imported {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
}

#[tauri::command]
pub fn config_import(app: AppHandle, game_dir: PathBuf, source: PathBuf) -> CmdResult<Imported> {
    let (path, diags) = library_change(&app, library::import_into(&game_dir, &source))?;
    Ok(Imported {
        path,
        diagnostics: diags.0,
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

/// `device`: a controller's own slots instead of the shared ones.
#[tauri::command]
pub fn apply_to_slot(
    state: State<AppState>,
    slot: usize,
    path: PathBuf,
    device: Option<String>,
) -> CmdResult<String> {
    state.call(ServiceCmd::ApplyToSlot { device, slot, path })
}

#[tauri::command]
pub fn select_slot(
    state: State<AppState>,
    slot: usize,
    device: Option<String>,
) -> CmdResult<String> {
    state.call(ServiceCmd::SelectSlot { device, slot })
}

#[tauri::command]
pub fn clear_slot(
    state: State<AppState>,
    slot: usize,
    device: Option<String>,
) -> CmdResult<String> {
    state.call(ServiceCmd::ClearSlot { device, slot })
}

/// Give a controller slots of its own, or let it use the shared slots again.
#[tauri::command]
pub fn own_slots(state: State<AppState>, device: String, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::OwnSlots { device, on })
}

#[tauri::command]
pub fn set_remap(state: State<AppState>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetRemap(on))
}

#[tauri::command]
pub fn set_auto_switch(state: State<AppState>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetAutoSwitch(on))
}

/// Ignore a device completely, or use it again.
#[tauri::command]
pub fn set_blocked(state: State<AppState>, id: String, blocked: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetBlocked { id, blocked })
}

/// Replace the groups of controllers that act as one.
#[tauri::command]
pub fn set_groups(state: State<AppState>, groups: Vec<DeviceGroup>) -> CmdResult<String> {
    state.call(ServiceCmd::SetGroups(groups))
}

/// Turn wireless controllers off after `minutes` without input; 0 never.
#[tauri::command]
pub fn set_power_off_after(state: State<AppState>, minutes: u32) -> CmdResult<String> {
    state.call(ServiceCmd::SetPowerOffAfter(minutes))
}

/// Store how a generic pad model's inputs map to pad controls.
#[tauri::command]
pub fn set_layout(state: State<AppState>, layout: GenericLayout) -> CmdResult<String> {
    state.call(ServiceCmd::SetLayout(layout))
}

/// Go back to the guessed layout for a generic pad model.
#[tauri::command]
pub fn forget_layout(state: State<AppState>, device: String) -> CmdResult<String> {
    state.call(ServiceCmd::ForgetLayout(device))
}

/// Turn the local HTTP API on or off, or change where it listens.
#[tauri::command]
pub fn set_http(state: State<AppState>, settings: HttpSettings) -> CmdResult<String> {
    state.call(ServiceCmd::SetHttp(settings))
}

/// Replace the HTTP API's token.
#[tauri::command]
pub fn new_http_token(state: State<AppState>) -> CmdResult<String> {
    state.call(ServiceCmd::NewHttpToken)
}

/// Sleep, focus and disconnect behaviour of remapping.
#[tauri::command]
pub fn set_runtime_options(state: State<AppState>, options: RuntimeOptions) -> CmdResult<String> {
    state.call(ServiceCmd::SetRuntimeOptions(options))
}

/// Turn the DSU motion server for emulators on or off, or move it.
#[tauri::command]
pub fn set_dsu(state: State<AppState>, settings: DsuSettings) -> CmdResult<String> {
    state.call(ServiceCmd::SetDsu(settings))
}

/// Turn a wireless controller off now.
#[tauri::command]
pub fn power_off(state: State<AppState>, id: String) -> CmdResult<String> {
    state.call(ServiceCmd::PowerOff(id))
}

/// What the controllers' player lights and microphone LED show.
#[tauri::command]
pub fn set_lights(state: State<AppState>, player: PlayerLights, mic: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetLights { player, mic })
}

#[tauri::command]
pub fn set_preferred_device(state: State<AppState>, id: Option<String>) -> CmdResult<String> {
    state.call(ServiceCmd::SetPreferredDevice(id))
}

#[tauri::command]
pub fn refresh_drivers(state: State<AppState>) -> CmdResult<String> {
    state.call(ServiceCmd::RefreshDrivers)
}

#[tauri::command]
pub fn hotkeys_get(state: State<AppState>) -> CmdResult<Hotkeys> {
    match state.service.lock().unwrap().call(ServiceCmd::GetHotkeys) {
        ServiceReply::Hotkeys(h) => Ok(h),
        other => Err(other.into_result().err().unwrap_or_default()),
    }
}

#[tauri::command]
pub fn hotkeys_set(state: State<AppState>, hotkeys: Hotkeys) -> CmdResult<String> {
    state.call(ServiceCmd::SetHotkeys(hotkeys))
}

/// Pause the hotkeys while the user records a new one.
#[tauri::command]
pub fn hotkeys_pause(state: State<AppState>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::PauseHotkeys(on))
}

/// Send one full in-game turn of `counts` mouse counts after a short delay (calibration).
#[tauri::command]
pub fn test_turn(state: State<AppState>, counts: f32) -> CmdResult<String> {
    state.call(ServiceCmd::TestTurn { counts })
}

/// Measure a controller's gyro bias (it should lie still); `None`: the controller in use.
#[tauri::command]
pub fn gyro_calibrate(state: State<AppState>, device: Option<String>) -> CmdResult<String> {
    state.call(ServiceCmd::CalibrateGyro { device })
}

/// Switch the automatic gyro calibration of a controller.
#[tauri::command]
pub fn gyro_auto(state: State<AppState>, device: Option<String>, on: bool) -> CmdResult<String> {
    state.call(ServiceCmd::SetGyroAuto { device, on })
}

/// Forget a controller's gyro bias.
#[tauri::command]
pub fn gyro_reset(state: State<AppState>, device: Option<String>) -> CmdResult<String> {
    state.call(ServiceCmd::ResetGyro { device })
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
    mut settings: GuiSettings,
) -> CmdResult<()> {
    settings.osd = settings.osd.clamped();
    settings.save().map_err(err)?;
    let stay_live = settings.stay_live;
    *state.settings.lock().unwrap() = settings;
    // "Keep remapping active" may have changed; the pause still needs the window in focus.
    let focused = state
        .main_focused
        .load(std::sync::atomic::Ordering::Relaxed);
    state.call(ServiceCmd::SetSuspended(focused && !stay_live))?;
    crate::tray::refresh(&app);
    Ok(())
}

/// Whether the app starts hidden in the tray when the user signs in; `None` where it
/// cannot (not Windows). Read from Windows each time: Task Manager can change it too.
#[tauri::command]
pub fn autostart_get() -> Option<bool> {
    crate::autostart::enabled()
}

#[tauri::command]
pub fn autostart_set(on: bool) -> CmdResult<()> {
    crate::autostart::set(on).map_err(err)
}

/// Show the window, e.g. to ask something at start-up when the app started hidden.
#[tauri::command]
pub fn window_show(app: AppHandle) {
    crate::tray::show_main(&app);
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
