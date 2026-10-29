//! Loading, saving and validating Doppelpad profiles, and importing JSON profiles
//! (schema 3.0).

pub mod bundle;
mod compat;
mod diag;
mod dirs;
mod format;
pub mod history;
pub mod launches;
pub mod library;
pub mod picture;
pub mod relocate;
pub mod store;
pub mod templates;
mod validate;

use std::path::{Path, PathBuf};

pub use compat::ImportError;
pub use diag::{Diagnostic, Diagnostics, Severity};
pub use dirs::{
    CALIBRATION_FILE, HOTKEYS_FILE, PORTABLE_MARKER, STATE_FILE, config_dir, default_profiles_dir,
    history_dir, log_dir, portable_dir, profiles_dir,
};
use dp_core::profile::Profile;
pub use format::{FileFormat, Migration};
pub use store::{
    Loaded, SaveError, load_toml_or_default, save_toml, save_toml_with_header, set_aside,
    write_atomic,
};
pub use validate::validate;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Toml(PathBuf, #[source] toml::de::Error),
    #[error("{0}: {1}")]
    Import(PathBuf, #[source] ImportError),
}

/// Profile file formats.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    /// Native Doppelpad TOML.
    Toml,
    /// JSON profile (schema 3.0), imported.
    Json,
}

fn is_toml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
}

impl Format {
    /// `.toml` files are native profiles; any other file whose content is a JSON
    /// object is imported, whatever its extension.
    pub fn detect(path: &Path, content: &str) -> Format {
        if looks_like_json(content) && !is_toml_path(path) {
            Format::Json
        } else {
            Format::Toml
        }
    }
}

/// Whether `text` starts with a JSON object (after a byte order mark and white space).
pub(crate) fn looks_like_json(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('{')
}

/// The format of TOML profiles. A change that older profiles do not survive as they are
/// (a renamed, moved or reinterpreted field) adds a migration here.
pub const PROFILE_FORMAT: FileFormat = FileFormat::new(&[]);

/// Parse a native TOML profile, bringing one of an older format up to date.
pub fn parse_toml(src: &str) -> Result<Profile, toml::de::Error> {
    PROFILE_FORMAT.parse(src)
}

/// Serialize a profile as TOML. Arrays of values stay on one line (`trigger = ["LB", "RB"]`,
/// `curve = [[0.3, 0.15], [0.7, 0.55]]`) like in hand-written profiles; the "pretty" style
/// would put every element, even of a curve point, on a line of its own.
pub fn to_toml(p: &Profile) -> Result<String, toml::ser::Error> {
    let body = toml::to_string(p)?;
    Ok(format!(
        "{PROFILE_HEADER} — {}\n{}",
        env!("CARGO_PKG_REPOSITORY"),
        PROFILE_FORMAT.stamp(&body)
    ))
}

/// How every profile Doppelpad writes starts; the library knows its own files by it.
pub(crate) const PROFILE_HEADER: &str = "# Doppelpad profile";

/// Load a profile from disk, importing JSON profiles. The diagnostics include
/// import notes and [`validate`] results.
pub fn load(path: &Path) -> Result<(Profile, Diagnostics), LoadError> {
    load_with_format(path).map(|(profile, diags, _)| (profile, diags))
}

/// [`load`], also telling which format the file is in.
pub fn load_with_format(path: &Path) -> Result<(Profile, Diagnostics, Format), LoadError> {
    let src = std::fs::read_to_string(path).map_err(|e| LoadError::Io(path.into(), e))?;
    parse_with_format(path, &src)
}

