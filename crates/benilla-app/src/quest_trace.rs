//! Host-owned interaction and client diagnostics, including worker-thread delivery results.
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use bevy::prelude::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
#[cfg(feature = "dev")]
use std::sync::atomic::{AtomicBool, AtomicU8};

use crate::net::CommandDelivery;

/// Maximum open interactions tracked concurrently before recording tracking loss.
pub(crate) const MAX_OPEN_INTERACTIONS: usize = 64;
/// Maximum run-local NPC aliases retained in the alias ring.
const MAX_NPC_ALIASES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Stage {
    Intent,
    GuardRefused,
    NoWorld,
    AlreadyPending,
    TrackingCapacity,
    ChannelDisconnected,
    ChannelAdmitted,
    Encoded,
    OpenCommandDequeued,
    OpenOutboxAdmitted,
    OpenOutboxRefused,
    GreetingPanel,
    DetailPanel,
    ProgressPanel,
    RewardPanel,
    QuestSlotAdded,
    QuestSlotProgressed { counters: [u16; 4], state: u32 },
    QuestSlotRemoved,
    JournalProjection,
    JournalRowInModel,
    JournalTemplatePending,
    JournalTemplateRefused,
    QuestCompletePacket { xp: u32, money: u32, reward_items: u32 },
    PlayerXpChanged { delta: u32 },
    PlayerMoneyChanged { delta: i64 },
    RewardItemPushed { item_id: u32, count: u32 },
    ServerQuestInvalid { reason: u32 },
    ServerQuestLogFull,
    ServerQuestFailed { reason: u32 },
    ScriptError { vm_session: u64, row_seq: u64, occurrences: u32 },
    ScriptLoadError { vm_session: u64, row_seq: u64, occurrences: u32 },
    ScriptWarning { vm_session: u64, row_seq: u64, occurrences: u32 },
    UiReload,
    ScriptRowsLost { vm_session: u64, count: u64 },
    ScriptRowsEvicted { vm_session: u64, count: u64 },
    SessionPhase { phase: SessionPhase },
    SessionEnded { failed: bool },
    WorldTransfer,
    ReplicaRevision { revision: u64 },
    NativeEventsLost { count: u64 },
    Cast { spell_id: Option<u32>, attempt: Option<u64>, edge: CastEdge },
    InventoryFailure { reason: u8 },
    AudioMixer {
        present: bool,
        voices: u32,
        denied: u64,
        stolen: u64,
        copies_dropped: u64,
    },
    AudioHealth {
        over_full_scale_samples: u64,
        nonfinite_samples: u64,
        voices_refused: u64,
        cycles: u64,
        underruns: u64,
        underrun_ms: u64,
        overloads: u64,
        last_overload_ago_ms: u64,
        lead_min_ms: Option<i64>,
        ring_min_ms: Option<i64>,
        io_wall_max_ms: u64,
        render_wall_max_ms: u64,
        cycle_ms: u64,
        chunk_ms: u64,
        peak_load_percent: u32,
    },
    MovementCorrectionApplied {
        distance_bucket: u8,
        movement_seq: Option<u64>,
        ambiguous: bool,
    },
    MovementPressure {
        baseline: bool,
        admitted: u64,
        coalesced: u64,
        refused: u64,
        written: u64,
        queue_depth: u64,
        oldest_age_ms: u64,
    },
    SceneSummary { world_cameras: u32, animated_rigs: u32 },
    FrameWindow { frames: u32, mean_us: u64, max_us: u64 },
    AssetFailure { family: AssetFamily, cause: AssetFailure },
    AssetEventsLost { count: u64 },
    AssetWindow { failures: u64, missed: u64, window_ms: u64 },
    PacketReceived { opcode: u16 },
    ReducerAccepted { opcode: u16 },
    ReducerRejected { opcode: u16 },
    GameplayDecoded { opcode: u16 },
    GameplayRejected { opcode: u16 },
    SocketReadFailed,
    SocketWriteFailed,
    Outbound { opcode: u32, state: OutboundState },
    UiFrame { frame: UiFrame, visible: Option<bool>, vm_session: u64 },
    UiPress {
        button: UiButton,
        target: UiTarget,
        consumed: bool,
        egui_pointer_over_last_sample: Option<bool>,
        vm_session: u64,
    },
    UiClick {
        button: UiButton,
        target: UiTarget,
        consumed: bool,
        visible: Option<bool>,
        clipped: bool,
        handler_invoked: bool,
        egui_pointer_over_last_sample: Option<bool>,
        vm_session: u64,
    },
    DomainEvent { event: DomainEvent },
    #[cfg(feature = "hotpatch")]
    Hotpatch { event: PatchEvent },
    Delivery(CommandDelivery),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AssetFamily {
    Model,
    World,
    Texture,
    Shader,
    Audio,
    Ui,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AssetFailure {
    Reader,
    LoaderMissing,
    LoaderError,
    LoaderPanic,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum OutboundState {
    Queued,
    Refused,
    Written,
    WriteFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum UiFrame {
    Questgiver,
    QuestLog,
    Merchant,
    Loot,
    SpellBook,
    Character,
    GameMenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum UiButton {
    Left,
    Right,
    Middle,
    Forward,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum UiTarget {
    Quest,
    QuestLog,
    Merchant,
    Loot,
    SpellBook,
    Character,
    World,
    Other,
    NoNamedFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum DomainEvent {
    LoginProgress,
    RealmList,
    WorldAuthenticated,
    Roster,
    RosterFailed,
    LoginQueued,
    LoginFailed,
    CharacterAction,
    CharacterLoginFailed,
    QuestComplete,
    QuestObjective,
    GossipMenu,
    VendorList,
    VendorBuy,
    VendorBuyFailed,
    VendorSellFailed,
    LootOpened,
    LootError,
    LootRemoved,
    LootMoney,
    LootReleased,
    ItemReceived,
    InventoryFailure,
    PlayerConnected,
    PlayerDisconnected,
    Worldport,
}

#[cfg(feature = "hotpatch")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum HotpatchReason {
    Publication,
    Evaluation,
    Retention,
    Frame,
    Unknown,
}

#[cfg(feature = "hotpatch")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum PatchEvent {
    CandidateStaged { attempt: u64 },
    PublicationRejected { attempt: Option<u64>, reason: HotpatchReason },
    CandidateRejected { attempt: Option<u64>, reason: HotpatchReason },
    Activated { attempt: Option<u64>, digest: Option<[u8; 20]> },
    FrameFailed { attempt: Option<u64>, reason: HotpatchReason },
    RetentionFailed { attempt: Option<u64>, reason: HotpatchReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum SessionPhase {
    CharacterSelection,
    EnteringWorld,
    InWorld,
    TransferPending,
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum CastEdge {
    /// A client-local spell request entered the cast ladder.
    Requested,
    /// The cast ladder refused the request locally before any packet.
    LocalRefusal { reason: u8 },
    /// The request passed local guards and reached the Wrath command producer.
    Admitted,
    /// A local cancel (move/esc) requested cancellation of an in-flight cast/channel.
    CancelRequested,
    Started,
    Delayed,
    Launched,
    Failed { reason: u8 },
    Interrupted { reason: u8 },
    ChannelStarted,
    ChannelUpdated,
    ChannelEnded,
    ScopeInvalidated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Operation {
    Open,
    Select,
    Accept,
    RequestReward,
    ChooseReward,
    Observed,
    Loot,
    Equip,
    Buy,
    Sell,
}

#[derive(Debug)]
struct Ingress {
    events: Sender<Event>,
    dropped: AtomicU64,
    next_id: AtomicU64,
    reload_generation: AtomicU64,
    update_packets_seen: AtomicU64,
    update_packets_sampled_out: AtomicU64,
    movement_sequence: AtomicU64,
    movement_admitted: AtomicU64,
    movement_coalesced: AtomicU64,
    movement_refused: AtomicU64,
    movement_written: AtomicU64,
    movement_last_written_seq: AtomicU64,
    movement_writes_since_correction: AtomicU64,
    npc_aliases: Mutex<(VecDeque<(u64, u32)>, u32)>,
    origin: Instant,
    #[cfg(feature = "dev")]
    run_id: String,
    #[cfg(feature = "dev")]
    asset_profile: Option<String>,
    #[cfg(feature = "dev")]
    exporting: AtomicBool,
    #[cfg(feature = "dev")]
    export_status: AtomicU8,
}

#[derive(Resource, Clone, Debug)]
pub(crate) struct Source(Arc<Ingress>);

#[derive(Resource, Default)]
pub(crate) struct ScriptCursor {
    generation: u64,
    vm_session: u64,
    highest_seq: u64,
    counts: std::collections::BTreeMap<u64, u32>,
    frames: [Option<Option<bool>>; 7],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Subject {
    interaction: u64,
    scope: Option<(u64, u64)>,
    quest: Option<u32>,
    npc_alias: Option<u32>,
    operation: Operation,
    reload_generation: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Interaction {
    source: Source,
    subject: Subject,
}

/// Snapshot header version validation error (D01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnapshotVersionError {
    MalformedJson,
    UnsupportedSchema(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for SnapshotVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedJson => write!(f, "snapshot header is not valid JSON"),
            Self::UnsupportedSchema(schema) => {
                write!(f, "unsupported snapshot schema '{schema}'")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported snapshot version {version}")
            }
        }
    }
}

impl std::error::Error for SnapshotVersionError {}

#[derive(Deserialize)]
struct SnapshotProbe {
    schema: String,
    #[serde(default = "default_snapshot_version")]
    version: u32,
}

fn default_snapshot_version() -> u32 {
    1
}

/// Validate the first JSONL header line of an exported snapshot before decoding any event records.
#[allow(dead_code)]
pub(crate) fn validate_snapshot_header(line: &str) -> Result<u32, SnapshotVersionError> {
    let probe: SnapshotProbe =
        serde_json::from_str(line).map_err(|_| SnapshotVersionError::MalformedJson)?;
    if probe.schema != "rustwrath.interaction-diagnostics.v1" {
        return Err(SnapshotVersionError::UnsupportedSchema(probe.schema));
    }
    if probe.version != 1 {
        return Err(SnapshotVersionError::UnsupportedVersion(probe.version));
    }
    Ok(probe.version)
}

/// One row of the additive per-vector evidence summary written into every snapshot header.
///
/// The summary is derived from the same bounded `entries` ring that supplies the exported
/// event rows, so a reader can correlate `retained_observations` and
/// `latest_retained_sequence` with the rows that follow the header.  It deliberately does
/// not turn "zero retained observations" into a healthy reading: absence of an event is
/// never proof that the corresponding condition did not occur.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct DiagnosticVectorEvidence {
    pub vector: String,
    pub instrumentation_available: bool,
    pub retained_observations: u64,
    pub latest_retained_sequence: Option<u64>,
    pub limiting_factors: Vec<String>,
    pub absence_not_proof: bool,
}

/// Map one retained diagnostic event to the exported diagnostic vector that owns it.
///
/// This is the single classification site used by both the in-memory summary and the
/// JSONL export, so an event can never be counted under two different vectors.
pub(crate) fn vector_for_stage(stage: &Stage) -> Option<&'static str> {
    match stage {
        Stage::Intent
        | Stage::GuardRefused
        | Stage::NoWorld
        | Stage::AlreadyPending
        | Stage::TrackingCapacity
        | Stage::ChannelDisconnected
        | Stage::ChannelAdmitted
        | Stage::Encoded
        | Stage::OpenCommandDequeued
        | Stage::OpenOutboxAdmitted
        | Stage::OpenOutboxRefused
        | Stage::QuestSlotAdded
        | Stage::QuestSlotProgressed { .. }
        | Stage::QuestSlotRemoved
        | Stage::JournalProjection
        | Stage::JournalRowInModel
        | Stage::JournalTemplatePending
        | Stage::JournalTemplateRefused
        | Stage::QuestCompletePacket { .. }
        | Stage::PlayerXpChanged { .. }
        | Stage::PlayerMoneyChanged { .. }
        | Stage::RewardItemPushed { .. }
        | Stage::ServerQuestInvalid { .. }
        | Stage::ServerQuestLogFull
        | Stage::ServerQuestFailed { .. }
        | Stage::Delivery(_) => Some("quest_interactions"),
        Stage::GreetingPanel
        | Stage::DetailPanel
        | Stage::ProgressPanel
        | Stage::RewardPanel
        | Stage::DomainEvent {
            event: DomainEvent::GossipMenu,
        } => Some("quest_panel_correlation"),
        Stage::UiFrame { .. } => Some("ui_frame_state"),
        Stage::UiPress { .. } | Stage::UiClick { .. } => Some("ui_pointer_categories"),
        Stage::ScriptError { .. }
        | Stage::ScriptLoadError { .. }
        | Stage::ScriptWarning { .. }
        | Stage::UiReload
        | Stage::ScriptRowsLost { .. }
        | Stage::ScriptRowsEvicted { .. } => Some("lua_diagnostic_counts"),
        Stage::SessionPhase { .. }
        | Stage::SessionEnded { .. }
        | Stage::WorldTransfer
        | Stage::ReplicaRevision { .. }
        | Stage::NativeEventsLost { .. }
        | Stage::DomainEvent {
            event: DomainEvent::LoginProgress
                | DomainEvent::RealmList
                | DomainEvent::WorldAuthenticated
                | DomainEvent::Roster
                | DomainEvent::RosterFailed
                | DomainEvent::LoginQueued
                | DomainEvent::LoginFailed
                | DomainEvent::CharacterAction
                | DomainEvent::CharacterLoginFailed
                | DomainEvent::PlayerConnected
                | DomainEvent::PlayerDisconnected
                | DomainEvent::Worldport,
        } => Some("session_lifecycle"),
        Stage::PacketReceived { .. }
        | Stage::ReducerAccepted { .. }
        | Stage::ReducerRejected { .. }
        | Stage::GameplayDecoded { .. }
        | Stage::GameplayRejected { .. }
        | Stage::SocketReadFailed
        | Stage::SocketWriteFailed
        | Stage::Outbound { .. } => Some("allowlisted_packets"),
        Stage::Cast { .. } => Some("casts"),
        Stage::InventoryFailure { .. }
        | Stage::DomainEvent {
            event: DomainEvent::InventoryFailure | DomainEvent::ItemReceived,
        } => Some("inventory_failures"),
        Stage::DomainEvent {
            event: DomainEvent::VendorList
                | DomainEvent::VendorBuy
                | DomainEvent::VendorBuyFailed
                | DomainEvent::VendorSellFailed
                | DomainEvent::LootOpened
                | DomainEvent::LootError
                | DomainEvent::LootRemoved
                | DomainEvent::LootMoney
                | DomainEvent::LootReleased,
        } => Some("loot_and_vendor_events"),
        Stage::AudioMixer { .. } | Stage::AudioHealth { .. } => Some("audio_mixer_state"),
        Stage::MovementCorrectionApplied { .. } | Stage::MovementPressure { .. } => {
            Some("movement_queue_and_corrections")
        }
        Stage::SceneSummary { .. } => Some("scene_camera_and_rig_counts"),
        Stage::FrameWindow { .. } => Some("frame_intervals"),
        Stage::AssetFailure { .. } | Stage::AssetEventsLost { .. } | Stage::AssetWindow { .. } => {
            Some("asset_load_failure_categories")
        }
        #[cfg(feature = "hotpatch")]
        Stage::Hotpatch { .. } => Some("hotpatch"),
        Stage::DomainEvent {
            event: DomainEvent::QuestComplete | DomainEvent::QuestObjective,
        } => Some("quest_interactions"),
    }
}

pub(crate) fn is_source_specific_loss_stage(stage: &Stage) -> bool {
    matches!(
        stage,
        Stage::NativeEventsLost { .. }
            | Stage::ScriptRowsLost { .. }
            | Stage::ScriptRowsEvicted { .. }
            | Stage::AssetEventsLost { .. }
    )
}

impl Source {
    /// Map a raw 64-bit NPC GUID to a run-local numeric alias so raw GUIDs never enter the
    /// diagnostic ring or exported reports. Zero returns `None`.
    pub(crate) fn npc_alias(&self, guid: u64) -> Option<u32> {
        if guid == 0 {
            return None;
        }
        let mut guard = self
            .0
            .npc_aliases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&(_, alias)) = guard.0.iter().find(|&&(g, _)| g == guid) {
            return Some(alias);
        }
        guard.1 = guard.1.saturating_add(1);
        let alias = guard.1;
        if guard.0.len() >= MAX_NPC_ALIASES {
            guard.0.pop_front();
        }
        guard.0.push_back((guid, alias));
        Some(alias)
    }

    pub(crate) fn npc_alias_count(&self) -> u32 {
        self.0
            .npc_aliases
            .lock()
            .map(|guard| guard.1)
            .unwrap_or(0)
    }

    pub(crate) fn mint_movement_sequence(&self) -> u64 {
        self.0.movement_sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn note_movement_admitted(&self) {
        self.0.movement_admitted.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_movement_coalesced(&self) {
        self.0.movement_coalesced.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_movement_refused(&self) {
        self.0.movement_refused.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_movement_written(&self, seq: u64) {
        self.0.movement_written.fetch_add(1, Ordering::Relaxed);
        self.0.movement_last_written_seq.store(seq, Ordering::Relaxed);
        self.0.movement_writes_since_correction.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn movement_written_seq_unambiguous(&self) -> (Option<u64>, bool) {
        let seq = self.0.movement_last_written_seq.load(Ordering::Relaxed);
        let writes = self.0.movement_writes_since_correction.load(Ordering::Relaxed);
        if seq == 0 {
            (None, true)
        } else if writes <= 1 {
            (Some(seq), false)
        } else {
            (None, true)
        }
    }

    pub(crate) fn clear_movement_writes_since_correction(&self) {
        self.0.movement_writes_since_correction.store(0, Ordering::Relaxed);
    }

    pub(crate) fn movement_counters(&self) -> (u64, u64, u64, u64) {
        (
            self.0.movement_admitted.load(Ordering::Relaxed),
            self.0.movement_coalesced.load(Ordering::Relaxed),
            self.0.movement_refused.load(Ordering::Relaxed),
            self.0.movement_written.load(Ordering::Relaxed),
        )
    }

    /// Sample high-volume object-update packet metadata without sampling state changes.
    pub(crate) fn sample_update_packet(&self) -> bool {
        let index = self.0.update_packets_seen.fetch_add(1, Ordering::Relaxed);
        if index & 63 == 0 {
            true
        } else {
            self.0.update_packets_sampled_out.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub(crate) fn capture_script_counts(
        &self,
        scope: Option<(u64, u64)>,
        cursor: &mut ScriptCursor,
        vm_session: u64,
        rows: &[(u64, benilla_ui::script::diagnostics::DiagnosticKind, u32)],
    ) {
        use benilla_ui::script::diagnostics::DiagnosticKind;
        let generation = self.0.reload_generation.load(Ordering::Acquire);
        if cursor.generation != generation || cursor.vm_session != vm_session {
            *cursor = ScriptCursor { generation, vm_session, ..Default::default() };
        }
        if let Some((first, _, _)) = rows.first() {
            let next = cursor.highest_seq.saturating_add(1);
            if *first > next {
                self.observe(scope, None, Stage::ScriptRowsLost {
                    vm_session,
                    count: *first - next,
                });
            }
        }
        for &(seq, kind, count) in rows {
            let previous = cursor.counts.get(&seq).copied().unwrap_or(0);
            let delta = count.saturating_sub(previous);
            if delta > 0 {
                let stage = match kind {
                    DiagnosticKind::Error => Stage::ScriptError { vm_session, row_seq: seq, occurrences: delta },
                    DiagnosticKind::Load => Stage::ScriptLoadError { vm_session, row_seq: seq, occurrences: delta },
                    DiagnosticKind::Warning => Stage::ScriptWarning { vm_session, row_seq: seq, occurrences: delta },
                };
                self.observe(scope, None, stage);
            }
            cursor.highest_seq = cursor.highest_seq.max(seq);
            cursor.counts.insert(seq, count);
        }
        let before = cursor.counts.len();
        cursor.counts.retain(|seq, _| rows.iter().any(|row| row.0 == *seq));
        let evicted = before.saturating_sub(cursor.counts.len());
        if evicted > 0 {
            self.observe(scope, None, Stage::ScriptRowsEvicted {
                vm_session,
                count: evicted as u64,
            });
        }
    }

    pub(crate) fn advance_reload_generation(&self) {
        self.0.reload_generation.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn observe(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        stage: Stage,
    ) {
        self.observe_with_npc(scope, quest, None, stage);
    }

    pub(crate) fn observe_with_npc(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        npc: Option<u64>,
        stage: Stage,
    ) {
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: 0,
                scope,
                quest,
                npc_alias: npc.and_then(|guid| self.npc_alias(guid)),
                operation: Operation::Observed,
                reload_generation: self.0.reload_generation.load(Ordering::Acquire),
            },
        };
        trace.record(stage);
    }

    pub(crate) fn begin(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        operation: Operation,
    ) -> Interaction {
        self.begin_with_npc(scope, quest, None, operation)
    }

    pub(crate) fn begin_with_npc(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        npc: Option<u64>,
        operation: Operation,
    ) -> Interaction {
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                scope,
                quest,
                npc_alias: npc.and_then(|guid| self.npc_alias(guid)),
                operation,
                reload_generation: self.0.reload_generation.load(Ordering::Acquire),
            },
        };
        trace.record(Stage::Intent);
        trace
    }
}

impl Interaction {
    pub(crate) fn record(&self, stage: Stage) {
        let ingress = &self.source.0;
        let event = Event {
            version: 1,
            subject: self.subject,
            elapsed_ms: ingress.origin.elapsed().as_millis() as u64,
            stage,
        };
        if ingress.events.try_send(event).is_err() {
            ingress.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Event {
    version: u8,
    subject: Subject,
    elapsed_ms: u64,
    stage: Stage,
}

#[derive(Debug, Clone, Serialize)]
struct Incident {
    version: u8,
    subject: Subject,
    outcome: IncidentOutcome,
    confidence: IncidentConfidence,
    terminal_stage: Stage,
    evidence: Vec<u64>,
    evidence_truncated: bool,
    telemetry_gap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum IncidentOutcome {
    LocalRefusal,
    WriteUnconfirmed,
    ServerReportedFailure,
    JournalMismatch,
    AmbiguousServerOutcome,
    ObservedFailure,
    TelemetryGap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum IncidentConfidence {
    Exact,
    Strong,
    Heuristic,
    Ambiguous,
    Unverified,
    Unmatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenState {
    Local,
    Written { seq: u64, elapsed_ms: u64 },
}

#[derive(Debug, Clone, Copy)]
struct OpenInteraction {
    subject: Subject,
    state: OpenState,
}

fn same_script_row(a: Stage, b: Stage) -> bool {
    match (a, b) {
        (
            Stage::ScriptError { vm_session: av, row_seq: ar, .. },
            Stage::ScriptError { vm_session: bv, row_seq: br, .. },
        )
        | (
            Stage::ScriptLoadError { vm_session: av, row_seq: ar, .. },
            Stage::ScriptLoadError { vm_session: bv, row_seq: br, .. },
        ) => av == bv && ar == br,
        _ => false,
    }
}

fn is_panel_stage(stage: Stage) -> bool {
    matches!(
        stage,
        Stage::GreetingPanel
            | Stage::DetailPanel
            | Stage::ProgressPanel
            | Stage::RewardPanel
            | Stage::DomainEvent { event: DomainEvent::GossipMenu }
    )
}

#[derive(Resource)]
pub(crate) struct History {
    source: Source,
    incoming: Receiver<Event>,
    entries: VecDeque<(u64, Event)>,
    incidents: VecDeque<Incident>,
    open_interactions: VecDeque<OpenInteraction>,
    interaction_tracking_lost: u64,
    incidents_evicted: u64,
    received: u64,
    evicted: u64,
    source_gaps: u64,
    #[cfg(feature = "hotpatch")]
    last_observed_patch_digest: Option<[u8; 20]>,
}

impl History {
    fn has_telemetry_gaps(&self) -> bool {
        self.evicted > 0
            || self.source_gaps > 0
            || self.interaction_tracking_lost > 0
            || self.source.0.dropped.load(Ordering::Relaxed) > 0
    }

    fn push_incident(
        &mut self,
        subject: Subject,
        outcome: IncidentOutcome,
        confidence: IncidentConfidence,
        terminal_stage: Stage,
        mut evidence: Vec<u64>,
        mut evidence_truncated: bool,
    ) {
        let telemetry_gap = self.has_telemetry_gaps();
        if evidence.len() > 8 {
            let drop_count = evidence.len() - 8;
            evidence.drain(0..drop_count);
            evidence_truncated = true;
        }
        if self.incidents.len() == 64 {
            self.incidents.pop_front();
            self.incidents_evicted += 1;
        }
        self.incidents.push_back(Incident {
            version: 1,
            subject,
            outcome,
            confidence,
            terminal_stage,
            evidence,
            evidence_truncated,
            telemetry_gap,
        });
    }

    fn collect_interaction_evidence(&self, interaction_id: u64, terminal_seq: u64) -> (Vec<u64>, bool) {
        if interaction_id == 0 {
            return (vec![terminal_seq], false);
        }
        let mut evidence: Vec<u64> = self
            .entries
            .iter()
            .rev()
            .filter(|(seq, prior)| {
                prior.subject.interaction == interaction_id && *seq != terminal_seq
            })
            .take(8)
            .map(|(prior_seq, _)| *prior_seq)
            .collect();
        let truncated = evidence.len() == 8;
        evidence.truncate(7);
        evidence.reverse();
        evidence.push(terminal_seq);
        (evidence, truncated)
    }

    fn retire_scope(&mut self, retired_scope: Option<(u64, u64)>, terminal_stage: Stage, seq: u64) {
        let mut remaining = VecDeque::with_capacity(self.open_interactions.len());
        let mut retired = Vec::new();
        while let Some(open) = self.open_interactions.pop_front() {
            let matches_scope = match (retired_scope, open.subject.scope) {
                (Some(target), Some(current)) => target == current,
                (None, _) => true,
                _ => false,
            };
            if matches_scope {
                retired.push(open);
            } else {
                remaining.push_back(open);
            }
        }
        self.open_interactions = remaining;
        let gaps = self.has_telemetry_gaps();
        for open in retired {
            let (evidence, truncated) =
                self.collect_interaction_evidence(open.subject.interaction, seq);
            match open.state {
                OpenState::Written { .. } => {
                    self.push_incident(
                        open.subject,
                        IncidentOutcome::WriteUnconfirmed,
                        IncidentConfidence::Unverified,
                        terminal_stage,
                        evidence,
                        truncated,
                    );
                }
                OpenState::Local => {
                    self.push_incident(
                        open.subject,
                        IncidentOutcome::LocalRefusal,
                        if gaps {
                            IncidentConfidence::Unverified
                        } else {
                            IncidentConfidence::Exact
                        },
                        terminal_stage,
                        evidence,
                        truncated,
                    );
                }
            }
        }
    }
}

#[cfg(feature = "dev")]
impl History {
    /// A time-window association is a lead, not proof that a press caused a script error.
    pub(crate) fn ui_error_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        self.entries.iter().rev().filter_map(|(_, event)| {
            let (vm_session, row_seq) = match event.stage {
                Stage::ScriptError { vm_session, row_seq, .. } => (vm_session, row_seq),
                _ => return None,
            };
            let candidates = self.entries.iter().filter(|(_, earlier)| {
                matches!(earlier.stage, Stage::UiPress { vm_session: vm, .. } if vm == vm_session)
                    && earlier.subject.scope == event.subject.scope
                    && earlier.elapsed_ms <= event.elapsed_ms
                    && event.elapsed_ms.saturating_sub(earlier.elapsed_ms) <= 2_000
            }).count();
            let result = match candidates {
                0 => format!("UI error {row_seq}: no retained press within two seconds"),
                1 => format!("UI error {row_seq}: one nearby press (possible association)"),
                _ => format!("UI error {row_seq}: {candidates} nearby presses (ambiguous)"),
            };
            Some(if gaps { format!("{result}; trace has gaps") } else { result })
        }).take(8).collect()
    }

    /// Summarize recent UI clicks without claiming causality for later gameplay refusals.
    #[cfg(feature = "dev")]
    pub(crate) fn ui_click_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, event)| match event.stage {
                Stage::UiClick {
                    button,
                    target,
                    consumed,
                    visible,
                    clipped,
                    handler_invoked,
                    vm_session,
                    ..
                } => {
                    let visibility = match visible {
                        Some(true) => "visible",
                        Some(false) => "not effectively visible",
                        None => "visibility unknown",
                    };
                    let line = format!(
                        "UI click {:?} on {:?}: consumed={} visible={} clipped={} handler_invoked={} vm={}",
                        button, target, consumed, visibility, clipped, handler_invoked, vm_session
                    );
                    Some(if gaps {
                        format!("{line}; trace has gaps")
                    } else {
                        line
                    })
                }
                _ => None,
            })
            .take(8)
            .collect()
    }

    pub(crate) fn recent_incidents(&self) -> impl Iterator<Item = String> + '_ {
        self.incidents.iter().rev().take(8).map(|incident| {
            let missing = incident.evidence.iter().any(|seq| {
                !self.entries.iter().any(|(retained, _)| retained == seq)
            });
            format!(
                "Action {} {:?} (npc {:?}, quest {:?}): {:?} [{:?}] {:?} (evidence {:?}{}{}{})",
                incident.subject.interaction,
                incident.subject.operation,
                incident.subject.npc_alias,
                incident.subject.quest,
                incident.outcome,
                incident.confidence,
                incident.terminal_stage,
                incident.evidence,
                if incident.evidence_truncated { ", truncated" } else { "" },
                if missing { ", some events evicted" } else { "" },
                if incident.telemetry_gap { ", trace has gaps" } else { "" },
            )
        })
    }

    /// Summarize observed questgiver/gossip panels against preceding `Open` and `Select` writes.
    pub(crate) fn panel_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, event)| {
                if !is_panel_stage(event.stage) {
                    return None;
                }
                let scope = event.subject.scope?;
                let npc_alias = event.subject.npc_alias;
                let quest = event.subject.quest;
                let candidates: Vec<_> = self
                    .entries
                    .iter()
                    .filter_map(|(_, candidate)| {
                        let written = matches!(
                            candidate.stage,
                            Stage::Delivery(CommandDelivery::Written)
                        );
                        let within_window = candidate.subject.scope == Some(scope)
                            && candidate.elapsed_ms <= event.elapsed_ms
                            && event.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000;
                        if !written || !within_window {
                            return None;
                        }
                        let op_matches = match event.stage {
                            Stage::GreetingPanel
                            | Stage::DomainEvent {
                                event: DomainEvent::GossipMenu,
                            } => {
                                candidate.subject.operation == Operation::Open
                                    && npc_alias.is_some()
                                    && candidate.subject.npc_alias == npc_alias
                            }
                            Stage::DetailPanel | Stage::ProgressPanel | Stage::RewardPanel => {
                                ((candidate.subject.operation == Operation::Select
                                    || (matches!(event.stage, Stage::RewardPanel)
                                        && candidate.subject.operation
                                            == Operation::RequestReward))
                                    && quest.is_some()
                                    && candidate.subject.quest == quest
                                    && (npc_alias.is_none()
                                        || candidate.subject.npc_alias.is_none()
                                        || candidate.subject.npc_alias == npc_alias))
                                    || (candidate.subject.operation == Operation::Open
                                        && npc_alias.is_some()
                                        && candidate.subject.npc_alias == npc_alias)
                            }
                            _ => false,
                        };
                        op_matches.then_some(candidate.subject.interaction)
                    })
                    .collect();
                let label = match event.stage {
                    Stage::GreetingPanel => "Greeting panel".to_string(),
                    Stage::DetailPanel => format!("Detail panel (quest {quest:?})"),
                    Stage::ProgressPanel => format!("Progress panel (quest {quest:?})"),
                    Stage::RewardPanel => format!("Reward panel (quest {quest:?})"),
                    Stage::DomainEvent {
                        event: DomainEvent::GossipMenu,
                    } => "Gossip panel".to_string(),
                    _ => return None,
                };
                let summary = match candidates.as_slice() {
                    [id] => format!(
                        "{label} on npc {npc_alias:?}: observed after action {id} (possible match)"
                    ),
                    [] => format!(
                        "{label} on npc {npc_alias:?}: observed without a retained write in 30s"
                    ),
                    many => format!(
                        "{label} on npc {npc_alias:?}: {} matching writes (ambiguous)",
                        many.len()
                    ),
                };
                Some(if gaps {
                    format!("{summary}; trace has gaps")
                } else {
                    summary
                })
            })
            .take(8)
            .collect()
    }

    pub(crate) fn action_summaries(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut output = Vec::new();
        let gaps = self.has_telemetry_gaps();
        for (_, event) in self.entries.iter().rev() {
            let id = event.subject.interaction;
            if id == 0 || !seen.insert(id) {
                continue;
            }
            let status = match event.stage {
                Stage::GuardRefused => "blocked by a local guard",
                Stage::NoWorld => "no active world connection",
                Stage::AlreadyPending | Stage::TrackingCapacity => "local tracking is busy",
                Stage::ChannelDisconnected => "command channel disconnected",
                Stage::Delivery(CommandDelivery::Rejected(_)) => "rejected before socket write",
                Stage::Delivery(CommandDelivery::WriteFailed) => "socket write failed; outcome unknown",
                Stage::Delivery(CommandDelivery::Written) => "sent; server outcome unconfirmed",
                Stage::Delivery(CommandDelivery::OutboxAdmitted) => "queued; socket write unconfirmed",
                Stage::ChannelAdmitted | Stage::Encoded => "command admitted; delivery unconfirmed",
                _ => "intent recorded; delivery unconfirmed",
            };
            let reason = match event.stage {
                Stage::Delivery(CommandDelivery::Rejected(reason)) => format!(" ({reason:?})"),
                _ => String::new(),
            };
            let suffix = if gaps { "; trace has gaps" } else { "" };
            let server_evidence = if matches!(event.stage, Stage::Delivery(CommandDelivery::Written))
            {
                self.entries
                    .iter()
                    .find_map(|(_, later)| {
                        let same_scope_and_window = event.subject.scope.is_some()
                            && later.subject.scope == event.subject.scope
                            && later.elapsed_ms >= event.elapsed_ms
                            && later.elapsed_ms.saturating_sub(event.elapsed_ms) <= 30_000;
                        if !same_scope_and_window {
                            return None;
                        }
                        match event.subject.operation {
                            Operation::Accept => {
                                if event.subject.quest.is_some()
                                    && later.subject.quest == event.subject.quest
                                    && matches!(
                                        later.stage,
                                        Stage::ServerQuestFailed { .. } | Stage::QuestSlotAdded
                                    )
                                {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Accept
                                                && candidate.subject.scope == later.subject.scope
                                                && candidate.subject.quest == later.subject.quest
                                                && matches!(
                                                    candidate.stage,
                                                    Stage::Delivery(CommandDelivery::Written)
                                                )
                                                && candidate.elapsed_ms <= later.elapsed_ms
                                                && later
                                                    .elapsed_ms
                                                    .saturating_sub(candidate.elapsed_ms)
                                                    <= 30_000
                                        })
                                        .count();
                                    let confidence = if matches > 1 {
                                        "ambiguous"
                                    } else {
                                        "possible match"
                                    };
                                    return match later.stage {
                                        Stage::ServerQuestFailed { reason } => Some(format!(
                                            "; server reported quest failure {reason} ({confidence})"
                                        )),
                                        Stage::QuestSlotAdded => {
                                            Some(format!("; quest slot added ({confidence})"))
                                        }
                                        _ => None,
                                    };
                                }
                                if matches!(
                                    later.stage,
                                    Stage::ServerQuestLogFull | Stage::ServerQuestInvalid { .. }
                                ) && (later.subject.npc_alias.is_none()
                                    || event.subject.npc_alias.is_none()
                                    || later.subject.npc_alias == event.subject.npc_alias)
                                {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Accept
                                                && candidate.subject.scope == later.subject.scope
                                                && matches!(
                                                    candidate.stage,
                                                    Stage::Delivery(CommandDelivery::Written)
                                                )
                                                && candidate.elapsed_ms <= later.elapsed_ms
                                                && later
                                                    .elapsed_ms
                                                    .saturating_sub(candidate.elapsed_ms)
                                                    <= 30_000
                                        })
                                        .count();
                                    let confidence = if matches > 1 {
                                        "ambiguous, no quest ID"
                                    } else {
                                        "heuristic match, no quest ID"
                                    };
                                    return match later.stage {
                                        Stage::ServerQuestLogFull => {
                                            Some(format!("; server quest log full ({confidence})"))
                                        }
                                        Stage::ServerQuestInvalid { reason } => Some(format!(
                                            "; server quest invalid {reason} ({confidence})"
                                        )),
                                        _ => None,
                                    };
                                }
                                None
                            }
                            Operation::Select => {
                                if event.subject.quest.is_some()
                                    && later.subject.quest == event.subject.quest
                                    && matches!(
                                        later.stage,
                                        Stage::DetailPanel
                                            | Stage::ProgressPanel
                                            | Stage::RewardPanel
                                            | Stage::ServerQuestFailed { .. }
                                    )
                                {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Select
                                                && candidate.subject.scope == later.subject.scope
                                                && candidate.subject.quest == later.subject.quest
                                                && matches!(
                                                    candidate.stage,
                                                    Stage::Delivery(CommandDelivery::Written)
                                                )
                                                && candidate.elapsed_ms <= later.elapsed_ms
                                                && later
                                                    .elapsed_ms
                                                    .saturating_sub(candidate.elapsed_ms)
                                                    <= 30_000
                                        })
                                        .count();
                                    let confidence = if matches > 1 {
                                        "ambiguous"
                                    } else {
                                        "possible match"
                                    };
                                    return match later.stage {
                                        Stage::DetailPanel => {
                                            Some(format!("; detail panel observed ({confidence})"))
                                        }
                                        Stage::ProgressPanel => Some(format!(
                                            "; progress panel observed ({confidence})"
                                        )),
                                        Stage::RewardPanel => {
                                            Some(format!("; reward panel observed ({confidence})"))
                                        }
                                        Stage::ServerQuestFailed { reason } => Some(format!(
                                            "; server reported quest failure {reason} ({confidence})"
                                        )),
                                        _ => None,
                                    };
                                }
                                None
                            }
                            Operation::Open => {
                                if event.subject.npc_alias.is_some()
                                    && later.subject.npc_alias == event.subject.npc_alias
                                    && is_panel_stage(later.stage)
                                {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Open
                                                && candidate.subject.scope == later.subject.scope
                                                && candidate.subject.npc_alias
                                                    == later.subject.npc_alias
                                                && matches!(
                                                    candidate.stage,
                                                    Stage::Delivery(CommandDelivery::Written)
                                                )
                                                && candidate.elapsed_ms <= later.elapsed_ms
                                                && later
                                                    .elapsed_ms
                                                    .saturating_sub(candidate.elapsed_ms)
                                                    <= 30_000
                                        })
                                        .count();
                                    let confidence = if matches > 1 {
                                        "ambiguous"
                                    } else {
                                        "possible match"
                                    };
                                    return Some(format!("; panel observed ({confidence})"));
                                }
                                None
                            }
                            Operation::RequestReward => {
                                if event.subject.quest.is_some()
                                    && later.subject.quest == event.subject.quest
                                    && matches!(
                                        later.stage,
                                        Stage::RewardPanel | Stage::ServerQuestFailed { .. }
                                    )
                                {
                                    return match later.stage {
                                        Stage::RewardPanel => {
                                            Some("; reward panel observed (possible match)".into())
                                        }
                                        Stage::ServerQuestFailed { reason } => Some(format!(
                                            "; server reported quest failure {reason} (possible match)"
                                        )),
                                        _ => None,
                                    };
                                }
                                None
                            }
                            Operation::ChooseReward => {
                                if event.subject.quest.is_some()
                                    && later.subject.quest == event.subject.quest
                                    && matches!(
                                        later.stage,
                                        Stage::QuestCompletePacket { .. }
                                            | Stage::QuestSlotRemoved
                                            | Stage::ServerQuestFailed { .. }
                                    )
                                {
                                    return match later.stage {
                                        Stage::QuestCompletePacket { .. } => Some(
                                            "; turn-in packet observed (awaiting/checking reward deltas)"
                                                .into(),
                                        ),
                                        Stage::QuestSlotRemoved => {
                                            Some("; quest slot removed (possible match)".into())
                                        }
                                        Stage::ServerQuestFailed { reason } => Some(format!(
                                            "; server reported quest failure {reason} (possible match)"
                                        )),
                                        _ => None,
                                    };
                                }
                                None
                            }
                            Operation::Observed
                            | Operation::Loot
                            | Operation::Equip
                            | Operation::Buy
                            | Operation::Sell => None,
                        }
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            output.push(format!(
                "Action {id} {:?} (npc {:?}, quest {:?}): {status}{reason}{server_evidence}{suffix}",
                event.subject.operation, event.subject.npc_alias, event.subject.quest
            ));
            if output.len() == 8 {
                break;
            }
        }
        output
    }

    pub(crate) fn export_status(&self) -> &'static str {
        match self.source.0.export_status.load(Ordering::Acquire) {
            1 => "Saving report…",
            2 => "Report saved in Logs/InteractionDiagnostics.",
            3 => "Report could not be saved. See the client log.",
            _ => "",
        }
    }

    pub(crate) fn counters(&self) -> (u64, u64, u64) {
        (self.received, self.evicted, self.source.0.dropped.load(Ordering::Relaxed))
    }

    /// Additive per-vector evidence summary derived from the same bounded snapshot that
    /// supplies the exported event rows.  Classification goes through `vector_for_stage`
    /// so both export paths agree.
    pub(crate) fn diagnostic_vectors(&self) -> Vec<DiagnosticVectorEvidence> {
        const VECTORS: &[&str] = &[
            "quest_interactions",
            "quest_panel_correlation",
            "ui_frame_state",
            "ui_pointer_categories",
            "lua_diagnostic_counts",
            "session_lifecycle",
            "allowlisted_packets",
            "casts",
            "inventory_failures",
            "loot_and_vendor_events",
            "audio_mixer_state",
            "movement_queue_and_corrections",
            "scene_camera_and_rig_counts",
            "frame_intervals",
            "asset_load_failure_categories",
            "hotpatch",
        ];
        let dropped = self.source.0.dropped.load(Ordering::Relaxed);
        let evicted = self.evicted;
        let interaction_tracking_lost = self.interaction_tracking_lost;
        let mut out = Vec::with_capacity(VECTORS.len());
        for &vector in VECTORS {
            let available = if vector == "hotpatch" {
                cfg!(feature = "hotpatch")
            } else {
                true
            };
            let mut retained_observations = 0u64;
            let mut latest_retained_sequence = None;
            let mut source_specific_loss = false;
            for (seq, event) in &self.entries {
                if vector_for_stage(&event.stage) == Some(vector) {
                    retained_observations = retained_observations.saturating_add(1);
                    latest_retained_sequence = Some(*seq);
                    if is_source_specific_loss_stage(&event.stage) {
                        source_specific_loss = true;
                    }
                }
            }
            if vector == "quest_interactions" && interaction_tracking_lost > 0 {
                source_specific_loss = true;
            }
            let mut limiting_factors = vec!["event_driven_reporting".to_string()];
            if vector == "allowlisted_packets" {
                limiting_factors.push("sampling".to_string());
            }
            if source_specific_loss {
                limiting_factors.push("source_specific_loss".to_string());
            }
            if dropped > 0 || evicted > 0 {
                limiting_factors.push("global_loss_unknown_vector".to_string());
            }
            let value = DiagnosticVectorEvidence {
                vector: vector.to_string(),
                instrumentation_available: available,
                retained_observations,
                latest_retained_sequence,
                limiting_factors: limiting_factors.clone(),
                absence_not_proof: true,
            };
            out.push(value);
        }
        out
    }

    pub(crate) fn source_gaps(&self) -> u64 {
        self.source_gaps
    }

    /// Recent one-second movement pressure events (bounded).
    pub(crate) fn movement_pressure(&self) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(seq, event)| match event.stage {
                Stage::MovementPressure {
                    baseline,
                    admitted,
                    coalesced,
                    refused,
                    written,
                    queue_depth,
                    oldest_age_ms,
                } => Some(format!(
                    "#{seq} movement pressure: baseline={baseline} admitted={admitted} coalesced={coalesced} refused={refused} written={written} queue_depth={queue_depth} oldest_age_ms={oldest_age_ms}"
                )),
                _ => None,
            })
            .take(8)
            .collect()
    }
    pub(crate) fn one_second_movement_pressure(&self) -> Vec<String> {
        self.movement_pressure()
    }
    pub(crate) fn movement_pressure_events(&self) -> Vec<String> {
        self.movement_pressure()
    }

    /// Movement correction attribution. Unambiguous only when a single movement write followed
    /// the previous correction; otherwise the attribution is ambiguous.
    pub(crate) fn movement_correction_attribution(&self) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(seq, event)| match event.stage {
                Stage::MovementCorrectionApplied {
                    distance_bucket,
                    movement_seq,
                    ambiguous,
                } => Some(format!(
                    "#{seq} movement correction: distance_bucket={distance_bucket} movement_seq={movement_seq:?} ambiguous={ambiguous} (attribution is unambiguous only when a single movement write followed the previous correction)"
                )),
                _ => None,
            })
            .take(8)
            .collect()
    }
    pub(crate) fn movement_corrections(&self) -> Vec<String> {
        self.movement_correction_attribution()
    }

    /// Recent audio output health windows (bounded). `cycles == 0` means no stream, not health.
    pub(crate) fn audio_output_health(&self) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(seq, event)| match event.stage {
                Stage::AudioHealth {
                    cycles,
                    underruns,
                    overloads,
                    lead_min_ms,
                    ring_min_ms,
                    ..
                } => Some(format!(
                    "#{seq} audio output: cycles={cycles} (no_stream={}) underruns={underruns} overloads={overloads} lead_min_ms={:?} ring_min_ms={:?}",
                    cycles == 0,
                    lead_min_ms,
                    ring_min_ms,
                )),
                _ => None,
            })
            .take(8)
            .collect()
    }
    pub(crate) fn audio_output(&self) -> Vec<String> {
        self.audio_output_health()
    }

    /// Recent asset per-window summaries (bounded).
    pub(crate) fn asset_window_summary(&self) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(seq, event)| match event.stage {
                Stage::AssetWindow { failures, missed, window_ms } => Some(format!(
                    "#{seq} asset window: observed_failures={failures} missed_events={missed} window_ms={window_ms} (Bevy untyped asset-failure messages only)"
                )),
                _ => None,
            })
            .take(8)
            .collect()
    }
    pub(crate) fn asset_window(&self) -> Vec<String> {
        self.asset_window_summary()
    }

    #[cfg(feature = "hotpatch")]
    pub(crate) fn hotpatch_attempt_correlation(&self) -> Vec<String> {
        // Correlate candidate staging to terminal events by attempt id within retained entries.
        let mut out = Vec::new();
        for (_, event) in self.entries.iter().rev() {
            if let Stage::Hotpatch {
                event: PatchEvent::CandidateStaged { attempt },
            } = event.stage
            {
                let terminals: Vec<Stage> = self
                    .entries
                    .iter()
                    .filter_map(|(_, later)| {
                        let att = match later.stage {
                            Stage::Hotpatch {
                                event: PatchEvent::Activated { attempt: a, .. },
                            } => *a,
                            Stage::Hotpatch {
                                event: PatchEvent::CandidateRejected { attempt: a, .. },
                            } => *a,
                            Stage::Hotpatch {
                                event: PatchEvent::FrameFailed { attempt: a, .. },
                            } => *a,
                            Stage::Hotpatch {
                                event: PatchEvent::RetentionFailed { attempt: a, .. },
                            } => *a,
                            _ => None,
                        };
                        if att == Some(attempt) {
                            Some(later.stage)
                        } else {
                            None
                        }
                    })
                    .collect();
                let terminal_count = terminals.len();
                let terminal = terminals.into_iter().last();
                let earlier = if terminal_count > 1 {
                    " (earlier terminal evidence exists)"
                } else {
                    ""
                };
                let line = match terminal {
                    Some(Stage::Hotpatch {
                        event: PatchEvent::Activated { digest, .. },
                    }) => format!("attempt {attempt}: activated (digest={digest:?}){earlier}"),
                    Some(Stage::Hotpatch { event }) => {
                        format!("attempt {attempt}: {:?}{earlier}", event)
                    }
                    None => format!("attempt {attempt}: no terminal event retained (ambiguous/unresolved)"),
                };
                out.push(line);
                if out.len() >= 8 {
                    break;
                }
            }
        }
        out
    }
    #[cfg(feature = "hotpatch")]
    pub(crate) fn hotpatch_attempts(&self) -> Vec<String> {
        self.hotpatch_attempt_correlation()
    }

    pub(crate) fn sampled_updates(&self) -> u64 {
        self.source.0.update_packets_sampled_out.load(Ordering::Relaxed)
    }

    pub(crate) fn recent(&self) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().rev().take(16).map(|(seq, e)| {
            format!("#{seq} +{}ms · action {} {:?} · npc {:?} · quest {:?} · {:?} · scope {:?}",
                e.elapsed_ms, e.subject.interaction, e.subject.operation, e.subject.npc_alias,
                e.subject.quest, e.stage, e.subject.scope)
        })
    }

    /// Summarize received quest-slot evidence without assigning it to a specific request
    /// when more than one earlier write could explain it. On Lua reload, require a fresh
    /// projection in the active VM generation before evaluating its row.
    pub(crate) fn slot_correlations(&self) -> Vec<String> {
        let incomplete = self.has_telemetry_gaps();
        let active_reload = self.source.0.reload_generation.load(Ordering::Acquire);
        self.entries.iter().rev().filter_map(|(_, event)| {
            if !matches!(event.stage, Stage::QuestSlotAdded) {
                return None;
            }
            let quest = event.subject.quest?;
            let scope = event.subject.scope?;
            let candidates: Vec<_> = self.entries.iter().filter_map(|(_, candidate)| {
                (candidate.subject.operation == Operation::Accept
                    && candidate.subject.scope == Some(scope)
                    && candidate.subject.quest == Some(quest)
                    && candidate.elapsed_ms <= event.elapsed_ms
                    && event.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000
                    && matches!(candidate.stage, Stage::Delivery(CommandDelivery::Written)))
                    .then_some(candidate.subject.interaction)
            }).collect();
            let projected_in_active_vm = self.entries.iter().any(|(_, later)| {
                matches!(later.stage, Stage::JournalProjection)
                    && later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.subject.reload_generation == active_reload
                    && later.elapsed_ms >= event.elapsed_ms
            });
            let projected_in_old_vm = !projected_in_active_vm
                && self.entries.iter().any(|(_, later)| {
                    matches!(later.stage, Stage::JournalProjection)
                        && later.subject.scope == Some(scope)
                        && later.subject.quest == Some(quest)
                        && later.subject.reload_generation < active_reload
                        && later.elapsed_ms >= event.elapsed_ms
                });
            let projection = if projected_in_active_vm {
                "later journal projection observed"
            } else if projected_in_old_vm {
                "old VM projection superseded by reload; awaiting fresh projection"
            } else {
                "journal projection unverified"
            };
            let row = self.entries.iter().any(|(_, later)| {
                matches!(later.stage, Stage::JournalRowInModel)
                    && later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.subject.reload_generation == active_reload
                    && later.elapsed_ms >= event.elapsed_ms
            });
            let template_refused = self.entries.iter().any(|(_, later)| {
                matches!(later.stage, Stage::JournalTemplateRefused)
                    && later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.elapsed_ms >= event.elapsed_ms
            });
            let template_pending = !row
                && !template_refused
                && self.entries.iter().any(|(_, later)| {
                    matches!(later.stage, Stage::JournalTemplatePending)
                        && later.subject.scope == Some(scope)
                        && later.subject.quest == Some(quest)
                        && later.subject.reload_generation == active_reload
                        && later.elapsed_ms >= event.elapsed_ms
                });
            let row_status = if row {
                "; row exists in journal model"
            } else if template_refused {
                "; quest template refused by server"
            } else if template_pending {
                "; awaiting quest template"
            } else if projected_in_active_vm && !incomplete {
                "; journal mismatch (projected without row in model)"
            } else {
                "; row in journal model unverified"
            };
            let latest_progress = self.entries.iter().rev().find_map(|(_, later)| {
                if later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.elapsed_ms >= event.elapsed_ms
                {
                    if let Stage::QuestSlotProgressed { counters, state } = later.stage {
                        return Some((counters, state));
                    }
                }
                None
            });
            let progress_status = match latest_progress {
                Some((counters, state)) => {
                    format!("; objective progress counters={counters:?} state={state}")
                }
                None => String::new(),
            };
            let result = match candidates.as_slice() {
                [id] => format!("Quest {quest}: slot observed after action {id} (possible match; {projection})"),
                [] => format!("Quest {quest}: slot observed without a retained write in the preceding 30 seconds; {projection}"),
                _ => format!("Quest {quest}: slot observed, {} matching writes (ambiguous); {projection}", candidates.len()),
            };
            Some(if incomplete {
                format!("{result}{row_status}{progress_status}; trace has gaps")
            } else {
                format!("{result}{row_status}{progress_status}")
            })
        }).take(8).collect()
    }

    /// Correlate `SMSG_QUESTGIVER_QUEST_COMPLETE` against `ChooseReward` writes and asynchronous
    /// state deltas (`QuestSlotRemoved`, `PlayerXpChanged`, `PlayerMoneyChanged`, `RewardItemPushed`).
    /// Never claim turn-in completion from `SMSG_QUESTGIVER_QUEST_COMPLETE` alone.
    pub(crate) fn turn_in_correlations(&self) -> Vec<String> {
        let incomplete = self.has_telemetry_gaps();
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, event)| {
                let Stage::QuestCompletePacket {
                    xp,
                    money,
                    reward_items,
                } = event.stage
                else {
                    return None;
                };
                let quest = event.subject.quest?;
                let scope = event.subject.scope?;
                let candidates: Vec<_> = self
                    .entries
                    .iter()
                    .filter_map(|(_, candidate)| {
                        (candidate.subject.operation == Operation::ChooseReward
                            && candidate.subject.scope == Some(scope)
                            && candidate.subject.quest == Some(quest)
                            && candidate.elapsed_ms <= event.elapsed_ms
                            && event.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000
                            && matches!(
                                candidate.stage,
                                Stage::Delivery(CommandDelivery::Written)
                            ))
                        .then_some(candidate.subject.interaction)
                    })
                    .collect();

                let in_delta_window = |other: &Event| {
                    other.subject.scope == Some(scope)
                        && other.elapsed_ms.abs_diff(event.elapsed_ms) <= 5_000
                };
                let slot_removed = self.entries.iter().any(|(_, other)| {
                    in_delta_window(other)
                        && other.subject.quest == Some(quest)
                        && matches!(other.stage, Stage::QuestSlotRemoved)
                });
                let xp_observed = xp == 0
                    || self.entries.iter().any(|(_, other)| {
                        in_delta_window(other)
                            && matches!(other.stage, Stage::PlayerXpChanged { delta } if delta > 0)
                    });
                let money_observed = money == 0
                    || self.entries.iter().any(|(_, other)| {
                        in_delta_window(other)
                            && matches!(
                                other.stage,
                                Stage::PlayerMoneyChanged { delta } if delta > 0
                            )
                    });
                let items_pushed = self
                    .entries
                    .iter()
                    .filter(|(_, other)| {
                        in_delta_window(other)
                            && matches!(other.stage, Stage::RewardItemPushed { count, .. } if count > 0)
                    })
                    .count() as u32;
                let items_observed = items_pushed >= reward_items;

                let mut missing = Vec::new();
                if !slot_removed {
                    missing.push("quest slot removal");
                }
                if !xp_observed {
                    missing.push("player XP delta");
                }
                if !money_observed {
                    missing.push("player money delta");
                }
                if !items_observed {
                    missing.push("reward item push");
                }

                let action_part = match candidates.as_slice() {
                    [id] => format!("after action {id} (possible match)"),
                    [] => "without a retained ChooseReward write in 30s".to_string(),
                    many => format!("{} matching ChooseReward writes (ambiguous)", many.len()),
                };
                let delta_part = if missing.is_empty() {
                    format!(
                        "turn-in deltas verified (slot removed, xp={xp}, money={money}, items={items_pushed}/{reward_items})"
                    )
                } else {
                    format!(
                        "incomplete turn-in projection (packet received; missing: {})",
                        missing.join(", ")
                    )
                };
                let summary = format!("Quest {quest} turn-in {action_part}: {delta_part}");
                Some(if incomplete {
                    format!("{summary}; trace has gaps")
                } else {
                    summary
                })
            })
            .take(8)
            .collect()
    }

    /// Save an allowlisted snapshot away from the frame thread.
    pub(crate) fn export(&self, build: benilla_world::build_id::BuildId) -> Result<(), String> {
        let directory = crate::local_state::home()
            .ok_or_else(|| "Local report storage is unavailable".to_owned())?
            .join("Logs").join("InteractionDiagnostics");
        if self.source.0.exporting.swap(true, Ordering::AcqRel) {
            return Err("A report is already being saved".into());
        }
        self.source.0.export_status.store(1, Ordering::Release);
        let ingress = self.source.0.clone();
        let rows: Vec<_> = self.entries.iter().cloned().collect();
        let incidents: Vec<_> = self.incidents.iter().map(|incident| {
            let missing: Vec<_> = incident.evidence.iter().copied()
                .filter(|seq| !self.entries.iter().any(|(retained, _)| retained == seq))
                .collect();
            serde_json::json!({ "record": incident, "missing_event_sequences": missing })
        }).collect();
        let incidents_evicted = self.incidents_evicted;
        let interaction_tracking_lost = self.interaction_tracking_lost;
        let (received, evicted, dropped) = self.counters();
        let ingress_pending = self.incoming.len();
        let source_gaps = self.source_gaps;
        #[cfg(feature = "hotpatch")]
        let last_observed_patch_digest = self.last_observed_patch_digest;
        #[cfg(not(feature = "hotpatch"))]
        let last_observed_patch_digest: Option<[u8; 20]> = None;
        let sampled_updates = self.sampled_updates();
        let diagnostic_vectors = self.diagnostic_vectors();
        let first_sequence = self.entries.front().map(|(seq, _)| *seq);
        let highest_sequence = self.entries.back().map(|(seq, _)| *seq);
        let npc_aliases_assigned = self.source.npc_alias_count();
        let run_id = self.source.0.run_id.clone();
        let asset_profile = self.source.0.asset_profile.clone();
        let action_summaries = self.action_summaries();
        let panel_correlations = self.panel_correlations();
        let slot_correlations = self.slot_correlations();
        let turn_in_correlations = self.turn_in_correlations();
        let ui_correlations = self.ui_error_correlations();
        let spawn = std::thread::Builder::new()
            .name("interaction-diagnostics-export".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    use std::io::{Read, Write};
                    use sha1::{Digest, Sha1};
                    std::fs::create_dir_all(&directory)?;
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?.as_nanos();
                    let path = directory.join(format!("interaction-{timestamp}.jsonl"));
                    let temporary = directory.join(format!("interaction-{timestamp}.tmp"));
                    let file_hash = std::env::current_exe().ok().and_then(|executable| {
                        let mut file = std::fs::File::open(executable).ok()?;
                        let mut digest = Sha1::new();
                        let mut buffer = [0u8; 65536];
                        loop {
                            let count = file.read(&mut buffer).ok()?;
                            if count == 0 {
                                break;
                            }
                            digest.update(&buffer[..count]);
                        }
                        Some(format!("{:x}", digest.finalize()))
                    });
                    let mut text = serde_json::json!({
                        "schema": "rustwrath.interaction-diagnostics.v1",
                        "version": 1,
                        "run_id": run_id,
                        "protocol_mode": "wrath_12340",
                        "coverage": {
                            "instrumented_paths": [
                                "quest_interactions", "quest_panel_correlation",
                                "ui_frame_state", "ui_pointer_categories",
                                "lua_diagnostic_counts", "session_lifecycle", "allowlisted_packets",
                                "casts", "inventory_failures", "loot_and_vendor_events",
                                "audio_mixer_state", "movement_queue_and_corrections",
                                "scene_camera_and_rig_counts", "frame_intervals",
                                "asset_load_failure_categories"
                            ],
                            "diagnostic_vector_coverage": true,
                            "hotpatch_compiled": cfg!(feature = "hotpatch"),
                            "not_yet_instrumented": [
                                "visible_pixels", "frame_occlusion_owner", "audio_output",
                                "movement_causality", "asset_dependency_revisions"
                            ]
                        },
                        "build_git_sha": if build.sha.is_empty() { None } else { Some(build.sha) },
                        "build_dirty_state": "unknown",
                        "executable_file_sha1": file_hash,
                        "asset_profile_requested": asset_profile,
                        "cargo_profile": if build.profile.is_empty() { None } else { Some(build.profile) },
                        "received": received,
                        "first_retained_sequence": first_sequence,
                        "highest_retained_sequence": highest_sequence,
                        "evicted": evicted,
                        "dropped": dropped,
                        "interaction_tracking_lost": interaction_tracking_lost,
                        "npc_aliases_assigned": npc_aliases_assigned,
                        "upstream_gap_events": source_gaps,
                        "last_observed_patch_sha1_bytes": last_observed_patch_digest,
                        "object_update_packets_sampled_out": sampled_updates,
                        "incidents": incidents,
                        "incidents_evicted": incidents_evicted,
                        "ingress_pending_at_snapshot": ingress_pending,
                        "action_summaries": action_summaries,
                        "panel_correlations": panel_correlations,
                        "slot_correlations": slot_correlations,
                        "turn_in_correlations": turn_in_correlations,
                        "ui_error_correlations": ui_correlations,
                        "diagnostic_vectors": diagnostic_vectors,
                    }).to_string();
                    text.push('\n');
                    for (sequence, event) in rows {
                        text.push_str(&serde_json::to_string(&(sequence, event))?);
                        text.push('\n');
                    }
                    let mut file = std::fs::OpenOptions::new().write(true)
                        .create_new(true).open(&temporary)?;
                    file.write_all(text.as_bytes())?;
                    file.sync_all()?;
                    drop(file);
                    std::fs::rename(&temporary, &path)?;
                    info!("quest diagnostics saved to {}", path.display());
                    Ok(())
                })();
                let status = match result {
                    Ok(()) => 2,
                    Err(error) => {
                        warn!("quest diagnostics export failed: {error:#}");
                        3
                    }
                };
                ingress.export_status.store(status, Ordering::Release);
                ingress.exporting.store(false, Ordering::Release);
            });
        if let Err(error) = spawn {
            self.source.0.export_status.store(3, Ordering::Release);
            self.source.0.exporting.store(false, Ordering::Release);
            return Err(error.to_string());
        }
        Ok(())
    }
}

