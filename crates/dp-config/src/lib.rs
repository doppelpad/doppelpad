//! Loading, saving and validating Doppelpad profiles, and importing JSON profiles
//! (schema 3.0).

pub mod compat;
pub mod diag;
pub mod library;

use std::path::{Path, PathBuf};

pub use compat::ImportError;
pub use diag::{Diagnostic, Diagnostics, Severity};
use dp_core::profile::{Action, Command, Profile};

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
        let json = content
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('{');
        if json && !is_toml_path(path) {
            Format::Json
        } else {
            Format::Toml
        }
    }
}

/// Parse a native TOML profile.
pub fn parse_toml(src: &str) -> Result<Profile, toml::de::Error> {
    toml::from_str(src)
}

/// Serialize a profile as TOML. Arrays of values stay on one line (`trigger = ["LB", "RB"]`,
/// `curve = [[0.3, 0.15], [0.7, 0.55]]`) like in hand-written profiles; the "pretty" style
/// would put every element, even of a curve point, on a line of its own.
pub fn to_toml(p: &Profile) -> String {
    let body = toml::to_string(p).unwrap_or_else(|e| format!("# serialization failed: {e}\n"));
    format!("# Doppelpad profile — https://github.com/doppelpad/doppelpad\n{body}")
}

/// Load a profile from disk, importing JSON profiles. The diagnostics include
/// import notes and [`validate`] results.
pub fn load(path: &Path) -> Result<(Profile, Diagnostics), LoadError> {
    let src = std::fs::read_to_string(path).map_err(|e| LoadError::Io(path.into(), e))?;
    let (mut profile, mut diags) = match Format::detect(path, &src) {
        Format::Toml => (
            parse_toml(&src).map_err(|e| LoadError::Toml(path.into(), e))?,
            Diagnostics::default(),
        ),
        Format::Json => compat::import(&src).map_err(|e| LoadError::Import(path.into(), e))?,
    };
    if profile.name.is_empty() {
        profile.name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile")
            .to_owned();
    }
    diags.0.extend(validate(&profile).0);
    Ok((profile, diags))
}

/// Check a profile for mistakes the engine would silently tolerate.
pub fn validate(p: &Profile) -> Diagnostics {
    let mut d = Diagnostics::default();
    let declared: Vec<u8> = p.layers.iter().map(|l| l.id).collect();
    for (i, l) in p.layers.iter().enumerate() {
        if l.id == 0 {
            d.error(
                format!("layers[{i}]"),
                "layer 0 is the base layer and must not be declared",
            );
        }
        if p.layers.iter().filter(|o| o.id == l.id).count() > 1 {
            d.error(
                format!("layers[{i}]"),
                format!("layer {} is declared twice", l.id),
            );
        }
    }
    for (i, m) in p.mappings.iter().enumerate() {
        let path = format!("mappings[{i}] ({})", m.trigger);
        if m.trigger.0.len() > 4 {
            d.warn(
                &path,
                "combos with more than four inputs are hard to press reliably",
            );
        }
        if m.layer != 0 && !declared.contains(&m.layer) {
            d.info(
                &path,
                format!(
                    "layer {} is not declared; it inherits the base layer",
                    m.layer
                ),
            );
        }
        if let Some(j) = &m.jump
            && j.layer != 0
            && !declared.contains(&j.layer)
            && !p.mappings.iter().any(|o| o.layer == j.layer)
        {
            d.warn(
                &path,
                format!("jumps to layer {} which has no mappings", j.layer),
            );
        }
        for a in &m.actions {
            match a {
                Action::Pause { pause } if *pause > 60_000 => {
                    d.warn(&path, "pause longer than a minute")
                }
                Action::Command {
                    command: Command::Unsupported(name),
                } => d.warn(
                    &path,
                    format!("command `{name}` is not supported and does nothing"),
                ),
                Action::Stick { x, y, .. } if x.abs() > 1.0 || y.abs() > 1.0 => {
                    d.warn(&path, "stick deflection outside [-1, 1] is clamped")
                }
                _ => {}
            }
        }
        if m.actions.len() > 4096 {
            d.error(&path, "macro has more than 4096 steps");
        }
    }
    for (name, s) in [("left", &p.sticks.left), ("right", &p.sticks.right)] {
        if !(0.0..1.0).contains(&s.deadzone) {
            d.error(format!("sticks.{name}.deadzone"), "must be in [0, 1)");
        }
        if s.curve
            .iter()
            .any(|pt| !(0.0..=1.0).contains(&pt[0]) || !(0.0..=1.0).contains(&pt[1]))
        {
            d.error(format!("sticks.{name}.curve"), "points must be in [0, 1]");
        }
        let [low, med, high] = s.zone_thresholds();
        let problem = zone_problem([low.x, med.x, high.x]).or(zone_problem([low.y, med.y, high.y]));
        if let Some(msg) = problem {
            d.error(format!("sticks.{name}.zones"), msg);
        }
    }
    for (name, t) in [("left", &p.triggers.left), ("right", &p.triggers.right)] {
        if let Some(msg) = zone_problem(t.zone_thresholds()) {
            d.error(format!("triggers.{name}.zones"), msg);
        }
    }
    d
}

