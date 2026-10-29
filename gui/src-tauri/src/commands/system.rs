//! Names and defaults for the window, the OS around the app (programs, game stores, the file
//! manager, driver pages), reWASD, and what the app was started with.

use std::path::{Path, PathBuf};

use dp_core::control::Control;
use dp_core::keys::{Key, MouseButton};
use dp_core::pad::PadButton;
use dp_core::profile::{
    AnalogMapping, Crosshair, GyroSettings, Light, Magnifier, Mapping, OutputSettings, Profile,
    RadialMenu, RadialSector, StickConfig, Timing, TouchpadSettings, TriggerConfig, Vibration,
    VirtualStick,
};
#[cfg(windows)]
use dp_service::ServiceCmd;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use super::{CmdError, CmdResult, ErrorCode, err};
use crate::settings::{PAD_COLORS, ZOOM, color_hex};
use crate::state::{AppState, lock};

/// Messages for the window that arrive before it is ready (`notices.<name>` in the catalogs).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Notice {
    /// The app took over a service that was already running.
    ServiceTakeover,
}

/// The drivers Doppelpad uses, by what they do.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Driver {
    VirtualPads,
    DeviceHiding,
    /// What keyboard and mouse need from the OS: on macOS the Accessibility permission.
    InputAccess,
    /// Switch 2 controllers over Bluetooth: on macOS the Bluetooth permission, elsewhere
    /// Bluetooth itself.
    Bluetooth,
}

// ----- catalog -----------------------------------------------------------------------

