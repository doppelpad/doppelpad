use std::sync::{Arc, Mutex};

use dp_core::hotkeys::Hotkeys;
use dp_ipc::{Request, Response};
use dp_platform::{ForegroundWatcher, NullPlatform, PadFamily, Platform};

use super::*;
use crate::httpapi::HttpSettings;
use crate::runtime::testing::{self, Shared};
use crate::slots::RuntimeOptions;

/// A service for a test, its profiles under the test's directory.
struct TestService {
    platform: Platform,
    opts: ServiceOptions,
}

impl TestService {
    /// On the null platform.
    fn new(dir: &Path) -> TestService {
        TestService {
            platform: NullPlatform::platform(),
            opts: ServiceOptions::new(dir.join("profiles")),
        }
    }

    /// On the fake platform with these controllers, and the world the fakes share.
    fn fake(dir: &Path, pads: &[(&str, PadFamily)]) -> (TestService, Shared) {
        let (platform, world) = testing::platform();
        world
            .lock()
            .unwrap()
            .pads
            .extend(pads.iter().map(|&(id, family)| testing::pad(id, family)));
        let t = TestService {
            platform,
            opts: ServiceOptions::new(dir.join("profiles")),
        };
        (t, world)
    }

    /// With this foreground watcher, asked on every tick.
    fn foreground(mut self, fg: impl ForegroundWatcher + 'static) -> TestService {
        self.platform.foreground = Box::new(fg);
        self.opts.foreground_poll_ms = 0;
        self
    }

    fn platform(mut self, f: impl FnOnce(&mut Platform)) -> TestService {
        f(&mut self.platform);
        self
    }

    fn opts(mut self, f: impl FnOnce(&mut ServiceOptions)) -> TestService {
        f(&mut self.opts);
        self
    }

    fn build(self) -> Service {
        Service::new(Runtime::with_platform(self.platform), self.opts).unwrap()
    }
}
struct FakeForeground(Arc<Mutex<Option<String>>>);

impl ForegroundWatcher for FakeForeground {
    fn foreground_process(&mut self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
}

fn write_profile(dir: &Path, file: &str, name: &str, processes: &[&str]) -> PathBuf {
    let path = dir.join(file);
    let procs: Vec<String> = processes.iter().map(|p| format!("\"{p}\"")).collect();
    std::fs::write(
        &path,
        format!(
            "name = \"{name}\"\nprocesses = [{}]\n[output]\nvirtual_pad = \"none\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{{ key = \"Space\" }}]\n",
            procs.join(", ")
        ),
    )
    .unwrap();
    path
}

fn service(dir: &Path, state: Option<PathBuf>) -> (Service, Arc<Mutex<Option<String>>>) {
    let fg = Arc::new(Mutex::new(None));
    let s = TestService::new(dir)
        .foreground(FakeForeground(fg.clone()))
        .opts(|o| o.state_path = state)
        .build();
    (s, fg)
}

fn active_name(s: &Service) -> Option<String> {
    s.runtime().profile().map(|p| p.name.clone())
}

struct Desktop(Arc<std::sync::atomic::AtomicBool>);

impl dp_platform::SystemControl for Desktop {
    fn power(&mut self, _: dp_platform::PowerAction) -> dp_platform::Result<()> {
        Ok(())
    }
    fn screenshot(&mut self) -> dp_platform::Result<PathBuf> {
        Ok(PathBuf::new())
    }
    fn close_foreground_window(&mut self) -> dp_platform::Result<()> {
        Ok(())
    }
    fn desktop_active(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[test]
fn answers_http_requests_on_its_tick() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _) = service(dir.path(), None);
    let reply = s.handle(ServiceCmd::SetHttp(HttpSettings {
        enabled: true,
        port: 0,
        ..Default::default()
    }));
    assert!(reply.into_result().is_ok());
    let live = s.live().http;
    assert_eq!(
        live.settings.token.len(),
        32,
        "a token is made on first use"
    );
    let addr: std::net::SocketAddr = live.address.unwrap().parse().unwrap();
    let client = std::thread::spawn(move || {
        let mut c = std::net::TcpStream::connect(addr).unwrap();
        write!(
            c,
            "GET /api/status HTTP/1.1
Host: 127.0.0.1:{}

",
            addr.port()
        )
        .unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        out
    });
    while !client.is_finished() {
        s.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let out = client.join().unwrap();
    assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
    assert!(out.contains(r#""type":"status""#), "{out}");
}

#[test]
fn pauses_while_locked_or_the_game_is_not_in_front() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = tempfile::tempdir().unwrap();
    let game = write_profile(dir.path(), "game.toml", "Game", &["game.exe"]);
    let fg = Arc::new(Mutex::new(Some("game.exe".to_string())));
    let active = Arc::new(AtomicBool::new(true));
    let mut s = TestService::new(dir.path())
        .foreground(FakeForeground(fg.clone()))
        .platform(|p| p.system = Box::new(Desktop(active.clone())))
        .build();
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path: game,
    });
    s.handle(ServiceCmd::SetRuntimeOptions(RuntimeOptions {
        pause_unfocused: true,
        ..Default::default()
    }));
    s.tick();
    assert!(!s.live().suspended);
    *fg.lock().unwrap() = Some("notepad.exe".into());
    s.tick();
    assert!(
        s.live().focus_paused && s.live().suspended,
        "the game is not in front"
    );
    *fg.lock().unwrap() = Some("GAME.EXE".into());
    s.tick();
    assert!(!s.live().suspended);

