//! The server as an assistant meets it: one JSON-RPC message in, one answer out.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use dp_ipc::{Device, IpcError, Request, Response, Status};
use serde_json::{Value, json};

use super::Server;

type Sent = Rc<RefCell<Vec<Request>>>;

/// A server on a library in `root` whose service answers with `service`; `sent` collects the
/// requests it gets.
fn server(root: &Path, sent: &Sent, service: fn(&Request) -> Result<Response, IpcError>) -> Server {
    let library = root.join("profiles");
    std::fs::create_dir_all(&library).unwrap();
    let sent = sent.clone();
    Server {
        library: Box::new(move || library.clone()),
        history: root.join("history"),
        service: Box::new(move |r| {
            sent.borrow_mut().push(r.clone());
            service(r)
        }),
    }
}

fn not_running(_: &Request) -> Result<Response, IpcError> {
    Err(IpcError::NotRunning(std::io::ErrorKind::NotFound.into()))
}

/// A service that takes every command, and has one controller.
fn running(r: &Request) -> Result<Response, IpcError> {
    Ok(match r {
        Request::Status => Response::Status(Status {
            profile: Some("Desktop".into()),
            profile_path: None,
            layer: 0,
            auto_switch: true,
            foreground: None,
            virtual_pad: None,
            hidden_devices: 0,
            warnings: Vec::new(),
            slots: vec![None; dp_service::SLOTS],
            active_slot: 1,
            remap_on: true,
        }),
        Request::Devices => Response::Devices {
            devices: vec![Device {
                id: "hid:1".into(),
                name: "DualSense".into(),
                backend: "hid".into(),
                vendor_id: 0x054c,
                product_id: 0x0ce6,
                instance_paths: Vec::new(),
            }],
        },
        _ => Response::ok("done"),
    })
}

/// Send request 1; its result, or its error as `Err`.
fn ask(s: &Server, method: &str, params: Value) -> Result<Value, Value> {
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let reply: Value = serde_json::from_str(&s.handle(&line.to_string()).unwrap()).unwrap();
    assert_eq!(
        (&reply["jsonrpc"], &reply["id"]),
        (&json!("2.0"), &json!(1))
    );
    match reply.get("error") {
        Some(e) => Err(e.clone()),
        None => Ok(reply["result"].clone()),
    }
}

/// Call a tool; its text, as `Err` when it is marked as an error.
fn call(s: &Server, tool: &str, args: Value) -> Result<String, String> {
    let r = ask(s, "tools/call", json!({ "name": tool, "arguments": args })).unwrap();
    let text = r["content"][0]["text"].as_str().unwrap().to_owned();
    if r["isError"] == true {
        Err(text)
    } else {
        Ok(text)
    }
}

/// What list_profiles says, read back.
fn library(s: &Server) -> Value {
    serde_json::from_str(&call(s, "list_profiles", json!({})).unwrap()).unwrap()
}

/// The path of the first config of the first game, as the tools give it.
fn first_config(s: &Server) -> String {
    library(s)["games"][0]["configs"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// That config's file, under the library in `root`.
fn first_file(s: &Server, root: &Path) -> PathBuf {
    root.join("profiles").join(first_config(s))
}

const SIMPLE: &str =
    "name = \"Gyro aim\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{ key = \"Space\" }]\n";

#[test]
fn it_answers_in_the_version_asked_for_when_it_speaks_it() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), not_running);
    let hello = json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": { "name": "test", "version": "1" },
    });
    let r = ask(&s, "initialize", hello).unwrap();
    assert_eq!(r["protocolVersion"], "2025-06-18");
    assert!(r["capabilities"]["tools"].is_object());
    assert_eq!(r["serverInfo"]["name"], "doppelpad");
    assert!(
        r["instructions"]
            .as_str()
            .unwrap()
            .contains("profile_reference")
    );
    let r = ask(&s, "initialize", json!({ "protocolVersion": "2099-01-01" })).unwrap();
    assert_eq!(r["protocolVersion"], *super::VERSIONS.last().unwrap());
}

