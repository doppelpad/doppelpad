//! Earlier versions of configs: before a config is saved over, the file as it was goes into a
//! history folder of its own (`history_dir()/<game folder>/<config>/<time>.toml`), and the
//! newest few are kept.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::library::{Result, io};

/// How many earlier versions of a config are kept.
pub const KEEP: usize = 20;

/// One kept version: its file and when it was replaced (ms since 1970).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Version {
    pub path: PathBuf,
    pub time_ms: u64,
}

/// The history folder of a config: by its game folder's name and its file name.
fn folder(root: &Path, config: &Path) -> PathBuf {
    let game = config
        .parent()
        .and_then(Path::file_name)
        .map_or_else(|| "_".into(), |n| n.to_string_lossy().into_owned());
    let stem = config
        .file_stem()
        .map_or_else(|| "_".into(), |n| n.to_string_lossy().into_owned());
    root.join(game).join(stem)
}

/// Keep the config file as it is now before it is saved over: a version with the time `now_ms`,
/// unless it equals the newest version already kept. The oldest go beyond `keep`. A config that
/// does not exist yet keeps nothing.
pub fn keep_version(config: &Path, root: &Path, now_ms: u64, keep: usize) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Ok(());
    };
    let dir = folder(root, config);
    let kept = versions(config, root);
    if kept
        .first()
        .is_some_and(|v| std::fs::read_to_string(&v.path).is_ok_and(|t| t == text))
    {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let path = dir.join(format!("{now_ms}.toml"));
    crate::store::write_atomic(&path, &text).map_err(io(&path))?;
    for old in versions(config, root).into_iter().skip(keep) {
        let _ = std::fs::remove_file(old.path);
    }
    Ok(())
}

/// The kept versions of a config, the newest first.
pub fn versions(config: &Path, root: &Path) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(folder(root, config)) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let time_ms = path
                .file_stem()?
                .to_str()?
                .parse()
                .ok()
                .filter(|_| path.extension().is_some_and(|x| x == "toml"))?;
            Some(Version { path, time_ms })
        })
        .collect();
    out.sort_by_key(|v| std::cmp::Reverse(v.time_ms));
    out
}

/// Milliseconds since 1970, for naming a version.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_changed_versions_newest_first_and_drops_the_oldest() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("profiles").join("halo");
        std::fs::create_dir_all(&game).unwrap();
        let config = game.join("default.toml");
        let root = tmp.path().join("history");

        keep_version(&config, &root, 1, 3).unwrap();
        assert!(versions(&config, &root).is_empty(), "nothing to keep yet");

        for (i, text) in ["a", "b", "b", "c", "d"].iter().enumerate() {
            std::fs::write(&config, text).unwrap();
            keep_version(&config, &root, 10 + i as u64, 3).unwrap();
        }
        let kept = versions(&config, &root);
        // "b" twice in a row is kept once; only the newest three stay.
        assert_eq!(
            kept.iter().map(|v| v.time_ms).collect::<Vec<_>>(),
            [14, 13, 11]
        );
        assert_eq!(std::fs::read_to_string(&kept[0].path).unwrap(), "d");
        assert!(kept[0].path.starts_with(root.join("halo").join("default")));
    }
}