    active.store(false, Ordering::Relaxed);
    s.rt.advance_ms(DESKTOP_POLL_MS + 20);
    s.tick();
    assert!(s.live().locked && s.live().suspended, "locked");
    active.store(true, Ordering::Relaxed);
    s.rt.advance_ms(DESKTOP_POLL_MS + 20);
    s.tick();
    assert!(!s.live().suspended);
}

#[test]
fn slots_apply_select_clear_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let b = write_profile(dir.path(), "b.toml", "B", &[]);
    let state = dir.path().join("state.toml");
    let (mut s, _) = service(dir.path(), Some(state.clone()));
    assert_eq!(active_name(&s), None);

    assert!(
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 0,
            path: a
        })
        .into_result()
        .is_ok()
    );
    assert!(
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 2,
            path: b.clone()
        })
        .into_result()
        .is_ok()
    );
    assert_eq!(active_name(&s).as_deref(), Some("B"));

    s.handle(ServiceCmd::SelectSlot {
        device: None,
        slot: 0,
    });
    assert_eq!(active_name(&s).as_deref(), Some("A"));

    s.handle(ServiceCmd::SetRemap(false));
    assert_eq!(active_name(&s), None, "remap off unloads but keeps slots");
    assert_eq!(s.slots().shared.slots[2], Some(b.clone()));
    s.handle(ServiceCmd::SetRemap(true));
    assert_eq!(active_name(&s).as_deref(), Some("A"));

    s.handle(ServiceCmd::SetRemap(false));
    s.handle(ServiceCmd::SelectSlot {
        device: None,
        slot: 2,
    });
    assert_eq!(
        active_name(&s).as_deref(),
        Some("B"),
        "selecting a slot turns remapping on"
    );
    assert!(s.live().remap_on);
    s.handle(ServiceCmd::SelectSlot {
        device: None,
        slot: 0,
    });

    s.handle(ServiceCmd::ClearSlot {
        device: None,
        slot: 0,
    });
    assert_eq!(active_name(&s), None, "clearing the active slot unloads it");
    assert!(
        s.handle(ServiceCmd::SelectSlot {
            device: None,
            slot: 9
        })
        .into_result()
        .is_err()
    );

    // A new service picks the persisted state up again.
    drop(s);
    let (s2, _) = service(dir.path(), Some(state));
    assert_eq!(s2.slots().shared.slots[2], Some(b));
    assert_eq!(s2.slots().shared.active, 0);
}

#[test]
fn apply_to_slot_rejects_broken_files() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.toml");
    std::fs::write(&bad, "[[mappings]]\ntrigger = \"NoSuchButton\"\n").unwrap();
    let (mut s, _) = service(dir.path(), None);
    assert!(
        s.handle(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 1,
            path: bad
        })
        .into_result()
        .is_err()
    );
    assert_eq!(s.slots().shared.slots[1], None, "slot untouched");
}

