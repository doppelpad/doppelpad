//! The device scanner thread the backends share: one pass at once, then one every period
//! and whenever asked, until the scanner is stopped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender};

/// How often the backends look for new devices on their own.
pub const RESCAN: Duration = Duration::from_secs(2);

/// One scan pass.
pub struct Pass<'a> {
    /// Asked for with [`Rescanner::wake`]: retry what failed before.
    pub requested: bool,
    stop: &'a AtomicBool,
}

impl Pass<'_> {
    /// The scanner is stopping; a long pass ends early.
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// A scanner thread, stopped and joined on drop.
pub struct Rescanner {
    stop: Arc<AtomicBool>,
    wake: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Rescanner {
    /// Run `pass` on a thread named `name`: at once, then every `period` and whenever woken.
    /// A thread that cannot start is logged; the backend then runs without discovery.
    pub fn spawn(
        name: &str,
        period: Duration,
        mut pass: impl FnMut(&Pass) + Send + 'static,
    ) -> Rescanner {
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, woken) = crossbeam_channel::bounded(1);
        let flag = stop.clone();
        let spawned = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut requested = false;
                while !flag.load(Ordering::Relaxed) {
                    pass(&Pass {
                        requested,
                        stop: &flag,
                    });
                    requested = match woken.recv_timeout(period) {
                        Ok(()) => true,
                        Err(RecvTimeoutError::Timeout) => false,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                }
            });
        let thread = spawned
            .inspect_err(|e| tracing::warn!("cannot start {name}, no devices are looked for: {e}"))
            .ok();
        Rescanner { stop, wake, thread }
    }

    /// Scan now and retry what failed before.
    pub fn wake(&self) {
        let _ = self.wake.try_send(());
    }

    /// Stop the thread and wait for the pass it is in.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Rescanner {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    const LONG: Duration = Duration::from_secs(3600);
    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn passes_at_once_then_when_woken() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let scanner = Rescanner::spawn("test-scan", LONG, move |p| {
            let _ = tx.send(p.requested);
        });
        assert_eq!(rx.recv_timeout(WAIT), Ok(false));
        scanner.wake();
        assert_eq!(rx.recv_timeout(WAIT), Ok(true));
    }

    #[test]
    fn stop_waits_for_the_running_pass() {
        let started = Arc::new(AtomicUsize::new(0));
        let ended = Arc::new(AtomicUsize::new(0));
        let (s, e) = (started.clone(), ended.clone());
        let mut scanner = Rescanner::spawn("test-scan", Duration::ZERO, move |_| {
            s.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            e.fetch_add(1, Ordering::SeqCst);
        });
        while started.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        scanner.stop();
        let n = ended.load(Ordering::SeqCst);
        assert_eq!(n, started.load(Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(started.load(Ordering::SeqCst), n, "no pass after stop");
    }

    #[test]
    fn a_long_pass_sees_the_stop() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let scanner = Rescanner::spawn("test-scan", LONG, move |p| {
            let _ = tx.send(());
            while !p.stopping() {
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        assert_eq!(rx.recv_timeout(WAIT), Ok(()));
        drop(scanner);
    }
}