pub(crate) fn new_pair() -> (Source, History) {
    let (events, incoming) = bounded(256);
    let source = Source(Arc::new(Ingress {
        events,
        dropped: AtomicU64::new(0),
        next_id: AtomicU64::new(1),
        reload_generation: AtomicU64::new(0),
        update_packets_seen: AtomicU64::new(0),
        update_packets_sampled_out: AtomicU64::new(0),
        movement_sequence: AtomicU64::new(0),
        movement_admitted: AtomicU64::new(0),
        movement_coalesced: AtomicU64::new(0),
        movement_refused: AtomicU64::new(0),
        movement_written: AtomicU64::new(0),
        movement_last_written_seq: AtomicU64::new(0),
        movement_writes_since_correction: AtomicU64::new(0),
        npc_aliases: Mutex::new((VecDeque::with_capacity(MAX_NPC_ALIASES), 0)),
        origin: Instant::now(),
        #[cfg(feature = "dev")]
        run_id: format!("{:x}-{:x}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos()),
            std::process::id()),
        #[cfg(feature = "dev")]
        asset_profile: if std::env::var_os("WM_ASSET_MANIFEST").is_some() {
            let name = std::env::var("WM_ASSET_PROFILE").unwrap_or_else(|_| "base".into());
            (!name.is_empty() && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)))
                .then_some(name)
        } else {
            Some("stock-discovery".into())
        },
        #[cfg(feature = "dev")]
        exporting: AtomicBool::new(false),
        #[cfg(feature = "dev")]
        export_status: AtomicU8::new(0),
    }));
    let history = History {
        source: source.clone(),
        incoming,
        entries: VecDeque::with_capacity(1024),
        incidents: VecDeque::with_capacity(64),
        open_interactions: VecDeque::with_capacity(MAX_OPEN_INTERACTIONS),
        interaction_tracking_lost: 0,
        incidents_evicted: 0,
        received: 0,
        evicted: 0,
        source_gaps: 0,
        #[cfg(feature = "hotpatch")]
        last_observed_patch_digest: None,
    };
    (source, history)
}

