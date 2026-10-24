//! Control protocol between the Doppelpad daemon and its clients (CLI, GUI).
//!
//! Transport: a local socket (a named pipe `\\.\pipe\doppelpad.sock` on Windows, an
//! abstract socket on Linux, a file socket in the user's temporary folder on macOS). Each
//! connection carries exactly one request and one response, each a single line of JSON.

use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream,
};
use serde::{Deserialize, Serialize};

const SOCKET_NAME: &str = "doppelpad.sock";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    /// Load and activate a profile file (TOML or importable JSON).
    Apply {
        path: String,
    },
    /// Deactivate the current profile (release everything, unhide devices).
    Stop,
    /// Put a profile file into a slot (1–4), make that slot active and turn remapping on.
    ApplyToSlot {
        path: String,
        slot: usize,
        /// The slots of this controller (`doppelpad devices` lists the ids) instead of the
        /// shared ones; it gets slots of its own if it has none yet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
    },
    /// Make a slot (1–4) active and turn remapping on.
    SelectSlot {
        slot: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
    },
    /// Empty a slot (1–4), or every slot.
    ClearSlot {
        slot: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
    },
    /// Turn remapping on (the active slot) or off.
    Remap {
        on: bool,
    },
    Devices,
    /// Enable or disable automatic profile switching by foreground application.
    AutoSwitch {
        enabled: bool,
    },
    /// Re-scan the profiles directory.
    Reload,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub profile: Option<String>,
    pub profile_path: Option<String>,
    pub layer: u8,
    pub auto_switch: bool,
    /// Executable of the application in front.
    pub foreground: Option<String>,
    pub virtual_pad: Option<String>,
    pub hidden_devices: usize,
    pub warnings: Vec<String>,
    /// Profile file of each slot (1–4), `None` when empty.
    pub slots: Vec<Option<String>>,
    /// The active slot, 1–4.
    pub active_slot: usize,
    pub remap_on: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// OS instance paths used to hide the device (two for a Joy-Con pair).
    pub instance_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok { message: String },
    Error { message: String },
    Status(Status),
    Devices { devices: Vec<Device> },
}

impl Response {
    pub fn ok(message: impl Into<String>) -> Self {
        Response::Ok {
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Response::Error {
            message: message.into(),
        }
    }
}

impl From<Result<String, String>> for Response {
    fn from(r: Result<String, String>) -> Self {
        match r {
            Ok(message) => Response::ok(message),
            Err(message) => Response::error(message),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("the Doppelpad daemon is not running ({0})")]
    NotRunning(std::io::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
}

/// How long a client waits for the daemon's answer.
const CALL_TIMEOUT: Duration = Duration::from_secs(15);

fn name(socket: &str) -> std::io::Result<Name<'static>> {
    // macOS has no abstract sockets: interprocess would put a namespaced name in `/tmp`.
    if GenericNamespaced::is_supported() && !cfg!(target_os = "macos") {
        socket.to_owned().to_ns_name::<GenericNamespaced>()
    } else {
        socket_dir().join(socket).to_fs_name::<GenericFilePath>()
    }
}

/// Where file sockets go: on macOS the user's own temporary folder rather than `/tmp`, which
/// all users share (a socket another user left there could not be replaced). Asked of the
/// system rather than read from `TMPDIR`, so a shell that changes `TMPDIR` still finds the
/// app's socket.
fn socket_dir() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0u8; 1024];
        // SAFETY: the buffer and its length match; confstr writes at most that many bytes,
        // NUL included.
        let n = unsafe {
            libc::confstr(
                libc::_CS_DARWIN_USER_TEMP_DIR,
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if n > 0 && n <= buf.len() {
            let dir = std::ffi::CStr::from_bytes_until_nul(&buf).unwrap_or_default();
            return std::path::PathBuf::from(dir.to_string_lossy().into_owned());
        }
    }
    std::path::PathBuf::from("/tmp")
}

/// Send one request to the daemon and wait for its response.
pub fn call(req: &Request) -> Result<Response, IpcError> {
    call_on(SOCKET_NAME, req, CALL_TIMEOUT)
}

fn call_on(socket: &str, req: &Request, timeout: Duration) -> Result<Response, IpcError> {
    let stream = Stream::connect(name(socket)?).map_err(IpcError::NotRunning)?;
    let line = json_line(req)?;
    // Named pipes have no timeouts on Windows: the exchange runs on a helper thread, and a
    // daemon that does not answer in time is given up on.
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("dp-ipc-call".into())
        .spawn(move || {
            let mut conn = BufReader::new(stream);
            let mut reply = String::new();
            let result = conn
                .get_mut()
                .write_all(line.as_bytes())
                .and_then(|()| conn.read_line(&mut reply))
                .map(|_| reply);
            let _ = tx.send(result);
        })?;
    let reply = rx
        .recv_timeout(timeout)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))??;
    Ok(serde_json::from_str(reply.trim_end())?)
}

