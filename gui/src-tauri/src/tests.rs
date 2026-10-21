//! Checks that the web frontend and the Rust model agree on names and JSON shapes.
//!
//! The fixtures in `gui/src/lib/model/fixtures` are shared with the frontend tests:
//! `keys.json` lists every key name, `examples.json` holds the example profiles in JSON
//! form, `defaults.json` the defaults the `catalog` command sends, `contract.json` the live
//! state, app settings, hotkeys and a notification as the frontend receives them,
//! `controls.json` the names of the pad controls and `shaping.json` what the stick and
//! trigger shaping makes of sample inputs. Regenerate them with
//! `DOPPELPAD_UPDATE_FIXTURES=1 cargo test -p dp-gui`.

use std::path::PathBuf;

use dp_core::control::{Control, Zone};
use dp_core::engine::{apply_curve, shape_stick, shape_trigger, stick_zone, trigger_zone};
use dp_core::hotkeys::Hotkeys;
use dp_core::keys::Key;
use dp_core::pad::{Motion, PadButton, PadState, Stick, TouchPoint, Trigger};
use dp_core::profile::{
    Crosshair, Profile, RadialMenu, StickConfig, StickZones, TriggerConfig, TriggerZones,
    ZoneRadius,
};
use dp_platform::generic::RawInput;
use dp_platform::groups::{DeviceGroup, SeenDevice};
use dp_platform::{Battery, PadFamily, VirtualPadKind};
use dp_service::calibration::CalibrationStatus;
use dp_service::{LiveDevice, LiveDeviceSlots, LiveRadial, LiveState, SessionInfo};
use serde::Serialize;
use serde_json::Value;

use crate::settings::GuiSettings;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/model/fixtures")
}

/// Compares `expected` with the fixture `name`, after writing it there when
/// `DOPPELPAD_UPDATE_FIXTURES` is set.
fn assert_fixture(name: &str, expected: String) {
    let path = fixtures().join(name);
    if std::env::var_os("DOPPELPAD_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
        "{name} is stale; run with DOPPELPAD_UPDATE_FIXTURES=1"
    );
}

/// Written like the app's commands answer: fields in declaration order, `f32` as written.
fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}

#[test]
fn key_names_fixture_is_current() {
    let names: Vec<&str> = Key::ALL.iter().map(|k| k.name()).collect();
    assert_fixture("keys.json", pretty(&names));
    for n in &names {
        assert_eq!(Key::from_name(n).map(|k| k.name()), Some(*n));
    }
}

/// Numbers compare by their value as `f32`, the profile's number type (`1` from JavaScript
/// equals `1.0` from Rust, `0.1` equals the `f32` nearest to it).
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => Value::from(f64::from(n.as_f64().unwrap() as f32)),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        other => other,
    }
}

/// The defaults the frontend gets from the `catalog` command, for the frontend tests.
#[test]
fn defaults_fixture_is_current() {
    assert_fixture(
        "defaults.json",
        pretty(&crate::commands::catalog().defaults),
    );
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

    let mut all = std::collections::BTreeMap::new();
    for path in names {
        let profile = dp_config::parse_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let json = serde_json::to_string(&profile).unwrap();
        let back: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, profile, "{}", path.display());
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        all.insert(name, profile);
    }
    assert_fixture("examples.json", pretty(&all));
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
    let toml = dp_config::to_toml(&profile).unwrap();
    assert_eq!(dp_config::parse_toml(&toml).unwrap(), profile);
}

