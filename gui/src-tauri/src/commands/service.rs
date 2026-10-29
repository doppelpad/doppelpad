//! What the remapping service does: slots, remapping, devices, hotkeys, gyro calibration, and
//! the live state it streams to the window.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use dp_core::hotkeys::Hotkeys;
use dp_platform::generic::GenericLayout;
use dp_platform::groups::DeviceGroup;
use dp_service::dsu::DsuSettings;
use dp_service::httpapi::HttpSettings;
use dp_service::slots::RuntimeOptions;
use dp_service::{LIVE_INTERVAL, LiveState, PlayerLights, ServiceCmd, ServiceReply};
use tauri::State;
use tauri::ipc::Channel;

use super::CmdResult;
use crate::state::AppState;

/// `device`: a controller's own slots instead of the shared ones.
#[tauri::command(async)]
pub fn apply_to_slot(
    state: State<AppState>,
    slot: usize,
    path: PathBuf,
    device: Option<String>,
) -> CmdResult<()> {
    state.call(ServiceCmd::ApplyToSlot { device, slot, path })
}

#[tauri::command(async)]
pub fn select_slot(state: State<AppState>, slot: usize, device: Option<String>) -> CmdResult<()> {
    state.call(ServiceCmd::SelectSlot { device, slot })
}

#[tauri::command(async)]
pub fn clear_slot(state: State<AppState>, slot: usize, device: Option<String>) -> CmdResult<()> {
    state.call(ServiceCmd::ClearSlot { device, slot })
}

/// Give a controller slots of its own, or let it use the shared slots again.
#[tauri::command(async)]
pub fn own_slots(state: State<AppState>, device: String, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::OwnSlots { device, on })
}

#[tauri::command(async)]
pub fn set_remap(state: State<AppState>, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetRemap(on))
}

#[tauri::command(async)]
pub fn set_auto_switch(state: State<AppState>, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetAutoSwitch(on))
}

/// Ignore a device completely, or use it again.
#[tauri::command(async)]
pub fn set_blocked(state: State<AppState>, id: String, blocked: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetBlocked { id, blocked })
}

/// Replace the groups of controllers that act as one.
#[tauri::command(async)]
pub fn set_groups(state: State<AppState>, groups: Vec<DeviceGroup>) -> CmdResult<()> {
    state.call(ServiceCmd::SetGroups(groups))
}

/// Turn wireless controllers off after `minutes` without input; 0 never.
#[tauri::command(async)]
pub fn set_power_off_after(state: State<AppState>, minutes: u32) -> CmdResult<()> {
    state.call(ServiceCmd::SetPowerOffAfter(minutes))
}

/// Look for Switch 2 controllers over Bluetooth, or stop looking.
#[tauri::command(async)]
pub fn set_switch2_bluetooth(state: State<AppState>, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetSwitch2Bluetooth(on))
}

/// Store how a generic pad model's inputs map to pad controls.
#[tauri::command(async)]
pub fn set_layout(state: State<AppState>, layout: GenericLayout) -> CmdResult<()> {
    state.call(ServiceCmd::SetLayout(layout))
}

/// Go back to the guessed layout for a generic pad model.
#[tauri::command(async)]
pub fn forget_layout(state: State<AppState>, device: String) -> CmdResult<()> {
    state.call(ServiceCmd::ForgetLayout(device))
}

/// Turn the local HTTP API on or off, or change where it listens.
#[tauri::command(async)]
pub fn set_http(state: State<AppState>, settings: HttpSettings) -> CmdResult<()> {
    state.call(ServiceCmd::SetHttp(settings))
}

/// Replace the HTTP API's token.
#[tauri::command(async)]
pub fn new_http_token(state: State<AppState>) -> CmdResult<()> {
    state.call(ServiceCmd::NewHttpToken)
}

/// Sleep, focus and disconnect behaviour of remapping.
#[tauri::command(async)]
pub fn set_runtime_options(state: State<AppState>, options: RuntimeOptions) -> CmdResult<()> {
    state.call(ServiceCmd::SetRuntimeOptions(options))
}