/// Serve requests, answering each with `handler`, until a `shutdown` the handler accepts (one
/// it answers with an error keeps the server running). Blocks the calling thread.
pub fn serve(handler: impl FnMut(Request) -> Response) -> Result<(), IpcError> {
    serve_on(SOCKET_NAME, handler)
}

fn serve_on(socket: &str, mut handler: impl FnMut(Request) -> Response) -> Result<(), IpcError> {
    let listener = ListenerOptions::new()
        .name(name(socket)?)
        .try_overwrite(true)
        .create_sync()?;
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, requests) = mpsc::channel::<(String, Stream)>();
    let accept = {
        let stop = stop.clone();
        std::thread::Builder::new()
            .name("dp-ipc-accept".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(conn) = conn else { continue };
                    let tx = tx.clone();
                    // Each request is read on a thread of its own, so a client that connects
                    // and sends nothing cannot hold up the others (named pipes have no read
                    // timeouts on Windows).
                    let _ = std::thread::Builder::new()
                        .name("dp-ipc-conn".into())
                        .spawn(move || {
                            let mut conn = BufReader::new(conn);
                            let mut line = String::new();
                            if conn.read_line(&mut line).is_ok() {
                                let _ = tx.send((line, conn.into_inner()));
                            }
                        });
                }
            })?
    };
    // Requests are answered one after the other, on this thread.
    for (line, mut conn) in &requests {
        let response = match serde_json::from_str::<Request>(line.trim_end()) {
            Ok(req) => {
                let shutdown = req == Request::Shutdown;
                let r = handler(req);
                if shutdown && !matches!(r, Response::Error { .. }) {
                    let _ = write_line(&mut conn, &r);
                    // Wake the accept loop so that it sees the flag and closes the socket.
                    stop.store(true, Ordering::Relaxed);
                    if Stream::connect(name(socket)?).is_ok() {
                        let _ = accept.join();
                    }
                    return Ok(());
                }
                r
            }
            Err(e) => Response::Error {
                message: format!("bad request: {e}"),
            },
        };
        let _ = write_line(&mut conn, &response);
    }
    Ok(())
}

/// One message as it travels: JSON on a single line.
fn json_line(value: &impl Serialize) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    Ok(line)
}