#[derive(Serialize)]
pub struct Defaults {
    profile: Profile,
    mapping: Mapping,
    output: OutputSettings,
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

/// Sectors a radial menu has at most, and crosshairs a config: what the validator allows (a
/// test checks it).
pub const MAX_RADIAL_SECTORS: usize = 12;
pub const MAX_CROSSHAIRS: usize = 10;

/// Limits the window keeps to.
#[derive(Serialize)]
pub struct Limits {
    /// The window's zoom in percent: smallest, largest.
    zoom: [u16; 2],
    radial_sectors: usize,
    crosshairs: usize,
    /// The palette the controllers' tray icons take their colour from (`#rrggbb`), in the
    /// order the pick by device id counts in.
    pad_colors: Vec<String>,
}

impl Limits {
    fn new() -> Limits {
        Limits {
            zoom: [*ZOOM.start(), *ZOOM.end()],
            radial_sectors: MAX_RADIAL_SECTORS,
            crosshairs: MAX_CROSSHAIRS,
            pad_colors: PAD_COLORS.into_iter().map(color_hex).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct Catalog {
    /// Keyboard key names (profile syntax).
    keys: Vec<&'static str>,
    /// Gamepad buttons in bit order of `PadState.buttons`.
    pad_buttons: Vec<&'static str>,
    mouse_buttons: Vec<&'static str>,
    pub(crate) defaults: Defaults,
    pub(crate) limits: Limits,
}

/// Names and default values, so the frontend never duplicates them.
#[tauri::command]
pub fn catalog() -> Catalog {
    Catalog {
        keys: Key::ALL.iter().map(|k| k.name()).collect(),
        pad_buttons: PadButton::ALL.iter().map(|b| b.name()).collect(),
        mouse_buttons: MouseButton::ALL.iter().map(|b| b.name()).collect(),
        defaults: Defaults {
            profile: Profile::default(),
            mapping: Mapping::new(Control::Button(PadButton::A), Vec::new()),
            output: OutputSettings::default(),
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
        limits: Limits::new(),
    }
}

/// A game a local store (Steam, Epic, GOG) has installed.
#[derive(Serialize)]
pub struct InstalledGame {
    pub store: &'static str,
    pub name: String,
    pub dir: PathBuf,
    /// Programs that may be the game's, the likeliest first.
    pub programs: Vec<String>,
    pub cover: Option<PathBuf>,
}

/// The games the local stores have installed (their own files only, no network).
#[tauri::command(async)]
pub fn installed_games() -> Vec<InstalledGame> {
    use dp_platform::stores::Store;
    dp_platform::installed_games()
        .into_iter()
        .map(|g| InstalledGame {
            store: match g.store {
                Store::Steam => "steam",
                Store::Epic => "epic",
                Store::Gog => "gog",
            },
            name: g.name,
            dir: g.dir,
            programs: g.programs,
            cover: g.cover,
        })
        .collect()
}

/// The executable file name of the program in front.
#[tauri::command(async)]
pub fn foreground_program() -> Option<String> {
    dp_platform::foreground_program()
}

/// A program that shows a window now, offered as a game's program.
#[derive(Serialize)]
pub struct RunningProgram {
    pub exe: String,
    pub title: String,
}

/// The programs with a visible window, sorted by file name (empty where the OS gives none).
#[tauri::command(async)]
pub fn running_programs() -> Vec<RunningProgram> {
    let mut out: Vec<RunningProgram> = dp_platform::windowed_programs()
        .into_iter()
        .map(|p| RunningProgram {
            exe: p.exe,
            title: p.title,
        })
        .collect();
    out.sort_by_key(|p| p.exe.to_lowercase());
    out
}

/// Open the download page of a driver Doppelpad uses, or on macOS the settings where the
/// user allows keyboard and mouse, or Bluetooth; on Windows the Bluetooth settings.
#[tauri::command]
pub fn open_driver_page(driver: Driver) -> CmdResult<()> {
    let url = match driver {
        Driver::VirtualPads => dp_platform::VIGEM_RELEASES,
        Driver::DeviceHiding => dp_platform::HIDHIDE_RELEASES,
        #[cfg(target_os = "macos")]
        Driver::InputAccess => {
            // The prompt adds Doppelpad to the list, where the user only has to turn it on.
            dp_platform::macos::permissions::request();
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        #[cfg(target_os = "macos")]
        Driver::Bluetooth => {
            // The first time macOS asks by itself; once refused, only its settings can allow it.
            if dp_platform::macos::ble::request() {
                return Ok(());
            }
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth"
        }
        #[cfg(windows)]
        Driver::Bluetooth => "ms-settings:bluetooth",
        #[cfg(not(target_os = "macos"))]
        Driver::InputAccess => {
            return Err(CmdError::new(ErrorCode::NothingToAllow));
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        Driver::Bluetooth => {
            return Err(CmdError::new(ErrorCode::NothingToAllow));
        }
    };
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer")
        .arg(url)
        .spawn()
        .map(drop);
    #[cfg(target_os = "macos")]
    let opened = dp_platform::spawn_detached(std::process::Command::new("open").arg(url));
    #[cfg(not(any(windows, target_os = "macos")))]
    let opened = dp_platform::spawn_detached(dp_platform::host_command("xdg-open").arg(url));
    opened.map_err(err)
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
        dp_platform::spawn_detached(std::process::Command::new("open").arg("-R").arg(path))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        dp_platform::spawn_detached(dp_platform::host_command("xdg-open").arg(dir))
    }
}

// ----- reWASD ------------------------------------------------------------------------

/// What of reWASD is running. reWASD keeps the controllers to itself while it runs.
#[derive(Serialize)]
pub struct RewasdStatus {
    /// Display name of its service while that runs (quitting it needs administrator rights).
    pub service: Option<String>,
    /// Its other running programs (file names).
    pub programs: Vec<String>,
}

/// `None` when reWASD is not installed.
#[tauri::command(async)]
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

/// Show the window, e.g. to ask something at start-up when the app started hidden.
#[tauri::command]
pub fn window_show(app: AppHandle) {
    crate::main_window::show_main(&app);
}

/// The app's log file, to attach to bug reports.
#[tauri::command]
pub fn log_file() -> PathBuf {
    crate::logging::path()
}

#[tauri::command]
pub fn take_notices(state: State<AppState>) -> Vec<Notice> {
    std::mem::take(&mut *lock(&state.notices))
}

/// The game bundles opened before the window's page asked (it listens for `open-files` first);
/// each is handed out once, and later ones arrive as events.
#[tauri::command]
pub fn take_open_files(state: State<AppState>) -> Vec<PathBuf> {
    lock(&state.open_files).take()
}
