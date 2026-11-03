//! Host-owned interaction and client diagnostics, including worker-thread delivery results.
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use bevy::prelude::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;
#[cfg(feature = "dev")]
use std::sync::atomic::{AtomicBool, AtomicU8};

use crate::net::CommandDelivery;

#[derive(Clone, Copy, Debug, Serialize)]
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
    QuestSlotRemoved,
    JournalProjection,
    JournalRowInModel,
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
    AudioHealth { over_full_scale_samples: u64, nonfinite_samples: u64, voices_refused: u64 },
    MovementCorrectionApplied,
    MovementPressure { coalesced: u64, refused: u64 },
    SceneSummary { world_cameras: u32, animated_rigs: u32 },
    FrameWindow { frames: u32, mean_us: u64, max_us: u64 },
    AssetFailure { family: AssetFamily, cause: AssetFailure },
    AssetEventsLost { count: u64 },
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
    DomainEvent { event: DomainEvent },
    #[cfg(feature = "hotpatch")]
    Hotpatch { event: PatchEvent },
    Delivery(CommandDelivery),
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum AssetFamily {
    Model,
    World,
    Texture,
    Shader,
    Audio,
    Ui,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum AssetFailure {
    Reader,
    LoaderMissing,
    LoaderError,
    LoaderPanic,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum OutboundState {
    Queued,
    Refused,
    Written,
    WriteFailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum UiFrame {
    Questgiver,
    QuestLog,
    Merchant,
    Loot,
    SpellBook,
    Character,
    GameMenu,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum UiButton {
    Left,
    Right,
    Middle,
    Forward,
    Back,
}

#[derive(Clone, Copy, Debug, Serialize)]
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

#[derive(Clone, Copy, Debug, Serialize)]
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
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum PatchEvent {
    CandidateStaged,
    PublicationRejected,
    CandidateRejected,
    Activated { digest: Option<[u8; 20]> },
    FrameFailed,
    RetentionFailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum SessionPhase {
    CharacterSelection,
    EnteringWorld,
    InWorld,
    TransferPending,
    Loading,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum CastEdge {
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
    Observed,
}

#[derive(Debug)]
struct Ingress {
    events: Sender<Event>,
    dropped: AtomicU64,
    next_id: AtomicU64,
    reload_generation: AtomicU64,
    update_packets_seen: AtomicU64,
    update_packets_sampled_out: AtomicU64,
    movement_coalesced: AtomicU64,
    movement_refused: AtomicU64,
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

#[derive(Clone, Copy, Debug, Serialize)]
struct Subject {
    interaction: u64,
    scope: Option<(u64, u64)>,
    quest: Option<u32>,
    operation: Operation,
    reload_generation: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Interaction {
    source: Source,
    subject: Subject,
}

impl Source {
    pub(crate) fn note_movement_coalesced(&self) {
        self.0.movement_coalesced.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_movement_refused(&self) {
        self.0.movement_refused.fetch_add(1, Ordering::Relaxed);
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
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: 0,
                scope,
                quest,
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
        let trace = Interaction {
            source: self.clone(),
            subject: Subject {
                interaction: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                scope,
                quest,
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
    terminal_stage: Stage,
    evidence: Vec<u64>,
    evidence_truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
enum IncidentOutcome {
    LocalRefusal,
    WriteUnconfirmed,
    ServerReportedFailure,
    ObservedFailure,
    TelemetryGap,
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

#[derive(Resource)]
pub(crate) struct History {
    #[cfg(feature = "dev")]
    source: Source,
    incoming: Receiver<Event>,
    entries: VecDeque<(u64, Event)>,
    incidents: VecDeque<Incident>,
    incidents_evicted: u64,
    received: u64,
    evicted: u64,
    source_gaps: u64,
    #[cfg(feature = "hotpatch")]
    last_observed_patch_digest: Option<[u8; 20]>,
}

#[cfg(feature = "dev")]
impl History {
    /// A time-window association is a lead, not proof that a press caused a script error.
    pub(crate) fn ui_error_correlations(&self) -> Vec<String> {
        let gaps = self.evicted > 0 || self.source_gaps > 0
            || self.source.0.dropped.load(Ordering::Relaxed) > 0;
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

    pub(crate) fn recent_incidents(&self) -> impl Iterator<Item = String> + '_ {
        self.incidents.iter().rev().take(8).map(|incident| {
            let missing = incident.evidence.iter().any(|seq| {
                !self.entries.iter().any(|(retained, _)| retained == seq)
            });
            format!(
                "Action {} {:?}, quest {:?}: {:?} {:?} (evidence {:?}{}{})",
                incident.subject.interaction,
                incident.subject.operation,
                incident.subject.quest,
                incident.outcome,
                incident.terminal_stage,
                incident.evidence,
                if incident.evidence_truncated { ", truncated" } else { "" },
                if missing { ", some events evicted" } else { "" }
            )
        })
    }

    pub(crate) fn action_summaries(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut output = Vec::new();
        let gaps = self.evicted > 0 || self.source_gaps > 0
            || self.source.0.dropped.load(Ordering::Relaxed) > 0;
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
            let server_evidence = if event.subject.operation == Operation::Accept
                && matches!(event.stage, Stage::Delivery(CommandDelivery::Written))
            {
                self.entries.iter().find_map(|(_, later)| {
                    let same_subject = event.subject.quest.is_some()
                        && later.subject.quest == event.subject.quest
                        && later.subject.scope == event.subject.scope
                        && later.elapsed_ms >= event.elapsed_ms
                        && later.elapsed_ms.saturating_sub(event.elapsed_ms) <= 30_000;
                    if !same_subject
                        || !matches!(
                            later.stage,
                            Stage::ServerQuestFailed { .. } | Stage::QuestSlotAdded
                        )
                    {
                        return None;
                    }
                    let matches = self.entries.iter().filter(|(_, candidate)| {
                        candidate.subject.operation == Operation::Accept
                            && candidate.subject.scope == later.subject.scope
                            && candidate.subject.quest == later.subject.quest
                            && matches!(candidate.stage, Stage::Delivery(CommandDelivery::Written))
                            && candidate.elapsed_ms <= later.elapsed_ms
                            && later.elapsed_ms.saturating_sub(candidate.elapsed_ms) <= 30_000
                    }).count();
                    let confidence = if matches > 1 { "ambiguous" } else { "possible match" };
                    match later.stage {
                        Stage::ServerQuestFailed { reason } => Some(format!(
                            "; server reported quest failure {reason} ({confidence})"
                        )),
                        Stage::QuestSlotAdded => Some(format!("; quest slot added ({confidence})")),
                        _ => None,
                    }
                }).unwrap_or_default()
            } else {
                String::new()
            };
            output.push(format!(
                "Action {id} {:?}, quest {:?}: {status}{reason}{server_evidence}{suffix}",
                event.subject.operation, event.subject.quest
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

    pub(crate) fn source_gaps(&self) -> u64 {
        self.source_gaps
    }

    pub(crate) fn sampled_updates(&self) -> u64 {
        self.source.0.update_packets_sampled_out.load(Ordering::Relaxed)
    }

    pub(crate) fn recent(&self) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().rev().take(16).map(|(seq, e)| {
            format!("#{seq} +{}ms · action {} {:?} · quest {:?} · {:?} · scope {:?}",
                e.elapsed_ms, e.subject.interaction, e.subject.operation, e.subject.quest,
                e.stage, e.subject.scope)
        })
    }

    /// Summarize received quest-slot evidence without assigning it to a specific request
    /// when more than one earlier write could explain it.
    pub(crate) fn slot_correlations(&self) -> Vec<String> {
        let incomplete = self.evicted > 0 || self.source_gaps > 0
            || self.source.0.dropped.load(Ordering::Relaxed) > 0;
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
            let projected = self.entries.iter().any(|(_, later)| {
                matches!(later.stage, Stage::JournalProjection)
                    && later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.elapsed_ms >= event.elapsed_ms
            });
            let projection = if projected { "later journal projection observed" } else { "journal projection unverified" };
            let row = self.entries.iter().any(|(_, later)| {
                matches!(later.stage, Stage::JournalRowInModel)
                    && later.subject.scope == Some(scope)
                    && later.subject.quest == Some(quest)
                    && later.elapsed_ms >= event.elapsed_ms
            });
            let row_status = if row { "; row exists in journal model" } else { "; row in journal model unverified" };
            let result = match candidates.as_slice() {
                [id] => format!("Quest {quest}: slot observed after action {id} (possible match; {projection})"),
                [] => format!("Quest {quest}: slot observed without a retained write in the preceding 30 seconds; {projection}"),
                _ => format!("Quest {quest}: slot observed, {} matching writes (ambiguous); {projection}", candidates.len()),
            };
            Some(if incomplete {
                format!("{result}{row_status}; trace has gaps")
            } else {
                format!("{result}{row_status}")
            })
        }).take(8).collect()
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
        let (received, evicted, dropped) = self.counters();
        let ingress_pending = self.incoming.len();
        let source_gaps = self.source_gaps;
        #[cfg(feature = "hotpatch")]
        let last_observed_patch_digest = self.last_observed_patch_digest;
        #[cfg(not(feature = "hotpatch"))]
        let last_observed_patch_digest: Option<[u8; 20]> = None;
        let sampled_updates = self.sampled_updates();
        let first_sequence = self.entries.front().map(|(seq, _)| *seq);
        let run_id = self.source.0.run_id.clone();
        let asset_profile = self.source.0.asset_profile.clone();
        let action_summaries = self.action_summaries();
        let slot_correlations = self.slot_correlations();
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
                        "run_id": run_id,
                        "protocol_mode": "wrath_12340",
                        "coverage": {
                            "instrumented_paths": [
                                "quest_interactions", "ui_frame_state", "ui_pointer_categories",
                                "lua_diagnostic_counts", "session_lifecycle", "allowlisted_packets",
                                "casts", "inventory_failures", "loot_and_vendor_events",
                                "audio_mixer_state", "movement_queue_and_corrections",
                                "scene_camera_and_rig_counts", "frame_intervals",
                                "asset_load_failure_categories"
                            ],
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
                        "evicted": evicted,
                        "dropped": dropped,
                        "upstream_gap_events": source_gaps,
                        "last_observed_patch_sha1_bytes": last_observed_patch_digest,
                        "object_update_packets_sampled_out": sampled_updates,
                        "incidents": incidents,
                        "incidents_evicted": incidents_evicted,
                        "ingress_pending_at_snapshot": ingress_pending,
                        "action_summaries": action_summaries,
                        "slot_correlations": slot_correlations,
                        "ui_error_correlations": ui_correlations,
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

pub(crate) fn install(app: &mut App) {
    let (events, incoming) = bounded(256);
    let source = Source(Arc::new(Ingress {
        events,
        dropped: AtomicU64::new(0),
        next_id: AtomicU64::new(1),
        reload_generation: AtomicU64::new(0),
        update_packets_seen: AtomicU64::new(0),
        update_packets_sampled_out: AtomicU64::new(0),
        movement_coalesced: AtomicU64::new(0),
        movement_refused: AtomicU64::new(0),
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
    app.insert_resource(source.clone())
        .init_resource::<ScriptCursor>()
        .insert_resource(History {
            #[cfg(feature = "dev")]
            source,
            incoming,
            entries: VecDeque::with_capacity(1024),
            incidents: VecDeque::with_capacity(64),
            incidents_evicted: 0,
            received: 0,
            evicted: 0,
            source_gaps: 0,
            #[cfg(feature = "hotpatch")]
            last_observed_patch_digest: None,
        })
        .add_systems(Last, (poll_script_diagnostics, poll_replica, poll_audio, poll_movement, poll_scene, poll_frame_time, poll_asset_failures, drain).chain());
}

fn poll_asset_failures(
    source: Res<Source>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
    events: Option<Res<Messages<bevy::asset::UntypedAssetLoadFailedEvent>>>,
    mut cursor: Local<bevy::ecs::message::MessageCursor<bevy::asset::UntypedAssetLoadFailedEvent>>,
) {
    let Some(events) = events else { return };
    let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
    let missed = cursor.missed_messages(&events);
    if missed > 0 {
        source.observe(scope, None, Stage::AssetEventsLost { count: missed as u64 });
    }
    for failure in cursor.read(&events) {
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
    time: Res<Time>,
    mut last: Local<Option<(u64, u64)>>,
    mut next_poll: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_poll {
        return;
    }
    *next_poll = now + 1.0;
    let current = (
        source.0.movement_coalesced.load(Ordering::Relaxed),
        source.0.movement_refused.load(Ordering::Relaxed),
    );
    if *last != Some(current) {
        *last = Some(current);
        source.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
            Stage::MovementPressure { coalesced: current.0, refused: current.1 });
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
        *last = Some(current);
        let scope = view.as_ref().map(|v| (v.session_epoch, v.world_generation));
        source.observe(scope, None, Stage::AudioMixer {
            present: current.0,
            voices: current.1,
            denied: current.2,
            stolen: current.3,
            copies_dropped: current.4,
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

fn drain(mut history: ResMut<History>) {
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
        if let Stage::Hotpatch { event: PatchEvent::Activated { digest } } = event.stage {
            history.last_observed_patch_digest = digest;
        }
        let outcome = match event.stage {
            Stage::GuardRefused | Stage::NoWorld | Stage::AlreadyPending
            | Stage::TrackingCapacity | Stage::ChannelDisconnected
            | Stage::Delivery(CommandDelivery::Rejected(_)) => Some(IncidentOutcome::LocalRefusal),
            Stage::Delivery(CommandDelivery::WriteFailed) => Some(IncidentOutcome::WriteUnconfirmed),
            Stage::ServerQuestInvalid { .. } | Stage::ServerQuestLogFull
            | Stage::ServerQuestFailed { .. } | Stage::InventoryFailure { .. }
            | Stage::DomainEvent { event: DomainEvent::LootError | DomainEvent::VendorBuyFailed
                | DomainEvent::VendorSellFailed | DomainEvent::InventoryFailure } =>
                Some(IncidentOutcome::ServerReportedFailure),
            Stage::SocketReadFailed | Stage::SocketWriteFailed | Stage::AssetFailure { .. }
            | Stage::ReducerRejected { .. } | Stage::GameplayRejected { .. }
            | Stage::SessionEnded { failed: true } | Stage::ScriptError { .. }
            | Stage::ScriptLoadError { .. } =>
                Some(IncidentOutcome::ObservedFailure),
            Stage::AudioHealth { nonfinite_samples, voices_refused, .. }
                if nonfinite_samples > 0 || voices_refused > 0 =>
                Some(IncidentOutcome::ObservedFailure),
            Stage::NativeEventsLost { .. } | Stage::ScriptRowsLost { .. }
            | Stage::ScriptRowsEvicted { .. } | Stage::AssetEventsLost { .. } =>
                Some(IncidentOutcome::TelemetryGap),
            #[cfg(feature = "hotpatch")]
            Stage::Hotpatch { event: PatchEvent::PublicationRejected | PatchEvent::CandidateRejected
                | PatchEvent::FrameFailed | PatchEvent::RetentionFailed } =>
                Some(IncidentOutcome::ObservedFailure),
            _ => None,
        };
        if let Some(outcome) = outcome {
            let existing = history.incidents.iter_mut().rev().find(|incident| {
                same_script_row(incident.terminal_stage, event.stage)
            });
            let updated = if let Some(incident) = existing {
                incident.terminal_stage = event.stage;
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
                let mut evidence: Vec<u64> = if event.subject.interaction == 0 {
                    Vec::new()
                } else {
                    history.entries.iter().rev()
                    .filter(|(_, prior)| prior.subject.interaction == event.subject.interaction)
                    .take(8)
                    .map(|(prior_seq, _)| *prior_seq)
                    .collect()
                };
                let evidence_truncated = evidence.len() == 8;
                evidence.truncate(7);
                evidence.reverse();
                evidence.push(seq);
                if history.incidents.len() == 64 {
                    history.incidents.pop_front();
                    history.incidents_evicted += 1;
                }
                history.incidents.push_back(Incident {
                    version: 1,
                    subject: event.subject,
                    outcome,
                    terminal_stage: event.stage,
                    evidence,
                    evidence_truncated,
                });
            }
        }
        if history.entries.len() == 1024 {
            history.entries.pop_front();
            history.evicted += 1;
        }
        history.entries.push_back((seq, event));
    }
}
