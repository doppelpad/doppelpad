//! Checks that the web frontend and the Rust model agree on names and JSON shapes.
//!
//! The fixtures in `gui/src/lib/model/fixtures` are shared with the frontend tests:
//! `keys.json` lists every key name, `examples.json` holds the example profiles in JSON
//! form, `defaults.json` the defaults the `catalog` command sends, `contract.json` the live
//! state, app settings, hotkeys, a notification and the launch commands of an import as the
//! frontend receives them, `bridge.json` every other answer of the commands, the event names,
//! the overlays' contents, the errors and the catalog's limits, `controls.json` the names of
//! the pad controls and `shaping.json` what the stick and trigger shaping makes of sample
//! inputs. Regenerate them with `DOPPELPAD_UPDATE_FIXTURES=1 cargo test -p dp-gui`.
//!
//! Also checked: every command main.rs registers has a permission (build.rs) that the editor
//! window holds (permissions/editor.toml).

use std::collections::BTreeSet;
use std::path::PathBuf;

use dp_config::launches::{Launch, LaunchPlace};
use dp_core::control::{Control, Zone};
use dp_core::engine::{apply_curve, shape_stick, shape_trigger, stick_zone, trigger_zone};
use dp_core::hotkeys::Hotkeys;
use dp_core::keys::{Key, MouseButton};
use dp_core::pad::{Motion, PadButton, PadState, Stick, TouchPoint, Trigger};
use dp_core::profile::{
    Crosshair, Profile, RadialMenu, StickConfig, StickZones, TriggerConfig, TriggerZones,
    ZoneRadius,
};
use dp_platform::generic::RawInput;
use dp_platform::groups::{DeviceGroup, SeenDevice};
use dp_platform::{Battery, PadFamily, VirtualPadKind};
use dp_service::calibration::CalibrationStatus;
use dp_service::{LiveDevice, LiveDeviceSlots, LiveRadial, LiveState, SessionInfo, SessionOutput};
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
        pretty(&crate::commands::system::catalog().defaults),
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
        launches: Vec<Launch>,
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
        mirror: false,
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
            output: SessionOutput {
                pad: Some(PadState::default()),
                keys: vec![Key::W],
                mouse_buttons: vec![MouseButton::Left],
            },
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
        launches: vec![
            Launch {
                file: Some("racing.toml".into()),
                config: "Racing".into(),
                place: LaunchPlace::Mapping {
                    layer: 1,
                    layer_name: "Shift".into(),
                    trigger: Control::Button(PadButton::A).into(),
                },
                path: "notepad.exe".into(),
                args: "notes.txt".into(),
            },
            Launch {
                file: None,
                config: "Racing".into(),
                place: LaunchPlace::Radial {
                    menu: 1,
                    name: "Tools".into(),
                    sector: 2,
                    label: "Notes".into(),
                },
                path: "notepad.exe".into(),
                args: String::new(),
            },
        ],
    };
    assert_fixture("contract.json", pretty(&contract));
}

