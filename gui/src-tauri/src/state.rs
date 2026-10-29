//! What the app keeps while it runs, shared by the commands, the tray and the watcher threads.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dp_service::{ServiceCmd, ServiceHandle, SharedLive};

use crate::commands::CmdResult;
use crate::commands::system::Notice;
use crate::settings::GuiSettings;

pub struct AppState {
    pub service: ServiceHandle,
    /// Held while a command changes the library, so changes run one at a time (commands
    /// that touch the disk run on the async runtime, side by side).
    pub disk: Mutex<()>,
    /// The service's live snapshot, for the threads that watch it.
    pub live: SharedLive,
    pub settings: Mutex<GuiSettings>,
    /// The library's folder (see [`crate::commands::library::library_move`]).
    pub profiles_dir: Mutex<PathBuf>,
    /// Messages for the UI collected before it was ready (e.g. a service takeover).
    pub notices: Mutex<Vec<Notice>>,
    /// Game bundles the app was started with (opened from Explorer), for the UI to import.
    pub open_files: Mutex<Vec<PathBuf>>,
    /// The UI has taken [`open_files`](Self::open_files): later bundles go to it as events.
    pub open_files_taken: AtomicBool,
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
    /// Run a service command; its success text is for logs and the CLI, not the window.
    pub fn call(&self, cmd: ServiceCmd) -> CmdResult<()> {
        self.service.call(cmd).into_result().map(drop)
    }

    /// The folder the library is in.
    pub fn library_dir(&self) -> PathBuf {
        self.profiles_dir.lock().unwrap().clone()
    }

    pub fn main_focused(&self) -> bool {
        self.main_focused.load(Ordering::Relaxed)
    }

    /// The main window got or lost the focus.
    pub fn set_main_focused(&self, focused: bool) {
        self.main_focused.store(focused, Ordering::Relaxed);
        let _ = self.sync_suspend();
    }

    /// Remapping pauses while the editor has the focus, so mapped keys do not land in it,
    /// unless "keep remapping active" is on.
    pub fn sync_suspend(&self) -> CmdResult<()> {
        let stay_live = self.settings.lock().unwrap().stay_live;
        self.call(ServiceCmd::SetSuspended(self.main_focused() && !stay_live))
    }
}
