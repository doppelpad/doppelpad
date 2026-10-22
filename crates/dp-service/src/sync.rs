//! Locks shared with helper threads.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Lock `m` even if a thread panicked while holding it: the data behind these locks stays
/// usable, and a panicking helper thread must not take the service loop down with it.
pub(crate) fn lock_ignore_poison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
