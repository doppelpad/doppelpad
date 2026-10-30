//! What the OSes' system commands ([`crate::SystemControl`]) share: running a system tool, and
//! naming a screenshot.

#![cfg_attr(
    not(any(windows, target_os = "linux", target_os = "macos")),
    allow(dead_code)
)]

use std::path::{Path, PathBuf};

use crate::{PlatformError, Result};

/// Run a system tool and wait for it; an error when it cannot start or fails.
#[cfg_attr(windows, allow(dead_code))] // Windows asks its APIs
pub(crate) fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = crate::host_command(program)
        .args(args)
        .status()
        .map_err(|source| PlatformError::Os {
            context: "starting a system tool",
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Other(format!(
            "`{program} {}` failed",
            args.join(" ")
        )))
    }
}

/// A screenshot's file in `dir`: `<app> <stamp>.png` (the program in front, without `.exe`,
/// else `Screenshot`), numbered if that exists already. `stamp` is the local time as
/// `2026-10-20 17-30-05`.
#[cfg_attr(target_os = "linux", allow(dead_code))] // no screenshots there yet
pub(crate) fn screenshot_file(dir: &Path, app: Option<&str>, stamp: &str) -> PathBuf {
    let app = app
        .map(|a| a.trim_end_matches(".exe").trim_end_matches(".EXE"))
        .filter(|a| !a.is_empty())
        .unwrap_or("Screenshot");
    let mut path = dir.join(format!("{app} {stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{app} {stamp} ({n}).png"));
        n += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_names() {
        let dir = tempfile::tempdir().unwrap();
        let stamp = "2026-10-20 17-30-05";
        let first = screenshot_file(dir.path(), Some("Game.exe"), stamp);
        assert_eq!(first, dir.path().join("Game 2026-10-20 17-30-05.png"));
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            screenshot_file(dir.path(), Some("Game.exe"), stamp),
            dir.path().join("Game 2026-10-20 17-30-05 (2).png")
        );
        assert_eq!(
            screenshot_file(dir.path(), None, stamp),
            dir.path().join("Screenshot 2026-10-20 17-30-05.png")
        );
    }
}
