//! Commands the web frontend invokes. The fallible ones fail with a [`CmdError`], a code
//! the window words in its language and a detail. Plain commands run on the main thread,
//! which also runs the windows and the tray; the ones that touch the disk or wait for the
//! service are `async` and run on Tauri's async runtime instead.
//!
//! - [`library`]: games, configs, presets and bundles on disk, and where the library is;
//! - [`service`]: what the remapping service does (slots, devices, hotkeys, live state);
//! - [`settings`]: the app's own settings;
//! - [`system`]: names and defaults, the OS (programs, stores, file manager, drivers), reWASD,
//!   and what the app was started with.

pub mod library;
pub mod service;
pub mod settings;
pub mod system;

pub use crate::error::{CmdError, CmdResult, ErrorCode, err};
