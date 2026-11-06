//! Producer-driven pump: one retry owner, one bounded producer retention slot, one bridge handoff.
//!
//! The producer owns its `unsent` snapshot. This pump never stores a second bridge-side retry copy:
//! `PublishOutcome::Retained` is not an accepted handoff and is followed by another producer
//! `export` on the next tick, with the producer's original source timestamp and revision preserved.
//! That also guarantees that an older bridge retry can never acknowledge a newer producer snapshot.

use std::fmt;

use crate::error::Error;
use crate::health::SourceHealth;
use crate::producer::{observation_from_producer, ProducerSnapshot};
use crate::publisher::{ObservationPublisher, PublishOutcome, PublishReceipt};

/// A producer export failure. The producer contract retains the un-handed-off revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducerExportError {
    message: String,
}

impl ProducerExportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ProducerExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProducerExportError {}

/// The producer operations required by [`pump_once`].
///
/// GLM can implement this on a local newtype around its `SessionState`; no bridge dependency is
/// needed in the pure producer crate. `mark_published` must be called only for the exact snapshot
/// returned by the immediately preceding `export` that the bridge accepted.
pub trait ProducerSession {
    type Snapshot: ProducerSnapshot;

    /// True while the producer retains an un-handed-off snapshot or has newer source state.
    fn export_due(&self) -> bool;

    /// Client↔world-server health, separate from observation freshness. `None` before a player
    /// scope exists.
    fn source_health(&self) -> Option<SourceHealth>;

    /// Returns the current snapshot to hand off, or `None` when fully handed off.
    fn export(
        &mut self,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
    ) -> std::result::Result<Option<Self::Snapshot>, ProducerExportError>;

    /// Acknowledges that the snapshot returned by the preceding `export` was accepted.
    fn mark_published(&mut self);
}

#[derive(Debug, PartialEq, Eq)]
pub enum PumpResult {
    /// No export was due.
    Idle,
    /// The bridge accepted the producer's current snapshot; the producer was marked published.
    HandedOff(PublishReceipt),
    /// The bridge handoff was busy. Nothing was acknowledged; the producer retains the snapshot
    /// and the next tick re-exports it.
    Retained,
}

#[derive(Debug)]
pub enum PumpError {
    Producer(ProducerExportError),
    Conversion(Error),
    Publisher(Error),
    /// The bridge already accepted a newer snapshot. Do not acknowledge the producer snapshot;
    /// this is an ordering-invariant failure under a single pump.
    Superseded {
        sequence: u64,
        latest_sequence: u64,
    },
}

impl fmt::Display for PumpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Producer(error) => write!(formatter, "producer export failed: {error}"),
            Self::Conversion(error) => write!(formatter, "observation conversion failed: {error}"),
            Self::Publisher(error) => write!(formatter, "publisher handoff failed: {error}"),
            Self::Superseded {
                sequence,
                latest_sequence,
            } => write!(
                formatter,
                "publisher already accepted newer sequence {latest_sequence}; refusing to acknowledge {sequence}"
            ),
        }
    }
}

impl std::error::Error for PumpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Producer(error) => Some(error),
            Self::Conversion(error) => Some(error),
            Self::Publisher(error) => Some(error),
            Self::Superseded { .. } => None,
        }
    }
}

impl From<ProducerExportError> for PumpError {
    fn from(error: ProducerExportError) -> Self {
        Self::Producer(error)
    }
}

/// Runs one retry/export/handoff step.
///
/// Call this on every driver tick, including ticks with no packets. `export_due` remains true
/// while the producer retains a snapshot, so retries do not depend on new source traffic.
pub fn pump_once<S>(
    publisher: &ObservationPublisher,
    producer: &mut S,
    exported_at_unix_ms: i64,
    client_uptime_ms: u64,
) -> std::result::Result<PumpResult, PumpError>
where
    S: ProducerSession,
{
    if let Some(health) = producer.source_health() {
        publisher
            .publish_health(health)
            .map_err(PumpError::Publisher)?;
    }

    if !producer.export_due() {
        return Ok(PumpResult::Idle);
    }

    let Some(snapshot) = producer
        .export(exported_at_unix_ms, client_uptime_ms)
        .map_err(PumpError::Producer)?
    else {
        return Ok(PumpResult::Idle);
    };

    let observation = observation_from_producer(&snapshot, exported_at_unix_ms)
        .map_err(PumpError::Conversion)?;

    match publisher.publish(observation) {
        Ok(PublishOutcome::HandedOff(receipt)) => {
            producer.mark_published();
            Ok(PumpResult::HandedOff(receipt))
        }
        Ok(PublishOutcome::Retained(_)) => {
            // The producer already owns the one bounded retained snapshot. Do not keep a second
            // bridge-side retry copy and do not acknowledge.
            Ok(PumpResult::Retained)
        }
        Ok(PublishOutcome::Superseded {
            sequence,
            latest_sequence,
        }) => Err(PumpError::Superseded {
            sequence,
            latest_sequence,
        }),
        Err(error) => Err(PumpError::Publisher(error)),
    }
}
