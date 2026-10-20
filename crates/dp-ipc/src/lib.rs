//! Control protocol between the Doppelpad daemon and its clients (CLI, GUI).
//!
//! Transport: a local socket (a named pipe `\\.\pipe\doppelpad.sock` on Windows, an
//! abstract or file socket elsewhere). Each connection carries exactly one request
//! and one response, each a single line of JSON.

use std::io::{BufRead, BufReader, Write};

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream,
};
use serde::{Deserialize, Serialize};

pub const SOCKET_NAME: &str = "doppelpad.sock";

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
    pub foreground: Option<String>,
    pub virtual_pad: Option<String>,
    pub hidden_devices: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// OS instance paths used to hide the device (two for a Joy-Con pair).
    #[serde(default)]
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

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("the Doppelpad daemon is not running ({0})")]
    NotRunning(std::io::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
}

fn name() -> std::io::Result<Name<'static>> {
    if GenericNamespaced::is_supported() {
        SOCKET_NAME.to_ns_name::<GenericNamespaced>()
    } else {
        format!("/tmp/{SOCKET_NAME}").to_fs_name::<GenericFilePath>()
    }
}

/// Send one request to the daemon and wait for its response.
pub fn call(req: &Request) -> Result<Response, IpcError> {
    let stream = Stream::connect(name()?).map_err(IpcError::NotRunning)?;
    let mut conn = BufReader::new(stream);
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    conn.get_mut().write_all(line.as_bytes())?;
    let mut reply = String::new();
    conn.read_line(&mut reply)?;
    Ok(serde_json::from_str(reply.trim_end())?)
}

/// Serve requests forever, answering each with `handler`. Blocks the calling thread.
pub fn serve(mut handler: impl FnMut(Request) -> Response) -> Result<(), IpcError> {
    let listener = ListenerOptions::new()
        .name(name()?)
        .try_overwrite(true)
        .create_sync()?;
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let mut conn = BufReader::new(conn);
        let mut line = String::new();
        if conn.read_line(&mut line).is_err() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(line.trim_end()) {
            Ok(req) => {
                let shutdown = req == Request::Shutdown;
                let r = handler(req);
                if shutdown {
                    let _ = send(conn.get_mut(), &r);
                    return Ok(());
                }
                r
            }
            Err(e) => Response::Error {
                message: format!("bad request: {e}"),
            },
        };
        let _ = send(conn.get_mut(), &response);
    }
    Ok(())
}

fn send(w: &mut impl Write, r: &Response) -> Result<(), IpcError> {
    let mut line = serde_json::to_string(r)?;
    line.push('\n');
    w.write_all(line.as_bytes())?;
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
        let resp = Response::Ok {
            message: "fine".into(),
        };
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            r#"{"type":"ok","message":"fine"}"#
        );
    }

    #[test]
    fn device_hiding_paths() {
        let d: Device = serde_json::from_str(
            r#"{"id":"hid:x","name":"Pad","backend":"hid","vendor_id":1,"product_id":2}"#,
        )
        .unwrap();
        assert!(d.instance_paths.is_empty(), "older daemons send no paths");
        let pair = Device {
            instance_paths: vec!["A".into(), "B".into()],
            ..d
        };
        let json = serde_json::to_string(&pair).unwrap();
        assert!(json.contains(r#""instance_paths":["A","B"]"#), "{json}");
    }
}