pub(crate) fn install(app: &mut App) {
    let (source, history) = new_pair();
    app.insert_resource(source)
        .init_resource::<ScriptCursor>()
        .insert_resource(history)
        .add_systems(Last, (poll_script_diagnostics, poll_replica, poll_audio, poll_movement, poll_scene, poll_frame_time, poll_asset_failures, drain).chain());
}

fn poll_asset_failures(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    events: Option<Res<Messages<bevy::asset::UntypedAssetLoadFailedEvent>>>,
    time: Res<Time>,
    mut cursor: Local<bevy::ecs::message::MessageCursor<bevy::asset::UntypedAssetLoadFailedEvent>>,
    mut window: Local<(f64, u64, u64)>,
) {
    let Some(events) = events else { return };
    let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
    let now = time.elapsed_secs_f64();
    if window.0 == 0.0 {
        window.0 = now;
    }
    let missed = cursor.missed_messages(&events);
    let mut failures_this = 0u64;
    for failure in cursor.read(&events) {
        failures_this += 1;
        let extension = failure.path.path().extension().and_then(|s| s.to_str()).unwrap_or("");
        let family = if extension.eq_ignore_ascii_case("m2") || extension.eq_ignore_ascii_case("skin") {
            AssetFamily::Model
        } else if extension.eq_ignore_ascii_case("wmo") || extension.eq_ignore_ascii_case("adt") {
            AssetFamily::World
        } else if extension.eq_ignore_ascii_case("blp") || extension.eq_ignore_ascii_case("png") {
            AssetFamily::Texture
        } else if extension.eq_ignore_ascii_case("wgsl") {
            AssetFamily::Shader
        } else if extension.eq_ignore_ascii_case("ogg") || extension.eq_ignore_ascii_case("wav")
            || extension.eq_ignore_ascii_case("mp3") {
            AssetFamily::Audio
        } else if extension.eq_ignore_ascii_case("xml") || extension.eq_ignore_ascii_case("lua") {
            AssetFamily::Ui
        } else {
            AssetFamily::Other
        };
        use bevy::asset::AssetLoadError as E;
        let cause = match &failure.error {
            E::AssetReaderError(_) => AssetFailure::Reader,
            E::MissingAssetLoader { .. } | E::MissingAssetLoaderForExtension(_)
            | E::MissingAssetLoaderForTypeName(_) | E::MissingAssetLoaderForTypeIdError(_) =>
                AssetFailure::LoaderMissing,
            E::AssetLoaderError(_) => AssetFailure::LoaderError,
            E::AssetLoaderPanic { .. } => AssetFailure::LoaderPanic,
            _ => AssetFailure::Other,
        };
        source.observe(scope, None, Stage::AssetFailure { family, cause });
    }
    if missed > 0 {
        source.observe(scope, None, Stage::AssetEventsLost { count: missed as u64 });
    }
    window.1 = window.1.saturating_add(failures_this);
    window.2 = window.2.saturating_add(missed as u64);
    if now - window.0 >= 1.0 {
        let window_ms = ((now - window.0) * 1000.0).round() as u64;
        source.observe(scope, None, Stage::AssetWindow {
            failures: window.1,
            missed: window.2,
            window_ms: window_ms.max(1),
        });
        window.0 = now;
        window.1 = 0;
        window.2 = 0;
    }
}

