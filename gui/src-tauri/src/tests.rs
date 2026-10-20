//! Checks that the web frontend and the Rust model agree on names and JSON shapes.
//!
//! `gui/src/lib/model/fixtures/keys.json` lists every key name and `examples.json` holds the
//! example profiles in JSON form; the frontend tests use both. Regenerate them with
//! `DOPPELPAD_UPDATE_FIXTURES=1 cargo test -p dp-gui`.

use std::path::PathBuf;

use dp_core::keys::Key;
use dp_core::profile::Profile;
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/model/fixtures")
}

#[test]
fn key_names_fixture_is_current() {
    let names: Vec<&str> = Key::ALL.iter().map(|k| k.name()).collect();
    let path = fixtures().join("keys.json");
    let expected = serde_json::to_string_pretty(&names).unwrap() + "\n";
    if std::env::var_os("DOPPELPAD_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
        "keys.json is stale; run with DOPPELPAD_UPDATE_FIXTURES=1"
    );
    for n in &names {
        assert_eq!(Key::from_name(n).map(|k| k.name()), Some(*n));
    }
}

/// Numbers compare by value (`1` from JavaScript equals `1.0` from Rust).
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => Value::from(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        other => other,
    }
}

/// The example profiles survive the editor's path (Rust → JSON → Rust) unchanged, and
/// their JSON form is shared with the frontend tests as `examples.json`.
#[test]
fn example_profiles_round_trip_through_json() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/examples");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    names.sort();
    assert!(!names.is_empty());

    let mut all = serde_json::Map::new();
    for path in names {
        let profile = dp_config::parse_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let json = serde_json::to_value(&profile).unwrap();
        let back: Profile = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(back, profile, "{}", path.display());
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        all.insert(name, json);
    }

    let path = fixtures().join("examples.json");
    let expected = serde_json::to_string_pretty(&all).unwrap() + "\n";
    if std::env::var_os("DOPPELPAD_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
        "examples.json is stale; run with DOPPELPAD_UPDATE_FIXTURES=1"
    );
}

#[test]
fn profile_fixture_round_trips() {
    let text = std::fs::read_to_string(fixtures().join("profile.json")).unwrap();
    let original: Value = serde_json::from_str(&text).unwrap();
    let profile: Profile = serde_json::from_value(original.clone()).expect("fixture deserializes");

    let diags = dp_config::validate(&profile);
    assert!(!diags.has_errors(), "fixture is invalid: {diags:?}");

    // What Rust sends to the frontend is exactly what the frontend writes.
    let back = serde_json::to_value(&profile).unwrap();
    assert_eq!(normalize(back), normalize(original));

    // And the TOML the app saves reads back to the same profile.
    let toml = dp_config::to_toml(&profile);
    assert_eq!(dp_config::parse_toml(&toml).unwrap(), profile);
}
