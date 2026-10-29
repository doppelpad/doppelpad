//! Runs a [`Service`] on its own thread at about 1 kHz, optionally serves the IPC
//! protocol, and publishes a [`LiveState`] snapshot for user interfaces.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{Result, anyhow};
use crossbeam_channel::{Sender, bounded, unbounded};
use dp_ipc::{Request, Response};
use dp_platform::Platform;

use crate::client::{CallError, round_trip};
use crate::live::{LIVE_INTERVAL, LiveState, SharedLive};
use dp_platform::timer::TimerResolution;

use crate::runtime::Runtime;
use crate::service::{Service, ServiceCmd, ServiceOptions, ServiceReply};
use crate::sync::lock_ignore_poison;
use crate::timing::TICK;

#[derive(Clone, Debug)]
pub struct HostOptions {
    pub service: ServiceOptions,
    pub ipc: Ipc,
}

/// Whether the service answers `doppelpad` CLI requests over the local socket.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ipc {
    Off,
    /// Every request, `shutdown` included (the daemon).
    On,
    /// Every request but `shutdown`: the app hosts the service until it quits.
    NoShutdown,
}

/// The answer to `doppelpad shutdown` when the app hosts the service.
const SHUTDOWN_REFUSED: &str =
    "the Doppelpad app runs this service; quit the app from its tray icon to stop it";

/// What other threads ask the service thread.
enum Job {
    Cmd(ServiceCmd, Sender<ServiceReply>),
    Ipc(Request, Sender<Response>),
}

/// Handle to a running service thread. It can be shared between threads as it is.
pub struct ServiceHandle {
    tx: Sender<Job>,
    live: SharedLive,
    thread: Mutex<Option<JoinHandle<()>>>,
    /// The IPC server: it serves until the process ends (`dp_ipc::serve` does not return
    /// while it works), so it is never joined; after a shutdown it answers that the service
    /// has stopped.
    _ipc: Option<JoinHandle<()>>,
}

impl ServiceHandle {
    /// Send a command and wait for the answer.
    pub fn call(&self, cmd: ServiceCmd) -> ServiceReply {
        match round_trip(&self.tx, |rtx| Job::Cmd(cmd, rtx)) {
            Ok(reply) => reply,
            Err(CallError::Stopped) => {
                ServiceReply::Err("the remapping service has stopped".into())
            }
            Err(CallError::Timeout) => {
                ServiceReply::Err("the remapping service did not answer".into())
            }
        }
    }

    /// Latest live snapshot.
    pub fn live(&self) -> Arc<LiveState> {
        self.live.get()
    }

    /// The latest live snapshot and its generation, if it changed since generation `seen`.
    pub fn live_since(&self, seen: u64) -> Option<(u64, Arc<LiveState>)> {
        self.live.since(seen)
    }

    /// The shared live snapshot (for a thread that watches it).
    pub fn live_shared(&self) -> SharedLive {
        self.live.clone()
    }

    pub fn is_running(&self) -> bool {
        lock_ignore_poison(&self.thread)
            .as_ref()
            .is_some_and(|t| !t.is_finished())
    }

    /// Stop the service (releasing outputs, unhiding devices) and wait for its thread.
    pub fn shutdown(&self) {
        if self.is_running() {
            let _ = self.call(ServiceCmd::Shutdown);
        }
        let thread = lock_ignore_poison(&self.thread).take();
        if let Some(t) = thread {
            let _ = t.join();
        }
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start the service on the native platform.
pub fn spawn(opts: HostOptions) -> Result<ServiceHandle> {
    spawn_with(opts, Platform::native)
}

/// Start the service on explicit backends (tests use `NullPlatform::platform`).
pub fn spawn_with(
    opts: HostOptions,
    make_platform: impl FnOnce() -> Platform + Send + 'static,
) -> Result<ServiceHandle> {
    let (tx, rx) = unbounded::<Job>();
    let (ready_tx, ready_rx) = bounded::<Result<SharedLive, String>>(1);

    let service_opts = opts.service.clone();
    let thread = std::thread::Builder::new()
        .name("dp-service".into())
        .spawn(move || {
            let _timer = TimerResolution::high();
            let mut svc = match Service::new(Runtime::with_platform(make_platform()), service_opts)
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("{e:#}")));
                    return;
                }
            };
            svc.publish_live();
            let _ = ready_tx.send(Ok(svc.shared_live()));

