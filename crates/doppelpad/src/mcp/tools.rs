//! The tools an assistant calls: what each takes, what it says and does.

use std::path::{Path, PathBuf};

use dp_config::launches::{self, Launch};
use dp_config::library::{self, ConfigEntry};
use dp_config::{Diagnostics, ProfileSyntax, history};
use dp_core::profile::Profile;
use dp_ipc::{IpcError, Request, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::Server;

/// The profile reference, as the documentation has it.
const REFERENCE: &str = include_str!("../../../../docs/PROFILES.md");

/// What a tool says: its result, or why it did nothing, for the assistant to correct.
pub(super) type Outcome = Result<String, String>;

/// What the tools that need the service say when it does not run.
const NOT_RUNNING: &str =
    "Doppelpad is not running: the user starts the app (or `doppelpad daemon`) first.";

/// Where in the app the user adds a launch command.
const LAUNCH_IN_APP: &str = "the user adds them in the app (on the mapping: Commands, Launch \
    program…)";

/// The tools as `tools/list` describes them to the assistant.
pub(super) fn list() -> Value {
    let toml = json!({ "type": "string", "description": "The whole profile as TOML." });
    let path = json!({
        "type": "string",
        "description": "A config's path as list_profiles gives it: relative to the library, \
            such as `elden-ring/default.toml`.",
    });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "profile_reference",
            "title": "Profile reference",
            "description": "The reference for Doppelpad profiles (TOML): every control, \
                mapping mode, action, layer and setting, with examples, and every name a \
                profile can use. Read it before writing or changing a profile: names are exact.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        },
        {
            "name": "check_profile",
            "title": "Check a profile",
            "description": "Check a profile's TOML without saving it, as `doppelpad check` \
                does. Errors must be fixed; warnings point at likely mistakes. Each names the \
                field or value at fault. Check every profile before saving it.",
            "inputSchema": {
                "type": "object",
                "properties": { "toml": toml },
                "required": ["toml"],
            },
            "annotations": read_only,
        },
        {
            "name": "list_profiles",
            "title": "List the library",
            "description": "The user's library: each game with its configs and the \
                `processes` (programs) that switch to its `default` config, and loose configs, \
                which switch on their own `processes`. Paths are relative to the library, with \
                `/`, as read_profile, save_profile and apply_profile take them.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        },
        {
            "name": "read_profile",
            "title": "Read a config",
            "description": "A config from the library as TOML, with its check. A JSON profile \
                (from reWASD) comes as Doppelpad imports it.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": path },
                "required": ["path"],
            },
            "annotations": read_only,
        },
        {
            "name": "save_profile",
            "title": "Save a profile",
            "description": "Save a profile to the library, as the app saves it. With `path`, \
                it replaces that config, whose old version stays in the app's history; with \
                `game`, it becomes a new config of that game, which is made if the library has \
                none of that name. Programs in the profile's `processes` are added to its \
                game. A profile with errors is not saved. A profile cannot gain launch \
                commands (which start programs) this way: the user adds them in the app. \
                Those the replaced config had may stay.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "toml": toml,
                    "path": {
                        "type": "string",
                        "description": "The config to replace, as list_profiles gives it.",
                    },
                    "game": {
                        "type": "string",
                        "description": "The game to add a new config to: its name in \
                            list_profiles, or a new game's name.",
                    },
                },
                "required": ["toml"],
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": false,
                "openWorldHint": false,
            },
        },
        {
            "name": "apply_profile",
            "title": "Apply a config",
            "description": "Make the running Doppelpad use a config from the library now, for \
                the user to try. Without `slot` it runs instead of what runs now, until \
                something else is applied; with `slot` it goes into that slot (1 to 4), which \
                becomes the active one. Apply a config in use again after saving it, to load \
                the new version. Configs that start programs are applied in the app.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path,
                    "slot": { "type": "integer", "minimum": 1, "maximum": dp_service::SLOTS },
                },
                "required": ["path"],
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            },
        },
        {
            "name": "status",
            "title": "Status",
            "description": "What Doppelpad does now: the profile in use and its layer, the \
                slots, whether remapping is on, and the connected controllers, whose names \
                tell which controls they have (a touchpad, gyro, back paddles).",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        },
    ])
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    toml: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    toml: String,
    path: Option<PathBuf>,
    game: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    path: PathBuf,
    slot: Option<usize>,
}

