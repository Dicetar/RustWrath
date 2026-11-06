//! Typed failures. Input/envelope failures are errors; gameplay refusals are outcomes.

use std::fmt;

use crate::identity::{ActionRevision, PlayerId, Sequence, SessionEpoch};
use crate::time::Ticks;

pub type CombatResult<T> = Result<T, CombatError>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CombatError {
    InvalidConfig(&'static str),
    TickRateOutOfRange(u16),
    TickArithmeticOverflow,
    TimeWentBackwards { now: Ticks, requested: Ticks },
    AdvanceTooLarge { from: Ticks, to: Ticks, limit: u64 },
    SessionMismatch { expected: SessionEpoch, got: SessionEpoch },
    PlayerMismatch { expected: PlayerId, got: PlayerId },
    ActionRevisionMismatch { expected: ActionRevision, got: ActionRevision },
    DuplicateSequence { last: Sequence },
    StaleSequence { last: Sequence, got: Sequence },
    SequenceGapTooLarge { last: Sequence, got: Sequence, limit: u64 },
    InputInPast { now: Ticks, input: Ticks },
    InputTooFarAhead { now: Ticks, input: Ticks, limit: u64 },
    InputOutOfOrderTick { last: Ticks, got: Ticks },
    InputQueueFull { limit: usize },
    InvalidButtonState,
    PlayerDead,
    EnemyDead,
    EnemyBusy,
    SessionNotAdvanced { previous: SessionEpoch, requested: SessionEpoch },
    SnapshotSessionMismatch { expected: SessionEpoch, got: SessionEpoch },
    SnapshotPlayerMismatch { expected: PlayerId, got: PlayerId },
    SnapshotRevisionMismatch { expected: ActionRevision, got: ActionRevision },
    SnapshotStaleTick { current: Ticks, snapshot: Ticks },
    SnapshotStaleSequence { current: Option<Sequence>, snapshot: Option<Sequence> },
    SnapshotInvalid(&'static str),
}

impl fmt::Display for CombatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => write!(f, "invalid combat configuration: {reason}"),
            Self::TickRateOutOfRange(hz) => write!(f, "tick rate {hz} Hz is outside 1..=1000"),
            Self::TickArithmeticOverflow => write!(f, "tick arithmetic overflowed"),
            Self::TimeWentBackwards { now, requested } => {
                write!(f, "time cannot move backwards from {} to {}", now.0, requested.0)
            }
            Self::AdvanceTooLarge { from, to, limit } => write!(
                f,
                "advance from {} to {} exceeds the {limit}-tick catch-up bound",
                from.0, to.0
            ),
            Self::SessionMismatch { expected, got } => {
                write!(f, "input session {} does not match active session {}", got.0, expected.0)
            }
            Self::PlayerMismatch { expected, got } => {
                write!(f, "input player {} does not match active player {}", got.0, expected.0)
            }
            Self::ActionRevisionMismatch { expected, got } => write!(
                f,
                "input action revision {} does not match active revision {}",
                got.0, expected.0
            ),
            Self::DuplicateSequence { last } => {
                write!(f, "input sequence {} was already accepted", last.0)
            }
            Self::StaleSequence { last, got } => {
                write!(f, "input sequence {} is older than accepted sequence {}", got.0, last.0)
            }
            Self::SequenceGapTooLarge { last, got, limit } => write!(
                f,
                "input sequence gap {} -> {} exceeds the {limit}-sequence bound",
                last.0, got.0
            ),
            Self::InputInPast { now, input } => {
                write!(f, "input tick {} is before current tick {}", input.0, now.0)
            }
            Self::InputTooFarAhead { now, input, limit } => write!(
                f,
                "input tick {} is more than {limit} ticks ahead of current tick {}",
                input.0, now.0
            ),
            Self::InputOutOfOrderTick { last, got } => write!(
                f,
                "input tick {} is before already-accepted input tick {}",
                got.0, last.0
            ),
            Self::InputQueueFull { limit } => {
                write!(f, "deferred input queue is full at {limit} entries")
            }
            Self::InvalidButtonState => write!(f, "dodge button frame has an impossible state"),
            Self::PlayerDead => write!(f, "the player is dead"),
            Self::EnemyDead => write!(f, "the enemy is dead"),
            Self::EnemyBusy => write!(f, "the enemy already has a telegraph in progress"),
            Self::SessionNotAdvanced { previous, requested } => write!(
                f,
                "reconnect session {} is not newer than {}",
                requested.0, previous.0
            ),
            Self::SnapshotSessionMismatch { expected, got } => write!(
                f,
                "snapshot session {} does not match active session {}",
                got.0, expected.0
            ),
            Self::SnapshotPlayerMismatch { expected, got } => write!(
                f,
                "snapshot player {} does not match active player {}",
                got.0, expected.0
            ),
            Self::SnapshotRevisionMismatch { expected, got } => write!(
                f,
                "snapshot revision {} does not match active revision {}",
                got.0, expected.0
            ),
            Self::SnapshotStaleTick { current, snapshot } => write!(
                f,
                "snapshot tick {} is older than current tick {}",
                snapshot.0, current.0
            ),
            Self::SnapshotStaleSequence { current, snapshot } => write!(
                f,
                "snapshot sequence {:?} is older than current sequence {:?}",
                snapshot.map(|value| value.0),
                current.map(|value| value.0)
            ),
            Self::SnapshotInvalid(reason) => write!(f, "invalid combat snapshot: {reason}"),
        }
    }
}

impl std::error::Error for CombatError {}