#[test]
fn suspend_is_reported_and_reversible() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let (mut s, _) = service(dir.path(), None);
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path: a,
    });
    s.handle(ServiceCmd::SetSuspended(true));
    s.tick();
    assert!(s.live().suspended);
    assert_eq!(
        active_name(&s).as_deref(),
        Some("A"),
        "profile stays loaded while paused"
    );
    s.handle(ServiceCmd::SetSuspended(false));
    assert!(!s.live().suspended);
}

/// Reports only which process is in front.
struct PidForeground(Arc<Mutex<Option<u32>>>);

impl ForegroundWatcher for PidForeground {
    fn foreground_process(&mut self) -> Option<String> {
        None
    }
    fn foreground_pid(&mut self) -> Option<u32> {
        *self.0.lock().unwrap()
    }
}

#[test]
fn editor_pause_holds_only_while_doppelpad_is_in_front() {
    let dir = tempfile::tempdir().unwrap();
    let pid = Arc::new(Mutex::new(Some(std::process::id())));
    let mut s = TestService::new(dir.path())
        .foreground(PidForeground(pid.clone()))
        .build();

    s.handle(ServiceCmd::SetSuspended(true));
    s.tick();
    assert!(s.live().suspended, "the editor in front pauses");

    *pid.lock().unwrap() = Some(std::process::id() + 1);
    s.tick();
    assert!(
        s.live().suspended,
        "trusted while the foreground check catches up"
    );

    s.opts.gui_focus_grace_ms = 0;
    s.tick();
    assert!(
        !s.live().suspended,
        "a missed focus change does not keep the pause"
    );

    *pid.lock().unwrap() = Some(std::process::id());
    s.tick();
    assert!(s.live().suspended, "back in the editor");

    s.handle(ServiceCmd::SetSuspended(false));
    assert!(!s.live().suspended);
}

#[test]
fn auto_switch_applies_and_restores() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    write_profile(&profiles, "shooter.toml", "Game", &["game.exe"]);
    let base = write_profile(dir.path(), "base.toml", "Base", &[]);
    let (mut s, fg) = service(dir.path(), None);
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path: base,
    });

    *fg.lock().unwrap() = Some("Game.EXE".into());
    s.tick();
    assert_eq!(active_name(&s).as_deref(), Some("Game"));
    assert_eq!(s.live().foreground.as_deref(), Some("Game.EXE"));

    *fg.lock().unwrap() = Some("explorer.exe".into());
    s.tick();
    assert_eq!(
        active_name(&s).as_deref(),
        Some("Base"),
        "slot profile restored"
    );

    s.handle(ServiceCmd::SetAutoSwitch(false));
    *fg.lock().unwrap() = Some("game.exe".into());
    s.tick();
    assert_eq!(active_name(&s).as_deref(), Some("Base"), "auto-switch off");
}

#[test]
fn foreground_is_reported_without_auto_switch() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, fg) = service(dir.path(), None);
    s.handle(ServiceCmd::SetAutoSwitch(false));
    s.handle(ServiceCmd::SetRemap(false));
    *fg.lock().unwrap() = Some("notepad.exe".into());
    s.tick();
    assert_eq!(s.live().foreground.as_deref(), Some("notepad.exe"));
    assert_eq!(s.status().foreground.as_deref(), Some("notepad.exe"));
}

#[test]
fn auto_switch_waits_while_remapping_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    write_profile(&profiles, "shooter.toml", "Game", &["game.exe"]);
    let (mut s, fg) = service(dir.path(), None);
    s.handle(ServiceCmd::SetRemap(false));
    *fg.lock().unwrap() = Some("game.exe".into());
    s.tick();
    assert_eq!(active_name(&s), None, "remapping is off");

    s.handle(ServiceCmd::SetRemap(true));
    s.tick();
    assert_eq!(
        active_name(&s).as_deref(),
        Some("Game"),
        "the game in the foreground takes over once remapping is on again"
    );
}

#[test]
fn auto_switch_uses_game_folders() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    let game = dp_config::library::create_game(&profiles, "Racer").unwrap();
    let meta = dp_config::library::GameMeta {
        processes: vec!["racer.exe".into()],
        ..game.meta.clone()
    };
    dp_config::library::update_game_meta(&game.dir, &meta).unwrap();
    let (mut s, fg) = service(dir.path(), None);
    *fg.lock().unwrap() = Some("racer.exe".into());
    s.tick();
    assert_eq!(active_name(&s).as_deref(), Some("Default"));
}

