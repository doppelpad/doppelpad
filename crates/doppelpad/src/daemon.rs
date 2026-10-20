//! Background daemon: runs the remapping service with its IPC server until Ctrl+C or
//! `doppelpad shutdown`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use dp_service::{HostOptions, ServiceOptions};

pub fn run(profiles_dir: PathBuf, initial: Option<PathBuf>, auto_switch: bool) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let r = running.clone();
        ctrlc::set_handler(move || r.store(false, Ordering::Relaxed))?;
    }

    let mut service = ServiceOptions::new(profiles_dir);
    service.initial_profile = initial;
    service.auto_switch = Some(auto_switch);
    // Slots and controller hotkeys are shared with the app.
    service.state_path = Some(dp_config::config_dir().join("state.toml"));
    service.hotkeys_path = Some(dp_config::config_dir().join("hotkeys.toml"));
    service.calibration_path = Some(dp_config::config_dir().join("calibration.toml"));
    let mut handle = dp_service::spawn(HostOptions { service, ipc: true })?;
    tracing::info!("daemon running; control it with `doppelpad apply|select-slot|remap|status`");

    while running.load(Ordering::Relaxed) && handle.is_running() {
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.shutdown();
    tracing::info!("daemon stopped");
    Ok(())
}
