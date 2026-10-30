//! `doppelpad mcp`: a Model Context Protocol server on stdin and stdout, so that an AI assistant
//! can write profiles from what the user asks for. JSON-RPC 2.0, one message per line. Its
//! tools ([`tools`]) read the profile reference, check profiles, list, read and save them in
//! the library like the app, and apply them through the running service. They reach no file
//! outside the library, and no launch command comes in through them.

mod tools;

#[cfg(test)]
mod tests;

use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;

use anyhow::Result;
use dp_ipc::{IpcError, Request, Response};
use serde_json::{Value, json};

/// The protocol versions this server speaks, the newest last. It uses only tools with text
/// results, which are alike in all of them.
const VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// What the assistant is told once, when it connects.
const INSTRUCTIONS: &str = "Doppelpad remaps game controllers. A profile (TOML) maps a \
controller's buttons, sticks, triggers, gyro and touchpad to keys, the mouse, a virtual \
controller and more. Read profile_reference before writing a profile: it names every control and \
action exactly. Check a profile with check_profile (the reference's `doppelpad check`) and fix \
every error before save_profile. Saved profiles go to the user's library, where the app shows \
them; apply_profile lets the user try one at once. Profiles cannot start programs through these \
tools: the user adds launch commands in the app.";

// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// The answer to a request: its result, or an error code and message.
type Reply = Result<Value, (i64, String)>;

/// A way to send a request to the running service.
type Service = dyn Fn(&Request) -> Result<Response, IpcError>;

/// What the tools work on: the library and the running service.
pub(crate) struct Server {
    /// The library folder, asked anew for each call: the user may move it while an assistant
    /// stays connected.
    library: Box<dyn Fn() -> PathBuf>,
    /// Where earlier versions of a config go before it is saved over.
    history: PathBuf,
    /// Sends a request to the running service (the app or `doppelpad daemon`).
    service: Box<Service>,
}

/// Serve the assistant on stdin and stdout until it closes stdin.
pub fn run() -> Result<()> {
    let server = Server {
        library: Box::new(dp_config::profiles_dir),
        history: dp_config::history_dir(),
        service: Box::new(dp_ipc::call),
    };
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        if let Some(reply) = server.handle(&line?) {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}

impl Server {
    /// Answer one message; notifications, and answers to requests (this server sends none),
    /// get no answer.
    fn handle(&self, line: &str) -> Option<String> {
        if line.trim().is_empty() {
            return None;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(msg) => msg,
            Err(e) => return Some(answer(&Value::Null, Err((PARSE_ERROR, e.to_string())))),
        };
        let id = msg.get("id");
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            if msg.get("result").is_some() || msg.get("error").is_some() {
                return None;
            }
            let why = "not a JSON-RPC request (one message per line, no batches)";
            return Some(answer(
                id.unwrap_or(&Value::Null),
                Err((INVALID_REQUEST, why.into())),
            ));
        };
        // A notification (`notifications/initialized`, `notifications/cancelled`) asks for
        // nothing this server does.
        let id = id?;
        let params = msg.get("params").unwrap_or(&Value::Null);
        let reply = match method {
            "initialize" => Ok(initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::list() })),
            "tools/call" => self.call_tool(params),
            _ => Err((METHOD_NOT_FOUND, format!("no method `{method}`"))),
        };
        Some(answer(id, reply))
    }

    /// Run a tool. What it says goes back as text; when it did not do what it was asked, the
    /// text says why and is marked as an error, for the assistant to correct its call.
    fn call_tool(&self, params: &Value) -> Reply {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Err((INVALID_PARAMS, "tools/call needs the tool's `name`".into()));
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let Some(outcome) = self.tool(name, args) else {
            return Err((INVALID_PARAMS, format!("no tool `{name}`")));
        };
        let (text, is_error) = match outcome {
            Ok(text) => (text, false),
            Err(text) => (text, true),
        };
        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": is_error,
        }))
    }
}

/// The answer to `initialize`: the version the client asked for when this server speaks it,
/// else the newest it speaks, for the client to decide.
fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| VERSIONS.contains(v))
        .unwrap_or(VERSIONS[VERSIONS.len() - 1]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "doppelpad",
            "title": "Doppelpad",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

/// A JSON-RPC response to the request `id`.
fn answer(id: &Value, reply: Reply) -> String {
    match reply {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    }
    .to_string()
}
