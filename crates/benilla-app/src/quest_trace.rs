//! Host-owned interaction and client diagnostics, including worker-thread delivery results.
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use bevy::prelude::*;
use benilla_protocol_wrath::gameplay::{DecodeFailure, DecoderFamily};
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
#[cfg(feature = "dev")]
use std::sync::atomic::{AtomicBool, AtomicU8};

use crate::net::CommandDelivery;

/// Maximum open interactions tracked concurrently before recording tracking loss.
pub(crate) const MAX_OPEN_INTERACTIONS: usize = 64;
/// Maximum run-local NPC aliases retained in the alias ring.
const MAX_NPC_ALIASES: usize = 256;

/// Closed coarse reason for a rejected reducer call. `Other` is used when the reducer error is
/// untyped or cannot be safely classified without inspecting free-form text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum ReducerReason {
    Truncated,
    TrailingBytes,
    UnterminatedString,
    InvalidCount,
    InvalidValue,
    Other,
}

impl ReducerReason {
    pub(crate) fn from_decode_failure(reason: benilla_protocol_wrath::gameplay::DecodeFailure) -> Self {
        use benilla_protocol_wrath::gameplay::DecodeFailure as D;
        match reason {
            D::Truncated => Self::Truncated,
            D::TrailingBytes => Self::TrailingBytes,
            D::UnterminatedString => Self::UnterminatedString,
            D::InvalidCount => Self::InvalidCount,
            D::InvalidValue => Self::InvalidValue,
            D::Other => Self::Other,
        }
    }
}

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
    AbandonSlot { slot: u8 },
    ServerQuestShareResult { msg: u32 },
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
    UiReload { old_vm_session: Option<u64>, old_reload_generation: Option<u64>, new_reload_generation: Option<u64> },
    /// The new VM reported its identity after a reload load attempt. This is separate evidence
    /// from [`Stage::UiReload`]: the reload request may have run while the load itself failed.
    UiReloadLoaded { vm_session: Option<u64>, reload_generation: Option<u64> },
    ScriptRowsLost { vm_session: u64, count: u64 },
    ScriptRowsEvicted { vm_session: u64, count: u64 },
    SessionPhase { phase: SessionPhase, attempt: Option<u64>, map_id: Option<u32> },
    SessionEnded { kind: SessionEndKind },
    WorldTransfer {
        map_id: Option<u32>,
        from: Option<(u64, u64)>,
        to: Option<(u64, u64)>,
        retired_local: u64,
        retired_written: u64,
    },
    LoginMilestone { attempt: Option<u64>, milestone: LoginMilestone },
    ReplicaRevision { revision: u64 },
    NativeEventsLost { count: u64 },
    Cast {
        spell_id: Option<u32>,
        attempt: Option<u64>,
        wire_cast_count: Option<u32>,
        edge: CastEdge,
    },
    InventoryFailure { reason: u8 },
    /// A loot slot's authoritative removal (`SMSG_LOOT_REMOVED`). The item id is known only when
    /// the producer still holds the window row; the wire event itself carries the slot alone.
    LootSlotRemoved {
        slot: u32,
        item_id: Option<u32>,
        count: Option<u32>,
    },
    /// `SMSG_LOOT_MONEY_NOTIFY`: the amount is server-reported.
    LootMoneyNotify { amount: u32 },
    /// `SMSG_LOOT_CLEAR_MONEY`: the amount is not carried, only the bounded fact.
    LootMoneyCleared,
    /// A loot refusal. `reason` is the protocol's numeric loot error, never its message text.
    LootErrorObserved { reason: u8 },
    /// `SMSG_LOOT_RESPONSE` opened a loot window on a source.
    LootResponseOpened,
    /// `SMSG_LOOT_RELEASE_RESPONSE`: the server released the loot interaction.
    LootReleasedObserved,
    /// A non-quest item push (`SMSG_ITEM_PUSH_RESULT`), kept distinct from the quest-only
    /// `RewardItemPushed` semantics.
    ItemPushResult { item_id: u32, count: u32 },
    /// `SMSG_LIST_INVENTORY`: a vendor alias and the bounded row count, never the rows.
    VendorInventoryObserved { vendor_alias: Option<u32>, rows: u32 },
    /// `SMSG_BUY_ITEM` for a vendor row. The protocol event names the row slot, not the entry.
    VendorBuyResult {
        item_id: Option<u32>,
        count: u32,
        slot: Option<u32>,
    },
    /// `SMSG_BUY_FAILED` with its numeric `buy_result` reason.
    VendorBuyFailedObserved { item_id: Option<u32>, reason: u8 },
    /// `SMSG_SELL_ITEM`'s numeric refusal reason. The item identity is a raw GUID on the wire and
    /// is deliberately not recorded.
    VendorSellFailedObserved { reason: u8 },
    /// An observed equipment-slot move. Recorded only where the source exposes a numeric item
    /// entry alongside the slot; the wire update alone carries no item entry.
    EquipmentSlotChanged { item_id: u32, slot: u32 },
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
    FrameWindow { frames: u32, mean_us: u64, max_us: u64, window_ms: u64 },
    AssetFailure { family: AssetFamily, cause: AssetFailure },
    AssetEventsLost { count: u64 },
    AssetWindow { failures: u64, missed: u64, window_ms: u64 },
    PacketReceived { opcode: u16 },
    ReducerAccepted { opcode: u16 },
    ReducerRejected { opcode: u16, reason: ReducerReason },
    GameplayDecoded { opcode: u16 },
    /// A gameplay decoder rejected the packet body. `decoder` and `reason` are closed values and
    /// `remaining_bytes` is a length only; no body bytes, server text or decoder message is kept.
    GameplayRejected {
        opcode: u16,
        decoder: DecoderFamily,
        reason: DecodeFailure,
        remaining_bytes: Option<u32>,
    },
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
        ui_dispatch_id: u64,
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
        ui_dispatch_id: u64,
    },
    DomainEvent { event: DomainEvent },
    #[cfg(feature = "hotpatch")]
    Hotpatch { event: PatchEvent },
    Delivery(CommandDelivery),
}

/// Closed retention class for one diagnostic observation.
///
/// The class is intentionally independent of `DIAGNOSTIC_VECTOR_NAMES`: a vector says which
/// subsystem produced the row, while the class says how much of the bounded ring it may hold.
/// Per-vector eviction counts stay available for compatibility, but class loss is never inferred
/// from them.
///
/// The mapping is total over the closed [`Stage`] enum, so current producers cannot emit
/// `Unclassified`. The variant and its counters are kept for forward compatibility: a new
/// `Stage` variant forces an explicit mapping decision at compile time instead of silently
/// landing in a disposable bucket. Per-class loss counters therefore reconcile to zero
/// unclassified rows today, and a future class that cannot be resolved is still counted rather
/// than dropped outside the totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EvidenceClass {
    /// A user or gameplay action chain: intent, admission, delivery, cast, quest/loot/vendor/
    /// panel/UI interaction, lifecycle transition, and the failures that explain one.
    Action,
    /// A periodic health observation: frame, asset, scene, replica, movement pressure, audio
    /// health/mixer and UI frame visibility. Cheap to lose and expected to repeat.
    Health,
    /// Decode-adjacent transport evidence: packet receipt, reducer acceptance, gameplay decode.
    /// A rejection is classed `Action` because it directly explains a failed action.
    Protocol,
    /// High-volume UI pointer traffic: presses and clicks. Bounded separately so a long
    /// click-heavy session cannot displace core action/protocol evidence, while recent
    /// presses/clicks remain available for UI-error and quest-dispatch correlation.
    #[serde(rename = "ui_input")]
    UiInput,
    /// Fallback class. See the type-level note: current producers map exhaustively.
    Unclassified,
}

/// Number of evidence classes; fixed-size per-class counters derive from this.
pub(crate) const EVIDENCE_CLASS_COUNT: usize = 5;

impl EvidenceClass {
    pub(crate) const ALL: [EvidenceClass; EVIDENCE_CLASS_COUNT] = [
        EvidenceClass::Action,
        EvidenceClass::Health,
        EvidenceClass::Protocol,
        EvidenceClass::UiInput,
        EvidenceClass::Unclassified,
    ];

    pub(crate) const fn index(self) -> usize {
        match self {
            EvidenceClass::Action => 0,
            EvidenceClass::Health => 1,
            EvidenceClass::Protocol => 2,
            EvidenceClass::UiInput => 3,
            EvidenceClass::Unclassified => 4,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            EvidenceClass::Action => "action",
            EvidenceClass::Health => "health",
            EvidenceClass::Protocol => "protocol",
            EvidenceClass::UiInput => "ui_input",
            EvidenceClass::Unclassified => "unclassified",
        }
    }
}

/// Explicit evidence-class mapping for every producer stage.
///
/// Deliberate choices:
/// - `ScriptError`, `ScriptLoadError` and `ScriptWarning` are `Action`, never `Health`. A script
///   failure can explain a UI action, and the Lua diagnostic poller is change-driven rather than
///   periodic, so these rows are not disposable retention churn.
/// - `ReducerRejected` and `GameplayRejected` are `Action`: they directly explain why an action
///   did not land. `PacketReceived`, `ReducerAccepted` and `GameplayDecoded` are `Protocol`.
/// - `UiFrame` is `Health` (visibility is a periodic frame observation) while `UiPress` and
///   `UiClick` are `UiInput` (high-volume pointer traffic, bounded separately).
/// - Loss annotations (`NativeEventsLost`, `ScriptRowsLost`, `ScriptRowsEvicted`,
///   `AssetEventsLost`) are `Action`. They are the evidence that a gap exists; letting periodic
///   churn reclaim them first would hide telemetry loss.
/// - `AssetFailure` and `AssetWindow` are `Health` because the asset vector is explicitly a
///   bounded periodic health channel (failures plus a 30-second heartbeat); `AssetEventsLost` is
///   the protected loss annotation.
pub(crate) fn evidence_class_for_stage(stage: &Stage) -> EvidenceClass {
    match stage {
        // Interaction, quest, inventory, loot, vendor, session and failure evidence.
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
        | Stage::GreetingPanel
        | Stage::DetailPanel
        | Stage::ProgressPanel
        | Stage::RewardPanel
        | Stage::QuestSlotAdded
        | Stage::QuestSlotProgressed { .. }
        | Stage::QuestSlotRemoved
        | Stage::AbandonSlot { .. }
        | Stage::ServerQuestShareResult { .. }
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
        | Stage::ScriptError { .. }
        | Stage::ScriptLoadError { .. }
        | Stage::ScriptWarning { .. }
        | Stage::UiReload { .. }
        | Stage::UiReloadLoaded { .. }
        | Stage::ScriptRowsLost { .. }
        | Stage::ScriptRowsEvicted { .. }
        | Stage::SessionPhase { .. }
        | Stage::SessionEnded { .. }
        | Stage::WorldTransfer { .. }
        | Stage::LoginMilestone { .. }
        | Stage::NativeEventsLost { .. }
        | Stage::Cast { .. }
        | Stage::InventoryFailure { .. }
        | Stage::LootSlotRemoved { .. }
        | Stage::LootMoneyNotify { .. }
        | Stage::LootMoneyCleared
        | Stage::LootErrorObserved { .. }
        | Stage::LootResponseOpened
        | Stage::LootReleasedObserved
        | Stage::ItemPushResult { .. }
        | Stage::VendorInventoryObserved { .. }
        | Stage::VendorBuyResult { .. }
        | Stage::VendorBuyFailedObserved { .. }
        | Stage::VendorSellFailedObserved { .. }
        | Stage::EquipmentSlotChanged { .. }
        | Stage::MovementCorrectionApplied { .. }
        | Stage::AssetEventsLost { .. }
        | Stage::ReducerRejected { .. }
        | Stage::GameplayRejected { .. }
        | Stage::SocketReadFailed
        | Stage::SocketWriteFailed
        | Stage::Outbound { .. }
        | Stage::DomainEvent { .. }
        | Stage::Delivery(_) => EvidenceClass::Action,

        // Periodic health channels.
        Stage::ReplicaRevision { .. }
        | Stage::AudioMixer { .. }
        | Stage::AudioHealth { .. }
        | Stage::MovementPressure { .. }
        | Stage::SceneSummary { .. }
        | Stage::FrameWindow { .. }
        | Stage::AssetFailure { .. }
        | Stage::AssetWindow { .. }
        | Stage::UiFrame { .. } => EvidenceClass::Health,

        // Decode-adjacent transport observations.
        Stage::PacketReceived { .. }
        | Stage::ReducerAccepted { .. }
        | Stage::GameplayDecoded { .. } => EvidenceClass::Protocol,

        // High-volume UI pointer traffic, bounded separately from core action evidence.
        Stage::UiPress { .. }
        | Stage::UiClick { .. } => EvidenceClass::UiInput,

        #[cfg(feature = "hotpatch")]
        Stage::Hotpatch { .. } => EvidenceClass::Action,
    }
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
    CandidateWithdrawn { attempt: u64 },
    PublicationRejected { attempt: Option<u64>, reason: HotpatchReason },
    CandidateRejected { attempt: Option<u64>, reason: HotpatchReason },
    Activated { attempt: Option<u64>, digest: Option<[u8; 20]> },
    FrameFailed { attempt: Option<u64>, reason: HotpatchReason },
    /// The failed active module was replaced by a retained predecessor.
    RestoredPrevious {
        failed_attempt: Option<u64>,
        restored_attempt: Option<u64>,
        digest: Option<[u8; 20]>,
    },
    /// The failed active module had no retained predecessor; the baseline pose is active.
    RestoreUnavailable { failed_attempt: Option<u64> },
    RetentionFailed { attempt: Option<u64>, reason: HotpatchReason },
}

/// Both attempt fields a hotpatch event can carry, for bounded correlation indexing.
#[cfg(feature = "hotpatch")]
fn patch_event_attempts(event: PatchEvent) -> [Option<u64>; 2] {
    match event {
        PatchEvent::CandidateStaged { attempt } | PatchEvent::CandidateWithdrawn { attempt } => {
            [Some(attempt), None]
        }
        PatchEvent::PublicationRejected { attempt, .. }
        | PatchEvent::CandidateRejected { attempt, .. }
        | PatchEvent::Activated { attempt, .. }
        | PatchEvent::FrameFailed { attempt, .. }
        | PatchEvent::RetentionFailed { attempt, .. }
        | PatchEvent::RestoreUnavailable {
            failed_attempt: attempt,
        } => [attempt, None],
        PatchEvent::RestoredPrevious {
            failed_attempt,
            restored_attempt,
            ..
        } => [failed_attempt, restored_attempt],
    }
}

#[cfg(feature = "hotpatch")]
fn patch_event_is_terminal_for(event: PatchEvent, attempt: u64) -> bool {
    match event {
        PatchEvent::CandidateStaged { .. } => false,
        PatchEvent::CandidateWithdrawn {
            attempt: withdrawn,
        } => withdrawn == attempt,
        PatchEvent::PublicationRejected {
            attempt: rejected, ..
        }
        | PatchEvent::CandidateRejected {
            attempt: rejected, ..
        }
        | PatchEvent::Activated {
            attempt: rejected, ..
        }
        | PatchEvent::FrameFailed {
            attempt: rejected, ..
        }
        | PatchEvent::RetentionFailed {
            attempt: rejected, ..
        } => rejected == Some(attempt),
        PatchEvent::RestoreUnavailable { failed_attempt } => failed_attempt == Some(attempt),
        PatchEvent::RestoredPrevious {
            failed_attempt,
            restored_attempt,
            ..
        } => failed_attempt == Some(attempt) || restored_attempt == Some(attempt),
    }
}

#[cfg(feature = "hotpatch")]
fn hotpatch_digest_label(digest: Option<[u8; 20]>) -> String {
    match digest {
        Some(bytes) => {
            let mut label = String::with_capacity(40);
            for byte in bytes {
                label.push_str(&format!("{byte:02x}"));
            }
            label
        }
        None => "unknown".to_string(),
    }
}

#[cfg(feature = "hotpatch")]
fn hotpatch_attempt_label(attempt: Option<u64>) -> String {
    attempt.map_or_else(|| "unknown".to_string(), |attempt| attempt.to_string())
}