#[test]
fn notifications_get_no_answer_and_mistakes_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), not_running);
    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    assert_eq!(s.handle(initialized), None);
    assert_eq!(s.handle("  "), None);
    assert_eq!(s.handle(r#"{"jsonrpc":"2.0","id":7,"result":{}}"#), None);
    assert_eq!(ask(&s, "ping", Value::Null).unwrap(), json!({}));
    assert_eq!(
        ask(&s, "resources/list", json!({})).unwrap_err()["code"],
        -32601
    );
    let code = |line: &str| {
        serde_json::from_str::<Value>(&s.handle(line).unwrap()).unwrap()["error"]["code"].clone()
    };
    assert_eq!(code("{not json"), -32700);
    assert_eq!(code("[]"), -32600);
    assert_eq!(
        ask(&s, "tools/call", json!({ "name": "format_disk" })).unwrap_err()["code"],
        -32602
    );
}

#[test]
fn every_listed_tool_can_be_called() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), not_running);
    let tools = ask(&s, "tools/list", json!({})).unwrap()["tools"].clone();
    let tools = tools.as_array().unwrap();
    assert_eq!(tools.len(), 7);
    for t in tools {
        let name = t["name"].as_str().unwrap();
        let schema = &t["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        for field in schema["required"].as_array().into_iter().flatten() {
            assert!(
                schema["properties"][field.as_str().unwrap()].is_object(),
                "{name}"
            );
        }
        assert!(t["description"].as_str().unwrap().len() > 80, "{name}");
        // Each runs, if only to say that its arguments are missing.
        assert!(s.tool(name, json!({})).is_some(), "{name}");
    }
}

#[test]
fn the_reference_is_the_whole_profile_doc() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), not_running);
    let text = call(&s, "profile_reference", json!({})).unwrap();
    assert!(text.starts_with("# Profile reference"), "{}", &text[..40]);
    assert!(text.contains("## All names") && !text.contains('\r'));
}

#[test]
fn check_says_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), not_running);
    let check = |toml: &str| call(&s, "check_profile", json!({ "toml": toml })).unwrap();
    let good = check(SIMPLE);
    assert!(
        good.contains("Gyro aim: 1 mappings") && good.ends_with("No errors."),
        "{good}"
    );
    let unreadable = check("[[mappings]]\ntrigger = \"A\"\nactions = [{ pad = \"Cross\" }]\n");
    assert!(
        unreadable.contains("write `A` for `Cross`") && unreadable.contains("does not load"),
        "{unreadable}"
    );
    let invalid = check(
        "[[mappings]]\ntrigger = \"A\"\nmode = \"once\"\nactions = [{ key = \"A\" }, { break = false }]\n",
    );
    assert!(
        invalid.contains("break = false") && invalid.ends_with("It has errors."),
        "{invalid}"
    );
    let wrong = call(&s, "check_profile", json!({ "profile": SIMPLE })).unwrap_err();
    assert!(wrong.contains("unknown field `profile`"), "{wrong}");
}

