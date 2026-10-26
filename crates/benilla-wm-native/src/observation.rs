use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Version of the observation object and the matching Python receiver.
pub const OBSERVATION_SCHEMA_VERSION: &str = "wm.native.observation.v2.3";

/// Hard bound on the number of visible GUIDs exported in one observation.
pub const MAX_VISIBLE_GUIDS: usize = 256;

/// Client power pairs on the wire: `UNIT_FIELD_POWER1..7` and `MAXPOWER1..7`.
pub const POWER_PAIR_COUNT: usize = 7;

/// Hard bound on the number of self aura slots exported in one observation.
pub const MAX_AURA_SLOTS: usize = 128;

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

/// Observed health and power. The active pair keeps the flat `power_type`/`power` fields;
/// all seven client power pairs are exported independently in `powers`/`max_powers`, indexed by
/// power type 0..=6. Each value is `Some` only when the source delivered that word: `Some(0)` is
/// an observed zero, `None` is unavailable. The two spellings of the active pair must agree when
/// both are observed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservation {
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    pub power_type: Option<u8>,
    pub power: Option<u32>,
    /// `UNIT_FIELD_POWER1..7` current values, indexed by power type.
    pub powers: Vec<Option<u32>>,
    /// `UNIT_FIELD_MAXPOWER1..7` values, indexed by power type.
    pub max_powers: Vec<Option<u32>>,
}

impl ResourceObservation {
    /// All-unobserved form: seven absent pairs, no active power word.
    pub fn all_unobserved() -> Self {
        Self {
            health: None,
            max_health: None,
            power_type: None,
            power: None,
            powers: vec![None; POWER_PAIR_COUNT],
            max_powers: vec![None; POWER_PAIR_COUNT],
        }
    }
    /// Wraps the seven paired values; never silently resizes.
    pub fn pairs(
        health: Option<u32>,
        max_health: Option<u32>,
        power_type: Option<u8>,
        power: Option<u32>,
        powers: [Option<u32>; POWER_PAIR_COUNT],
        max_powers: [Option<u32>; POWER_PAIR_COUNT],
    ) -> Result<Self> {
        let resources = Self {
            health,
            max_health,
            power_type,
            power,
            powers: powers.to_vec(),
            max_powers: max_powers.to_vec(),
        };
        resources.validate()?;
        Ok(resources)
    }

