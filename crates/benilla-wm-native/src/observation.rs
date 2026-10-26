use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Version of the observation object and the matching Python receiver.
pub const OBSERVATION_SCHEMA_VERSION: &str = "wm.native.observation.v2.2";

/// Hard bound on the number of visible GUIDs exported in one observation.
pub const MAX_VISIBLE_GUIDS: usize = 256;

/// Client-space position. Values are source-observed world coordinates; no unit conversion belongs
/// in this seam.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub orientation: f32,
}

impl Position {
    pub fn new(x: f32, y: f32, z: f32, orientation: f32) -> Result<Self> {
        let position = Self {
            x,
            y,
            z,
            orientation,
        };
        position.validate()?;
        Ok(position)
    }

    pub fn validate(&self) -> Result<()> {
        if [self.x, self.y, self.z, self.orientation]
            .iter()
            .all(|value| value.is_finite())
        {
            Ok(())
        } else {
            Err(Error::InvalidObservation(
                "position components must be finite",
            ))
        }
    }
}

/// Target knowledge, not merely an optional target identifier.
///
/// `Unknown` means the producer has not observed target state. `None` means the producer observed
/// that there is no target. These states must never be collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetObservation {
    Unknown,
    None,
    Target { guid: u64 },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TargetState {
    Unknown,
    None,
    Target,
}

impl TargetObservation {
    pub fn target(guid: u64) -> Result<Self> {
        let target = Self::Target { guid };
        target.validate()?;
        Ok(target)
    }

    /// Maps a source that has already observed target state. `None` means observed no-target, not
    /// unknown; an unobserved target must use [`Self::Unknown`] directly or skip export.
    pub fn observed_optional(guid: Option<u64>) -> Result<Self> {
        match guid {
            None => Ok(Self::None),
            Some(guid) => Self::target(guid),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if matches!(self, Self::Target { guid: 0 }) {
            Err(Error::InvalidObservation("target guid must be non-zero"))
        } else {
            Ok(())
        }
    }
}

impl Serialize for TargetObservation {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unknown => {
                let mut target = serializer.serialize_struct("TargetObservation", 1)?;
                target.serialize_field("state", "unknown")?;
                target.end()
            }
            Self::None => {
                let mut target = serializer.serialize_struct("TargetObservation", 1)?;
                target.serialize_field("state", "none")?;
                target.end()
            }
            Self::Target { guid } => {
                let mut target = serializer.serialize_struct("TargetObservation", 2)?;
                target.serialize_field("state", "target")?;
                target.serialize_field("guid", guid)?;
                target.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for TargetObservation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(TargetVisitor)
    }
}

struct TargetVisitor;

impl<'de> Visitor<'de> for TargetVisitor {
    type Value = TargetObservation;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a target object with exact state and optional guid keys")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut state: Option<TargetState> = None;
        let mut guid: Option<Option<u64>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "state" => {
                    if state.is_some() {
                        return Err(de::Error::duplicate_field("state"));
                    }
                    state = Some(map.next_value()?);
                }
                "guid" => {
                    if guid.is_some() {
                        return Err(de::Error::duplicate_field("guid"));
                    }
                    guid = Some(map.next_value()?);
                }
                other => {
                    return Err(de::Error::unknown_field(other, &["state", "guid"]));
                }
            }
        }
        let state = state.ok_or_else(|| de::Error::missing_field("state"))?;
        match state {
            TargetState::Unknown => {
                if guid.is_some() {
                    return Err(de::Error::custom(
                        "unknown target state must not carry a guid",
                    ));
                }
                Ok(TargetObservation::Unknown)
            }
            TargetState::None => {
                if guid.is_some() {
                    return Err(de::Error::custom(
                        "none target state must not carry a guid",
                    ));
                }
                Ok(TargetObservation::None)
            }
            TargetState::Target => match guid {
                Some(Some(0)) | Some(None) | None => Err(de::Error::custom(
                    "target state requires a non-zero guid",
                )),
                Some(Some(guid)) => Ok(TargetObservation::Target { guid }),
            },
        }
    }
}

/// A bounded export of the producer's currently visible GUID set.
///
/// `total_guid_count` is the producer's total before the 256-entry export bound. `complete` is true
/// only when the producer asserts that `guids` is the complete visible set; consequently a complete
/// export must have `total_guid_count == guids.len()`. An incomplete export is explicit and may not
/// be presented by consumers as complete visibility.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Visibility {
    pub complete: bool,
    pub total_guid_count: u64,
    pub guids: Vec<u64>,
}

impl Visibility {
    pub fn new(complete: bool, total_guid_count: u64, guids: Vec<u64>) -> Result<Self> {
        let visibility = Self {
            complete,
            total_guid_count,
            guids,
        };
        visibility.validate()?;
        Ok(visibility)
    }
    /// Constructs an explicitly complete export. Rejects over-bound/unsorted/invalid lists; never
    /// silently truncates.
    pub fn from_complete_guids(guids: Vec<u64>) -> Result<Self> {
        let total_guid_count = guids.len() as u64;
        Self::new(true, total_guid_count, guids)
    }

