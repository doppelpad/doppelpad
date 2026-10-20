//! Building blocks for controllers that are driven through raw reports.
//!
//! A [`PadDriver`] owns one controller's protocol: it initialises the device, parses its
//! input reports and paces its output (rumble, keep-alives). All I/O goes through
//! [`ReportIo`] (HID reports) and [`BulkIo`] (vendor command channels), so protocol code
//! never touches the OS, never sleeps and reads the time only through [`ReportIo::now`].
//! Tests drive drivers with scripted fakes on a virtual clock.

use std::io;
use std::time::{Duration, Instant};

use dp_core::pad::{PadState, Rgb, Rumble};

use crate::{JoyConSide, PadFamily};

/// Report transport of one HID device. Reports start with their report-ID byte.
pub trait ReportIo {
    /// Send an output report; the implementation zero-pads it to the device's output
    /// report length.
    fn write(&mut self, report: &[u8]) -> io::Result<()>;
    /// Wait up to `timeout` for the next input report. `Ok(None)` on timeout.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>>;
    /// Read a feature report (`buf[0]` = report ID); returns the bytes read.
    fn get_feature(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Command channel next to the HID interface (Switch 2 USB vendor interface).
pub trait BulkIo: Send {
    /// Send one command and return the device's reply (up to `reply_len` bytes, which may
    /// take several transfers).
    fn command(&mut self, cmd: &[u8], reply_len: usize, timeout: Duration) -> io::Result<Vec<u8>>;
}

/// A command channel that could not be opened at the start (another program held it) and
/// is handed over once a [`BulkRetry`] manages to open it. Dropping this tells the retrying
/// side to give up.
pub struct LateBulk {
    rx: crossbeam_channel::Receiver<Box<dyn BulkIo>>,
    _alive: crossbeam_channel::Sender<()>,
}

/// The side of a [`LateBulk`] that keeps trying to open the channel.
pub struct BulkRetry {
    tx: crossbeam_channel::Sender<Box<dyn BulkIo>>,
    alive: crossbeam_channel::Receiver<()>,
}

pub fn late_bulk() -> (LateBulk, BulkRetry) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let (alive_tx, alive) = crossbeam_channel::bounded(0);
    (
        LateBulk {
            rx,
            _alive: alive_tx,
        },
        BulkRetry { tx, alive },
    )
}

impl LateBulk {
    /// The channel, if it has been opened meanwhile.
    pub fn try_take(&self) -> Option<Box<dyn BulkIo>> {
        self.rx.try_recv().ok()
    }
}

impl BulkRetry {
    /// Wait `d` before the next attempt; `false` once the [`LateBulk`] is gone.
    pub fn wait(&self, d: Duration) -> bool {
        matches!(
            self.alive.recv_timeout(d),
            Err(crossbeam_channel::RecvTimeoutError::Timeout)
        )
    }

