//! Control protocol between the Doppelpad daemon and its clients (CLI, GUI).
//!
//! Transport: a local socket (a named pipe `\\.\pipe\doppelpad.sock` on Windows; a file socket
//! in the user's runtime folder, `$XDG_RUNTIME_DIR`, on Linux and in the user's temporary
//! folder on macOS, where the daemon also answers only processes of its own user). Each
//! connection carries exactly one request and one response, each a single line of JSON.
//!
//! The command-line tool and the app can be of different versions, so the protocol only
//! grows: new reply fields get a default, new requests are new variants (an older daemon
//! answers those with "bad request"), and nothing is renamed.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use serde::{Deserialize, Serialize};

const SOCKET_NAME: &str = "doppelpad.sock";
/// Longest request line the daemon reads.
const MAX_REQUEST: u64 = 64 * 1024;
/// Connections the daemon reads requests from at once; more are turned away.
const MAX_PENDING: usize = 16;
/// How long the daemon waits for a request line (where the platform has read timeouts).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

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
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        socket.to_owned().to_ns_name::<GenericNamespaced>()
    }
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        socket_dir()?.join(socket).to_fs_name::<GenericFilePath>()
    }
}

/// Where the socket file goes: a folder only its user can enter. Not an abstract socket on
/// Linux (anyone could connect to it) and not `/tmp` itself, which all users share.
#[cfg(unix)]
fn socket_dir() -> std::io::Result<std::path::PathBuf> {
    // Asked of the system rather than read from `TMPDIR`, so a shell that changes `TMPDIR`
    // still finds the app's socket.
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
            return Ok(std::path::PathBuf::from(dir.to_string_lossy().into_owned()));
        }
    }
    // The session's runtime folder (0700), else one of our own in `/tmp`.
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        && dir.is_absolute()
    {
        return Ok(dir);
    }
    private_dir(std::path::Path::new(&format!(
        "/tmp/doppelpad-{}",
        own_uid()
    )))
}

/// `dir`, made if missing, that only we can enter; refused when someone else owns it or can
/// get in.
#[cfg(unix)]
fn private_dir(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != own_uid() || meta.mode() & 0o077 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is not a private folder of this user", dir.display()),
        ));
    }
    Ok(dir.to_path_buf())
}

#[cfg(unix)]
fn own_uid() -> u32 {
    // SAFETY: geteuid cannot fail and has no preconditions.
    unsafe { libc::geteuid() }
}

/// Whether the peer of `conn` runs as our user (always on Windows, where the pipe's default
/// security already keeps other users out).
fn same_user(conn: &Stream) -> bool {
    #[cfg(unix)]
    {
        conn.peer_creds().is_ok_and(|c| c.euid() == Some(own_uid()))
    }
    #[cfg(windows)]
    {
        let _ = conn;
        true
    }
}

/// Send one request to the daemon and wait for its response.
pub fn call(req: &Request) -> Result<Response, IpcError> {
    call_on(SOCKET_NAME, req, CALL_TIMEOUT)
}

/// Whether a daemon answers at all, whatever its version: even a reply this client cannot
/// read means one runs (a check for one to take over must not start a second one).
pub fn is_running() -> bool {
    running_on(SOCKET_NAME)
}

fn running_on(socket: &str) -> bool {
    // Once connected, something listens, whether it answers in time or understandably.
    !matches!(
        call_on(socket, &Request::Status, CALL_TIMEOUT),
        Err(IpcError::NotRunning(_))
    )
}

fn call_on(socket: &str, req: &Request, timeout: Duration) -> Result<Response, IpcError> {
    let name = name(socket).map_err(IpcError::NotRunning)?;
    let stream = Stream::connect(name).map_err(IpcError::NotRunning)?;
    if !same_user(&stream) {
        return Err(IpcError::NotRunning(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the socket belongs to another user",
        )));
    }
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
    let reply = rx.recv_timeout(timeout).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("the daemon did not answer within {} s", timeout.as_secs()),
        )
    })??;
    let response: Response = serde_json::from_str(reply.trim_end())?;
    Ok(match response {
        // A request this daemon does not know: it is older than this client.
        Response::Error { message } if message.starts_with(BAD_REQUEST) => Response::error(
            format!("{message} (the running daemon is of another Doppelpad version; restart it)"),
        ),
        r => r,
    })
}

/// How the daemon's answer to a request it cannot read starts.
const BAD_REQUEST: &str = "bad request: ";

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
        let pending = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new()
            .name("dp-ipc-accept".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(conn) = conn else { continue };
                    if !same_user(&conn) || pending.load(Ordering::Relaxed) >= MAX_PENDING {
                        continue; // dropped: closed without an answer
                    }
                    // Where reads can time out, a silent client lets go of its slot.
                    let _ = conn.set_recv_timeout(Some(REQUEST_TIMEOUT));
                    pending.fetch_add(1, Ordering::Relaxed);
                    let (tx, slot) = (tx.clone(), pending.clone());
                    // Each request is read on a thread of its own, so a client that connects
                    // and sends nothing cannot hold up the others (named pipes have no read
                    // timeouts on Windows).
                    let spawned = std::thread::Builder::new()
                        .name("dp-ipc-conn".into())
                        .spawn(move || {
                            let mut conn = BufReader::new(conn);
                            let mut line = String::new();
                            let read = (&mut conn).take(MAX_REQUEST).read_line(&mut line);
                            slot.fetch_sub(1, Ordering::Relaxed);
                            // A line cut at the limit is not read as a request.
                            if read.is_ok_and(|n| (n as u64) < MAX_REQUEST) {
                                let _ = tx.send((line, conn.into_inner()));
                            }
                        });
                    if spawned.is_err() {
                        pending.fetch_sub(1, Ordering::Relaxed);
                    }
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
            Err(e) => Response::error(format!("{BAD_REQUEST}{e}")),
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

    /// Over-long requests are not read, unknown ones get a hint at another version, and any
    /// answer counts as a running daemon.
    #[test]
    fn long_and_unknown_requests() {
        let socket = format!("doppelpad-test-long-{}.sock", std::process::id());
        assert!(!running_on(&socket), "nothing listens yet");
        let server = {
            let socket = socket.clone();
            std::thread::spawn(move || serve_on(&socket, |req| Response::ok(format!("{req:?}"))))
        };
        let start = std::time::Instant::now();
        while !running_on(&socket) {
            assert!(start.elapsed() < Duration::from_secs(5), "never listened");
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut long = Stream::connect(name(&socket).unwrap()).unwrap();
        let huge = format!(
            "{{\"cmd\":\"apply\",\"path\":\"{}\"}}\n",
            "x".repeat(70_000)
        );
        let _ = long.write_all(huge.as_bytes());
        let mut reply = String::new();
        let _ = BufReader::new(&mut long).read_line(&mut reply);
        assert_eq!(reply, "", "closed without an answer");

        let mut newer = Stream::connect(name(&socket).unwrap()).unwrap();
        newer.write_all(b"{\"cmd\":\"teleport\"}\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&mut newer).read_line(&mut reply).unwrap();
        assert!(reply.contains("bad request: unknown variant"), "{reply}");

        call_on(&socket, &Request::Shutdown, Duration::from_secs(3)).unwrap();
        server.join().unwrap().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_folder_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("dp-ipc-private-{}", std::process::id()));
        let dir = private_dir(&base).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_dir(&base).is_err(), "others could get in");
        std::fs::remove_dir(&base).unwrap();
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