/// A tool's arguments, or what is wrong with them.
fn args<T: DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("wrong arguments: {e}"))
}

impl Server {
    /// Run the tool `name`; `None` when there is none of that name.
    pub(super) fn tool(&self, name: &str, a: Value) -> Option<Outcome> {
        Some(match name {
            "profile_reference" => Ok(REFERENCE.replace("\r\n", "\n")),
            "check_profile" => args(a).map(|a: Check| check(&a.toml)),
            "list_profiles" => Ok(self.list_profiles()),
            "read_profile" => args(a).and_then(|a: Read| self.read_profile(&a.path)),
            "save_profile" => args(a).and_then(|a| self.save_profile(a)),
            "apply_profile" => args(a).and_then(|a| self.apply_profile(a)),
            "status" => self.status(),
            _ => return None,
        })
    }

    fn list_profiles(&self) -> String {
        let root = (self.library)();
        let lib = library::scan(&root);
        let games: Vec<Value> = lib
            .games
            .iter()
            .map(|g| {
                let default = g.default_config().map(|c| &c.path);
                let configs: Vec<Value> = g
                    .configs
                    .iter()
                    .map(|c| {
                        let mut v = config(&root, c);
                        if default == Some(&c.path) {
                            v["default"] = json!(true);
                        }
                        v
                    })
                    .collect();
                json!({
                    "game": g.meta.name,
                    "folder": shown(&root, &g.dir),
                    "processes": g.meta.processes,
                    "configs": configs,
                })
            })
            .collect();
        let mut out = json!({ "library": root, "games": games });
        if !lib.unsorted.is_empty() {
            let loose = lib.unsorted.iter().map(|c| {
                let mut v = config(&root, c);
                v["processes"] = json!(c.processes);
                v
            });
            out["loose_configs"] = loose.collect();
        }
        if !lib.errors.is_empty() {
            out["unreadable"] = json!(lib.errors);
        }
        format!("{out:#}")
    }

    fn read_profile(&self, path: &Path) -> Outcome {
        let path = listed_config(&(self.library)(), path)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Ok(match dp_config::load_with_format(&path) {
            Ok((profile, diags, ProfileSyntax::Toml)) => {
                format!(
                    "```toml\n{}\n```\n\n{}",
                    text.trim_end(),
                    report(profile, &diags)
                )
            }
            Ok((profile, diags, ProfileSyntax::Json)) => {
                let toml = dp_config::to_toml(&profile).map_err(|e| e.to_string())?;
                format!(
                    "A JSON profile, here as Doppelpad imports it. It cannot be saved over: \
                     save_profile with `game` adds it as a new config.\n\n```toml\n{}\n```\n\n{}",
                    toml.trim_end(),
                    report(profile, &diags)
                )
            }
            Err(e) => format!("```\n{}\n```\n\nerror: {e}", text.trim_end()),
        })
    }