#[test]
fn gyro_calibration_commands_reach_the_controller_and_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calibration.toml");
    let (t, world) = TestService::fake(dir.path(), &[]);
    let mut s = t.opts(|o| o.calibration_path = Some(path.clone())).build();
    assert!(matches!(
        s.handle(ServiceCmd::CalibrateGyro { device: None }),
        ServiceReply::Err(_)
    ));
    world
        .lock()
        .unwrap()
        .pads
        .push(crate::runtime::testing::pad("pad", PadFamily::DualSense));
    s.tick();
    assert!(matches!(
        s.handle(ServiceCmd::SetGyroAuto {
            device: None,
            on: false
        }),
        ServiceReply::Ok(_)
    ));
    s.tick();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("[devices.pad]") && saved.contains("auto = false"),
        "{saved}"
    );
}

#[test]
fn hotkeys_select_slots_and_switch_remapping() {
    use dp_core::pad::PadButton::{self, *};
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let b = write_profile(dir.path(), "b.toml", "B", &[]);
    let (t, world) = TestService::fake(dir.path(), &[("pad", PadFamily::Xbox)]);
    let mut s = t
        .opts(|o| o.hotkeys_path = Some(dir.path().join("hotkeys.toml")))
        .build();
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path: a,
    });
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 1,
        path: b,
    });
    assert_eq!(active_name(&s).as_deref(), Some("B"));
    let hk = Hotkeys {
        hold_ms: 50,
        remap: vec![Guide],
        ..Default::default()
    };
    assert!(
        s.handle(ServiceCmd::SetHotkeys(hk.clone()))
            .into_result()
            .is_ok()
    );
    assert_eq!(s.handle(ServiceCmd::GetHotkeys), ServiceReply::Hotkeys(hk));

    let hold = |s: &mut Service, buttons: &[PadButton]| {
        world.lock().unwrap().pads[0].state.buttons = PadButton::mask(buttons);
        s.tick();
        s.rt.advance_ms(70);
        s.tick();
        world.lock().unwrap().pads[0].state.buttons = 0;
        s.tick();
    };
    hold(&mut s, &[LB, RB, X]);
    assert_eq!(active_name(&s).as_deref(), Some("A"), "slot 1 hotkey");
    hold(&mut s, &[LB, RB, B]);
    assert_eq!(active_name(&s).as_deref(), Some("A"), "slot 3 is empty");
    hold(&mut s, &[Guide]);
    assert!(!s.live().remap_on);
    assert_eq!(active_name(&s), None, "remap hotkey turns remapping off");
    hold(&mut s, &[Guide]);
    assert_eq!(active_name(&s).as_deref(), Some("A"), "and on again");
    hold(&mut s, &[Back, Start, LB, RB]);
    assert!(!s.live().remap_on, "emergency stop");

    s.handle(ServiceCmd::PauseHotkeys(true));
    hold(&mut s, &[Guide]);
    assert!(!s.live().remap_on, "paused while recording");
    assert!(
        s.handle(ServiceCmd::SetHotkeys(Hotkeys {
            hold_ms: 5,
            ..Default::default()
        }))
        .into_result()
        .is_err()
    );
    let saved = crate::hotkeys::load(&dir.path().join("hotkeys.toml"));
    assert_eq!(saved.remap, vec![Guide], "settings persisted");
}

