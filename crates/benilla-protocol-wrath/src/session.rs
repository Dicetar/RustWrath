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
use crate::updates::{Reader, PositionSnapshot};

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

/// Observed combat state from `UNIT_FIELD_FLAGS`; unknown until that word is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfCombat {
    Unknown,
    OutOfCombat,
    InCombat,
}

/// Self health and power, each field independently observed. Beyond the active `power` spelling,
/// all seven client power pairs and their maxima are carried for the v2.3 observation contract;
/// each is `Some` only when the wire delivered that word (`Some(0)` is an observed zero).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelfResources {
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    pub power_type: Option<u8>,
    pub power: Option<u32>,
    /// `UNIT_FIELD_POWER1..7`, indexed by power type.
    pub powers: [Option<u32>; 7],
    /// `UNIT_FIELD_MAXPOWER1..7`, indexed by power type.
    pub max_powers: [Option<u32>; 7],
}

/// Self life state; ghost takes precedence over dead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfDeath {
    Unknown,
    Alive,
    Dead,
    Ghost,
}

/// A valid self-world snapshot: every populated field is source-observed.
#[derive(Clone, Debug, PartialEq)]
pub struct SelfObservation {
    pub session_epoch: u64,
    /// World scope identity: increments per far transfer (Astra R2). Producer-side until the
    /// wire contract grows it (Phase B proposal) — never collapsed into map_id.
    pub world_generation: u64,
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
    /// UNIT_FIELD_FLAGS combat bit; unknown before that word is delivered.
    pub combat: SelfCombat,
    /// Self health and power, each field independently observed.
    pub resources: SelfResources,
    /// Self life state (ghost before dead).
    pub death: SelfDeath,
    pub visibility: SelfVisibility,
}

/// The session lifecycle, expressed in real protocol events (Astra R2). This lives in the
/// session producer — the existing lifecycle owner — not in a parallel manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// After `start_session`, before the driver sends `CMSG_PLAYER_LOGIN`.
    CharacterSelection,
    /// `CMSG_PLAYER_LOGIN` sent (`note_player_login`), awaiting the login burst.
    EnteringWorld,
    /// `LOGIN_VERIFY_WORLD` received: the login world is established.
    InWorld,
    /// `SMSG_TRANSFER_PENDING` received (`Player.cpp:1593`): a far teleport was announced.
    /// The player is still in the old world; old-world packets keep reducing.
    TransferPending,
    /// `SMSG_NEW_WORLD` received (`Player.cpp:1613`): the old world's replicated state has
    /// been discarded and the new world scope has begun. Update/destroy traffic received in
    /// this phase is the old world's in-flight tail and is discarded. The rationale is the
    /// server's processing order, not TCP alone: the server acts on the worldport
    /// acknowledgment (`MovementHandler.cpp:47`) before any new-world visibility traffic is
    /// sent, and stops old-world traffic for this player at removal from the old map in the
    /// same handler. Updates arriving in this window are therefore sent before the server saw
    /// the acknowledgment; any straggler after it matches the stock client's own
    /// apply-after-teardown behavior and self-heals via out-of-range/destroy.
    Loading,
    // The driver sent `MSG_MOVE_WORLDPORT_ACK` (`worldport_acknowledged`): new-world
    // traffic reduces normally. `LOGIN_VERIFY_WORLD` is not part of the transfer path
    // (it is login-only — `CharacterHandler.cpp:806`), so this transition is driven by the
    // acknowledgment the client itself sends: the `InWorld` variant above is re-entered.
}

/// Lifecycle signals the driver must act on (publisher retirement per the bridge contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeEvent {
    /// A new world scope began (`SMSG_NEW_WORLD`, or a defensive LOGIN_VERIFY_WORLD map
    /// mismatch). Replicated state was cleared; the driver must retire the WM publisher via
    /// `retire` + `spawn_after` ordering before publishing new-scope observations.
    WorldTransfer { world_generation: u64, map_id: u32 },
}

