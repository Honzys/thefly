//! The MSVC C runtime `rand` (`fly-and-world.md` §1), injected into the sim as one stream.
//!
//! The original reseeded from `time()` in several constructors; the port seeds once, so a seed
//! plus the inputs replays a session exactly. The call order is the tick order.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsvcRand {
    state: u32,
}

impl MsvcRand {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Current LCG state, for logging and replays.
    pub fn state(&self) -> u32 {
        self.state
    }

    /// `rand()`: 0..=32767.
    pub fn rand(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7fff) as i32
    }

    /// `R(n)` = `rand() % n` with C remainder semantics. `n` must not be 0; callers guard the
    /// cases where the original divided by zero (see the UB constants in the plan).
    pub fn r(&mut self, n: i32) -> i32 {
        self.rand() % n
    }

    /// `U` = `rand() / 32767.0`, in [0, 1].
    pub fn u(&mut self) -> f64 {
        f64::from(self.rand()) / 32767.0
    }

    /// The sign roll: -1 when a fresh `rand()` is at most 16383, else +1.
    pub fn sign(&mut self) -> i32 {
        if self.rand() <= 16383 { -1 } else { 1 }
    }

    /// "rand odd": `rand() % 2 == 1`.
    pub fn odd(&mut self) -> bool {
        self.rand() % 2 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_msvc_rand_for_seed_1() {
        let mut r = MsvcRand::new(1);
        assert_eq!([r.rand(), r.rand(), r.rand()], [41, 18467, 6334]);
    }

    #[test]
    fn helpers_draw_one_value_each() {
        let mut a = MsvcRand::new(1);
        assert_eq!(a.r(10), 1); // 41 % 10
        assert_eq!(a.sign(), 1); // 18467 > 16383
        assert!(!a.odd()); // 6334
        let mut b = MsvcRand::new(1);
        assert_eq!(b.u(), 41.0 / 32767.0);
    }
}