/// Every other type the commands answer with, the events the window listens for, what the
/// overlays receive, the bridge's errors and the limits of the catalog, so that their type
/// declarations can be checked against them, and the window's own copies of the limits too.
#[test]
fn bridge_fixture_is_current() {
    use std::collections::BTreeMap;

    use dp_config::bundle::{BundleLaunches, PresetEntry, Unchecked};
    use dp_config::library::{ConfigEntry, Game, GameMeta, Library};
    use dp_config::{Diagnostic, Severity, history, relocate};

    use crate::commands::library::{Imported, LibraryPlace, LoadedConfig};
    use crate::commands::system::{InstalledGame, Limits, Notice, RewasdStatus, RunningProgram};
    use crate::error::{CmdError, ErrorCode};
    use crate::osd::{Event, Hint};
    use crate::settings::{Corner, ScreenRef, color_hex, pad_color};

    /// The event names the window listens for.
    #[derive(Serialize)]
    struct Events {
        open_files: &'static str,
        hint: &'static str,
        /// `<overlay window label>` + this: a change of some fields of an overlay's content.
        overlay_patch: &'static str,
    }

    #[derive(Serialize)]
    struct Bridge {
        events: Events,
        limits: Limits,
        /// The tray icon's colour of these device ids, as the window shows it.
        pad_colors: BTreeMap<&'static str, String>,
        error: CmdError,
        error_codes: Vec<ErrorCode>,
        notices: Vec<Notice>,
        update_info: crate::updates::UpdateInfo,
        library: Library,
        library_place: LibraryPlace,
        library_targets: Vec<relocate::Target>,
        library_moved: relocate::Moved,
        loaded_config: LoadedConfig,
        imported: Imported,
        versions: Vec<history::Version>,
        bundle_launches: BundleLaunches,
        presets: Vec<PresetEntry>,
        installed_games: Vec<InstalledGame>,
        running_programs: Vec<RunningProgram>,
        rewasd_status: RewasdStatus,
        hint: Hint,
        osd_events: Vec<Event>,
        osd_screen: ScreenRef,
        radial: crate::radial::Content,
        radial_pointed: crate::radial::Pointed,
        pad_overlay: crate::pad_overlay::PadContent,
        mappings_overlay: crate::pad_overlay::MappingsContent,
    }

    let racing = Profile {
        name: "Racing".into(),
        ..Profile::default()
    };
    let diagnostic = Diagnostic {
        severity: Severity::Warning,
        path: "mappings[0] (A)".into(),
        message: "pause longer than a minute".into(),
    };
    let config = ConfigEntry {
        path: r"C:\p\racing\fast.toml".into(),
        name: "Fast".into(),
        error: None,
        processes: Vec::new(),
    };
    let game = Game {
        dir: r"C:\p\racing".into(),
        meta: GameMeta {
            name: "Racing".into(),
            processes: vec!["racing.exe".into()],
            image: Some("cover.png".into()),
            default_config: Some("fast.toml".into()),
            order: Some(1),
            description: Some("Career".into()),
        },
        configs: vec![config],
    };
    let device = LiveDevice {
        id: "hid:pad".into(),
        name: "DualSense".into(),
        family: PadFamily::DualSense,
        backend: "hid".into(),
        vendor_id: 0x054C,
        product_id: 0x0CE6,
        state: PadState::default(),
        gyro_calibration: None,
        battery: None,
        raw: None,
        mirror: false,
    };
    let bridge = Bridge {
        events: Events {
            open_files: crate::open_file::OPEN_EVENT,
            hint: crate::osd::HINT_EVENT,
            overlay_patch: ":patch",
        },
        limits: crate::commands::system::catalog().limits,
        pad_colors: [
            "hid:a",
            "xinput:0",
            "evdev:045e:028e:usb-0000:04:00.3-1/input0",
        ]
        .into_iter()
        .map(|id| (id, color_hex(pad_color(id, &BTreeMap::new()))))
        .collect(),
        error: CmdError::with(ErrorCode::NotInLibrary, r"C:\elsewhere\x.toml"),
        error_codes: ErrorCode::ALL.to_vec(),
        notices: vec![Notice::ServiceTakeover],
        update_info: crate::updates::UpdateInfo {
            version: "0.8.0".into(),
            current: "0.7.0".into(),
            notes: Some("Faster".into()),
        },
        library: Library {
            games: vec![game],
            unsorted: vec![ConfigEntry {
                path: r"C:\p\loose.toml".into(),
                name: "loose".into(),
                error: Some("line 1: unknown field `x`".into()),
                processes: Vec::new(),
            }],
            errors: vec![r"C:\p\broken\game.toml: line 2: invalid type".into()],
        },
        library_place: LibraryPlace {
            dir: r"D:\Sync\pads".into(),
            default: r"C:\Users\me\AppData\Roaming\doppelpad\profiles".into(),
            is_default: false,
            portable: None,
        },
        library_targets: vec![
            relocate::Target::Empty,
            relocate::Target::Library,
            relocate::Target::Foreign,
        ],
        library_moved: relocate::Moved {
            dir: r"D:\Sync\pads".into(),
            moved_from: Some(r"C:\p".into()),
            left_behind: 0,
        },
        loaded_config: LoadedConfig {
            profile: racing.clone(),
            diagnostics: vec![diagnostic.clone()],
            read_only: false,
        },
        imported: Imported {
            path: r"C:\p\racing\shared.toml".into(),
            diagnostics: vec![diagnostic],
        },
        versions: vec![history::Version {
            path: r"C:\history\racing\fast\1793313942372.toml".into(),
            time_ms: 1_793_313_942_372,
        }],
        bundle_launches: BundleLaunches {
            launches: Vec::new(),
            unchecked: vec![Unchecked {
                file: "future.toml".into(),
                error: "made by a newer Doppelpad (format 2; this one reads up to 1)".into(),
            }],
        },
        presets: vec![PresetEntry {
            name: "Driving".into(),
            path: r"C:\p\presets\driving.toml".into(),
        }],
        installed_games: vec![InstalledGame {
            store: "steam",
            name: "Hades".into(),
            dir: r"C:\Steam\steamapps\common\Hades".into(),
            programs: vec!["Hades.exe".into()],
            cover: Some(r"C:\Steam\appcache\librarycache\1145360_library_600x900.jpg".into()),
        }],
        running_programs: vec![RunningProgram {
            exe: "game.exe".into(),
            title: "The Game".into(),
        }],
        rewasd_status: RewasdStatus {
            service: Some("Mapping Service".into()),
            programs: vec!["tray.exe".into()],
        },
        hint: Hint {
            id: crate::osd::STEAM_HINT,
            game: "Racing".into(),
        },
        osd_events: vec![
            Event::RemapOn,
            Event::RemapOff,
            Event::Slot {
                slot: 2,
                config: Some("Racing › Fast".into()),
            },
            Event::Layer {
                layer: 1,
                name: Some("Shift".into()),
            },
            Event::Connected {
                name: "DualSense".into(),
            },
            Event::Disconnected {
                name: "DualSense".into(),
            },
            Event::BatteryLow {
                name: "DualSense".into(),
                level: 35,
            },
            Event::BatteryCritical {
                name: "DualSense".into(),
                level: 15,
            },
            Event::SteamInput {
                game: "Racing".into(),
            },
            Event::Screenshot {
                file: Some("Racing 2026-10-30 00-01-02.png".into()),
                error: None,
            },
            Event::Test,
        ],
        osd_screen: ScreenRef {
            name: r"\\.\DISPLAY2".into(),
            x: 1920,
            y: 0,
        },
        radial: crate::radial::Content {
            menu: RadialMenu::default(),
            pointed: Some(1),
            images: vec![Some("data:image/png;base64,iVBORw==".into()), None],
            cursor: Some([960.0, 540.0]),
        },
        radial_pointed: crate::radial::Pointed { pointed: Some(2) },
        pad_overlay: crate::pad_overlay::PadContent {
            device,
            opacity: 90,
            corner: Corner::TopLeft,
        },
        mappings_overlay: crate::pad_overlay::MappingsContent {
            profile: racing,
            layer: 1,
            family: Some(PadFamily::DualSense),
            lang: "en".into(),
            opacity: 90,
            corner: Corner::TopRight,
        },
    };
    assert_fixture("bridge.json", pretty(&bridge));
}