#[derive(Default)]
struct FrameWindow {
    start: f64,
    frames: u32,
    sum_us: u64,
    max_us: u64,
}

fn poll_frame_time(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    time: Res<Time<bevy::time::Real>>,
    mut window: Local<FrameWindow>,
) {
    let micros = time.delta().as_micros().min(u64::MAX as u128) as u64;
    window.frames = window.frames.saturating_add(1);
    window.sum_us = window.sum_us.saturating_add(micros);
    window.max_us = window.max_us.max(micros);
    let now = time.elapsed_secs_f64();
    if now - window.start >= 1.0 {
        let mean_us = window.sum_us / u64::from(window.frames);
        source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
            Stage::FrameWindow { frames: window.frames, mean_us, max_us: window.max_us });
        *window = FrameWindow { start: now, ..Default::default() };
    }
}

fn poll_scene(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    cameras: Query<(), With<benilla_world::view::WorldCamera>>,
    rigs: Query<(), With<benilla_world::rig_anim::RigPose>>,
    time: Res<Time>,
    mut last: Local<Option<(u32, u32)>>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    let current = (
        cameras.iter().count().min(u32::MAX as usize) as u32,
        rigs.iter().count().min(u32::MAX as usize) as u32,
    );
    if *last != Some(current) {
        *last = Some(current);
        source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
            Stage::SceneSummary { world_cameras: current.0, animated_rigs: current.1 });
    }
}

