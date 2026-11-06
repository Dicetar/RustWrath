//! Monotonic server ticks. No wall clock and no floating-point timing enters the reducer.

/// An absolute server tick. Ticks are monotonic for the lifetime of one session.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Ticks(pub u64);

impl Ticks {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, ticks: u64) -> Option<Self> {
        self.0.checked_add(ticks).map(Self)
    }

    /// Saturates instead of wrapping. Used only where a late deadline is safer than a panic.
    pub fn saturating_add(self, ticks: u64) -> Self {
        Self(self.0.saturating_add(ticks))
    }

    pub fn saturating_sub(self, earlier: Self) -> Self {
        Self(self.0.saturating_sub(earlier.0))
    }

    pub fn elapsed_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

impl From<u64> for Ticks {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A half-open range of tick offsets: `[from, to)`.
///
/// Action data uses offsets from the tick an action started, not absolute ticks. The half-open
/// convention matches the donor's `from <= frame < to` windows and makes adjacent windows share a
/// boundary without overlapping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TickWindow {
    pub from: u64,
    pub to: u64,
}

impl TickWindow {
    pub fn new(from: u64, to: u64) -> Option<Self> {
        (from < to).then_some(Self { from, to })
    }

    pub fn contains(self, elapsed_ticks: u64) -> bool {
        self.from <= elapsed_ticks && elapsed_ticks < self.to
    }

    pub fn len(self) -> u64 {
        self.to.saturating_sub(self.from)
    }
}

/// The server's fixed tick rate, in whole hertz.
///
/// The reducer itself never derives time from this value. It exists so adapters can quantise a
/// client's millisecond timestamp to the next server tick with one documented rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TickRate(u16);

impl TickRate {
    pub const MIN_HZ: u16 = 1;
    pub const MAX_HZ: u16 = 1000;

    pub fn new(hz: u16) -> Option<Self> {
        (Self::MIN_HZ..=Self::MAX_HZ).contains(&hz).then_some(Self(hz))
    }

    pub const fn hz(self) -> u16 {
        self.0
    }

    /// Round a duration up to whole ticks. This is the only timestamp conversion the crate
    /// defines; adapters must round **up** so an input is never moved before the tick the server
    /// observed it.
    pub fn ticks_for_millis_ceil(self, millis: u64) -> Option<u64> {
        let numerator = millis.checked_mul(self.0 as u64)?;
        numerator.checked_add(999).map(|value| value / 1000)
    }

    /// Quantise a client elapsed-time value after an authoritative tick.
    pub fn quantise_after(self, now: Ticks, elapsed_millis: u64) -> Option<Ticks> {
        now.checked_add(self.ticks_for_millis_ceil(elapsed_millis)?)
    }

    /// Milliseconds represented by one tick, rounded up. This is descriptive only.
    pub fn millis_per_tick_ceil(self) -> u64 {
        (1000_u64 + self.0 as u64 - 1) / self.0 as u64
    }
}
