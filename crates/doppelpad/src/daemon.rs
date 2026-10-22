//! Background daemon: runs the remapping service with its IPC server until Ctrl+C or
//! `doppelpad shutdown`.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use dp_service::{HostOptions, ServiceOptions};

pub fn run(profiles_dir: PathBuf, initial: Option<PathBuf>, auto_switch: bool) -> Result<()> {
    let running = crate::stop_flag()?;

    let mut service = ServiceOptions::with_user_files(profiles_dir);
    service.initial_profile = initial;
    service.auto_switch = Some(auto_switch);
    let handle = dp_service::spawn(HostOptions { service, ipc: true })?;
    tracing::info!("daemon running; control it with `doppelpad apply|select-slot|remap|status`");

    while running.load(Ordering::Relaxed) && handle.is_running() {
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.shutdown();
    tracing::info!("daemon stopped");
    Ok(())
}