fn poll_movement(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    input: Option<Res<crate::wrath::WrathInput>>,
    time: Res<Time>,
    mut last: Local<Option<(u64, u64, u64, u64, u64, u64)>>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    let (admitted, coalesced, refused, written) = source.movement_counters();
    // Queue depth and oldest movement age are read from the live outbox without changing it.
    let mut queue_depth = 0u64;
    let mut oldest_age_ms = 0u64;
    if let Some(input) = input.as_ref() {
        if let Ok(queue) = input.0.lock() {
            let mut oldest: Option<std::time::Instant> = None;
            for entry in queue.iter() {
                if entry.movement {
                    queue_depth += 1;
                    if let Some(enqueued) = entry.enqueued_at {
                        oldest = Some(match oldest {
                            Some(current) if current <= enqueued => current,
                            _ => enqueued,
                        });
                    }
                }
            }
            if let Some(oldest) = oldest {
                oldest_age_ms = oldest.elapsed().as_millis().min(u64::MAX as u128) as u64;
            }
        }
    }
    let current = (admitted, coalesced, refused, written, queue_depth, oldest_age_ms);
    if let Some(previous) = *last {
        let deltas = (
            admitted.saturating_sub(previous.0),
            coalesced.saturating_sub(previous.1),
            refused.saturating_sub(previous.2),
            written.saturating_sub(previous.3),
        );
        let depth_changed = queue_depth != previous.4;
        if deltas.0 > 0 || deltas.1 > 0 || deltas.2 > 0 || deltas.3 > 0 || depth_changed {
            *last = Some(current);
            source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
                Stage::MovementPressure {
                    baseline: false,
                    admitted: deltas.0,
                    coalesced: deltas.1,
                    refused: deltas.2,
                    written: deltas.3,
                    queue_depth,
                    oldest_age_ms,
                });
        } else {
            *last = Some(current);
        }
    } else {
        *last = Some(current);
        // First sample: emit baseline deltas (current counts) so evidence exists.
        source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
            Stage::MovementPressure {
                baseline: true,
                admitted: 0,
                coalesced: 0,
                refused: 0,
                written: 0,
                queue_depth,
                oldest_age_ms,
            });
    }
}