/// [`load_with_format`] for `src`, the text of the file `path`.
pub(crate) fn parse_with_format(
    path: &Path,
    src: &str,
) -> Result<(Profile, Diagnostics, Format), LoadError> {
    let format = Format::detect(path, src);
    let (mut profile, mut diags) = match format {
        Format::Toml => (
            parse_toml(src).map_err(|e| LoadError::Toml(path.into(), e))?,
            Diagnostics::default(),
        ),
        Format::Json => compat::import(src).map_err(|e| LoadError::Import(path.into(), e))?,
    };
    // An import without a name of its own (the importer says "Game") is named after its file,
    // as the library names it.
    let unnamed = format == Format::Json && profile.name == compat::DEFAULT_IMPORT_NAME;
    if profile.name.is_empty() || unnamed {
        profile.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile")
            .to_owned();
    }
    diags.0.extend(validate(&profile).0);
    Ok((profile, diags, format))
}

/// Whether a file looks like a profile: `.toml`, or content starting with a JSON object.
pub(crate) fn is_profile_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if is_toml_path(path) {
        return true;
    }
    let mut head = [0u8; 64];
    let n = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .unwrap_or(0);
    looks_like_json(&String::from_utf8_lossy(&head[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_through_to_toml() {
        let src = "name = \"x\"\n[[mappings]]\ntrigger = [\"LB\", \"RB\"]\nactions = [{ key = \"Escape\" }]\n";
        let p = parse_toml(src).unwrap();
        let text = to_toml(&p).unwrap();
        assert!(
            text.starts_with("# Doppelpad profile — https://github.com/doppelpad/doppelpad\n"),
            "{text}"
        );
        let again = parse_toml(&text).unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn to_toml_keeps_value_arrays_on_one_line() {
        let src = r#"
name = "x"
processes = ["a.exe", "b.exe"]

[sticks.right]
curve = [[0.3, 0.15], [0.7, 0.55]]

[[mappings]]
trigger = ["LB", "RB"]
actions = [{ key = "Escape" }, { pause = 20 }]

[[analog]]
source = "gyro"
target = "right_stick"
while_held = ["LT", "Mouse:Right"]
"#;
        let p = parse_toml(src).unwrap();
        let text = to_toml(&p).unwrap();
        for line in [
            r#"processes = ["a.exe", "b.exe"]"#,
            "curve = [[0.3, 0.15], [0.7, 0.55]]",
            r#"trigger = ["LB", "RB"]"#,
            r#"while_held = ["LT", "Mouse:Right"]"#,
            "[[mappings]]",
            "[[mappings.actions]]",
            "[[analog]]",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing `{line}` in\n{text}"
            );
        }
        assert_eq!(parse_toml(&text).unwrap(), p);
    }

    #[test]
    fn key_repeat_names_and_milliseconds() {
        use dp_core::profile::KeyRepeat;
        let p = parse_toml("[virtual.keyboard]\nrepeat = \"fast\"\n").unwrap();
        assert_eq!(p.virtual_devices.keyboard.repeat, KeyRepeat::Fast);
        let p = parse_toml("[virtual.keyboard]\nrepeat = 40\n").unwrap();
        assert_eq!(p.virtual_devices.keyboard.repeat, KeyRepeat::Every(40));
        assert!(to_toml(&p).unwrap().contains("repeat = 40"));
        assert_eq!(parse_toml(&to_toml(&p).unwrap()).unwrap(), p);
        assert!(parse_toml("[virtual.keyboard]\nrepeat = 0\n").is_err());
        assert!(parse_toml("[virtual.keyboard]\nrepeat = \"turbo\"\n").is_err());
        assert!(
            !to_toml(&Profile::default()).unwrap().contains("virtual"),
            "native is the default"
        );
    }

    #[test]
    fn format_detection() {
        let json = "\u{feff}  {\"schemaVersion\": 3.0}";
        assert_eq!(Format::detect(Path::new("game.cfg"), json), Format::Json);
        assert_eq!(Format::detect(Path::new("game.json"), json), Format::Json);
        assert_eq!(
            Format::detect(Path::new("a.TOML"), "name = \"x\""),
            Format::Toml
        );
        assert_eq!(
            Format::detect(Path::new("noext"), "name = \"x\""),
            Format::Toml
        );
    }
}
