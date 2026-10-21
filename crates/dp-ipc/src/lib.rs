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
    pub foreground: Option<String>,
    pub virtual_pad: Option<String>,
    pub hidden_devices: usize,
    pub warnings: Vec<String>,
    /// Profile file of each slot (1–4), `None` when empty.
    #[serde(default)]
    pub slots: Vec<Option<String>>,
    /// The active slot, 1–4.
    #[serde(default)]
    pub active_slot: usize,
    #[serde(default)]
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
