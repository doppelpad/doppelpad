//! Installed games of the local game stores (Steam, Epic, GOG), from their own files only (no
//! network), to suggest games and their programs. The file formats and the stores' folders are
//! read here; where the stores keep them is per OS (`windows::stores`, `macos::stores`).

use std::path::{Path, PathBuf};

/// The store a game was installed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    Steam,
    Epic,
    Gog,
}

/// An installed game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledGame {
    pub store: Store,
    pub name: String,
    /// The game's folder.
    pub dir: PathBuf,
    /// Programs that may be the game's, the likeliest first (file names).
    pub programs: Vec<String>,
    /// A cover picture from the store's local cache.
    pub cover: Option<PathBuf>,
}

/// A value of Valve's KeyValues text format (`.vdf`, `.acf`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vdf {
    Str(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    /// The value of `key` in a map (case-insensitive, as Steam's own files mix cases).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Map(entries) => entries,
            Vdf::Str(_) => &[],
        }
    }
}

/// Parse KeyValues text: quoted keys and values, `{ … }` for maps, `//` comments. The result is
/// the top level as a map; malformed input ends the map where it breaks.
pub fn parse_vdf(text: &str) -> Vdf {
    let mut tokens = Tokens {
        chars: text.chars().peekable(),
    };
    Vdf::Map(parse_map(&mut tokens))
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

enum Token {
    Str(String),
    Open,
    Close,
}

impl Iterator for Tokens<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        loop {
            match self.chars.next()? {
                c if c.is_whitespace() => {}
                '/' if self.chars.peek() == Some(&'/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                '{' => return Some(Token::Open),
                '}' => return Some(Token::Close),
                '"' => {
                    let mut s = String::new();
                    while let Some(c) = self.chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match self.chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(other) => s.push(other),
                                None => break,
                            },
                            _ => s.push(c),
                        }
                    }
                    return Some(Token::Str(s));
                }
                c => {
                    // An unquoted word.
                    let mut s = c.to_string();
                    while let Some(&c) = self.chars.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        s.push(c);
                        self.chars.next();
                    }
                    return Some(Token::Str(s));
                }
            }
        }
    }
}

fn parse_map(tokens: &mut Tokens) -> Vec<(String, Vdf)> {
    let mut out = Vec::new();
    while let Some(t) = tokens.next() {
        let key = match t {
            Token::Str(k) => k,
            Token::Close | Token::Open => break,
        };
        match tokens.next() {
            Some(Token::Str(v)) => out.push((key, Vdf::Str(v))),
            Some(Token::Open) => out.push((key, Vdf::Map(parse_map(tokens)))),
            _ => break,
        }
    }
    out
}

/// The library folders of a Steam installation (`steamapps/libraryfolders.vdf`).
pub fn steam_libraries(libraryfolders: &str) -> Vec<PathBuf> {
    let vdf = parse_vdf(libraryfolders);
    let Some(root) = vdf.get("libraryfolders") else {
        return Vec::new();
    };
    root.entries()
        .iter()
        .filter_map(|(_, lib)| lib.get("path").and_then(Vdf::str).map(PathBuf::from))
        .collect()
}

/// Steam's tools that are not games.
const STEAM_TOOLS: [&str; 4] = ["228980", "1070560", "1391110", "1628350"];

/// The app id, name and install folder name of an installed Steam app (`appmanifest_*.acf`),
/// `None` for tools and runtimes.
pub fn steam_app(acf: &str) -> Option<(String, String, String)> {
    let vdf = parse_vdf(acf);
    let app = vdf.get("AppState")?;
    let id = app.get("appid")?.str()?.to_owned();
    let name = app.get("name")?.str()?.to_owned();
    let dir = app.get("installdir")?.str()?.to_owned();
    let tool = STEAM_TOOLS.contains(&id.as_str())
        || name.starts_with("Proton")
        || name.starts_with("Steam Linux Runtime");
    (!tool).then_some((id, name, dir))
}