fn poll_audio(
    source: Res<Source>,
    output: Option<NonSend<crate::sound::SoundOutput>>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    time: Res<Time>,
    mut last: Local<Option<(bool, u32, u64, u64, u64)>>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    let Some(output) = output else { return };
    let current = (
        output.mixer.is_some(),
        output.live_voices().min(u32::MAX as usize) as u32,
        output.voices_denied,
        output.voices_stolen,
        output.copies_dropped,
    );
    if *last != Some(current) {
        let previous = *last;
        *last = Some(current);
        let denied_delta = current.2.saturating_sub(previous.map_or(0, |p| p.2));
        let stolen_delta = current.3.saturating_sub(previous.map_or(0, |p| p.3));
        let copies_delta = current.4.saturating_sub(previous.map_or(0, |p| p.4));
        let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
        source.observe(scope, None, Stage::AudioMixer {
            present: current.0,
            voices: current.1,
            denied: denied_delta,
            stolen: stolen_delta,
            copies_dropped: copies_delta,
        });
    }
}

fn poll_replica(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    time: Res<Time>,
    mut last: Local<Option<(u64, u64, u64)>>,
    mut next_poll: Local<f64>,
) {
    let Some(view) = view else { return };
    if view.phase.is_none() {
        *last = None;
        return;
    }
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    let current = (view.session_epoch, view.world_generation, view.state_revision);
    if *last != Some(current) {
        *last = Some(current);
        source.observe(Some((current.0, current.1)), None,
            Stage::ReplicaRevision { revision: current.2 });
    }
}

fn poll_script_diagnostics(
    script: Option<NonSend<benilla_ui::script::UiScript>>,
    source: Res<Source>,
    mut cursor: ResMut<ScriptCursor>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    time: Res<Time>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    if let Some(script) = script {
        let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
        source.capture_script_counts(scope, &mut cursor, script.session(), &script.diagnostic_counters());
        let frames = [
            (UiFrame::Questgiver, "QuestFrame"),
            (UiFrame::QuestLog, "QuestLogFrame"),
            (UiFrame::Merchant, "MerchantFrame"),
            (UiFrame::Loot, "LootFrame"),
            (UiFrame::SpellBook, "SpellBookFrame"),
            (UiFrame::Character, "CharacterFrame"),
            (UiFrame::GameMenu, "GameMenuFrame"),
        ];
        for (index, (frame, name)) in frames.into_iter().enumerate() {
            let visible = script.frame_visibility(name);
            if cursor.frames[index] != Some(visible) {
                cursor.frames[index] = Some(visible);
                source.observe(scope, None, Stage::UiFrame {
                    frame,
                    visible,
                    vm_session: script.session(),
                });
            }
        }
    }
}

pub(crate) fn drain(mut history: ResMut<History>) {
    drain_history(&mut history);
}