/// Human-readable one-line terminal for one correlated attempt. Digest is a verified content
/// identity; an unavailable digest stays `unknown` rather than becoming a zero digest.
#[cfg(feature = "hotpatch")]
fn patch_event_summary(event: PatchEvent, attempt: u64) -> String {
    match event {
        PatchEvent::CandidateStaged { .. } => {
            format!("attempt {attempt}: staged, awaiting activation")
        }
        PatchEvent::CandidateWithdrawn { .. } => {
            format!("attempt {attempt}: withdrawn before activation")
        }
        PatchEvent::PublicationRejected {
            attempt: rejected,
            reason,
        } => format!(
            "attempt {attempt}: publication rejected (attempt={}, reason={reason:?})",
            hotpatch_attempt_label(rejected)
        ),
        PatchEvent::CandidateRejected { reason, .. } => format!(
            "attempt {attempt}: rejected ({reason:?}); active module retained"
        ),
        PatchEvent::Activated { digest, .. } => format!(
            "attempt {attempt}: activated (digest={})",
            hotpatch_digest_label(digest)
        ),
        PatchEvent::FrameFailed { reason, .. } => format!(
            "attempt {attempt}: frame failed ({reason:?}); no restore evidence retained"
        ),
        PatchEvent::RestoredPrevious {
            failed_attempt,
            restored_attempt,
            digest,
        } => {
            if failed_attempt == Some(attempt) {
                format!(
                    "attempt {attempt}: active frame failed; previous module restored (restored_attempt={}, digest={})",
                    hotpatch_attempt_label(restored_attempt),
                    hotpatch_digest_label(digest)
                )
            } else {
                format!(
                    "attempt {attempt}: restored as active module after attempt {} failed (digest={})",
                    hotpatch_attempt_label(failed_attempt),
                    hotpatch_digest_label(digest)
                )
            }
        }
        PatchEvent::RestoreUnavailable { failed_attempt } => format!(
            "attempt {attempt}: active frame failed; no previous module to restore (baseline pose; failed_attempt={})",
            hotpatch_attempt_label(failed_attempt)
        ),
        PatchEvent::RetentionFailed { reason, .. } => {
            format!("attempt {attempt}: artifact retention failed ({reason:?})")
        }
    }
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
pub(crate) enum SessionEndKind {
    LoggedOut,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum LoginMilestone {
    Connecting,
    Authenticating,
    Handshaking,
    WorldConnected,
    RealmListReceived,
    RealmListFailed,
    WorldAuthenticated,
    RosterReceived,
    RosterFailed,
    LoginQueued,
    /// The server refused the login (a realmd/world auth result byte was present).
    LoginRefused,
    /// The login failed without a server refusal category: a dial/transport failure, or a
    /// worker failure whose refusal code was not carried in the event.
    LoginFailed,
    CharacterAction,
    CharacterLoginFailed { result: u8 },
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
    Confirm,
    Abandon,
    Share,
    Observed,
    Loot,
    Equip,
    Buy,
    Sell,
}

/// Canonical diagnostic vector names, in stable order.  This is the single source of truth
/// shared by the recorder, the inspector and the export.  `unclassified` is the explicit
/// bucket for stages that are not (yet) attributed to a named vector.
pub(crate) const DIAGNOSTIC_VECTOR_NAMES: &[&str] = &[
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
    "unclassified",
];

/// Number of named diagnostic vectors, including `unclassified`.  Kept as a separate const so
/// fixed-size per-vector counters derive from the same registry that names the vectors.
pub(crate) const DIAGNOSTIC_VECTOR_COUNT: usize = DIAGNOSTIC_VECTOR_NAMES.len();

/// Bounded retained-event ring capacity (oldest entries are evicted first).
pub(crate) const RETAINED_CAPACITY: usize = 1024;

/// Maximum number of `Health` observations retained at once.
///
/// Health rows are cheap, repetitive observations. Capping them guarantees that at least
/// `RETAINED_CAPACITY - HEALTH_RETENTION_BUDGET` slots stay available for action and protocol
/// evidence, so a noisy frame/asset/replica cadence can never crowd an action chain out by
/// itself. A health row that is pushed out by this ceiling is counted as a class-aware eviction;
/// health is never suppressed at the producer.
pub(crate) const HEALTH_RETENTION_BUDGET: usize = 256;

/// Minimum number of `Health` observations protected from action/protocol pressure.
///
/// When the ring is full the oldest `Health` row is reclaimed first while retained health is
/// above this reserve. Below the reserve, action/protocol evidence is evicted so a burst of UI
/// clicks cannot leave the report with no frame, asset, scene, movement or audio context at all.
/// Health therefore always yields within its own bounded budget before action evidence yields,
/// and the per-class eviction counters expose exactly how much health was reclaimed.
pub(crate) const HEALTH_RETENTION_RESERVE: usize = 64;

/// Maximum number of `UiInput` (press/click) observations retained at once.
///
/// Pointer traffic is high-volume and change-driven, but it is not core action/protocol
/// evidence: a long click-heavy session must not displace cast/loot/login evidence. Capping it
/// at its own budget keeps recent presses/clicks available for UI-error and quest-dispatch
/// correlation while bounding their footprint. A UI input row that is pushed out by this
/// ceiling is counted as a class-aware eviction; pointer observations are never suppressed at
/// the producer.
pub(crate) const UI_INPUT_RETENTION_BUDGET: usize = 256;

/// Minimum number of `UiInput` observations protected from action/protocol pressure.
///
/// UI input reclaims its own oldest rows first. Below this reserve, an incoming
/// action/protocol/unclassified row may reclaim a UI input row above the reserve, but UI input
/// never consumes the protected core action share: an incoming pointer row that cannot be
/// inserted without displacing protected core evidence is refused and counted rather than
/// overwriting action evidence.
pub(crate) const UI_INPUT_RETENTION_RESERVE: usize = 32;

// Retention invariants: health has a real ceiling, the protected reserve fits inside it, and
// health can never consume the whole ring by itself.
const _: () = assert!(HEALTH_RETENTION_RESERVE <= HEALTH_RETENTION_BUDGET);
const _: () = assert!(HEALTH_RETENTION_BUDGET < RETAINED_CAPACITY);
// UI input has a real ceiling, the protected reserve fits inside it, and it cannot consume
// the whole ring by itself.
const _: () = assert!(UI_INPUT_RETENTION_RESERVE <= UI_INPUT_RETENTION_BUDGET);
const _: () = assert!(UI_INPUT_RETENTION_BUDGET < RETAINED_CAPACITY);

/// Maximum bytes in one inspector/export summary line.
const SNAPSHOT_LINE_BYTES: usize = 240;

/// Truncate one summary line at a UTF-8 boundary, bounded by [`SNAPSHOT_LINE_BYTES`].
///
/// The inspector's cached snapshot truncates with its own `cap`; the saved report writes the
/// same strings directly, so the outcome lines bound themselves here to keep the exported
/// report as bounded as the inspector.
fn bounded_line(mut line: String) -> String {
    if line.len() > SNAPSHOT_LINE_BYTES {
        let mut boundary = SNAPSHOT_LINE_BYTES;
        while boundary > 0 && !line.is_char_boundary(boundary) {
            boundary -= 1;
        }
        line.truncate(boundary);
    }
    line
}

/// Maximum sequence references and self-contained evidence notes per incident.
const MAX_INCIDENT_EVIDENCE: usize = 8;

/// Asset failure/missed summaries are emitted at most once per second while a real condition is
/// accumulating, plus a slow zero-value heartbeat so a healthy vector still has a denominator.
const ASSET_SCAN_WINDOW_SECS: f64 = 1.0;
const ASSET_HEARTBEAT_SECS: f64 = 30.0;

/// Frame intervals are aggregated over five seconds; mean/max are preserved for the window.
const FRAME_WINDOW_SECS: f64 = 5.0;

/// Replica revision changes are coalesced to the latest revision at most once per five seconds.
const REPLICA_WINDOW_SECS: f64 = 5.0;

/// Map a vector name to its index in `DIAGNOSTIC_VECTOR_NAMES`.
pub(crate) fn diagnostic_vector_index(name: &str) -> usize {
    match name {
        "quest_interactions" => 0,
        "quest_panel_correlation" => 1,
        "ui_frame_state" => 2,
        "ui_pointer_categories" => 3,
        "lua_diagnostic_counts" => 4,
        "session_lifecycle" => 5,
        "allowlisted_packets" => 6,
        "casts" => 7,
        "inventory_failures" => 8,
        "loot_and_vendor_events" => 9,
        "audio_mixer_state" => 10,
        "movement_queue_and_corrections" => 11,
        "scene_camera_and_rig_counts" => 12,
        "frame_intervals" => 13,
        "asset_load_failure_categories" => 14,
        "hotpatch" => 15,
        "unclassified" => 16,
        _ => 16,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiDispatchToken {
    pub id: u64,
    pub elapsed_ms: u64,
    pub scope: Option<(u64, u64)>,
    pub vm_session: u64,
    pub reload_generation: u64,
    pub button: UiButton,
    pub target: UiTarget,
    pub consumed: bool,
    pub visible: Option<bool>,
    pub clipped: bool,
    pub handler_invoked: bool,
    /// Bounded capture-time hit and handler facts for this edge.
    pub witness: UiHitEvidence,
}

/// Whether this retained edge could have fired `OnClick` for its button registration, and did.
///
/// `OnClick` follows `RegisterForClicks`: a press edge only for a `…ButtonDown` registration, a
/// release edge only for a `…ButtonUp`. A press row of an Up-only click is pointer traffic, not a
/// click antecedent. `engine_click` counts because the engine click action (a LootButton take)
/// runs the same slot without a Lua handler.
fn click_edge_can_claim(witness: &UiHitEvidence) -> bool {
    if !(witness.onclick_ran || witness.engine_click) {
        return false;
    }
    match witness.edge {
        UiEdge::Press => matches!(witness.clicks, UiClickMask::Down | UiClickMask::Both),
        UiEdge::Release => matches!(witness.clicks, UiClickMask::Up | UiClickMask::Both),
    }
}

/// Whether two retained edges are the press and the release of one physical click.
///
/// Cross-row identity is not retained, so the pair is the release row's press target matching
/// the press row's hit frame (kind and coarse family), on the same button, scope, VM and reload
/// generation inside the same two-second window. A pair that does not match stays two candidates,
/// which the caller reports as ambiguous rather than guessing.
fn same_physical_click(press: &UiDispatchToken, release: &UiDispatchToken) -> bool {
    press.witness.edge == UiEdge::Press
        && release.witness.edge == UiEdge::Release
        && press.button == release.button
        && press.scope == release.scope
        && press.vm_session == release.vm_session
        && press.reload_generation == release.reload_generation
        && release.elapsed_ms >= press.elapsed_ms
        && release.elapsed_ms - press.elapsed_ms <= 2_000
        && release.witness.press_target.is_some()
        && release.witness.press_target == press.witness.hit
}

#[derive(Debug)]
struct Ingress {
    events: Sender<Event>,
    dropped: AtomicU64,
    /// Per-vector producer-drop counters, indexed by `DIAGNOSTIC_VECTOR_NAMES`.
    dropped_by_vector: Vec<AtomicU64>,
    /// Per-evidence-class producer-drop / rejected-ingress counters. The sum (including
    /// `unclassified`) equals `dropped`; the class is resolved from the stage at ingress, before
    /// the event can be lost.
    dropped_by_class: [AtomicU64; EVIDENCE_CLASS_COUNT],
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
    ui_dispatch_counter: AtomicU64,
    ui_dispatches: Mutex<VecDeque<UiDispatchToken>>,
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
    /// Numeric item entry / template id. Never a raw item GUID.
    item: Option<u32>,
    /// Item count or quantity where the producer knows it.
    count: Option<u32>,
    /// Loot wire slot, vendor row slot or equipment slot index where relevant.
    slot: Option<u32>,
    operation: Operation,
    reload_generation: u64,
    ui_dispatch_id: Option<u64>,
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
pub(crate) struct SamplingCoverage {
    pub object_update_packets_seen: u64,
    pub object_update_packets_sampled: u64,
    pub object_update_packets_sampled_out: u64,
    pub object_update_sampling_interval: u64,
    pub movement_packets_coalesced: u64,
    pub movement_packets_refused: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DiagnosticCoverage {
    pub instrumented_paths: Vec<String>,
    pub compiled_out_paths: Vec<String>,
    pub event_driven_paths: Vec<String>,
    pub sampled_paths: Vec<String>,
    pub paths_with_producer_loss_counters: Vec<String>,
    /// Vectors whose retained observations were actually evicted from the ring.
    pub paths_with_retained_evictions: Vec<String>,
    pub not_yet_instrumented: Vec<String>,
    pub hotpatch_compiled: bool,
    pub export_and_inspector_compiled: bool,
    pub sampling: SamplingCoverage,
    /// Class-aware retention: exactly one row per `EvidenceClass`, including `unclassified`.
    pub evidence_classes: Vec<EvidenceClassCoverage>,
    /// Cross-check of the per-class counters against the global counters.
    pub evidence_class_totals: EvidenceClassTotals,
    pub health_retention_budget: u64,
    pub health_retention_reserve: u64,
    pub ui_input_retention_budget: u64,
    pub ui_input_retention_reserve: u64,
}

/// One evidence-class row of the retention summary.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EvidenceClassCoverage {
    pub class: &'static str,
    /// Rows of this class currently in the retained ring.
    pub retained: u64,
    /// Rows of this class evicted by the class-aware ring policy.
    pub evicted: u64,
    /// Rows of this class lost at the nonblocking ingress (producer drop / rejected ingress).
    pub dropped: u64,
}

/// Explicit reconciliation of the class counters with the global counters.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EvidenceClassTotals {
    pub retained: u64,
    pub evicted: u64,
    pub dropped: u64,
    pub global_received: u64,
    pub global_evicted: u64,
    pub global_dropped: u64,
    pub unclassified_retained: u64,
    pub unclassified_evicted: u64,
    pub unclassified_dropped: u64,
    /// `retained == retained_observations`, `evicted == global evicted`,
    /// `dropped == global dropped`, and `received == retained + evicted`.
    pub reconciles: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DiagnosticVectorEvidence {
    pub vector: String,
    pub instrumentation_available: bool,
    pub retained_observations: u64,
    pub latest_retained_sequence: Option<u64>,
    /// Events of this vector that were dropped from the retained ring by capacity eviction.
    /// Only vectors that actually lost retained observations carry a nonzero value.
    pub retained_evictions: u64,
    pub producer_drops: u64,
    pub sampled_out: Option<u64>,
    pub sampling_interval: Option<u64>,
    pub sampling_denominator: Option<u64>,
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
        | Stage::AbandonSlot { .. }
        | Stage::ServerQuestShareResult { .. }
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
        | Stage::UiReload { .. }
        | Stage::UiReloadLoaded { .. }
        | Stage::ScriptRowsLost { .. }
        | Stage::ScriptRowsEvicted { .. } => Some("lua_diagnostic_counts"),
        Stage::SessionPhase { .. }
        | Stage::SessionEnded { .. }
        | Stage::WorldTransfer { .. }
        | Stage::LoginMilestone { .. }
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
        | Stage::ItemPushResult { .. }
        | Stage::EquipmentSlotChanged { .. }
        | Stage::DomainEvent {
            event: DomainEvent::InventoryFailure | DomainEvent::ItemReceived,
        } => Some("inventory_failures"),
        Stage::LootSlotRemoved { .. }
        | Stage::LootMoneyNotify { .. }
        | Stage::LootMoneyCleared
        | Stage::LootErrorObserved { .. }
        | Stage::LootResponseOpened
        | Stage::LootReleasedObserved
        | Stage::VendorInventoryObserved { .. }
        | Stage::VendorBuyResult { .. }
        | Stage::VendorBuyFailedObserved { .. }
        | Stage::VendorSellFailedObserved { .. }
        | Stage::DomainEvent {
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
        _ => Some("unclassified"),
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

    pub(crate) fn producer_drops_for_vector(&self, vector: &str) -> u64 {
        let index = diagnostic_vector_index(vector);
        self.0.dropped_by_vector[index].load(Ordering::Relaxed)
    }

    /// Producer-drop / rejected-ingress loss for one evidence class. The class is resolved from
    /// the stage before the nonblocking enqueue is attempted, so a refused event is still counted
    /// under the class whose evidence was lost.
    pub(crate) fn producer_drops_for_class(&self, class: EvidenceClass) -> u64 {
        self.0.dropped_by_class[class.index()].load(Ordering::Relaxed)
    }

    pub(crate) fn update_packets_seen(&self) -> u64 {
        self.0.update_packets_seen.load(Ordering::Relaxed)
    }

    pub(crate) fn update_packets_sampled_out(&self) -> u64 {
        self.0.update_packets_sampled_out.load(Ordering::Relaxed)
    }

    pub(crate) fn movement_coalesced(&self) -> u64 {
        self.0.movement_coalesced.load(Ordering::Relaxed)
    }

    pub(crate) fn movement_refused(&self) -> u64 {
        self.0.movement_refused.load(Ordering::Relaxed)
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

    /// The current reload generation, without advancing it. A reload records the old value
    /// before disposal and the new value after the replacement VM reports its session.
    pub(crate) fn reload_generation(&self) -> u64 {
        self.0.reload_generation.load(Ordering::Acquire)
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
                item: None,
                count: None,
                slot: None,
                operation: Operation::Observed,
                reload_generation: self.0.reload_generation.load(Ordering::Acquire),
                ui_dispatch_id: None,
            },
        };
        trace.record(stage);
    }

    /// Observe inbound evidence with a numeric subject: an item entry, count and slot. The NPC is
    /// still mapped to a run-local alias; raw GUIDs never reach the ring.
    pub(crate) fn observe_with_subject(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        npc: Option<u64>,
        item: Option<u32>,
        count: Option<u32>,
        slot: Option<u32>,
        stage: Stage,
    ) {
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: 0,
                scope,
                quest,
                npc_alias: npc.and_then(|guid| self.npc_alias(guid)),
                item,
                count,
                slot,
                operation: Operation::Observed,
                reload_generation: self.0.reload_generation.load(Ordering::Acquire),
                ui_dispatch_id: None,
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
        self.begin_with_details(scope, quest, npc, None, None, None, operation)
    }

    /// Start a user-originated interaction carrying the numeric subject used for later
    /// correlation: item entry, count and slot. `npc` is the loot/vendor source GUID, mapped to a
    /// run-local alias here.
    #[allow(clippy::too_many_arguments)] // scope + quest + source + numeric subject + operation
    pub(crate) fn begin_with_details(
        &self,
        scope: Option<(u64, u64)>,
        quest: Option<u32>,
        npc: Option<u64>,
        item: Option<u32>,
        count: Option<u32>,
        slot: Option<u32>,
        operation: Operation,
    ) -> Interaction {
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                scope,
                quest,
                npc_alias: npc.and_then(|guid| self.npc_alias(guid)),
                item,
                count,
                slot,
                operation,
                reload_generation: self.0.reload_generation.load(Ordering::Acquire),
                ui_dispatch_id: self.plausible_ui_dispatch(scope),
            },
        };
        trace.record(Stage::Intent);
        trace
    }
}

impl Source {
    pub(crate) fn record_ui_press(
        &self,
        scope: Option<(u64, u64)>,
        button: UiButton,
        target: UiTarget,
        consumed: bool,
        egui_pointer_over_last_sample: Option<bool>,
        vm_session: u64,
    ) -> u64 {
        let id = self.0.ui_dispatch_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let reload_generation = self.0.reload_generation.load(Ordering::Acquire);
        let elapsed_ms = self.0.origin.elapsed().as_millis() as u64;
        let token = UiDispatchToken {
            id,
            elapsed_ms,
            scope,
            vm_session,
            reload_generation,
            button,
            target,
            consumed,
            visible: None,
            clipped: false,
            handler_invoked: consumed,
        };
        if let Ok(mut guard) = self.0.ui_dispatches.lock() {
            if guard.len() >= 8 {
                guard.pop_front();
            }
            guard.push_back(token);
        }
        self.observe(scope, None, Stage::UiPress {
            button,
            target,
            consumed,
            egui_pointer_over_last_sample,
            vm_session,
            ui_dispatch_id: id,
        });
        id
    }

    pub(crate) fn record_ui_click(
        &self,
        scope: Option<(u64, u64)>,
        button: UiButton,
        target: UiTarget,
        consumed: bool,
        visible: Option<bool>,
        clipped: bool,
        handler_invoked: bool,
        egui_pointer_over_last_sample: Option<bool>,
        vm_session: u64,
    ) -> u64 {
        let id = self.0.ui_dispatch_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let reload_generation = self.0.reload_generation.load(Ordering::Acquire);
        let elapsed_ms = self.0.origin.elapsed().as_millis() as u64;
        let token = UiDispatchToken {
            id,
            elapsed_ms,
            scope,
            vm_session,
            reload_generation,
            button,
            target,
            consumed,
            visible,
            clipped,
            handler_invoked,
        };
        if let Ok(mut guard) = self.0.ui_dispatches.lock() {
            if guard.len() >= 8 {
                guard.pop_front();
            }
            guard.push_back(token);
        }
        self.observe(scope, None, Stage::UiClick {
            button,
            target,
            consumed,
            visible,
            clipped,
            handler_invoked,
            egui_pointer_over_last_sample,
            vm_session,
            ui_dispatch_id: id,
        });
        id
    }

    /// The retained click antecedent for an intent minted now, or `None` when the evidence does
    /// not support exactly one.
    ///
    /// This is a host-side attribution, not a carried identity: it returns the single edge whose
    /// registration could have fired the click's handler in the same scope, VM and reload
    /// generation within two seconds. A press and the release of one physical click count once.
    /// An Up-only click is claimed by its release row, a Down-only click by its press row, and a
    /// `Both` registration by the release row once the pair is complete. Two clicks in the window
    /// stay `None` (ambiguous) rather than choosing the newest by adjacency.
    pub(crate) fn plausible_ui_dispatch(&self, scope: Option<(u64, u64)>) -> Option<u64> {
        let now = self.0.origin.elapsed().as_millis() as u64;
        let current_rg = self.0.reload_generation.load(Ordering::Acquire);
        let guard = self.0.ui_dispatches.lock().ok()?;
        let current_vm = guard.iter().map(|t| t.vm_session).max().unwrap_or(0);
        let eligible: Vec<UiDispatchToken> = guard
            .iter()
            .filter(|t| {
                t.scope == scope
                    && t.reload_generation == current_rg
                    && t.vm_session == current_vm
                    && t.elapsed_ms <= now
                    && now.saturating_sub(t.elapsed_ms) <= 2_000
                    // Capture-time hit: the hit frame took the point, so it was effectively
                    // visible and its effective bounds were resolvable on screen. `visible` is
                    // derived from `hit.is_some()`, so this also states the capture-time fact.
                    && t.visible == Some(true)
                    && t.witness
                        .hit
                        .is_some_and(|hit| hit.family != UiTarget::World)
                    && !matches!(
                        t.witness.bounds,
                        UiBoundsState::Unresolved | UiBoundsState::Empty | UiBoundsState::OffScreen
                    )
                    && click_edge_can_claim(&t.witness)
            })
            .copied()
            .collect();
        // One physical click can leave a press row and a release row; count it once, naming the
        // release edge. An unmatched press stays its own candidate, so a half-observed click is
        // still reportable rather than silently dropped.
        let mut paired_presses: Vec<u64> = Vec::new();
        for release in eligible.iter().filter(|t| t.witness.edge == UiEdge::Release) {
            let mut presses = eligible
                .iter()
                .filter(|t| same_physical_click(*t, release));
            if let (Some(press), None) = (presses.next(), presses.next()) {
                paired_presses.push(press.id);
            }
        }
        let candidates: Vec<u64> = eligible
            .iter()
            .filter(|t| !paired_presses.contains(&t.id))
            .map(|t| t.id)
            .collect();
        if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            None
        }
    }

    pub(crate) fn recent_ui_dispatches(&self) -> Vec<UiDispatchToken> {
        self.0
            .ui_dispatches
            .lock()
            .map(|g| g.iter().copied().collect())
            .unwrap_or_default()
    }
}

impl Interaction {
    pub(crate) fn record(&self, stage: Stage) {
        let ingress = &self.source.0;
        let vector_name = vector_for_stage(&stage);
        let vector_index = diagnostic_vector_index(vector_name.unwrap_or("unclassified"));
        let class_index = evidence_class_for_stage(&stage).index();
        let event = Event {
            version: 1,
            subject: self.subject,
            elapsed_ms: ingress.origin.elapsed().as_millis() as u64,
            stage,
        };
        if ingress.events.try_send(event).is_err() {
            ingress.dropped.fetch_add(1, Ordering::Relaxed);
            ingress.dropped_by_vector[vector_index].fetch_add(1, Ordering::Relaxed);
            ingress.dropped_by_class[class_index].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Explicit transport delivery recorder; transport code must use this instead of
    /// constructing `Stage::Delivery` ad hoc.
    ///
    /// Contract: one interaction must receive exactly one terminal delivery event. If a
    /// trace-only envelope wraps a receipt-bearing command, the receipt remains the
    /// authority and the envelope trace must not emit a second terminal.
    pub(crate) fn record_delivery(&self, delivery: crate::net::CommandDelivery) {
        self.record(Stage::Delivery(delivery));
    }

    /// The interaction's scope, exposed for stale-scope checks without making `Subject`
    /// public across the crate boundary. Returns `None` when no scope was carried.
    pub(crate) fn scope(&self) -> Option<(u64, u64)> {
        self.subject.scope
    }

    pub(crate) fn interaction_id(&self) -> u64 {
        self.subject.interaction
    }
}

#[derive(Debug, Clone, Serialize)]
struct Event {
    version: u8,
    subject: Subject,
    elapsed_ms: u64,
    stage: Stage,
}

/// Self-contained, bounded description of one incident evidence row.
///
/// The note is captured while the row is still retained, so the incident stays interpretable
/// after the ring evicts the referenced sequence. It deliberately carries only allowlisted
/// structured values: the sequence, the producer-relative time, the evidence class and the
/// bounded `Stage` enum. No packet body, free-form text, path or raw GUID is copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct EvidenceNote {
    seq: u64,
    elapsed_ms: u64,
    class: EvidenceClass,
    stage: Stage,
    /// True when this note was synthesized from bounded start metadata after the original
    /// event row was already evicted. False for notes captured directly from a retained row.
    #[serde(default)]
    recovered: bool,
    /// Alias for `recovered`, using the synthetic-provenance wording from the recovery path.
    #[serde(default)]
    synthetic: bool,
    /// Stable provenance label: `observed` or `recovered`.
    #[serde(default = "default_evidence_note_source")]
    source: &'static str,
}

fn default_evidence_note_source() -> &'static str {
    "observed"
}

impl EvidenceNote {
    fn from_event(seq: u64, event: &Event) -> Self {
        Self {
            seq,
            elapsed_ms: event.elapsed_ms,
            class: evidence_class_for_stage(&event.stage),
            stage: event.stage,
            recovered: false,
            synthetic: false,
            source: "observed",
        }
    }

    /// Synthesize a bounded opening note from retained start metadata after the original row has
    /// been evicted. The stored local boundary time is reported as-is; no stage, outcome or
    /// timestamp is fabricated beyond the stored metadata.
    fn recovered(seq: u64, elapsed_ms: u64, class: EvidenceClass, stage: Stage) -> Self {
        Self {
            seq,
            elapsed_ms,
            class,
            stage,
            recovered: true,
            synthetic: true,
            source: "recovered",
        }
    }

    /// Compact inspector rendering: `seq, elapsed_ms, class, stage`.
    fn label(&self) -> String {
        format!(
            "#{} +{}ms {} {:?}",
            self.seq,
            self.elapsed_ms,
            self.class.label(),
            self.stage
        )
    }
}

#[derive(Debug, Clone, Serialize)]
struct Incident {
    version: u8,
    subject: Subject,
    outcome: IncidentOutcome,
    confidence: IncidentConfidence,
    terminal_stage: Stage,
    evidence: Vec<u64>,
    /// Bounded self-contained copy of the same evidence rows, resolved before eviction.
    /// `evidence` stays the compatibility surface; this is the readable one.
    evidence_notes: Vec<EvidenceNote>,
    evidence_truncated: bool,
    evidence_notes_truncated: bool,
    telemetry_gap: bool,
    /// Additive provenance for the incident's opening evidence: `observed`,
    /// `recovered_from_start_metadata`, or `unavailable`.
    #[serde(default = "default_opening_note_source")]
    opening_note_source: &'static str,
    /// Additive boolean form of `opening_note_source == "recovered_from_start_metadata"`.
    #[serde(default)]
    opening_recovered: bool,
}

fn default_opening_note_source() -> &'static str {
    "unavailable"
}

/// The bounded evidence chain for one incident.
struct EvidenceChain {
    sequences: Vec<u64>,
    notes: Vec<EvidenceNote>,
    truncated: bool,
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
    /// Bounded start boundary for this open interaction. Captured when the `Stage::Intent`
    /// row enters the open set; not an unbounded history.
    start_seq: u64,
    start_elapsed_ms: u64,
}

/// One written cast interaction eligible to explain one native cast edge.
///
/// The local interaction id, the wire cast count and the native `attempt_serial` are distinct
/// identifier spaces. A candidate joins the first two when the transport carried them; the
/// native edge supplies the third. A candidate is scoped and is never reused across a session
/// or world generation.
#[derive(Debug, Clone, Copy)]
struct CastCandidate {
    subject: Subject,
    spell_id: u32,
    /// The wire cast count carried through admission, when the producing command had one.
    wire_cast_count: Option<u32>,
    written_elapsed_ms: u64,
    consumed: bool,
    /// Bounded request/admission start boundary for this written cast. When no `Requested`
    /// row was retained, this falls back to the admission or written row and
    /// `started_from_request` is false.
    started_seq: u64,
    started_elapsed_ms: u64,
    started_from_request: bool,
    started_from_admission: bool,
}

/// How one native cast edge maps onto the retained written local casts.
///
/// The local interaction id, the wire cast count and the native `attempt_serial` are distinct
/// identifier spaces. `candidate` is the single local write the edge can be attributed to;
/// `considered` names every retained local interaction that could plausibly explain the edge so
/// an ambiguous or conflicting edge can be reported without choosing one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CastAttribution {
    confidence: IncidentConfidence,
    /// Index into [`History::cast_candidates`] for the unique match.
    candidate: Option<usize>,
    /// First plausible candidate, used as the incident subject when the edge is ambiguous.
    first: Option<usize>,
    /// Non-zero local interaction ids that could explain the edge.
    considered: Vec<u64>,
}

impl CastAttribution {
    fn none(confidence: IncidentConfidence) -> Self {
        Self {
            confidence,
            candidate: None,
            first: None,
            considered: Vec::new(),
        }
    }
}

fn cast_confidence_rank(confidence: IncidentConfidence) -> u8 {
    match confidence {
        IncidentConfidence::Exact => 5,
        IncidentConfidence::Strong => 4,
        IncidentConfidence::Heuristic => 3,
        IncidentConfidence::Ambiguous => 2,
        IncidentConfidence::Unverified => 1,
        IncidentConfidence::Unmatched => 0,
    }
}

fn cast_confidence_label(confidence: IncidentConfidence) -> &'static str {
    match confidence {
        IncidentConfidence::Exact => "exact",
        IncidentConfidence::Strong => "strong",
        IncidentConfidence::Heuristic => "heuristic",
        IncidentConfidence::Ambiguous => "ambiguous",
        IncidentConfidence::Unverified => "unverified",
        IncidentConfidence::Unmatched => "unmatched",
    }
}

