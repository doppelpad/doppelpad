//! Fractions of output units (pixels, wheel steps) carried over until they add up, so slow
//! motion is not lost to rounding.

/// The fraction of one output axis not sent yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Remainder(f32);

impl Remainder {
    /// Add `amount`; the whole units (towards zero) to send now. The rest stays.
    pub(crate) fn take(&mut self, amount: f32) -> i32 {
        self.0 += amount;
        let whole = self.0.trunc();
        self.0 -= whole;
        whole as i32
    }
}

/// The fractions of a mouse motion not sent yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct MotionRemainder {
    x: Remainder,
    y: Remainder,
}

impl MotionRemainder {
    /// Add `(dx, dy)`; the whole pixels to move now.
    pub(crate) fn take(&mut self, dx: f32, dy: f32) -> (i32, i32) {
        (self.x.take(dx), self.y.take(dy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_add_up() {
        let mut r = Remainder::default();
        assert_eq!(r.take(0.4), 0);
        assert_eq!(r.take(0.4), 0);
        assert_eq!(r.take(0.4), 1);
        assert_eq!(r.take(2.5), 2);
        assert!((r.0 - 0.7).abs() < 1e-5);
    }

    #[test]
    fn negative_amounts_round_towards_zero() {
        let mut r = Remainder::default();
        assert_eq!(r.take(-0.6), 0);
        assert_eq!(r.take(-0.6), -1);
        assert_eq!(r.take(0.3), 0);
        assert!((r.0 - 0.1).abs() < 1e-5);
    }

    #[test]
    fn motion_axes_carry_separately() {
        let mut m = MotionRemainder::default();
        assert_eq!(m.take(0.5, -1.5), (0, -1));
        assert_eq!(m.take(0.5, -0.5), (1, -1));
        assert_eq!(m, MotionRemainder::default());
    }
}