#[test]
fn a_saved_profile_lands_in_its_game_and_keeps_its_old_version() {
    let dir = tempfile::tempdir().unwrap();
    let sent = Sent::default();
    let s = server(dir.path(), &sent, running);
    let said = call(
        &s,
        "save_profile",
        json!({ "toml": SIMPLE, "game": "Elden Ring" }),
    )
    .unwrap();
    assert!(
        said.starts_with("Made the game \"Elden Ring\".\nSaved \"Gyro aim\""),
        "{said}"
    );
    assert_eq!(
        *sent.borrow(),
        [Request::Reload],
        "the service lists the library anew"
    );

    let list = library(&s);
    let game = &list["games"][0];
    assert_eq!(game["game"], "Elden Ring");
    assert_eq!(game["folder"], "elden-ring");
    assert_eq!(game["configs"][0]["name"], "Gyro aim");
    assert_eq!(game["configs"][0]["default"], true);
    let path = first_config(&s);
    assert_eq!(path, "elden-ring/gyro-aim.toml");

    let other = SIMPLE.replace("Gyro aim", "Stick aim");
    let said = call(
        &s,
        "save_profile",
        json!({ "toml": other, "game": "elden ring" }),
    )
    .unwrap();
    assert!(
        said.starts_with("Saved \"Stick aim\""),
        "the same game: {said}"
    );

    let changed = SIMPLE.replace("Space", "Enter");
    call(&s, "save_profile", json!({ "toml": changed, "path": path })).unwrap();
    let read = call(&s, "read_profile", json!({ "path": path })).unwrap();
    assert!(
        read.contains("key = \"Enter\"") && read.ends_with("No errors."),
        "{read}"
    );
    // The whole path names it too.
    let file = first_file(&s, dir.path());
    assert_eq!(call(&s, "read_profile", json!({ "path": file })), Ok(read));
    let library = dir.path().join("profiles");
    let kept = dp_config::history::versions(&file, &dir.path().join("history"), &library);
    assert_eq!(kept.len(), 1);
    assert!(
        std::fs::read_to_string(&kept[0].path)
            .unwrap()
            .contains("Space")
    );
}

#[test]
fn nothing_is_saved_that_has_errors_or_lies_outside_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), running);
    let outside = dir.path().join("outside.toml");
    std::fs::write(&outside, SIMPLE).unwrap();
    call(&s, "save_profile", json!({ "toml": SIMPLE, "game": "G" })).unwrap();
    let game_file = dir.path().join("profiles").join("g").join("game.toml");
    assert!(game_file.is_file());
    for path in [&outside, &game_file] {
        let tries = [
            ("read_profile", json!({ "path": path })),
            ("apply_profile", json!({ "path": path })),
            ("save_profile", json!({ "toml": SIMPLE, "path": path })),
        ];
        for (tool, args) in tries {
            let e = call(&s, tool, args).unwrap_err();
            assert!(e.contains("is no config in the library"), "{tool}: {e}");
        }
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), SIMPLE);

    let e = call(&s, "save_profile", json!({ "toml": SIMPLE })).unwrap_err();
    assert!(e.starts_with("Give either `path`"), "{e}");
    let broken = "[[mappings]]\ntrigger = \"A\"\nmode = \"once\"\nactions = [{ break = false }]\n";
    let e = call(&s, "save_profile", json!({ "toml": broken, "game": "G" })).unwrap_err();
    assert!(
        e.ends_with("Nothing was saved: fix the errors first."),
        "{e}"
    );
    assert_eq!(first_config(&s), "g/gyro-aim.toml");
}

#[test]
fn launch_commands_do_not_come_in_this_way() {
    let dir = tempfile::tempdir().unwrap();
    let sent = Sent::default();
    let s = server(dir.path(), &sent, running);
    let launch = "name = \"Tools\"\n[[mappings]]\ntrigger = \"A\"\nactions = [{ command = { launch = { path = 'C:\\x.exe' } } }]\n";
    let e = call(&s, "save_profile", json!({ "toml": launch, "game": "G" })).unwrap_err();
    assert!(
        e.contains("main layer, A: C:\\x.exe") && e.contains("Nothing was saved"),
        "{e}"
    );

    // A launch command the user put in a config stays when an assistant changes the rest...
    call(&s, "save_profile", json!({ "toml": SIMPLE, "game": "G" })).unwrap();
    let path = first_config(&s);
    std::fs::write(first_file(&s, dir.path()), launch).unwrap();
    let more = format!("{launch}[[mappings]]\ntrigger = \"B\"\nactions = [{{ key = \"W\" }}]\n");
    call(&s, "save_profile", json!({ "toml": more, "path": path })).unwrap();
    let read = call(&s, "read_profile", json!({ "path": path })).unwrap();
    assert!(
        read.contains("x.exe") && read.contains("note: it starts programs"),
        "{read}"
    );
    // ...but no other comes in with it, and the config is applied in the app only.
    let other = more.replace("x.exe", "y.exe");
    let e = call(&s, "save_profile", json!({ "toml": other, "path": path })).unwrap_err();
    assert!(e.contains("y.exe") && !e.contains("x.exe"), "{e}");
    sent.borrow_mut().clear();
    let e = call(&s, "apply_profile", json!({ "path": path })).unwrap_err();
    assert!(e.contains("the user applies it, in the app"), "{e}");
    assert!(sent.borrow().is_empty());
}

