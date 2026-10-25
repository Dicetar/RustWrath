//! Bounded, loopback-only transport seam for native client observations.
//!
//! This crate owns the wire contract that GLM's application-boundary producer adapter can call and
//! that the Python WM receiver validates. It is not a packet reducer, combat authority, scheduler,
//! or general-purpose network API.
//!
//! # Contract
//!
//! - TCP loopback only, one active connection per receiver.
//! - 4-byte big-endian JSON length prefix. Frames are non-empty and at most
//!   [`MAX_FRAME_BYTES`].
//! - A required owner-provided bearer token is sent once in `hello`; it is never logged here.
//! - Observations are versioned, player-scoped, session-scoped, and sequence/revision ordered.
//! - [`LoopbackSink`] is synchronous. [`ObservationPublisher`] moves connect, handshake, and writes
//!   to one worker with a latest-only bounded handoff and explicit reconnect backoff. A busy
//!   handoff returns a caller-retained snapshot, not a dropped error.
//! - [`ProducerSnapshot`] is the accessor-based conversion boundary; it does not require a
//!   mirrored producer struct or a bridge dependency in the packet reducer. [`pump_once`] keeps
//!   the producer as the single retry owner and acknowledges only an accepted handoff.
//!
//! The matching Python receiver lives under `integrations/wm-native`.

#![forbid(unsafe_code)]

mod auth;
mod client;
mod error;
mod frame;
mod health;
mod observation;
mod producer;
mod publisher;
mod pump;

pub use auth::AuthToken;
pub use client::{
    AckReceipt, HealthAckReceipt, PeerState, DEFAULT_PORT, LoopbackConfig, LoopbackSink,
    SendReceipt,
};
pub use error::{Error, Result};
pub use frame::{MAX_FRAME_BYTES, PROTOCOL_NAME, PROTOCOL_VERSION};
pub use health::{SourceHealth, SourceSessionState};
pub use observation::{
    MAX_VISIBLE_GUIDS, OBSERVATION_SCHEMA_VERSION, Observation, Position, TargetObservation,
    Visibility,
};
pub use producer::{observation_from_producer, ProducerPosition, ProducerSnapshot};
pub use publisher::{
    DEFAULT_BUSY_RETRY_DELAY, DEFAULT_HEALTH_POLL_INTERVAL, DEFAULT_HEALTH_SEND_INTERVAL,
    DEFAULT_RECONNECT_INITIAL_DELAY, DEFAULT_RECONNECT_MAX_DELAY, DEFAULT_RETRY_MAX_AGE, LinkState,
    ObservationPublisher,
    PublishOutcome, PublishReceipt, PublisherConfig, PublisherHealth, PublisherRetirement,
    RetainReason, RetainedObservation, SnapshotStamp, WorkerState,
};
pub use pump::{
    pump_once, ProducerExportError, ProducerSession, PumpError, PumpResult,
};