#[test]
fn ipc_slots_and_remap() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let b = write_profile(dir.path(), "b.toml", "B", &[]);
    let (mut s, _) = service(dir.path(), None);
    let ok = |r: Response| matches!(r, Response::Ok { .. });
    assert!(ok(s.handle_ipc(Request::ApplyToSlot {
        path: a.display().to_string(),
        slot: 1,
        device: None
    })));
    assert!(ok(s.handle_ipc(Request::ApplyToSlot {
        path: b.display().to_string(),
        slot: 3,
        device: None
    })));
    assert_eq!(active_name(&s).as_deref(), Some("B"));
    assert!(ok(s.handle_ipc(Request::SelectSlot {
        slot: 1,
        device: None
    })));
    assert_eq!(active_name(&s).as_deref(), Some("A"));
    let Response::Status(st) = s.handle_ipc(Request::Status) else {
        panic!()
    };
    assert_eq!((st.active_slot, st.remap_on), (1, true));
    assert_eq!(
        st.slots[2].as_deref(),
        Some(b.display().to_string().as_str())
    );

    assert!(ok(s.handle_ipc(Request::Remap { on: false })));
    assert_eq!(active_name(&s), None);
    assert!(ok(s.handle_ipc(Request::Remap { on: true })));
    assert_eq!(active_name(&s).as_deref(), Some("A"));
    assert!(
        !ok(s.handle_ipc(Request::SelectSlot {
            slot: 0,
            device: None
        })),
        "slots are 1–4"
    );
    assert!(!ok(s.handle_ipc(Request::SelectSlot {
        slot: 5,
        device: None
    })));
    assert!(ok(s.handle_ipc(Request::ClearSlot {
        slot: Some(3),
        device: None
    })));
    assert_eq!(s.slots().shared.slots[2], None);
    assert!(ok(s.handle_ipc(Request::ClearSlot {
        slot: None,
        device: None
    })));
    assert!(s.slots().shared.slots.iter().all(Option::is_none));
    assert_eq!(active_name(&s), None);
}

#[test]
fn two_controllers_with_their_own_profiles() {
    use dp_core::keys::Key;
    use dp_core::pad::PadButton;
    let dir = tempfile::tempdir().unwrap();
    let profile = |name: &str, key: &str| {
        let path = dir.path().join(format!("{name}.toml"));
        std::fs::write(
            &path,
            format!("name = \"{name}\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{{ key = \"{key}\" }}]\n"),
        )
        .unwrap();
        path
    };
    let one = profile("One", "Q");
    let two = profile("Two", "E");
    let (t, world) = TestService::fake(
        dir.path(),
        &[("p1", PadFamily::Xbox), ("p2", PadFamily::DualSense)],
    );
    let mut s = t.build();
    s.tick();
    let ok = |r: ServiceReply| r.into_result().unwrap();
    ok(s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path: one,
    }));
    ok(s.handle(ServiceCmd::ApplyToSlot {
        device: Some("p2".into()),
        slot: 1,
        path: two,
    }));
    let sessions = s.runtime().sessions();
    assert_eq!(sessions.len(), 2);
    assert_eq!(
        (sessions[0].owner.as_deref(), sessions[0].device.as_deref()),
        (None, Some("p1")),
        "the shared slots run on the controller without its own"
    );
    assert_eq!(
        (sessions[1].owner.as_deref(), sessions[1].device.as_deref()),
        (Some("p2"), Some("p2"))
    );
    assert_eq!(
        world.lock().unwrap().virtual_pads.len(),
        2,
        "one virtual pad each"
    );
    assert_eq!(s.live().device_slots[0].active_slot, 1);
    s.tick(); // the engines start (inputs held at start are no presses)

    let press = |s: &mut Service, pad: usize| {
        world.lock().unwrap().pads[pad]
            .state
            .set(PadButton::A, true);
        s.tick();
        world.lock().unwrap().pads[pad]
            .state
            .set(PadButton::A, false);
        s.tick();
    };
    press(&mut s, 1);
    press(&mut s, 0);
    let downs: Vec<Key> = world
        .lock()
        .unwrap()
        .keys
        .iter()
        .filter(|k| k.1)
        .map(|k| k.0)
        .collect();
    assert_eq!(
        downs,
        [Key::E, Key::Q],
        "each controller drives its own profile"
    );

    // Its session ends when the controller goes away, and comes back with it.
    let p2 = world.lock().unwrap().pads.remove(1);
    s.tick();
    assert_eq!(s.runtime().sessions().len(), 1);
    world.lock().unwrap().pads.push(p2);
    s.tick();
    assert_eq!(s.runtime().sessions().len(), 2);

    ok(s.handle(ServiceCmd::OwnSlots {
        device: "p2".into(),
        on: false,
    }));
    assert_eq!(s.runtime().sessions().len(), 1);
    assert!(s.live().device_slots.is_empty());
    ok(s.handle(ServiceCmd::SetRemap(false)));
    assert!(s.runtime().sessions().is_empty());
}