/// Turn the DSU motion server for emulators on or off, or move it.
#[tauri::command(async)]
pub fn set_dsu(state: State<AppState>, settings: DsuSettings) -> CmdResult<()> {
    state.call(ServiceCmd::SetDsu(settings))
}

/// Turn a wireless controller off now.
#[tauri::command(async)]
pub fn power_off(state: State<AppState>, id: String) -> CmdResult<()> {
    state.call(ServiceCmd::PowerOff(id))
}

/// What the controllers' player lights and microphone LED show.
#[tauri::command(async)]
pub fn set_lights(state: State<AppState>, player: PlayerLights, mic: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetLights { player, mic })
}

#[tauri::command(async)]
pub fn set_preferred_device(state: State<AppState>, id: Option<String>) -> CmdResult<()> {
    state.call(ServiceCmd::SetPreferredDevice(id))
}

#[tauri::command(async)]
pub fn refresh_drivers(state: State<AppState>) -> CmdResult<()> {
    state.call(ServiceCmd::RefreshDrivers)
}

/// Look for controllers now and retry those that could not be opened (another program may
/// have let them go).
#[tauri::command(async)]
pub fn rescan_devices(state: State<AppState>) -> CmdResult<()> {
    state.call(ServiceCmd::RescanDevices)
}

#[tauri::command(async)]
pub fn hotkeys_get(state: State<AppState>) -> CmdResult<Hotkeys> {
    match state.service.call(ServiceCmd::GetHotkeys) {
        ServiceReply::Hotkeys(h) => Ok(h),
        other => Err(other
            .into_result()
            .err()
            .unwrap_or_else(|| "the service answered something else than hotkeys".into())),
    }
}

#[tauri::command(async)]
pub fn hotkeys_set(state: State<AppState>, hotkeys: Hotkeys) -> CmdResult<()> {
    state.call(ServiceCmd::SetHotkeys(hotkeys))
}

/// Pause the hotkeys while the user records a new one.
#[tauri::command(async)]
pub fn hotkeys_pause(state: State<AppState>, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::PauseHotkeys(on))
}

/// Send one full in-game turn of `counts` mouse counts after a short delay (calibration).
#[tauri::command(async)]
pub fn test_turn(state: State<AppState>, counts: f32) -> CmdResult<()> {
    state.call(ServiceCmd::TestTurn { counts })
}

/// Measure a controller's gyro bias (it should lie still); `None`: the controller in use.
#[tauri::command(async)]
pub fn gyro_calibrate(state: State<AppState>, device: Option<String>) -> CmdResult<()> {
    state.call(ServiceCmd::CalibrateGyro { device })
}

/// Switch the automatic gyro calibration of a controller.
#[tauri::command(async)]
pub fn gyro_auto(state: State<AppState>, device: Option<String>, on: bool) -> CmdResult<()> {
    state.call(ServiceCmd::SetGyroAuto { device, on })
}

/// Forget a controller's gyro bias.
#[tauri::command(async)]
pub fn gyro_reset(state: State<AppState>, device: Option<String>) -> CmdResult<()> {
    state.call(ServiceCmd::ResetGyro { device })
}

/// Stream live snapshots (devices, slots, layer, ...) to the frontend while they change.
#[tauri::command]
pub fn live_subscribe(state: State<AppState>, on_event: Channel<LiveState>) {
    let live = state.live.clone();
    let generation = state.live_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let current = state.live_generation.clone();
    std::thread::Builder::new()
        .name("dp-live".into())
        .spawn(move || {
            let mut seen = 0;
            while current.load(Ordering::SeqCst) == generation {
                if let Some((version, snap)) = live.since(seen) {
                    if on_event.send((*snap).clone()).is_err() {
                        break;
                    }
                    seen = version;
                }
                std::thread::sleep(LIVE_INTERVAL);
            }
        })
        .ok();
}