    pub fn validate(&self) -> Result<()> {
        if self.guids.len() > MAX_VISIBLE_GUIDS {
            return Err(Error::InvalidObservation(
                "visibility guids exceed the observation bound; mark the export incomplete and bound it before construction",
            ));
        }
        let mut previous = None;
        for &guid in &self.guids {
            if guid == 0 {
                return Err(Error::InvalidObservation(
                    "visibility guids must not contain zero",
                ));
            }
            if let Some(previous) = previous {
                if previous == guid {
                    return Err(Error::InvalidObservation(
                        "visibility guids must not contain duplicates",
                    ));
                }
                if previous > guid {
                    return Err(Error::InvalidObservation(
                        "visibility guids must be sorted by full u64 GUID",
                    ));
                }
            }
            previous = Some(guid);
        }
        let exported_count = self.guids.len() as u64;
        if self.total_guid_count < exported_count {
            return Err(Error::InvalidObservation(
                "visibility total_guid_count is smaller than the exported list",
            ));
        }
        if self.complete && self.total_guid_count != exported_count {
            return Err(Error::InvalidObservation(
                "complete visibility requires total_guid_count == guids.len()",
            ));
        }
        Ok(())
    }
}

/// Observed combat state, from the replicated self unit's `UNIT_FIELD_FLAGS`.
///
/// `Unknown` means the producer has not observed the flag word; it is never a synonym for
/// out of combat.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CombatObservation {
    Unknown,
    OutOfCombat,
    InCombat,
}

/// Observed life state. Ghost is a player-only state and takes precedence over zero health.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeathObservation {
    Unknown,
    Alive,
    Dead,
    Ghost,
}

/// Observed health and current power. Each field is independently observed: a missing value is
/// `None`, never a zero.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservation {
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    pub power_type: Option<u8>,
    pub power: Option<u32>,
}

impl ResourceObservation {
    pub fn validate(&self) -> Result<()> {
        if let (Some(health), Some(max_health)) = (self.health, self.max_health) {
            if health > max_health {
                return Err(Error::InvalidObservation(
                    "resources.health must not exceed resources.max_health",
                ));
            }
        }
        if let Some(power_type) = self.power_type {
            if power_type > 6 {
                return Err(Error::InvalidObservation(
                    "resources.power_type must be 0..=6",
                ));
            }
        }
        if self.power.is_some() && self.power_type.is_none() {
            return Err(Error::InvalidObservation(
                "resources.power requires resources.power_type",
            ));
        }
        Ok(())
    }
}

/// One versioned native-client observation.
///
/// `session_epoch` is a nonzero, process-local generation. It strictly increases on a new world
/// session or reducer reset within one process. A new process starts a new authenticated transport
/// connection, which resets the receiver's numeric epoch comparison, so epochs need not be globally
/// persistent or monotonic across process restarts.
///
/// `sequence` strictly increases per exported observation within an epoch. `state_revision`
/// increases only when source-observed state changes. `observed_at_unix_ms` is the source-observation
/// time associated with `state_revision`; re-exporting unchanged state keeps both values unchanged.
/// `exported_at_unix_ms` and `client_uptime_ms` describe this export and do not renew freshness.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    schema_version: String,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub player_guid: u64,
    pub sequence: u64,
    pub state_revision: u64,
    pub observed_at_unix_ms: i64,
    pub exported_at_unix_ms: i64,
    pub client_uptime_ms: u64,
    pub map_id: u32,
    pub zone_id: Option<u32>,
    pub area_id: Option<u32>,
    pub position_observed_at_unix_ms: i64,
    pub position: Position,
    pub target: TargetObservation,
    /// The self unit's replicated combat flag; `Unknown` before that field is observed.
    pub combat: CombatObservation,
    /// Self health and current power, each independently observed.
    pub resources: ResourceObservation,
    /// Self life state (ghost precedence over zero health).
    pub death: DeathObservation,
    pub visibility: Visibility,
}

