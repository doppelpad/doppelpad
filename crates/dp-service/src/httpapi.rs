//! A local HTTP API for stream decks and scripts: the same requests as the `doppelpad` CLI,
//! the live state, and a Server-Sent Events stream of changes.
//!
//! Off by default. It listens on 127.0.0.1 unless the network is allowed, and then every
//! request needs the token. Browsers are kept out: requests that carry an `Origin` (or a
//! `Host` other than this computer) need the token too, and commands must be JSON.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use dp_ipc::{Request, Response};
use serde::{Deserialize, Serialize};

use crate::client::round_trip;
use crate::live::LiveState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpSettings {
    pub enabled: bool,
    /// Listen on all network interfaces; every request then needs the token.
    pub lan: bool,
    pub port: u16,
    /// Sent as `Authorization: Bearer <token>` or `?token=`.
    pub token: String,
}

impl Default for HttpSettings {
    fn default() -> Self {
        HttpSettings {
            enabled: false,
            lan: false,
            port: 26761,
            token: String::new(),
        }
    }
}

/// A new random token (128 bits from the OS-seeded hasher).
pub fn new_token() -> String {
    let part = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

/// What the server asks the service for; answered on the service thread.
pub enum Job {
    Ipc(Request, Sender<Response>),
    Live(Sender<LiveState>),
}

pub struct HttpApi {
    pub addr: SocketAddr,
    jobs: Receiver<Job>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

const MAX_HEAD: usize = 16 << 10;
const MAX_BODY: usize = 64 << 10;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// How often the event stream looks for changes.
const EVENT_EVERY: Duration = Duration::from_millis(200);
/// Comment lines keep idle event streams (and their proxies) open.
const HEARTBEAT_EVERY: u32 = 75;

impl HttpApi {
    pub fn start(settings: &HttpSettings) -> io::Result<HttpApi> {
        let ip = if settings.lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let listener = TcpListener::bind((ip, settings.port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, jobs) = bounded::<Job>(64);
        let stop = Arc::new(AtomicBool::new(false));
        let access = Access {
            lan: settings.lan,
            token: settings.token.clone(),
            port: addr.port(),
        };
        let s = stop.clone();
        let thread = std::thread::Builder::new()
            .name("dp-http".into())
            .spawn(move || accept_loop(listener, access, tx, s))?;
        tracing::info!(%addr, "local HTTP API listening");
        Ok(HttpApi {
            addr,
            jobs,
            stop,
            thread: Some(thread),
        })
    }

    /// Requests waiting for the service.
    pub fn jobs(&self) -> &Receiver<Job> {
        &self.jobs
    }
}

impl Drop for HttpApi {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[derive(Clone)]
struct Access {
    lan: bool,
    token: String,
    port: u16,
}

fn accept_loop(listener: TcpListener, access: Access, tx: Sender<Job>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let (access, tx, stop) = (access.clone(), tx.clone(), stop.clone());
                let spawned = std::thread::Builder::new()
                    .name("dp-http-conn".into())
                    .spawn(move || {
                        if let Err(e) = serve(stream, &access, &tx, &stop) {
                            tracing::debug!("HTTP connection: {e}");
                        }
                    });
                if let Err(e) = spawned {
                    tracing::warn!("cannot serve an HTTP request: {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                tracing::warn!("HTTP API: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// One parsed request.
#[derive(Debug, PartialEq)]
struct HttpRequest {
    method: String,
    path: String,
    query: Option<String>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn token(&self) -> Option<&str> {
        let bearer = self
            .header("authorization")
            .and_then(|v| v.strip_prefix("Bearer "));
        let query = self
            .query
            .as_deref()
            .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")));
        bearer.or(query)
    }
}

fn read_request(stream: &mut impl Read) -> io::Result<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = reader
            .by_ref()
            .take(MAX_HEAD as u64)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        head.extend_from_slice(&line);
        if head.len() > MAX_HEAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "header too large",
            ));
        }
    }
    let head = String::from_utf8_lossy(&head);
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_owned(), Some(q.to_owned())),
        None => (target.to_owned(), None),
    };
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let len = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

/// Why a request is refused, if it is.
fn refused(req: &HttpRequest, access: &Access) -> Option<(u16, &'static str)> {
    let with_token = !access.token.is_empty() && req.token() == Some(access.token.as_str());
    if with_token {
        return None;
    }
    if access.lan {
        return Some((401, "this API needs its token"));
    }
    // A web page (or a DNS rebinding trick) talking to this computer.
    let local_host = req.header("host").is_some_and(|h| {
        let port = access.port.to_string();
        [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
            .iter()
            .any(|ok| h.eq_ignore_ascii_case(ok))
    });
    if !local_host || req.header("origin").is_some() {
        return Some((403, "requests from web pages need the token"));
    }
    None
}

fn respond(stream: &mut TcpStream, code: u16, body: &str) -> io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        415 => "Unsupported Media Type",
        _ => "Service Unavailable",
    };
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn error_json(message: &str) -> String {
    serde_json::json!({ "type": "error", "message": message }).to_string()
}

fn ask_ipc(tx: &Sender<Job>, req: Request) -> Response {
    round_trip(tx, |rtx| Job::Ipc(req, rtx)).unwrap_or_else(Response::from)
}

fn ask_live(tx: &Sender<Job>) -> Option<LiveState> {
    round_trip(tx, Job::Live).ok()
}

/// The parts of the live state the event stream reports. Slots count 1–4, as in the status.
fn summary(l: &LiveState) -> serde_json::Value {
    serde_json::json!({
        "remap_on": l.remap_on,
        "suspended": l.suspended,
        "active_slot": l.active_slot + 1,
        "profile": l.profile,
        "layer": l.layer,
        "layer_name": l.layer_name,
        "devices": l.devices.iter().map(|d| serde_json::json!({
            "id": d.id, "name": d.name, "battery": d.battery,
        })).collect::<Vec<_>>(),
        "warnings": l.warnings,
    })
}

fn route(req: &HttpRequest, tx: &Sender<Job>) -> (u16, String) {
    let json = |r: &Response| serde_json::to_string(r).unwrap_or_default();
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/status") => (200, json(&ask_ipc(tx, Request::Status))),
        ("GET", "/api/devices") => (200, json(&ask_ipc(tx, Request::Devices))),
        ("GET", "/api/live") => match ask_live(tx) {
            Some(l) => (200, serde_json::to_string(&l).unwrap_or_default()),
            None => (503, error_json("Doppelpad did not answer")),
        },
        ("POST", "/api") => {
            let json_body = req
                .header("content-type")
                .is_some_and(|c| c.starts_with("application/json"));
            if !json_body {
                return (415, error_json("send the request as application/json"));
            }
            match serde_json::from_slice::<Request>(&req.body) {
                Ok(Request::Shutdown) => (403, error_json("quit Doppelpad from its tray")),
                Ok(r) => {
                    let answer = ask_ipc(tx, r);
                    let code = if matches!(answer, Response::Error { .. }) {
                        400
                    } else {
                        200
                    };
                    (code, json(&answer))
                }
                Err(e) => (400, error_json(&format!("not a request: {e}"))),
            }
        }
        (_, "/api" | "/api/status" | "/api/devices" | "/api/live" | "/api/events") => {
            (405, error_json("wrong method"))
        }
        _ => (404, error_json("unknown path; see docs/API.md")),
    }
}