/// Name, folder and program of an Epic installation (`*.item` manifest JSON).
pub fn epic_item(json: &str) -> Option<(String, PathBuf, String)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("bIsIncompleteInstall")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return None;
    }
    let name = v.get("DisplayName")?.as_str()?.to_owned();
    let dir = PathBuf::from(v.get("InstallLocation")?.as_str()?);
    let exe = v.get("LaunchExecutable")?.as_str()?;
    let file = Path::new(exe).file_name()?.to_str()?.to_owned();
    (!file.is_empty()).then_some((name, dir, file))
}

/// Programs that ship with games but are not the game.
fn is_helper(file: &str) -> bool {
    let f = file.to_ascii_lowercase();
    [
        "unins",
        "crash",
        "setup",
        "redist",
        "vcredist",
        "dxsetup",
        "dotnet",
        "launcherpatcher",
        "easyanticheat",
        "beservice",
        "uninstall",
        "updater",
        "cefprocess",
        "ue4prereq",
    ]
    .iter()
    .any(|h| f.contains(h))
}

/// The `.exe` files in a game's folder (two levels deep), biggest first, helpers left out.
pub fn programs_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<(u64, String)> = Vec::new();
    let visit = |d: &Path, found: &mut Vec<(u64, String)>| -> Vec<PathBuf> {
        let mut subdirs = Vec::new();
        let Ok(entries) = std::fs::read_dir(d) else {
            return subdirs;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                subdirs.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
            {
                let name = e.file_name().to_string_lossy().into_owned();
                if !is_helper(&name) && !found.iter().any(|(_, n)| n.eq_ignore_ascii_case(&name)) {
                    found.push((e.metadata().map(|m| m.len()).unwrap_or(0), name));
                }
            }
        }
        subdirs
    };
    let mut level = vec![dir.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for d in &level {
            next.extend(visit(d, &mut found));
        }
        level = next;
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, n)| n).collect()
}

/// The programs of a game folder with the one the store names first.
pub fn programs_with(first: Option<String>, dir: &Path) -> Vec<String> {
    with_first(first, programs_in(dir))
}

/// `rest` after `first`, without `first` twice.
fn with_first(first: Option<String>, rest: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = first.into_iter().collect();
    for p in rest {
        if !out.iter().any(|q| q.eq_ignore_ascii_case(&p)) {
            out.push(p);
        }
    }
    out
}

/// The programs of a Mac game folder: the executables of the app bundles in it (two levels
/// deep), the bundle's own one first, biggest first otherwise.
pub fn app_programs_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<(u64, String)> = Vec::new();
    let mut level = vec![dir.to_path_buf()];
    for _ in 0..2 {
        let mut next = Vec::new();
        for d in &level {
            let Ok(entries) = std::fs::read_dir(d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let is_app = p
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("app"));
                if is_app {
                    for (size, name) in bundle_programs(&p) {
                        if !is_helper(&name) && !found.iter().any(|(_, n)| *n == name) {
                            found.push((size, name));
                        }
                    }
                } else {
                    next.push(p);
                }
            }
        }
        level = next;
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, n)| n).collect()
}

/// The files in an app bundle's `Contents/MacOS` with their sizes; the one named like the
/// bundle (usually the one macOS starts) counts as the biggest.
fn bundle_programs(app: &Path) -> Vec<(u64, String)> {
    let stem = app.file_stem().map(|s| s.to_string_lossy().into_owned());
    let Ok(entries) = std::fs::read_dir(app.join("Contents/MacOS")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let size = if Some(&name) == stem.as_ref() {
                u64::MAX
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            };
            (size, name)
        })
        .collect()
}

/// The games of the Steam installation at `root`, in every library folder it lists, with the
/// programs `programs` finds in each game's folder.
pub fn steam_games(root: &Path, programs: fn(&Path) -> Vec<String>) -> Vec<InstalledGame> {
    let folders = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf"))
        .unwrap_or_default();
    let mut libraries = steam_libraries(&folders);
    if libraries.is_empty() {
        libraries.push(root.to_path_buf());
    }
    let cache = root.join("appcache").join("librarycache");
    let mut out = Vec::new();
    for library in libraries {
        let apps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Some((id, name, dir)) = std::fs::read_to_string(e.path())
                .ok()
                .as_deref()
                .and_then(steam_app)
            else {
                continue;
            };
            let dir = apps.join("common").join(dir);
            if !dir.is_dir() {
                continue;
            }
            out.push(InstalledGame {
                store: Store::Steam,
                name,
                programs: programs(&dir),
                cover: steam_cover(&cache, &id),
                dir,
            });
        }
    }
    out
}