/// `wire` is a wire cast count, never the local interaction id.
fn cast_wire_label(wire: Option<u32>) -> String {
    wire.map_or_else(|| "unknown".to_string(), |wire| wire.to_string())
}

/// The native attempt serial, or an explicit unknown/multiple reading. Never the action id.
fn cast_attempt_label(attempts: &[u64]) -> String {
    match attempts {
        [] => "unknown".to_string(),
        [attempt] => attempt.to_string(),
        _ => "multiple".to_string(),
    }
}

fn cast_interaction_list(interactions: &[u64]) -> String {
    interactions
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether a cast stage is a native (server/worker-observed) edge rather than a local request,
/// admission, cancel or refusal. Only these edges are evidence about the server's outcome.
fn is_native_cast_edge(edge: CastEdge) -> bool {
    matches!(
        edge,
        CastEdge::Started
            | CastEdge::Delayed
            | CastEdge::Launched
            | CastEdge::Failed { .. }
            | CastEdge::Interrupted { .. }
            | CastEdge::ChannelStarted
            | CastEdge::ChannelUpdated
            | CastEdge::ChannelEnded
            | CastEdge::ScopeInvalidated
    )
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

/// An exact key comparison only when both sides carry it. A missing key is not an exact match.
fn slot_matches(action: Option<u32>, evidence: Option<u32>) -> bool {
    match (action, evidence) {
        (Some(a), Some(e)) => a == e,
        _ => true,
    }
}

fn item_matches(action: Option<u32>, evidence: Option<u32>) -> bool {
    match (action, evidence) {
        (Some(a), Some(e)) => a == e,
        // A known action item with unidentified evidence (or the reverse) stays eligible but can
        // never earn exact confidence; the time window and duplicate count decide.
        _ => true,
    }
}

fn alias_matches(action: Option<u32>, evidence: Option<u32>) -> bool {
    match (action, evidence) {
        (Some(a), Some(e)) => a == e,
        _ => true,
    }
}

/// Human label for the numeric inbound evidence, plus whether the evidence itself carries an
/// authoritative numeric identity strong enough for `strong` confidence.
fn loot_vendor_evidence_label(stage: &Stage) -> Option<(String, bool)> {
    Some(match stage {
        Stage::LootSlotRemoved { .. } => ("loot slot removed".to_owned(), true),
        Stage::LootMoneyNotify { .. } => ("loot money notified".to_owned(), true),
        Stage::LootMoneyCleared => ("loot money cleared".to_owned(), true),
        Stage::LootErrorObserved { reason } => {
            (format!("server reported loot error {reason}"), false)
        }
        Stage::LootResponseOpened => ("loot window observed".to_owned(), false),
        Stage::LootReleasedObserved => ("loot release observed".to_owned(), false),
        Stage::ItemPushResult { .. } => ("item push observed".to_owned(), true),
        Stage::VendorInventoryObserved { .. } => ("vendor inventory observed".to_owned(), false),
        Stage::VendorBuyResult { item_id, slot, .. } => (
            "vendor buy result observed".to_owned(),
            item_id.is_some() || slot.is_some(),
        ),
        Stage::VendorBuyFailedObserved { reason, .. } => {
            (format!("server reported buy failure {reason}"), false)
        }
        Stage::VendorSellFailedObserved { reason } => {
            (format!("server reported sell failure {reason}"), false)
        }
        Stage::EquipmentSlotChanged { .. } => ("equipment slot change observed".to_owned(), true),
        Stage::InventoryFailure { reason } => {
            (format!("server reported inventory failure {reason}"), false)
        }
        Stage::DomainEvent { event: DomainEvent::LootError } => {
            ("server reported loot error".to_owned(), false)
        }
        Stage::DomainEvent { event: DomainEvent::VendorBuyFailed } => {
            ("server reported buy failure".to_owned(), false)
        }
        Stage::DomainEvent { event: DomainEvent::VendorSellFailed } => {
            ("server reported sell failure".to_owned(), false)
        }
        Stage::DomainEvent { event: DomainEvent::InventoryFailure } => {
            ("server reported inventory failure".to_owned(), false)
        }
        _ => return None,
    })
}

/// A refusal carries its own authoritative numeric subject only for the vendor buy failure, whose
/// `item_entry` the server sends. Loot errors and sell failures do not carry an item entry.
fn candidate_subject_is_authoritative(stage: Stage) -> bool {
    matches!(stage, Stage::VendorBuyFailedObserved { item_id: Some(_), .. })
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

#[cfg(feature = "dev")]
#[derive(Clone, Debug)]
pub(crate) struct InspectorSnapshot {
    pub built_at: Instant,
    pub received: u64,
    pub evicted: u64,
    pub dropped: u64,
    pub source_gaps: u64,
    pub sampled_updates: u64,
    pub incidents: Vec<String>,
    pub actions: Vec<String>,
    pub panels: Vec<String>,
    pub slots: Vec<String>,
    pub turn_ins: Vec<String>,
    pub ui_errors: Vec<String>,
    pub ui_clicks: Vec<String>,
    /// Bounded per-click handler/action outcomes, newer than `ui_clicks` in wording but additive.
    pub ui_click_outcomes: Vec<String>,
    pub cast_chains: Vec<String>,
    pub loot_vendor: Vec<String>,
    pub quest_ui: Vec<String>,
    pub session: Vec<String>,
    pub movement: Vec<String>,
    pub movement_corrections: Vec<String>,
    pub audio: Vec<String>,
    pub assets: Vec<String>,
    pub hotpatch: Vec<String>,
    pub recent: Vec<String>,
    /// Bounded retention pressure and per-vector eviction summary.
    pub retention: Vec<String>,
}

#[derive(Resource)]
pub(crate) struct History {
    source: Source,
    incoming: Receiver<Event>,
    /// Retained rows in global sequence order. A `BTreeMap` keeps ordered iteration for every
    /// existing reader while allowing an interior (class-selected) eviction in O(log capacity)
    /// instead of scanning or shifting the ring.
    entries: BTreeMap<u64, Event>,
    /// Per-class sequence index. Membership is exact: every retained row is in exactly one set
    /// and is removed from it in the same operation that removes it from `entries`.
    class_sequences: [BTreeSet<u64>; EVIDENCE_CLASS_COUNT],
    incidents: VecDeque<Incident>,
    open_interactions: VecDeque<OpenInteraction>,
    /// Bounded written-cast candidates for native outcome correlation.
    cast_candidates: VecDeque<CastCandidate>,
    interaction_tracking_lost: u64,
    incidents_evicted: u64,
    received: u64,
    evicted: u64,
    /// Class-aware retained-ring evictions, indexed by `EvidenceClass`.
    /// The sum (including `unclassified`) must equal `evicted`.
    evicted_by_class: [u64; EVIDENCE_CLASS_COUNT],
    /// Retained-ring evictions attributed to each vector, indexed by `DIAGNOSTIC_VECTOR_NAMES`.
    /// The sum (including `unclassified`) must equal `evicted`.
    retained_evictions: [u64; DIAGNOSTIC_VECTOR_COUNT],
    source_gaps: u64,
    #[cfg(feature = "hotpatch")]
    last_observed_patch_digest: Option<[u8; 20]>,
    #[cfg(feature = "dev")]
    inspector_snapshot: Option<InspectorSnapshot>,
}

impl History {
    fn retained_by_class(&self, class: EvidenceClass) -> u64 {
        self.class_sequences[class.index()].len() as u64
    }

    /// Remove one retained row and attribute the loss to its class and vector in the same step.
    /// The class index is the only place retention membership is tracked, so the per-class counts
    /// can never drift from `entries`.
    fn evict_sequence(&mut self, seq: u64) -> bool {
        let Some(event) = self.entries.remove(&seq) else {
            return false;
        };
        let class = evidence_class_for_stage(&event.stage);
        self.class_sequences[class.index()].remove(&seq);
        let vector = vector_for_stage(&event.stage).unwrap_or("unclassified");
        let index = diagnostic_vector_index(vector);
        self.retained_evictions[index] = self.retained_evictions[index].saturating_add(1);
        self.evicted_by_class[class.index()] =
            self.evicted_by_class[class.index()].saturating_add(1);
        self.evicted = self.evicted.saturating_add(1);
        true
    }

    fn evict_oldest_of(&mut self, class: EvidenceClass) -> bool {
        match self.class_sequences[class.index()].iter().next().copied() {
            Some(seq) => self.evict_sequence(seq),
            None => false,
        }
    }

    /// Class-aware eviction under full-ring pressure.
    ///
    /// Victims are chosen by the incoming class:
    /// - incoming `Health`: reclaim the oldest health row (health never needs core permission).
    /// - incoming `UiInput`: reclaim the oldest UI input row first; only above the UI reserve
    ///   may it reclaim health above its reserve. It never touches protected core evidence; if
    ///   no evictable UI/health row exists the caller refuses insertion instead.
    /// - incoming `Action`/`Protocol`/`Unclassified`: reclaim health above its reserve, then UI
    ///   input above its reserve, then the oldest core action/protocol/unclassified row.
    fn evict_for_incoming(&mut self, incoming: EvidenceClass) -> bool {
        match incoming {
            EvidenceClass::Health => {
                if self.retained_by_class(EvidenceClass::Health) > 0 {
                    return self.evict_oldest_of(EvidenceClass::Health);
                }
                // Fall through to core only when no health row exists at all.
            }
            EvidenceClass::UiInput => {
                if self.retained_by_class(EvidenceClass::UiInput) > 0 {
                    return self.evict_oldest_of(EvidenceClass::UiInput);
                }
                let health = self.retained_by_class(EvidenceClass::Health);
                if health > HEALTH_RETENTION_RESERVE as u64 {
                    return self.evict_oldest_of(EvidenceClass::Health);
                }
                // No evictable UI/health row: refuse rather than touch core evidence.
                return false;
            }
            EvidenceClass::Action
            | EvidenceClass::Protocol
            | EvidenceClass::Unclassified => {
                let health = self.retained_by_class(EvidenceClass::Health);
                if health > HEALTH_RETENTION_RESERVE as u64 {
                    return self.evict_oldest_of(EvidenceClass::Health);
                }
                let ui = self.retained_by_class(EvidenceClass::UiInput);
                if ui > UI_INPUT_RETENTION_RESERVE as u64 {
                    return self.evict_oldest_of(EvidenceClass::UiInput);
                }
            }
        }
        // Core incoming (or health with no health row): reclaim the oldest core row.
        let mut oldest: Option<(EvidenceClass, u64)> = None;
        for class in [
            EvidenceClass::Action,
            EvidenceClass::Protocol,
            EvidenceClass::Unclassified,
        ] {
            if let Some(&seq) = self.class_sequences[class.index()].iter().next() {
                if oldest.map_or(true, |(_, candidate)| seq < candidate) {
                    oldest = Some((class, seq));
                }
            }
        }
        if let Some((_, seq)) = oldest {
            return self.evict_sequence(seq);
        }
        // Only health (or ui) rows remain: reclaim one above its reserve if possible, else refuse.
        if incoming == EvidenceClass::Health {
            return self.evict_oldest_of(EvidenceClass::Health);
        }
        if self.retained_by_class(EvidenceClass::Health) > HEALTH_RETENTION_RESERVE as u64 {
            return self.evict_oldest_of(EvidenceClass::Health);
        }
        if self.retained_by_class(EvidenceClass::UiInput) > UI_INPUT_RETENTION_RESERVE as u64 {
            return self.evict_oldest_of(EvidenceClass::UiInput);
        }
        false
    }

    /// Insert one retained row, applying the health ceiling and the global capacity policy.
    /// O(log capacity) per event: no scan of the ring, no shift of retained entries.
    fn insert_retained(&mut self, seq: u64, event: Event) {
        let class = evidence_class_for_stage(&event.stage);
        let incoming_health = class == EvidenceClass::Health;
        let incoming_ui = class == EvidenceClass::UiInput;
        if incoming_health {
            while self.class_sequences[EvidenceClass::Health.index()].len()
                >= HEALTH_RETENTION_BUDGET
            {
                if !self.evict_oldest_of(EvidenceClass::Health) {
                    break;
                }
            }
        }
        if incoming_ui {
            while self.class_sequences[EvidenceClass::UiInput.index()].len()
                >= UI_INPUT_RETENTION_BUDGET
            {
                if !self.evict_oldest_of(EvidenceClass::UiInput) {
                    break;
                }
            }
        }
        while self.entries.len() >= RETAINED_CAPACITY {
            if !self.evict_for_incoming(class) {
                // Refuse insertion rather than displace protected core evidence. Count the row
                // as a retained-ring eviction of its own class so the loss is explicit.
                let vector = vector_for_stage(&event.stage).unwrap_or("unclassified");
                let index = diagnostic_vector_index(vector);
                self.retained_evictions[index] = self.retained_evictions[index].saturating_add(1);
                self.evicted_by_class[class.index()] =
                    self.evicted_by_class[class.index()].saturating_add(1);
                self.evicted = self.evicted.saturating_add(1);
                return;
            }
        }
        self.class_sequences[class.index()].insert(seq);
        self.entries.insert(seq, event);
        debug_assert_eq!(
            self.class_sequences
                .iter()
                .map(|set| set.len() as u64)
                .fold(0u64, u64::saturating_add),
            self.entries.len() as u64,
            "per-class retained counts must sum to the retained observation count"
        );
        debug_assert_eq!(
            self.evicted_by_class
                .iter()
                .copied()
                .fold(0u64, u64::saturating_add),
            self.evicted,
            "per-class retained evictions must sum to the global evicted count"
        );
    }

    /// Whether retained evidence that can support or refute an action conclusion was lost.
    ///
    /// Health-only churn is exported in the per-class counters but does not by itself make a
    /// negative action conclusion unverified: periodic frame/asset/scene rows are not evidence
    /// for or against an interaction. Losing an action, protocol or unclassified row does.
    fn has_telemetry_gaps(&self) -> bool {
        self.evicted_by_class[EvidenceClass::Action.index()] > 0
            || self.evicted_by_class[EvidenceClass::Protocol.index()] > 0
            || self.evicted_by_class[EvidenceClass::Unclassified.index()] > 0
            || self.source_gaps > 0
            || self.interaction_tracking_lost > 0
            || self.source.0.dropped.load(Ordering::Relaxed) > 0
    }

    /// Whether any UI pointer (press/click) evidence was lost from the retained ring or ingress.
    /// Used to keep UI-error and quest-dispatch correlations truthful after pointer eviction.
    pub(crate) fn ui_input_rows_lost(&self) -> bool {
        self.evicted_by_class[EvidenceClass::UiInput.index()] > 0
            || self.source.producer_drops_for_class(EvidenceClass::UiInput) > 0
    }

    fn push_incident(
        &mut self,
        subject: Subject,
        outcome: IncidentOutcome,
        confidence: IncidentConfidence,
        terminal_stage: Stage,
        mut chain: EvidenceChain,
    ) {
        let telemetry_gap = self.has_telemetry_gaps();
        // Both views of the chain are bounded to the same eight rows. Keep the opening row and
        // the most recent rows; middle rows are dropped and the truncation is marked rather than
        // silently forgotten.
        let overflow = chain.sequences.len().saturating_sub(MAX_INCIDENT_EVIDENCE);
        if overflow > 0 {
            // Preserve index 0 (the opening note) and drop the oldest middle rows.
            chain.sequences.drain(1..=overflow);
            let note_overflow = chain.notes.len().saturating_sub(MAX_INCIDENT_EVIDENCE);
            if note_overflow > 0 {
                chain.notes.drain(1..=note_overflow);
            }
            chain.truncated = true;
        }
        debug_assert_eq!(
            chain.sequences.len(),
            chain.notes.len(),
            "incident evidence sequences and notes must stay aligned"
        );
        // A one-note chain whose only note is the terminal is not an opening: a server-only
        // observation (interaction 0) has no local opener. Do not present the terminal row as
        // the incident's opening just because it is first in the chain.
        let opening_note_present = chain
            .notes
            .first()
            .is_some_and(|note| chain.notes.len() > 1 || note.stage != terminal_stage);
        let opening_recovered = opening_note_present
            && chain.notes.first().is_some_and(|note| note.recovered);
        let opening_note_source = if !opening_note_present {
            "unavailable"
        } else if opening_recovered {
            "recovered_from_start_metadata"
        } else {
            "observed"
        };
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
            evidence: chain.sequences,
            evidence_notes: chain.notes,
            evidence_truncated: chain.truncated,
            evidence_notes_truncated: chain.truncated,
            telemetry_gap,
            opening_note_source,
            opening_recovered,
        });
    }

    /// Collect the bounded evidence chain for one interaction, resolving notes from the events
    /// themselves while they are still retained.
    ///
    /// The chain keeps the opening row and the most recent rows when it overflows: the opening
    /// note explains how the interaction started, and the closing rows explain what happened.
    /// The terminal row is the event currently being drained, which has not entered the ring yet
    /// and is therefore always available regardless of ring pressure.
    fn collect_interaction_evidence(
        &self,
        interaction_id: u64,
        terminal_seq: u64,
        terminal: &Event,
        opening_note: Option<EvidenceNote>,
    ) -> EvidenceChain {
        self.collect_evidence_with_terminal(
            interaction_id,
            EvidenceNote::from_event(terminal_seq, terminal),
            opening_note,
        )
    }

    /// As [`Self::collect_interaction_evidence`], but with a terminal note synthesized from
    /// bounded state when the terminal row is no longer available as a retained `Event`.
    /// `opening_note` is an optional note synthesized from bounded start metadata; it is
    /// prepended only when its sequence is not already retained in the chain.
    fn collect_evidence_with_terminal(
        &self,
        interaction_id: u64,
        terminal_note: EvidenceNote,
        opening_note: Option<EvidenceNote>,
    ) -> EvidenceChain {
        let terminal_seq = terminal_note.seq;
        if interaction_id == 0 {
            let mut sequences = Vec::new();
            let mut notes = Vec::new();
            if let Some(note) = opening_note {
                if !sequences.contains(&note.seq) {
                    sequences.push(note.seq);
                    notes.push(note);
                }
            }
            if !sequences.contains(&terminal_seq) {
                sequences.push(terminal_seq);
                notes.push(terminal_note);
            }
            let truncated = sequences.len() > MAX_INCIDENT_EVIDENCE;
            if truncated {
                let remove = sequences.len() - MAX_INCIDENT_EVIDENCE;
                sequences.drain(1..=remove);
                notes.drain(1..=remove);
            }
            return EvidenceChain {
                sequences,
                notes,
                truncated,
            };
        }
        let priors: Vec<(u64, &Event)> = self
            .entries
            .iter()
            .filter(|(seq, prior)| {
                prior.subject.interaction == interaction_id && **seq != terminal_seq
            })
            .map(|(prior_seq, prior)| (*prior_seq, prior))
            .collect();
        let prior_truncated = priors.len() > MAX_INCIDENT_EVIDENCE - 1;
        let selected: Vec<(u64, &Event)> = if prior_truncated {
            let mut selected = Vec::with_capacity(MAX_INCIDENT_EVIDENCE);
            selected.push(priors[0]);
            selected.extend_from_slice(
                &priors[priors.len() - (MAX_INCIDENT_EVIDENCE - 2)..],
            );
            selected
        } else {
            priors
        };
        let mut sequences: Vec<u64> = selected.iter().map(|(seq, _)| *seq).collect();
        let mut notes: Vec<EvidenceNote> = selected
            .iter()
            .map(|(seq, event)| EvidenceNote::from_event(*seq, event))
            .collect();
        // Prepend a recovered opening note only when the original row is no longer retained.
        if let Some(note) = opening_note {
            if !sequences.contains(&note.seq) {
                sequences.insert(0, note.seq);
                notes.insert(0, note);
            }
        }
        sequences.push(terminal_seq);
        notes.push(terminal_note);
        let mut truncated = prior_truncated;
        if sequences.len() > MAX_INCIDENT_EVIDENCE {
            let remove = sequences.len() - MAX_INCIDENT_EVIDENCE;
            sequences.drain(1..=remove);
            notes.drain(1..=remove);
            truncated = true;
        }
        debug_assert_eq!(sequences.len(), notes.len());
        EvidenceChain {
            sequences,
            notes,
            truncated,
        }
    }

    /// Build the evidence chain for a correlated server outcome whose earlier rows were captured
    /// as bounded open-interaction state rather than re-read from the ring.
    fn correlate_written_evidence(
        &self,
        writes: impl Iterator<Item = (u64, u64)>,
        terminal_seq: u64,
        terminal: &Event,
        opening_note: Option<EvidenceNote>,
    ) -> EvidenceChain {
        let mut sequences = Vec::with_capacity(MAX_INCIDENT_EVIDENCE);
        let mut notes = Vec::with_capacity(MAX_INCIDENT_EVIDENCE);
        if let Some(note) = opening_note {
            if !sequences.contains(&note.seq) {
                sequences.push(note.seq);
                notes.push(note);
            }
        }
        for (seq, elapsed_ms) in writes {
            if sequences.contains(&seq) {
                continue;
            }
            sequences.push(seq);
            notes.push(EvidenceNote {
                seq,
                elapsed_ms,
                class: EvidenceClass::Action,
                stage: Stage::Delivery(CommandDelivery::Written),
                recovered: false,
                synthetic: false,
                source: "observed",
            });
        }
        if !sequences.contains(&terminal_seq) {
            sequences.push(terminal_seq);
            notes.push(EvidenceNote::from_event(terminal_seq, terminal));
        }
        let mut truncated = false;
        if sequences.len() > MAX_INCIDENT_EVIDENCE {
            let remove = sequences.len() - MAX_INCIDENT_EVIDENCE;
            sequences.drain(1..=remove);
            notes.drain(1..=remove);
            truncated = true;
        }
        debug_assert_eq!(sequences.len(), notes.len());
        EvidenceChain {
            sequences,
            notes,
            truncated,
        }
    }

    fn retire_scope(
        &mut self,
        retired_scope: Option<(u64, u64)>,
        terminal: &Event,
        seq: u64,
    ) -> (u64, u64) {
        let terminal_stage = terminal.stage;
        // An old scope's written cast can never explain a native edge in the new world.
        self.cast_candidates.retain(|candidate| {
            !match (retired_scope, candidate.subject.scope) {
                (Some(target), Some(current)) => target == current,
                (None, _) => true,
                _ => false,
            }
        });
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
        let mut retired_local = 0u64;
        let mut retired_written = 0u64;
        for open in retired {
            let opening_note = if open.start_seq != 0
                && !self.entries.contains_key(&open.start_seq)
            {
                Some(EvidenceNote::recovered(
                    open.start_seq,
                    open.start_elapsed_ms,
                    EvidenceClass::Action,
                    Stage::Intent,
                ))
            } else {
                None
            };
            let chain = self.collect_interaction_evidence(
                open.subject.interaction,
                seq,
                terminal,
                opening_note,
            );
            match open.state {
                OpenState::Written { .. } => {
                    retired_written = retired_written.saturating_add(1);
                    self.push_incident(
                        open.subject,
                        IncidentOutcome::WriteUnconfirmed,
                        IncidentConfidence::Unverified,
                        terminal_stage,
                        chain,
                    );
                }
                OpenState::Local => {
                    retired_local = retired_local.saturating_add(1);
                    self.push_incident(
                        open.subject,
                        IncidentOutcome::LocalRefusal,
                        if gaps {
                            IncidentConfidence::Unverified
                        } else {
                            IncidentConfidence::Exact
                        },
                        terminal_stage,
                        chain,
                    );
                }
            }
        }
        (retired_local, retired_written)
    }

    /// A complete socket write of a cast interaction makes it a bounded candidate for one native
    /// cast edge. The wire cast count came from admission, when the command carried one.
    fn note_written_cast(&mut self, seq: u64, event: &Event) {
        let interaction = event.subject.interaction;
        if interaction == 0 {
            return;
        }
        if self
            .cast_candidates
            .iter()
            .any(|candidate| candidate.subject.interaction == interaction)
        {
            return;
        }
        let mut spell_id = None;
        let mut wire_cast_count = None;
        let mut started_seq = 0u64;
        let mut started_elapsed_ms = 0u64;
        let mut started_from_request = false;
        let mut started_from_admission = false;
        for (prior_seq, prior) in &self.entries {
            if prior.subject.interaction != interaction {
                continue;
            }
            if let Stage::Cast {
                spell_id: Some(id),
                wire_cast_count: count,
                edge,
                ..
            } = prior.stage
            {
                if !matches!(edge, CastEdge::Requested | CastEdge::Admitted) {
                    continue;
                }
                if spell_id.is_none() {
                    spell_id = Some(*id);
                }
                if count.is_some() && wire_cast_count.is_none() {
                    wire_cast_count = *count;
                }
                match edge {
                    CastEdge::Requested => {
                        if !started_from_request {
                            started_seq = *prior_seq;
                            started_elapsed_ms = prior.elapsed_ms;
                            started_from_request = true;
                            started_from_admission = false;
                        }
                    }
                    CastEdge::Admitted => {
                        if !started_from_request && !started_from_admission {
                            started_seq = *prior_seq;
                            started_elapsed_ms = prior.elapsed_ms;
                            started_from_admission = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        let Some(spell_id) = spell_id else { return };
        if started_seq == 0 {
            // The request row is already gone: use the admission/written boundary and mark
            // that this is not the original request row.
            started_seq = seq;
            started_elapsed_ms = event.elapsed_ms;
        }
        if self.cast_candidates.len() >= MAX_OPEN_INTERACTIONS {
            // A bounded ring: the oldest candidate is dropped, and the loss remains visible
            // through the interaction tracking-loss counter rather than a fabricated match.
            self.cast_candidates.pop_front();
            self.interaction_tracking_lost = self.interaction_tracking_lost.saturating_add(1);
        }
        self.cast_candidates.push_back(CastCandidate {
            subject: event.subject,
            spell_id,
            wire_cast_count,
            written_elapsed_ms: event.elapsed_ms,
            consumed: false,
            started_seq,
            started_elapsed_ms,
            started_from_request,
            started_from_admission,
        });
    }

    /// Match one native cast edge against written local casts in the same scope.
    ///
    /// Exact: the same wire cast count on both sides. Strong: exactly one written cast of the
    /// same spell in the window, with its admitted wire count and no conflicting native count.
    /// Heuristic: exactly one candidate but the authoritative count is missing on the local side
    /// (item uses). Ambiguous: more than one eligible candidate. Unmatched: none. This is a
    /// read-only query: [`Self::correlate_cast_edge`] consumes the unique match for a terminal
    /// failure. `include_consumed` is used by the inspector so a terminal edge that already
    /// consumed its candidate still names that local action.
    fn cast_attribution(&self, event: &Event, include_consumed: bool) -> CastAttribution {
        let Stage::Cast {
            spell_id: Some(spell),
            wire_cast_count: native_count,
            ..
        } = event.stage
        else {
            return CastAttribution::none(IncidentConfidence::Unmatched);
        };
        let scope = event.subject.scope;
        let window_ms = 30_000u64;
        // Exact count matches do not need the time window: the wire count is monotonic within a
        // session, so a late START/GO remains usable evidence. Spell/scope/time fallback does.
        let all: Vec<usize> = self
            .cast_candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                (include_consumed || !candidate.consumed)
                    && candidate.subject.scope == scope
                    && candidate.spell_id == spell
                    && candidate.written_elapsed_ms <= event.elapsed_ms
            })
            .map(|(index, _)| index)
            .collect();
        if all.is_empty() {
            return CastAttribution::none(IncidentConfidence::Unmatched);
        }
        let interactions = |indices: &[usize]| -> Vec<u64> {
            let mut ids: Vec<u64> = indices
                .iter()
                .map(|&index| self.cast_candidates[index].subject.interaction)
                .filter(|&interaction| interaction != 0)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        // Exact match on the authoritative wire count.
        if let Some(wanted) = native_count {
            let exact: Vec<usize> = all
                .iter()
                .copied()
                .filter(|&index| self.cast_candidates[index].wire_cast_count == Some(wanted))
                .collect();
            if exact.len() == 1 {
                let index = exact[0];
                return CastAttribution {
                    confidence: IncidentConfidence::Exact,
                    candidate: Some(index),
                    first: Some(index),
                    considered: interactions(&exact),
                };
            }
            if exact.len() > 1 {
                // Duplicate counts: the count cannot decide which local interaction owns the edge.
                return CastAttribution {
                    confidence: IncidentConfidence::Ambiguous,
                    candidate: None,
                    first: Some(exact[0]),
                    considered: interactions(&exact),
                };
            }
        }

        // No exact count match. Spell/scope/time fallback only.
        let within_window: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&index| {
                event
                    .elapsed_ms
                    .saturating_sub(self.cast_candidates[index].written_elapsed_ms)
                    <= window_ms
            })
            .collect();
        let mut eligible = within_window.clone();
        if native_count.is_some() {
            // A count present on both sides that differs is a conflict, not evidence.
            eligible.retain(|&index| self.cast_candidates[index].wire_cast_count.is_none());
        }

        if eligible.is_empty() {
            // Keep the conflicting same-spell writes visible as considered candidates; they are
            // explicitly not a unique match.
            return CastAttribution {
                confidence: IncidentConfidence::Unmatched,
                candidate: None,
                first: None,
                considered: interactions(&within_window),
            };
        }
        if eligible.len() == 1 {
            let index = eligible[0];
            // A local admission count plus a unique spell/scope/time is a strong lead when the
            // native edge omits the count (channel continuations). Without any local count the
            // item-use case is only heuristic.
            let local_count_present = self.cast_candidates[index].wire_cast_count.is_some();
            return CastAttribution {
                confidence: if local_count_present {
                    IncidentConfidence::Strong
                } else {
                    IncidentConfidence::Heuristic
                },
                candidate: Some(index),
                first: Some(index),
                considered: interactions(&eligible),
            };
        }
        CastAttribution {
            confidence: IncidentConfidence::Ambiguous,
            candidate: None,
            first: Some(eligible[0]),
            considered: interactions(&eligible),
        }
    }

    /// Correlate one native cast edge against written local casts in the same scope.
    ///
    /// Terminal failures consume a uniquely matched candidate; continuation edges (`Started`,
    /// `Delayed`, channel updates) never do.
    fn correlate_cast_edge(
        &mut self,
        event: &Event,
        consume: bool,
    ) -> (IncidentConfidence, Subject) {
        let attribution = self.cast_attribution(event, false);
        if consume {
            if let Some(index) = attribution.candidate {
                self.cast_candidates[index].consumed = true;
            }
        }
        let subject = attribution
            .candidate
            .or(attribution.first)
            .map(|index| self.cast_candidates[index].subject)
            .unwrap_or(event.subject);
        (attribution.confidence, subject)
    }

    /// Close a written cast that never received native evidence inside the observation window.
    /// This is explicitly unverified, never a failure: the missing START/GO may be a telemetry
    /// gap rather than a server refusal.
    fn expire_cast_windows(&mut self, now_ms: u64) {
        let window_ms = 30_000u64;
        let mut remaining = VecDeque::with_capacity(self.open_interactions.len());
        let mut expired = Vec::new();
        while let Some(open) = self.open_interactions.pop_front() {
            let is_cast = self
                .cast_candidates
                .iter()
                .any(|candidate| candidate.subject.interaction == open.subject.interaction);
            let old_written = matches!(
                open.state,
                OpenState::Written { elapsed_ms, .. }
                    if now_ms.saturating_sub(elapsed_ms) > window_ms
            );
            if is_cast && old_written {
                expired.push(open);
            } else {
                remaining.push_back(open);
            }
        }
        self.open_interactions = remaining;
        for open in expired {
            let candidate_data = self
                .cast_candidates
                .iter()
                .find(|candidate| candidate.subject.interaction == open.subject.interaction)
                .map(|candidate| {
                    (
                        candidate.started_seq,
                        candidate.started_elapsed_ms,
                        candidate.started_from_request,
                        candidate.started_from_admission,
                        candidate.spell_id,
                        candidate.wire_cast_count,
                    )
                });
            let opening_note = if let Some((
                started_seq,
                started_elapsed_ms,
                started_from_request,
                started_from_admission,
                spell_id,
                wire_cast_count,
            )) = candidate_data
            {
                if started_seq != 0 && !self.entries.contains_key(&started_seq) {
                    let stage = if started_from_request {
                        Stage::Cast {
                            spell_id: Some(spell_id),
                            attempt: None,
                            wire_cast_count,
                            edge: CastEdge::Requested,
                        }
                    } else if started_from_admission {
                        Stage::Cast {
                            spell_id: Some(spell_id),
                            attempt: None,
                            wire_cast_count,
                            edge: CastEdge::Admitted,
                        }
                    } else {
                        Stage::Delivery(CommandDelivery::Written)
                    };
                    Some(EvidenceNote::recovered(
                        started_seq,
                        started_elapsed_ms,
                        EvidenceClass::Action,
                        stage,
                    ))
                } else {
                    None
                }
            } else if open.start_seq != 0 && !self.entries.contains_key(&open.start_seq) {
                Some(EvidenceNote::recovered(
                    open.start_seq,
                    open.start_elapsed_ms,
                    EvidenceClass::Action,
                    Stage::Intent,
                ))
            } else {
                None
            };
            let chain = match open.state {
                OpenState::Written { seq, elapsed_ms } => self.collect_evidence_with_terminal(
                    open.subject.interaction,
                    EvidenceNote {
                        seq,
                        elapsed_ms,
                        class: EvidenceClass::Action,
                        stage: Stage::Delivery(CommandDelivery::Written),
                        recovered: false,
                        synthetic: false,
                        source: "observed",
                    },
                    opening_note,
                ),
                OpenState::Local => {
                    let mut sequences = Vec::new();
                    let mut notes = Vec::new();
                    if let Some(note) = opening_note {
                        if !sequences.contains(&note.seq) {
                            sequences.push(note.seq);
                            notes.push(note);
                        }
                    }
                    EvidenceChain {
                        sequences,
                        notes,
                        truncated: false,
                    }
                }
            };
            self.push_incident(
                open.subject,
                IncidentOutcome::WriteUnconfirmed,
                IncidentConfidence::Unverified,
                Stage::Delivery(CommandDelivery::Written),
                chain,
            );
        }
    }

    /// Bounded inspector lines for the most recent cast/channel chains. Each line names the local
    /// action the native edge was attributed to, the requested spell, the wire cast count, the
    /// native attempt and the terminal edge. The local action id, the wire count and the native
    /// attempt stay distinct: a native-only chain never claims action 0.
    ///
    /// A chain is anchored by its non-zero local interaction when the edge can be attributed to
    /// one. Two rapid same-spell casts stay ambiguous unless an authoritative wire count uniquely
    /// matches one local write; newest-event adjacency is never used.
    pub(crate) fn cast_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();

        #[derive(PartialEq, Eq)]
        enum ChainKey {
            Local(u64),
            Native {
                scope: Option<(u64, u64)>,
                spell: Option<u32>,
                attempt: Option<u64>,
                wire: Option<u32>,
            },
        }

        struct Chain<'a> {
            key: ChainKey,
            events: Vec<&'a Event>,
            last_seq: u64,
            last_elapsed_ms: u64,
        }

        let mut chains: Vec<Chain> = Vec::new();
        for (seq, event) in &self.entries {
            let Stage::Cast {
                spell_id,
                wire_cast_count,
                attempt,
                edge,
                ..
            } = event.stage
            else {
                continue;
            };
            let key = if event.subject.interaction != 0 {
                ChainKey::Local(event.subject.interaction)
            } else if is_native_cast_edge(edge) {
                // A native edge joins the local chain it can be attributed to. When no unique
                // local write exists it keeps its own bounded chain, so the line can say the
                // local action was not retained instead of printing interaction 0.
                match self.cast_attribution(event, true).candidate {
                    Some(index) => ChainKey::Local(self.cast_candidates[index].subject.interaction),
                    None => ChainKey::Native {
                        scope: event.subject.scope,
                        spell: spell_id,
                        attempt,
                        wire: wire_cast_count,
                    },
                }
            } else {
                // A local request/admission/cancel whose action id was not retained is not
                // native outcome evidence and is not attributed to a different action.
                ChainKey::Native {
                    scope: event.subject.scope,
                    spell: spell_id,
                    attempt,
                    wire: wire_cast_count,
                }
            };
            let existing = chains.iter().position(|chain| {
                chain.key == key
                    && match &key {
                        ChainKey::Local(_) => true,
                        // A native-only chain with no attempt/wire is bounded by the same
                        // observation window; a later same-spell cast is a new chain.
                        ChainKey::Native { .. } => {
                            event.elapsed_ms.saturating_sub(chain.last_elapsed_ms) <= 30_000
                        }
                    }
            });
            match existing {
                Some(index) => {
                    chains[index].events.push(event);
                    chains[index].last_seq = *seq;
                    chains[index].last_elapsed_ms = event.elapsed_ms;
                }
                None => chains.push(Chain {
                    key,
                    events: vec![event],
                    last_seq: *seq,
                    last_elapsed_ms: event.elapsed_ms,
                }),
            }
        }
        chains.sort_by_key(|chain| std::cmp::Reverse(chain.last_seq));
        chains
            .into_iter()
            .take(8)
            .map(|chain| {
                let scope = match &chain.key {
                    ChainKey::Local(_) => chain.events.last().and_then(|event| event.subject.scope),
                    ChainKey::Native { scope, .. } => *scope,
                };
                let spell = match &chain.key {
                    ChainKey::Local(_) => chain.events.iter().find_map(|event| match event.stage {
                        Stage::Cast { spell_id, .. } => spell_id,
                        _ => None,
                    }),
                    ChainKey::Native { spell, .. } => *spell,
                };
                let events = chain.events;
                let last = events.last().expect("a chain has at least one event");
                let edge = match last.stage {
                    Stage::Cast { edge, .. } => format!("{edge:?}"),
                    _ => "?".to_string(),
                };

                let mut interactions: Vec<u64> = events
                    .iter()
                    .filter_map(|event| match event.stage {
                        // A cancel is a continuation of the active cast, not a second candidate.
                        Stage::Cast {
                            edge: CastEdge::CancelRequested,
                            ..
                        } => None,
                        _ => Some(event.subject.interaction),
                    })
                    .filter(|&interaction| interaction != 0)
                    .collect();
                if let ChainKey::Local(interaction) = &chain.key {
                    // The written candidate can outlive its local events in `cast_candidates`;
                    // keep naming it rather than falling back to "unknown".
                    interactions.push(*interaction);
                }
                interactions.sort_unstable();
                interactions.dedup();

                let mut attempts: Vec<u64> = events
                    .iter()
                    .filter_map(|event| match event.stage {
                        Stage::Cast {
                            attempt: Some(attempt),
                            ..
                        } => Some(attempt),
                        _ => None,
                    })
                    .collect();
                attempts.sort_unstable();
                attempts.dedup();

                let mut wires: Vec<u32> = events
                    .iter()
                    .filter_map(|event| match event.stage {
                        Stage::Cast {
                            wire_cast_count: Some(wire),
                            ..
                        } => Some(wire),
                        _ => None,
                    })
                    .collect();
                wires.sort_unstable();
                wires.dedup();
                let wire = if wires.len() == 1 {
                    wires.first().copied()
                } else {
                    None
                };

                let native_events: Vec<&Event> = events
                    .iter()
                    .copied()
                    .filter(|event| {
                        event.subject.interaction == 0
                            && matches!(
                                event.stage,
                                Stage::Cast { edge, .. } if is_native_cast_edge(edge)
                            )
                    })
                    .collect();
                let has_native = !native_events.is_empty();
                let attributions: Vec<CastAttribution> = native_events
                    .iter()
                    .map(|event| self.cast_attribution(*event, true))
                    .collect();
                let mut considered: Vec<u64> = attributions
                    .iter()
                    .flat_map(|attribution| attribution.considered.iter().copied())
                    .collect();
                considered.sort_unstable();
                considered.dedup();

                let ambiguous = interactions.len() > 1
                    || attempts.len() > 1
                    || wires.len() > 1
                    || attributions
                        .iter()
                        .any(|attribution| attribution.confidence == IncidentConfidence::Ambiguous);

                let confidence = if !has_native {
                    // A local write without native evidence is explicitly unverified, never a
                    // claimed cast outcome.
                    IncidentConfidence::Unverified
                } else if ambiguous {
                    IncidentConfidence::Ambiguous
                } else if interactions.len() == 1 {
                    attributions
                        .iter()
                        .map(|attribution| attribution.confidence)
                        .max_by_key(|confidence| cast_confidence_rank(*confidence))
                        .unwrap_or(IncidentConfidence::Unmatched)
                } else {
                    // Native evidence with no retained local interaction (or an unresolvable
                    // set) stays unverified: the action is not known.
                    IncidentConfidence::Unverified
                };

                let action = match interactions.as_slice() {
                    [interaction] => format!("action {interaction}"),
                    [] if !considered.is_empty()
                        && confidence == IncidentConfidence::Ambiguous =>
                    {
                        format!(
                            "action ambiguous (candidates {})",
                            cast_interaction_list(&considered)
                        )
                    }
                    [] if !considered.is_empty() => format!(
                        "action unknown (no unique local write; candidates {})",
                        cast_interaction_list(&considered)
                    ),
                    [] if has_native => "action unknown (local action not retained)".to_string(),
                    [] => "action unknown (no local action)".to_string(),
                    candidates => format!(
                        "action ambiguous (candidates {})",
                        cast_interaction_list(candidates)
                    ),
                };

                let mut line = format!(
                    "Cast {action} spell={} wire={} attempt={} scope={scope:?}: {edge} confidence={}",
                    spell.map_or_else(|| "unknown".to_string(), |spell| spell.to_string()),
                    cast_wire_label(wire),
                    cast_attempt_label(&attempts),
                    cast_confidence_label(confidence),
                );
                if !has_native {
                    line.push_str("; no native evidence retained");
                }
                if events.iter().any(|event| {
                    matches!(
                        event.stage,
                        Stage::Cast {
                            edge: CastEdge::Admitted,
                            wire_cast_count: None,
                            ..
                        }
                    )
                }) {
                    line.push_str("; local wire count unavailable");
                }
                if wire.is_some()
                    && native_events.iter().any(|event| {
                        matches!(
                            event.stage,
                            Stage::Cast {
                                wire_cast_count: None,
                                ..
                            }
                        )
                    })
                {
                    line.push_str("; native wire count unavailable");
                }
                if gaps {
                    line.push_str("; trace has gaps");
                }
                line
            })
            .collect()
    }
}

/// Whether one written loot/equip/buy/sell action's numeric subject can explain an inbound
/// evidence event. Matching is deliberately conservative: an absent key never earns exact
/// confidence, and a raw GUID or an item name is never consulted.
fn loot_vendor_evidence_matches_action(action: &Subject, evidence: &Event) -> bool {
    let stage_item = match &evidence.stage {
        Stage::LootSlotRemoved { item_id, .. }
        | Stage::VendorBuyResult { item_id, .. }
        | Stage::VendorBuyFailedObserved { item_id, .. } => *item_id,
        Stage::ItemPushResult { item_id, .. } => Some(*item_id),
        Stage::EquipmentSlotChanged { item_id, .. } => Some(*item_id),
        _ => None,
    };
    match (action.operation, &evidence.stage) {
        (Operation::Loot, Stage::LootSlotRemoved { slot, .. }) => {
            slot_matches(action.slot, Some(*slot))
                && item_matches(action.item, stage_item)
        }
        (Operation::Loot, Stage::LootMoneyNotify { .. } | Stage::LootMoneyCleared) => true,
        (Operation::Loot, Stage::LootErrorObserved { .. }) => {
            alias_matches(action.npc_alias, evidence.subject.npc_alias)
        }
        (Operation::Loot, Stage::LootResponseOpened | Stage::LootReleasedObserved) => {
            alias_matches(action.npc_alias, evidence.subject.npc_alias)
        }
        (Operation::Loot, Stage::ItemPushResult { item_id, .. }) => {
            item_matches(action.item, Some(*item_id))
        }
        // An item push says the item arrived, not that it occupies an equipment slot. Only a
        // slot change (or an inventory refusal) is equipment evidence.
        // The action's `slot` is the source bag slot, not the destination equipment slot, so
        // only the item entry can be compared here.
        (Operation::Equip, Stage::EquipmentSlotChanged { item_id, .. }) => {
            item_matches(action.item, Some(*item_id))
        }
        (Operation::Equip, Stage::InventoryFailure { .. }) => true,
        (Operation::Buy, Stage::VendorBuyResult { item_id, slot, .. }) => {
            item_matches(action.item, *item_id)
                && slot_matches(action.slot, *slot)
                && alias_matches(action.npc_alias, evidence.subject.npc_alias)
        }
        (Operation::Buy, Stage::VendorBuyFailedObserved { item_id, .. }) => {
            item_matches(action.item, *item_id)
                && alias_matches(action.npc_alias, evidence.subject.npc_alias)
        }
        (Operation::Buy, Stage::InventoryFailure { .. }) => true,
        (Operation::Sell, Stage::VendorSellFailedObserved { .. }) => {
            alias_matches(action.npc_alias, evidence.subject.npc_alias)
        }
        (Operation::Sell, Stage::InventoryFailure { .. }) => true,
        (
            Operation::Loot,
            Stage::DomainEvent {
                event: DomainEvent::LootError,
            },
        ) => true,
        (
            Operation::Buy,
            Stage::DomainEvent {
                event: DomainEvent::VendorBuyFailed,
            },
        ) => true,
        (
            Operation::Sell,
            Stage::DomainEvent {
                event: DomainEvent::VendorSellFailed,
            },
        ) => true,
        (
            Operation::Loot | Operation::Equip | Operation::Buy | Operation::Sell,
            Stage::DomainEvent {
                event: DomainEvent::InventoryFailure,
            },
        ) => true,
        _ => false,
    }
}

/// Closed handler outcome for one retained click edge.
///
/// Derived only from the witness, so the inspector and the saved report cannot disagree with the
/// row they describe. The variant set is closed: an unrecognised state cannot be invented.
#[cfg(feature = "dev")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiClickOutcome {
    /// No frame took the point. `clipped` names the rect-containing frame that effective hit
    /// excluded, and only when that exclusion is a scroll clip or an unresolvable clip owner.
    NoHit { clipped: Option<UiBlockReason> },
    /// A frame took the point but carries no recognised coarse category.
    NoNamedHit,
    /// A named frame took the point and no handler (Lua function, script hook or engine click
    /// action) is bound for the edge that fires `OnClick`.
    NoHandler,
    /// A handler is bound but the click slot did not reach it.
    HandlerBlocked { gate: UiClickBlock },
    /// A Lua handler ran, or the engine click action ran without one.
    HandlerRan,
}

/// Classify one edge from its witness alone.
#[cfg(feature = "dev")]
fn click_outcome(witness: &UiHitEvidence) -> UiClickOutcome {
    if witness.onclick_ran || witness.engine_click {
        return UiClickOutcome::HandlerRan;
    }
    let Some(hit) = witness.hit else {
        // Only a scroll clip or a missing clip owner is a clipped-target statement. Hit insets,
        // a disabled mouse bit and the nameplate veto are by design or by veto, never a clip.
        let clipped = witness.blocker.and_then(|blocker| match blocker.reason {
            UiBlockReason::OutsideClip | UiBlockReason::ClipUnresolved => Some(blocker.reason),
            UiBlockReason::HitInsets
            | UiBlockReason::NotMouseEnabled
            | UiBlockReason::NameplateVeto => None,
        });
        return UiClickOutcome::NoHit { clipped };
    };
    if hit.family == UiTarget::NoNamedFrame {
        return UiClickOutcome::NoNamedHit;
    }
    if witness.onclick_present {
        UiClickOutcome::HandlerBlocked {
            gate: witness.blocked,
        }
    } else {
        UiClickOutcome::NoHandler
    }
}

/// What the outcome establishes, in the closed vocabulary.
#[cfg(feature = "dev")]
fn click_outcome_phrase(witness: &UiHitEvidence) -> String {
    match click_outcome(witness) {
        UiClickOutcome::NoHit { clipped: None } => {
            "no hit (no frame took the point; a fully hidden frame is indistinguishable from empty space)"
                .to_string()
        }
        UiClickOutcome::NoHit {
            clipped: Some(reason),
        } => format!(
            "no hit; a rect-containing frame was excluded because {}",
            block_reason_phrase(reason)
        ),
        UiClickOutcome::NoNamedHit => {
            "a frame took the point but has no recognised category".to_string()
        }
        UiClickOutcome::NoHandler => {
            "no handler bound for this button edge (no Lua handler, hook or engine click action)"
                .to_string()
        }
        UiClickOutcome::HandlerBlocked { gate } => {
            format!("handler present, not run: {}", click_block_phrase(gate))
        }
        UiClickOutcome::HandlerRan => {
            if witness.onclick_ran {
                "handler ran".to_string()
            } else {
                "engine click action ran (no Lua handler)".to_string()
            }
        }
    }
}

/// Why a rect-containing frame did not take the point.
#[cfg(feature = "dev")]
fn block_reason_phrase(reason: UiBlockReason) -> &'static str {
    match reason {
        UiBlockReason::OutsideClip => "its scroll clip excluded the point",
        UiBlockReason::ClipUnresolved => "it has no resolvable clip owner (layout gap)",
        UiBlockReason::HitInsets => "the point is outside its hit-rect insets (by design)",
        UiBlockReason::NotMouseEnabled => "it is not mouse-enabled (by design)",
        UiBlockReason::NameplateVeto => "a nameplate veto refused the hit",
    }
}

