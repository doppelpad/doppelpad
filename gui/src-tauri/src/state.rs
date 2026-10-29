//! What the app keeps while it runs, shared by the commands, the tray and the watcher threads.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dp_service::{ServiceCmd, ServiceHandle, SharedLive};
use tauri::{AppHandle, Manager};

use crate::commands::system::Notice;
use crate::error::{CmdResult, err};
use crate::open_file::OpenFiles;
use crate::settings::GuiSettings;

/// Lock `m`, also after a thread panicked while it held it. What these locks guard stays
/// usable, and a panic in one watcher thread must not take the window and the tray with it.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct AppState {
    pub service: ServiceHandle,
    /// Held while a command changes the library, so changes run one at a time (commands
    /// that touch the disk run on the async runtime, side by side).
    pub disk: Mutex<()>,
    /// The service's live snapshot, for the threads that watch it.
    pub live: SharedLive,
    /// Read with [`AppState::settings`], changed with [`AppState::save_settings`].
    settings: Mutex<GuiSettings>,
    /// Held while the settings are saved, so that saves keep their order without holding up
    /// the readers of [`settings`](Self::settings).
    saving: Mutex<()>,
    /// Held while the service is told whether the editor has the focus; see
    /// [`AppState::sync_suspend`].
    suspending: Mutex<()>,
    /// The library's folder (see [`crate::commands::library::library_move`]).
    pub profiles_dir: Mutex<PathBuf>,
    /// Messages for the UI collected before it was ready (e.g. a service takeover).
    pub notices: Mutex<Vec<Notice>>,
    /// Game bundles opened from Explorer or the Finder, for the UI to import.
    pub open_files: Mutex<OpenFiles>,
    /// Incremented per live subscription; older streaming threads stop.
    pub live_generation: Arc<AtomicU64>,
    /// The main window has the focus (window events; the webview inside holds the keyboard
    /// focus, so the window itself does not report it). See [`AppState::set_main_focused`].
    pub main_focused: AtomicBool,
    /// The main window was maximized or minimized when the zoom changed, so its new minimum
    /// size waits; see [`crate::main_window::apply_zoom`].
    pub min_size_held: AtomicBool,
}

impl AppState {
    pub fn new(
        service: ServiceHandle,
        settings: GuiSettings,
        profiles_dir: PathBuf,
        notices: Vec<Notice>,
        open_files: Vec<PathBuf>,
    ) -> AppState {
        AppState {
            live: service.live_shared(),
            service,
            disk: Mutex::default(),
            settings: Mutex::new(settings),
            saving: Mutex::default(),
            suspending: Mutex::default(),
            profiles_dir: Mutex::new(profiles_dir),
            notices: Mutex::new(notices),
            open_files: Mutex::new(OpenFiles::new(open_files)),
            live_generation: Arc::new(AtomicU64::new(0)),
            main_focused: AtomicBool::new(false),
            min_size_held: AtomicBool::new(false),
        }
    }

    /// Run a service command; its success text is for logs and the CLI, not the window.
    pub fn call(&self, cmd: ServiceCmd) -> CmdResult<()> {
        self.service.call(cmd).into_result().map(drop).map_err(err)
    }

    /// What `f` reads from the settings, so that nobody copies all of them for one field.
    pub fn settings<R>(&self, f: impl FnOnce(&GuiSettings) -> R) -> R {
        f(&lock(&self.settings))
    }

    /// Store `settings` in `gui.toml` and use them from now on; returns the settings they
    /// replace. Saves run one after the other, outside the lock the readers take, so the tray
    /// on the main thread never waits for the disk.
    pub fn save_settings(&self, settings: GuiSettings) -> std::io::Result<GuiSettings> {
        let _in_order = lock(&self.saving);
        settings.save()?;
        Ok(std::mem::replace(&mut *lock(&self.settings), settings))
    }

    /// The folder the library is in.
    pub fn library_dir(&self) -> PathBuf {
        lock(&self.profiles_dir).clone()
    }

    pub fn main_focused(&self) -> bool {
        self.main_focused.load(Ordering::Relaxed)
    }

    /// The main window got or lost the focus. Called on the main thread, which runs the
    /// windows and the tray, so the service hears of it on another one.
    pub fn set_main_focused(&self, app: &AppHandle, focused: bool) {
        self.main_focused.store(focused, Ordering::Relaxed);
        in_background(app, |app| {
            let _ = app.state::<AppState>().sync_suspend();
        });
    }

    /// Remapping pauses while the editor has the focus, so mapped keys do not land in it,
    /// unless "keep remapping active" is on. Calls run one at a time and each sends the focus
    /// as it is then, so after a quick series of focus changes the last one sent is right.
    pub fn sync_suspend(&self) -> CmdResult<()> {
        let _one_at_a_time = lock(&self.suspending);
        let stay_live = self.settings(|s| s.stay_live);
        self.call(ServiceCmd::SetSuspended(self.main_focused() && !stay_live))
    }
}

/// Run `job` on a thread of the blocking pool, for work the main thread must not wait for
/// (it runs the windows and the tray, and a service call can take seconds while controllers
/// connect).
pub fn in_background(app: &AppHandle, job: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || job(&app));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_whose_holder_panicked_still_opens() {
        let m = Arc::new(Mutex::new(1));
        let held = m.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock();
            panic!("a watcher thread failed");
        })
        .join();
        assert!(m.is_poisoned());
        *lock(&m) += 1;
        assert_eq!(*lock(&m), 2);
    }
}
