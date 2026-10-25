//! The session-state producer: owns the replicated [`WorldState`] for one live client session
//! and exports typed self-state snapshots for OpenCode's `benilla_wm_native` v2 bridge
//! (`wm.native.observation.v2`, contract:
//! `D:/Projects/project-b-opencode/docs/opencode/convergence-consumer.md`).
//!
//! Time semantics (the core of the v2 contract):
//! - The driver stamps every reduced packet with its arrival wall-clock
//!   ([`SessionState::reduce`]). The producer remembers the arrival time of the packet that
//!   last changed source-observed state; that remembered time is the snapshot's
//!   `observed_at_unix_ms`.
//! - [`SessionState::export`] takes `exported_at_unix_ms` — the handoff time, supplied by the
//!   driver at export. Export time never renews source freshness; the receiver enforces that.
//! - [`state_revision`][SelfObservation::state_revision] increases only when
//!   observation-relevant state changes: a values block delivering a new value, a movement
//!   block changing position/flags/speeds (a periodic re-heartbeat with an identical
//!   position is a clock tick, not state), a create, or a visibility loss. Unrelated packets
//!   (pong, chat, addon info) advance nothing.
//!
//! Retention and retry: a produced snapshot is retained until the driver reports the bridge
//! accepted it ([`SessionState::mark_published`]). If the publisher reports
//! `PublisherBusy`, the driver calls `export` again later: the retained snapshot is
//! re-exported with a fresh `sequence` and export time but the same `state_revision` and
//! source time — the unchanged re-export the contract explicitly permits. A newer state
//! replaces an unpublished older one (latest-only coalescing, by design).
//!
//! Honesty rules: a value that was never source-observed is never fabricated. Position
//! without a source observation blocks export (`SkipReason::NoPosition`); target state that
//! was never observed exports as [`SelfTarget::Unknown`] rather than suppressing an
//! otherwise-usable snapshot; visibility over the bridge bound exports as explicitly
//! incomplete with the pre-bound total. No WM, socket, serde, or clock dependency lives here.

use anyhow::{Context, Result};

use crate::fields;
use crate::opcode;
use crate::reducer::{ChangeEvent, WorldState};
use crate::updates::PositionSnapshot;

/// The bridge's per-observation visibility bound (v2 `observation.rs`).
pub const MAX_VISIBLE_GUIDS: usize = 256;

/// Why an export produced nothing while the session is otherwise alive. These are the only
/// unavailable-value cases; everything else exports, explicitly marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// No player GUID has been established for this session yet.
    NoPlayer,
    /// Neither a LOGIN_VERIFY_WORLD position nor a replicated self position exists: there is
    /// no source-observed location, and none may be invented.
    NoPosition,
}

/// Observed target state, mapped 1:1 onto the bridge's `TargetObservation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelfTarget {
    /// The producer has not observed target state. Never a synonym for no-target.
    Unknown,
    /// The target field was delivered as zero: observed no target.
    None,
    /// An observed nonzero full GUID.
    Target(u64),
}

/// Replicated visibility, mapped 1:1 onto the bridge's `Visibility`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfVisibility {
    /// True only when `guids` is the complete replicated set.
    pub complete: bool,
    /// The producer's total before the export bound; always >= `guids.len()`.
    pub total_guid_count: u64,
    /// Sorted, unique, excludes the player, at most [`MAX_VISIBLE_GUIDS`] entries.
    pub guids: Vec<u64>,
}

/// A valid self-world snapshot: every populated field is source-observed.
#[derive(Clone, Debug, PartialEq)]
pub struct SelfObservation {
    pub session_epoch: u64,
    pub player_guid: u64,
    /// Export sequence: strictly increasing within the epoch, including unchanged re-exports.
    pub sequence: u64,
    /// Source-change revision: increases only when observed state changes.
    pub state_revision: u64,
    /// Arrival wall-clock of the packet that produced `state_revision` (driver-supplied).
    pub observed_at_unix_ms: i64,
    pub client_uptime_ms: u64,
    pub map_id: u32,
    /// None until an AreaTable-backed source exists; never guessed.
    pub zone_id: Option<u32>,
    /// None until an AreaTable-backed source exists; never guessed.
    pub area_id: Option<u32>,
    pub position: PositionSnapshot,
    /// Arrival wall-clock of the packet that last changed the exported position — diagnostic
    /// provenance beyond what schema v2 carries (proposed for v2.1).
    pub position_observed_at_unix_ms: i64,
    pub target: SelfTarget,
    pub visibility: SelfVisibility,
}

