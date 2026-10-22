//! Asking the service thread and waiting for its answer, for the callers on other threads
//! (the GUI and CLI host, the IPC server, the HTTP API).

use std::time::Duration;

use crossbeam_channel::{Sender, bounded};
use dp_ipc::Response;

/// How long a caller waits for the service to answer.
pub(crate) const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a round trip to the service thread failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallError {
    /// The service thread is gone.
    Stopped,
    /// No answer within [`REPLY_TIMEOUT`].
    Timeout,
}

/// Send the job `job` builds around a reply channel and wait for the answer.
pub(crate) fn round_trip<J, R>(
    tx: &Sender<J>,
    job: impl FnOnce(Sender<R>) -> J,
) -> Result<R, CallError> {
    let (rtx, rrx) = bounded(1);
    tx.send(job(rtx)).map_err(|_| CallError::Stopped)?;
    rrx.recv_timeout(REPLY_TIMEOUT)
        .map_err(|_| CallError::Timeout)
}

/// The answer to an IPC or HTTP request whose round trip failed.
impl From<CallError> for Response {
    fn from(e: CallError) -> Response {
        Response::error(match e {
            CallError::Stopped => "Doppelpad is shutting down",
            CallError::Timeout => "Doppelpad did not answer",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_and_failures() {
        let (tx, rx) = crossbeam_channel::unbounded::<(u32, Sender<u32>)>();
        let server = std::thread::spawn(move || {
            let (n, reply) = rx.recv().unwrap();
            reply.send(n + 1).unwrap();
            // The second job is dropped unanswered.
            drop(rx.recv().unwrap());
        });
        assert_eq!(round_trip(&tx, |r| (1, r)), Ok(2));
        assert_eq!(round_trip(&tx, |r| (2, r)), Err(CallError::Timeout));
        server.join().unwrap();
        assert_eq!(round_trip(&tx, |r| (3, r)), Err(CallError::Stopped));
    }
}