/// The first gate that stopped a bound handler, in the order the click slots test them.
#[cfg(feature = "dev")]
fn click_block_phrase(gate: UiClickBlock) -> &'static str {
    match gate {
        UiClickBlock::None => "no gate recorded",
        UiClickBlock::LayoutDrag => "a layout drag took the press",
        UiClickBlock::WorldHook => "the world input hook took the press",
        UiClickBlock::TitleMove => "the press landed in the title region",
        UiClickBlock::PressTargetNone => "there was no press target",
        UiClickBlock::Disabled => "the button was disabled",
        UiClickBlock::NotRegistered => "the frame is not registered for this button edge",
        UiClickBlock::CursorOutside => "the release point was outside the pressed frame",
        UiClickBlock::FrameGone => "the pressed frame no longer exists",
        UiClickBlock::CaptureLost => "the pressed frame hid before the release",
        UiClickBlock::NotPushed => {
            "the button was not pushed (disabled, hidden, never pressed, or the press was cleared)"
        }
        UiClickBlock::DragStarted => "a drag consumed the release",
        UiClickBlock::HandlerMissing => "no handler is bound for this edge",
        UiClickBlock::HandlerError => "the handler raised",
    }
}

/// Rank an incident confidence so two claims can be composed by the weaker one.
#[cfg(feature = "dev")]
fn confidence_rank(confidence: IncidentConfidence) -> u8 {
    match confidence {
        IncidentConfidence::Exact => 5,
        IncidentConfidence::Strong => 4,
        IncidentConfidence::Heuristic => 3,
        IncidentConfidence::Ambiguous => 2,
        IncidentConfidence::Unverified => 1,
        IncidentConfidence::Unmatched => 0,
    }
}