    fn save_profile(&self, a: Save) -> Outcome {
        let mut profile = dp_config::parse_toml(&a.toml)
            .map_err(|e| format!("error: {}\nNothing was saved.", e.to_string().trim_end()))?;
        let diags = dp_config::validate(&profile);
        if diags.has_errors() {
            return Err(format!(
                "{}\nNothing was saved: fix the errors first.",
                lines(&diags)
            ));
        }
        let root = (self.library)();
        let target = match (a.path, a.game) {
            (Some(path), None) => Target::Replace(listed_config(&root, &path)?),
            (None, Some(game)) if !game.trim().is_empty() => Target::Add(game.trim().to_owned()),
            _ => {
                return Err(
                    "Give either `path`, a config to replace, or `game`, a game to add a new \
                     config to."
                        .into(),
                );
            }
        };
        // A config that does not load has no launch commands to keep, nor a name or programs.
        let old = match &target {
            Target::Replace(path) => dp_config::load(path).ok().map(|(p, _)| p),
            Target::Add(_) => None,
        };
        let had: Vec<String> = old
            .iter()
            .flat_map(launches::find)
            .map(|l| l.command_line())
            .collect();
        let new: Vec<Launch> = launches::find(&profile)
            .into_iter()
            .filter(|l| !had.contains(&l.command_line()))
            .collect();
        if !new.is_empty() {
            return Err(format!(
                "This profile starts programs: {}. Nothing was saved: launch commands do not \
                 come in through an assistant; {LAUNCH_IN_APP}.",
                list_launches(&new)
            ));
        }
        let listed_before = old.as_ref().map_or_else(Vec::new, |p| p.processes.clone());
        let mut out = Vec::new();
        let path = match target {
            Target::Replace(path) => {
                if profile.name.is_empty() {
                    profile.name = old.map(|p| p.name).unwrap_or_default();
                }
                history::save_profile(&path, &profile, &self.history, &root)
                    .map_err(|e| e.to_string())?;
                path
            }
            Target::Add(game) => {
                if profile.name.is_empty() {
                    profile.name.clone_from(&game);
                }
                match find_game(&root, &game) {
                    Some(dir) => {
                        library::create_config_with(&dir, &profile).map_err(|e| e.to_string())?
                    }
                    None => {
                        let new = library::create_game_with(&root, &game, &profile)
                            .map_err(|e| e.to_string())?;
                        out.push(format!("Made the game \"{game}\"."));
                        let first = new.configs.into_iter().next().map(|c| c.path);
                        first.ok_or("the new game has no config")?
                    }
                }
            }
        };
        out.push(format!(
            "Saved \"{}\" as {}; apply_profile applies it.",
            profile.name,
            shown(&root, &path)
        ));
        if let Some(dir) = path.parent() {
            out.extend(adopt_programs(dir, &profile.processes, &listed_before));
        }
        // The service lists the library anew, as after a save in the app.
        let _ = (self.service)(&Request::Reload);
        out.extend(diags.iter().map(ToString::to_string));
        Ok(out.join("\n"))
    }

    fn apply_profile(&self, a: Apply) -> Outcome {
        let path = listed_config(&(self.library)(), &a.path)?;
        let (profile, diags) = dp_config::load(&path).map_err(|e| e.to_string())?;
        if diags.has_errors() {
            return Err(format!(
                "{}\nIt has errors, so it was not applied.",
                lines(&diags)
            ));
        }
        let starts = launches::find(&profile);
        if !starts.is_empty() {
            return Err(format!(
                "It starts programs ({}), so the user applies it, in the app.",
                list_launches(&starts)
            ));
        }
        let path = crate::profile_path(&path).map_err(|e| format!("{e:#}"))?;
        let request = match a.slot {
            None => Request::Apply { path },
            Some(slot) if (1..=dp_service::SLOTS).contains(&slot) => Request::ApplyToSlot {
                path,
                slot,
                device: None,
            },
            Some(slot) => {
                return Err(format!(
                    "there is no slot {slot}; they are 1 to {}",
                    dp_service::SLOTS
                ));
            }
        };
        match (self.service)(&request) {
            Ok(Response::Ok { message }) => Ok(message),
            Ok(Response::Error { message }) => Err(message),
            Ok(other) => Err(format!("unexpected answer: {other:?}")),
            Err(e) => Err(service_error(e)),
        }
    }

