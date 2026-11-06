//! Identity and ordering values shared by inputs, snapshots, and reconciliation.

/// A connection generation. Reconnect creates a new epoch; an old epoch's inputs are not current.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SessionEpoch(pub u64);

/// The stable player identity carried by observations and combat inputs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct PlayerId(pub u64);

/// A per-player monotonically increasing input sequence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Sequence(pub u64);

/// Identifies the authored action table revision an input or state was produced against.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ActionRevision(pub u32);
