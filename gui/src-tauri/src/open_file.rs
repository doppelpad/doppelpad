//! Game bundles handed to the app from Explorer or the Finder. On Windows and Linux a
//! double-clicked `.dpgame` arrives in the command line, at start-up or (while the app runs)
//! through the single-instance handler; macOS sends it to the running app as an event. Either
//! way a bundle waits until the window's page has asked for the bundles (it may still be
//! loading, and would miss the event), and goes to it as an event after that.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager};

use crate::main_window::{MAIN_WINDOW, show_main};
use crate::state::{AppState, lock};

/// The event that brings bundles opened while the app runs to the window.
pub const OPEN_EVENT: &str = "open-files";

/// Whether `path` names a game bundle.
fn is_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dpgame"))
}

/// The game bundles among command-line arguments (not the program itself or flags), made
/// absolute against `cwd`.
pub fn bundles_in(args: &[String], cwd: &Path) -> Vec<PathBuf> {
    args.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .filter(|p| is_bundle(p))
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) })
        .collect()
}

/// Bundles on their way to the window's page.
#[derive(Debug, Default)]
pub struct OpenFiles {
    waiting: Vec<PathBuf>,
    /// The page has taken the waiting bundles: it listens for [`OPEN_EVENT`] since.
    taken: bool,
}

impl OpenFiles {
    /// The bundles the app was started with.
    pub fn new(files: Vec<PathBuf>) -> OpenFiles {
        OpenFiles {
            waiting: files,
            taken: false,
        }
    }

    /// The waiting bundles, for the page; later ones go to it as events.
    pub fn take(&mut self) -> Vec<PathBuf> {
        self.taken = true;
        std::mem::take(&mut self.waiting)
    }

    /// `files` arrived: the ones to send to the page now, or `None` while they wait.
    fn arrive(&mut self, files: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
        if self.taken {
            Some(files)
        } else {
            self.waiting.extend(files);
            None
        }
    }
}

/// Files the app is asked to open while it runs: the bundles among them go to the window, or
/// wait for its page (see [`OpenFiles`]).
pub fn opened(app: &AppHandle, files: Vec<PathBuf>) {
    let files: Vec<PathBuf> = files.into_iter().filter(|p| is_bundle(p)).collect();
    if files.is_empty() {
        return;
    }
    show_main(app);
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!("bundles opened before the app was set up: {files:?}");
        return;
    };
    // Sent while locked: a page taking the bundles now gets them either way, and only once.
    let mut open = lock(&state.open_files);
    if let Some(files) = open.arrive(files) {
        let _ = app.emit_to(MAIN_WINDOW, OPEN_EVENT, files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_bundles_among_arguments() {
        let args: Vec<String> = [
            "C:/Doppelpad/doppelpad-gui.exe",
            "--hidden",
            "Halo.DPGAME",
            "notes.txt",
        ]
        .map(String::from)
        .into();
        let cwd = Path::new("C:/Users/me/Downloads");
        assert_eq!(bundles_in(&args, cwd), [cwd.join("Halo.DPGAME")]);
        let absolute = std::env::temp_dir().join("x.dpgame");
        let args = vec!["app".to_owned(), absolute.display().to_string()];
        assert_eq!(bundles_in(&args, cwd), [absolute]);
        assert!(bundles_in(&["app.dpgame".to_owned()], cwd).is_empty());
    }

    #[test]
    fn bundles_wait_until_the_page_takes_them() {
        let (a, b, c) = ("a.dpgame".into(), "b.dpgame".into(), "c.dpgame".into());
        let mut open = OpenFiles::new(vec![a]);
        // Opened again while the app starts: the page does not listen yet.
        assert_eq!(open.arrive(vec![b]), None);
        assert_eq!(
            open.take(),
            [PathBuf::from("a.dpgame"), PathBuf::from("b.dpgame")]
        );
        assert_eq!(open.arrive(vec![c]), Some(vec![PathBuf::from("c.dpgame")]));
        assert!(open.take().is_empty(), "each handed out once");
    }
}