pub(crate) fn drain_history(history: &mut History) {
    for _ in 0..64 {
        let Ok(event) = history.incoming.try_recv() else { break };
        history.received += 1;
        let seq = history.received;
        if matches!(event.stage,
            Stage::NativeEventsLost { .. } | Stage::ScriptRowsLost { .. }
                | Stage::ScriptRowsEvicted { .. } | Stage::AssetEventsLost { .. })
        {
            history.source_gaps = history.source_gaps.saturating_add(1);
        }
        #[cfg(feature = "hotpatch")]
        if let Stage::Hotpatch { event: PatchEvent::Activated { digest, .. } } = event.stage {
            history.last_observed_patch_digest = digest;
        }

        // Track bounded open interactions for lifecycle retirement and server outcome correlation.
        if event.subject.interaction != 0 {
            match event.stage {
                Stage::Intent => {
                    if !history
                        .open_interactions
                        .iter()
                        .any(|open| open.subject.interaction == event.subject.interaction)
                    {
                        if history.open_interactions.len() >= MAX_OPEN_INTERACTIONS {
                            history.open_interactions.pop_front();
                            history.interaction_tracking_lost =
                                history.interaction_tracking_lost.saturating_add(1);
                        }
                        history.open_interactions.push_back(OpenInteraction {
                            subject: event.subject,
                            state: OpenState::Local,
                        });
                    }
                }
                Stage::Delivery(CommandDelivery::Written) => {
                    if let Some(open) = history
                        .open_interactions
                        .iter_mut()
                        .find(|open| open.subject.interaction == event.subject.interaction)
                    {
                        open.state = OpenState::Written {
                            seq,
                            elapsed_ms: event.elapsed_ms,
                        };
                    }
                }
                Stage::GuardRefused
                | Stage::NoWorld
                | Stage::AlreadyPending
                | Stage::TrackingCapacity
                | Stage::ChannelDisconnected
                | Stage::Delivery(CommandDelivery::Rejected(_))
                | Stage::Delivery(CommandDelivery::WriteFailed) => {
                    history
                        .open_interactions
                        .retain(|open| open.subject.interaction != event.subject.interaction);
                }
                _ => {}
            }
        }

        // On disconnect or world transfer, retire open interactions for the old scope.
        match event.stage {
            Stage::SessionEnded { .. }
            | Stage::DomainEvent {
                event: DomainEvent::PlayerDisconnected,
            } => {
                history.retire_scope(None, event.stage, seq);
            }
            Stage::WorldTransfer
            | Stage::DomainEvent {
                event: DomainEvent::Worldport,
            } => {
                // WorldTransfer carries the new world_generation in event.subject.scope;
                // retire any open interaction from an older scope in the same session.
                let new_scope = event.subject.scope;
                let stale_scopes: Vec<_> = history
                    .open_interactions
                    .iter()
                    .filter_map(|open| {
                        match (open.subject.scope, new_scope) {
                            (Some((oe, ow)), Some((ne, nw))) if oe < ne || (oe == ne && ow < nw) => {
                                Some(Some((oe, ow)))
                            }
                            _ => None,
                        }
                    })
                    .collect();
                for stale in stale_scopes {
                    history.retire_scope(stale, event.stage, seq);
                }
            }
            _ => {}
        }

        // Resolve open interactions when a matching panel, quest-slot addition, or turn-in packet arrives,
        // and emit an AmbiguousServerOutcome incident if multiple prior writes match.
        if matches!(
            event.stage,
            Stage::QuestSlotAdded | Stage::QuestCompletePacket { .. }
        ) || is_panel_stage(event.stage)
        {
            let matching_indices: Vec<usize> = history
                .open_interactions
                .iter()
                .enumerate()
                .filter_map(|(idx, open)| {
                    let OpenState::Written { elapsed_ms, .. } = open.state else {
                        return None;
                    };
                    if open.subject.scope != event.subject.scope
                        || event.elapsed_ms < elapsed_ms
                        || event.elapsed_ms.saturating_sub(elapsed_ms) > 30_000
                    {
                        return None;
                    }
                    let matched = match event.stage {
                        Stage::QuestSlotAdded => {
                            open.subject.operation == Operation::Accept
                                && event.subject.quest.is_some()
                                && open.subject.quest == event.subject.quest
                        }
                        Stage::QuestCompletePacket { .. } => {
                            open.subject.operation == Operation::ChooseReward
                                && event.subject.quest.is_some()
                                && open.subject.quest == event.subject.quest
                        }
                        Stage::GreetingPanel
                        | Stage::DomainEvent {
                            event: DomainEvent::GossipMenu,
                        } => {
                            open.subject.operation == Operation::Open
                                && event.subject.npc_alias.is_some()
                                && open.subject.npc_alias == event.subject.npc_alias
                        }
                        Stage::DetailPanel | Stage::ProgressPanel | Stage::RewardPanel => {
                            ((open.subject.operation == Operation::Select
                                || (matches!(event.stage, Stage::RewardPanel)
                                    && open.subject.operation == Operation::RequestReward))
                                && event.subject.quest.is_some()
                                && open.subject.quest == event.subject.quest)
                                || (open.subject.operation == Operation::Open
                                    && event.subject.npc_alias.is_some()
                                    && open.subject.npc_alias == event.subject.npc_alias)
                        }
                        _ => false,
                    };
                    matched.then_some(idx)
                })
                .collect();

            if matching_indices.len() > 1 {
                let mut evidence: Vec<u64> = matching_indices
                    .iter()
                    .filter_map(|&idx| match history.open_interactions[idx].state {
                        OpenState::Written { seq: write_seq, .. } => Some(write_seq),
                        OpenState::Local => None,
                    })
                    .collect();
                evidence.push(seq);
                let first_subject = history.open_interactions[matching_indices[0]].subject;
                history.push_incident(
                    first_subject,
                    IncidentOutcome::AmbiguousServerOutcome,
                    IncidentConfidence::Ambiguous,
                    event.stage,
                    evidence,
                    false,
                );
            }
            for idx in matching_indices.into_iter().rev() {
                history.open_interactions.remove(idx);
            }
        }

        let gaps = history.has_telemetry_gaps();
        let outcome = match event.stage {
            Stage::GuardRefused | Stage::NoWorld | Stage::AlreadyPending
            | Stage::TrackingCapacity | Stage::ChannelDisconnected
            | Stage::Delivery(CommandDelivery::Rejected(_)) => Some((
                IncidentOutcome::LocalRefusal,
                IncidentConfidence::Exact,
                event.subject,
            )),
            Stage::Delivery(CommandDelivery::WriteFailed) => Some((
                IncidentOutcome::WriteUnconfirmed,
                IncidentConfidence::Unverified,
                event.subject,
            )),
            Stage::ServerQuestFailed { .. }
            | Stage::ServerQuestLogFull
            | Stage::ServerQuestInvalid { .. } => {
                // Correlate server quest refusals against eligible Written actions in the same scope.
                let candidates: Vec<(u64, Subject)> = history
                    .entries
                    .iter()
                    .filter_map(|(c_seq, candidate)| {
                        let written = matches!(
                            candidate.stage,
                            Stage::Delivery(CommandDelivery::Written)
                        );
                        let in_window = candidate.subject.scope.is_some()
                            && candidate.subject.scope == event.subject.scope
                            && candidate.elapsed_ms <= event.elapsed_ms
                            && event.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000;
                        if !written || !in_window {
                            return None;
                        }
                        let matches_key = if event.subject.quest.is_some() {
                            candidate.subject.quest == event.subject.quest
                                && matches!(
                                    candidate.subject.operation,
                                    Operation::Accept
                                        | Operation::Select
                                        | Operation::RequestReward
                                        | Operation::ChooseReward
                                )
                        } else {
                            matches!(
                                candidate.subject.operation,
                                Operation::Accept
                                    | Operation::Select
                                    | Operation::Open
                                    | Operation::RequestReward
                                    | Operation::ChooseReward
                            ) && (event.subject.npc_alias.is_none()
                                || candidate.subject.npc_alias.is_none()
                                || candidate.subject.npc_alias == event.subject.npc_alias)
                        };
                        matches_key.then_some((*c_seq, candidate.subject))
                    })
                    .collect();
                for (_, matched_subj) in &candidates {
                    history
                        .open_interactions
                        .retain(|open| open.subject.interaction != matched_subj.interaction);
                }
                let (confidence, attributed_subject) = match candidates.as_slice() {
                    [(_, subj)] => {
                        let conf = if gaps {
                            IncidentConfidence::Unverified
                        } else if event.subject.quest.is_some() && subj.npc_alias.is_some() {
                            IncidentConfidence::Strong
                        } else {
                            IncidentConfidence::Heuristic
                        };
                        (conf, *subj)
                    }
                    [] => (IncidentConfidence::Unmatched, event.subject),
                    many => (IncidentConfidence::Ambiguous, many[0].1),
                };
                Some((
                    IncidentOutcome::ServerReportedFailure,
                    confidence,
                    attributed_subject,
                ))
            }
            Stage::InventoryFailure { .. }
            | Stage::DomainEvent {
                event:
                    DomainEvent::LootError
                    | DomainEvent::VendorBuyFailed
                    | DomainEvent::VendorSellFailed
                    | DomainEvent::InventoryFailure,
            } => Some((
                IncidentOutcome::ServerReportedFailure,
                if gaps {
                    IncidentConfidence::Unverified
                } else {
                    IncidentConfidence::Heuristic
                },
                event.subject,
            )),
            Stage::SocketReadFailed | Stage::SocketWriteFailed | Stage::AssetFailure { .. }
            | Stage::ReducerRejected { .. } | Stage::GameplayRejected { .. }
            | Stage::SessionEnded { failed: true } | Stage::ScriptError { .. }
            | Stage::ScriptLoadError { .. } => Some((
                IncidentOutcome::ObservedFailure,
                IncidentConfidence::Exact,
                event.subject,
            )),
            Stage::AudioHealth {
                nonfinite_samples,
                voices_refused,
                underruns,
                overloads,
                ..
            } if nonfinite_samples > 0 || voices_refused > 0 || underruns > 0 || overloads > 0 =>
                Some((
                    IncidentOutcome::ObservedFailure,
                    IncidentConfidence::Exact,
                    event.subject,
                ))
            }
            Stage::NativeEventsLost { .. } | Stage::ScriptRowsLost { .. }
            | Stage::ScriptRowsEvicted { .. } | Stage::AssetEventsLost { .. } => Some((
                IncidentOutcome::TelemetryGap,
                IncidentConfidence::Exact,
                event.subject,
            )),
            #[cfg(feature = "hotpatch")]
            Stage::Hotpatch {
                event:
                    PatchEvent::PublicationRejected { .. }
                    | PatchEvent::CandidateRejected { .. }
                    | PatchEvent::FrameFailed { .. }
                    | PatchEvent::RetentionFailed { .. },
            } => Some((
                IncidentOutcome::ObservedFailure,
                IncidentConfidence::Exact,
                event.subject,
            )),
            _ => None,
        };
        if let Some((outcome, confidence, attributed_subject)) = outcome {
            let existing = history.incidents.iter_mut().rev().find(|incident| {
                same_script_row(incident.terminal_stage, event.stage)
            });
            let updated = if let Some(incident) = existing {
                incident.terminal_stage = event.stage;
                incident.telemetry_gap |= gaps;
                if incident.evidence.len() == 8 {
                    incident.evidence.remove(0);
                    incident.evidence_truncated = true;
                }
                incident.evidence.push(seq);
                true
            } else {
                false
            };
            if !updated {
                let (evidence, evidence_truncated) =
                    history.collect_interaction_evidence(attributed_subject.interaction, seq);
                history.push_incident(
                    attributed_subject,
                    outcome,
                    confidence,
                    event.stage,
                    evidence,
                    evidence_truncated,
                );
            }
        }
        if history.entries.len() == 1024 {
            history.entries.pop_front();
            history.evicted += 1;
        }
        history.entries.push_back((seq, event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{CommandDelivery, CommandRefusal};
    use benilla_ui::script::diagnostics::DiagnosticKind;

    fn drain_all(history: &mut History) {
        for _ in 0..8 {
            drain_history(history);
        }
    }

    #[test]
    fn full_ingress_drops_excess_without_blocking_gameplay_and_marks_gaps() {
        let (source, mut history) = new_pair();
        for _ in 0..300 {
            source.observe(Some((1, 1)), Some(783), Stage::QuestSlotAdded);
        }
        drain_all(&mut history);
        let (received, evicted, dropped) = history.counters();
        assert_eq!(received, 256, "bounded 256-entry ingress drains without blocking");
        assert_eq!(evicted, 0);
        assert_eq!(dropped, 44, "excess events increment dropped counter immediately");
        assert!(history.has_telemetry_gaps());
        #[cfg(feature = "dev")]
        {
            let slots = history.slot_correlations();
            assert!(
                slots.iter().any(|line| line.contains("trace has gaps")),
                "telemetry gap must qualify negative/correlation conclusions: {slots:?}"
            );
        }
    }

    #[test]
    fn local_refusal_incident_records_exact_confidence_and_evidence_chain() {
        let (source, mut history) = new_pair();
        let action = source.begin_with_npc(
            Some((3, 7)),
            Some(783),
            Some(0xF130_0000_0000_0042),
            Operation::Accept,
        );
        action.record(Stage::ChannelAdmitted);
        action.record(Stage::Delivery(CommandDelivery::Rejected(
            CommandRefusal::OutboxFull,
        )));
        drain_all(&mut history);

        assert_eq!(history.incidents.len(), 1);
        let incident = &history.incidents[0];
        assert_eq!(incident.outcome, IncidentOutcome::LocalRefusal);
        assert_eq!(incident.confidence, IncidentConfidence::Exact);
        assert_eq!(incident.evidence, vec![1, 2, 3]);
        assert_eq!(incident.subject.npc_alias, Some(1));
        assert!(!incident.evidence_truncated);
    }

    #[test]
    fn duplicate_same_npc_clicks_and_accepts_yield_ambiguous_correlation() {
        let (source, mut history) = new_pair();
        let npc = 0xF130_0000_0000_0099;
        let open_a = source.begin_with_npc(Some((1, 1)), None, Some(npc), Operation::Open);
        open_a.record(Stage::Delivery(CommandDelivery::Written));
        let open_b = source.begin_with_npc(Some((1, 1)), None, Some(npc), Operation::Open);
        open_b.record(Stage::Delivery(CommandDelivery::Written));
        source.observe_with_npc(Some((1, 1)), None, Some(npc), Stage::GreetingPanel);

        let accept_a = source.begin_with_npc(Some((1, 1)), Some(783), Some(npc), Operation::Accept);
        accept_a.record(Stage::Delivery(CommandDelivery::Written));
        let accept_b = source.begin_with_npc(Some((1, 1)), Some(783), Some(npc), Operation::Accept);
        accept_b.record(Stage::Delivery(CommandDelivery::Written));
        source.observe_with_npc(Some((1, 1)), Some(783), Some(npc), Stage::QuestSlotAdded);

        drain_all(&mut history);
        assert!(
            history.incidents.iter().any(|inc| {
                inc.outcome == IncidentOutcome::AmbiguousServerOutcome
                    && inc.confidence == IncidentConfidence::Ambiguous
            }),
            "duplicate eligible writes before a panel or slot change must record Ambiguous"
        );
        #[cfg(feature = "dev")]
        {
            assert!(
                history
                    .panel_correlations()
                    .iter()
                    .any(|line| line.contains("ambiguous")),
                "panel correlations must flag duplicate Open writes as ambiguous"
            );
            assert!(
                history
                    .slot_correlations()
                    .iter()
                    .any(|line| line.contains("ambiguous")),
                "slot correlations must flag duplicate Accept writes as ambiguous"
            );
        }
    }

    #[test]
    fn stale_world_replies_never_correlate_across_session_or_world_generation() {
        let (source, mut history) = new_pair();
        let old_accept =
            source.begin_with_npc(Some((1, 1)), Some(783), Some(0x1111), Operation::Accept);
        old_accept.record(Stage::Delivery(CommandDelivery::Written));
        // World transfer to generation 2 retires the old scope's open write as unverified.
        source.observe(Some((1, 2)), None, Stage::WorldTransfer);
        // A slot addition in the new world (1, 2) must not match the (1, 1) write.
        source.observe(Some((1, 2)), Some(783), Stage::QuestSlotAdded);
        drain_all(&mut history);

        assert!(
            history.incidents.iter().any(|inc| {
                inc.subject.interaction == old_accept.subject.interaction
                    && inc.outcome == IncidentOutcome::WriteUnconfirmed
                    && inc.confidence == IncidentConfidence::Unverified
            }),
            "world transfer after a write must retire the open action as WriteUnconfirmed/Unverified"
        );
        #[cfg(feature = "dev")]
        {
            let slots = history.slot_correlations();
            assert!(
                slots
                    .iter()
                    .any(|line| line.contains("without a retained write")),
                "new-world slot delta must not reuse old-world write: {slots:?}"
            );
        }
    }

    #[test]
    fn repeated_lua_rows_across_reload_reset_cursor_and_supersede_old_vm_projection() {
        let (source, mut history) = new_pair();
        let mut cursor = ScriptCursor::default();
        source.capture_script_counts(
            Some((1, 1)),
            &mut cursor,
            10,
            &[(1, DiagnosticKind::Error, 1)],
        );
        source.capture_script_counts(
            Some((1, 1)),
            &mut cursor,
            10,
            &[(1, DiagnosticKind::Error, 4)],
        );
        source.observe(Some((1, 1)), Some(783), Stage::QuestSlotAdded);
        source.observe(Some((1, 1)), Some(783), Stage::JournalProjection);

        // Simulate /reload: advance reload generation and start VM session 11.
        source.observe(Some((1, 1)), None, Stage::UiReload);
        source.advance_reload_generation();
        source.capture_script_counts(
            Some((1, 1)),
            &mut cursor,
            11,
            &[(1, DiagnosticKind::Error, 2)],
        );
        drain_all(&mut history);

        let script_incidents: Vec<_> = history
            .incidents
            .iter()
            .filter(|inc| matches!(inc.terminal_stage, Stage::ScriptError { .. }))
            .collect();
        assert_eq!(
            script_incidents.len(),
            2,
            "same VM row collapses into one incident; new VM session after reload starts a second"
        );
        assert_eq!(script_incidents[0].evidence.len(), 2);
        #[cfg(feature = "dev")]
        {
            let slots = history.slot_correlations();
            assert!(
                slots
                    .iter()
                    .any(|line| line.contains("superseded by reload")),
                "old VM projection must be marked superseded after reload: {slots:?}"
            );
        }
    }

    #[test]
    fn partial_socket_write_and_malformed_quest_packets_record_unconfirmed_and_rejected_incidents()
    {
        let (source, mut history) = new_pair();
        let accept =
            source.begin_with_npc(Some((1, 1)), Some(783), Some(0x2222), Operation::Accept);
        accept.record(Stage::Delivery(CommandDelivery::OutboxAdmitted));
        accept.record(Stage::Delivery(CommandDelivery::WriteFailed));
        source.observe(
            Some((1, 1)),
            None,
            Stage::GameplayRejected { opcode: 0x0188 },
        );
        drain_all(&mut history);

        assert!(history.incidents.iter().any(|inc| {
            inc.outcome == IncidentOutcome::WriteUnconfirmed
                && inc.confidence == IncidentConfidence::Unverified
        }));
        assert!(history.incidents.iter().any(|inc| {
            inc.outcome == IncidentOutcome::ObservedFailure
                && matches!(inc.terminal_stage, Stage::GameplayRejected { opcode: 0x0188 })
        }));
    }

    #[test]
    fn npc_aliases_and_redacted_snapshot_export_omit_raw_guids_and_reject_unknown_versions() {
        let (source, mut history) = new_pair();
        let raw_guid = 0xF130_0007_C300_BEEF_u64;
        let open = source.begin_with_npc(Some((1, 1)), None, Some(raw_guid), Operation::Open);
        open.record(Stage::Delivery(CommandDelivery::Written));
        source.observe_with_npc(Some((1, 1)), None, Some(raw_guid), Stage::ServerQuestLogFull);
        drain_all(&mut history);

        let serialized = serde_json::to_string(&history.entries.iter().collect::<Vec<_>>())
            .expect("serialize entries");
        assert!(
            !serialized.contains(&raw_guid.to_string())
                && !serialized.contains("f1300007c300beef")
                && !serialized.contains("F1300007C300BEEF"),
            "raw 64-bit NPC GUID must never appear in serialized diagnostic events"
        );
        assert_eq!(
            validate_snapshot_header(
                r#"{"schema":"rustwrath.interaction-diagnostics.v1","version":1}"#
            ),
            Ok(1)
        );
        assert_eq!(
            validate_snapshot_header(
                r#"{"schema":"rustwrath.interaction-diagnostics.v1","version":99}"#
            ),
            Err(SnapshotVersionError::UnsupportedVersion(99))
        );
        assert_eq!(
            validate_snapshot_header(r#"{"schema":"unknown.schema","version":1}"#),
            Err(SnapshotVersionError::UnsupportedSchema(
                "unknown.schema".to_string()
            ))
        );
    }

    #[test]
    fn q03_objective_progress_and_delayed_template_distinguish_awaiting_template_from_row_mismatch()
    {
        let (source, mut history) = new_pair();
        let scope = Some((1, 1));
        let npc = 0xF130_0000_0000_0077;

        // Quest 783: slot added, projected while template is still pending -> must report awaiting template, not mismatch.
        let accept = source.begin_with_npc(scope, Some(783), Some(npc), Operation::Accept);
        accept.record(Stage::Delivery(CommandDelivery::Written));
        source.observe(scope, Some(783), Stage::QuestSlotAdded);
        source.observe(scope, Some(783), Stage::JournalProjection);
        source.observe(scope, Some(783), Stage::JournalTemplatePending);
        source.observe(
            scope,
            Some(783),
            Stage::QuestSlotProgressed {
                counters: [4, 0, 0, 0],
                state: 0,
            },
        );
        drain_all(&mut history);

        #[cfg(feature = "dev")]
        {
            let slots = history.slot_correlations();
            assert!(
                slots.iter().any(|line| {
                    line.contains("Quest 783")
                        && line.contains("awaiting quest template")
                        && line.contains("counters=[4, 0, 0, 0]")
                }),
                "pending template must be reported as awaiting template with objective counters: {slots:?}"
            );
        }

        // Now SMSG_QUEST_QUERY_RESPONSE lands and the row appears in the journal model.
        source.observe(scope, Some(783), Stage::JournalProjection);
        source.observe(scope, Some(783), Stage::JournalRowInModel);
        drain_all(&mut history);

        #[cfg(feature = "dev")]
        {
            let slots = history.slot_correlations();
            assert!(
                slots.iter().any(|line| {
                    line.contains("Quest 783") && line.contains("row exists in journal model")
                }),
                "once template lands and row is in model, slot correlation must confirm row: {slots:?}"
            );
        }
    }

    #[test]
    fn q04_turn_in_requires_asynchronous_slot_removal_and_reward_deltas_not_packet_alone() {
        let (source, mut history) = new_pair();
        let scope = Some((2, 1));
        let npc = 0xF130_0000_0000_0088;

        let choose = source.begin_with_npc(scope, Some(7), Some(npc), Operation::ChooseReward);
        choose.record(Stage::Delivery(CommandDelivery::Written));
        // SMSG_QUESTGIVER_QUEST_COMPLETE arrives expecting xp=250, money=125, reward_items=1,
        // before the UPDATE_OBJECT / SMSG_ITEM_PUSH_RESULT deltas have landed.
        source.observe_with_npc(
            scope,
            Some(7),
            Some(npc),
            Stage::QuestCompletePacket {
                xp: 250,
                money: 125,
                reward_items: 1,
            },
        );
        drain_all(&mut history);

        #[cfg(feature = "dev")]
        {
            let turn_ins = history.turn_in_correlations();
            assert!(
                turn_ins.iter().any(|line| {
                    line.contains("Quest 7 turn-in")
                        && line.contains("incomplete turn-in projection")
                        && line.contains("quest slot removal")
                }),
                "SMSG_QUESTGIVER_QUEST_COMPLETE alone must never claim complete turn-in: {turn_ins:?}"
            );
        }

        // Now the asynchronous UPDATE_OBJECT and SMSG_ITEM_PUSH_RESULT deltas arrive in the same scope.
        source.observe(scope, Some(7), Stage::QuestSlotRemoved);
        source.observe(scope, None, Stage::PlayerXpChanged { delta: 250 });
        source.observe(scope, None, Stage::PlayerMoneyChanged { delta: 125 });
        source.observe(
            scope,
            None,
            Stage::RewardItemPushed {
                item_id: 60,
                count: 1,
            },
        );
        drain_all(&mut history);

        #[cfg(feature = "dev")]
        {
            let turn_ins = history.turn_in_correlations();
            assert!(
                turn_ins.iter().any(|line| {
                    line.contains("Quest 7 turn-in")
                        && line.contains("turn-in deltas verified")
                        && line.contains("items=1/1")
                }),
                "once slot removal, XP, money, and item push arrive, turn-in is verified: {turn_ins:?}"
            );
        }
    }
}

