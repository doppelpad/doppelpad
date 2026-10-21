//! Auto-repeat for keys that mappings hold on the virtual keyboard.
//!
//! Injected key-downs do not repeat by themselves the way a physical keyboard's do, so text
//! fields and menus would see one character per press. Like a physical keyboard, only the
//! most recently pressed key repeats, and only while it is held.

use crate::keys::Key;
use crate::profile::KeyRepeat;

/// Delay before the first repeat in native mode.
const NATIVE_DELAY_MS: u64 = 500;
/// Native interval and how far it varies (± 20 %).
const NATIVE_MS: f32 = 30.0;
const NATIVE_JITTER: f32 = 0.2;
const FAST_MS: u64 = 10;
const SLOW_MS: u64 = 50;

#[derive(Clone, Debug)]
pub struct KeyRepeater {
    mode: KeyRepeat,
    /// The key that repeats and when its next key-down is due.
    current: Option<(Key, u64)>,
    rng: u32,
}

impl KeyRepeater {
    pub fn new(mode: KeyRepeat) -> Self {
        KeyRepeater {
            mode,
            current: None,
            rng: 0x2545_F491,
        }
    }

    /// Pseudo-random native interval (xorshift; repeatable for tests).
    fn native_interval(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        let unit = (x as f32) / (u32::MAX as f32); // 0..1
        (NATIVE_MS * (1.0 + NATIVE_JITTER * (2.0 * unit - 1.0))).round() as u64
    }

    fn interval(&mut self) -> Option<u64> {
        match self.mode {
            KeyRepeat::Native => Some(self.native_interval()),
            KeyRepeat::Fast => Some(FAST_MS),
            KeyRepeat::Slow => Some(SLOW_MS),
            KeyRepeat::Every(ms) => Some(u64::from(ms.max(1))),
            KeyRepeat::Off => None,
        }
    }

    /// Key transitions sent at `now`.
    pub fn observe(&mut self, keys: &[(Key, bool)], now: u64) {
        for &(k, down) in keys {
            if down {
                let first = match self.mode {
                    KeyRepeat::Native => Some(NATIVE_DELAY_MS),
                    _ => self.interval(),
                };
                self.current = first.map(|d| (k, now + d));
            } else if self.current.is_some_and(|(c, _)| c == k) {
                self.current = None;
            }
        }
    }

    /// The key whose next key-down is due at `now`, if any.
    pub fn due(&mut self, now: u64) -> Option<Key> {
        let (k, at) = self.current?;
        if now < at {
            return None;
        }
        let next = self.interval()?;
        // After a stall, continue from now instead of catching up in a burst.
        self.current = Some((k, now + next));
        Some(k)
    }

    pub fn clear(&mut self) {
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: KeyRepeat, hold_ms: u64) -> Vec<u64> {
        let mut r = KeyRepeater::new(mode);
        r.observe(&[(Key::A, true)], 0);
        (1..=hold_ms).filter(|&t| r.due(t).is_some()).collect()
    }

    #[test]
    fn native_waits_then_repeats_with_jitter() {
        let at = run(KeyRepeat::Native, 1000);
        assert_eq!(at[0], 500, "initial delay");
        let gaps: Vec<u64> = at.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| (24..=36).contains(g)), "{gaps:?}");
        assert!(gaps.iter().any(|g| *g != gaps[0]), "jittered");
    }

    #[test]
    fn fixed_modes_and_off() {
        assert_eq!(run(KeyRepeat::Fast, 35), vec![10, 20, 30]);
        assert_eq!(run(KeyRepeat::Slow, 120), vec![50, 100]);
        assert_eq!(run(KeyRepeat::Every(40), 90), vec![40, 80]);
        assert!(run(KeyRepeat::Off, 1000).is_empty());
    }

    #[test]
    fn only_the_latest_key_repeats_while_held() {
        let mut r = KeyRepeater::new(KeyRepeat::Fast);
        r.observe(&[(Key::W, true)], 0);
        r.observe(&[(Key::A, true)], 5);
        assert_eq!(r.due(15), Some(Key::A));
        r.observe(&[(Key::W, false)], 16);
        assert_eq!(
            r.due(25),
            Some(Key::A),
            "releasing another key changes nothing"
        );
        r.observe(&[(Key::A, false)], 26);
        assert_eq!(r.due(100), None, "W does not take over");
    }
}