fn write_line(w: &mut impl Write, value: &impl Serialize) -> Result<(), IpcError> {
    w.write_all(json_line(value)?.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format() {
        let r = Request::Apply {
            path: r"C:\p.toml".into(),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"cmd":"apply","path":"C:\\p.toml"}"#
        );
        let back: Request =
            serde_json::from_str(r#"{"cmd":"auto_switch","enabled":true}"#).unwrap();
        assert_eq!(back, Request::AutoSwitch { enabled: true });
        assert_eq!(
            serde_json::to_string(&Request::ClearSlot {
                slot: None,
                device: None
            })
            .unwrap(),
            r#"{"cmd":"clear_slot","slot":null}"#
        );
        let back: Request = serde_json::from_str(r#"{"cmd":"remap","on":false}"#).unwrap();
        assert_eq!(back, Request::Remap { on: false });
        let resp = Response::Ok {
            message: "fine".into(),
        };
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            r#"{"type":"ok","message":"fine"}"#
        );
    }

    /// The JSON of the two structured replies, field by field, so that format changes are
    /// deliberate (the CLI, the GUI and scripts read them).
    #[test]
    fn reply_json() {
        let status = Response::Status(Status {
            profile: Some("Racing".into()),
            profile_path: Some(r"C:\p\racing.toml".into()),
            layer: 2,
            auto_switch: true,
            foreground: Some("game.exe".into()),
            virtual_pad: Some("xbox360".into()),
            hidden_devices: 1,
            warnings: vec!["ViGEmBus is missing".into()],
            slots: vec![Some(r"C:\p\racing.toml".into()), None, None, None],
            active_slot: 1,
            remap_on: true,
        });
        let json = concat!(
            r#"{"type":"status","profile":"Racing","profile_path":"C:\\p\\racing.toml","#,
            r#""layer":2,"auto_switch":true,"foreground":"game.exe","virtual_pad":"xbox360","#,
            r#""hidden_devices":1,"warnings":["ViGEmBus is missing"],"#,
            r#""slots":["C:\\p\\racing.toml",null,null,null],"active_slot":1,"remap_on":true}"#
        );
        assert_eq!(serde_json::to_string(&status).unwrap(), json);
        assert_eq!(serde_json::from_str::<Response>(json).unwrap(), status);

        let devices = Response::Devices {
            devices: vec![Device {
                id: "hid:pad".into(),
                name: "DualSense".into(),
                backend: "hid".into(),
                vendor_id: 0x054C,
                product_id: 0x0CE6,
                instance_paths: vec![r"HID\VID_054C&PID_0CE6\1".into()],
            }],
        };
        let json = concat!(
            r#"{"type":"devices","devices":[{"id":"hid:pad","name":"DualSense","backend":"hid","#,
            r#""vendor_id":1356,"product_id":3302,"instance_paths":["HID\\VID_054C&PID_0CE6\\1"]}]}"#
        );
        assert_eq!(serde_json::to_string(&devices).unwrap(), json);
        assert_eq!(serde_json::from_str::<Response>(json).unwrap(), devices);
    }

    /// A client that connects and never sends a request must not stop the daemon from
    /// answering everyone else.
    #[test]
    fn a_silent_client_does_not_block_the_others() {
        // Not the real socket name: the installed app may be listening on that one.
        let socket = format!("doppelpad-test-{}.sock", std::process::id());
        let server = {
            let socket = socket.clone();
            std::thread::spawn(move || {
                serve_on(&socket, |req| Response::Ok {
                    message: format!("{req:?}"),
                })
            })
        };
        let start = std::time::Instant::now();
        let silent = loop {
            match Stream::connect(name(&socket).unwrap()) {
                Ok(s) => break s,
                Err(_) if start.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("the server does not listen: {e}"),
            }
        };
        let r = call_on(&socket, &Request::Status, Duration::from_secs(3)).unwrap();
        assert_eq!(
            r,
            Response::Ok {
                message: "Status".into()
            }
        );
        call_on(&socket, &Request::Shutdown, Duration::from_secs(3)).unwrap();
        server.join().unwrap().unwrap();
        drop(silent);
    }

    /// A host that refuses `shutdown` (the app) keeps answering afterwards.
    #[test]
    fn a_refused_shutdown_keeps_serving() {
        let socket = format!("doppelpad-test-refuse-{}.sock", std::process::id());
        let server = {
            let socket = socket.clone();
            let mut asked = 0;
            std::thread::spawn(move || {
                serve_on(&socket, |req| match req {
                    Request::Shutdown if asked == 0 => {
                        asked += 1;
                        Response::error("no")
                    }
                    _ => Response::ok(format!("{req:?}")),
                })
            })
        };
        let start = std::time::Instant::now();
        let refused = loop {
            match call_on(&socket, &Request::Shutdown, Duration::from_secs(3)) {
                Ok(r) => break r,
                Err(_) if start.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("the server does not listen: {e}"),
            }
        };
        assert_eq!(refused, Response::error("no"));
        let r = call_on(&socket, &Request::Status, Duration::from_secs(3)).unwrap();
        assert_eq!(r, Response::ok("Status"));
        call_on(&socket, &Request::Shutdown, Duration::from_secs(3)).unwrap();
        server.join().unwrap().unwrap();
    }

    #[test]
    fn device_hiding_paths() {
        let pair = Device {
            id: "hid:x".into(),
            name: "Joy-Con (L/R)".into(),
            backend: "hid".into(),
            vendor_id: 1,
            product_id: 2,
            instance_paths: vec!["A".into(), "B".into()],
        };
        let json = serde_json::to_string(&pair).unwrap();
        assert!(json.contains(r#""instance_paths":["A","B"]"#), "{json}");
        assert_eq!(serde_json::from_str::<Device>(&json).unwrap(), pair);
        let without = r#"{"id":"hid:x","name":"Pad","backend":"hid","vendor_id":1,"product_id":2}"#;
        assert!(
            serde_json::from_str::<Device>(without).is_err(),
            "the paths are required"
        );
    }
}