/// Stream `live` events while the client listens.
fn events(stream: &mut TcpStream, tx: &Sender<Job>, stop: &AtomicBool) -> io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n",
    )?;
    let mut last = serde_json::Value::Null;
    let mut quiet = 0;
    while !stop.load(Ordering::Relaxed) {
        let Some(live) = ask_live(tx) else {
            return Ok(());
        };
        let now = summary(&live);
        if now != last {
            write!(stream, "event: live\ndata: {now}\n\n")?;
            last = now;
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= HEARTBEAT_EVERY {
                stream.write_all(b": still here\n\n")?;
                quiet = 0;
            }
        }
        stream.flush()?;
        std::thread::sleep(EVENT_EVERY);
    }
    Ok(())
}

fn serve(
    mut stream: TcpStream,
    access: &Access,
    tx: &Sender<Job>,
    stop: &AtomicBool,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let req = match read_request(&mut stream) {
        Ok(r) => r,
        Err(e) => return respond(&mut stream, 400, &error_json(&e.to_string())),
    };
    if let Some((code, why)) = refused(&req, access) {
        return respond(&mut stream, code, &error_json(why));
    }
    if req.method == "GET" && req.path == "/api/events" {
        return events(&mut stream, tx, stop);
    }
    let (code, body) = route(&req, tx);
    respond(&mut stream, code, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> HttpRequest {
        read_request(&mut raw.as_bytes()).unwrap()
    }

    fn access(lan: bool) -> Access {
        Access {
            lan,
            token: "secret".into(),
            port: 26761,
        }
    }

    #[test]
    fn parses_requests() {
        let r = parse(
            "POST /api?token=secret HTTP/1.1\r\nHost: 127.0.0.1:26761\r\nContent-Length: 4\r\n\r\nbody",
        );
        assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/api"));
        assert_eq!(r.body, b"body");
        assert_eq!(r.token(), Some("secret"));
        assert_eq!(r.header("HOST"), Some("127.0.0.1:26761"));
    }

    #[test]
    fn keeps_web_pages_and_strangers_out() {
        let local = parse("GET /api/status HTTP/1.1\r\nHost: localhost:26761\r\n\r\n");
        assert_eq!(refused(&local, &access(false)), None);
        let page = parse(
            "GET /api/status HTTP/1.1\r\nHost: localhost:26761\r\nOrigin: https://evil.example\r\n\r\n",
        );
        assert_eq!(refused(&page, &access(false)).map(|r| r.0), Some(403));
        let rebound = parse("GET /api/status HTTP/1.1\r\nHost: evil.example:26761\r\n\r\n");
        assert_eq!(refused(&rebound, &access(false)).map(|r| r.0), Some(403));
        assert_eq!(
            refused(&local, &access(true)).map(|r| r.0),
            Some(401),
            "LAN needs the token"
        );
        let with_token = parse(
            "GET /api/status HTTP/1.1\r\nHost: 192.168.1.5:26761\r\nAuthorization: Bearer secret\r\n\r\n",
        );
        assert_eq!(refused(&with_token, &access(true)), None);
    }

    #[test]
    fn events_number_slots_like_the_status() {
        let live = LiveState {
            active_slot: 1,
            ..LiveState::default()
        };
        assert_eq!(
            summary(&live)["active_slot"],
            2,
            "slots are 1–4 on the wire"
        );
    }

    #[test]
    fn tokens_differ() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn serves_requests_through_the_queue() {
        let api = HttpApi::start(&HttpSettings {
            enabled: true,
            lan: false,
            port: 0,
            token: String::new(),
        })
        .unwrap();
        let addr = api.addr;
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            let body = r#"{"cmd":"remap","on":false}"#;
            write!(
                s,
                "POST /api HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                addr.port(),
                body.len()
            )
            .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        });
        match api.jobs().recv_timeout(Duration::from_secs(5)).unwrap() {
            Job::Ipc(Request::Remap { on: false }, reply) => {
                reply.send(Response::ok("remapping off")).unwrap();
            }
            _ => panic!("unexpected job"),
        }
        let out = client.join().unwrap();
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(
            out.ends_with(r#"{"type":"ok","message":"remapping off"}"#),
            "{out}"
        );
    }
}