    pub fn validate(&self) -> Result<()> {
        if let (Some(health), Some(max_health)) = (self.health, self.max_health) {
            if health > max_health {
                return Err(Error::InvalidObservation(
                    "resources.health must not exceed resources.max_health",
                ));
            }
        }
        if let Some(power_type) = self.power_type {
            if power_type as usize >= POWER_PAIR_COUNT {
                return Err(Error::InvalidObservation(
                    "resources.power_type must be 0..=6",
                ));
            }
        }
        if self.powers.len() != POWER_PAIR_COUNT {
            return Err(Error::InvalidObservation(
                "resources.powers must carry exactly 7 power pairs",
            ));
        }
        if self.max_powers.len() != POWER_PAIR_COUNT {
            return Err(Error::InvalidObservation(
                "resources.max_powers must carry exactly 7 power pairs",
            ));
        }
        for (word, max_word) in self.powers.iter().zip(self.max_powers.iter()) {
            if let (Some(word), Some(max_word)) = (word, max_word) {
                if word > max_word {
                    return Err(Error::InvalidObservation(
                        "resources.powers must not exceed resources.max_powers",
                    ));
                }
            }
        }
        if let (Some(power_type), Some(power)) = (self.power_type, self.power) {
            if let Some(active) = self.powers.get(usize::from(power_type)) {
                if let Some(active) = *active {
                    if active != power {
                        return Err(Error::InvalidObservation(
                            "resources.power must agree with the active powers entry",
                        ));
                    }
                }
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

/// The complete parsed spell target block of one cast attempt.
///
/// Every member is independently optional exactly as the wire block carries them; a member the
/// START block omitted is `None`, not a derived zero.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpellTargetsObservation {
    pub mask: u32,
    pub unit: Option<u64>,
    pub gameobject: Option<u64>,
    pub item: Option<u64>,
    pub source: Option<[f32; 3]>,
    pub source_transport: Option<u64>,
    pub dest: Option<[f32; 3]>,
    pub dest_transport: Option<u64>,
    pub string: Option<String>,
}

impl SpellTargetsObservation {
    pub fn validate(&self) -> Result<()> {
        if let Some(source) = self.source {
            if source.iter().any(|value| !value.is_finite()) {
                return Err(Error::InvalidObservation(
                    "spell targets source must be finite",
                ));
            }
        }
        if let Some(dest) = self.dest {
            if dest.iter().any(|value| !value.is_finite()) {
                return Err(Error::InvalidObservation("spell targets dest must be finite"));
            }
        }
        Ok(())
    }
}

/// Explicit cast-channel-knowledge states shared by the cast/channel blocks: the producer has
/// not observed the unit, observed no activity, or observes an active attempt. An observed
/// no-cast is never folded into `Unknown`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Unknown,
    None,
    Active,
}

/// One observed cancel request is client authority state: a local cancel command was issued and
/// the authoritative terminal is still awaited. It never implies an outcome.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CastObservation {
    pub state: ActivityState,
    /// This model's stable per-attempt handle while the START edge is open.
    pub attempt_serial: u64,
    pub spell_id: u32,
    /// 12340's `m_cast_count`; `None` only where the specific wire edge omits it.
    pub cast_count: Option<u8>,
    pub cast_item: Option<u64>,
    pub targets: Option<SpellTargetsObservation>,
    pub cast_flags: Option<u32>,
    pub ammo_display_id: Option<u32>,
    /// 12340 START `m_timer`: milliseconds left at packet write. `None` when the START omitted it.
    pub start_remaining_ms: Option<u32>,
    pub pushback_ms: u32,
    pub cancel_requested: bool,
    /// `None` unless the wire established it; consumers use spell metadata for the hard bit.
    pub interruptible: Option<bool>,
    /// 12340 GO game-time timestamp, `None` before the launch edge.
    pub go_server_time_ms: Option<u32>,
    /// Packet-arrival wall clock of the attempt's START edge (ms since Unix epoch).
    pub start_observed_at_unix_ms: i64,
    /// Client `GetTime()` seconds at the START drain; client-process-local domain, not shared
    /// with the receiver.
    pub start_ui_now_seconds: Option<f64>,
    /// Client `GetTime()` seconds of the latest resolved edge; updates on pushback, never the origin.
    pub last_update_ui_now_seconds: Option<f64>,
}

impl CastObservation {
    /// All-unobserved form: no cast knowledge at all, distinct from an observed no-cast.
    pub fn unknown() -> Self {
        Self {
            state: ActivityState::Unknown,
            attempt_serial: 0,
            spell_id: 0,
            cast_count: None,
            cast_item: None,
            targets: None,
            cast_flags: None,
            ammo_display_id: None,
            start_remaining_ms: None,
            pushback_ms: 0,
            cancel_requested: false,
            interruptible: None,
            go_server_time_ms: None,
            start_observed_at_unix_ms: 0,
            start_ui_now_seconds: None,
            last_update_ui_now_seconds: None,
        }
    }

    /// An observed no-cast: the source has seen the authoritative terminal edge.
    pub fn none() -> Self {
        let mut cast = Self::unknown();
        cast.state = ActivityState::None;
        cast
    }

