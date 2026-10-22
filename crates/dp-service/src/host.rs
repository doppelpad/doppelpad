//! Runs a [`Service`] on its own thread at about 1 kHz, optionally serves the IPC
//! protocol, and publishes a [`LiveState`] snapshot for user interfaces.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use crossbeam_channel::{Sender, bounded, unbounded};
use dp_ipc::{Request, Response};
use dp_platform::Platform;

use crate::client::{CallError, round_trip};
use crate::live::LiveState;
use crate::runtime::{Runtime, TimerResolution};
use crate::service::{Service, ServiceCmd, ServiceOptions, ServiceReply};
use crate::timing::TICK;

/// How often the live snapshot is refreshed.
const LIVE_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, Debug)]
pub struct HostOptions {
    pub service: ServiceOptions,
    /// Also answer `doppelpad` CLI requests over the local socket.
    pub ipc: bool,
}

/// What other threads ask the service thread.
enum Job {
    Cmd(ServiceCmd, Sender<ServiceReply>),
    Ipc(Request, Sender<Response>),
}

/// Handle to a running service thread.
pub struct ServiceHandle {
    tx: Sender<Job>,
    live: Arc<Mutex<LiveState>>,
    thread: Option<JoinHandle<()>>,
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
    pub fn live(&self) -> LiveState {
        self.live.lock().unwrap().clone()
    }

    /// Shared live snapshot (for a streaming thread).
    pub fn live_shared(&self) -> Arc<Mutex<LiveState>> {
        self.live.clone()
    }

    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Stop the service (releasing outputs, unhiding devices) and wait for its thread.
    pub fn shutdown(&mut self) {
        if self.is_running() {
            let _ = self.call(ServiceCmd::Shutdown);
        }
        if let Some(t) = self.thread.take() {
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
    let live = Arc::new(Mutex::new(LiveState::default()));
    let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);

    let live_out = live.clone();
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
            *live_out.lock().unwrap() = svc.live();
            let _ = ready_tx.send(Ok(()));

            let mut last_live = Instant::now();
            'run: loop {
                for job in rx.try_iter() {
                    // Publish the effect before answering, so a caller reading `live()`
                    // right after `call()` sees it.
                    let shutdown = match job {
                        Job::Cmd(cmd, reply) => {
                            let shutdown = cmd == ServiceCmd::Shutdown;
                            let answer = svc.handle(cmd);
                            *live_out.lock().unwrap() = svc.live();
                            let _ = reply.send(answer);
                            shutdown
                        }
                        Job::Ipc(req, reply) => {
                            let shutdown = req == Request::Shutdown;
                            let answer = svc.handle_ipc(req);
                            *live_out.lock().unwrap() = svc.live();
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
                    *live_out.lock().unwrap() = svc.live();
                    last_live = Instant::now();
                }
                std::thread::sleep(TICK);
            }
            svc.shutdown();
            *live_out.lock().unwrap() = svc.live();
            tracing::info!("remapping service stopped");
        })?;

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = thread.join();
            return Err(anyhow!(e));
        }
        Err(_) => {
            return Err(anyhow!(
                "the remapping service thread exited during start-up"
            ));
        }
    }

    if opts.ipc {
        let ipc_tx = tx.clone();
        std::thread::Builder::new()
            .name("dp-ipc".into())
            .spawn(move || {
                let result =
                    dp_ipc::serve(|req| match round_trip(&ipc_tx, |rtx| Job::Ipc(req, rtx)) {
                        Ok(reply) => reply,
                        Err(e) => e.into(),
                    });
                if let Err(e) = result {
                    tracing::error!("IPC server stopped: {e}");
                }
            })?;
    }

    Ok(ServiceHandle {
        tx,
        live,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use dp_platform::NullPlatform;

    use super::*;

    #[test]
    fn spawn_call_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("p.toml");
        std::fs::write(&profile, "name = \"P\"\n[output]\nvirtual_pad = \"none\"\n").unwrap();
        let opts = HostOptions {
            service: ServiceOptions::new(dir.path().join("profiles")),
            ipc: false,
        };
        let mut h = spawn_with(opts, NullPlatform::platform).unwrap();
        assert!(h.is_running());
        let r = h.call(ServiceCmd::ApplyToSlot {
            device: None,
            slot: 1,
            path: profile,
        });
        assert!(r.into_result().is_ok());
        std::thread::sleep(Duration::from_millis(50));
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
    fn start_up_errors_are_reported() {
        let mut service = ServiceOptions::new("profiles".into());
        service.initial_profile = Some("does-not-exist.toml".into());
        let r = spawn_with(
            HostOptions {
                service,
                ipc: false,
            },
            NullPlatform::platform,
        );
        assert!(r.is_err());
    }
}