#[test]
fn ipc_apply_and_stop() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let (mut s, _) = service(dir.path(), None);
    let r = s.handle_ipc(Request::Apply {
        path: a.display().to_string(),
    });
    assert!(matches!(r, Response::Ok { .. }), "{r:?}");
    assert!(
        matches!(s.handle_ipc(Request::Status), Response::Status(st) if st.profile.as_deref() == Some("A"))
    );
    s.handle_ipc(Request::Stop);
    assert_eq!(active_name(&s), None);
    assert!(!s.live().remap_on);
}

#[test]
fn virtual_pad_kind_is_spelled_like_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pad.toml");
    std::fs::write(&path, "name = \"Pad\"\n").unwrap();
    let (t, _) = TestService::fake(dir.path(), &[("p1", PadFamily::Xbox)]);
    let mut s = t.build();
    s.tick();
    s.handle(ServiceCmd::ApplyToSlot {
        device: None,
        slot: 0,
        path,
    })
    .into_result()
    .unwrap();
    assert_eq!(s.status().virtual_pad.as_deref(), Some("xbox360"));
    assert_eq!(s.live().virtual_pad.as_deref(), Some("xbox360"));
}

#[test]
fn replies_say_when_the_state_cannot_be_saved() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the state's folder should be: saving always fails.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "").unwrap();
    let (mut s, _) = service(dir.path(), Some(blocker.join("state.toml")));
    let reply = s.handle(ServiceCmd::SetLights {
        player: PlayerLights::Always,
        mic: false,
    });
    let msg = reply.into_result().unwrap();
    assert!(
        msg.starts_with("lights set") && msg.contains("cannot save"),
        "{msg}"
    );
    match s.handle_ipc(Request::Remap { on: false }) {
        Response::Ok { message } => assert!(message.contains("cannot save"), "{message}"),
        r => panic!("{r:?}"),
    }
    assert!(
        matches!(s.handle_ipc(Request::Status), Response::Status(_)),
        "only replies to changes carry it"
    );
}

#[test]
fn stop_forgets_a_failed_apply() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _) = service(dir.path(), None);
    let missing = dir.path().join("missing.toml");
    let r = s.handle_ipc(Request::Apply {
        path: missing.display().to_string(),
    });
    assert!(matches!(r, Response::Error { .. }), "{r:?}");
    let mentions = |s: &Service| s.status().warnings.iter().any(|w| w.contains("missing"));
    assert!(mentions(&s), "the failure is reported");
    s.handle_ipc(Request::Stop);
    assert!(!mentions(&s), "and forgotten once stopped");
}

#[test]
fn keep_offline_pads_holds_a_controllers_session() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_profile(dir.path(), "a.toml", "A", &[]);
    let (t, world) = TestService::fake(dir.path(), &[("p1", PadFamily::Xbox)]);
    let mut s = t.build();
    s.tick();
    s.handle(ServiceCmd::ApplyToSlot {
        device: Some("p1".into()),
        slot: 0,
        path: a,
    })
    .into_result()
    .unwrap();
    let owned = |s: &Service| {
        s.runtime()
            .sessions()
            .iter()
            .any(|x| x.owner.as_deref() == Some("p1"))
    };
    assert!(owned(&s));
    let p1 = world.lock().unwrap().pads.remove(0);
    s.tick();
    assert!(!owned(&s), "its session ends with it");
    world.lock().unwrap().pads.push(p1);
    s.tick();
    assert!(owned(&s), "and starts again");

    s.handle(ServiceCmd::SetRuntimeOptions(RuntimeOptions {
        keep_offline_pads: true,
        ..Default::default()
    }));
    world.lock().unwrap().pads.clear();
    s.tick();
    assert!(owned(&s), "kept while it is away");
}

#[test]
fn unrelated_settings_keep_the_idle_time() {
    let dir = tempfile::tempdir().unwrap();
    let (t, world) = TestService::fake(dir.path(), &[("p1", PadFamily::DualSense)]);
    let mut s = t.build();
    s.handle(ServiceCmd::SetPowerOffAfter(1));
    s.tick();
    s.rt.advance_ms(40_000);
    s.tick();
    s.handle(ServiceCmd::SetBlocked {
        id: "another".into(),
        blocked: true,
    });
    s.rt.advance_ms(25_000);
    s.tick();
    assert_eq!(
        world.lock().unwrap().powered_off.len(),
        1,
        "the idle minute counts on through the change"
    );
}