    pub fn validate(&self) -> Result<()> {
        match self.state {
            ActivityState::Unknown | ActivityState::None => {
                if self.attempt_serial != 0
                    || self.spell_id != 0
                    || self.cast_count.is_some()
                    || self.cast_item.is_some()
                    || self.targets.is_some()
                    || self.cast_flags.is_some()
                    || self.ammo_display_id.is_some()
                    || self.start_remaining_ms.is_some()
                    || self.pushback_ms != 0
                    || self.cancel_requested
                    || self.interruptible.is_some()
                    || self.go_server_time_ms.is_some()
                    || self.start_ui_now_seconds.is_some()
                    || self.last_update_ui_now_seconds.is_some()
                {
                    return Err(Error::InvalidObservation(
                        "a non-active cast block must not carry activity fields",
                    ));
                }
                if self.start_observed_at_unix_ms != 0 {
                    return Err(Error::InvalidObservation(
                        "a non-active cast block must not carry a start clock",
                    ));
                }
            }
            ActivityState::Active => {
                if self.spell_id == 0 {
                    return Err(Error::InvalidObservation(
                        "an active cast requires a non-zero spell id",
                    ));
                }
                if self.attempt_serial == 0 {
                    return Err(Error::InvalidObservation(
                        "an active cast requires a non-zero attempt_serial",
                    ));
                }
                if self.start_observed_at_unix_ms < 0 {
                    return Err(Error::InvalidObservation(
                        "cast start clock must not be negative",
                    ));
                }
                if self.last_update_ui_now_seconds.is_some()
                    && self.start_ui_now_seconds.is_none()
                {
                    return Err(Error::InvalidObservation(
                        "a cast update clock requires a start clock",
                    ));
                }
                if self
                    .start_ui_now_seconds
                    .is_some_and(|clock| !clock.is_finite())
                    || self
                        .last_update_ui_now_seconds
                        .is_some_and(|clock| !clock.is_finite())
                {
                    return Err(Error::InvalidObservation(
                        "cast UI clocks must be finite",
                    ));
                }
                if let Some(targets) = &self.targets {
                    targets.validate()?;
                }
                if let Some(go_server_time_ms) = self.go_server_time_ms {
                    if go_server_time_ms >= 86_400_000 {
                        return Err(Error::InvalidObservation(
                            "cast GO server time exceeds a one-day window",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

/// One active channel as observed: the START anchor survives updates for reload, and update
/// clocks advance while the channel runs. No channel terminal is exported as state; a completed
/// channel removes the entry (observed `none`), matching the native read model.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelObservation {
    pub state: ActivityState,
    pub spell_id: Option<u32>,
    pub duration_ms: Option<u32>,
    pub remaining_ms: Option<u32>,
    /// Packet-arrival wall clock of the channel's START edge.
    pub start_observed_at_unix_ms: Option<i64>,
    /// Packet-arrival wall clock of the latest channel update edge.
    pub update_observed_at_unix_ms: Option<i64>,
    /// Client `GetTime()` seconds at the START drain; client-process-local domain.
    pub start_ui_now_seconds: Option<f64>,
    pub last_update_ui_now_seconds: Option<f64>,
}

impl ChannelObservation {
    /// All-unobserved form.
    pub fn unknown() -> Self {
        Self {
            state: ActivityState::Unknown,
            spell_id: None,
            duration_ms: None,
            remaining_ms: None,
            start_observed_at_unix_ms: None,
            update_observed_at_unix_ms: None,
            start_ui_now_seconds: None,
            last_update_ui_now_seconds: None,
        }
    }

    /// An observed no-channel: the source tracks channel edges and has none active.
    pub fn none() -> Self {
        let mut channel = Self::unknown();
        channel.state = ActivityState::None;
        channel
    }

    pub fn validate(&self) -> Result<()> {
        match self.state {
            ActivityState::Unknown | ActivityState::None => {
                if self.spell_id.is_some()
                    || self.duration_ms.is_some()
                    || self.remaining_ms.is_some()
                    || self.start_observed_at_unix_ms.is_some()
                    || self.update_observed_at_unix_ms.is_some()
                    || self.start_ui_now_seconds.is_some()
                    || self.last_update_ui_now_seconds.is_some()
                {
                    return Err(Error::InvalidObservation(
                        "a non-active channel block must not carry activity fields",
                    ));
                }
            }
            ActivityState::Active => {
                let spell_id = self
                    .spell_id
                    .ok_or_else(|| Error::InvalidObservation("an active channel requires a spell id"))?;
                if spell_id == 0 {
                    return Err(Error::InvalidObservation(
                        "an active channel requires a non-zero spell id",
                    ));
                }
                self.duration_ms.ok_or_else(|| {
                    Error::InvalidObservation("an active channel requires a duration")
                })?;
                self.remaining_ms.ok_or_else(|| {
                    Error::InvalidObservation("an active channel requires a remaining time")
                })?;
                self.start_observed_at_unix_ms.ok_or_else(|| {
                    Error::InvalidObservation("an active channel requires a start clock")
                })?;
                self.update_observed_at_unix_ms.ok_or_else(|| {
                    Error::InvalidObservation("an active channel requires an update clock")
                })?;
                if self.last_update_ui_now_seconds.is_some()
                    && self.start_ui_now_seconds.is_none()
                {
                    return Err(Error::InvalidObservation(
                        "a channel update clock requires a start clock",
                    ));
                }
                if self
                    .start_ui_now_seconds
                    .is_some_and(|clock| !clock.is_finite())
                    || self
                        .last_update_ui_now_seconds
                        .is_some_and(|clock| !clock.is_finite())
                {
                    return Err(Error::InvalidObservation(
                        "channel UI clocks must be finite",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Terminal cast outcomes exactly as the wire resolves them. `Scoped` covers a transfer/replacement
/// or session loss that closed the attempt without a gameplay verdict.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CastOutcome {
    Launched,
    Failed,
    Interrupted,
    ScopeLost,
    Unknown,
}

/// Terminal-edge presence: the producer has not observed a terminal edge, or it retains one.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Unknown,
    Present,
}

/// Aura-slot knowledge: not yet observed, or the observed slot set (possibly empty).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuraKnowledge {
    Unknown,
    Known,
}

/// The last terminal cast edge, retaining the attempt identity after the active entry is gone.
/// Terminal distinctions are preserved: none of these outcomes is inferred from another.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LastCastEndObservation {
    pub state: TerminalState,
    pub outcome: CastOutcome,
    pub attempt_serial: Option<u64>,
    pub spell_id: Option<u32>,
    pub cast_count: Option<u8>,
    pub cast_item: Option<u64>,
    pub target: Option<u64>,
    pub targets: Option<SpellTargetsObservation>,
    /// Packet-arrival wall clock of the terminal edge.
    pub ended_observed_at_unix_ms: Option<i64>,
}

impl LastCastEndObservation {
    /// All-unobserved form: no terminal edge retained.
    pub fn unknown() -> Self {
        Self {
            state: TerminalState::Unknown,
            outcome: CastOutcome::Unknown,
            attempt_serial: None,
            spell_id: None,
            cast_count: None,
            cast_item: None,
            target: None,
            targets: None,
            ended_observed_at_unix_ms: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self.state {
            TerminalState::Unknown => {
                if !matches!(self.outcome, CastOutcome::Unknown) {
                    return Err(Error::InvalidObservation(
                        "a non-present last-cast-end must not carry an outcome",
                    ));
                }
                if self.attempt_serial.is_some()
                    || self.spell_id.is_some()
                    || self.cast_count.is_some()
                    || self.cast_item.is_some()
                    || self.target.is_some()
                    || self.targets.is_some()
                    || self.ended_observed_at_unix_ms.is_some()
                {
                    return Err(Error::InvalidObservation(
                        "a non-present last-cast-end must not carry attempt fields",
                    ));
                }
            }
            TerminalState::Present => {
                if matches!(self.outcome, CastOutcome::Unknown) {
                    return Err(Error::InvalidObservation(
                        "a terminal cast edge requires an outcome",
                    ));
                }
                let spell_id = self
                    .spell_id
                    .ok_or_else(|| Error::InvalidObservation("a terminal cast edge requires a spell id"))?;
                if spell_id == 0 {
                    return Err(Error::InvalidObservation(
                        "a terminal cast edge requires a non-zero spell id",
                    ));
                }
                if let Some(ended) = self.ended_observed_at_unix_ms {
                    if ended < 0 {
                        return Err(Error::InvalidObservation(
                            "terminal cast clock must not be negative",
                        ));
                    }
                }
                if let Some(targets) = &self.targets {
                    targets.validate()?;
                }
            }
        }
        Ok(())
    }
}

/// One self aura slot exactly as the aura wire carried it: raw spell/flags/level/charges/caster
/// and both durations. Helpful/harmful classification does not exist in the payload and is never
/// inferred here; the caster guid of `0` is the wire's own-aura marker (`AFLAG_CASTER`), not the
/// player's guid.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuraObservation {
    pub slot: u8,
    pub spell_id: u32,
    pub flags: u8,
    pub level: u8,
    pub charges: u8,
    pub caster_guid: u64,
    pub max_duration_ms: u32,
    pub duration_ms: u32,
}

impl AuraObservation {
    pub fn validate(&self) -> Result<()> {
        // A zero spell id is the wire's aura-removal row; it is an observed slot, not unknown.
        Ok(())
    }
}

/// The self unit's observed aura slot set.
///
/// `state` separates "not yet observed" from "observed": `known` with an empty slot list is an
/// observed zero-aura state and must never be presented as unknown. `complete` is false only
/// when the export bound cut the producer's slot set.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AurasObservation {
    pub state: AuraKnowledge,
    pub complete: bool,
    #[serde(default)]
    pub slots: Vec<AuraObservation>,
}

impl AurasObservation {
    /// All-unobserved form.
    pub fn unknown() -> Self {
        Self {
            state: AuraKnowledge::Unknown,
            complete: false,
            slots: Vec::new(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.slots.len() > MAX_AURA_SLOTS {
            return Err(Error::InvalidObservation(
                "aura slots exceed the observation bound; mark the export incomplete and bound it before construction",
            ));
        }
        if self.complete && self.state == AuraKnowledge::Unknown {
            return Err(Error::InvalidObservation(
                "an unknown aura state cannot claim completeness",
            ));
        }
        if self.state == AuraKnowledge::Unknown && !self.slots.is_empty() {
            return Err(Error::InvalidObservation(
                "an unobserved aura state must not carry slots",
            ));
        }
        let mut previous: Option<u8> = None;
        for aura in &self.slots {
            aura.validate()?;
            if let Some(previous) = previous {
                if previous >= aura.slot {
                    return Err(Error::InvalidObservation(
                        "aura slots must be sorted ascending by slot",
                    ));
                }
            }
            previous = Some(aura.slot);
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
    /// Self health and current power, each independently observed. The active pair keeps the
    /// flat `power` spelling; all seven pairs carry their own observedness.
    pub resources: ResourceObservation,
    /// Self life state (ghost precedence over zero health).
    pub death: DeathObservation,
    /// The self unit's active cast, unknown before the native feed observes one.
    pub casting: CastObservation,
    /// The self unit's active channel.
    pub channeling: ChannelObservation,
    /// The self unit's last terminal cast edge with its outcome.
    pub last_cast_end: LastCastEndObservation,
    /// The self unit's observed aura slots.
    pub auras: AurasObservation,
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
        casting: CastObservation,
        channeling: ChannelObservation,
        last_cast_end: LastCastEndObservation,
        auras: AurasObservation,
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
            casting,
            channeling,
            last_cast_end,
            auras,
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
        self.casting.validate()?;
        self.channeling.validate()?;
        self.last_cast_end.validate()?;
        self.auras.validate()?;
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
            && self.casting == other.casting
            && self.channeling == other.channeling
            && self.last_cast_end == other.last_cast_end
            && self.auras == other.auras
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
