//! The deterministic RNG: SplitMix64 for the stream, plus a domain fork that derives
//! independent substreams from the master seed without consuming stream state.
//!
//! The fork rule is the crate's isolation contract: `fork(domain)` is a pure function of
//! `(master_seed, domain)` — never of how many rolls happened before it — so subsystems
//! cannot perturb each other through call order.

/// Master-seeded RNG with domain forking.
pub struct ProcRng {
    /// Immutable origin; forks derive from this, never from the rolling state.
    master: u64,
    /// The rolling stream state.
    state: u64,
}

impl ProcRng {
    pub fn new(seed: u64) -> Self {
        Self { master: seed, state: seed }
    }

    /// Next value in the stream (SplitMix64).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 24 bits of mantissa entropy.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }

    /// Uniform float in `[min, max)`.
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    /// Integer in `[min, max]` (inclusive), or `min` when the bounds are reversed.
    /// Modulo reduction preserves the stream contract; non-power-of-two spans have modulo bias.
    pub fn int(&mut self, min: i64, max: i64) -> i64 {
        if max <= min {
            return min;
        }
        // The inclusive i64 domain has 2^64 values; neither its span nor every offset fits i64.
        let span = (i128::from(max) - i128::from(min) + 1) as u128;
        let offset = u128::from(self.next_u64()) % span;
        (i128::from(min) + offset as i128) as i64
    }

    /// A uniform element of `slice` (slice must be non-empty).
    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        &slice[self.int(0, slice.len() as i64 - 1) as usize]
    }

    /// A weighted element: `(value, weight)` pairs with positive weights summing > 0.
    pub fn weighted<'a, T>(&mut self, items: &'a [(T, u32)]) -> &'a T {
        let total: u64 = items.iter().map(|(_, w)| *w as u64).sum();
        let mut pick = self.next_u64() % total;
        for (value, weight) in items {
            if pick < *weight as u64 {
                return value;
            }
            pick -= *weight as u64;
        }
        // Unreachable when weights are positive; fall back to the last element otherwise.
        &items[items.len() - 1].0
    }

    /// The current stream state without advancing (identity derivation).
    pub fn peek_state(&self) -> u64 {
        self.state
    }

    /// Derive an independent substream for `domain`. Does not consume or alter `self`.
    pub fn fork(&self, domain: u64) -> ProcRng {
        let mixed = mix(self.master ^ domain.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        ProcRng::new(mixed)
    }
}

/// The mixing step of SplitMix64, as a pure function (the fork mixer).
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = ProcRng::new(42);
        let mut b = ProcRng::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn fork_is_history_independent() {
        let mut a = ProcRng::new(7);
        let mut b = ProcRng::new(7);
        for _ in 0..100 {
            a.next_u64();
        }
        // `a` has rolled 100 times; `b` has not. Forks from the same domain must agree.
        let mut fa = a.fork(99);
        let mut fb = b.fork(99);
        assert_eq!(fa.next_u64(), fb.next_u64());
        assert_eq!(fa.next_u64(), fb.next_u64());
    }

    #[test]
    fn distinct_domains_fork_apart() {
        let rng = ProcRng::new(7);
        let mut x = rng.fork(1);
        let mut y = rng.fork(2);
        assert_ne!(x.next_u64(), y.next_u64());
    }

    #[test]
    fn ranges_stay_inside() {
        let mut rng = ProcRng::new(1234);
        for _ in 0..1000 {
            let v = rng.range(-1.0, 1.0);
            assert!((-1.0..1.0).contains(&v));
            let i = rng.int(3, 7);
            assert!((3..=7).contains(&i));
        }
    }
}