            let mut last_live = Instant::now();
            let mut next_tick = Instant::now();
            'run: loop {
                for job in rx.try_iter() {
                    // Publish the effect before answering, so a caller reading `live()`
                    // right after `call()` sees it.
                    let shutdown = match job {
                        Job::Cmd(cmd, reply) => {
                            let shutdown = cmd == ServiceCmd::Shutdown;
                            let answer = svc.handle(cmd);
                            svc.publish_live();
                            let _ = reply.send(answer);
                            shutdown
                        }
                        Job::Ipc(req, reply) => {
                            let shutdown = req == Request::Shutdown;
                            let answer = svc.handle_ipc(req);
                            svc.publish_live();
                            let _ = reply.send(answer);
                            shutdown
                        }
                    };
                    if shutdown {
                        break 'run;
                    }
                }
                svc.tick();
                if last_live.elapsed() >= LIVE_INTERVAL {
                    svc.publish_live();
                    last_live = Instant::now();
                }
                // The next tick is due a tick after this one was, however long the work took,
                // so the rate stays at TICK (after falling behind, it starts over from now).
                next_tick += TICK;
                let now = Instant::now();
                match next_tick.checked_duration_since(now) {
                    Some(wait) => std::thread::sleep(wait),
                    None => next_tick = now,
                }
            }
            svc.shutdown();
            svc.publish_live();
            tracing::info!("remapping service stopped");
        })?;

    let live = match ready_rx.recv() {
        Ok(Ok(live)) => live,
        Ok(Err(e)) => {
            let _ = thread.join();
            return Err(anyhow!(e));
        }
        Err(_) => {
            return Err(anyhow!(
                "the remapping service thread exited during start-up"
            ));
        }
    };

    let ipc = if opts.ipc != Ipc::Off {
        let ipc_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("dp-ipc".into())
            .spawn(move || {
                let result = dp_ipc::serve(|req| ipc_answer(&ipc_tx, req, opts.ipc));
                if let Err(e) = result {
                    tracing::error!("IPC server stopped: {e}");
                }
            });
        match spawned {
            Ok(t) => Some(t),
            Err(e) => {
                // Stop the service again rather than leave it running with nobody to stop it:
                // it holds the virtual pads and the hidden controllers.
                let (reply, _answer) = bounded(1);
                let _ = tx.send(Job::Cmd(ServiceCmd::Shutdown, reply));
                let _ = thread.join();
                return Err(e.into());
            }
        }
    } else {
        None
    };

    Ok(ServiceHandle {
        tx,
        live,
        thread: Mutex::new(Some(thread)),
        _ipc: ipc,
    })
}

/// Answer one IPC request through the service thread.
fn ipc_answer(tx: &Sender<Job>, req: Request, ipc: Ipc) -> Response {
    if ipc == Ipc::NoShutdown && req == Request::Shutdown {
        return Response::error(SHUTDOWN_REFUSED);
    }
    match round_trip(tx, |rtx| Job::Ipc(req, rtx)) {
        Ok(reply) => reply,
        Err(e) => e.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dp_platform::NullPlatform;

    use super::*;

    #[test]
    fn spawn_call_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("p.toml");
        std::fs::write(&profile, "name = \"P\"\n[output]\nvirtual_pad = \"none\"\n").unwrap();
        let opts = HostOptions {
            service: ServiceOptions::new(dir.path().join("profiles")),
            ipc: Ipc::Off,
        };
        let h = spawn_with(opts, NullPlatform::platform).unwrap();
        assert!(h.is_running());
        let r = h.call(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 1,
            path: profile,
        });
        assert!(r.into_result().is_ok());
        // No waiting: the call publishes before it answers.
        let live = h.live();
        assert_eq!(live.profile.as_deref(), Some("P"));
        assert_eq!(live.active_slot, 1);
        h.call(ServiceCmd::SetRemap(false));
        assert!(
            !h.live().remap_on,
            "the snapshot is current when the call returns"
        );
        h.shutdown();
        assert!(!h.is_running());
        assert_eq!(h.live().profile, None, "stopped on shutdown");
    }

    #[test]
    fn only_the_daemon_stops_on_an_ipc_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let opts = HostOptions {
            service: ServiceOptions::new(dir.path().join("profiles")),
            ipc: Ipc::Off,
        };
        let h = spawn_with(opts, NullPlatform::platform).unwrap();
        let refused = ipc_answer(&h.tx, Request::Shutdown, Ipc::NoShutdown);
        assert_eq!(refused, Response::error(SHUTDOWN_REFUSED));
        // A later call still gets an answer: the service thread went on.
        assert!(matches!(
            h.call(ServiceCmd::GetStatus),
            ServiceReply::Status(_)
        ));
        assert!(h.is_running(), "the app's service keeps running");
        let stopped = ipc_answer(&h.tx, Request::Shutdown, Ipc::On);
        assert!(!matches!(stopped, Response::Error { .. }), "{stopped:?}");
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while h.is_running() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!h.is_running(), "the daemon's service stops");
    }

    #[test]
    fn start_up_errors_are_reported() {
        let mut service = ServiceOptions::new("profiles".into());
        service.initial_profile = Some("does-not-exist.toml".into());
        let r = spawn_with(
            HostOptions {
                service,
                ipc: Ipc::Off,
            },
            NullPlatform::platform,
        );
        assert!(r.is_err());
    }
}
