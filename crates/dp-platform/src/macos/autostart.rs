//! Starting a program when the user logs in: a launch agent in `~/Library/LaunchAgents`, which
//! launchd runs at the next login. macOS lists it under System Settings › General › Login
//! Items, where the user can also turn it off.

use std::io;
use std::path::{Path, PathBuf};

/// The agent's file for `label`.
fn plist(label: &str) -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("no home folder"))?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist")))
}

/// Whether the agent `label` is installed.
pub fn is_enabled(label: &str) -> bool {
    plist(label).is_ok_and(|p| p.is_file())
}

/// Run `program` with `args` when the user logs in.
pub fn enable(label: &str, program: &Path, args: &[&str]) -> io::Result<()> {
    let path = plist(label)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, agent(label, program, args))
}

/// Stop running the agent `label` at login. Not being installed is fine. A copy that runs now
/// keeps running.
pub fn disable(label: &str) -> io::Result<()> {
    match std::fs::remove_file(plist(label)?) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The property list of a launch agent that runs `program` with `args` once at login, in the
/// user's graphical session.
fn agent(label: &str, program: &Path, args: &[&str]) -> String {
    let program = program.display().to_string();
    let arguments: String = std::iter::once(program.as_str())
        .chain(args.iter().copied())
        .map(|a| format!("\t\t<string>{}</string>\n", escape(a)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         {arguments}\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>LimitLoadToSessionType</key>\n\
         \t<string>Aqua</string>\n\
         </dict>\n\
         </plist>\n",
        escape(label)
    )
}

/// Text as XML character data.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_lists_the_program_and_its_arguments() {
        let xml = agent(
            "dev.doppelpad.app",
            Path::new("/Applications/Doppelpad & Co.app/Contents/MacOS/doppelpad-gui"),
            &["--hidden"],
        );
        assert!(xml.contains("<string>dev.doppelpad.app</string>"));
        assert!(xml.contains(
            "\t\t<string>/Applications/Doppelpad &amp; Co.app/Contents/MacOS/doppelpad-gui</string>\n\
             \t\t<string>--hidden</string>\n"
        ));
        assert!(xml.contains("<key>RunAtLoad</key>\n\t<true/>"));
    }
}
