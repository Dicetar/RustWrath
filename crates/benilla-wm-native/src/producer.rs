//! Conversion boundary from a source session snapshot into the stable observation schema.
//!
//! The trait deliberately uses accessors rather than a mirrored struct layout. GLM's application
//! layer can implement it on a local newtype over its `SelfObservation`, avoiding both a mirrored
//! struct and a bridge dependency in the pure packet reducer.

use crate::error::Result;
use crate::observation::{
    AurasObservation, CastObservation, ChannelObservation, CombatObservation, DeathObservation, LastCastEndObservation, Observation, Position, ResourceObservation, TargetObservation, Visibility,
};

/// Producer position with bridge-facing orientation naming. GLM's `PositionSnapshot::o` maps here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProducerPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub orientation: f32,
}

/// The source semantics required by the consumer boundary.
///
/// A successful source snapshot must already reflect source-observed state. Missing optional
/// fields default to `None`; they are not prerequisites for conversion.
pub trait ProducerSnapshot {
    fn session_epoch(&self) -> u64;
    fn world_generation(&self) -> u64;
    fn player_guid(&self) -> u64;
    fn sequence(&self) -> u64;

    /// Source-change revision. This is the producer's own `state_revision`, never its export
    /// sequence.
    fn state_revision(&self) -> u64;

    fn observed_at_unix_ms(&self) -> i64;
    fn client_uptime_ms(&self) -> u64;
    fn map_id(&self) -> u32;
    fn position_observed_at_unix_ms(&self) -> i64;
    fn position(&self) -> ProducerPosition;

    /// `Unknown` is only for a producer that exports before target state is observed.
    fn target(&self) -> Result<TargetObservation>;

    /// Must be explicitly complete or explicitly incomplete; never silently truncated.
    fn visibility(&self) -> Result<Visibility>;

    /// The self unit's observed combat flag; default Unknown.
    fn combat(&self) -> CombatObservation {
        CombatObservation::Unknown
    }

    /// Observed self health and power; construction errors reach the pump as a producer failure.
    fn resources(&self) -> Result<ResourceObservation> {
        Ok(ResourceObservation::all_unobserved())
    }

    /// Observed self life state; default Unknown.
    fn death(&self) -> DeathObservation {
        DeathObservation::Unknown
    }

    /// The self unit's active cast; default Unknown (never an observed no-cast).
    fn casting(&self) -> CastObservation {
        CastObservation::unknown()
    }

    /// The self unit's active channel; default Unknown.
    fn channeling(&self) -> ChannelObservation {
        ChannelObservation::unknown()
    }

    /// The self unit's last terminal cast edge; default Unknown.
    fn last_cast_end(&self) -> LastCastEndObservation {
        LastCastEndObservation::unknown()
    }

    /// The self unit's observed aura slots; default Unknown (distinct from zero auras).
    fn auras(&self) -> AurasObservation {
        AurasObservation::unknown()
    }

    fn zone_id(&self) -> Option<u32> {
        None
    }

    fn area_id(&self) -> Option<u32> {
        None
    }
}

/// Converts one source snapshot into the bridge's versioned observation.
pub fn observation_from_producer<P>(
    source: &P,
    exported_at_unix_ms: i64,
) -> Result<Observation>
where
    P: ProducerSnapshot + ?Sized,
{
    let position = source.position();
    Observation::new(
        source.session_epoch(),
        source.world_generation(),
        source.player_guid(),
        source.sequence(),
        source.state_revision(),
        source.observed_at_unix_ms(),
        exported_at_unix_ms,
        source.client_uptime_ms(),
        source.map_id(),
        source.zone_id(),
        source.area_id(),
        source.position_observed_at_unix_ms(),
        Position::new(
            position.x,
            position.y,
            position.z,
            position.orientation,
        )?,
        source.target()?,
        source.combat(),
        source.resources()?,
        source.death(),
        source.casting(),
        source.channeling(),
        source.last_cast_end(),
        source.auras(),
        source.visibility()?,
    )
}
