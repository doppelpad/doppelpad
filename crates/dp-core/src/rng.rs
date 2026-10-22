//! A small deterministic random generator (xorshift64*), so runs and tests repeat.

#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Rng(0x9e37_79b9_7f4a_7c15)
    }
}

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `0..=n`.
    pub(crate) fn upto(&mut self, n: u64) -> u64 {
        self.next() % (n + 1)
    }
}