/// Lifecycle signals the driver must act on (publisher retirement per the bridge contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeEvent {
    /// A LOGIN_VERIFY_WORLD map change: replicated state was cleared; the driver must retire
    /// the WM publisher (retire + `spawn_after` ordering); receiver invalidation follows the
    /// publisher's own disconnect handling on the bridge side.
    MapTransition { from: u32, to: u32 },
}

/// One session's replicated world, revision bookkeeping, and export state.
pub struct SessionState {
    epoch: u64,
    player_guid: Option<u64>,
    world: WorldState,
    map_id: Option<u32>,
    /// Authoritative initial/teleport position from LOGIN_VERIFY_WORLD, with the arrival time
    /// of the packet that delivered it.
    verify_position: Option<(PositionSnapshot, i64)>,
    /// Arrival time of the packet that last changed the self position.
    position_observed_at: Option<i64>,
    /// Input packet counter: diagnostics only; advances on every reduced packet.
    input_seq: u64,
    /// Source-change revision; session-scoped, resets with the epoch.
    state_revision: u64,
    /// Arrival time of the packet that produced the current `state_revision`.
    last_state_change_at: Option<i64>,
    /// Export sequence; session-scoped.
    next_sequence: u64,
    /// Revision of the last snapshot the driver reported as handed off.
    handoff_revision: u64,
    /// The retained snapshot: produced, awaiting successful handoff, replaced by newer state.
    unsent: Option<SelfObservation>,
    pending_lifecycle: Vec<LifeEvent>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            player_guid: None,
            world: WorldState::new(),
            map_id: None,
            verify_position: None,
            position_observed_at: None,
            input_seq: 0,
            state_revision: 0,
            last_state_change_at: None,
            next_sequence: 1,
            handoff_revision: 0,
            unsent: None,
            pending_lifecycle: Vec::new(),
        }
    }

    /// The session epoch: constant within a session, bumped by every `start_session`.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Begin or re-begin a session for `player_guid` (full GUID, from the character enum).
    /// Bumps the epoch — the bridge's reset signal — and clears all revision, sequence, and
    /// replicated state. Reducer generations persist, so a recreated GUID keeps its history.
    pub fn start_session(&mut self, player_guid: u64) {
        assert!(player_guid != 0, "player guid must be non-zero");
        self.epoch += 1;
        self.player_guid = Some(player_guid);
        self.world = WorldState::new();
        self.map_id = None;
        self.verify_position = None;
        self.position_observed_at = None;
        self.state_revision = 0;
        self.last_state_change_at = None;
        self.next_sequence = 1;
        self.handoff_revision = 0;
        self.unsent = None;
    }

    /// Logout/disconnect: drop replicated state. The driver must retire the WM publisher;
    /// the next `start_session` opens the new epoch.
    pub fn finish_session(&mut self) {
        self.player_guid = None;
        self.world = WorldState::new();
        self.map_id = None;
        self.verify_position = None;
        self.position_observed_at = None;
        self.unsent = None;
    }

    /// Reduce one world packet, stamping it with the driver's arrival wall-clock. Lifecycle
    /// opcodes (`LOGIN_VERIFY_WORLD`, `SMSG_DESTROY_OBJECT`) are handled here; unrelated
    /// session packets reduce to nothing and advance neither revision nor freshness.
    pub fn reduce(
        &mut self,
        op: u16,
        payload: &[u8],
        observed_at_unix_ms: i64,
    ) -> Result<Vec<ChangeEvent>> {
        let mut events = Vec::new();
        let mut changed = false;
        match op {
            opcode::SMSG_UPDATE_OBJECT | opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
                let (evs, ch) = self.world.reduce_changed(op, payload)?;
                events = evs;
                changed = ch;
            }
            opcode::SMSG_DESTROY_OBJECT => {
                // `Object.cpp:278-286`: full u64 GUID, u8 on-death flag.
                let bytes: [u8; 8] = payload
                    .get(0..8)
                    .context("destroy object payload cut short")?
                    .try_into()
                    .unwrap();
                let guid = u64::from_le_bytes(bytes);
                let _on_death = payload
                    .get(8)
                    .context("destroy object on-death flag cut short")?;
                let (evs, ch) = self.world.reduce_changed(
                    opcode::SMSG_UPDATE_OBJECT,
                    &destroy_as_out_of_range(guid),
                )?;
                events = evs;
                changed = ch;
            }
            opcode::SMSG_LOGIN_VERIFY_WORLD => {
                // `CharacterHandler.cpp:804-810`: u32 map, f32 x y z o.
                let b: [u8; 20] = payload
                    .get(0..20)
                    .context("login-verify payload cut short")?
                    .try_into()
                    .unwrap();
                let map_id = u32::from_le_bytes(b[0..4].try_into().unwrap());
                let position = PositionSnapshot {
                    x: f32::from_le_bytes(b[4..8].try_into().unwrap()),
                    y: f32::from_le_bytes(b[8..12].try_into().unwrap()),
                    z: f32::from_le_bytes(b[12..16].try_into().unwrap()),
                    o: f32::from_le_bytes(b[16..20].try_into().unwrap()),
                };
                if let Some(from) = self.map_id {
                    if from != map_id {
                        // Map transition: the old map's objects leave replicated visibility.
                        // The epoch is kept (bridge contract, publisher-retirement section);
                        // the driver retires the publisher on this event.
                        self.world = WorldState::new();
                        self.pending_lifecycle
                            .push(LifeEvent::MapTransition { from, to: map_id });
                        changed = true;
                    }
                }
                self.map_id = Some(map_id);
                self.verify_position = Some((position, observed_at_unix_ms));
                changed = true;
            }
            _ => {}
        }
        self.input_seq += 1;
        if changed {
            self.state_revision += 1;
            self.last_state_change_at = Some(observed_at_unix_ms);
            let position_changed = op == opcode::SMSG_LOGIN_VERIFY_WORLD
                || events
                    .iter()
                    .any(|e| matches!(e, ChangeEvent::Moved { guid } if Some(*guid) == self.player_guid));
            if position_changed && self.player_guid.is_some() {
                self.position_observed_at = Some(observed_at_unix_ms);
            }
        }
        Ok(events)
    }

    /// Drain lifecycle events the driver must act on (publisher retirement).
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent> {
        std::mem::take(&mut self.pending_lifecycle)
    }

    /// Whether an export is due: a state change is un-handed-off, or a produced snapshot
    /// awaits handoff (the PublisherBusy retry path).
    pub fn export_due(&self) -> bool {
        self.unsent.is_some() || self.state_revision > self.handoff_revision
    }

    /// Export the self snapshot. Returns `Some` when there is something to hand off — a new
    /// source state, or a retained snapshot re-exported after `PublisherBusy` (fresh
    /// `sequence` and export time; unchanged `state_revision` and source time). Returns
    /// `Ok(None)` when the state is unchanged and fully handed off. `Err(SkipReason)` reports
    /// a genuinely unavailable required value; the un-handoff revision is retained, so the
    /// driver may retry a later export when state has moved on.
    pub fn export(
        &mut self,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
    ) -> Result<Option<SelfObservation>> {
        // A newer state always supersedes a retained snapshot (latest-only coalescing).
        if self.state_revision > self.handoff_revision {
            let mut snapshot = self.build_snapshot().map_err(|reason| anyhow::Error::new(reason))?;
            snapshot.sequence = self.next_sequence;
            snapshot.client_uptime_ms = client_uptime_ms;
            self.next_sequence += 1;
            self.unsent = Some(snapshot);
        } else if self.unsent.is_some() {
            // Unchanged re-export of the retained snapshot: same revision, same source time.
            let snapshot = self.unsent.as_mut().expect("checked above");
            snapshot.sequence = self.next_sequence;
            snapshot.client_uptime_ms = client_uptime_ms;
            self.next_sequence += 1;
            return Ok(Some(snapshot.clone()));
        } else {
            return Ok(None);
        }
        Ok(self.unsent.clone())
    }

    /// The driver reports the bridge accepted the last handed-off snapshot.
    pub fn mark_published(&mut self) {
        if let Some(snapshot) = self.unsent.take() {
            self.handoff_revision = snapshot.state_revision;
        }
    }

    /// The retained, not-yet-handed-off snapshot, if any (diagnostics).
    pub fn unsent(&self) -> Option<&SelfObservation> {
        self.unsent.as_ref()
    }

    fn build_snapshot(&self) -> std::result::Result<SelfObservation, SkipReason> {
        let player_guid = self.player_guid.ok_or(SkipReason::NoPlayer)?;
        let map_id = self.map_id.ok_or(SkipReason::NoPosition)?;

        // Position: the replicated self entry's movement block when present, else the
        // authoritative LOGIN_VERIFY_WORLD position. Neither observed => refuse.
        let self_entry = self.world.get(player_guid);
        let (position, position_time) = match self_entry.and_then(|e| e.movement.as_ref()) {
            Some(movement) => match (&movement.position, self.position_observed_at) {
                (Some(p), Some(t)) => (Some(*p), t),
                _ => match self.verify_position {
                    Some((p, t)) => (Some(p), t),
                    None => (None, 0),
                },
            },
            None => match self.verify_position {
                Some((p, t)) => (Some(p), t),
                None => (None, 0),
            },
        };
        let position = position.ok_or(SkipReason::NoPosition)?;
        let position_observed_at_unix_ms = position_time;

        // Target: delivered zero = observed no-target; delivered nonzero = observed target;
        // undelivered or no self entry = unknown. All three are exportable in v2.
        let target = match self_entry {
            Some(entry) if entry.field_delivered(fields::UNIT_FIELD_TARGET) => {
                match entry.self_guid(fields::UNIT_FIELD_TARGET) {
                    Some(0) | None => SelfTarget::None,
                    Some(guid) => SelfTarget::Target(guid),
                }
            }
            _ => SelfTarget::Unknown,
        };

        // Replicated visibility: every currently-present GUID except the player, sorted
        // (BTreeMap iteration is sorted — deterministic), deduplicated by construction.
        let all: Vec<u64> = self
            .world
            .iter()
            .map(|(guid, _)| *guid)
            .filter(|guid| *guid != player_guid)
            .collect();
        let total = all.len();
        let complete = total <= MAX_VISIBLE_GUIDS;
        let guids = if complete { all } else { all[..MAX_VISIBLE_GUIDS].to_vec() };

        Ok(SelfObservation {
            session_epoch: self.epoch,
            player_guid,
            sequence: 0, // assigned by export()
            state_revision: self.state_revision,
            observed_at_unix_ms: self.last_state_change_at.unwrap_or(0),
            client_uptime_ms: 0, // assigned by export()
            map_id,
            zone_id: None,
            area_id: None,
            position,
            position_observed_at_unix_ms,
            target,
            visibility: SelfVisibility {
                complete,
                total_guid_count: total as u64,
                guids,
            },
        })
    }
}

/// `SMSG_DESTROY_OBJECT` expressed as the equivalent out-of-range update block, so removal
/// goes through `WorldState`'s own (tombstone-bumping) path.
fn destroy_as_out_of_range(guid: u64) -> Vec<u8> {
    let mut p = 1u32.to_le_bytes().to_vec(); // one block
    p.push(4); // UPDATETYPE_OUT_OF_RANGE_OBJECTS
    p.extend_from_slice(&1u32.to_le_bytes());
    let mut mask = 0u8;
    let mut bytes = Vec::new();
    for i in 0..8 {
        let b = (guid >> (i * 8)) as u8;
        if b != 0 {
            mask |= 1 << i;
            bytes.push(b);
        }
    }
    p.push(mask);
    p.extend_from_slice(&bytes);
    p
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for SkipReason {}

impl SkipReason {
    pub fn message(&self) -> String {
        match self {
            SkipReason::NoPlayer => "no player guid established for this session".into(),
            SkipReason::NoPosition => "no source-observed self position yet".into(),
        }
    }
}
