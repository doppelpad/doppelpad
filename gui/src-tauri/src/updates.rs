//! Updates from the project's GitHub releases (tauri-plugin-updater; endpoints and public key
//! in `tauri.conf.json`). The window asks whether a newer version is out and whether to
//! install it. Installing downloads the setup and checks its signature, stops the remapping
//! service (it shows hidden controllers again), then starts the setup in passive mode and
//! exits. The setup replaces the files and starts the app again. On Linux the download is
//! the new AppImage, which replaces the running one's file before the app restarts.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Error as UpdaterError, Update, UpdaterExt};

use crate::error::{CmdError, CmdResult, ErrorCode, err};
use crate::state::{AppState, lock};

/// The update the last check found, for `update_install`.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// A newer version, as the window shows it.
#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    /// The running version.
    current: String,
    /// The release notes, as plain text.
    notes: Option<String>,
}

/// Ask the update endpoints whether a newer version is out; `None` when this one is the newest.
/// On Linux only the AppImage updates itself (it replaces its own file), on macOS only the app
/// bundle (it replaces the bundle), on Windows only an installed copy (the setup installs into
/// Program Files, not beside a portable one); a build from source asks for nothing.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<UpdateInfo>> {
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        tracing::debug!("not an AppImage: no update check");
        return Ok(None);
    }
    if cfg!(target_os = "macos") && !in_app_bundle() {
        tracing::debug!("not in an app bundle: no update check");
        return Ok(None);
    }
    if dp_config::portable_dir().is_some() {
        tracing::debug!("a portable copy: no update check");
        return Ok(None);
    }
    let handle = app.clone();
    let update = app
        .updater_builder()
        // Stop the service right before the setup starts, after the download and its checks.
        .on_before_exit(move || {
            handle.state::<AppState>().service.shutdown();
            handle.cleanup_before_exit();
        })
        // The setup starts the app without the arguments of this run, so it comes back with
        // its window even when it was started hidden with Windows (`/R` without `/ARGS`).
        .restart_after_install(false)
        .installer_arg("/R")
        .build()
        .map_err(err)?
        .check()
        .await;
    let update = match update {
        // A release without a build for this system (macOS before its first): nothing newer.
        Err(e @ (UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_))) => {
            tracing::info!("no update for this system: {e}");
            None
        }
        Err(e) => {
            tracing::warn!("update check failed: {e}");
            return Err(err(e));
        }
        Ok(update) => update,
    };
    let info = update.as_ref().map(|u| {
        tracing::info!("update available: {} -> {}", u.current_version, u.version);
        UpdateInfo {
            version: u.version.clone(),
            current: u.current_version.clone(),
            notes: u.body.clone().filter(|n| !n.trim().is_empty()),
        }
    });
    *lock(&pending.0) = update;
    Ok(info)
}

/// Whether this program runs from an app bundle (`….app/Contents/MacOS/`).
fn in_app_bundle() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.parent()
            .is_some_and(|dir| dir.ends_with("Contents/MacOS"))
    })
}

/// Download the update the last check found and install it; `on_progress` gets the download's
/// progress in percent. The download is rejected unless its signature matches the public key.
/// On Windows the app exits and the setup takes over; this returns only when that failed.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    on_progress: Channel<u8>,
) -> CmdResult<()> {
    // The setup installs into Program Files, not into a portable copy's folder.
    if dp_config::portable_dir().is_some() {
        return Err(CmdError::new(ErrorCode::PortableUpdate));
    }
    let update = lock(&pending.0)
        .clone()
        .ok_or(CmdError::new(ErrorCode::NoUpdate))?;
    let mut received: u64 = 0;
    let mut shown = None;
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                if let Some(total) = total.filter(|&t| t > 0) {
                    let percent = (received * 100 / total).min(100) as u8;
                    if shown != Some(percent) {
                        shown = Some(percent);
                        let _ = on_progress.send(percent);
                    }
                }
            },
            || {},
        )
        .await
        .map_err(|e| {
            tracing::warn!("update {} not installed: {e}", update.version);
            err(e)
        })?;
    tracing::info!("installing Doppelpad {}", update.version);
    let result = update.install(bytes);
    let state = app.state::<AppState>();
    // Installed in place (not Windows), or the setup did not start after the service had
    // stopped (the administrator prompt was declined): start again, with the service.
    if result.is_ok() || !state.service.is_running() {
        if let Err(e) = &result {
            tracing::warn!("the setup did not start: {e}");
        }
        state.service.shutdown();
        app.request_restart();
    }
    result.map_err(err)
}