impl Observation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_epoch: u64,
        world_generation: u64,
        player_guid: u64,
        sequence: u64,
        state_revision: u64,
        observed_at_unix_ms: i64,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
        map_id: u32,
        zone_id: Option<u32>,
        area_id: Option<u32>,
        position_observed_at_unix_ms: i64,
        position: Position,
        target: TargetObservation,
        combat: CombatObservation,
        resources: ResourceObservation,
        death: DeathObservation,
        visibility: Visibility,
    ) -> Result<Self> {
        let observation = Self {
            schema_version: OBSERVATION_SCHEMA_VERSION.to_owned(),
            session_epoch,
            world_generation,
            player_guid,
            sequence,
            state_revision,
            observed_at_unix_ms,
            exported_at_unix_ms,
            client_uptime_ms,
            map_id,
            zone_id,
            area_id,
            position_observed_at_unix_ms,
            position,
            target,
            combat,
            resources,
            death,
            visibility,
        };
        observation.validate()?;
        Ok(observation)
    }

    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    /// Validates invariants that both the Rust sender and Python receiver enforce.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != OBSERVATION_SCHEMA_VERSION {
            return Err(Error::InvalidObservation(
                "unsupported observation schema_version",
            ));
        }
        if self.session_epoch == 0 {
            return Err(Error::InvalidObservation(
                "session_epoch must be non-zero",
            ));
        }
        if self.world_generation == 0 {
            return Err(Error::InvalidObservation(
                "world_generation must be non-zero",
            ));
        }
        if self.player_guid == 0 {
            return Err(Error::InvalidObservation("player_guid must be non-zero"));
        }
        if self.observed_at_unix_ms < 0 {
            return Err(Error::InvalidObservation(
                "observed_at_unix_ms must not be negative",
            ));
        }
        if self.exported_at_unix_ms < 0 {
            return Err(Error::InvalidObservation(
                "exported_at_unix_ms must not be negative",
            ));
        }
        if self.exported_at_unix_ms < self.observed_at_unix_ms {
            return Err(Error::InvalidObservation(
                "exported_at_unix_ms must not precede observed_at_unix_ms",
            ));
        }
        if self.position_observed_at_unix_ms < 0 {
            return Err(Error::InvalidObservation(
                "position_observed_at_unix_ms must not be negative",
            ));
        }
        if self.position_observed_at_unix_ms > self.observed_at_unix_ms {
            return Err(Error::InvalidObservation(
                "position_observed_at_unix_ms must not follow observed_at_unix_ms",
            ));
        }
        self.position.validate()?;
        self.target.validate()?;
        self.resources.validate()?;
        self.visibility.validate()?;
        if self
            .visibility
            .guids
            .binary_search(&self.player_guid)
            .is_ok()
        {
            return Err(Error::InvalidObservation(
                "visibility must exclude the player's own guid",
            ));
        }
        Ok(())
    }

    /// Compares only source-observed state, excluding export ordering and timestamps.
    pub fn same_state(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.session_epoch == other.session_epoch
            && self.world_generation == other.world_generation
            && self.player_guid == other.player_guid
            && self.map_id == other.map_id
            && self.zone_id == other.zone_id
            && self.area_id == other.area_id
            && self.position_observed_at_unix_ms == other.position_observed_at_unix_ms
            && self.position == other.position
            && self.target == other.target
            && self.combat == other.combat
            && self.resources == other.resources
            && self.death == other.death
            && self.visibility == other.visibility
    }

    /// Validates ordering and freshness against the previously published or accepted observation.
    ///
    /// A larger epoch starts a new ordering scope. Within one epoch, an unchanged `state_revision`
    /// must describe identical state and retain exactly the same `observed_at_unix_ms`; this rejects
    /// refreshing stale state by republishing it with a new source timestamp. A larger revision may
    /// have the same exported content because a latest-only transport can coalesce A -> B -> A.
    pub fn validate_after(&self, previous: &Self) -> Result<()> {
        self.validate()?;
        if self.player_guid != previous.player_guid {
            return Err(Error::PlayerChanged {
                last: previous.player_guid,
                got: self.player_guid,
            });
        }
        if self.session_epoch < previous.session_epoch {
            return Err(Error::StaleSession {
                last: previous.session_epoch,
                got: self.session_epoch,
            });
        }
        if self.session_epoch > previous.session_epoch {
            return Ok(());
        }
        if self.world_generation < previous.world_generation {
            return Err(Error::WorldGenerationRegression {
                last: previous.world_generation,
                got: self.world_generation,
            });
        }
        if self.sequence == previous.sequence {
            return Err(Error::DuplicateSequence {
                last: previous.sequence,
            });
        }
        if self.sequence < previous.sequence {
            return Err(Error::SequenceNotIncreasing {
                last: previous.sequence,
                got: self.sequence,
            });
        }
        if self.client_uptime_ms < previous.client_uptime_ms {
            return Err(Error::UptimeNotIncreasing {
                last: previous.client_uptime_ms,
                got: self.client_uptime_ms,
            });
        }
        if self.observed_at_unix_ms < previous.observed_at_unix_ms {
            return Err(Error::ObservationTimeRegression {
                last: previous.observed_at_unix_ms,
                got: self.observed_at_unix_ms,
            });
        }
        if self.world_generation == previous.world_generation
            && self.position_observed_at_unix_ms < previous.position_observed_at_unix_ms
        {
            return Err(Error::PositionTimeRegression {
                last: previous.position_observed_at_unix_ms,
                got: self.position_observed_at_unix_ms,
            });
        }
        if self.state_revision < previous.state_revision {
            return Err(Error::StateRevisionRegression {
                last: previous.state_revision,
                got: self.state_revision,
            });
        }
        if self.state_revision == previous.state_revision {
            if !self.same_state(previous) {
                return Err(Error::StateRevisionConflict {
                    revision: self.state_revision,
                });
            }
            if self.observed_at_unix_ms != previous.observed_at_unix_ms {
                return Err(Error::UnchangedStateRefresh {
                    last: previous.observed_at_unix_ms,
                    got: self.observed_at_unix_ms,
                });
            }
        }
        Ok(())
    }
}