/// The weaker of two confidences: a later exact server report can never upgrade a weaker click
/// link, and a click link can never upgrade an unverified refusal.
#[cfg(feature = "dev")]
fn weaker(a: IncidentConfidence, b: IncidentConfidence) -> IncidentConfidence {
    if confidence_rank(a) <= confidence_rank(b) {
        a
    } else {
        b
    }
}

/// The association between a click whose handler ran and the retained script-error rows.
///
/// A row is message identity: identical messages collapse across frames and the row time is a
/// poll boundary. A message first retained after the click is a new appearance; one retained
/// before the click is a repeat. Neither establishes that this click raised it.
#[cfg(feature = "dev")]
fn script_error_attachment(
    errors: &[(u64, u64, u64, u64)],
    vm_session: u64,
    click_elapsed: u64,
) -> String {
    let mut fresh = 0usize;
    let mut repeats = 0usize;
    for &(vm, _, first, last) in errors {
        if vm != vm_session {
            continue;
        }
        if first > click_elapsed {
            if first - click_elapsed <= 2_000 {
                fresh += 1;
            }
        } else if last > click_elapsed && last - click_elapsed <= 2_000 {
            repeats += 1;
        }
    }
    match (fresh, repeats) {
        (0, 0) => String::new(),
        (1, 0) => "; a script-error row first appeared after this click (polled 1 Hz, message-keyed; association only)".to_string(),
        (0, 1) => "; a script-error message already retained was polled again after this click (association only)".to_string(),
        _ => format!(
            "; {} script-error rows polled after this click ({fresh} new, {repeats} repeats; ambiguous, association only)",
            fresh + repeats
        ),
    }
}


