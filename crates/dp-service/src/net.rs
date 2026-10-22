//! What the network servers (HTTP API, DSU motion server) share.

use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

/// The address to listen on: every interface when the network is allowed, else this
/// computer only.
pub(crate) fn bind_ip(lan: bool) -> Ipv4Addr {
    if lan {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    }
}

/// A server thread that watches a stop flag; stopped and joined on drop.
pub(crate) struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Run `f` with the stop flag on a thread named `name`.
    pub(crate) fn spawn(
        name: &str,
        f: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
    ) -> io::Result<Worker> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || f(flag))?;
        Ok(Worker {
            stop,
            thread: Some(thread),
        })
    }
}

impl Worker {
    /// Ask the thread to stop (dropping the worker also waits for it).
    pub(crate) fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A server the settings switch on and off, and why it does not run.
pub(crate) struct Listener<S> {
    pub(crate) running: Option<S>,
    pub(crate) error: Option<String>,
}

impl<S> Default for Listener<S> {
    fn default() -> Self {
        Listener {
            running: None,
            error: None,
        }
    }
}

impl<S> Listener<S> {
    /// Stop the server (freeing its port), then start it again if `enabled`. `what` names
    /// the server in the error.
    pub(crate) fn restart(
        &mut self,
        enabled: bool,
        what: &str,
        port: u16,
        start: impl FnOnce() -> io::Result<S>,
    ) {
        self.running = None;
        self.error = None;
        if !enabled {
            return;
        }
        match start() {
            Ok(s) => self.running = Some(s),
            Err(e) => {
                let e = format!("{what} cannot use port {port}: {e}");
                tracing::warn!("{e}");
                self.error = Some(e);
            }
        }
    }
}