/// One session's replicated world, revision bookkeeping, and export state.
pub struct SessionState {
    epoch: u64,
    phase: Phase,
    /// World scope identity (Astra R2): increments at session start and at every
    /// `SMSG_NEW_WORLD`, including same-map instance transfers. Map ID alone cannot scope a
    /// world — two dungeons can share a map id, and the far-teleport path reuses it.
    world_generation: u64,
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
    /// Arrival time of the last reduced server packet (driver-supplied; health provenance).
    last_server_packet_at: Option<i64>,
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
    /// Order counters from `MSG_MOVE_TELEPORT_ACK` (server→client) awaiting the client's
    /// acknowledgment packet (`Player.cpp:1375`: the counter increments per teleport).
    pending_teleport_acks: Vec<u32>,
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
            phase: Phase::CharacterSelection,
            world_generation: 0,
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
            pending_teleport_acks: Vec::new(),
            last_server_packet_at: None,
        }
    }

    /// The session epoch: constant within a session, bumped by every `start_session`.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Begin or re-begin a session for `player_guid` (full GUID, from the character enum).
    /// Bumps the epoch — the bridge's reset signal — and clears all revision, sequence, and
    /// replicated state. A new world scope opens with the epoch (world generation 1).
    pub fn start_session(&mut self, player_guid: u64) {
        assert!(player_guid != 0, "player guid must be non-zero");
        self.epoch += 1;
        self.phase = Phase::CharacterSelection;
        self.world_generation += 1;
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
        self.pending_teleport_acks.clear();
    }

    /// The current lifecycle phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Whether a world scope is established (the bridge's InWorld vs Disconnected
    /// distinction); the driver's adapter maps phases to its own health enum.
    pub fn world_established(&self) -> bool {
        self.map_id.is_some()
    }

    /// Arrival time of the last reduced server packet (driver-supplied at reduce time).
    pub fn last_server_packet_at_unix_ms(&self) -> Option<i64> {
        self.last_server_packet_at
    }

    /// The current world scope generation. Same-map instance transfers increment it: map id
    /// alone is not world identity.
    pub fn world_generation(&self) -> u64 {
        self.world_generation
    }

    /// The driver sent `CMSG_PLAYER_LOGIN`: the login burst (creates) may now arrive.
    pub fn note_player_login(&mut self) {
        if self.phase == Phase::CharacterSelection {
            self.phase = Phase::EnteringWorld;
        }
    }

    /// The driver sent `MSG_MOVE_WORLDPORT_ACK` after its own loading teardown: the transfer
    /// boundary is exact from here (TCP ordering) — new-world traffic reduces normally.
    pub fn worldport_acknowledged(&mut self) {
        if self.phase == Phase::Loading {
            self.phase = Phase::InWorld;
        }
    }

    /// Logout/disconnect: drop replicated state. The driver must retire the WM publisher;
    /// the next `start_session` opens the new epoch.
    pub fn finish_session(&mut self) {
        self.phase = Phase::CharacterSelection;
        self.player_guid = None;
        self.last_server_packet_at = None;
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
        // The loading window is the old world's in-flight tail: this client has received
        // NEW_WORLD (old scope discarded) and has not yet sent its worldport acknowledgment,
        // so every update/destroy in between belongs to the old scope and must not populate
        // the new world (Astra R2 late-packet rule).
        if self.phase == Phase::Loading
            && matches!(
                op,
                opcode::SMSG_UPDATE_OBJECT
                    | opcode::SMSG_COMPRESSED_UPDATE_OBJECT
                    | opcode::SMSG_DESTROY_OBJECT
                    | opcode::SMSG_AURA_UPDATE
                    | opcode::SMSG_AURA_UPDATE_ALL
            )
        {
            self.input_seq += 1;
            return Ok(Vec::new());
        }
        match op {
            opcode::SMSG_UPDATE_OBJECT | opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
                let (evs, ch) = self.world.reduce_changed(op, payload)?;
                events = evs;
                changed = ch;
            }
            opcode::SMSG_AURA_UPDATE | opcode::SMSG_AURA_UPDATE_ALL => {
                // 12340 sends auras as packets rather than descriptor fields
                // (`SpellAuras.cpp:187-235`); they ride the same replicated-state revision as
                // the update stream so the app's digest republishes them.
                let (evs, ch) = self.world.reduce_changed(op, payload)?;
                events = evs;
                changed = ch;
            }
            opcode::SMSG_TRANSFER_PENDING => {
                // `Player.cpp:1593`: u32 mapid, optionally u32 transport entry + u32 map.
                // Diagnostics only: the authoritative destination arrives with NEW_WORLD.
                // The player is still in the old world, so traffic keeps reducing.
                let map = u32::from_le_bytes(
                    payload
                        .get(0..4)
                        .context("transfer-pending payload cut short")?
                        .try_into()
                        .unwrap(),
                );
                if self.phase == Phase::InWorld || self.phase == Phase::EnteringWorld {
                    self.phase = Phase::TransferPending;
                }
                let _ = map;
            }
            opcode::SMSG_NEW_WORLD => {
                // `Player.cpp:1613`: u32 mapid + f32 x/y/z/o destination. This begins the
                // new world scope: the old world's replicated state is discarded here, the
                // destination position becomes the authoritative position source, and the
                // driver retires the WM publisher (lifecycle event below). Same-map
                // instance transfers increment the world generation exactly like
                // cross-map ones — map id alone is not world identity.
                let b: [u8; 20] = payload
                    .get(0..20)
                    .context("new-world payload cut short")?
                    .try_into()
                    .unwrap();
                let map_id = u32::from_le_bytes(b[0..4].try_into().unwrap());
                let position = PositionSnapshot {
                    x: f32::from_le_bytes(b[4..8].try_into().unwrap()),
                    y: f32::from_le_bytes(b[8..12].try_into().unwrap()),
                    z: f32::from_le_bytes(b[12..16].try_into().unwrap()),
                    o: f32::from_le_bytes(b[16..20].try_into().unwrap()),
                };
                self.world = WorldState::new();
                self.world_generation += 1;
                self.map_id = Some(map_id);
                self.verify_position = Some((position, observed_at_unix_ms));
                self.position_observed_at = Some(observed_at_unix_ms);
                if self.phase == Phase::TransferPending || self.phase == Phase::InWorld {
                    self.phase = Phase::Loading;
                }
                self.pending_lifecycle.push(LifeEvent::WorldTransfer {
                    world_generation: self.world_generation,
                    map_id,
                });
                changed = true;
            }
            opcode::MSG_MOVE_TELEPORT_ACK => {
                // The controlled player's near-teleport correction (`Player.cpp:1375`):
                // packed GUID + u32 order counter + standalone movement payload. This is the
                // authoritative position for the teleport; the driver owes the server an
                // acknowledgment packet (packed GUID + u32 flags + u32 time, both read and
                // unused by `MovementHandler.cpp:277-282`).
                let mut r = Reader::new(payload);
                let guid = r.packed_guid().context("teleport-ack guid")?;
                let counter = r.u32().context("teleport-ack counter")?;
                let snapshot = crate::updates::standalone_movement_block(&mut r)
                    .context("teleport-ack movement block")?;
                changed |= self.world.apply_movement(guid, snapshot);
                events.push(ChangeEvent::Moved { guid });
                if Some(guid) == self.player_guid {
                    self.pending_teleport_acks.push(counter);
                }
            }
            opcode::MSG_MOVE_TELEPORT => {
                // Observer near-teleport broadcast (`Unit.cpp:16198`,
                // `SendMessageToSet(&data2, false)` — never sent to the mover): packed GUID +
                // standalone movement payload (no update-flags prefix). Same world scope — a
                // position update, not a transfer. The controlled player instead receives
                // MSG_MOVE_TELEPORT_ACK above.
                let mut r = Reader::new(payload);
                let guid = r.packed_guid().context("teleport guid")?;
                let snapshot = crate::updates::standalone_movement_block(&mut r)
                    .context("teleport movement block")?;
                changed |= self.world.apply_movement(guid, snapshot);
                events.push(ChangeEvent::Moved { guid });
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
                // LOGIN_VERIFY_WORLD is the login world establishment, not the transfer
                // mechanism (Astra R2: transfers use TRANSFER_PENDING/NEW_WORLD and this
                // message is login-only). A map mismatch here is therefore a protocol
                // anomaly; treat it defensively as a world scope change.
                if let Some(from) = self.map_id {
                    if from != map_id {
                        self.world = WorldState::new();
                        self.world_generation += 1;
                        self.pending_lifecycle.push(LifeEvent::WorldTransfer {
                            world_generation: self.world_generation,
                            map_id,
                        });
                        changed = true;
                    }
                }
                if self.phase == Phase::EnteringWorld {
                    self.phase = Phase::InWorld;
                }
                self.map_id = Some(map_id);
                self.verify_position = Some((position, observed_at_unix_ms));
                changed = true;
            }
            _ => {}
        }
        self.input_seq += 1;
        self.last_server_packet_at = Some(observed_at_unix_ms);
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

    /// Drain order counters owed a `MSG_MOVE_TELEPORT_ACK` acknowledgment packet (client to
    /// server: packed player GUID, u32 flags, u32 time — both read and unused by the server).
    pub fn take_pending_teleport_acks(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_teleport_acks)
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

    /// Read-only replicated world access for the application feed (graphical Net-stage
    /// translation). The producer owns mutation; the app only reads.
    pub fn world(&self) -> &WorldState {
        &self.world
    }

    /// The authoritative map id, once a world is established.
    pub fn map_id(&self) -> Option<u32> {
        self.map_id
    }

    /// The session player's full GUID, once the character is selected.
    pub fn player_guid(&self) -> Option<u64> {
        self.player_guid
    }

    /// The current source-change revision.
    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// The current presentation snapshot, built from authoritative session state rather than the
    /// export/ack/retry slot. `sequence` and `client_uptime_ms` are zero because this is a read
    /// model, not an export; every source-observed field is the same one an export would carry.
    /// `None` before a player and world position exist or outside an eligible phase.
    pub fn current_self_snapshot(&self) -> Option<SelfObservation> {
        self.build_snapshot().ok()
    }

    /// The most recent successful export (diagnostics only; presentation reads
    /// [`Self::current_self_snapshot`]).
    pub fn last_self_observation(&self) -> Option<&SelfObservation> {
        self.unsent.as_ref()
    }

    /// The session phase.
    
    fn build_snapshot(&self) -> std::result::Result<SelfObservation, SkipReason> {
        let player_guid = self.player_guid.ok_or(SkipReason::NoPlayer)?;
        let map_id = self.map_id.ok_or(SkipReason::NoPosition)?;
        // Observations export the current world scope. A snapshot can only be produced in
        // InWorld (established world) or TransferPending (old world still live, still the
        // player's real location); a Loading-phase snapshot would mix scopes and is never
        // built — the driver retires the publisher instead.
        match self.phase {
            Phase::InWorld | Phase::TransferPending => {}
            _ => return Err(SkipReason::NoPosition),
        }

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

        // Target (Astra R1, atomic multiword knowledge): the target is a LONG field — two
        // u32 words on the wire, and the server may transmit them independently. Knowledge
        // requires BOTH words delivered in this generation; a delivered low word with an
        // undelivered high word is partial knowledge and exports as `Unknown` rather than
        // inventing no-target from an unavailable combined value. Once both are known, a
        // combined zero is an observed no-target and a nonzero combined GUID is an observed
        // target. All three states are exportable in v2.
        let target = match self_entry {
            Some(entry) => {
                let low_known = entry.field_delivered(fields::UNIT_FIELD_TARGET);
                let high_known = entry.field_delivered(fields::UNIT_FIELD_TARGET + 1);
                if low_known && high_known {
                    match entry.self_guid(fields::UNIT_FIELD_TARGET) {
                        Some(0) | None => SelfTarget::None,
                        Some(guid) => SelfTarget::Target(guid),
                    }
                } else {
                    SelfTarget::Unknown
                }
            }
            None => SelfTarget::Unknown,
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

        let observed = |index: u16| -> Option<u32> {
            let entry = self_entry?;
            entry.field_delivered(index).then(|| entry.field(index)).flatten()
        };
        let health = observed(fields::UNIT_FIELD_HEALTH);
        let max_health = observed(fields::UNIT_FIELD_MAXHEALTH);
        let power_type = observed(fields::UNIT_FIELD_BYTES_0)
            .map(|bytes| ((bytes >> 24) & 0xff) as u8)
            .filter(|power_type| *power_type <= 6);
        let power = power_type.and_then(|power_type| {
            observed(fields::UNIT_FIELD_POWER1 + u16::from(power_type))
        });
        let mut powers = [None; 7];
        let mut max_powers = [None; 7];
        for index in 0..7u16 {
            powers[usize::from(index)] = observed(fields::UNIT_FIELD_POWER1 + index);
            max_powers[usize::from(index)] = observed(fields::UNIT_FIELD_MAXPOWER1 + index);
        }
        let resources = SelfResources {
            health,
            max_health,
            power_type,
            power,
            powers,
            max_powers,
        };
        let combat = match observed(fields::UNIT_FIELD_FLAGS) {
            Some(flags) if flags & 0x0008_0000 != 0 => SelfCombat::InCombat,
            Some(_) => SelfCombat::OutOfCombat,
            None => SelfCombat::Unknown,
        };
        let death = match observed(fields::PLAYER_FLAGS) {
            Some(flags) if flags & 0x10 != 0 => SelfDeath::Ghost,
            _ => match health {
                Some(0) => SelfDeath::Dead,
                Some(_) => SelfDeath::Alive,
                None => SelfDeath::Unknown,
            },
        };

        Ok(SelfObservation {
            session_epoch: self.epoch,
            world_generation: self.world_generation,
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
            combat,
            resources,
            death,
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