    fn status(&self) -> Outcome {
        let status = match (self.service)(&Request::Status) {
            Ok(Response::Status(s)) => s,
            Ok(other) => return Err(format!("unexpected answer: {other:?}")),
            Err(IpcError::NotRunning(_)) => return Ok(NOT_RUNNING.into()),
            Err(e) => return Err(service_error(e)),
        };
        let controllers: Vec<Value> = match (self.service)(&Request::Devices) {
            Ok(Response::Devices { devices }) => devices
                .iter()
                .map(|d| json!({ "id": d.id, "name": d.name, "backend": d.backend }))
                .collect(),
            _ => Vec::new(),
        };
        // Configs in the library as the other tools name them.
        let root = (self.library)();
        let mut status = json!(status);
        let short = |v: &mut Value| {
            if let Some(path) = v.as_str() {
                *v = json!(shown(&root, Path::new(path)));
            }
        };
        short(&mut status["profile_path"]);
        status["slots"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .for_each(short);
        Ok(format!(
            "{:#}",
            json!({ "status": status, "controllers": controllers })
        ))
    }
}

/// Where save_profile puts a profile.
enum Target {
    /// Over this config.
    Replace(PathBuf),
    /// As a new config of the game of this name.
    Add(String),
}

/// The config `path` (relative to the library `root`, or whole) as the library lists it, or why
/// there is none: the tools touch no other file, not even a game's own `game.toml`.
fn listed_config(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let full = root.join(path);
    let lib = library::scan(root);
    lib.games
        .iter()
        .flat_map(|g| &g.configs)
        .chain(&lib.unsorted)
        .find(|c| crate::same_file(&c.path, &full))
        .map(|c| c.path.clone())
        .ok_or_else(|| {
            format!(
                "`{}` is no config in the library; list_profiles lists them",
                path.display()
            )
        })
}

/// `path` as the tools show it: relative to the library `root`, with `/` on every system, or
/// whole when it lies outside.
fn shown(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(inside) => inside
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

/// The folder of the game called `name` (or kept in a folder of that name), if there is one.
fn find_game(root: &Path, name: &str) -> Option<PathBuf> {
    library::scan(root)
        .games
        .into_iter()
        .find(|g| {
            g.meta.name.eq_ignore_ascii_case(name)
                || g.dir
                    .file_name()
                    .is_some_and(|f| f.eq_ignore_ascii_case(name))
        })
        .map(|g| g.dir)
}

/// In a game folder `dir`, the game's programs switch to its default config, and a config's own
/// `processes` do nothing. So the programs a saved profile lists anew (not in `before`, the
/// replaced config's) go to the game. What to tell the assistant about it, if anything.
fn adopt_programs(dir: &Path, processes: &[String], before: &[String]) -> Option<String> {
    // Not a game: a loose config, which switches on its own `processes`.
    let mut meta = library::read_game_meta(dir).ok()?;
    let known = |list: &[String], p: &str| list.iter().any(|q| library::same_program(q, p));
    let new: Vec<String> = processes
        .iter()
        .filter(|p| !known(before, p) && !known(&meta.processes, p))
        .cloned()
        .collect();
    if new.is_empty() {
        return None;
    }
    meta.processes.extend(new.iter().cloned());
    let programs = new.join(" or ");
    if let Err(e) = library::update_game_meta(dir, &meta) {
        return Some(format!(
            "warning: {programs} could not be added to the game's programs: {e}"
        ));
    }
    let default = library::read_game(dir)
        .ok()
        .and_then(|g| g.default_config().map(|c| c.name.clone()))
        .unwrap_or_default();
    Some(format!(
        "\"{}\" now switches to its default config, \"{default}\", while {programs} is in front.",
        meta.name
    ))
}

/// A config as list_profiles shows it: its name and path, and why it does not load.
fn config(root: &Path, c: &ConfigEntry) -> Value {
    let mut v = json!({ "name": c.name, "path": shown(root, &c.path) });
    if let Some(e) = &c.error {
        v["error"] = json!(e);
    }
    v
}

/// Check a profile's TOML: what `doppelpad check` says about it.
fn check(src: &str) -> String {
    match dp_config::parse_toml(src) {
        Ok(profile) => {
            let diags = dp_config::validate(&profile);
            report(profile, &diags)
        }
        Err(e) => format!(
            "error: {}\nThe profile does not load; nothing else was checked.",
            e.to_string().trim_end()
        ),
    }
}

/// A profile's diagnostics and the engine's warnings, the programs it starts, a summary, and
/// whether it has errors.
fn report(profile: Profile, diags: &Diagnostics) -> String {
    let mut out: Vec<String> = diags.iter().map(ToString::to_string).collect();
    let engine = dp_core::engine::Engine::new(profile);
    out.extend(engine.warnings().iter().map(|w| format!("warning: {w}")));
    let starts = launches::find(engine.profile());
    if !starts.is_empty() {
        out.push(format!(
            "note: it starts programs ({}); save_profile keeps only those the config already \
             had, and apply_profile leaves it to the app",
            list_launches(&starts)
        ));
    }
    out.push(crate::summary(engine.profile()));
    out.push(
        if diags.has_errors() {
            "It has errors."
        } else {
            "No errors."
        }
        .into(),
    );
    out.join("\n")
}

/// Diagnostics one per line, as `doppelpad check` prints them.
fn lines(diags: &Diagnostics) -> String {
    diags
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `main layer, A: notepad.exe; …`
fn list_launches(found: &[Launch]) -> String {
    found
        .iter()
        .map(|l| format!("{}: {}", l.place, l.command_line()))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why the service did not answer.
fn service_error(e: IpcError) -> String {
    match e {
        IpcError::NotRunning(_) => NOT_RUNNING.into(),
        e => e.to_string(),
    }
}