    pub fn hand_over(self, bulk: Box<dyn BulkIo>) {
        let _ = self.tx.send(bulk);
    }
}

/// What a driver learned about its device during [`PadDriver::init`].
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    pub name: String,
    pub family: PadFamily,
    pub joycon: Option<JoyConSide>,
    pub has_motion: bool,
    pub has_touchpad: bool,
    pub has_rumble: bool,
    /// Shown to the user when the device works with limitations.
    pub notice: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("the controller did not answer")]
    NoResponse,
    /// The device cannot be used; the message explains why to the user.
    #[error("{0}")]
    Blocked(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Protocol of one controller. Runs on the device's own thread.
pub trait PadDriver: Send {
    /// Bring the device into a reporting state and describe it.
    fn init(&mut self, io: &mut dyn ReportIo) -> Result<Identity, InitError>;
    /// Parse one input report (report ID first). `None` for reports without pad state.
    fn parse(&mut self, report: &[u8], now: Instant) -> Option<PadState>;
    /// Periodic output (rumble, keep-alives). Called after every report, timeout and
    /// wake-up, with the rumble the engine currently wants.
    fn service(&mut self, io: &mut dyn ReportIo, rumble: Rumble) -> io::Result<()>;
    /// Light bar colour to show from the next [`service`](Self::service) on; `None` leaves
    /// the light bar as it is. Ignored by pads without one.
    fn set_lightbar(&mut self, _color: Option<Rgb>) {}
    /// When [`service`](Self::service) must run again even if no report arrives.
    fn next_deadline(&self) -> Option<Instant>;
    /// The limitation notice if it changed since [`init`](Self::init) or the last call:
    /// `Some(None)` once the device works fully. Checked after every `service`.
    fn take_notice_change(&mut self) -> Option<Option<String>> {
        None
    }
    /// Restore the device for other programs before the handle closes.
    fn shutdown(&mut self, _io: &mut dyn ReportIo) {}
}

/// Wait until `until` while discarding input reports (used for command spacing).
pub fn pause(io: &mut dyn ReportIo, until: Instant) -> io::Result<()> {
    let mut buf = [0u8; 64];
    loop {
        let now = io.now();
        if now >= until {
            return Ok(());
        }
        io.read(&mut buf, until - now)?;
    }
}

/// Rate limiting for rumble output: never faster than `min_interval`, refresh an active
/// rumble every `refresh` (some pads stop by themselves), and send the stop several
/// times (a single lost report would leave the motors running).
#[derive(Clone, Debug)]
pub struct RumblePacer {
    min_interval: Duration,
    refresh: Option<Duration>,
    stop_repeats: u8,
    sent: Rumble,
    last_sent: Option<Instant>,
    stops_left: u8,
    want: Rumble,
}

impl RumblePacer {
    pub fn new(min_interval: Duration, refresh: Option<Duration>, stop_repeats: u8) -> Self {
        RumblePacer {
            min_interval,
            refresh,
            stop_repeats: stop_repeats.max(1),
            sent: Rumble::default(),
            last_sent: None,
            stops_left: 0,
            want: Rumble::default(),
        }
    }

    fn pending(&self, now: Instant) -> bool {
        if self.want != self.sent {
            return true;
        }
        if self.want == Rumble::default() {
            return self.stops_left > 0;
        }
        match (self.refresh, self.last_sent) {
            (Some(r), Some(t)) => now >= t + r,
            _ => false,
        }
    }

    /// The rumble to send now, if any. The caller must send it.
    pub fn poll(&mut self, now: Instant, want: Rumble) -> Option<Rumble> {
        self.want = want;
        if let Some(t) = self.last_sent
            && now < t + self.min_interval
        {
            return None;
        }
        if !self.pending(now) {
            return None;
        }
        if want == Rumble::default() {
            self.stops_left = if want != self.sent {
                self.stop_repeats - 1
            } else {
                self.stops_left.saturating_sub(1)
            };
        }
        self.sent = want;
        self.last_sent = Some(now);
        Some(want)
    }

    /// When [`poll`](Self::poll) will next have something to send.
    pub fn next_deadline(&self) -> Option<Instant> {
        let last = self.last_sent?;
        let earliest = last + self.min_interval;
        if self.want != self.sent || (self.want == Rumble::default() && self.stops_left > 0) {
            Some(earliest)
        } else if self.want != Rumble::default() {
            self.refresh.map(|r| (last + r).max(earliest))
        } else {
            None
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Scripted transports on a virtual clock.

    use std::collections::VecDeque;

    use super::*;

    type Responder = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>> + Send>;

    /// Fake HID device: every write is recorded and handed to a responder, whose replies
    /// become readable after `reply_delay`. Reads without data advance the clock by
    /// their timeout.
    pub struct ScriptIo {
        start: Instant,
        pub clock: Duration,
        pub written: Vec<(Duration, Vec<u8>)>,
        pending: VecDeque<(Duration, Vec<u8>)>,
        responder: Responder,
        pub reply_delay: Duration,
        pub features: Vec<Vec<u8>>,
    }

    impl ScriptIo {
        pub fn new(responder: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) -> Self {
            ScriptIo {
                start: Instant::now(),
                clock: Duration::ZERO,
                written: Vec::new(),
                pending: VecDeque::new(),
                responder: Box::new(responder),
                reply_delay: Duration::from_millis(5),
                features: Vec::new(),
            }
        }

        /// A device that never answers.
        pub fn silent() -> Self {
            Self::new(|_| Vec::new())
        }

        /// Make an unsolicited input report readable at virtual time `at`.
        pub fn push_input(&mut self, at: Duration, report: Vec<u8>) {
            let i = self.pending.partition_point(|(t, _)| *t <= at);
            self.pending.insert(i, (at, report));
        }

        pub fn advance(&mut self, d: Duration) {
            self.clock += d;
        }

        pub fn writes(&self) -> Vec<&[u8]> {
            self.written.iter().map(|(_, w)| w.as_slice()).collect()
        }
    }

    impl ReportIo for ScriptIo {
        fn write(&mut self, report: &[u8]) -> io::Result<()> {
            self.written.push((self.clock, report.to_vec()));
            let at = self.clock + self.reply_delay;
            for reply in (self.responder)(report) {
                self.push_input(at, reply);
            }
            Ok(())
        }

        fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
            match self.pending.front() {
                Some((t, _)) if *t <= self.clock + timeout => {
                    let (t, r) = self.pending.pop_front().unwrap();
                    self.clock = self.clock.max(t);
                    let n = r.len().min(buf.len());
                    buf[..n].copy_from_slice(&r[..n]);
                    Ok(Some(n))
                }
                _ => {
                    self.clock += timeout;
                    Ok(None)
                }
            }
        }

        fn get_feature(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.features.push(buf.to_vec());
            Ok(buf.len())
        }

        fn now(&self) -> Instant {
            self.start + self.clock
        }
    }

    type BulkResponder = Box<dyn FnMut(&[u8]) -> io::Result<Vec<u8>> + Send>;

    /// Fake command channel.
    pub struct ScriptBulk {
        pub sent: std::sync::Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
        responder: BulkResponder,
    }

    impl ScriptBulk {
        pub fn new(responder: impl FnMut(&[u8]) -> io::Result<Vec<u8>> + Send + 'static) -> Self {
            ScriptBulk {
                sent: Default::default(),
                responder: Box::new(responder),
            }
        }
    }

    impl BulkIo for ScriptBulk {
        fn command(
            &mut self,
            cmd: &[u8],
            reply_len: usize,
            _timeout: Duration,
        ) -> io::Result<Vec<u8>> {
            self.sent.lock().unwrap().push(cmd.to_vec());
            let mut reply = (self.responder)(cmd)?;
            reply.truncate(reply_len);
            Ok(reply)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::ScriptIo;
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn r(low: f32, high: f32) -> Rumble {
        Rumble { low, high }
    }

    #[test]
    fn idle_pacer_sends_nothing() {
        let t0 = Instant::now();
        let mut p = RumblePacer::new(30 * MS, Some(50 * MS), 3);
        for i in 0..10 {
            assert_eq!(p.poll(t0 + i * 10 * MS, Rumble::default()), None);
        }
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn pacer_rate_limits_and_refreshes() {
        let t0 = Instant::now();
        let mut p = RumblePacer::new(30 * MS, Some(50 * MS), 3);
        assert_eq!(p.poll(t0, r(1.0, 0.0)), Some(r(1.0, 0.0)));
        // A change 10 ms later must wait for the 30 ms spacing.
        assert_eq!(p.poll(t0 + 10 * MS, r(0.5, 0.0)), None);
        assert_eq!(p.next_deadline(), Some(t0 + 30 * MS));
        assert_eq!(p.poll(t0 + 30 * MS, r(0.5, 0.0)), Some(r(0.5, 0.0)));
        // Unchanged: refreshed after 50 ms.
        assert_eq!(p.poll(t0 + 60 * MS, r(0.5, 0.0)), None);
        assert_eq!(p.next_deadline(), Some(t0 + 80 * MS));
        assert_eq!(p.poll(t0 + 80 * MS, r(0.5, 0.0)), Some(r(0.5, 0.0)));
    }

    #[test]
    fn pacer_repeats_stop() {
        let t0 = Instant::now();
        let mut p = RumblePacer::new(30 * MS, None, 3);
        p.poll(t0, r(1.0, 1.0));
        let zero = Rumble::default();
        let mut sent = 0;
        for i in 1..20 {
            if p.poll(t0 + i * 30 * MS, zero).is_some() {
                sent += 1;
            }
        }
        assert_eq!(sent, 3);
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn pacer_without_refresh_holds_active_rumble() {
        let t0 = Instant::now();
        let mut p = RumblePacer::new(10 * MS, None, 1);
        assert!(p.poll(t0, r(0.3, 0.3)).is_some());
        assert_eq!(p.poll(t0 + 500 * MS, r(0.3, 0.3)), None);
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn late_bulk_tells_the_retrying_side_when_it_is_gone() {
        let (late, retry) = late_bulk();
        assert!(retry.wait(MS), "still wanted");
        drop(late);
        assert!(
            !retry.wait(Duration::from_secs(5)),
            "returns at once, without waiting"
        );
    }

    #[test]
    fn pause_discards_reports_until_deadline() {
        let mut io = ScriptIo::silent();
        io.push_input(5 * MS, vec![0x30]);
        let until = io.now() + 20 * MS;
        pause(&mut io, until).unwrap();
        assert!(io.now() >= until);
    }
}
