//! Game bundles handed to the app from Explorer or the Finder. On Windows and Linux a
//! double-clicked `.dpgame` arrives in the command line, at start-up or (while the app runs)
//! through the single-instance handler; macOS sends it to the running app as an event.

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Emitter, Manager};

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

/// Files the Finder asks the app to open: the bundles among them go to the window, or wait
/// for it while its page has not asked for the files yet (it may still be loading).
#[cfg(target_os = "macos")]
pub fn opened(app: &AppHandle, files: Vec<PathBuf>) {
    let files: Vec<PathBuf> = files.into_iter().filter(|p| is_bundle(p)).collect();
    if files.is_empty() {
        return;
    }
    crate::main_window::show_main(app);
    let state = app.state::<crate::state::AppState>();
    let mut waiting = crate::state::lock(&state.open_files);
    if state.open_files_taken.load(Ordering::Relaxed) {
        let _ = app.emit_to(crate::main_window::MAIN_WINDOW, OPEN_EVENT, files);
    } else {
        waiting.extend(files);
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
}