/// Every field of the types the frontend receives besides profiles, so that its type
/// declarations can be checked against them.
#[test]
fn contract_fixture_is_current() {
    #[derive(Serialize)]
    struct Contract {
        live: LiveState,
        gui_settings: GuiSettings,
        hotkeys: Hotkeys,
        osd_message: crate::osd::Message,
    }
    let mut state = PadState::default();
    state.set(PadButton::A, true);
    state.motion = Some(Motion {
        gyro: [1.0, 2.0, 3.0],
        accel: [0.0, 1.0, 0.0],
    });
    state.touch[0] = Some(TouchPoint { x: 0.25, y: 0.5 });
    let device = LiveDevice {
        id: "hid:pad".into(),
        name: "DualSense".into(),
        family: PadFamily::DualSense,
        backend: "hid".into(),
        vendor_id: 0x054C,
        product_id: 0x0CE6,
        state,
        gyro_calibration: Some(CalibrationStatus {
            gyro_bias: [0.5, -0.25, 0.0],
            auto: true,
            measuring: Some(0.5),
            last_manual: Some(true),
        }),
        battery: Some(Battery {
            level: 60,
            charging: true,
            wired: false,
        }),
        raw: Some(RawInput {
            buttons: vec![true, false],
            axes: vec![0.5],
            hats: vec![Some(2), None],
        }),
    };
    let live = LiveState {
        devices: vec![device],
        primary: Some("hid:pad".into()),
        profile: Some("Racing".into()),
        profile_path: Some(r"C:\p\racing.toml".into()),
        layer: 1,
        layer_name: Some("Shift".into()),
        slots: vec![Some(r"C:\p\racing.toml".into()), None, None, None],
        seen_devices: vec![SeenDevice {
            id: "hid:pad".into(),
            name: "DualSense".into(),
        }],
        blocked: vec!["xinput:3".into()],
        groups: vec![DeviceGroup {
            name: "Cockpit".into(),
            members: vec!["hid:a".into(), "hid:b".into()],
        }],
        layouts: vec!["0079:0006".into()],
        foreground: Some("game.exe".into()),
        virtual_pad: Some("xbox360".into()),
        warnings: vec!["ViGEmBus is missing".into()],
        device_slots: vec![LiveDeviceSlots {
            id: "hid:pad".into(),
            slots: vec![None, Some("own.toml".into()), None, None],
            active_slot: 1,
        }],
        sessions: vec![SessionInfo {
            owner: Some("hid:pad".into()),
            device: Some("hid:pad".into()),
            profile: "Racing".into(),
            profile_path: Some(r"C:\p\racing.toml".into()),
            layer: 1,
            virtual_pad: Some(VirtualPadKind::Xbox360),
        }],
        crosshair: Some(Crosshair::default()),
        radial: Some(LiveRadial {
            menu: RadialMenu::default(),
            pointed: Some(2),
        }),
        ..LiveState::default()
    };
    let contract = Contract {
        live,
        gui_settings: GuiSettings::default(),
        hotkeys: Hotkeys::default(),
        osd_message: crate::osd::Message::sample(),
    };
    assert_fixture("contract.json", pretty(&contract));
}

/// The names of every pad control (buttons, stick and trigger controls, touchpad, gyro).
#[test]
fn controls_fixture_is_current() {
    let controls = PadButton::ALL
        .map(Control::Button)
        .into_iter()
        .chain(Stick::ALL.into_iter().flat_map(Control::of_stick))
        .chain(Trigger::ALL.into_iter().flat_map(Control::of_trigger))
        .chain(Control::of_touchpad())
        .chain(Control::of_gyro());
    let names: Vec<String> = controls.map(|c| c.to_string()).collect();
    for n in &names {
        assert_eq!(n.parse::<Control>().map(|c| c.to_string()).as_ref(), Ok(n));
    }
    assert_fixture("controls.json", pretty(&names));
}

/// Zone names as in control names (`LeftStickUpHigh`).
fn zone_name(z: Option<Zone>) -> Option<String> {
    z.map(|z| format!("{z:?}"))
}

#[derive(Serialize)]
struct CurveCase {
    points: Vec<[f32; 2]>,
    t: Vec<f32>,
    out: Vec<f32>,
}

/// Thresholds per axis, the way the frontend computes them.
#[derive(Serialize)]
struct Radii {
    x: [f32; 3],
    y: [f32; 3],
}

#[derive(Serialize)]
struct ShapeCase<C, V, T> {
    config: C,
    #[serde(rename = "in")]
    input: Vec<V>,
    out: Vec<V>,
    thresholds: T,
    zones: Vec<Option<String>>,
}