/// The games in Epic's folder of install manifests, with the program each names first and the
/// others `programs` finds in its folder after it.
pub fn epic_games(manifests: &Path, programs: fn(&Path) -> Vec<String>) -> Vec<InstalledGame> {
    let Ok(entries) = std::fs::read_dir(manifests) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| epic_item(&text))
        .filter(|(_, dir, _)| dir.is_dir())
        .map(|(name, dir, exe)| InstalledGame {
            store: Store::Epic,
            programs: with_first(Some(exe), programs(&dir)),
            name,
            dir,
            cover: None,
        })
        .collect()
}

/// The cover of a Steam app in `appcache/librarycache`: the portrait capsule in the app's folder
/// (any depth, as newer Steam versions nest it), else the older flat file.
pub fn steam_cover(librarycache: &Path, appid: &str) -> Option<PathBuf> {
    let dir = librarycache.join(appid);
    let mut stack = vec![dir];
    let mut fallback = None;
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if p.is_dir() {
                stack.push(p);
            } else if name.starts_with("library_600x900") {
                return Some(p);
            } else if name.starts_with("library_capsule") {
                fallback = Some(p);
            }
        }
    }
    fallback.or_else(|| {
        Some(librarycache.join(format!("{appid}_library_600x900.jpg"))).filter(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keyvalues() {
        let v = parse_vdf("\"a\" { \"b\" \"1\" // note\n \"C\" { \"d\" \"x\\\\y\" } }");
        let a = v.get("a").unwrap();
        assert_eq!(a.get("b").and_then(Vdf::str), Some("1"));
        assert_eq!(
            a.get("c").and_then(|c| c.get("d")).and_then(Vdf::str),
            Some("x\\y")
        );
        // Broken input ends quietly.
        assert_eq!(parse_vdf("\"a\" {").get("a"), Some(&Vdf::Map(vec![])));
    }

    #[test]
    fn steam_files() {
        let folders = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\" { \"228980\" \"1\" }\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"G:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(
            steam_libraries(folders),
            [
                PathBuf::from("C:\\Program Files (x86)\\Steam"),
                PathBuf::from("G:\\SteamLibrary")
            ]
        );
        let game = "\"AppState\" { \"appid\" \"1030300\" \"name\" \"Hollow Knight: Silksong\" \"installdir\" \"Hollow Knight Silksong\" }";
        assert_eq!(
            steam_app(game),
            Some((
                "1030300".into(),
                "Hollow Knight: Silksong".into(),
                "Hollow Knight Silksong".into()
            ))
        );
        let tool = "\"AppState\" { \"appid\" \"228980\" \"name\" \"Steamworks Common Redistributables\" \"installdir\" \"Steamworks Shared\" }";
        assert_eq!(steam_app(tool), None);
    }

    #[test]
    fn epic_manifests() {
        let item = r#"{ "DisplayName": "Ghostrunner 2", "InstallLocation": "G:\\EpicGames\\Ghostrunner2", "LaunchExecutable": "Ghostrunner2.exe", "bIsIncompleteInstall": false }"#;
        assert_eq!(
            epic_item(item),
            Some((
                "Ghostrunner 2".into(),
                PathBuf::from("G:\\EpicGames\\Ghostrunner2"),
                "Ghostrunner2.exe".into()
            ))
        );
        let nested = r#"{ "DisplayName": "X", "InstallLocation": "D:\\X", "LaunchExecutable": "Bin/Win64/X-Win64.exe" }"#;
        assert_eq!(epic_item(nested).unwrap().2, "X-Win64.exe");
        assert_eq!(
            epic_item(
                r#"{ "DisplayName": "X", "InstallLocation": "D:\\X", "LaunchExecutable": "" }"#
            ),
            None
        );
        assert_eq!(
            epic_item(
                r#"{ "DisplayName": "X", "InstallLocation": "D:\\X", "LaunchExecutable": "a.exe", "bIsIncompleteInstall": true }"#
            ),
            None
        );
    }

    #[test]
    fn programs_and_covers_from_folders() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("Game");
        std::fs::create_dir_all(game.join("Bin")).unwrap();
        std::fs::write(game.join("unins000.exe"), vec![0; 900]).unwrap();
        std::fs::write(game.join("Launcher.exe"), vec![0; 10]).unwrap();
        std::fs::write(game.join("Bin").join("Game.exe"), vec![0; 500]).unwrap();
        assert_eq!(programs_in(&game), ["Game.exe", "Launcher.exe"]);

        let cache = root.path().join("librarycache");
        std::fs::create_dir_all(cache.join("42").join("abc")).unwrap();
        std::fs::write(
            cache.join("42").join("abc").join("library_capsule.jpg"),
            b"x",
        )
        .unwrap();
        assert_eq!(
            steam_cover(&cache, "42"),
            Some(cache.join("42").join("abc").join("library_capsule.jpg"))
        );
        std::fs::write(cache.join("7_library_600x900.jpg"), b"x").unwrap();
        assert_eq!(
            steam_cover(&cache, "7"),
            Some(cache.join("7_library_600x900.jpg"))
        );
        assert_eq!(steam_cover(&cache, "8"), None);
    }

    #[test]
    fn programs_of_mac_app_bundles() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("Game");
        let app = game.join("Game.app").join("Contents").join("MacOS");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("Game"), vec![0; 10]).unwrap();
        std::fs::write(app.join("CrashReporter"), vec![0; 900]).unwrap();
        std::fs::write(app.join("Helper"), vec![0; 500]).unwrap();
        let tools = game
            .join("Tools")
            .join("Editor.app")
            .join("Contents")
            .join("MacOS");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("Editor"), vec![0; 50]).unwrap();
        // The bundles' own programs first, the top level's before the folder below.
        assert_eq!(app_programs_in(&game), ["Game", "Editor", "Helper"]);
        assert!(app_programs_in(&root.path().join("missing")).is_empty());
    }

    #[test]
    fn steam_and_epic_folders() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let apps = steam.join("steamapps");
        std::fs::create_dir_all(apps.join("common").join("Silksong")).unwrap();
        std::fs::write(
            apps.join("appmanifest_1030300.acf"),
            "\"AppState\" { \"appid\" \"1030300\" \"name\" \"Hollow Knight: Silksong\" \"installdir\" \"Silksong\" }",
        )
        .unwrap();
        std::fs::write(
            apps.join("appmanifest_1.acf"),
            "\"AppState\" { \"appid\" \"1\" \"name\" \"Gone\" \"installdir\" \"Gone\" }",
        )
        .unwrap();
        let games = steam_games(&steam, |_| vec!["Silksong".into()]);
        assert_eq!(games.len(), 1, "a game without its folder is left out");
        assert_eq!(games[0].name, "Hollow Knight: Silksong");
        assert_eq!(games[0].dir, apps.join("common").join("Silksong"));
        assert_eq!(games[0].programs, ["Silksong"]);

        let manifests = root.path().join("Manifests");
        std::fs::create_dir_all(&manifests).unwrap();
        let dir = root.path().join("Fortnite");
        std::fs::create_dir_all(&dir).unwrap();
        let item = serde_json::json!({
            "DisplayName": "Fortnite",
            "InstallLocation": dir,
            "LaunchExecutable": "FortniteClient.app/Contents/MacOS/FortniteClient",
        });
        std::fs::write(manifests.join("a.item"), item.to_string()).unwrap();
        let games = epic_games(&manifests, |_| {
            vec!["Other".into(), "FortniteClient".into()]
        });
        assert_eq!(games[0].programs, ["FortniteClient", "Other"]);
    }
}
