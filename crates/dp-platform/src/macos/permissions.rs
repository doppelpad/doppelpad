//! The privacy permission keyboard and mouse need on macOS: Accessibility, which lets a
//! program send input and change other programs' input (an active event tap). The user grants
//! it in System Settings › Privacy & Security › Accessibility, per program: for the
//! command-line tool it goes to the terminal it runs in.

use objc2_core_graphics::{CGPreflightPostEventAccess, CGRequestPostEventAccess};

/// Whether Doppelpad has the Accessibility permission.
pub fn granted() -> bool {
    CGPreflightPostEventAccess()
}

/// Ask for the permission if it is missing: macOS shows its prompt, which leads to System
/// Settings, once per program.
pub fn request() {
    if !granted() {
        CGRequestPostEventAccess();
    }
}