#[cfg(feature = "dev")]
impl History {
    /// A time-window association is a lead, not proof that a press caused a script error.
    ///
    /// `ScriptError` rows are polled once per second from the VM's diagnostic log and keyed by
    /// the error message, not by the handler or frame that raised it. The row time is therefore a
    /// poll boundary rather than the error time, and identical messages raise one row however
    /// many frames raise them. A click's local `handler_error` is the only exact fault fact this
    /// recorder keeps; every other statement here is an association.
    pub(crate) fn ui_error_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        let pointer_lost = self.ui_input_rows_lost();
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, event)| {
                let (vm_session, row_seq) = match event.stage {
                    Stage::ScriptError { vm_session, row_seq, .. } => (vm_session, row_seq),
                    _ => return None,
                };
                // A repeated message bumps its retained row's count; a new message mints a row.
                // Only retained rows can say which happened, and only approximately: a row is
                // message identity, never handler identity.
                let repeated = self.entries.iter().any(|(_, earlier)| {
                    earlier.elapsed_ms < event.elapsed_ms
                        && matches!(
                            earlier.stage,
                            Stage::ScriptError { vm_session: vm, row_seq: row, .. }
                                if vm == vm_session && row == row_seq
                        )
                });
                let mut pointers = 0usize;
                let mut antecedents = 0usize;
                let mut local_errors: Vec<u64> = Vec::new();
                for (_, earlier) in self.entries.iter() {
                    if earlier.subject.scope != event.subject.scope
                        || earlier.elapsed_ms > event.elapsed_ms
                        || event.elapsed_ms.saturating_sub(earlier.elapsed_ms) > 2_000
                    {
                        continue;
                    }
                    let (witness, dispatch) = match earlier.stage {
                        Stage::UiPress {
                            vm_session: vm,
                            witness,
                            ui_dispatch_id,
                            ..
                        }
                        | Stage::UiClick {
                            vm_session: vm,
                            witness,
                            ui_dispatch_id,
                            ..
                        } if vm == vm_session => (witness, ui_dispatch_id),
                        _ => continue,
                    };
                    pointers += 1;
                    if witness.handler_error {
                        local_errors.push(dispatch);
                    }
                    if witness.handler_error || click_edge_can_claim(&witness) {
                        antecedents += 1;
                    }
                }
                let poll_note = if repeated {
                    "a repeat of a message already retained"
                } else {
                    "a first retained appearance"
                };
                let result = if pointers == 0 {
                    if pointer_lost {
                        "no retained press or click in this VM within two seconds; pointer rows were lost (association unavailable)".to_string()
                    } else {
                        "no retained press or click in this VM within two seconds (absence, not proof)".to_string()
                    }
                } else if local_errors.len() == 1 {
                    format!(
                        "pointer edge {} caught a local handler error in the window (exact); this polled row is {poll_note} and is not handler-identified",
                        local_errors[0]
                    )
                } else if local_errors.len() > 1 {
                    format!(
                        "{} pointer edges caught a local handler error in the window (ambiguous; no causal claim)",
                        local_errors.len()
                    )
                } else if antecedents == 1 {
                    format!(
                        "one nearby click edge whose handler ran in the same VM; polled row is {poll_note}; possible association, polled once per second and message-keyed"
                    )
                } else if antecedents > 1 {
                    format!(
                        "{antecedents} nearby click edges whose handlers ran (ambiguous; no causal claim)"
                    )
                } else {
                    "pointer input within two seconds but no click antecedent for the edge that fires the handler (no causal claim)".to_string()
                };
                Some(bounded_line(if gaps || pointer_lost {
                    let mut line = format!("UI error {row_seq}: {result}");
                    if gaps {
                        line.push_str("; trace has gaps");
                    }
                    if pointer_lost {
                        line.push_str("; pointer rows were lost");
                    }
                    line
                } else {
                    format!("UI error {row_seq}: {result}")
                }))
            })
            .take(8)
            .collect()
    }

    /// Summarize recent UI clicks with their handler outcome and retained attachments.
    ///
    /// The verdict comes from the witness captured at the click edge. `visible` is the
    /// capture-time hit fact, so a click whose handler hid its own frame stays a click; the
    /// after-handler facts are separate fields. Attachments are associations unless the witness
    /// literally establishes them.
    pub(crate) fn ui_click_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        let pointer_lost = self.ui_input_rows_lost();
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, event)| match event.stage {
                Stage::UiClick {
                    button,
                    target,
                    ui_dispatch_id,
                    witness,
                    ..
                } => {
                    let hops = if witness.hit.is_some() && witness.owner_hops != 0xff {
                        format!(", hops {}", witness.owner_hops)
                    } else {
                        String::new()
                    };
                    let mut line = format!(
                        "UI click {ui_dispatch_id} {button:?} on {target:?} (owner {:?}{hops}, bounds {:?}/{:?}, clicks {:?}): {}",
                        witness.owner,
                        witness.bounds,
                        witness.bounds_bucket,
                        witness.clicks,
                        click_outcome_phrase(&witness),
                    );
                    if witness.hit.is_some() && !witness.visible_after {
                        line.push_str("; hit frame hidden after the click");
                    }
                    if witness.hit.is_some() && !witness.alive_after {
                        line.push_str("; hit frame no longer exists after the click");
                    }
                    if gaps {
                        line.push_str("; trace has gaps");
                    }
                    if pointer_lost {
                        line.push_str("; pointer rows were lost");
                    }
                    Some(bounded_line(line))
                }
                _ => None,
            })
            .take(8)
            .collect()
    }

    /// Bounded per-click outcome lines for the inspector and the saved report.
    ///
    /// Each line states what the retained evidence establishes (the click row's own witness),
    /// what it links through the host-minted `ui_dispatch_id`, and what stays an association.
    /// Missing evidence is reported as absence, never as a negative outcome.
    pub(crate) fn ui_click_outcomes(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        let pointer_lost = self.ui_input_rows_lost();
        // One bounded pass over the retained script-error rows: first and last poll time per
        // (VM, message row). This keeps the per-click attachment cheap, and lets a line say
        // whether the message pre-dates the click.
        let mut errors: Vec<(u64, u64, u64, u64)> = Vec::new();
        for (_, event) in self.entries.iter() {
            let (vm_session, row_seq) = match event.stage {
                Stage::ScriptError {
                    vm_session,
                    row_seq,
                    ..
                } => (vm_session, row_seq),
                _ => continue,
            };
            if let Some(entry) = errors
                .iter_mut()
                .find(|entry| entry.0 == vm_session && entry.1 == row_seq)
            {
                entry.2 = entry.2.min(event.elapsed_ms);
                entry.3 = entry.3.max(event.elapsed_ms);
            } else {
                errors.push((vm_session, row_seq, event.elapsed_ms, event.elapsed_ms));
            }
        }
        let mut lines: Vec<String> = self
            .entries
            .iter()
            .rev()
            .filter_map(|(_, event)| match event.stage {
                Stage::UiClick {
                    vm_session,
                    ui_dispatch_id,
                    witness,
                    ..
                } => {
                    let mut line = format!(
                        "UI click {ui_dispatch_id} (vm{vm_session} reload{}): {}",
                        event.subject.reload_generation,
                        click_outcome_phrase(&witness),
                    );
                    if witness.handler_error {
                        line.push_str("; handler error caught locally (exact)");
                    } else if witness.onclick_ran || witness.engine_click {
                        line.push_str(&script_error_attachment(
                            &errors,
                            vm_session,
                            event.elapsed_ms,
                        ));
                    }
                    if click_outcome(&witness) == UiClickOutcome::HandlerRan {
                        line.push_str(&self.click_link_phrase(event, ui_dispatch_id));
                    }
                    if gaps {
                        line.push_str("; trace has gaps");
                    }
                    if pointer_lost {
                        line.push_str("; pointer rows were lost");
                    }
                    Some(bounded_line(line))
                }
                _ => None,
            })
            .take(8)
            .collect();
        if lines.is_empty() {
            lines.push(bounded_line(if pointer_lost {
                "UI click outcome: no retained click rows; pointer rows were lost (association unavailable)".to_string()
            } else {
                "UI click outcome: no retained click rows (absence, not proof)".to_string()
            }));
        }
        lines
    }

    /// The link for a retained click row.
    fn click_link_phrase(&self, click: &Event, ui_dispatch_id: u64) -> String {
        self.dispatch_link_phrase(click.subject.scope, ui_dispatch_id)
    }

    /// What the retained actions that carry `ui_dispatch_id` can establish.
    ///
    /// The link is the host-minted id on the action's `Subject`, not temporal adjacency. That id
    /// is one eligible candidate chosen at intent time, so the link is a `Heuristic` attribution,
    /// and the incident's confidence is composed with it by the weaker.
    fn dispatch_link_phrase(&self, scope: Option<(u64, u64)>, ui_dispatch_id: u64) -> String {
        let mut links: Vec<(Subject, Stage)> = Vec::new();
        for (_, earlier) in self.entries.iter().rev() {
            if earlier.subject.ui_dispatch_id != Some(ui_dispatch_id)
                || earlier.subject.interaction == 0
                || earlier.subject.scope != scope
            {
                continue;
            }
            if links
                .iter()
                .any(|(subject, _)| subject.interaction == earlier.subject.interaction)
            {
                continue;
            }
            links.push((earlier.subject, earlier.stage));
            if links.len() == 4 {
                break;
            }
        }
        match links.as_slice() {
            [] => {
                if self.ui_input_rows_lost() {
                    "; no retained action carries this click's id; pointer rows were lost (association unavailable)".to_string()
                } else {
                    "; no retained action carries this click's id (absence, not proof)".to_string()
                }
            }
            [(subject, stage)] => {
                match self
                    .incidents
                    .iter()
                    .rev()
                    .find(|incident| incident.subject.interaction == subject.interaction)
                {
                    Some(incident) => format!(
                        "; host-attributed link action {} {:?} last {:?}; outcome {:?} [{:?}]",
                        subject.interaction,
                        subject.operation,
                        stage,
                        incident.outcome,
                        weaker(IncidentConfidence::Heuristic, incident.confidence),
                    ),
                    None => format!(
                        "; host-attributed link action {} {:?} last {:?}; outcome in progress/unconfirmed [Unverified]",
                        subject.interaction, subject.operation, stage,
                    ),
                }
            }
            many => format!(
                "; {} host-attributed links (ambiguous reporting)",
                many.len()
            ),
        }
    }

    pub(crate) fn inspector_snapshot(&mut self) -> &InspectorSnapshot {
        let stale = self
            .inspector_snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot.received != self.received
                    || snapshot.evicted != self.evicted
                    || snapshot.dropped != self.source.0.dropped.load(Ordering::Relaxed)
                    || snapshot.source_gaps != self.source_gaps
                    || snapshot.sampled_updates != self.sampled_updates()
            })
            .unwrap_or(true);
        let periodic = self
            .inspector_snapshot
            .as_ref()
            .map(|snapshot| snapshot.built_at.elapsed() >= Duration::from_millis(250))
            .unwrap_or(true);
        if self.inspector_snapshot.is_none() || stale || periodic {
            let truncate = |line: String| -> String {
                if line.len() > 240 {
                    let mut boundary = 240;
                    while boundary > 0 && !line.is_char_boundary(boundary) {
                        boundary -= 1;
                    }
                    let mut truncated = line;
                    truncated.truncate(boundary);
                    truncated
                } else {
                    line
                }
            };
            let cap = |lines: Vec<String>, limit: usize| -> Vec<String> {
                lines.into_iter().take(limit).map(truncate).collect()
            };
            let (received, evicted, dropped) = self.counters();
            self.inspector_snapshot = Some(InspectorSnapshot {
                built_at: Instant::now(),
                received,
                evicted,
                dropped,
                source_gaps: self.source_gaps,
                sampled_updates: self.sampled_updates(),
                incidents: cap(
                    self.recent_incidents().map(|line| line.to_string()).collect(),
                    8,
                ),
                actions: cap(self.action_summaries(), 8),
                panels: cap(self.panel_correlations(), 8),
                slots: cap(self.slot_correlations(), 8),
                turn_ins: cap(self.turn_in_correlations(), 8),
                ui_errors: cap(self.ui_error_correlations(), 8),
                ui_clicks: cap(self.ui_click_correlations(), 8),
                ui_click_outcomes: cap(self.ui_click_outcomes(), 8),
                cast_chains: cap(self.cast_correlations(), 8),
                loot_vendor: cap(self.loot_vendor_correlations(), 8),
                quest_ui: cap(self.quest_ui_correlations(), 8),
                session: cap(self.session_transition_summaries(), 8),
                movement: cap(self.movement_pressure(), 8),
                movement_corrections: cap(self.movement_correction_attribution(), 8),
                audio: cap(self.audio_output_health(), 8),
                assets: cap(self.asset_window_summary(), 8),
                hotpatch: {
                    #[cfg(feature = "hotpatch")]
                    {
                        cap(self.hotpatch_attempt_correlation(), 8)
                    }
                    #[cfg(not(feature = "hotpatch"))]
                    {
                        Vec::new()
                    }
                },
                recent: cap(self.recent().map(|line| line.to_string()).collect(), 16),
                retention: cap(self.retention_summary(), 13),
            });
        }
        self.inspector_snapshot.as_ref().unwrap()
    }

    /// Bounded quest/UI summary for the inspector: recent quest interactions with their last
    /// milestone, the most recent UI dispatch token under the 2-second visibility/consumption
    /// rules, and the latest journal projection/row with the reload generation.
    ///
    /// Causality is never claimed. A token is either a `possible association` (exactly one
    /// eligible click in the window), `ambiguous` (more than one), or explicitly not a plausible
    /// cause (hidden, clipped, unconsumed, handler not invoked, old VM, or old reload generation).
    pub(crate) fn quest_ui_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        let mut lines = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (_, event) in self.entries.iter().rev() {
            let id = event.subject.interaction;
            if id == 0 || !seen.insert(id) {
                continue;
            }
            let operation = event.subject.operation;
            if !matches!(
                operation,
                Operation::Open
                    | Operation::Select
                    | Operation::Accept
                    | Operation::RequestReward
                    | Operation::ChooseReward
                    | Operation::Confirm
                    | Operation::Abandon
                    | Operation::Share
            ) {
                continue;
            }
            let incident = self
                .incidents
                .iter()
                .rev()
                .find(|incident| incident.subject.interaction == id);
            let (outcome, confidence) = incident.map_or_else(
                || ("in progress/unconfirmed".to_string(), "Unverified".to_string()),
                |incident| {
                    (
                        format!("{:?}", incident.outcome),
                        format!("{:?}", incident.confidence),
                    )
                },
            );
            let gap = gaps || incident.is_some_and(|incident| incident.telemetry_gap);
            lines.push(format!(
                "Quest action {id} {operation:?} (npc {:?}, quest {:?}): last milestone {:?}; outcome {outcome} [{confidence}]; dispatch {:?}{}",
                event.subject.npc_alias,
                event.subject.quest,
                event.stage,
                event.subject.ui_dispatch_id,
                if gap { "; trace has gaps" } else { "" },
            ));
            if lines.len() == 8 {
                break;
            }
        }

        // The newest retained click token, joined to the retained actions that carry its id.
        //
        // This is a reverse join on the host-minted `ui_dispatch_id`, not a temporal count. The
        // token's own click row is never a separate candidate, and a press row of an Up-only
        // click cannot claim the antecedent at all because `OnClick` fires on its release.
        let dispatches = self.source.recent_ui_dispatches();
        let newest = dispatches.iter().rev().find(|token| {
            token.witness.edge == UiEdge::Release
                || matches!(token.witness.clicks, UiClickMask::Down | UiClickMask::Both)
                || click_edge_can_claim(&token.witness)
        });
        match newest {
            Some(token) => {
                let current_rg = self.source.0.reload_generation.load(Ordering::Acquire);
                let mut line = format!(
                    "UI dispatch {}: button {:?} target {:?} edge {:?} clicks {:?} (vm{} reload{}): {}",
                    token.id,
                    token.button,
                    token.target,
                    token.witness.edge,
                    token.witness.clicks,
                    token.vm_session,
                    token.reload_generation,
                    if token.reload_generation != current_rg {
                        "not a plausible cause (old reload generation)"
                    } else if !click_edge_can_claim(&token.witness) {
                        "not a click antecedent (no handler ran on the edge that fires OnClick)"
                    } else {
                        "established click antecedent for this registration"
                    },
                );
                line.push_str(&self.dispatch_link_phrase(token.scope, token.id));
                if gaps {
                    line.push_str("; trace has gaps");
                }
                if self.ui_input_rows_lost() {
                    line.push_str("; pointer rows were lost");
                }
                lines.push(bounded_line(line));
            }
            None => {
                lines.push("UI dispatch antecedent: no retained press/click token".to_string());
            }
        }

        let current_rg = self
            .source
            .0
            .reload_generation
            .load(Ordering::Acquire);
        if let Some((_, event)) = self.entries.iter().rev().find(|(_, event)| {
            matches!(
                event.stage,
                Stage::JournalProjection
                    | Stage::JournalRowInModel
                    | Stage::JournalTemplatePending
                    | Stage::JournalTemplateRefused
            )
        }) {
            lines.push(format!(
                "Journal: quest {:?} {:?} in reload generation {} (current {}{})",
                event.subject.quest,
                event.stage,
                event.subject.reload_generation,
                current_rg,
                if event.subject.reload_generation < current_rg {
                    "; old VM projection superseded"
                } else {
                    ""
                },
            ));
        } else {
            lines.push(format!(
                "Journal: no retained projection or row (reload generation {current_rg})"
            ));
        }
        lines
    }

    pub(crate) fn recent_incidents(&self) -> impl Iterator<Item = String> + '_ {
        self.incidents.iter().rev().take(8).map(|incident| {
            let action = if incident.subject.interaction == 0 {
                "Action unknown".to_string()
            } else {
                format!("Action {}", incident.subject.interaction)
            };
            let missing = incident
                .evidence
                .iter()
                .filter(|seq| !self.entries.contains_key(*seq))
                .count();
            // The opening and terminal notes are self-contained copies resolved before eviction,
            // so they stay readable even when both referenced rows are gone from the ring.
            // Provenance distinguishes a directly retained row from one recovered from bounded
            // start metadata, and an explicit unavailable state when no metadata exists.
            let opening = match (incident.opening_note_source, incident.evidence_notes.first()) {
                ("unavailable", _) | (_, None) => "opening unavailable".to_string(),
                (_, Some(note)) if note.recovered => {
                    format!("recovered from start metadata ({})", note.label())
                }
                (_, Some(note)) => note.label(),
            };
            let terminal = incident.evidence_notes.last().map_or_else(
                || "terminal note unavailable".to_string(),
                EvidenceNote::label,
            );
            let mut flags: Vec<String> = Vec::new();
            if incident.evidence_notes_truncated || incident.evidence_truncated {
                flags.push("notes truncated".to_string());
            }
            if incident.evidence_notes.is_empty() {
                flags.push("notes unavailable".to_string());
            }
            if missing > 0 {
                flags.push(format!("{missing} evidence rows evicted"));
            }
            if incident.telemetry_gap {
                flags.push("trace has gaps".to_string());
            }
            format!(
                "{action} {:?} npc {:?} quest {:?} item {:?} count {:?} slot {:?}: open {opening}; terminal {terminal}; notes {}; {:?} [{:?}]",
                incident.subject.operation,
                incident.subject.npc_alias,
                incident.subject.quest,
                incident.subject.item,
                incident.subject.count,
                incident.subject.slot,
                if flags.is_empty() {
                    "complete".to_string()
                } else {
                    flags.join(", ")
                },
                incident.outcome,
                incident.confidence,
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
                            && candidate.subject.reload_generation
                                == event.subject.reload_generation
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

    /// Native cast evidence for one written local cast interaction. The native edge is only
    /// attributed to this action when [`Self::cast_attribution`] uniquely matches it; two rapid
    /// same-spell casts with no authoritative wire count stay ambiguous instead of borrowing the
    /// newest native edge.
    fn cast_native_summary(&self, event: &Event) -> Option<String> {
        if matches!(
            event.stage,
            Stage::Cast {
                edge: CastEdge::LocalRefusal { .. },
                ..
            }
        ) {
            return None;
        }
        let interaction = event.subject.interaction;
        if interaction == 0 {
            // Native-only evidence is not a local action; never present it as one.
            return None;
        }
        let spell = self.entries.iter().find_map(|(_, prior)| {
            if prior.subject.interaction != event.subject.interaction {
                return None;
            }
            match prior.stage {
                Stage::Cast {
                    spell_id: Some(spell),
                    ..
                } => Some(spell),
                _ => None,
            }
        })?;
        let local_wire = self
            .entries
            .iter()
            .find_map(|(_, prior)| {
                if prior.subject.interaction != event.subject.interaction {
                    return None;
                }
                match prior.stage {
                    Stage::Cast {
                        wire_cast_count: Some(wire),
                        edge: CastEdge::Admitted,
                        ..
                    } => Some(wire),
                    _ => None,
                }
            })
            .or_else(|| {
                self.entries.iter().find_map(|(_, prior)| {
                    if prior.subject.interaction != event.subject.interaction {
                        return None;
                    }
                    match prior.stage {
                        Stage::Cast {
                            wire_cast_count: Some(wire),
                            ..
                        } => Some(wire),
                        _ => None,
                    }
                })
            });
        let native: Vec<&Event> = self
            .entries
            .iter()
            .map(|(_, candidate)| candidate)
            .filter(|candidate| {
                candidate.subject.interaction == 0
                    && candidate.subject.scope == event.subject.scope
                    && candidate.elapsed_ms >= event.elapsed_ms
                    && candidate.elapsed_ms.saturating_sub(event.elapsed_ms) <= 30_000
                    && matches!(
                        candidate.stage,
                        Stage::Cast {
                            spell_id: Some(edge_spell),
                            edge,
                            ..
                        } if edge_spell == spell && is_native_cast_edge(edge)
                    )
            })
            .collect();
        if native.is_empty() {
            return Some(format!(
                "; cast spell {spell} wire {local_wire:?}: no native evidence yet (unverified)"
            ));
        }
        let mut matched: Vec<(&Event, IncidentConfidence)> = Vec::new();
        let mut ambiguous_edges = 0usize;
        for candidate in native {
            let attribution = self.cast_attribution(candidate, true);
            match attribution.candidate {
                Some(index)
                    if self.cast_candidates[index].subject.interaction == interaction =>
                {
                    matched.push((candidate, attribution.confidence));
                }
                // The edge belongs to a different local action (or none); never borrow it.
                Some(_) => {}
                None if attribution.confidence == IncidentConfidence::Ambiguous => {
                    ambiguous_edges += 1;
                }
                None => {}
            }
        }
        if matched.is_empty() {
            if ambiguous_edges > 0 {
                return Some(format!(
                    "; cast spell {spell} wire {local_wire:?}: native evidence ambiguous; no local action assigned"
                ));
            }
            return Some(format!(
                "; cast spell {spell} wire {local_wire:?}: no native evidence for this action (unverified)"
            ));
        }
        let mut wires: Vec<u32> = matched
            .iter()
            .filter_map(|(candidate, _)| match candidate.stage {
                Stage::Cast {
                    wire_cast_count: Some(wire),
                    ..
                } => Some(wire),
                _ => None,
            })
            .collect();
        wires.sort_unstable();
        wires.dedup();
        let mut attempts: Vec<u64> = matched
            .iter()
            .filter_map(|(candidate, _)| match candidate.stage {
                Stage::Cast {
                    attempt: Some(attempt),
                    ..
                } => Some(attempt),
                _ => None,
            })
            .collect();
        attempts.sort_unstable();
        attempts.dedup();
        if wires.len() > 1 || attempts.len() > 1 {
            return Some(format!(
                "; cast spell {spell} wire {local_wire:?}: {} matched native edges (ambiguous)",
                matched.len()
            ));
        }
        let (latest, confidence) = matched
            .into_iter()
            .max_by_key(|(candidate, _)| candidate.elapsed_ms)
            .expect("matched is non-empty");
        let (edge, native_wire, attempt) = match latest.stage {
            Stage::Cast {
                edge,
                wire_cast_count,
                attempt,
                ..
            } => (edge, wire_cast_count, attempt),
            _ => return None,
        };
        Some(format!(
            "; cast spell {spell} wire {local_wire:?}/{native_wire:?} attempt {attempt:?}: {edge:?} ({})",
            cast_confidence_label(confidence)
        ))
    }

    /// Written actions in the same scope that could explain `evidence`. More than one makes the
    /// attribution ambiguous even when each individual subject looks exact.
    fn eligible_loot_vendor_writes(&self, evidence: &Event, operation: Operation) -> usize {
        self.entries
            .iter()
            .filter(|(_, candidate)| {
                candidate.subject.operation == operation
                    && candidate.subject.scope.is_some()
                    && candidate.subject.scope == evidence.subject.scope
                    && candidate.elapsed_ms <= evidence.elapsed_ms
                    && evidence.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000
                    && matches!(candidate.stage, Stage::Delivery(CommandDelivery::Written))
                    && loot_vendor_evidence_matches_action(&candidate.subject, evidence)
            })
            .count()
    }

    /// Evidence summary for one written loot/equip/buy/sell action. `exact` is reserved for a
    /// carried item/slot identity (or an authoritative refusal carrying the item); a bare
    /// time-window match is heuristic.
    fn loot_vendor_action_evidence(&self, action: &Event, evidence: &Event) -> Option<String> {
        let (label, mut exact) = loot_vendor_evidence_label(&evidence.stage)?;
        // An item push says the item arrived; it does not prove the loot row was removed.
        if action.subject.operation == Operation::Loot
            && matches!(evidence.stage, Stage::ItemPushResult { .. })
        {
            exact = false;
        }
        let matches = self.eligible_loot_vendor_writes(evidence, action.subject.operation);
        let confidence = if matches > 1 {
            "ambiguous"
        } else if exact {
            "strong"
        } else {
            "heuristic"
        };
        Some(format!("; {label} ({confidence})"))
    }

    /// Bounded inspector lines for user-originated loot, equipment and merchant actions: the last
    /// written milestone plus the strongest matching server evidence, or an explicit uncertainty.
    #[allow(dead_code)] // integrated by the inspector/export owner
    pub(crate) fn loot_vendor_correlations(&self) -> Vec<String> {
        let gaps = self.has_telemetry_gaps();
        let mut seen = std::collections::HashSet::new();
        let mut output = Vec::new();
        for (_, event) in self.entries.iter().rev() {
            let subject = event.subject;
            if subject.interaction == 0
                || !matches!(
                    subject.operation,
                    Operation::Loot | Operation::Equip | Operation::Buy | Operation::Sell
                )
                || !matches!(event.stage, Stage::Delivery(CommandDelivery::Written))
                || !seen.insert(subject.interaction)
            {
                continue;
            }
            let candidate = self.entries.iter().rev().find(|(_, later)| {
                later.subject.scope.is_some()
                    && later.subject.scope == subject.scope
                    && later.elapsed_ms >= event.elapsed_ms
                    && later.elapsed_ms.saturating_sub(event.elapsed_ms) <= 30_000
                    && loot_vendor_evidence_matches_action(&subject, later)
            });
            let line = match candidate {
                Some((_, later)) => {
                    let (label, _) = loot_vendor_evidence_label(&later.stage)
                        .unwrap_or(("server evidence observed".to_owned(), false));
                    let matches = self.eligible_loot_vendor_writes(later, subject.operation);
                    let confidence = if matches > 1 {
                        "ambiguous"
                    } else {
                        "possible match"
                    };
                    format!(
                        "Action {} {:?} (npc {:?}, item {:?}, count {:?}, slot {:?}): {label} ({confidence})",
                        subject.interaction,
                        subject.operation,
                        subject.npc_alias,
                        subject.item,
                        subject.count,
                        subject.slot,
                    )
                }
                None => format!(
                    "Action {} {:?} (npc {:?}, item {:?}, count {:?}, slot {:?}): sent; no matching server evidence in 30s (unverified)",
                    subject.interaction,
                    subject.operation,
                    subject.npc_alias,
                    subject.item,
                    subject.count,
                    subject.slot,
                ),
            };
            output.push(if gaps {
                format!("{line}; trace has gaps")
            } else {
                line
            });
            if output.len() == 8 {
                break;
            }
        }
        output
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
                Stage::GuardRefused
                | Stage::Cast {
                    edge: CastEdge::LocalRefusal { .. },
                    ..
                } => "blocked by a local guard",
                Stage::NoWorld => "no active world connection",
                Stage::AlreadyPending | Stage::TrackingCapacity => "local tracking is busy",
                Stage::ChannelDisconnected => "command channel disconnected",
                Stage::Delivery(CommandDelivery::Rejected(_))
                | Stage::Outbound {
                    state: OutboundState::Refused,
                    ..
                } => "rejected before socket write",
                Stage::Delivery(CommandDelivery::WriteFailed)
                | Stage::Outbound {
                    state: OutboundState::WriteFailed,
                    ..
                }
                | Stage::SocketWriteFailed => "socket write failed; outcome unknown",
                Stage::Delivery(CommandDelivery::Written)
                | Stage::Outbound {
                    state: OutboundState::Written,
                    ..
                } => "sent; server outcome unconfirmed",
                Stage::Delivery(CommandDelivery::OutboxAdmitted)
                | Stage::Outbound {
                    state: OutboundState::Queued,
                    ..
                } => "queued; socket write unconfirmed",
                Stage::ChannelAdmitted | Stage::Encoded => "command admitted; delivery unconfirmed",
                Stage::Cast { edge, .. } => match edge {
                    CastEdge::Requested => "cast requested; delivery unconfirmed",
                    CastEdge::LocalRefusal { .. } => "cast refused locally",
                    CastEdge::Admitted => "cast admitted; delivery unconfirmed",
                    CastEdge::CancelRequested => {
                        "cancel requested; authoritative outcome pending"
                    }
                    CastEdge::Started => "cast started; outcome pending",
                    CastEdge::Delayed => "cast delayed; outcome pending",
                    CastEdge::Launched => "cast launched",
                    CastEdge::Failed { .. } => "cast failed",
                    CastEdge::Interrupted { .. } => "cast interrupted",
                    CastEdge::ChannelStarted => "channel started",
                    CastEdge::ChannelUpdated => "channel updated",
                    CastEdge::ChannelEnded => "channel ended",
                    CastEdge::ScopeInvalidated => "cast scope invalidated; unverified",
                },
                _ => "intent recorded; delivery unconfirmed",
            };
            let reason = match event.stage {
                Stage::Delivery(CommandDelivery::Rejected(reason)) => format!(" ({reason:?})"),
                Stage::Cast {
                    edge:
                        CastEdge::LocalRefusal { reason }
                        | CastEdge::Failed { reason }
                        | CastEdge::Interrupted { reason },
                    ..
                } => format!(" (reason {reason:#04x})"),
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
                            && later.elapsed_ms.saturating_sub(event.elapsed_ms) <= 30_000
                            && (!is_panel_stage(later.stage)
                                || later.subject.reload_generation
                                    == event.subject.reload_generation);
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
                            Operation::Confirm => {
                                if later.subject.quest == event.subject.quest
                                    && matches!(later.stage, Stage::QuestSlotAdded)
                                {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Confirm
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
                                    return Some(format!(
                                        "; quest slot added ({})",
                                        if matches > 1 {
                                            "ambiguous"
                                        } else {
                                            "possible match"
                                        }
                                    ));
                                }
                                None
                            }
                            Operation::Abandon => {
                                if matches!(later.stage, Stage::QuestSlotRemoved) {
                                    let matches = self
                                        .entries
                                        .iter()
                                        .filter(|(_, candidate)| {
                                            candidate.subject.operation == Operation::Abandon
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
                                    return Some(format!(
                                        "; quest slot removed ({})",
                                        if matches > 1 {
                                            "ambiguous"
                                        } else {
                                            "possible match"
                                        }
                                    ));
                                }
                                None
                            }
                            Operation::Share => {
                                if let Stage::ServerQuestShareResult { msg } = later.stage {
                                    if later.subject.scope == event.subject.scope
                                        && later.elapsed_ms.saturating_sub(event.elapsed_ms)
                                            <= 30_000
                                    {
                                        let matches = self
                                            .entries
                                            .iter()
                                            .filter(|(_, candidate)| {
                                                candidate.subject.operation == Operation::Share
                                                    && candidate.subject.scope
                                                        == later.subject.scope
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
                                        return Some(format!(
                                            "; share result {msg} ({})",
                                            if matches > 1 {
                                                "ambiguous"
                                            } else {
                                                "possible match; no quest ID"
                                            }
                                        ));
                                    }
                                }
                                None
                            }
                            Operation::Loot | Operation::Equip | Operation::Buy | Operation::Sell => {
                                self.loot_vendor_action_evidence(event, later)
                            }
                            Operation::Observed => None,
                        }
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let cast_evidence = self.cast_native_summary(event).unwrap_or_default();
            output.push(format!(
                "Action {id} {:?} (npc {:?}, quest {:?}, item {:?}, count {:?}, slot {:?}): {status}{reason}{server_evidence}{cast_evidence}{suffix}",
                event.subject.operation,
                event.subject.npc_alias,
                event.subject.quest,
                event.subject.item,
                event.subject.count,
                event.subject.slot,
            ));
            if output.len() == 8 {
                break;
            }
        }
        output
    }

    /// Bounded inspector summary of the login/session/world/reload chain.
    ///
    /// The lines are derived from the same retained ring as the export, so a reader can correlate
    /// them with the event rows. Missing milestones are named as unknown rather than assigned to
    /// the newest attempt, and any loss makes a negative conclusion explicitly unverified.
    #[allow(dead_code)] // the export/coverage owner reads this accessor
    pub(crate) fn session_transition_summaries(&self) -> Vec<String> {
        let mut out = Vec::new();
        let gaps = self.has_telemetry_gaps();

        let login = self.entries.iter().rev().find_map(|(_, event)| match event.stage {
            Stage::LoginMilestone { attempt, milestone } => Some((attempt, milestone)),
            _ => None,
        });
        match login {
            Some((attempt, milestone)) => out.push(format!(
                "login attempt {:?}: last milestone {:?}",
                attempt, milestone
            )),
            None => out.push("login attempt: unknown (no retained login milestone)".to_string()),
        }
        if let Some((phase, attempt, map_id)) =
            self.entries.iter().rev().find_map(|(_, event)| match event.stage {
                Stage::SessionPhase {
                    phase,
                    attempt,
                    map_id,
                } => Some((phase, attempt, map_id)),
                _ => None,
            })
        {
            out.push(format!(
                "last session phase: {:?} (attempt {:?}, map {:?})",
                phase, attempt, map_id
            ));
        }
        if let Some(scope) = self
            .entries
            .iter()
            .filter_map(|(_, event)| event.subject.scope)
            .max()
        {
            // The highest observed scope, not the last ingested one: a delayed old event must
            // not make a retired scope look current.
            out.push(format!(
                "most recent scope: epoch {}, world generation {}",
                scope.0, scope.1
            ));
        }

        let failure = self.entries.iter().rev().find_map(|(_, event)| {
            let (category, attempt) = match event.stage {
                Stage::LoginMilestone {
                    attempt,
                    milestone: LoginMilestone::LoginRefused,
                } => ("login refusal", attempt),
                Stage::LoginMilestone {
                    attempt,
                    milestone: LoginMilestone::LoginFailed,
                } => ("login failure without a refusal category", attempt),
                Stage::LoginMilestone {
                    attempt,
                    milestone: LoginMilestone::RosterFailed,
                } => ("roster failure", attempt),
                Stage::LoginMilestone {
                    attempt,
                    milestone: LoginMilestone::CharacterLoginFailed { .. },
                } => ("character-login refusal", attempt),
                Stage::LoginMilestone {
                    attempt,
                    milestone: LoginMilestone::RealmListFailed,
                } => ("realm-list failure", attempt),
                Stage::SessionEnded {
                    kind: SessionEndKind::Lost,
                } => ("session end (lost link)", None),
                _ => return None,
            };
            Some((category, attempt))
        });
        match failure {
            Some((category, attempt)) => out.push(format!(
                "last failure: {category} (attempt {:?})",
                attempt
            )),
            None => out.push("last failure: none retained".to_string()),
        }
        let last_end = self
            .entries
            .iter()
            .filter_map(|(_, event)| match event.stage {
                Stage::SessionEnded { kind } => {
                    Some((event.subject.scope.unwrap_or((0, 0)), kind))
                }
                _ => None,
            })
            .max_by_key(|(scope, _)| *scope)
            .map(|(_, kind)| kind);
        if let Some(kind) = last_end {
            out.push(format!("last session end: {:?}", kind));
        }

        let transfer = self
            .entries
            .iter()
            .filter_map(|(_, event)| match event.stage {
                Stage::WorldTransfer {
                    map_id,
                    from,
                    to,
                    retired_local,
                    retired_written,
                } => Some((to.unwrap_or((0, 0)), (from, to, map_id, retired_local, retired_written))),
                _ => None,
            })
            .max_by_key(|(to, _)| *to)
            .map(|(_, transfer)| transfer);
        match transfer {
            Some((from, to, map_id, retired_local, retired_written)) => out.push(format!(
                "last transfer: {:?} -> {:?}, map {:?}, retired local={} written={}",
                from, to, map_id, retired_local, retired_written
            )),
            None => out.push("last transfer: none retained".to_string()),
        }

        match self.entries.iter().rev().find_map(|(_, event)| match event.stage {
            Stage::UiReload {
                old_vm_session,
                old_reload_generation,
                new_reload_generation,
            } => Some((old_vm_session, old_reload_generation, new_reload_generation)),
            _ => None,
        }) {
            Some((old_vm_session, old_generation, new_generation)) => out.push(format!(
                "last reload: old VM {:?}, generation {:?} -> {:?}",
                old_vm_session, old_generation, new_generation
            )),
            None => out.push("last reload: none retained".to_string()),
        }
        if let Some((vm_session, reload_generation)) =
            self.entries.iter().rev().find_map(|(_, event)| match event.stage {
                Stage::UiReloadLoaded {
                    vm_session,
                    reload_generation,
                } => Some((vm_session, reload_generation)),
                _ => None,
            })
        {
            out.push(format!(
                "VM after reload attempt: session {:?}, generation {:?} (load result separate)",
                vm_session, reload_generation
            ));
        }

        let current_reload = self.source.0.reload_generation.load(Ordering::Acquire);
        let superseded_projections = self
            .entries
            .iter()
            .filter(|(_, event)| {
                matches!(event.stage, Stage::JournalProjection)
                    && event.subject.reload_generation < current_reload
            })
            .count();
        if superseded_projections > 0 {
            out.push(format!(
                "superseded projections from older VM generations: {superseded_projections}"
            ));
        }
        let stale_pending = self
            .open_interactions
            .iter()
            .filter(|open| open.subject.reload_generation < current_reload)
            .count();
        if stale_pending > 0 {
            out.push(format!(
                "pending actions from older VM generations: {stale_pending} (stale, not resent)"
            ));
        }
        if gaps {
            out.push(format!(
                "telemetry gaps: evicted={}, dropped={}, tracking_lost={}, source_gaps={}; negative conclusions unverified",
                self.evicted,
                self.source.0.dropped.load(Ordering::Relaxed),
                self.interaction_tracking_lost,
                self.source_gaps
            ));
        }
        out.truncate(12);
        out
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
        // The class counters are the authoritative breakdown of rejected ingress (every drop is
        // classified before the nonblocking send is attempted), so the reported global drop
        // count is their sum. This keeps `dropped == sum(per-class dropped)` exact for any
        // snapshot, including `unclassified`, instead of racing two independent atomics.
        let dropped = EvidenceClass::ALL
            .iter()
            .map(|class| self.source.producer_drops_for_class(*class))
            .fold(0u64, u64::saturating_add);
        (self.received, self.evicted, dropped)
    }

    /// Delivery counts across retained entries (admitted, written, rejected, write-failed).
    pub(crate) fn delivery_counts(&self) -> (u64, u64, u64, u64) {
        let mut admitted = 0u64;
        let mut written = 0u64;
        let mut rejected = 0u64;
        let mut failed = 0u64;
        for (_, event) in &self.entries {
            if let Stage::Delivery(delivery) = event.stage {
                match delivery {
                    crate::net::CommandDelivery::OutboxAdmitted => admitted = admitted.saturating_add(1),
                    crate::net::CommandDelivery::Written => written = written.saturating_add(1),
                    crate::net::CommandDelivery::Rejected(_) => rejected = rejected.saturating_add(1),
                    crate::net::CommandDelivery::WriteFailed => failed = failed.saturating_add(1),
                }
            }
        }
        (admitted, written, rejected, failed)
    }

    /// Additive per-vector evidence summary derived from the same bounded snapshot that
    /// supplies the exported event rows.  Classification goes through `vector_for_stage`
    /// so both export paths agree.
    pub(crate) fn diagnostic_vectors(&self) -> Vec<DiagnosticVectorEvidence> {
        let interaction_tracking_lost = self.interaction_tracking_lost;
        let sampled_out_total = self.sampled_updates();
        let update_packets_seen = self.update_packets_seen();
        let mut out = Vec::with_capacity(DIAGNOSTIC_VECTOR_NAMES.len());
        for &vector in DIAGNOSTIC_VECTOR_NAMES {
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
            let retained_evictions = self.retained_evictions[diagnostic_vector_index(vector)];
            let producer_drops = self.source.producer_drops_for_vector(vector);
            let mut limiting_factors = vec!["event_driven_reporting".to_string()];
            if vector == "allowlisted_packets" {
                limiting_factors.push("sampling".to_string());
            }
            if source_specific_loss {
                limiting_factors.push("source_specific_loss".to_string());
            }
            if producer_drops > 0 {
                limiting_factors.push("producer_drop".to_string());
            }
            // Retained eviction is a per-vector fact, never inferred from the global counter.
            if retained_evictions > 0 {
                limiting_factors.push("retained_eviction".to_string());
            }
            if !available {
                limiting_factors.push("uninstrumented".to_string());
            }
            let (sampled_out, sampling_interval, sampling_denominator) = if vector == "allowlisted_packets" {
                (Some(sampled_out_total), Some(64u64), Some(update_packets_seen))
            } else {
                (None, None, None)
            };
            let value = DiagnosticVectorEvidence {
                vector: vector.to_string(),
                instrumentation_available: available,
                retained_observations,
                latest_retained_sequence,
                retained_evictions,
                producer_drops,
                sampled_out,
                sampling_interval,
                sampling_denominator,
                limiting_factors,
                absence_not_proof: true,
            };
            out.push(value);
        }
        out
    }

    /// Bounded retention-pressure summary for the inspector and export header.
    ///
    /// Lines are capped in count and byte length.  Per-vector evictions are listed only for
    /// vectors that actually lost retained observations; the printed sum is checked against the
    /// global counter so a reader can verify the invariant.  Zero retained observations is not a
    /// healthy reading, so that caveat is always present.
    pub(crate) fn retention_summary(&self) -> Vec<String> {
        const MAX_LINES: usize = 13;
        const MAX_EVICTION_LINES: usize = 4;
        fn bound(mut line: String) -> String {
            if line.len() > SNAPSHOT_LINE_BYTES {
                let mut boundary = SNAPSHOT_LINE_BYTES;
                while boundary > 0 && !line.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                line.truncate(boundary);
            }
            line
        }

        let first = self.entries.keys().next().copied();
        let highest = self.entries.keys().next_back().copied();
        let (received, evicted, _) = self.counters();
        let mut lines = Vec::with_capacity(MAX_LINES);

        let class_line = |classes: &[EvidenceClass]| -> (u64, u64, u64) {
            let mut retained = 0u64;
            let mut evicted = 0u64;
            let mut dropped = 0u64;
            for &class in classes {
                retained = retained.saturating_add(self.retained_by_class(class));
                evicted = evicted.saturating_add(self.evicted_by_class[class.index()]);
                dropped = dropped.saturating_add(self.source.producer_drops_for_class(class));
            }
            (retained, evicted, dropped)
        };
        let action = class_line(&[EvidenceClass::Action]);
        let health = class_line(&[EvidenceClass::Health]);
        let protocol = class_line(&[EvidenceClass::Protocol]);
        let ui_input = class_line(&[EvidenceClass::UiInput]);
        let unclassified = class_line(&[EvidenceClass::Unclassified]);
        let totals = (
            action.0 + health.0 + protocol.0 + ui_input.0 + unclassified.0,
            action.1 + health.1 + protocol.1 + ui_input.1 + unclassified.1,
            action.2 + health.2 + protocol.2 + ui_input.2 + unclassified.2,
        );
        // Report the class-sum drop count so the summary line and the per-class lines agree.
        let dropped = totals.2;
        lines.push(format!(
            "retention: retained seq {:?}..{:?} of {RETAINED_CAPACITY}; received={received} evicted={evicted} dropped={dropped} source_gaps={}",
            first, highest, self.source_gaps
        ));
        lines.push(format!(
            "evidence classes: action held={} evicted={} dropped={}; health held={} evicted={} dropped={}",
            action.0, action.1, action.2, health.0, health.1, health.2
        ));
        lines.push(format!(
            "evidence classes: protocol held={} evicted={} dropped={}; ui_input held={} evicted={} dropped={}",
            protocol.0, protocol.1, protocol.2, ui_input.0, ui_input.1, ui_input.2
        ));
        lines.push(format!(
            "evidence classes: unclassified held={} evicted={} dropped={}; sum held={} evicted={} dropped={}",
            unclassified.0, unclassified.1, unclassified.2,
            totals.0, totals.1, totals.2
        ));
        lines.push(format!(
            "health retention: budget={HEALTH_RETENTION_BUDGET} reserve={HEALTH_RETENTION_RESERVE}; class-aware eviction reclaims the oldest health row first, then action/protocol/unclassified"
        ));
        lines.push(format!(
            "ui input retention: budget={UI_INPUT_RETENTION_BUDGET} reserve={UI_INPUT_RETENTION_RESERVE}; incoming UiInput reclaims oldest UiInput first, then health above reserve; refuses if core evidence would be displaced"
        ));
        lines.push(
            "zero retained observations is not proof that no event occurred (absence_not_proof=true)"
                .to_string(),
        );

        let mut retained_by_vector = [0u64; DIAGNOSTIC_VECTOR_COUNT];
        for (_, event) in &self.entries {
            let vector = vector_for_stage(&event.stage).unwrap_or("unclassified");
            let index = diagnostic_vector_index(vector);
            retained_by_vector[index] = retained_by_vector[index].saturating_add(1);
        }
        let mut ranked: Vec<(usize, u64)> = retained_by_vector
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, count)| *count > 0)
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let top: Vec<String> = ranked
            .iter()
            .take(4)
            .map(|(index, count)| format!("{}={count}", DIAGNOSTIC_VECTOR_NAMES[*index]))
            .collect();
        lines.push(format!(
            "high-volume retained: {}",
            if top.is_empty() { "none".to_string() } else { top.join(" ") }
        ));
        lines.push(
            "cadence: asset=failure/missed or 30s heartbeat; frame=5s aggregate; replica=5s coalesced; ui_press/click, scene, movement, audio, script=change-driven"
                .to_string(),
        );

        let mut vector_entries = Vec::new();
        let mut vector_sum = 0u64;
        for (index, &vector) in DIAGNOSTIC_VECTOR_NAMES.iter().enumerate() {
            let count = self.retained_evictions[index];
            if count > 0 {
                vector_sum = vector_sum.saturating_add(count);
                vector_entries.push(format!("{vector}={count}"));
            }
        }
        let mut packed = Vec::new();
        let mut current =
            format!("retained_evictions (sum={vector_sum}, global_evicted={evicted}):");
        if vector_entries.is_empty() {
            current.push_str(" none");
            packed.push(current);
        } else {
            for entry in vector_entries {
                if !current.ends_with(':') && current.len() + 1 + entry.len() > SNAPSHOT_LINE_BYTES {
                    packed.push(current);
                    if packed.len() >= MAX_EVICTION_LINES - 1 {
                        packed.push(
                            "retained_evictions (further nonzero vectors omitted; sum above is complete)"
                                .to_string(),
                        );
                        current.clear();
                        break;
                    }
                    current = "retained_evictions (cont.):".to_string();
                }
                current.push(' ');
                current.push_str(&entry);
            }
            if !current.is_empty() {
                packed.push(current);
            }
        }
        packed.truncate(MAX_EVICTION_LINES);
        lines.extend(packed);

        lines.truncate(MAX_LINES);
        lines.into_iter().map(bound).collect()
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
        // Correlate each retained attempt to its latest terminal evidence. A frame failure and
        // the restore decision it triggered are one terminal family, so the failure is not
        // reported as "earlier evidence" when its restore is the actual outcome.
        let gaps = self.has_telemetry_gaps();
        let mut attempts: Vec<u64> = Vec::new();
        for (_, event) in self.entries.iter().rev() {
            let Stage::Hotpatch { event } = event.stage else {
                continue;
            };
            for attempt in patch_event_attempts(event).into_iter().flatten() {
                if !attempts.contains(&attempt) {
                    attempts.push(attempt);
                }
            }
        }
        attempts
            .into_iter()
            .take(8)
            .map(|attempt| {
                let mut staged = false;
                let mut terminals: Vec<PatchEvent> = Vec::new();
                for (_, event) in &self.entries {
                    let Stage::Hotpatch { event } = event.stage else {
                        continue;
                    };
                    if matches!(
                        event,
                        PatchEvent::CandidateStaged { attempt: staged_attempt }
                            if staged_attempt == attempt
                    ) {
                        staged = true;
                    }
                    if patch_event_is_terminal_for(event, attempt) {
                        terminals.push(event);
                    }
                }
                let has_restore_terminal = terminals.iter().any(|event| match *event {
                    PatchEvent::RestoredPrevious { failed_attempt, .. } => {
                        failed_attempt == Some(attempt)
                    }
                    PatchEvent::RestoreUnavailable { failed_attempt } => {
                        failed_attempt == Some(attempt)
                    }
                    _ => false,
                });
                if has_restore_terminal {
                    terminals.retain(|event| {
                        !matches!(
                            event,
                            PatchEvent::FrameFailed { attempt: Some(failed), .. }
                                if *failed == attempt
                        )
                    });
                }
                let terminal_count = terminals.len();
                let terminal = terminals.last().copied();
                let earlier = if terminal_count > 1 {
                    " (earlier terminal evidence exists)"
                } else {
                    ""
                };
                let gap = if gaps {
                    " (trace has gaps; outcome unverified)"
                } else {
                    ""
                };
                let line = match terminal {
                    Some(event) => patch_event_summary(event, attempt),
                    None if staged => format!(
                        "attempt {attempt}: staged; no terminal event retained (unresolved)"
                    ),
                    None => format!(
                        "attempt {attempt}: no terminal evidence retained (ambiguous/unresolved)"
                    ),
                };
                format!("{line}{earlier}{gap}")
            })
            .collect()
    }
    #[cfg(feature = "hotpatch")]
    pub(crate) fn hotpatch_attempts(&self) -> Vec<String> {
        self.hotpatch_attempt_correlation()
    }

    pub(crate) fn sampled_updates(&self) -> u64 {
        self.source.0.update_packets_sampled_out.load(Ordering::Relaxed)
    }

    pub(crate) fn update_packets_seen(&self) -> u64 {
        self.source.update_packets_seen()
    }

    pub(crate) fn update_packets_sampled(&self) -> u64 {
        self.update_packets_seen()
            .saturating_sub(self.sampled_updates())
    }

    pub(crate) fn movement_coalesced(&self) -> u64 {
        self.source.movement_coalesced()
    }

    pub(crate) fn movement_refused(&self) -> u64 {
        self.source.movement_refused()
    }

    pub(crate) fn coverage(&self) -> DiagnosticCoverage {
        let vectors = self.diagnostic_vectors();
        let mut instrumented_paths = Vec::new();
        let mut compiled_out_paths = Vec::new();
        for vector in &vectors {
            if vector.vector == "unclassified" {
                continue;
            }
            if vector.instrumentation_available {
                instrumented_paths.push(vector.vector.clone());
            } else {
                compiled_out_paths.push(vector.vector.clone());
            }
        }
        let not_yet_instrumented = vec![
            "visible_pixels".to_string(),
            "frame_occlusion_owner".to_string(),
            "audio_audibility".to_string(),
            "movement_causality".to_string(),
            "asset_dependency_revisions".to_string(),
            // Module validation/rollback is instrumented; the trust decision itself is not.
            // Hotpatch modules are explicitly trusted local native code, and the host provides
            // no memory isolation or sandbox, so no trust boundary is claimed here.
            "hotpatch_trust_policy".to_string(),
        ];
        let mut evidence_classes = Vec::with_capacity(EVIDENCE_CLASS_COUNT);
        let mut class_retained = 0u64;
        let mut class_evicted = 0u64;
        let mut class_dropped = 0u64;
        for class in EvidenceClass::ALL {
            let retained = self.retained_by_class(class);
            let evicted = self.evicted_by_class[class.index()];
            let dropped = self.source.producer_drops_for_class(class);
            class_retained = class_retained.saturating_add(retained);
            class_evicted = class_evicted.saturating_add(evicted);
            class_dropped = class_dropped.saturating_add(dropped);
            evidence_classes.push(EvidenceClassCoverage {
                class: class.label(),
                retained,
                evicted,
                dropped,
            });
        }
        let (global_received, global_evicted, _global_dropped) = self.counters();
        let unclassified = &evidence_classes[EvidenceClass::Unclassified.index()];
        let evidence_class_totals = EvidenceClassTotals {
            retained: class_retained,
            evicted: class_evicted,
            dropped: class_dropped,
            global_received,
            global_evicted,
            // Rejected ingress is classified before the nonblocking send, so the authoritative
            // global drop count is the class sum rather than a second racing atomic.
            global_dropped: class_dropped,
            unclassified_retained: unclassified.retained,
            unclassified_evicted: unclassified.evicted,
            unclassified_dropped: unclassified.dropped,
            reconciles: class_retained == self.entries.len() as u64
                && class_evicted == global_evicted
                && global_received == class_retained.saturating_add(class_evicted),
        };
        DiagnosticCoverage {
            instrumented_paths,
            compiled_out_paths,
            event_driven_paths: vectors
                .iter()
                .filter(|v| v.instrumentation_available && v.vector != "unclassified")
                .map(|v| v.vector.clone())
                .collect(),
            sampled_paths: vec!["allowlisted_packets".to_string()],
            paths_with_producer_loss_counters: vectors
                .iter()
                .filter(|v| v.instrumentation_available && v.vector != "unclassified")
                .map(|v| v.vector.clone())
                .collect(),
            paths_with_retained_evictions: vectors
                .iter()
                .filter(|v| v.retained_evictions > 0)
                .map(|v| v.vector.clone())
                .collect(),
            not_yet_instrumented,
            hotpatch_compiled: cfg!(feature = "hotpatch"),
            export_and_inspector_compiled: cfg!(feature = "dev"),
            sampling: SamplingCoverage {
                object_update_packets_seen: self.update_packets_seen(),
                object_update_packets_sampled: self.update_packets_sampled(),
                object_update_packets_sampled_out: self.sampled_updates(),
                object_update_sampling_interval: 64,
                movement_packets_coalesced: self.movement_coalesced(),
                movement_packets_refused: self.movement_refused(),
            },
            evidence_classes,
            evidence_class_totals,
            health_retention_budget: HEALTH_RETENTION_BUDGET as u64,
            health_retention_reserve: HEALTH_RETENTION_RESERVE as u64,
            ui_input_retention_budget: UI_INPUT_RETENTION_BUDGET as u64,
            ui_input_retention_reserve: UI_INPUT_RETENTION_RESERVE as u64,
        }
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
    pub(crate) fn export(&self, build: benilla_world::build_id::BuildId, protocol_mode: &'static str) -> Result<(), String> {
        let directory = crate::local_state::home()
            .ok_or_else(|| "Local report storage is unavailable".to_owned())?
            .join("Logs").join("InteractionDiagnostics");
        if self.source.0.exporting.swap(true, Ordering::AcqRel) {
            return Err("A report is already being saved".into());
        }
        self.source.0.export_status.store(1, Ordering::Release);
        let ingress = self.source.0.clone();
        let rows: Vec<(u64, Event)> = self
            .entries
            .iter()
            .map(|(seq, event)| (*seq, event.clone()))
            .collect();
        let incidents: Vec<_> = self.incidents.iter().map(|incident| {
            let missing: Vec<_> = incident.evidence.iter().copied()
                .filter(|seq| !self.entries.contains_key(seq))
                .collect();
            serde_json::json!({ "record": incident, "missing_event_sequences": missing })
        }).collect();
        let incidents_evicted = self.incidents_evicted;
        let interaction_tracking_lost = self.interaction_tracking_lost;
        let ingress_pending = self.incoming.len();
        let source_gaps = self.source_gaps;
        #[cfg(feature = "hotpatch")]
        let last_observed_patch_digest = self.last_observed_patch_digest;
        #[cfg(not(feature = "hotpatch"))]
        let last_observed_patch_digest: Option<[u8; 20]> = None;
        let sampled_updates = self.sampled_updates();
        let update_packets_seen = self.update_packets_seen();
        let update_packets_sampled = self.update_packets_sampled();
        let movement_coalesced = self.movement_coalesced();
        let movement_refused = self.movement_refused();
        let coverage = self.coverage();
        // Use the same coverage snapshot for the top-level counters so `dropped`,
        // `evicted` and `received` always agree with `evidence_class_totals`, even while
        // producers are still running.
        let received = coverage.evidence_class_totals.global_received;
        let evicted = coverage.evidence_class_totals.global_evicted;
        let dropped = coverage.evidence_class_totals.dropped;
        let diagnostic_vectors = self.diagnostic_vectors();
        let retention_summary = self.retention_summary();
        let first_sequence = self.entries.keys().next().copied();
        let highest_sequence = self.entries.keys().next_back().copied();
        let npc_aliases_assigned = self.source.npc_alias_count();
        let run_id = self.source.0.run_id.clone();
        let asset_profile = self.source.0.asset_profile.clone();
        let action_summaries = self.action_summaries();
        let panel_correlations = self.panel_correlations();
        let slot_correlations = self.slot_correlations();
        let turn_in_correlations = self.turn_in_correlations();
        let ui_correlations = self.ui_error_correlations();
        let ui_click_correlations = self.ui_click_correlations();
        let ui_click_outcomes = self.ui_click_outcomes();
        let ui_input_rows_lost = self.ui_input_rows_lost();
        let cast_chains = self.cast_correlations();
        let loot_vendor = self.loot_vendor_correlations();
        let quest_ui = self.quest_ui_correlations();
        let session = self.session_transition_summaries();
        let movement = self.movement_pressure();
        let movement_corrections = self.movement_correction_attribution();
        let audio = self.audio_output_health();
        let assets = self.asset_window_summary();
        #[cfg(feature = "hotpatch")]
        let hotpatch = self.hotpatch_attempt_correlation();
        #[cfg(not(feature = "hotpatch"))]
        let hotpatch: Vec<String> = Vec::new();
        let spawn = std::thread::Builder::new()
            .name("interaction-diagnostics-export".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    use std::io::{Read, Write};
                    use sha1::{Digest, Sha1};
                    std::fs::create_dir_all(&directory)?;
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?.as_nanos();
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
                    let evidence_class_retention = coverage.evidence_classes.clone();
                    let evidence_class_totals = coverage.evidence_class_totals.clone();
                    let mut text = serde_json::json!({
                        "schema": "rustwrath.interaction-diagnostics.v1",
                        "version": 1,
                        "run_id": run_id,
                        "protocol_mode": protocol_mode,
                        "coverage": coverage,
                        "evidence_class_retention": evidence_class_retention,
                        "evidence_class_totals": evidence_class_totals,
                        "health_retention_budget": HEALTH_RETENTION_BUDGET,
                        "health_retention_reserve": HEALTH_RETENTION_RESERVE,
                        "ui_input_retention_budget": UI_INPUT_RETENTION_BUDGET,
                        "ui_input_retention_reserve": UI_INPUT_RETENTION_RESERVE,
                        "build_git_sha": if build.sha.is_empty() { None } else { Some(build.sha) },
                        "build_dirty_state": "unknown",
                        "executable_file_sha1": file_hash,
                        "asset_profile_requested": asset_profile,
                        "cargo_profile": if build.profile.is_empty() { None } else { Some(build.profile) },
                        "received": received,
                        "first_retained_sequence": first_sequence,
                        "highest_retained_sequence": highest_sequence,
                        "retained_capacity": RETAINED_CAPACITY,
                        "retention_summary": retention_summary,
                        "evicted": evicted,
                        "dropped": dropped,
                        "interaction_tracking_lost": interaction_tracking_lost,
                        "npc_aliases_assigned": npc_aliases_assigned,
                        "upstream_gap_events": source_gaps,
                        "last_observed_patch_sha1_bytes": last_observed_patch_digest,
                        "object_update_packets_seen": update_packets_seen,
                        "object_update_packets_sampled": update_packets_sampled,
                        "object_update_packets_sampled_out": sampled_updates,
                        "object_update_sampling_interval": 64,
                        "movement_packets_coalesced": movement_coalesced,
                        "movement_packets_refused": movement_refused,
                        "incidents": incidents,
                        "incidents_evicted": incidents_evicted,
                        "ingress_pending_at_snapshot": ingress_pending,
                        "action_summaries": action_summaries,
                        "panel_correlations": panel_correlations,
                        "slot_correlations": slot_correlations,
                        "turn_in_correlations": turn_in_correlations,
                        "ui_error_correlations": ui_correlations,
                        "ui_click_correlations": ui_click_correlations,
                        "ui_click_outcomes": ui_click_outcomes,
                        "ui_input_rows_lost": ui_input_rows_lost,
                        "cast_correlations": cast_chains,
                        "loot_vendor_correlations": loot_vendor,
                        "quest_ui_correlations": quest_ui,
                        "session_transition_summaries": session,
                        "movement_pressure": movement,
                        "movement_correction_attribution": movement_corrections,
                        "audio_output_health": audio,
                        "asset_window_summaries": assets,
                        "hotpatch_attempts": hotpatch,
                        "diagnostic_vectors": diagnostic_vectors,
                    }).to_string();
                    text.push('\n');
                    for (sequence, event) in rows {
                        text.push_str(&serde_json::to_string(&(sequence, event))?);
                        text.push('\n');
                    }
                    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
                    file.write_all(text.as_bytes())?;
                    file.as_file().sync_all()?;
                    let mut path = directory.join(format!("interaction-{timestamp}.jsonl"));
                    let mut suffix = 0_u64;
                    loop {
                        match file.persist_noclobber(&path) {
                            Ok(_) => break,
                            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                                file = error.file;
                                suffix += 1;
                                path = directory.join(format!("interaction-{timestamp}-{suffix}.jsonl"));
                            }
                            Err(error) => return Err(error.error.into()),
                        }
                    }
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
        dropped_by_vector: (0..DIAGNOSTIC_VECTOR_NAMES.len())
            .map(|_| AtomicU64::new(0))
            .collect(),
        dropped_by_class: std::array::from_fn(|_| AtomicU64::new(0)),
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
        ui_dispatch_counter: AtomicU64::new(1),
        ui_dispatches: Mutex::new(VecDeque::with_capacity(8)),
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
        entries: BTreeMap::new(),
        class_sequences: std::array::from_fn(|_| BTreeSet::new()),
        incidents: VecDeque::with_capacity(64),
        open_interactions: VecDeque::with_capacity(MAX_OPEN_INTERACTIONS),
        cast_candidates: VecDeque::with_capacity(MAX_OPEN_INTERACTIONS),
        interaction_tracking_lost: 0,
        incidents_evicted: 0,
        received: 0,
        evicted: 0,
        evicted_by_class: [0; EVIDENCE_CLASS_COUNT],
        retained_evictions: [0; DIAGNOSTIC_VECTOR_COUNT],
        source_gaps: 0,
        #[cfg(feature = "hotpatch")]
        last_observed_patch_digest: None,
        #[cfg(feature = "dev")]
        inspector_snapshot: None,
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

#[derive(Default)]
struct AssetWindowState {
    scan_start: f64,
    last_emit: f64,
    failures: u64,
    missed: u64,
}

fn poll_asset_failures(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    events: Option<Res<Messages<bevy::asset::UntypedAssetLoadFailedEvent>>>,
    time: Res<Time>,
    mut cursor: Local<bevy::ecs::message::MessageCursor<bevy::asset::UntypedAssetLoadFailedEvent>>,
    mut window: Local<AssetWindowState>,
) {
    let Some(events) = events else { return };
    let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
    let now = time.elapsed_secs_f64();
    if window.scan_start == 0.0 {
        window.scan_start = now;
        window.last_emit = now;
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
    window.failures = window.failures.saturating_add(failures_this);
    window.missed = window.missed.saturating_add(missed as u64);
    let elapsed = now - window.scan_start;
    // A real failure or missed-event burst is coalesced into at most one window per scan
    // interval, so the exact count survives without one event per failure.  The surviving
    // zero-value heartbeat only supplies a denominator for a healthy vector.
    let condition_due = (window.failures > 0 || window.missed > 0) && elapsed >= ASSET_SCAN_WINDOW_SECS;
    let heartbeat_due = now - window.last_emit >= ASSET_HEARTBEAT_SECS;
    if condition_due || heartbeat_due {
        let window_ms = (elapsed * 1000.0).round() as u64;
        source.observe(scope, None, Stage::AssetWindow {
            failures: window.failures,
            missed: window.missed,
            window_ms: window_ms.max(1),
        });
        window.failures = 0;
        window.missed = 0;
        window.scan_start = now;
        window.last_emit = now;
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
    if now - window.start >= FRAME_WINDOW_SECS {
        let window_ms = ((now - window.start) * 1000.0).round().max(1.0) as u64;
        let frames = window.frames.max(1);
        let mean_us = window.sum_us / u64::from(frames);
        source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
            Stage::FrameWindow { frames, mean_us, max_us: window.max_us, window_ms });
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
    // Already change-driven: the poll only decides whether a real count change occurred.
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
    // Already change-driven: the one-second poll emits only a nonzero delta or queue-depth change.
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

#[derive(Default)]
struct ReplicaWindow {
    observed: Option<(u64, u64, u64)>,
    emitted: Option<(u64, u64, u64)>,
    next_poll: f64,
    last_emit: f64,
}

fn poll_replica(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    time: Res<Time>,
    mut window: Local<ReplicaWindow>,
) {
    let Some(view) = view else { return };
    if view.phase.is_none() {
        window.observed = None;
        window.emitted = None;
        return;
    }
    let now = time.elapsed_secs_f64();
    if now < window.next_poll {
        return;
    }
    window.next_poll = now + 1.0;
    let current = (view.session_epoch, view.world_generation, view.state_revision);
    if window.observed != Some(current) {
        window.observed = Some(current);
    }
    // Revisions are monotonic.  A change is still reported after every coalescing window, so
    // the latest revision and its progress remain visible without one event per revision.
    let due = window.emitted.is_none() || now - window.last_emit >= REPLICA_WINDOW_SECS;
    if window.observed != window.emitted && due {
        window.emitted = window.observed;
        window.last_emit = now;
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
        let Ok(mut event) = history.incoming.try_recv() else { break };
        history.received += 1;
        let seq = history.received;
        let mut captured_opening_note: Option<EvidenceNote> = None;
        if matches!(event.stage,
            Stage::NativeEventsLost { .. } | Stage::ScriptRowsLost { .. }
                | Stage::ScriptRowsEvicted { .. } | Stage::AssetEventsLost { .. })
        {
            history.source_gaps = history.source_gaps.saturating_add(1);
        }
        #[cfg(feature = "hotpatch")]
        match event.stage {
            Stage::Hotpatch {
                event: PatchEvent::Activated { digest, .. },
            }
            | Stage::Hotpatch {
                event: PatchEvent::RestoredPrevious { digest, .. },
            } => history.last_observed_patch_digest = digest,
            Stage::Hotpatch {
                event:
                    PatchEvent::FrameFailed { .. }
                    | PatchEvent::RestoreUnavailable { .. },
            } => history.last_observed_patch_digest = None,
            _ => {}
        }

        // Track bounded open interactions for lifecycle retirement and server outcome correlation.
        if event.subject.interaction != 0 {
            match event.stage {
                Stage::Intent
                    if matches!(
                        event.subject.operation,
                        Operation::Open
                            | Operation::Select
                            | Operation::Accept
                            | Operation::RequestReward
                            | Operation::ChooseReward
                            | Operation::Confirm
                            | Operation::Abandon
                            | Operation::Share
                            | Operation::Loot
                            | Operation::Equip
                            | Operation::Buy
                            | Operation::Sell
                    ) =>
                {
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
                            start_seq: seq,
                            start_elapsed_ms: event.elapsed_ms,
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
                    history.note_written_cast(seq, &event);
                }
                Stage::GuardRefused
                | Stage::NoWorld
                | Stage::AlreadyPending
                | Stage::TrackingCapacity
                | Stage::ChannelDisconnected
                | Stage::Delivery(CommandDelivery::Rejected(_))
                | Stage::Delivery(CommandDelivery::WriteFailed)
                | Stage::Outbound {
                    state: OutboundState::Refused | OutboundState::WriteFailed,
                    ..
                }
                | Stage::SocketWriteFailed
                | Stage::Cast {
                    edge: CastEdge::LocalRefusal { .. } | CastEdge::CancelRequested,
                    ..
                } => {
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
                let ended_scope = event.subject.scope;
                let _ = history.retire_scope(ended_scope, &event, seq);
            }
            Stage::WorldTransfer { .. }
            | Stage::DomainEvent {
                event: DomainEvent::Worldport,
            } => {
                // WorldTransfer carries the new world_generation in event.subject.scope;
                // retire only interactions from a strictly older scope in the same session.
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
                let mut retired_local = 0u64;
                let mut retired_written = 0u64;
                for stale in stale_scopes {
                    let (local, written) = history.retire_scope(stale, &event, seq);
                    retired_local = retired_local.saturating_add(local);
                    retired_written = retired_written.saturating_add(written);
                }
                // Replace the placeholder counts with the bounded summary of what this
                // transfer actually retired; one event, not one per interaction.
                if let Stage::WorldTransfer { map_id, from, to, .. } = event.stage {
                    event.stage = Stage::WorldTransfer {
                        map_id,
                        from,
                        to,
                        retired_local,
                        retired_written,
                    };
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
                    // A panel is a VM presentation fact. An action admitted in an older VM
                    // generation is superseded, not matched to the replacement VM's panel.
                    if is_panel_stage(event.stage)
                        && open.subject.reload_generation != event.subject.reload_generation
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
                let writes: Vec<(u64, u64)> = matching_indices
                    .iter()
                    .filter_map(|&idx| match history.open_interactions[idx].state {
                        OpenState::Written { seq: write_seq, elapsed_ms } => {
                            Some((write_seq, elapsed_ms))
                        }
                        OpenState::Local => None,
                    })
                    .collect();
                let first_open = &history.open_interactions[matching_indices[0]];
                let opening_note = if first_open.start_seq != 0
                    && !history.entries.contains_key(&first_open.start_seq)
                {
                    Some(EvidenceNote::recovered(
                        first_open.start_seq,
                        first_open.start_elapsed_ms,
                        EvidenceClass::Action,
                        Stage::Intent,
                    ))
                } else {
                    None
                };
                let first_subject = first_open.subject;
                let chain = history.correlate_written_evidence(
                    writes.into_iter(),
                    seq,
                    &event,
                    opening_note,
                );
                history.push_incident(
                    first_subject,
                    IncidentOutcome::AmbiguousServerOutcome,
                    IncidentConfidence::Ambiguous,
                    event.stage,
                    chain,
                );
            }
            for idx in matching_indices.into_iter().rev() {
                history.open_interactions.remove(idx);
            }
        }

        // Resolve a native cast edge against written local casts before classifying it. Only an
        // authoritative terminal failure consumes the candidate; Started/Launched and channel
        // edges remain continuations of the same attempt.
        let cast_terminal = match event.stage {
            Stage::Cast {
                edge: CastEdge::Started | CastEdge::Launched | CastEdge::ChannelStarted
                    | CastEdge::ChannelUpdated | CastEdge::ChannelEnded,
                ..
            } => Some(history.correlate_cast_edge(&event, false)),
            Stage::Cast {
                edge: CastEdge::Failed { .. } | CastEdge::Interrupted { .. },
                ..
            } => Some(history.correlate_cast_edge(&event, true)),
            Stage::Cast {
                edge: CastEdge::ScopeInvalidated,
                ..
            } => Some(history.correlate_cast_edge(&event, true)),
            _ => None,
        };
        // A correlated positive launch or authoritative terminal failure closes the local write's
        // open lifecycle, so a later scope change cannot re-report it as unverified.
        if matches!(
            event.stage,
            Stage::Cast {
                edge: CastEdge::Launched
                    | CastEdge::ChannelStarted
                    | CastEdge::Failed { .. }
                    | CastEdge::Interrupted { .. }
                    | CastEdge::ScopeInvalidated,
                ..
            }
        ) {
            if let Some((_, subject)) = cast_terminal {
                if subject.interaction != 0 {
                    if captured_opening_note.is_none() {
                        let candidate_data = history
                            .cast_candidates
                            .iter()
                            .find(|candidate| candidate.subject.interaction == subject.interaction)
                            .map(|candidate| {
                                (
                                    candidate.started_seq,
                                    candidate.started_elapsed_ms,
                                    candidate.started_from_request,
                                    candidate.started_from_admission,
                                    candidate.spell_id,
                                    candidate.wire_cast_count,
                                )
                            });
                        if let Some((
                            started_seq,
                            started_elapsed_ms,
                            started_from_request,
                            started_from_admission,
                            spell_id,
                            wire_cast_count,
                        )) = candidate_data
                        {
                            if started_seq != 0
                                && !history.entries.contains_key(&started_seq)
                            {
                                let stage = if started_from_request {
                                    Stage::Cast {
                                        spell_id: Some(spell_id),
                                        attempt: None,
                                        wire_cast_count,
                                        edge: CastEdge::Requested,
                                    }
                                } else if started_from_admission {
                                    Stage::Cast {
                                        spell_id: Some(spell_id),
                                        attempt: None,
                                        wire_cast_count,
                                        edge: CastEdge::Admitted,
                                    }
                                } else {
                                    Stage::Delivery(CommandDelivery::Written)
                                };
                                captured_opening_note = Some(EvidenceNote::recovered(
                                    started_seq,
                                    started_elapsed_ms,
                                    EvidenceClass::Action,
                                    stage,
                                ));
                            }
                        } else if let Some(open) = history
                            .open_interactions
                            .iter()
                            .find(|open| open.subject.interaction == subject.interaction)
                        {
                            if open.start_seq != 0
                                && !history.entries.contains_key(&open.start_seq)
                            {
                                captured_opening_note = Some(EvidenceNote::recovered(
                                    open.start_seq,
                                    open.start_elapsed_ms,
                                    EvidenceClass::Action,
                                    Stage::Intent,
                                ));
                            }
                        }
                    }
                    history
                        .open_interactions
                        .retain(|open| open.subject.interaction != subject.interaction);
                }
            }
        }
        // Loot, vendor-buy and equipment observations are authoritative outcomes for a written
        // action, but they only resolve it when exactly one written action can explain them. A
        // duplicate same-subject write stays ambiguous. An equip's item push is NOT authoritative:
        // it says the item arrived, not that it occupied an equipment slot.
        if matches!(
            event.stage,
            Stage::LootSlotRemoved { .. }
                | Stage::LootMoneyNotify { .. }
                | Stage::LootMoneyCleared
                | Stage::LootResponseOpened
                | Stage::LootReleasedObserved
                | Stage::VendorBuyResult { .. }
                | Stage::EquipmentSlotChanged { .. }
        ) {
            let operation = match event.stage {
                Stage::LootSlotRemoved { .. }
                | Stage::LootMoneyNotify { .. }
                | Stage::LootMoneyCleared
                | Stage::LootResponseOpened
                | Stage::LootReleasedObserved => Some(Operation::Loot),
                Stage::VendorBuyResult { .. } => Some(Operation::Buy),
                Stage::EquipmentSlotChanged { .. } => Some(Operation::Equip),
                _ => None,
            };
            if let Some(operation) = operation {
                let matching_indices: Vec<usize> = history
                    .open_interactions
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, open)| {
                        let OpenState::Written { elapsed_ms, .. } = open.state else {
                            return None;
                        };
                        if open.subject.operation != operation
                            || open.subject.scope != event.subject.scope
                            || event.elapsed_ms < elapsed_ms
                            || event.elapsed_ms.saturating_sub(elapsed_ms) > 30_000
                        {
                            return None;
                        }
                        loot_vendor_evidence_matches_action(&open.subject, &event)
                            .then_some(idx)
                    })
                    .collect();
                if matching_indices.len() > 1 {
                    let writes: Vec<(u64, u64)> = matching_indices
                        .iter()
                        .filter_map(|&idx| match history.open_interactions[idx].state {
                            OpenState::Written { seq: write_seq, elapsed_ms } => {
                                Some((write_seq, elapsed_ms))
                            }
                            OpenState::Local => None,
                        })
                        .collect();
                    let first_open = &history.open_interactions[matching_indices[0]];
                    let opening_note = if first_open.start_seq != 0
                        && !history.entries.contains_key(&first_open.start_seq)
                    {
                        Some(EvidenceNote::recovered(
                            first_open.start_seq,
                            first_open.start_elapsed_ms,
                            EvidenceClass::Action,
                            Stage::Intent,
                        ))
                    } else {
                        None
                    };
                    let first_subject = first_open.subject;
                    let chain = history.correlate_written_evidence(
                        writes.into_iter(),
                        seq,
                        &event,
                        opening_note,
                    );
                    history.push_incident(
                        first_subject,
                        IncidentOutcome::AmbiguousServerOutcome,
                        IncidentConfidence::Ambiguous,
                        event.stage,
                        chain,
                    );
                }
                for idx in matching_indices.into_iter().rev() {
                    history.open_interactions.remove(idx);
                }
            }
        }

        let gaps = history.has_telemetry_gaps();
        let outcome = match event.stage {
            Stage::Cast {
                edge: CastEdge::LocalRefusal { .. },
                ..
            } => Some((
                IncidentOutcome::LocalRefusal,
                if gaps {
                    IncidentConfidence::Unverified
                } else {
                    IncidentConfidence::Exact
                },
                event.subject,
            )),
            Stage::Cast {
                edge: CastEdge::Failed { .. },
                ..
            }
            | Stage::Cast {
                edge: CastEdge::Interrupted { .. },
                ..
            } => {
                let (confidence, subject) =
                    cast_terminal.unwrap_or((IncidentConfidence::Unmatched, event.subject));
                // A negative match during a telemetry gap is unverified, not proof of no local cast.
                let confidence = if gaps && confidence == IncidentConfidence::Unmatched {
                    IncidentConfidence::Unverified
                } else {
                    confidence
                };
                Some((IncidentOutcome::ServerReportedFailure, confidence, subject))
            }
            Stage::Cast {
                edge: CastEdge::ScopeInvalidated,
                ..
            } => {
                let (_, subject) =
                    cast_terminal.unwrap_or((IncidentConfidence::Unverified, event.subject));
                Some((
                    IncidentOutcome::WriteUnconfirmed,
                    IncidentConfidence::Unverified,
                    subject,
                ))
            }
            Stage::GuardRefused | Stage::NoWorld | Stage::AlreadyPending
            | Stage::TrackingCapacity | Stage::ChannelDisconnected
            | Stage::Delivery(CommandDelivery::Rejected(_))
            | Stage::Outbound {
                state: OutboundState::Refused,
                ..
            } => Some((
                IncidentOutcome::LocalRefusal,
                IncidentConfidence::Exact,
                event.subject,
            )),
            Stage::Delivery(CommandDelivery::WriteFailed)
            | Stage::Outbound {
                state: OutboundState::WriteFailed,
                ..
            } => Some((
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
                if captured_opening_note.is_none() {
                    for (_, matched_subj) in &candidates {
                        if let Some(open) = history
                            .open_interactions
                            .iter()
                            .find(|open| open.subject.interaction == matched_subj.interaction)
                        {
                            if open.start_seq != 0
                                && !history.entries.contains_key(&open.start_seq)
                            {
                                captured_opening_note = Some(EvidenceNote::recovered(
                                    open.start_seq,
                                    open.start_elapsed_ms,
                                    EvidenceClass::Action,
                                    Stage::Intent,
                                ));
                                break;
                            }
                        }
                    }
                }
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
            | Stage::LootErrorObserved { .. }
            | Stage::VendorBuyFailedObserved { .. }
            | Stage::VendorSellFailedObserved { .. }
            | Stage::DomainEvent {
                event:
                    DomainEvent::LootError
                    | DomainEvent::VendorBuyFailed
                    | DomainEvent::VendorSellFailed
                    | DomainEvent::InventoryFailure,
            } => {
                let operation = match event.stage {
                    Stage::LootErrorObserved { .. }
                    | Stage::DomainEvent {
                        event: DomainEvent::LootError,
                    } => Some(Operation::Loot),
                    Stage::VendorBuyFailedObserved { .. }
                    | Stage::DomainEvent {
                        event: DomainEvent::VendorBuyFailed,
                    } => Some(Operation::Buy),
                    Stage::VendorSellFailedObserved { .. }
                    | Stage::DomainEvent {
                        event: DomainEvent::VendorSellFailed,
                    } => Some(Operation::Sell),
                    _ => None,
                };
                let candidates: Vec<Subject> = match operation {
                    Some(operation) => history
                        .entries
                        .iter()
                        .filter_map(|(_, candidate)| {
                            let written = matches!(
                                candidate.stage,
                                Stage::Delivery(CommandDelivery::Written)
                            );
                            let in_window = candidate.subject.scope.is_some()
                                && candidate.subject.scope == event.subject.scope
                                && candidate.subject.operation == operation
                                && candidate.elapsed_ms <= event.elapsed_ms
                                && event.elapsed_ms.saturating_sub(candidate.elapsed_ms)
                                    <= 30_000;
                            (written
                                && in_window
                                && loot_vendor_evidence_matches_action(
                                    &candidate.subject,
                                    &event,
                                ))
                            .then_some(candidate.subject)
                        })
                        .collect(),
                    None => history
                        .entries
                        .iter()
                        .filter_map(|(_, candidate)| {
                            let written = matches!(
                                candidate.stage,
                                Stage::Delivery(CommandDelivery::Written)
                            );
                            let in_window = candidate.subject.scope.is_some()
                                && candidate.subject.scope == event.subject.scope
                                && matches!(
                                    candidate.subject.operation,
                                    Operation::Loot
                                        | Operation::Equip
                                        | Operation::Buy
                                        | Operation::Sell
                                )
                                && candidate.elapsed_ms <= event.elapsed_ms
                                && event.elapsed_ms.saturating_sub(candidate.elapsed_ms)
                                    <= 30_000;
                            (written
                                && in_window
                                && loot_vendor_evidence_matches_action(
                                    &candidate.subject,
                                    &event,
                                ))
                            .then_some(candidate.subject)
                        })
                        .collect(),
                };
                if captured_opening_note.is_none() {
                    for subject in &candidates {
                        if let Some(open) = history
                            .open_interactions
                            .iter()
                            .find(|open| open.subject.interaction == subject.interaction)
                        {
                            if open.start_seq != 0
                                && !history.entries.contains_key(&open.start_seq)
                            {
                                captured_opening_note = Some(EvidenceNote::recovered(
                                    open.start_seq,
                                    open.start_elapsed_ms,
                                    EvidenceClass::Action,
                                    Stage::Intent,
                                ));
                                break;
                            }
                        }
                    }
                }
                for subject in &candidates {
                    history
                        .open_interactions
                        .retain(|open| open.subject.interaction != subject.interaction);
                }
                let (confidence, attributed_subject) = match candidates.as_slice() {
                    [subject] => {
                        let exact = candidate_subject_is_authoritative(event.stage);
                        (
                            if gaps {
                                IncidentConfidence::Unverified
                            } else if exact {
                                IncidentConfidence::Strong
                            } else {
                                IncidentConfidence::Heuristic
                            },
                            *subject,
                        )
                    }
                    [] => (
                        if gaps {
                            IncidentConfidence::Unverified
                        } else {
                            IncidentConfidence::Heuristic
                        },
                        event.subject,
                    ),
                    many => (IncidentConfidence::Ambiguous, many[0]),
                };
                Some((
                    IncidentOutcome::ServerReportedFailure,
                    confidence,
                    attributed_subject,
                ))
            }
            Stage::SocketReadFailed | Stage::SocketWriteFailed | Stage::AssetFailure { .. }
            | Stage::ReducerRejected { .. } | Stage::GameplayRejected { .. }
            | Stage::SessionEnded { kind: SessionEndKind::Lost } | Stage::ScriptError { .. }
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
                )),
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
                    | PatchEvent::RestoreUnavailable { .. }
                    | PatchEvent::RetentionFailed { .. },
            } => Some((
                IncidentOutcome::ObservedFailure,
                IncidentConfidence::Exact,
                event.subject,
            )),
            _ => None,
        };
        if let Some((outcome, confidence, attributed_subject)) = outcome {
            let note = EvidenceNote::from_event(seq, &event);
            let existing = history.incidents.iter_mut().rev().find(|incident| {
                same_script_row(incident.terminal_stage, event.stage)
            });
            let updated = if let Some(incident) = existing {
                incident.terminal_stage = event.stage;
                incident.telemetry_gap |= gaps;
                if incident.evidence.len() == MAX_INCIDENT_EVIDENCE {
                    incident.evidence.remove(0);
                    incident.evidence_truncated = true;
                }
                incident.evidence.push(seq);
                if incident.evidence_notes.len() == MAX_INCIDENT_EVIDENCE {
                    incident.evidence_notes.remove(0);
                    incident.evidence_notes_truncated = true;
                }
                incident.evidence_notes.push(note);
                true
            } else {
                false
            };
            if !updated {
                let opening_note = captured_opening_note.or_else(|| {
                    if attributed_subject.interaction == 0 {
                        return None;
                    }
                    let candidate_data = history
                        .cast_candidates
                        .iter()
                        .find(|candidate| {
                            candidate.subject.interaction == attributed_subject.interaction
                        })
                        .map(|candidate| {
                            (
                                candidate.started_seq,
                                candidate.started_elapsed_ms,
                                candidate.started_from_request,
                                candidate.started_from_admission,
                                candidate.spell_id,
                                candidate.wire_cast_count,
                            )
                        });
                    if let Some((
                        started_seq,
                        started_elapsed_ms,
                        started_from_request,
                        started_from_admission,
                        spell_id,
                        wire_cast_count,
                    )) = candidate_data
                    {
                        if started_seq != 0
                            && !history.entries.contains_key(&started_seq)
                        {
                            let stage = if started_from_request {
                                Stage::Cast {
                                    spell_id: Some(spell_id),
                                    attempt: None,
                                    wire_cast_count,
                                    edge: CastEdge::Requested,
                                }
                            } else if started_from_admission {
                                Stage::Cast {
                                    spell_id: Some(spell_id),
                                    attempt: None,
                                    wire_cast_count,
                                    edge: CastEdge::Admitted,
                                }
                            } else {
                                Stage::Delivery(CommandDelivery::Written)
                            };
                            return Some(EvidenceNote::recovered(
                                started_seq,
                                started_elapsed_ms,
                                EvidenceClass::Action,
                                stage,
                            ));
                        }
                    }
                    if let Some(open) = history
                        .open_interactions
                        .iter()
                        .find(|open| {
                            open.subject.interaction == attributed_subject.interaction
                        })
                    {
                        if open.start_seq != 0
                            && !history.entries.contains_key(&open.start_seq)
                        {
                            return Some(EvidenceNote::recovered(
                                open.start_seq,
                                open.start_elapsed_ms,
                                EvidenceClass::Action,
                                Stage::Intent,
                            ));
                        }
                    }
                    None
                });
                let chain = history.collect_interaction_evidence(
                    attributed_subject.interaction,
                    seq,
                    &event,
                    opening_note,
                );
                history.push_incident(
                    attributed_subject,
                    outcome,
                    confidence,
                    event.stage,
                    chain,
                );
            }
        }
        history.insert_retained(seq, event);
    }
    let now_ms = history.source.0.origin.elapsed().as_millis() as u64;
    history.expire_cast_windows(now_ms);
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
        source.observe(Some((1, 2)), None, Stage::WorldTransfer { map_id: None, from: None, to: None, retired_local: 0, retired_written: 0 });
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
        source.observe(Some((1, 1)), None, Stage::UiReload { old_vm_session: Some(10), old_reload_generation: Some(0), new_reload_generation: Some(1) });
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
            Stage::GameplayRejected {
                opcode: 0x0188,
                decoder: DecoderFamily::QuestDetails,
                reason: DecodeFailure::TrailingBytes,
                remaining_bytes: Some(3),
            },
        );
        drain_all(&mut history);

        assert!(history.incidents.iter().any(|inc| {
            inc.outcome == IncidentOutcome::WriteUnconfirmed
                && inc.confidence == IncidentConfidence::Unverified
        }));
        assert!(history.incidents.iter().any(|inc| {
            inc.outcome == IncidentOutcome::ObservedFailure
                && matches!(
                    inc.terminal_stage,
                    Stage::GameplayRejected {
                        opcode: 0x0188,
                        decoder: DecoderFamily::QuestDetails,
                        reason: DecodeFailure::TrailingBytes,
                        remaining_bytes: Some(3),
                    }
                )
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