/// The limits the catalog sends are those the validator keeps to.
#[test]
fn the_catalog_limits_are_the_validators() {
    use crate::commands::system::{MAX_CROSSHAIRS, MAX_RADIAL_SECTORS};
    use dp_core::profile::RadialSector;

    let errors = |p: &Profile| dp_config::validate(p).has_errors();
    let menu = |sectors: usize| Profile {
        radial_menus: vec![RadialMenu {
            sectors: vec![RadialSector::default(); sectors],
            ..RadialMenu::default()
        }],
        ..Profile::default()
    };
    assert!(!errors(&menu(MAX_RADIAL_SECTORS)));
    assert!(errors(&menu(MAX_RADIAL_SECTORS + 1)));
    let crosshairs = |n: usize| Profile {
        crosshairs: vec![Crosshair::default(); n],
        ..Profile::default()
    };
    assert!(!errors(&crosshairs(MAX_CROSSHAIRS)));
    assert!(errors(&crosshairs(MAX_CROSSHAIRS + 1)));
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

/// The part of `text` between `start` and the next `end`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let rest = &text[text.find(start).unwrap() + start.len()..];
    &rest[..rest.find(end).unwrap()]
}

/// The strings in double quotes.
fn quoted(text: &str) -> BTreeSet<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_command_has_a_permission_the_editor_holds() {
    let registered: BTreeSet<String> = between(include_str!("main.rs"), "generate_handler![", "]")
        .split(',')
        .filter_map(|item| item.trim().rsplit("::").next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let permitted = quoted(between(
        include_str!("../build.rs"),
        "const COMMANDS: &[&str] = &[",
        "];",
    ));
    let editor: BTreeSet<String> = quoted(between(
        include_str!("../permissions/editor.toml"),
        "permissions = [",
        "]",
    ))
    .iter()
    .map(|p| p.strip_prefix("allow-").unwrap().replace('-', "_"))
    .collect();
    assert_eq!(
        permitted, registered,
        "build.rs COMMANDS and main.rs differ"
    );
    assert_eq!(
        editor, registered,
        "permissions/editor.toml and main.rs differ"
    );
}