/// What is wrong with a control's effective low, medium and high zone thresholds.
fn zone_problem([low, med, high]: [f32; 3]) -> Option<&'static str> {
    if !(low > 0.0 && high <= 1.0) {
        Some("thresholds must be in (0, 1]")
    } else if !(low < med && med < high) {
        Some("low, med and high must increase")
    } else {
        None
    }
}

/// Per-user configuration directory (`%APPDATA%\doppelpad` or `~/.config/doppelpad`).
pub fn config_dir() -> PathBuf {
    let base = if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata)
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        return PathBuf::from("doppelpad");
    };
    base.join("doppelpad")
}

/// Default directory for profiles.
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Per-user directory for log files: `%LOCALAPPDATA%\doppelpad\logs`, or
/// `$XDG_STATE_HOME/doppelpad/logs` (default `~/.local/state/doppelpad/logs`). Local
/// rather than roaming: logs describe this machine.
pub fn log_dir() -> PathBuf {
    let base = if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local)
    } else if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local").join("state")
    } else {
        return PathBuf::from("doppelpad").join("logs");
    };
    base.join("doppelpad").join("logs")
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
    let text = String::from_utf8_lossy(&head[..n]);
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('{')
}

/// All loadable profiles in `dir` (TOML and importable JSON), with load errors reported
/// separately.
pub fn list_profiles(dir: &Path) -> (Vec<(PathBuf, Profile)>, Vec<LoadError>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (ok, errors);
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if !is_profile_file(&path) {
            continue;
        }
        match load(&path) {
            Ok((p, _)) => ok.push((path, p)),
            Err(e) => errors.push(e),
        }
    }
    (ok, errors)
}

/// The profile whose `processes` list contains `exe` (case-insensitive).
pub fn profile_for_process<'a>(
    profiles: &'a [(PathBuf, Profile)],
    exe: &str,
) -> Option<&'a (PathBuf, Profile)> {
    profiles
        .iter()
        .find(|(_, p)| p.processes.iter().any(|n| n.eq_ignore_ascii_case(exe)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_through_to_toml() {
        let src = "name = \"x\"\n[[mappings]]\ntrigger = [\"LB\", \"RB\"]\nactions = [{ key = \"Escape\" }]\n";
        let p = parse_toml(src).unwrap();
        let again = parse_toml(&to_toml(&p)).unwrap();
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
        let text = to_toml(&p);
        for line in [
            r#"processes = ["a.exe", "b.exe"]"#,
            "curve = [[0.3, 0.15], [0.7, 0.55]]",
            r#"trigger = ["LB", "RB"]"#,
            r#"while_held = ["LT", "Mouse:Right"]"#,
            "[[mappings]]",
            "[[mappings.actions]]",
            "[[analog]]",
        ] {
            assert!(text.lines().any(|l| l == line), "missing `{line}` in\n{text}");
        }
        assert_eq!(parse_toml(&text).unwrap(), p);
    }

    #[test]
    fn validation_flags_bad_layers() {
        let p = parse_toml("[[layers]]\nid = 0\n").unwrap();
        assert!(validate(&p).has_errors());
    }

    #[test]
    fn validation_checks_zone_order() {
        let ok = "[sticks.left]\nzones = { low = 0.2, med = [0.4, 0.5] }\n[triggers.left]\nzones = { low = 0.1, high = 0.9 }\n";
        assert!(!validate(&parse_toml(ok).unwrap()).has_errors());
        for bad in [
            "[sticks.left]\nzones = { low = 0.5, med = 0.4 }\n",
            "[sticks.right]\nzones = { low = 0.2, high = [0.9, 0.3] }\n",
            "[triggers.right]\nzones = { low = 0.0 }\n",
            "[triggers.right]\nzones = { low = 0.1, med = 0.5, high = 1.2 }\n",
        ] {
            assert!(validate(&parse_toml(bad).unwrap()).has_errors(), "{bad}");
        }
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

    #[test]
    fn process_matching() {
        let mut p = Profile::default();
        p.processes.push("Game.exe".into());
        let list = vec![(PathBuf::from("g.toml"), p)];
        assert!(profile_for_process(&list, "game.EXE").is_some());
        assert!(profile_for_process(&list, "other.exe").is_none());
    }
}
