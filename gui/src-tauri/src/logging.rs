//! The app's log file, `dp_config::log_dir()/doppelpad-gui.log`, so bug reports can
//! include it (release builds have no console). Each start moves the previous run's log
//! to `doppelpad-gui.log.1`; a run stops writing after [`MAX_BYTES`].

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

const FILE_NAME: &str = "doppelpad-gui.log";

/// Per run, so a warning repeated every tick cannot fill the disk.
const MAX_BYTES: u64 = 16 << 20;

pub fn path() -> PathBuf {
    dp_config::log_dir().join(FILE_NAME)
}

/// Log to the file, and to stdout in debug builds. Called from the app's setup, which runs
/// after the single-instance check: a second launch must not rotate the running app's log.
pub fn init() {
    let filter = EnvFilter::try_from_env("DOPPELPAD_LOG").unwrap_or_else(|_| "info".into());
    let path = path();
    let (file, open_error) = match open(&path) {
        Ok(f) => (Some(f), None),
        Err(e) => (None, Some(e)),
    };
    let file_layer = file.map(|f| {
        fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(Capped::new(f, MAX_BYTES)))
    });
    let stdout_layer = cfg!(debug_assertions).then(|| fmt::layer().with_target(false));
    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();
    if let Some(e) = open_error {
        tracing::warn!("cannot write the log file {}: {e}", path.display());
    }
    log_panics();
}

/// Start an empty log at `path`, keeping the previous one as `<path>.1`.
fn open(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut previous = path.as_os_str().to_owned();
    previous.push(".1");
    // Missing on first start; otherwise losing the old log is no reason not to log.
    let _ = std::fs::rename(path, previous);
    File::create(path)
}

/// Record panics in the log; without a console they would vanish.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!(
            "thread '{}' {info}\n{}",
            thread.name().unwrap_or("<unnamed>"),
            std::backtrace::Backtrace::force_capture()
        );
        default(info);
    }));
}

/// Passes writes through until `limit` bytes, then notes once that the rest is dropped.
struct Capped<W> {
    inner: W,
    written: u64,
    limit: u64,
}

impl<W> Capped<W> {
    fn new(inner: W, limit: u64) -> Self {
        Capped {
            inner,
            written: 0,
            limit,
        }
    }
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written >= self.limit {
            return Ok(buf.len());
        }
        if self.written + buf.len() as u64 > self.limit {
            self.written = self.limit;
            self.inner
                .write_all(b"log size limit reached; later messages are not logged\n")?;
            return Ok(buf.len());
        }
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_keeps_the_previous_run() {
        let dir = std::env::temp_dir().join(format!("dp-gui-log-{}", std::process::id()));
        let path = dir.join("nested").join(FILE_NAME);
        let previous = path.with_extension("log.1");

        open(&path).unwrap().write_all(b"first").unwrap();
        assert!(!previous.exists());
        open(&path).unwrap().write_all(b"second").unwrap();
        open(&path).unwrap().write_all(b"third").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third");
        assert_eq!(std::fs::read_to_string(&previous).unwrap(), "second");

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn capped_drops_writes_past_the_limit() {
        let mut w = Capped::new(Vec::new(), 10);
        w.write_all(b"12345\n").unwrap();
        w.write_all(b"too long\n").unwrap();
        w.write_all(b"x\n").unwrap();
        let text = String::from_utf8(w.inner).unwrap();
        assert_eq!(
            text,
            "12345\nlog size limit reached; later messages are not logged\n"
        );
    }
}
