//! Updates from the project's GitHub releases (tauri-plugin-updater; endpoints and public key
//! in `tauri.conf.json`). The window asks whether a newer version is out and whether to
//! install it. Installing downloads the setup and checks its signature, stops the remapping
//! service (it shows hidden controllers again), then starts the setup in passive mode and
//! exits. The setup replaces the files and starts the app again.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::commands::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

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
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<UpdateInfo>> {
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
        .await
        .map_err(|e| {
            tracing::warn!("update check failed: {e}");
            err(e)
        })?;
    let info = update.as_ref().map(|u| {
        tracing::info!("update available: {} -> {}", u.current_version, u.version);
        UpdateInfo {
            version: u.version.clone(),
            current: u.current_version.clone(),
            notes: u.body.clone().filter(|n| !n.trim().is_empty()),
        }
    });
    *pending.0.lock().unwrap() = update;
    Ok(info)
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
        return Err(
            "this is a portable copy: replace its files with the new version from the \
                    releases page"
                .into(),
        );
    }
    let update = pending
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("no update to install: check for one first")?;
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
