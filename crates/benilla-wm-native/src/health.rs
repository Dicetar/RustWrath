use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Client↔world-server session health, independent of observation freshness and receiver transport.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSessionState {
    /// The client has an active world session. A quiet world is still connected.
    InWorld,
    /// A transfer/loading window is active; old-world action eligibility is not valid.
    Transferring,
    /// Logout/disconnect/failure. No observation from this scope is eligible.
    Disconnected,
}

/// Producer-owned world-session health sent over the observation transport.
///
/// This frame never changes `observed_at_unix_ms`, `state_revision`, or field provenance. It only
/// reports scope and world-session liveness for the current snapshot.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceHealth {
    pub session_epoch: u64,
    pub world_generation: u64,
    pub player_guid: u64,
    pub state: SourceSessionState,
    /// Arrival wall time of the last client↔world-server packet, for diagnostics/correlation.
    pub last_server_packet_at_unix_ms: Option<i64>,
}

impl SourceHealth {
    pub fn new(
        session_epoch: u64,
        world_generation: u64,
        player_guid: u64,
        state: SourceSessionState,
        last_server_packet_at_unix_ms: Option<i64>,
    ) -> Result<Self> {
        let health = Self {
            session_epoch,
            world_generation,
            player_guid,
            state,
            last_server_packet_at_unix_ms,
        };
        health.validate()?;
        Ok(health)
    }

    pub fn validate(&self) -> Result<()> {
        if self.session_epoch == 0 {
            return Err(Error::InvalidObservation(
                "health session_epoch must be non-zero",
            ));
        }
        if self.world_generation == 0 {
            return Err(Error::InvalidObservation(
                "health world_generation must be non-zero",
            ));
        }
        if self.player_guid == 0 {
            return Err(Error::InvalidObservation(
                "health player_guid must be non-zero",
            ));
        }
        if self
            .last_server_packet_at_unix_ms
            .is_some_and(|value| value < 0)
        {
            return Err(Error::InvalidObservation(
                "health last_server_packet_at_unix_ms must not be negative",
            ));
        }
        Ok(())
    }

    pub fn same_scope(&self, session_epoch: u64, world_generation: u64, player_guid: u64) -> bool {
        self.session_epoch == session_epoch
            && self.world_generation == world_generation
            && self.player_guid == player_guid
    }

    pub fn is_in_world(&self) -> bool {
        self.state == SourceSessionState::InWorld
    }
}