#[derive(Serialize)]
struct Shaping {
    curve: Vec<CurveCase>,
    stick: Vec<ShapeCase<StickConfig, [f32; 2], Radii>>,
    trigger: Vec<ShapeCase<TriggerConfig, f32, [f32; 3]>>,
}

/// What the engine's stick and trigger shaping makes of sample inputs; the frontend draws
/// the same curves and zones.
#[test]
fn shaping_fixture_is_current() {
    let t: Vec<f32> = (0..=10).map(|i| i as f32 / 10.0).collect();
    let curves = [
        vec![],
        vec![[0.5, 0.2]],
        vec![[0.25, 0.5], [0.75, 0.6]],
        vec![[0.0, 0.3]],
        vec![[1.2, 0.5]],
    ];
    let curve = curves
        .into_iter()
        .map(|points| CurveCase {
            out: t.iter().map(|&t| apply_curve(&points, t)).collect(),
            points,
            t: t.clone(),
        })
        .collect();

    let sticks = [
        StickConfig::default(),
        StickConfig {
            deadzone: 0.1,
            outer: 0.9,
            anti_deadzone: 0.2,
            sensitivity: 1.5,
            curve: vec![[0.5, 0.2]],
            zones: StickZones {
                low: ZoneRadius::round(0.2),
                med: Some(ZoneRadius { x: 0.4, y: 0.6 }),
                high: None,
            },
            ..StickConfig::default()
        },
        StickConfig {
            deadzone: 0.3,
            outer: 0.25,
            zones: StickZones {
                low: ZoneRadius { x: 0.1, y: 0.3 },
                med: None,
                high: Some(ZoneRadius::round(0.9)),
            },
            ..StickConfig::default()
        },
    ];
    let vectors = [
        (0.0, 0.0),
        (0.05, 0.0),
        (0.3, 0.4),
        (-0.6, 0.6),
        (1.0, 0.0),
        (0.7, -0.7),
        (0.0, -0.95),
        (0.15, 0.25),
    ];
    let stick = sticks
        .into_iter()
        .map(|cfg| {
            let t = cfg.zone_thresholds();
            ShapeCase {
                input: vectors.iter().map(|&(x, y)| [x, y]).collect(),
                out: vectors
                    .iter()
                    .map(|&v| {
                        let (x, y) = shape_stick(v, &cfg);
                        [x, y]
                    })
                    .collect(),
                thresholds: Radii {
                    x: t.map(|r| r.x),
                    y: t.map(|r| r.y),
                },
                zones: vectors
                    .iter()
                    .map(|&v| zone_name(stick_zone(v, t)))
                    .collect(),
                config: cfg,
            }
        })
        .collect();

    let triggers = [
        TriggerConfig::default(),
        TriggerConfig {
            deadzone: 0.1,
            full_pull: 0.8,
            zones: TriggerZones {
                low: 0.2,
                med: Some(0.5),
                high: None,
            },
            ..TriggerConfig::default()
        },
        TriggerConfig {
            deadzone: 0.5,
            full_pull: 0.4,
            zones: TriggerZones {
                low: 0.0,
                med: None,
                high: Some(0.95),
            },
            ..TriggerConfig::default()
        },
    ];
    let pulls = [0.0, 0.05, 0.2, 0.33, 0.5, 0.79, 0.8, 0.95, 1.0];
    let trigger = triggers
        .into_iter()
        .map(|cfg| {
            let t = cfg.zone_thresholds();
            ShapeCase {
                input: pulls.to_vec(),
                out: pulls.iter().map(|&v| shape_trigger(v, &cfg)).collect(),
                thresholds: t,
                zones: pulls
                    .iter()
                    .map(|&v| zone_name(trigger_zone(v, t)))
                    .collect(),
                config: cfg,
            }
        })
        .collect();

    let shaping = Shaping {
        curve,
        stick,
        trigger,
    };
    assert_fixture("shaping.json", pretty(&shaping));
}