#[test]
fn apply_and_status_ask_the_service() {
    let dir = tempfile::tempdir().unwrap();
    let sent = Sent::default();
    let s = server(dir.path(), &sent, running);
    call(&s, "save_profile", json!({ "toml": SIMPLE, "game": "G" })).unwrap();
    let path = first_config(&s);
    sent.borrow_mut().clear();
    assert_eq!(
        call(&s, "apply_profile", json!({ "path": path, "slot": 2 })).unwrap(),
        "done"
    );
    assert!(
        matches!(
            &sent.borrow()[..],
            [Request::ApplyToSlot {
                slot: 2,
                device: None,
                path,
            }] if Path::new(path).is_absolute()
        ),
        "{:?}",
        sent.borrow()
    );
    let e = call(&s, "apply_profile", json!({ "path": path, "slot": 5 })).unwrap_err();
    assert!(e.contains("no slot 5"), "{e}");
    let status: Value = serde_json::from_str(&call(&s, "status", json!({})).unwrap()).unwrap();
    assert_eq!(status["status"]["profile"], "Desktop");
    assert_eq!(status["controllers"][0]["name"], "DualSense");

    let s = server(dir.path(), &Sent::default(), not_running);
    assert!(
        call(&s, "status", json!({}))
            .unwrap()
            .contains("not running")
    );
    let e = call(&s, "apply_profile", json!({ "path": path })).unwrap_err();
    assert!(e.contains("not running"), "{e}");
}

#[test]
fn programs_a_profile_switches_on_go_to_its_game() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), &Sent::default(), running);
    let aim = format!(
        "processes = [\"eldenring.exe\"]
{SIMPLE}"
    );
    let said = call(
        &s,
        "save_profile",
        json!({ "toml": aim, "game": "Elden Ring" }),
    )
    .unwrap();
    assert!(
        said.contains("\"Elden Ring\" now switches to its default config, \"Gyro aim\", while eldenring.exe is in front."),
        "{said}"
    );
    assert_eq!(
        library(&s)["games"][0]["processes"],
        json!(["eldenring.exe"])
    );

    // Known already: nothing to add, and a program the user took off the game stays off.
    let path = first_config(&s);
    let said = call(&s, "save_profile", json!({ "toml": aim, "path": path })).unwrap();
    assert!(!said.contains("now switches"), "{said}");
    let game = dir.path().join("profiles").join("elden-ring");
    let mut meta = dp_config::library::read_game_meta(&game).unwrap();
    meta.processes.clear();
    dp_config::library::update_game_meta(&game, &meta).unwrap();
    call(&s, "save_profile", json!({ "toml": aim, "path": path })).unwrap();
    assert_eq!(library(&s)["games"][0]["processes"], json!([]));

    // A loose config switches on its own processes.
    std::fs::write(
        dir.path().join("profiles").join("desk.toml"),
        aim.replace("Gyro aim", "Desk"),
    )
    .unwrap();
    let said = call(
        &s,
        "save_profile",
        json!({ "toml": aim, "path": "desk.toml" }),
    )
    .unwrap();
    assert!(!said.contains("now switches"), "{said}");
    assert_eq!(
        library(&s)["loose_configs"][0]["processes"],
        json!(["eldenring.exe"])
    );
}
