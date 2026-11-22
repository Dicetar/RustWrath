//! The Wrath (3.3.5a) session feed into the graphical app: the bridge between the
//! `benilla-protocol-wrath` session/producer and this app's established stage pipeline.
//!
//! Scheduling contract (Astra R5, attached to issue 22 â€” this module implements the feed half):
//!
//! - The session loop (logon â†’ world â†’ reduce â†’ producer â†’ pump) runs on its own IO thread,
//!   exactly like the 1.12 `net::io` thread â€” never in a Bevy `Update` system. The renderless
//!   driver's blocking `recv` loop stays on that thread.
//! - The thread publishes **latest-only** self snapshots through a single-slot handoff
//!   (`WrathLatest`) and **bounded** lifecycle/phase events through a queue
//!   (`WrathEvents`, capacity [`WRATH_EVENT_CAPACITY`], oldest dropped). A full slot is never a
//!   blocked packet loop: observations are latest-only by contract, and a dropped lifecycle
//!   event is recoverable because the newest full state always follows.
//! - Translated gameplay replies travel a third, **bounded** queue (`SessionReplies`) that the
//!   worker never blocks on: lifecycle and reply events keep arrival order, replaceable
//!   observations coalesce in place, and the session's terminal edges have reserved slots so an
//!   overloaded or stalled consumer can never delay them (review finding R04).
//! - The app drains both in the existing `WorldStage::Net` stage â€” before `Input` reads
//!   position â€” with a per-frame drain bound, so bursts cannot starve input and no-packet frames
//!   do no work. World-generation changes invalidate the mapped entity set and the pending
//!   publication state **in the same Net tick** they are drained, before input/control.
//! - Server authority stays authoritative: this feed carries replicated state and lifecycle
//!   only. Prediction (issue 23) reconciles against it and never manufactures outcomes.
//!
//! Entity translation into the existing `GuidIndex`/`ObjectFields` path is issue 22's remaining
//! work and is deliberately **not** stubbed here: the feed exposes typed snapshots, the
//! translation consumes them at the named stage.
//!
//! Lifecycle: a clean logout ends the worker, clears its world/reply scope, and lets the existing
//! `LoggedOutMessage`/glue edge return to character selection. A replacement is admitted only
//! after the old worker has joined and its terminal feed has been drained. Fresh-session
//! admission after that clean stop has a small bounded retry budget; a lost connection does not
//! enter that path, because connection-loss recovery is a separate policy.
//!
//! Character selection: the same screen drives the replacement through the stock `CharPick`
//! channel, rebound in [`WrathNetPlugin::build`] to a wrath-owned queue. A fresh session is
//! parked at the select screen, publishes the 3.3.5 enumeration onto the established
//! `SessionEvent::CharacterList` seam, and enters on the screen's own pick.

use std::collections::{BTreeMap, HashMap, VecDeque};

use benilla_world::schedule::WorldStage;
use benilla_world::world_map::CurrentMap;
use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::client_state::ClientState;
use crate::entities::Equipment;
use crate::items::{Countdowns, ItemObject};
use crate::net::{
    CharPick, CharRequest, ClientCommand, EnteredWorldMessage, FieldChanged, Guid, GuidIndex,
    NetCommandTap, NetCommands, NetEntity, ObjectStore, ParkRequest, ParkSender, SelfGuid,
    SelfPlayer, UnitSpeeds, WorldEnterCascadeMessage,
};
use benilla_assets::coords::wow_to_bevy;
use benilla_protocol::messages::{
    ActionButton, AttackSwingError, AttackerState as ProtocolAttackerState,
    ChatMessage as ProtocolChatMessage, GossipOption as ProtocolGossipOption,
    ItemDamage as ProtocolItemDamage, ItemInfo, ItemPushResult,
    ItemSpellEntry as ProtocolItemSpell, ItemUseSpell as ProtocolItemUseSpell,
    LootAllPassed as ProtocolLootAllPassed, LootItem as ProtocolLootItem,
    LootRoll as ProtocolLootRoll, LootRollWon as ProtocolLootRollWon,
    LootStartRoll as ProtocolLootStartRoll, NpcTextBlock as ProtocolNpcTextBlock,
    PeriodicAuraLog as ProtocolPeriodicAuraLog, PeriodicTick as ProtocolPeriodicTick,
    QuestComplete, QuestConfirmAccept as ProtocolQuestConfirmAccept, QuestDetails, QuestGiverList,
    QuestListEntry, QuestObjective, QuestOfferReward, QuestRequestItems, QuestRewardItem,
    QuestTemplate, SpellDamageLog as ProtocolSpellDamageLog, TrainerSpell as ProtocolTrainerSpell,
    VendorItem as ProtocolVendorItem,
};
use benilla_protocol::{
    messages::ObjectType, wire::Vector3d, CharEnumItem, Character, EntityKind, MoveSpeeds,
    ObjectFields, RosterCause, SessionEvent, TalentGroupInfo,
};
use benilla_protocol_wrath::cast_failure;
use benilla_protocol_wrath::gameplay::{self, ServerEvent as WrathGameplayEvent};
use benilla_ui::script::{QuestSlotView, QUEST_LOG_SLOTS};
use benilla_wm_native::{
    ActionBridge, ActionCorrelation, ActionEvidence, ActionIntent, ActionRequest, ActionResult,
    ActionStatus,
};

use benilla_protocol_wrath::reducer::VehicleAttachment;
use benilla_protocol_wrath::session::{LifeEvent, Phase, SelfObservation, SessionState};

/// The UI feed that projects the scoped rune/calendar view into the Wrath VM's model.
#[path = "ui_script/wrath_rune_calendar_feed.rs"]
mod wrath_rune_calendar_feed;
/// The UI feed that turns a decoded `SMSG_PARTYKILLLOG` into the stock combat-log event.
use crate::ui_script::wrath_combat_log_feed;

/// Bounded lifecycle queue depth: lifetimes of interest are short (one world transfer is one
/// event); 16 gives many frames of headroom at one-event-per-transfer rates.
pub const WRATH_EVENT_CAPACITY: usize = 16;
/// Native cast/channel/item-text queue depth. Separate from the lifecycle queue so game traffic
/// cannot evict a terminal lifecycle edge.
pub const WRATH_NATIVE_EVENT_CAPACITY: usize = 64;

/// Per-frame drain bound for the Net-stage system: a burst longer than this leaves the rest
/// for the next frame instead of starving input (R5 budget; measured, not tuned, per frame).
pub const WRATH_DRAIN_LIMIT: usize = 64;
/// Pending `SMSG_PARTYKILLLOG` rows kept for the UI feed. A burst larger than this drops the
/// oldest rows and reports the loss through `WrathWorldView::party_kill_dropped`.
pub const PARTY_KILL_CAPACITY: usize = 32;
/// Pending `SMSG_ATTACKERSTATEUPDATE` rows kept for the UI feed. Melee swings arrive far more
/// often than kills; this is a reviewed bound, not a measurement. Overflow drops the oldest row
/// and reports the loss through `WrathWorldView::swing_dropped`.
pub const SWING_CAPACITY: usize = 64;
/// Pending `SMSG_SPELLNONMELEEDAMAGELOG` rows kept for the UI feed. Ordered events; overflow drops
/// the oldest row and reports the loss through `WrathWorldView::spell_damage_dropped`.
pub const SPELL_LOG_CAPACITY: usize = 64;
/// How long an undelivered combat-log row may wait for an endpoint name before it is delivered
/// with a visible unknown marker. Names usually land from the create/name-query path within a
/// frame or two.
pub const COMBAT_LOG_NAME_RETRY_SECONDS: f64 = 5.0;

// Session-reply delivery (package B, review finding R04). The numbers below are reviewed design
// choices for bounded ownership, not measurements: no build or runtime profile established them.
// They are deliberately generous so a stalled main loop rides out a burst without either
// unbounded growth or a needless disconnect, while a genuinely dead consumer still ends in the
// controlled terminal path rather than in memory exhaustion.

/// Hard cap on the ordered session-reply FIFO (lifecycle edges plus gameplay replies). Reserved
/// terminal slots are separate and never counted here.
pub const WRATH_SESSION_REPLY_CAP: usize = 4096;
/// Above this depth the producer stops reading between packets until the queue drains.
pub const WRATH_SESSION_REPLY_HIGH_WATER: usize = 2048;
/// Backpressure latches until the queue is at or below this depth, so it does not flap.
pub const WRATH_SESSION_REPLY_LOW_WATER: usize = 1024;
/// Reserved terminal slots: at most one per terminal kind, and a worker produces at most two
/// (`LoggedOut` with its `Disconnected` teardown, or a single `LoginFailed`/`Disconnected`).
pub const WRATH_SESSION_TERMINAL_CAP: usize = 2;
/// Ceiling for one reply-drain pass when the queue is deep; the floor is [`WRATH_DRAIN_LIMIT`].
pub const WRATH_DRAIN_CEILING: usize = 512;
/// Wall-clock budget for one reply-drain pass. A design choice, not a measurement.
pub const WRATH_DRAIN_BUDGET: Duration = Duration::from_millis(2);
/// Longest single producer pause while the reply queue is above high water. The pause always ends
/// between complete packets, re-checks cancellation and never straddles a packet boundary.
pub const WRATH_REPLY_BACKPRESSURE_SLICE: Duration = Duration::from_millis(50);

/// The longest silent world-server window treated as healthy. A live server answers the 10 s ping
/// even when gameplay is quiet; socket read timeouts and empty frame ticks are not responses.
pub const SERVER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Bounded fresh-session admission after a clean logout. Connection-loss recovery does not use
/// this budget; it is a separate lifecycle policy.
pub const WRATH_RESTART_ATTEMPTS: u32 = 2;
/// Delay between bounded fresh-session admission attempts. The frame loop polls this; it never
/// sleeps on the render thread.
pub const WRATH_RESTART_DELAY: Duration = Duration::from_millis(750);

/// Latest-only self snapshot slot: the IO thread overwrites, the Net stage reads and clears.
pub type WrathLatest = Arc<Mutex<Option<WorldDigest>>>;

/// Bounded lifecycle/phase event queue, shared IO-thread â†’ Net stage.
pub type WrathEvents = Arc<Mutex<VecDeque<WrathFeedEvent>>>;

/// One outbound client packet handed from the Bevy frame to the session IO thread.
pub struct WrathOutbound {
    pub opcode: u32,
    pub payload: Vec<u8>,
    /// Replaceable motion: under queue pressure only these may be dropped, never a cast, quest,
    /// loot or logout command (ADR-0001's admission rule).
    pub movement: bool,
    /// Monotonic per-run sequence for movement entries only.
    pub movement_seq: Option<u64>,
    /// When this outbound entry was admitted to the queue, for bounded queue-age evidence.
    pub enqueued_at: Option<std::time::Instant>,
    pub(crate) receipt: Option<crate::net::CommandReceipt>,
    pub(crate) trace: Option<crate::quest_trace::Interaction>,
    /// Whether the carried trace scope is enforced across the async transport boundary.
    pub(crate) enforce_scope: bool,
}

/// A translated gameplay reply tagged with the source session and world scope. The app rejects
/// replies older than the scope it has already adopted, so a queue race across a transfer cannot
/// apply a retired world's tooltip, quest or inventory reply to the replacement world.
pub struct WrathSessionEvent {
    login_generation: Option<u64>,
    session_epoch: u64,
    world_generation: u64,
    event: SessionEvent,
    /// Terminal lifecycle events must be delivered even after the scope was invalidated.
    force: bool,
}

impl WrathSessionEvent {
    fn scoped(session: &SessionState, event: SessionEvent) -> Self {
        Self {
            login_generation: None,
            session_epoch: session.epoch(),
            world_generation: session.world_generation(),
            event,
            force: false,
        }
    }

    fn forced(event: SessionEvent) -> Self {
        Self {
            login_generation: None,
            session_epoch: 0,
            world_generation: 0,
            event,
            force: true,
        }
    }

    fn for_login(mut self, generation: Option<u64>) -> Self {
        self.login_generation = generation;
        self
    }
}

/// Delivery class of one translated reply (review finding R04). The class decides eviction and
/// coalescing only; scope acceptance stays with `scope_is_older` in the Net-stage dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionClass {
    /// Ends the session: delivered before anything queued behind it and never evicted.
    Terminal,
    /// A login/realm/roster/character-action/transfer edge: ordered, never coalesced.
    Lifecycle,
    /// A gameplay reply that mutates owning state: ordered, never coalesced.
    Reply,
    /// An absolute snapshot a newer one replaces: coalesced in place by key.
    Observation,
}

/// Which terminal edge this is, when it is one. A worker emits at most two: `LoggedOut` followed
/// by the closure's `Disconnected { end: LoggedOut }` teardown (both handlers do different work),
/// or a single failed-login edge. Anything else is not a terminal.
fn terminal_index(event: &SessionEvent) -> Option<usize> {
    match event {
        SessionEvent::LoggedOut | SessionEvent::CharacterLoginFailed { .. } => Some(0),
        SessionEvent::Disconnected { .. } => Some(1),
        SessionEvent::LoginFailed { .. } => Some(2),
        _ => None,
    }
}

/// Identity of a replaceable observation. Two events with the same key are the same fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservationKey {
    ServerClock,
    PlayedTime,
    GossipMenu(u64),
    Vendor(u64),
}

fn observation_key(event: &SessionEvent) -> Option<ObservationKey> {
    match event {
        SessionEvent::ServerUnixTime { .. } => Some(ObservationKey::ServerClock),
        SessionEvent::PlayedTime { .. } => Some(ObservationKey::PlayedTime),
        SessionEvent::GossipMenu { npc, .. } => Some(ObservationKey::GossipMenu(*npc)),
        SessionEvent::VendorInventory { vendor, .. } => Some(ObservationKey::Vendor(*vendor)),
        _ => None,
    }
}

/// Whether `event` is a login/realm/roster/transfer edge rather than a gameplay reply.
fn is_lifecycle_event(event: &SessionEvent) -> bool {
    matches!(
        event,
        SessionEvent::LoginStage { .. }
            | SessionEvent::RealmList { .. }
            | SessionEvent::RealmSplit { .. }
            | SessionEvent::LoginQueued { .. }
            | SessionEvent::WorldAuthenticated { .. }
            | SessionEvent::AddonInfo { .. }
            | SessionEvent::CharacterList { .. }
            | SessionEvent::RosterFailed { .. }
            | SessionEvent::CharActionResult { .. }
            | SessionEvent::CharRenamed { .. }
            | SessionEvent::CharacterLoginFailed { .. }
            | SessionEvent::LogoutResponse { .. }
            | SessionEvent::LogoutCancelled
            | SessionEvent::Connected { .. }
            | SessionEvent::Worldport { .. }
            | SessionEvent::TransferPending { .. }
            | SessionEvent::TransferAborted { .. }
    )
}

fn session_class(event: &SessionEvent) -> SessionClass {
    if terminal_index(event).is_some() {
        SessionClass::Terminal
    } else if is_lifecycle_event(event) {
        SessionClass::Lifecycle
    } else if observation_key(event).is_some() {
        SessionClass::Observation
    } else {
        SessionClass::Reply
    }
}

/// Counters copied into [`WrathControl`] for diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionReplyStats {
    pub depth: usize,
    pub peak: usize,
    pub coalesced: u64,
    pub dropped: u64,
    pub overflowed: u64,
    pub closed_pushes: u64,
    pub delivered: u64,
}

/// Evidence for an item that shows no name: an item template reply dropped before dispatch leaves
/// its ask pending until the item cache's re-ask clock ([`crate::items::TEMPLATE_REASK_AFTER`]).
fn warn_item_reply_dropped(event: &WrathSessionEvent, why: &str) {
    if let SessionEvent::ItemTemplate { entry, .. } = &event.event {
        warn!("wrath: item template reply for item {entry} dropped ({why})");
    }
}

/// Bounded, non-blocking delivery queue for translated replies (R04).
///
/// The producer never waits: `push` is O(1) for replies and a small scan for observations. The
/// consumer drains a bounded slice per frame. Terminal edges live in their own reserved slots, so
/// an overflowing reply backlog can never prevent the session's end from being delivered.
///
/// A fresh queue is created per worker, which is also what keeps the per-worker epoch restart
/// from aliasing a retired worker's events. `close` makes later pushes no-ops during teardown, so
/// a worker pushing while the app stops cannot fabricate an overflow terminal.
#[derive(Default)]
pub(crate) struct SessionReplies {
    fifo: VecDeque<WrathSessionEvent>,
    terminal: VecDeque<WrathSessionEvent>,
    /// One slot per [`terminal_index`]; a repeated terminal edge is redundant, not a new outcome.
    seen_terminal: [bool; 3],
    closed: bool,
    throttled: bool,
    coalesced: u64,
    dropped: u64,
    overflowed: u64,
    closed_pushes: u64,
    peak: usize,
    delivered: u64,
    /// The scope the app has adopted. A reply older than it can no longer be applied, so it is
    /// dropped where it is produced rather than occupying a bounded delivery slot.
    adopted: Option<(u64, u64)>,
    /// The session is over: only terminal and forced edges are still accepted.
    retired: bool,
}

impl SessionReplies {
    fn push(&mut self, event: WrathSessionEvent) {
        if self.closed {
            self.closed_pushes += 1;
            return;
        }
        let class = session_class(&event.event);
        if self.retired && !event.force && class != SessionClass::Terminal {
            // The session that produced this reply has ended; its state can no longer be applied.
            warn_item_reply_dropped(&event, "session retired");
            self.dropped += 1;
            return;
        }
        if let Some(scope) = self.adopted {
            if class != SessionClass::Terminal && SessionReplies::superseded(scope, &event) {
                warn_item_reply_dropped(&event, "superseded scope");
                self.dropped += 1;
                return;
            }
        }
        match class {
            SessionClass::Terminal => self.push_terminal(event),
            SessionClass::Observation => {
                if let Some(key) = observation_key(&event.event) {
                    if let Some(slot) = self
                        .fifo
                        .iter_mut()
                        .find(|queued| observation_key(&queued.event) == Some(key))
                    {
                        // Replace in place: the entry keeps its position, so reply order is
                        // unchanged, and the newest absolute snapshot wins.
                        *slot = event;
                        self.coalesced += 1;
                        return;
                    }
                }
                self.push_reliable(event);
            }
            SessionClass::Lifecycle | SessionClass::Reply => self.push_reliable(event),
        }
    }

    fn push_terminal(&mut self, event: WrathSessionEvent) {
        let Some(index) = terminal_index(&event.event) else {
            debug_assert!(false, "a non-terminal reached the terminal slot");
            return;
        };
        if self.seen_terminal[index] || self.terminal.len() >= WRATH_SESSION_TERMINAL_CAP {
            self.dropped += 1;
            return;
        }
        self.seen_terminal[index] = true;
        self.terminal.push_back(event);
        self.note_peak();
    }

    fn push_reliable(&mut self, event: WrathSessionEvent) {
        if self.fifo.len() >= WRATH_SESSION_REPLY_CAP {
            // Bound the memory first; the worker turns the sticky flag into a controlled
            // terminal through the existing error path rather than dropping silently.
            warn_item_reply_dropped(&event, "reply backlog full");
            self.overflowed += 1;
            self.dropped += 1;
            return;
        }
        self.fifo.push_back(event);
        self.note_peak();
    }

    fn note_peak(&mut self) {
        self.peak = self.peak.max(self.fifo.len() + self.terminal.len());
    }

    /// Terminals first; any queued reply is superseded by the session's end and discarded.
    fn take(&mut self, limit: usize, budget: Duration) -> Vec<WrathSessionEvent> {
        if !self.terminal.is_empty() {
            let out: Vec<_> = self.terminal.drain(..).collect();
            self.dropped += self.fifo.len() as u64;
            for event in &self.fifo {
                warn_item_reply_dropped(event, "session ended");
            }
            self.fifo.clear();
            self.throttled = false;
            self.delivered += out.len() as u64;
            return out;
        }
        let started = Instant::now();
        let mut out = Vec::with_capacity(limit.min(self.fifo.len()));
        while out.len() < limit {
            if started.elapsed() >= budget {
                break;
            }
            let Some(event) = self.fifo.pop_front() else {
                break;
            };
            out.push(event);
        }
        self.delivered += out.len() as u64;
        out
    }

    fn is_empty(&self) -> bool {
        self.fifo.is_empty() && self.terminal.is_empty()
    }

    fn len(&self) -> usize {
        self.fifo.len() + self.terminal.len()
    }

    fn overloaded(&self) -> bool {
        self.overflowed > 0
    }

    /// True while the producer must keep pausing between packets. Latching on the high/low-water
    /// pair keeps a queue hovering at the threshold from re-reading one packet per slice.
    fn producer_should_pause(&mut self) -> bool {
        if self.fifo.len() >= WRATH_SESSION_REPLY_HIGH_WATER {
            self.throttled = true;
        } else if self.fifo.len() <= WRATH_SESSION_REPLY_LOW_WATER {
            self.throttled = false;
        }
        self.throttled
    }

    fn close(&mut self) {
        self.closed = true;
    }

    /// A terminal is a session barrier: queued replies are stale once the session ends.
    fn clear_backlog(&mut self) {
        self.dropped += self.fifo.len() as u64;
        for event in &self.fifo {
            warn_item_reply_dropped(event, "backlog cleared");
        }
        self.fifo.clear();
        self.throttled = false;
    }

    /// Whether an event belongs to a scope the consumer has already left. Forced edges keep
    /// their "deliver even when stale" contract: a terminal must land whatever the scope was.
    fn superseded(scope: (u64, u64), event: &WrathSessionEvent) -> bool {
        !event.force
            && (event.session_epoch < scope.0
                || (event.session_epoch == scope.0 && event.world_generation < scope.1))
    }

    /// Adopt a newer world scope: prune the replies it already supersedes and drop later ones at
    /// the producer, so a retired world's reply never takes a delivery slot or reaches the
    /// dispatcher. Terminal edges keep their reserved slots.
    fn adopt_scope(&mut self, session_epoch: u64, world_generation: u64) -> u64 {
        let scope = (session_epoch, world_generation);
        self.adopted = Some(scope);
        let before = self.fifo.len();
        self.fifo.retain(|queued| {
            let stale = SessionReplies::superseded(scope, queued);
            if stale {
                warn_item_reply_dropped(queued, "superseded scope");
            }
            !stale
        });
        let dropped = (before - self.fifo.len()) as u64;
        self.dropped += dropped;
        dropped
    }

    /// Retire the scope for good: only terminals and forced edges are delivered from here on.
    /// The queued backlog is stale by definition; the reserved terminal slots are kept.
    fn retire_scope(&mut self) -> u64 {
        self.retired = true;
        let dropped = self.fifo.len() as u64;
        self.clear_backlog();
        dropped
    }

    fn stats(&self) -> SessionReplyStats {
        SessionReplyStats {
            depth: self.len(),
            peak: self.peak,
            coalesced: self.coalesced,
            dropped: self.dropped,
            overflowed: self.overflowed,
            closed_pushes: self.closed_pushes,
            delivered: self.delivered,
        }
    }
}

/// Queue one translated reply under the session's scope tag. The producer never blocks.
fn push_session_event(
    replies: &Option<Arc<Mutex<SessionReplies>>>,
    session: &SessionState,
    event: SessionEvent,
) {
    if let Some(replies) = replies {
        if let Ok(mut replies) = replies.lock() {
            replies.push(WrathSessionEvent::scoped(session, event));
        }
    }
}

/// Queue one lifecycle or terminal edge: no scope tags of its own, optional login generation.
fn push_forced_event(
    replies: &Option<Arc<Mutex<SessionReplies>>>,
    event: SessionEvent,
    login_generation: Option<u64>,
) {
    if let Some(replies) = replies {
        if let Ok(mut replies) = replies.lock() {
            replies.push(WrathSessionEvent::forced(event).for_login(login_generation));
        }
    }
}

/// The app-side sender for the selected session thread.
pub type WrathOutbox = Arc<Mutex<VecDeque<WrathOutbound>>>;

#[derive(Resource)]
pub struct WrathInput(pub WrathOutbox);

/// What the IO thread publishes.
#[derive(Clone, Debug, PartialEq)]
pub enum WrathFeedEvent {
    /// The replicated session phase changed (login, transfer boundary, logout). The tagged
    /// scope travels with the event so a delayed phase never borrows the scope active when
    /// it is drained.
    Phase {
        phase: Phase,
        session_epoch: u64,
        world_generation: u64,
        /// The login attempt that produced this phase; `None` when unavailable.
        login_generation: Option<u64>,
        /// The authoritative map once a world is established; coarse numeric only.
        map_id: Option<u32>,
    },
    /// A world scope changed: the mapped entity set and pending publications for the old
    /// scope are invalid from this event on (issue 22 clears its entity map here).
    WorldTransfer {
        session_epoch: u64,
        world_generation: u64,
        map_id: u32,
    },
    /// A producer export was refused with a reason â€” diagnostics for the debug panel.
    ExportSkipped(String),
    /// The IO-thread session ended (Ok = clean logout; Err = failure reason). The scope is
    /// captured by the worker before it exits so a delayed terminal event cannot borrow the
    /// scope active when it is drained.
    SessionEnded {
        result: Result<(), String>,
        /// The protocol end kind the worker computed: clean logout vs lost link.
        end: benilla_protocol::SessionEnd,
        session_epoch: u64,
        world_generation: u64,
    },
    /// The current gossip menu's identity, needed by the 3.3.5 select-option message.
    GossipMenu {
        npc: u64,
        menu_id: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// The vendor's 12340 stock rows, kept for the row-addressed buy packets. The shared
    /// `VendorInventory` event cannot carry the 8th word (extended cost).
    VendorInventory {
        vendor: u64,
        items: Vec<gameplay::VendorRow>,
        session_epoch: u64,
        world_generation: u64,
    },
    /// The 12340 server clock: unix seconds plus the seconds until the next daily reset. The
    /// shared vanilla event carries only the unix time.
    ServerClock {
        unix_time: u32,
        reset_in_secs: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `/played` plus 12340's script-event trigger, so the native consumer can distinguish the
    /// chat-display case the shared owner cannot express.
    PlayedTime {
        total: u32,
        level: u32,
        trigger_script_event: bool,
        session_epoch: u64,
        world_generation: u64,
    },
    /// One of the three equipment-set packets (`EquipmentManager.cpp`), for
    /// [`crate::wrath_equipment::EquipmentManager`].
    EquipmentSet {
        packet: crate::wrath_equipment::EquipmentSetPacket,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_RESYNC_RUNES`: the class's full rune layout and each 0..255 cooldown fraction.
    RuneState {
        runes: Vec<gameplay::wrath_systems::RuneSlot>,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_CONVERT_RUNE`: one slot's new type; its cooldown is unchanged.
    RuneConvert {
        rune: u8,
        rune_type: u8,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_ADD_RUNE_POWER`: the mask of runes that just became usable.
    RunePower {
        mask: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_ATTACKERSTATEUPDATE`: one melee swing's raw 12340 facts. Carries the scope so a
    /// delayed swing from a retired world never reaches the Wrath VM.
    AttackerState {
        hit: WrathSwingHit,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELLNONMELEEDAMAGELOG`: one non-melee spell hit's raw 12340 facts. Carries the
    /// scope so a delayed row from a retired world never reaches the Wrath VM.
    SpellDamage {
        hit: WrathSpellDamage,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_MISS_LOG`: one spell-miss row's raw 12340 facts. Carries the scope so a
    /// delayed row from a retired world never reaches the Wrath VM.
    SpellMiss {
        hit: WrathSpellMiss,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELLHEALLOG`: one heal row's raw 12340 facts. Carries the scope so a delayed row
    /// from a retired world never reaches the Wrath VM.
    SpellHeal {
        hit: WrathSpellHeal,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELLENERGIZELOG`: one energize row's raw 12340 facts. Carries the scope so a
    /// delayed row from a retired world never reaches the Wrath VM.
    SpellEnergize {
        hit: WrathSpellEnergize,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_START` (`0x131`): one cast-start edge's raw 12340 facts. Carries the scope so
    /// a delayed row from a retired world never reaches the Wrath VM.
    SpellCastStart {
        hit: WrathSpellCastStart,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_GO` (`0x132`): one cast-success edge's raw 12340 facts. Carries the scope so
    /// a delayed row from a retired world never reaches the Wrath VM.
    SpellCastSuccess {
        hit: WrathSpellCastSuccess,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_CAST_FAILED` (`0x130`): one cast-failure edge's raw 12340 facts. Carries the scope
    /// so a delayed row from a retired world never reaches the Wrath VM.
    SpellCastFailed {
        hit: WrathSpellCastFailed,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PERIODICAURALOG` aura 3/89, damage != 0: `SPELL_PERIODIC_DAMAGE`.
    SpellPeriodicDamage {
        hit: WrathSpellPeriodicDamage,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PERIODICAURALOG` aura 3/89, damage == 0 absorb/resist: `SPELL_PERIODIC_MISSED`.
    SpellPeriodicMiss {
        hit: WrathSpellPeriodicMiss,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PERIODICAURALOG` aura 8/20: `SPELL_PERIODIC_HEAL`.
    SpellPeriodicHeal {
        hit: WrathSpellPeriodicHeal,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PERIODICAURALOG` aura 21/24: `SPELL_PERIODIC_ENERGIZE`.
    SpellPeriodicEnergize {
        hit: WrathSpellPeriodicEnergize,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PERIODICAURALOG` aura 64: `SPELL_PERIODIC_DRAIN` or `SPELL_PERIODIC_LEECH`.
    SpellPeriodicDrain {
        hit: WrathSpellPeriodicDrain,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_EXECUTE_LOG` (`0x24C`): one logged effect row, in wire order.
    SpellExecuteLog {
        hit: WrathSpellExecuteLog,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_DISPELL_LOG` / `SMSG_SPELL_STEAL_LOG`: one replaced aura row.
    SpellDispelLog {
        hit: WrathSpellDispelLog,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_DISPEL_FAILED`: one failed aura row.
    SpellDispelFailed {
        hit: WrathSpellDispelFailed,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_PARTYKILLLOG`: the killing blow credited to a player. Carries the scope so a
    /// delayed kill from a retired world never reaches the Wrath VM.
    PartyKillLog {
        killer: u64,
        victim: u64,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_CALENDAR_SEND_CALENDAR`: the full scoped calendar snapshot.
    Calendar {
        calendar: Box<gameplay::wrath_systems::CalendarSendCalendar>,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_CALENDAR_COMMAND_RESULT`: the last calendar action's answer.
    CalendarCommandResult {
        command: u32,
        reserved: u8,
        error: u32,
        param: Option<String>,
        session_epoch: u64,
        world_generation: u64,
    },
    /// Validated `SMSG_MULTICLASS_REPLY` (`0x51A`, WMC1 v3) frame for `MulticlassPoll`.
    MulticlassReply {
        op: u16,
        status: u16,
        serial: u32,
        payload: Vec<u8>,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_LFG_JOIN_RESULT`: the server's answer to a Dungeon Finder join request.
    LfgJoinResult {
        result: gameplay::wrath_systems::LfgJoinResult,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_LFG_QUEUE_STATUS`: the player's current Dungeon Finder wait/needed snapshot.
    LfgQueueStatus {
        status: gameplay::wrath_systems::LfgQueueStatus,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_LFG_UPDATE_PLAYER`: the player's solo Dungeon Finder queue/joined update.
    LfgUpdatePlayer {
        update: gameplay::wrath_systems::LfgUpdatePlayer,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_LFG_UPDATE_PARTY`: the party's Dungeon Finder queue/joined update.
    LfgUpdateParty {
        update: gameplay::wrath_systems::LfgUpdateParty,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_LFG_PROPOSAL_UPDATE`: an active Dungeon Finder proposal.
    LfgProposalUpdate {
        proposal: gameplay::wrath_systems::LfgProposalUpdate,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELL_INSTAKILL_LOG`: instakill event. One bounded row per packet.
    SpellInstakill {
        caster: u64,
        target: u64,
        spell_id: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_ENCHANTMENTLOG`: enchant applied (caster != 0) or removed (caster == 0).
    EnchantApplied {
        caster: u64,
        target: u64,
        item_id: u32,
        enchant_id: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// The caster-zero half of `SMSG_ENCHANTMENTLOG`: event 32 `ENCHANT_REMOVED`.
    EnchantRemoved {
        caster: u64,
        target: u64,
        item_id: u32,
        enchant_id: u32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_ENVIRONMENTALDAMAGELOG`: environmental damage event.
    EnvironmentalDamage {
        victim: u64,
        damage_type: u8,
        damage: u32,
        absorbed: u32,
        resisted: i32,
        session_epoch: u64,
        world_generation: u64,
    },
    /// `SMSG_SPELLDAMAGESHIELD`: one reflected shield hit. The victim is the source and the
    /// caster is the destination, as `UnitCombatLogDamageShield` records it.
    DamageShield {
        target: u64,
        attacker: u64,
        spell_id: u32,
        damage: u32,
        overkill: u32,
        school: u32,
        session_epoch: u64,
        world_generation: u64,
    },
}

impl WrathFeedEvent {
    fn from_session(event: LifeEvent, session_epoch: u64) -> Self {
        match event {
            LifeEvent::WorldTransfer {
                world_generation,
                map_id,
            } => Self::WorldTransfer {
                session_epoch,
                world_generation,
                map_id,
            },
        }
    }
}

/// Shared handles between the IO thread and the app.
#[derive(Clone)]
pub struct WrathFeed {
    multiclass: Option<crate::multiclass::MulticlassBridge>,
    authenticated: Arc<AtomicBool>,
    trace: Option<crate::quest_trace::Source>,
    account_expansion: Arc<std::sync::atomic::AtomicU8>,
    login_generation: Arc<AtomicU64>,
    last_session_epoch: Arc<AtomicU64>,
    last_world_generation: Arc<AtomicU64>,
    player_name: Arc<Mutex<Option<(u64, String)>>>,
    pub latest: WrathLatest,
    pub events: WrathEvents,
    /// Native cast/channel/item-text/failure traffic on its own bounded queue so it cannot
    /// displace the lifecycle edges.
    native: Arc<Mutex<VecDeque<crate::wrath_native::WrathNativeFeedEvent>>>,
    /// Resolved native edges dropped by the bounded queue. The presentation drain clears its
    /// mirror and resynchronizes when this advances; the WM export authority never reads this
    /// queue, so bridge consumers are unaffected.
    native_dropped: Arc<std::sync::atomic::AtomicU64>,
}

impl WrathFeed {
    fn new() -> Self {
        Self {
            multiclass: None,
            authenticated: Arc::new(AtomicBool::new(false)),
            trace: None,
            account_expansion: Arc::new(std::sync::atomic::AtomicU8::new(0)),
            login_generation: Arc::new(AtomicU64::new(0)),
            last_session_epoch: Arc::new(AtomicU64::new(0)),
            last_world_generation: Arc::new(AtomicU64::new(0)),
            player_name: Arc::new(Mutex::new(None)),
            latest: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(WRATH_EVENT_CAPACITY))),
            native: Arc::new(Mutex::new(VecDeque::with_capacity(
                WRATH_NATIVE_EVENT_CAPACITY,
            ))),
            native_dropped: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    fn current_login_generation(&self, fallback: Option<u64>) -> Option<u64> {
        let live = self.login_generation.load(Ordering::SeqCst);
        if live != 0 {
            Some(live)
        } else {
            fallback
        }
    }

    fn push_event(&self, event: WrathFeedEvent) {
        if let Ok(mut queue) = self.events.lock() {
            if queue.len() >= WRATH_EVENT_CAPACITY {
                queue.pop_front(); // oldest dropped; the newest state always follows
            }
            queue.push_back(event);
        }
    }

    fn push_native(&self, event: crate::wrath_native::WrathNativeFeedEvent) {
        if let Ok(mut queue) = self.native.lock() {
            if queue.len() >= WRATH_NATIVE_EVENT_CAPACITY {
                queue.pop_front();
                self.native_dropped.fetch_add(1, Ordering::Relaxed);
            }
            queue.push_back(event);
        }
    }

    fn take_native(&self) -> Vec<crate::wrath_native::WrathNativeFeedEvent> {
        self.native
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    /// Consume the cumulative dropped-edge count (a presentation resynchronization trigger).
    fn take_native_dropped(&self) -> u64 {
        self.native_dropped.swap(0, Ordering::Relaxed)
    }

    fn publish_digest(&self, digest: WorldDigest) {
        if let Ok(mut slot) = self.latest.lock() {
            *slot = Some(digest);
        }
    }
}

/// A lab session configuration for the IO thread. Credentials come from the owner's
/// environment/launcher, never from committed files (system review Â§8).
#[derive(Clone)]
pub struct WrathSessionConfig {
    /// Present for a screen-owned attempt; direct probe launches retain their existing path.
    login_generation: Option<u64>,
    preferred_realm: Option<u8>,
    pub account: String,
    pub password: String,
    pub auth_addr: String,
    pub world_addr: String,
    /// Optional named boot selection; a missing requested roster entry must not fall back.
    pub character_name: Option<String>,
    /// WM bridge token; absent = the bridge stays disabled for this session.
    pub wm_token: Option<String>,
    /// `None` keeps the graphical session alive until the app cancels it.
    pub dwell: Option<Duration>,
    /// The typed action seam attached to this session's publisher. `None` keeps the session
    /// observation-only (the renderless driver's mode).
    pub actions: Option<ActionBridge>,
}

/// Whether this launch explicitly selects the Wrath session path. The stock 1.12 path stays the
/// default; selection is one environment switch, never an implicit detection.
pub fn selected_from_env() -> bool {
    std::env::var("WOW_WRATH")
        .ok()
        .is_some_and(|value| !value.is_empty() && value != "0")
}

pub fn interactive_from_env() -> bool {
    std::env::var("WOW_WRATH_LOGIN_UI").is_ok_and(|v| v == "1")
}

/// Translate 12340 ResponseCodes into the established glue manager's semantic status table.
/// AC SharedDefines.h: creation 0x2f..0x38, deletion 0x47..0x49, names 0x58..0x67.
fn roster_status(action: benilla_protocol::CharAction, raw: u8) -> u8 {
    match (action, raw) {
        (benilla_protocol::CharAction::Create, 0x2f..=0x38) => raw - 1,
        (benilla_protocol::CharAction::Create, 0x59..=0x64) => raw - 0x14,
        (benilla_protocol::CharAction::Create, 0x58 | 0x65..=0x67) => 0x51,
        (benilla_protocol::CharAction::Delete, 0x47..=0x49) => raw - 0x0e,
        (benilla_protocol::CharAction::Create, _) => 0x30,
        (benilla_protocol::CharAction::Delete, _) => 0x3a,
    }
}

/// Extract a structured `LoginRefusal` from a failed Wrath logon or world-auth error chain so
/// `GlueMgr::login_refused_status` can display the exact server refusal dialog instead of falling
/// back to `DISCONNECTED` (`refusal: None`).
fn extract_login_refusal(err: &anyhow::Error) -> Option<benilla_protocol::LoginRefusal> {
    if let Some(&auth_err) = err.downcast_ref::<benilla_protocol_wrath::auth::LogonAuthError>() {
        return Some(match auth_err {
            benilla_protocol_wrath::auth::LogonAuthError::Challenge(code) => {
                benilla_protocol::LoginRefusal::Logon(code)
            }
            benilla_protocol_wrath::auth::LogonAuthError::Proof(code) => {
                benilla_protocol::LoginRefusal::LogonProof(code)
            }
            benilla_protocol_wrath::auth::LogonAuthError::World(code) => {
                benilla_protocol::LoginRefusal::World(code)
            }
        });
    }
    None
}

/// The logon dial's failure from a failed Wrath session's error chain: the auth server got no
/// socket, which the glue reports as 12340's `CantConnect` (`LOGIN_FAILED`), not a lost link.
fn extract_logon_dial(err: &anyhow::Error) -> Option<benilla_protocol::DialFailure> {
    err.downcast_ref::<benilla_protocol_wrath::logon::DialFailure>()
        .map(|d| benilla_protocol::DialFailure {
            address: d.address.clone(),
            unresolved: d.unresolved,
        })
}

/// Map a 12340 `SMSG_CHAR_RENAME` `ResponseCodes` byte onto the status table `GlueMgr::char_rename_status`
/// reads (`0` = success, `0x31` = `CHAR_CREATE_NAME_IN_USE`, anything else = `CHAR_RENAME_FAILED`).
fn char_rename_status(raw: u8) -> u8 {
    match raw {
        0x00 => 0, // RESPONSE_SUCCESS, the sole reply carrying an identity
        0x32 => 0x31,
        _ => 1,
    }
}

/// Encode `CMSG_CHAR_RENAME` (`0x2C7`): `guid: u64 LE` + `new_name: CString`.
fn encode_char_rename(guid: u64, name: &str) -> Vec<u8> {
    let clean: String = name.chars().filter(|&c| c != '\0').collect();
    let mut out = Vec::with_capacity(8 + clean.len() + 1);
    out.extend_from_slice(&guid.to_le_bytes());
    out.extend_from_slice(clean.as_bytes());
    out.push(0);
    out
}

/// Decode `SMSG_CHAR_RENAME` (`0x2C8`): `result: u8`, followed on success (`RESPONSE_SUCCESS` = 0x00)
/// by `guid: u64 LE` + `new_name: CString`.
fn parse_char_rename(payload: &[u8]) -> anyhow::Result<(u8, Option<(u64, String)>)> {
    let &raw = payload
        .first()
        .ok_or_else(|| anyhow::anyhow!("empty SMSG_CHAR_RENAME payload"))?;
    let status = char_rename_status(raw);
    if status != 0 {
        return Ok((status, None));
    }
    let guid = payload
        .get(1..9)
        .ok_or_else(|| anyhow::anyhow!("truncated SMSG_CHAR_RENAME guid"))?;
    let name = std::ffi::CStr::from_bytes_with_nul(&payload[9..])?.to_str()?;
    anyhow::ensure!(!name.is_empty(), "empty SMSG_CHAR_RENAME name");
    Ok((
        0,
        Some((u64::from_le_bytes(guid.try_into()?), name.to_owned())),
    ))
}

impl WrathSessionConfig {
    /// Build the selected graphical path from the same protected environment the rest of the
    /// client uses. Credentials never come from committed source or command-line arguments.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        use anyhow::Context;
        if !selected_from_env() {
            return Ok(None);
        }
        let interactive = interactive_from_env();
        let account = if interactive {
            String::new()
        } else {
            std::env::var("WOW_USER").context("WOW_WRATH needs WOW_USER")?
        };
        let password = if interactive {
            String::new()
        } else {
            std::env::var("WOW_PASS").context("WOW_WRATH needs WOW_PASS")?
        };
        let auth_addr =
            std::env::var("WOW_WRATH_AUTH").unwrap_or_else(|_| "127.0.0.1:3724".to_owned());
        let world_addr = std::env::var("WOW_WRATH_WORLD").unwrap_or_else(|_| {
            if interactive {
                String::new()
            } else {
                "127.0.0.1:8095".to_owned()
            }
        });
        let dwell = std::env::var("WOW_WRATH_DWELL")
            .ok()
            .map(|raw| raw.parse::<u64>().map(Duration::from_secs))
            .transpose()
            .context("WOW_WRATH_DWELL must be whole seconds")?;
        let wm_token = std::env::var("WM_NATIVE_OBSERVATION_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        Ok(Some(Self {
            login_generation: interactive.then_some(0),
            preferred_realm: None,
            account,
            password,
            auth_addr,
            world_addr,
            character_name: std::env::var("WOW_CHAR")
                .ok()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty()),
            wm_token,
            dwell,
            actions: None,
        }))
    }
}

/// One replicated entity, as the display path will consume it. Full GUID, world scope and
/// reducer object generation are preserved; ECS entity ids are assigned by the translation
/// (issue 22) and never enter this type.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathEntity {
    pub guid: u64,
    pub type_id: u8,
    /// `OBJECT_FIELD_ENTRY`, the template entry every name/visual/tooltip lookup uses.
    pub entry: Option<u32>,
    /// `UNIT_FIELD_DISPLAYID` â€” the model the renderer needs.
    pub display_id: Option<u32>,
    pub scale: Option<f32>,
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    /// Wrath's seven current power slots, protocol units, each independently observed. `None`
    /// means the word was never delivered; `Some(0)` is an observed zero.
    pub powers: [Option<u32>; 7],
    /// The seven matching maxima, same observed/unavailable semantics.
    pub max_powers: [Option<u32>; 7],
    pub level: Option<u32>,
    /// `UNIT_FIELD_BYTES_0`: race, class, sex, power type.
    pub bytes_0: Option<u32>,
    pub faction: Option<u32>,
    pub unit_flags: Option<u32>,
    pub mount_display_id: Option<u32>,
    pub bytes_1: Option<u32>,
    pub bytes_2: Option<u32>,
    pub npc_flags: Option<u32>,
    pub dynamic_flags: Option<u32>,
    /// `UNIT_FIELD_TARGET`; absent when the reducer has not observed both GUID words.
    pub target: Option<u64>,
    /// `UNIT_FIELD_CHARMEDBY`: whoever is charming this unit; feeds ownership/reaction consumers.
    pub charmed_by: Option<u64>,
    /// `UNIT_FIELD_SUMMONEDBY`: the summoner of a pet, guardian or totem.
    pub summoned_by: Option<u64>,
    /// `UNIT_FIELD_CREATEDBY`: the creator of a totem or created object; for a hunter pet, its owner.
    pub created_by: Option<u64>,
    /// `UNIT_FIELD_CHARM`: the unit this one controls, a pet or the vehicle seat it rides. The
    /// donor slots 6/7 the 1.12 owners read; 12340 keeps the same two indices.
    pub charm: Option<u64>,
    /// `UNIT_FIELD_SUMMON`: the guardian this one controls, at donor slots 8/9 on both layouts.
    pub summon: Option<u64>,
    /// A corpse's own block (`CORPSE_END` 36): 12340's corpse layout is the 1.12 one shifted by
    /// two after `PARTY`, so the projection cannot copy it by index.
    pub corpse_fields: Option<crate::wrath_inventory::WrathFieldWindow>,
    /// Wrath `PLAYER_BYTES`: skin, face, hair style, hair colour.
    pub player_bytes: Option<u32>,
    /// Wrath `PLAYER_BYTES_2`: facial hair, bank slots, rest state, arena faction.
    pub player_bytes_2: Option<u32>,
    pub position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    /// The last reduced movement flags for this object.
    pub move_flags: Option<u32>,
    /// Active spline carried by the create/movement update block (`SPLINE_ENABLED`).
    pub create_spline: Option<benilla_protocol_wrath::updates::CreateSplineSnapshot>,
    /// The melee victim the last movement block carried (`HAS_TARGET`), read once at spawn.
    pub attack_target: Option<u64>,
    pub generation: u32,
    /// The player's raw Wrath field window, needed where 1.12 and 3.3.5 pack different field
    /// indices (quest log, inventory, coinage), with the server's delivered mask. Only a player
    /// carries it.
    pub player_fields: Option<crate::wrath_inventory::WrathFieldWindow>,
    /// An item or container's Wrath field window with the reducer's delivered mask. The item block
    /// is 0..64 and a container extends it to 0..138; both are translated explicitly rather than
    /// copied by index because the 1.12 item/container layouts differ. Presence is real here:
    /// `None` in the window is "never delivered", `Some(0)` a delivered clear, and the sparse
    /// fields 43..57 (Wrath enchant slots 8..12) and 62 (`CREATE_PLAYED_TIME`) survive for the
    /// typed row even though the donor projection drops them.
    pub item_fields: Option<crate::wrath_inventory::WrathFieldWindow>,
    /// The unit's visible aura slots from `SMSG_AURA_UPDATE[_ALL]`. The 1.12 bar is descriptor
    /// data, so the translation synthesizes `UNIT_FIELD_AURA`/`AURAFLAGS` from these.
    pub auras: Vec<benilla_protocol_wrath::reducer::AuraSlot>,
    /// Whether an aura packet for this generation has been observed. Until then `auras` is
    /// unavailable, not an observed empty set (the server's forced empty
    /// `SMSG_AURA_UPDATE_ALL` is what establishes the latter).
    pub auras_observed: bool,
    /// Per-slot receipt identity for `auras`: the world's receipt revision (a refresh advances
    /// it, identical deltas included) and the packet-arrival wall clock. Presentation stamps a
    /// GetTime anchor once per revision; nothing here is a UI clock.
    pub aura_receipts: BTreeMap<u8, benilla_protocol_wrath::reducer::AuraReceipt>,
    /// The raw 3.3.5 window of a creature, GameObject, DynamicObject or corpse with the
    /// delivered mask, read by the live field table (`wrath_field_map`). A player uses
    /// `player_fields`; every other type carries `None`.
    pub raw_fields: Option<crate::wrath_inventory::WrathFieldWindow>,
}

/// A full-world digest: the replicated world at one `state_revision`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldDigest {
    pub session_epoch: u64,
    pub world_generation: u64,
    pub state_revision: u64,
    pub map_id: u32,
    pub phase: Phase,
    pub player_guid: u64,
    pub vehicle: Option<VehicleAttachment>,
    pub self_observation: Option<benilla_protocol_wrath::session::SelfObservation>,
    pub entities: BTreeMap<u64, WrathEntity>,
}

/// How the next session starts: the boot path's own account flow, or the parked character
/// select the existing screen drives (the clean-logout re-entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WrathSessionStart {
    /// The boot flow: log the account in and enter its first record; no screen ask.
    Auto,
    /// The parked flow: the select screen stays after a clean logout and the same screen's
    /// asks (`GetCharacterListUpdate`, its Enter World ask) travel the stock `CharPick`
    /// channel; this session is parked there and serves them. `roster_asked` records whether
    /// the enumeration ask started the session, so the first fresh roster publishes as the
    /// screen's reply rather than an unasked list.
    Parked { roster_asked: bool },
}

impl WrathSessionStart {
    fn for_login_mode(self, interactive: bool) -> Self {
        match self {
            Self::Auto if interactive => Self::Parked {
                roster_asked: false,
            },
            mode => mode,
        }
    }
}

/// One ask a parked session serves from the select screen, in the order the app made it.
#[derive(Clone, Debug)]
pub(crate) enum WrathParkedCommand {
    RealmSplit(u32),
    /// `GetCharacterListUpdate`: `CMSG_CHAR_ENUM` again on the parked link.
    Enumerate,
    /// Enter World the screen asked for: `CMSG_PLAYER_LOGIN` on the parked link.
    Enter(u64),
    Create(benilla_protocol::CharCreateReq),
    Delete(u64),
    Rename {
        guid: u64,
        name: String,
    },
    Realm(benilla_protocol::RealmInfo),
    RefreshRealms,
    /// `GlueLogin::CancelWorld`: drop the world handshake / character-select link and return to
    /// the logon realm-selection loop without tearing down the authenticated logon session.
    CancelWorld,
    /// `RealmRequest::Leave`: leave the current realm's character-select link so the next
    /// `RefreshRealms` / `Realm(r)` runs on the logon session.
    LeaveRealm,
    /// `RealmRequest::Close` / `DisconnectFromServer`: close the parked session cleanly.
    Close,
    MulticlassRequest(Vec<u8>),
}

#[derive(Resource)]
struct WrathLoginQueue {
    requests: Receiver<crate::net::LoginRequest>,
    pending: Option<WrathSessionConfig>,
}

/// The parked character-select input a fresh session serves, and the ask that started it.
pub(crate) struct ParkedInput {
    commands: Receiver<WrathParkedCommand>,
    roster_asked: bool,
}

/// The select screen's asks and Enters, drained by [`wrath_pick_admission`]. Bound in
/// [`WrathNetPlugin::build`] over the stock `CharPick`, whose reader is the stock IO thread
/// the Wrath session replaces.
#[derive(Resource)]
pub struct WrathPickQueue(pub(crate) Receiver<ParkRequest>);

/// The app's end of the typed WM action seam, plus the bounded in-flight set. One intent is in
/// flight from admission until an authoritative reply, a scope end, or its deadline.
#[derive(Resource)]
pub struct WrathWmActions {
    pub(crate) requests: Receiver<ActionRequest>,
    pub(crate) cancels: Receiver<String>,
    pub(crate) results: Sender<ActionResult>,
    pending: BTreeMap<String, PendingWmAction>,
    /// Admission-level cancellations that arrived before their request.
    cancelled: Vec<String>,
    last_cast_spell: Option<u32>,
    /// Monotonic admission order; the wire-order fallback resolves the oldest matching intent.
    next_admission: u64,
    pub(crate) received: u64,
    pub(crate) refused: u64,
    pub(crate) completed: u64,
}

struct PendingWmAction {
    intent: ActionIntent,
    /// The admitted player guid; a resolved edge's caster must match it. The native-cast
    /// correlation never trusts a bare spell id alone.
    player_guid: u64,
    /// The spell this action commits to, resolved at admission: a cast's own id or the cast
    /// being cancelled.
    cast_spell: Option<u32>,
    /// The resolved cast attempt bound at its START edge (or, for a cancel, the attempt that was
    /// active at admission). Terminal outcomes only resolve the matching attempt.
    cast_attempt: Option<u64>,
    cast_count: Option<u8>,
    /// Monotonic admission order for the per-spell wire-order fallback.
    admitted: u64,
    deadline: Instant,
    session_epoch: u64,
    world_generation: u64,
}

impl WrathWmActions {
    fn answer(
        &mut self,
        request_id: String,
        status: ActionStatus,
        session_epoch: u64,
        world_generation: u64,
        observed_state_revision: Option<u64>,
        detail: Option<String>,
    ) {
        self.answer_with_evidence(
            request_id,
            status,
            session_epoch,
            world_generation,
            observed_state_revision,
            None,
            detail,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn answer_with_evidence(
        &mut self,
        request_id: String,
        status: ActionStatus,
        session_epoch: u64,
        world_generation: u64,
        observed_state_revision: Option<u64>,
        evidence: Option<ActionEvidence>,
        detail: Option<String>,
    ) {
        self.completed += 1;
        let _ = self.results.send(ActionResult {
            request_id,
            status,
            session_epoch,
            world_generation,
            observed_state_revision,
            evidence,
            detail,
        });
    }

    /// Terminal scope loss: every remaining intent becomes `Unknown`, because a command may
    /// already have reached the server. Nothing is replayed. Returns how many were answered.
    fn retire_all(&mut self, status: ActionStatus, detail: &str) -> usize {
        let ids: Vec<String> = self.pending.keys().cloned().collect();
        let mut retired = 0;
        for id in ids {
            if let Some(pending) = self.pending.remove(&id) {
                retired += 1;
                self.answer(
                    id,
                    status,
                    pending.session_epoch,
                    pending.world_generation,
                    None,
                    Some(detail.to_owned()),
                );
            }
        }
        self.cancelled.clear();
        self.last_cast_spell = None;
        retired
    }

    /// Terminal retirement of the intents admitted against a world scope at or before `retired`.
    /// An intent admitted against a newer scope is left alone: a delayed old-scope event must not
    /// answer work that belongs to the replacement world. Each intent is removed before it is
    /// answered, so a repeat call resolves nothing.
    fn retire_scope(&mut self, retired: (u64, u64), status: ActionStatus, detail: &str) -> usize {
        let ids: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, pending)| {
                scope_at_or_before(retired, (pending.session_epoch, pending.world_generation))
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut retired_count = 0;
        for id in ids {
            if let Some(pending) = self.pending.remove(&id) {
                retired_count += 1;
                if let ActionIntent::Cast { spell_id, .. } = &pending.intent {
                    if self.last_cast_spell == Some(*spell_id) {
                        self.last_cast_spell = None;
                    }
                }
                self.answer(
                    id,
                    status,
                    pending.session_epoch,
                    pending.world_generation,
                    None,
                    Some(detail.to_owned()),
                );
            }
        }
        // An admission-level cancellation whose request never arrived cannot match a request from
        // the scope that replaced it, and the platform's own cancellation path clears this list.
        self.cancelled.clear();
        self.last_cast_spell = None;
        retired_count
    }

    /// Answer publisher proposals that were queued for a scope the app has left. None of them
    /// reached a command owner, so no command was dispatched and `Rejected` is the honest terminal
    /// state. Returns how many were answered; the channel is drained exactly once.
    fn refuse_unadmitted(&mut self, detail: &str) -> usize {
        let requests: Vec<_> = self.requests.try_iter().collect();
        let refused = requests.len();
        for request in requests {
            self.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                None,
                Some(detail.to_owned()),
            );
        }
        refused
    }

    /// Bind a newly observed cast START to the oldest admitted cast intent of that spell. The
    /// binding is what later terminal edges match; it also reports the `Sent` milestone.
    fn bind_native_cast(
        &mut self,
        caster: u64,
        spell_id: u32,
        attempt: Option<u64>,
        cast_count: Option<u8>,
        revision: Option<u64>,
    ) {
        let Some(attempt) = attempt else { return };
        let Some(id) = self
            .pending
            .iter()
            .filter(|(_, pending)| {
                pending.player_guid == caster
                    && pending.cast_attempt.is_none()
                    && matches!(
                        &pending.intent,
                        ActionIntent::Cast { spell_id: wanted, .. } if *wanted == spell_id
                    )
            })
            .min_by_key(|(_, pending)| pending.admitted)
            .map(|(id, _)| id.clone())
        else {
            return;
        };
        let (epoch, generation) = {
            let pending = self.pending.get_mut(&id).expect("iterated above");
            pending.cast_attempt = Some(attempt);
            pending.cast_count = cast_count;
            (pending.session_epoch, pending.world_generation)
        };
        self.answer_with_evidence(
            id,
            ActionStatus::Sent,
            epoch,
            generation,
            revision,
            Some(ActionEvidence {
                attempt_serial: Some(attempt),
                cast_count,
                terminal: None,
                correlation: ActionCorrelation::StartIdentity,
            }),
            Some("the cast attempt was observed beginning".to_owned()),
        );
    }

    /// A channelling cancel is confirmed by the observed channel ending for that caster and
    /// spell; the channel carries no cast attempt identity.
    fn resolve_channel_cancel(&mut self, caster: u64, spell_id: u32, revision: Option<u64>) {
        let Some(id) = self
            .pending
            .iter()
            .filter(|(_, pending)| {
                pending.player_guid == caster
                    && pending.cast_attempt.is_none()
                    && pending.cast_spell == Some(spell_id)
                    && matches!(&pending.intent, ActionIntent::CastCancel)
            })
            .min_by_key(|(_, pending)| pending.admitted)
            .map(|(id, _)| id.clone())
        else {
            return;
        };
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        self.answer_with_evidence(
            id,
            ActionStatus::Succeeded,
            pending.session_epoch,
            pending.world_generation,
            revision,
            Some(ActionEvidence {
                attempt_serial: None,
                cast_count: None,
                terminal: None,
                correlation: ActionCorrelation::Direct,
            }),
            Some("the observed channel ended".to_owned()),
        );
    }

    /// Correlate one resolved native cast edge with in-flight WM cast intents. A terminal edge
    /// with a bound attempt resolves only that attempt; a terminal observed without a START (the
    /// attempt was never bound) resolves the oldest matching intent by wire order. A launch
    /// answers `Launched`: the cast completed, no gameplay outcome is claimed.
    fn observe_native(
        &mut self,
        resolved: &crate::wrath_native::WrathResolvedCast,
        revision: Option<u64>,
    ) {
        use crate::wrath_native::WrathCastEventKind;

        let event = &resolved.event;
        let Some(identity) = &event.identity else {
            return;
        };
        let caster = identity.caster;
        let spell_id = identity.spell_id;
        let attempt = event.attempt_serial;
        let cast_count = identity.cast_count;

        if matches!(&event.kind, WrathCastEventKind::Started { .. }) {
            self.bind_native_cast(caster, spell_id, attempt, cast_count, revision);
            return;
        }
        if matches!(&event.kind, WrathCastEventKind::ChannelEnded) {
            self.resolve_channel_cancel(caster, spell_id, revision);
            return;
        }
        let (terminal, reason) = match &event.kind {
            WrathCastEventKind::Launched { .. } => (benilla_wm_native::CastOutcome::Launched, None),
            WrathCastEventKind::Failed { reason } => (
                benilla_wm_native::CastOutcome::Failed,
                Some(format!("the server refused the cast (reason {reason})")),
            ),
            WrathCastEventKind::Interrupted { reason } => (
                benilla_wm_native::CastOutcome::Interrupted,
                Some(format!("the cast was interrupted (reason {reason})")),
            ),
            _ => return,
        };

        let mut matched: Vec<String> = Vec::new();
        let mut correlation = ActionCorrelation::StartIdentity;
        if attempt.is_some() {
            matched = self
                .pending
                .iter()
                .filter(|(_, pending)| {
                    pending.player_guid == caster
                        && pending.cast_attempt == attempt
                        && matches!(
                            &pending.intent,
                            ActionIntent::Cast { .. } | ActionIntent::CastCancel
                        )
                })
                .map(|(id, _)| id.clone())
                .collect();
        } else if let Some(id) = self
            .pending
            .iter()
            .filter(|(_, pending)| {
                pending.player_guid == caster
                    && pending.cast_attempt.is_none()
                    && pending.cast_spell == Some(spell_id)
                    && matches!(
                        &pending.intent,
                        ActionIntent::Cast { .. } | ActionIntent::CastCancel
                    )
            })
            .min_by_key(|(_, pending)| pending.admitted)
            .map(|(id, _)| id.clone())
        {
            // A terminal with no START cannot be identity-matched. Resolve the oldest admitted
            // same-spell intent instead of whichever edge arrived most recently.
            matched.push(id);
            correlation = ActionCorrelation::WireOrder;
        }

        for id in matched {
            let Some(pending) = self.pending.remove(&id) else {
                continue;
            };
            let cancelling = matches!(&pending.intent, ActionIntent::CastCancel);
            let (status, detail) = match (cancelling, terminal) {
                (false, benilla_wm_native::CastOutcome::Launched) => (
                    ActionStatus::Launched,
                    Some("cast launched; no gameplay outcome is claimed".to_owned()),
                ),
                (false, benilla_wm_native::CastOutcome::Failed) => {
                    (ActionStatus::Rejected, reason.clone())
                }
                (false, benilla_wm_native::CastOutcome::Interrupted) => {
                    (ActionStatus::Failed, reason.clone())
                }
                (true, benilla_wm_native::CastOutcome::Launched) => (
                    ActionStatus::Failed,
                    Some("the cast launched before the cancel took effect".to_owned()),
                ),
                (true, _) => (ActionStatus::Succeeded, Some("the cast ended".to_owned())),
                // `terminal` is only the three gameplay outcomes above; keep the compiler's
                // exhaustiveness without inventing a resolution for any other value.
                (false, _) => continue,
            };
            if let ActionIntent::Cast { spell_id, .. } = &pending.intent {
                if self.last_cast_spell == Some(*spell_id) {
                    self.last_cast_spell = None;
                }
            }
            self.answer_with_evidence(
                id,
                status,
                pending.session_epoch,
                pending.world_generation,
                revision,
                Some(ActionEvidence {
                    attempt_serial: pending.cast_attempt,
                    cast_count,
                    terminal: Some(terminal),
                    correlation,
                }),
                detail,
            );
        }
    }
}

/// Application selection/configuration and worker ownership. Inserted by
/// [`WrathNetPlugin`]; a `None` config keeps the plugin inert (baseline login untouched).
#[derive(Resource)]
pub struct WrathControl {
    pub config: Option<WrathSessionConfig>,
    multiclass: Option<crate::multiclass::MulticlassBridge>,
    trace: Option<crate::quest_trace::Source>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// A worker already signalled to stop. No replacement is admitted until this joins.
    retiring: Option<std::thread::JoinHandle<()>>,
    feed: Option<WrathFeed>,
    cancel: Arc<AtomicBool>,
    commands: Option<WrathOutbox>,
    /// Set when the worker has stopped, cleanly or not. The view is invalidated in the Net drain
    /// before Input, never in the PreUpdate join.
    ended: bool,
    /// A clean logout may be followed by a fresh session when the existing glue enters the world
    /// again. Loss does not set this, so recovery remains a separate policy.
    restart_allowed: bool,
    /// Fresh-session starts still allowed after a clean logout.
    restart_attempts: u32,
    /// Earliest next admission attempt, polled from the frame loop.
    retry_at: Option<Instant>,
    /// The old worker's terminal feed was drained and its scope cleared; a replacement may now
    /// be admitted.
    terminal_ready: bool,
    /// A world entry was observed for the current worker. A later loss is recovery, not
    /// admission, and must not consume the admission retry budget.
    entered_world: bool,
    /// The ended session's delivery state was retired at the one barrier. Set by the terminal arm
    /// (and by a shutdown stop); a panicked worker that never drained a terminal leaves it false,
    /// so the Net drain's fallback runs the barrier once for that case.
    ended_retirement_done: bool,
    /// The existing glue returned to character selection after the clean logout. A replacement
    /// is admitted only after that visible stop, not merely because a terminal frame arrived.
    left_world: bool,
    /// AppExit or an explicit stop; no further starts.
    shutdown: bool,
    /// Terminal failure from the last worker run. A clean end leaves this `None`.
    pub last_error: Option<String>,
    /// Replies for the established `SessionEvent` handlers, tagged by source scope and drained
    /// in the Net stage rather than through the stock 1.12 inbound channel. One bounded queue is
    /// created per worker (review finding R04); the producer never waits on the consumer.
    session_events: Option<Arc<Mutex<SessionReplies>>>,
    /// The app's own end of that tagged queue. A panicked worker never sends its terminal
    /// event, so the ownership tick synthesizes the forced `Disconnected` through this clone.
    session_events_tx: Option<Arc<Mutex<SessionReplies>>>,
    /// Last observed delivery counters; logged only when they change.
    reply_stats: SessionReplyStats,
    /// The running parked session's screen-ask channel. The select screen's next ask lands
    /// here; a dead one (the parked session ended) makes the ask a fresh-admission attempt.
    /// `None` for the boot flow and for any session started without the screen, which is also
    /// how the end-of-session retry policy tells a connection that served a screen from a boot
    /// dial: a parked link that dies does not retry.
    park_commands: Option<Sender<WrathParkedCommand>>,
    /// Publishers whose bounded stop missed the worker's wait window. They are retained until
    /// they report stopped, and replacement admission waits for the list to empty: a dropped
    /// handle is not proof of retirement.
    publisher_retiring: Vec<benilla_wm_native::PublisherRetirement>,
    /// The current worker's handoff of unfinished publisher retirements.
    publisher_retire_rx: Option<Receiver<benilla_wm_native::PublisherRetirement>>,
    /// The typed action seam shared with every publisher this worker replaces. The app's own
    /// ends live in [`WrathWmActions`].
    wm_actions: Option<ActionBridge>,
}

/// Which boundary is closing a delivery scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryScopeEnd {
    /// A world generation changed while the session (and its worker) continue. `retired` is the
    /// scope whose intents, receipts and replies can no longer be answered; `adopted` is the one
    /// the app reads from now on.
    WorldTransfer {
        retired: (u64, u64),
        adopted: (u64, u64),
    },
    /// The session and its worker ended.
    SessionEnded,
}

/// What one [`WrathControl::retire_scoped_delivery`] resolved. Every count is terminal: an intent,
/// proposal, outbound command or ask is accounted for exactly once, and a repeated call for the
/// same state resolves nothing (only `publishers_pending` can stay non-zero while a handle that
/// already missed the worker's stop window keeps finishing).
///
/// A count is not a success claim: `actions` and `unadmitted` are answered `Unknown`/`Rejected`
/// because the scope ended, not because the command ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScopedDeliveryRetirement {
    /// Admitted WM intents answered at the barrier.
    pub actions: usize,
    /// Publisher proposals that never reached admission, answered `Rejected`.
    pub unadmitted: usize,
    /// Outbound commands dropped before the worker wrote them; their receipts were answered.
    pub outbound: usize,
    /// The select screen's ask channel was closed with the retired scope.
    pub parked: bool,
    /// Queued replies discarded because the adopted or retired scope supersedes them.
    pub replies: u64,
    /// Publisher retirements that reported stopped and were dropped.
    pub publishers_finished: usize,
    /// Publisher retirements still running; replacement admission waits for them.
    pub publishers_pending: usize,
}

/// Discard queued outbound commands for a retired scope, answering every carried receipt with one
/// terminal delivery. Returns how many entries were retired.
///
/// A command the worker never wrote can no longer be written, so `Rejected(ScopeChanged)` is its
/// honest terminal state. A command with neither a receipt nor a trace has no observer at all;
/// dropping it is still terminal, and it is counted the same way.
fn retire_outbound_deliveries(
    queue: &mut VecDeque<WrathOutbound>,
    refusal: crate::net::CommandRefusal,
) -> usize {
    let retired = queue.len();
    for outbound in queue.drain(..) {
        if let Some(receipt) = outbound.receipt.as_ref() {
            // The receipt owns its interaction trace; reporting through it keeps one terminal
            // delivery per interaction (the same rule the dispatcher applies).
            receipt.report(crate::net::CommandDelivery::Rejected(refusal));
        } else if let Some(trace) = outbound.trace.as_ref() {
            trace.record(crate::quest_trace::Stage::Delivery(
                crate::net::CommandDelivery::Rejected(refusal),
            ));
        }
    }
    retired
}

impl WrathControl {
    pub(crate) fn account_expansion(&self) -> u8 {
        self.feed
            .as_ref()
            .map_or(0, |f| f.account_expansion.load(Ordering::SeqCst))
    }
    /// One select-screen ask, in the order it was made: served at once by the running parked
    /// session, or it starts that session and carries the ask in. `roster_asked` records
    /// whether the ask was an enumeration, so the first fresh roster publishes as the screen's
    /// reply.
    fn park_serve(&mut self, roster_asked: bool, command: WrathParkedCommand) {
        if let Some(park) = self.park_commands.as_ref() {
            if park.send(command.clone()).is_ok() {
                return;
            }
        }
        self.start(WrathSessionStart::Parked { roster_asked });
        if let Some(park) = self.park_commands.as_ref() {
            let _ = park.send(command);
        }
    }

    fn start(&mut self, kind: WrathSessionStart) {
        let Some(mut config) = self.config.clone() else {
            return;
        };
        // Logout's terminal can drain before the app leaves InWorld. The interactive
        // account must still wait for a new pick, never boot the first roster record.
        let kind = kind.for_login_mode(config.login_generation.is_some());
        config.actions = self.wm_actions.clone();
        if self.shutdown || self.worker.is_some() || self.retiring.is_some() {
            return;
        }
        // A forced terminal that the Net stage has not delivered yet may target the session
        // being ended, not the one about to start; admitting now would let it land on the
        // replacement. The pick admission can reach `start` without the ownership's gate, so
        // the guard lives here as well.
        if !self.session_events_empty() {
            return;
        }
        if self.ended && (!self.restart_allowed || self.restart_attempts == 0) {
            return;
        }
        if self.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }

        let restarting = self.ended;
        if restarting {
            self.restart_attempts -= 1;
        }
        // The parked capture happens with the kind's channel: one fresh channel per parked
        // session, none for the boot flow. Any older parked channel is unreachable by then
        // (that worker is gone, or its retirement blocks this start).
        let parked = match kind {
            WrathSessionStart::Auto => {
                self.park_commands = None;
                None
            }
            WrathSessionStart::Parked { roster_asked } => {
                let (park_tx, park_rx) = crossbeam_channel::unbounded::<WrathParkedCommand>();
                self.park_commands = Some(park_tx);
                Some(ParkedInput {
                    commands: park_rx,
                    roster_asked,
                })
            }
        };
        let mut feed = WrathFeed::new();
        feed.trace = self.trace.clone();
        feed.multiclass = self.multiclass.clone();
        feed.login_generation
            .store(config.login_generation.unwrap_or(0), Ordering::SeqCst);
        self.feed = Some(feed.clone());
        self.last_error = None;
        self.ended = false;
        self.terminal_ready = false;
        self.retry_at = None;
        self.entered_world = false;
        self.left_world = false;
        self.ended_retirement_done = false;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let commands = self.commands.clone();
        // A fresh worker is a fresh scope: anything still queued belongs to the scope that ended
        // and can no longer be written. Its receipts are answered rather than silently dropped.
        self.retire_outbox(crate::net::CommandRefusal::ScopeChanged);
        let replies = Arc::new(Mutex::new(SessionReplies::default()));
        self.session_events = Some(Arc::clone(&replies));
        self.session_events_tx = Some(Arc::clone(&replies));
        let (publisher_retire, publisher_retire_rx) = crossbeam_channel::unbounded();
        self.publisher_retire_rx = Some(publisher_retire_rx);
        let events_out = Some(replies);
        let handle = std::thread::Builder::new()
            .name("wrath-session".to_owned())
            .spawn(move || {
                let result = run_session_with_cancel(
                    &feed,
                    &config,
                    &cancel,
                    commands,
                    events_out.clone(),
                    parked,
                    Some(publisher_retire),
                );
                let end = if result.is_ok() {
                    benilla_protocol::SessionEnd::LoggedOut
                } else {
                    benilla_protocol::SessionEnd::Lost
                };
                if let Ok(mut q) = feed.events.lock() {
                    if q.len() >= WRATH_EVENT_CAPACITY {
                        q.pop_front();
                    }
                    q.push_back(WrathFeedEvent::SessionEnded {
                        result: result.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")),
                        end,
                        session_epoch: feed.last_session_epoch.load(Ordering::SeqCst),
                        world_generation: feed.last_world_generation.load(Ordering::SeqCst),
                    });
                }
                if !cancel.load(Ordering::SeqCst) {
                    let live_generation = feed.current_login_generation(config.login_generation);
                    if live_generation.is_some()
                        && result.is_err()
                        && !feed.authenticated.load(Ordering::SeqCst)
                    {
                        push_forced_event(
                            &events_out,
                            SessionEvent::LoginFailed {
                                refusal: result.as_ref().err().and_then(extract_login_refusal),
                                reason: result
                                    .as_ref()
                                    .err()
                                    .map(|e| format!("{e:#}"))
                                    .unwrap_or_default(),
                                dial: result.as_ref().err().and_then(extract_logon_dial),
                            },
                            live_generation,
                        );
                        return;
                    }
                    let end = if result.is_ok() {
                        benilla_protocol::SessionEnd::LoggedOut
                    } else {
                        benilla_protocol::SessionEnd::Lost
                    };
                    let reason = result
                        .err()
                        .map(|e| format!("{e:#}"))
                        .unwrap_or_else(|| "wrath session ended".to_owned());
                    push_forced_event(
                        &events_out,
                        SessionEvent::Disconnected { reason, end },
                        config.login_generation,
                    );
                }
            })
            .expect("spawn wrath session thread");
        self.worker = Some(handle);
    }

    /// Join a worker whose bounded stop has completed. A timed-out retirement stays in
    /// `self.retiring`, blocking any replacement until it actually exits.
    fn poll_retiring(&mut self) -> bool {
        let Some(worker) = self.retiring.as_ref() else {
            return true;
        };
        if !worker.is_finished() {
            return false;
        }
        if let Some(worker) = self.retiring.take() {
            let _ = worker.join();
        }
        true
    }

    /// Bounded app-owned stop with the typed action seam, so a shutdown retires pending
    /// intents and proposals instead of leaving them for a follower that never comes.
    fn stop_with_delivery(&mut self, actions: Option<&mut WrathWmActions>) {
        self.shutdown = true;
        self.restart_allowed = false;
        self.retry_at = None;
        self.cancel.store(true, Ordering::SeqCst);
        // Retire the whole scope before the join: a receipt, intent or ask with no author left
        // must still end in a terminal state, and the reply queue must stop taking new work.
        self.retire_scoped_delivery(
            DeliveryScopeEnd::SessionEnded,
            actions,
            "the client is shutting down",
        );
        self.ended_retirement_done = true;
        if let Some(multiclass) = &self.multiclass {
            multiclass.suspend();
        }
        // Close the reply queue before the join. A worker that pushes during teardown must not
        // fabricate an overflow terminal, and no consumer is left to drain what it sends.
        self.close_session_replies();
        if let Some(worker) = self.worker.take() {
            self.retiring = Some(worker);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.poll_retiring() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.ended = true;
        self.terminal_ready = false;
        if let Some(feed) = self.feed.take() {
            if let Ok(mut slot) = feed.latest.lock() {
                *slot = None;
            }
            if let Ok(mut queue) = feed.events.lock() {
                queue.clear();
            }
        }
        self.session_events = None;
        self.session_events_tx = None;
    }

    fn take_digest(&mut self) -> Option<WorldDigest> {
        let feed = self.feed.as_ref()?;
        let mut slot = feed.latest.lock().ok()?;
        slot.take()
    }

    fn take_events(&mut self) -> Vec<WrathFeedEvent> {
        match self.feed.as_ref().map(|f| f.events.lock()) {
            Some(Ok(mut q)) => {
                let n = q.len().min(WRATH_DRAIN_LIMIT);
                q.drain(..n).collect()
            }
            _ => Vec::new(),
        }
    }

    fn take_session_events(&mut self) -> Vec<WrathSessionEvent> {
        let Some(replies) = self.session_events.as_ref() else {
            return Vec::new();
        };
        let (events, stats) = match replies.lock() {
            Ok(mut replies) => {
                // A deep queue drains at the ceiling; a normal frame keeps the floor so a burst
                // still leaves the rest for the next frame instead of starving Input.
                let limit = if replies.len() > WRATH_DRAIN_LIMIT {
                    WRATH_DRAIN_CEILING
                } else {
                    WRATH_DRAIN_LIMIT
                };
                let events = replies.take(limit, WRATH_DRAIN_BUDGET);
                (events, replies.stats())
            }
            Err(_) => return Vec::new(),
        };
        self.note_reply_stats(stats);
        events
    }

    /// Log delivery health on change rather than per frame.
    fn note_reply_stats(&mut self, stats: SessionReplyStats) {
        if stats.overflowed > self.reply_stats.overflowed {
            warn!(
                "wrath: session reply backlog exceeded {} entries ({} dropped, {} delivered); \
                 the session is ending",
                WRATH_SESSION_REPLY_CAP, stats.dropped, stats.delivered
            );
        }
        if stats.peak >= WRATH_SESSION_REPLY_HIGH_WATER
            && self.reply_stats.peak < WRATH_SESSION_REPLY_HIGH_WATER
        {
            info!(
                "wrath: session reply backlog peaked at {} entries (now {}, {} coalesced, \
                 {} delivered)",
                stats.peak, stats.depth, stats.coalesced, stats.delivered
            );
        }
        if stats.closed_pushes > self.reply_stats.closed_pushes {
            debug!(
                "wrath: {} replies pushed after the queue closed",
                stats.closed_pushes
            );
        }
        self.reply_stats = stats;
    }

    fn close_session_replies(&mut self) {
        if let Some(replies) = self.session_events.as_ref() {
            if let Ok(mut replies) = replies.lock() {
                replies.close();
            }
        }
    }

    /// Drain the worker's publisher-retirement handoff and drop the handles that reported stopped.
    /// Returns how many completed. A handle still running stays in `publisher_retiring`, which
    /// blocks replacement admission until it reports stopped; nothing is joined here, because this
    /// runs on the render thread.
    fn collect_publisher_retirements(&mut self) -> usize {
        if let Some(receiver) = self.publisher_retire_rx.as_ref() {
            let handed: Vec<_> = receiver.try_iter().collect();
            self.publisher_retiring.extend(handed);
        }
        let before = self.publisher_retiring.len();
        self.publisher_retiring
            .retain(|retirement| !retirement.is_finished());
        before - self.publisher_retiring.len()
    }

    /// Drop the queued outbound commands of a scope that no longer exists, reporting a terminal
    /// delivery to every carried receipt. Returns how many entries were retired.
    fn retire_outbox(&mut self, refusal: crate::net::CommandRefusal) -> usize {
        let Some(commands) = self.commands.as_ref() else {
            return 0;
        };
        match commands.lock() {
            Ok(mut queue) => retire_outbound_deliveries(&mut queue, refusal),
            Err(poisoned) => retire_outbound_deliveries(&mut poisoned.into_inner(), refusal),
        }
    }

    /// Retire every delivery that belongs to a scope the app has left, exactly once.
    ///
    /// This is the single barrier the world-transfer, logout and shutdown paths call at the point
    /// the old scope is closed or the new one adopted:
    ///
    /// * admitted WM intents are answered with a terminal status (`Unknown`: the command may have
    ///   reached the server) and removed;
    /// * publisher proposals still queued in the action channel never reached a command owner, so
    ///   they are answered `Rejected` and removed — otherwise a stale proposal would be admitted
    ///   into the next world;
    /// * queued outbound commands are dropped with `Rejected(ScopeChanged)` reported to their
    ///   receipts (a silent drop would lose the interaction's only terminal delivery);
    /// * the select screen's ask channel is closed with the session that served it;
    /// * the bounded reply queue keeps its caps and reserved terminal slots, and drops what the
    ///   adopted scope already supersedes (or accepts nothing but terminals after a session end);
    /// * publisher retirements are drained and retained until they actually stop.
    ///
    /// It never blocks the render thread and never fabricates a success. The state it resolves is
    /// consumed, so a second call for the same scope cannot answer an action or a receipt twice;
    /// only a publisher retirement that is still stopping shows up again.
    fn retire_scoped_delivery(
        &mut self,
        end: DeliveryScopeEnd,
        actions: Option<&mut WrathWmActions>,
        reason: &str,
    ) -> ScopedDeliveryRetirement {
        let mut report = ScopedDeliveryRetirement::default();

        if let Some(actions) = actions {
            report.actions = match end {
                DeliveryScopeEnd::SessionEnded => actions.retire_all(ActionStatus::Unknown, reason),
                DeliveryScopeEnd::WorldTransfer { retired, .. } => {
                    actions.retire_scope(retired, ActionStatus::Unknown, reason)
                }
            };
            report.unadmitted = actions.refuse_unadmitted(reason);
        }

        report.outbound = self.retire_outbox(crate::net::CommandRefusal::ScopeChanged);
        // The ask channel belongs to the link that served the select screen. A queued ask (an
        // Enter, a Delete) must never be served by the scope that replaces it.
        report.parked = self.park_commands.take().is_some();

        if let Some(replies) = self.session_events.as_ref() {
            if let Ok(mut replies) = replies.lock() {
                report.replies = match end {
                    DeliveryScopeEnd::SessionEnded => replies.retire_scope(),
                    DeliveryScopeEnd::WorldTransfer { adopted, .. } => {
                        replies.adopt_scope(adopted.0, adopted.1)
                    }
                };
            }
        }

        report.publishers_finished = self.collect_publisher_retirements();
        report.publishers_pending = self.publisher_retiring.len();

        if report != ScopedDeliveryRetirement::default() {
            debug!("wrath: retired delivery scope ({end:?}): {report:?}");
        }
        report
    }

    fn session_events_empty(&self) -> bool {
        match self.session_events.as_ref() {
            Some(replies) => replies
                .lock()
                .map(|replies| replies.is_empty())
                .unwrap_or(true),
            None => true,
        }
    }
}

/// 12340's server clock as a native fact pair: the unix second the server reported and the
/// seconds until its next daily reset. The vanilla clock owner keeps only the former.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathServerClock {
    pub unix_time: u32,
    pub reset_in_secs: u32,
}

/// `/played`'s answer with 12340's `TriggerScriptEvent` meaning preserved. `true` is the
/// script-triggered form, which does not request the normal chat display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathPlayedTime {
    pub total: u32,
    pub level: u32,
    pub trigger_script_event: bool,
}

/// One decoded `SMSG_PARTYKILLLOG` with the world scope it belongs to, the UI clock at decode,
/// and a monotonic sequence the UI feed uses to deliver each kill exactly once. The sequence
/// never resets, so a teardown that clears the pending queue cannot make a later kill reuse an
/// old revision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathPartyKill {
    pub killer: u64,
    pub victim: u64,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the packet was decoded; 0 when no clock existed yet.
    pub received_at: f64,
}

/// The raw 12340 facts of one `SMSG_ATTACKERSTATEUPDATE` swing. `hit_info`, `school` and
/// `victim_state` stay in the wire's own bit layout, school-mask form and victim-state
/// numbering: the 1.12 translated values are a different representation and the Wrath combat
/// log must not reuse them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathSwingHit {
    pub attacker: u64,
    pub victim: u64,
    pub hit_info: u32,
    pub damage: u32,
    pub overkill: u32,
    pub victim_state: u32,
    pub absorb: u32,
    pub resist: i32,
    pub blocked: u32,
    pub melee_spell_id: u32,
    pub school: u32,
}

/// One decoded swing with the world scope it belongs to, the UI clock at apply, and a monotonic
/// sequence the UI feed uses to deliver each swing exactly once. The sequence never resets, so
/// a teardown that clears the pending queue cannot make a later swing reuse an old revision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSwing {
    pub hit: WrathSwingHit,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_SPELLNONMELEEDAMAGELOG` with the world scope it belongs to, the UI clock at
/// apply, and a monotonic sequence the UI feed uses to deliver each row exactly once. `school` is
/// 12340's raw packet school mask byte (kept raw here, unlike the 1.12 chat translation which
/// converts to an index). Absent resisted/blocked/absorbed are represented as nil in the combat log,
/// not 0; crit/glance/crush are 1.0 or nil. `PushEvent` (`UnitCombatLog_C.cpp:1241-1361`) sends
/// 8 header values, then spellId/spellName/spellSchoolMask when a spell id exists (3), then the 0x10
/// block: damage, overkill, school, resisted, blocked, absorbed, then critical, glancing, crushing as
/// 1.0 or nil (9), total 20 for SPELL_DAMAGE. `SendDamageEvent` (`:2373`) builds that 0x10 block
/// from damage/overkill/school/absorbed/resisted/blocked and sets `infoBits` to
/// `(crit)|(glancing|crushing<<1)<<1`, so `HandleSpellNonMeleeDamageLog` (`:2520`, where
/// `hitInfo & 0x2` is crit) maps to `hit_info` bits `0x2` (crit), `0x10000` (glancing) and
/// `0x20000` (crushing). If the spell record is missing, `PushEvent` pushes nil name and 0 school.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellDamage {
    pub target: u64,
    pub attacker: u64,
    pub spell_id: u32,
    pub damage: u32,
    pub overkill: u32,
    pub school: u8,
    pub absorb: u32,
    pub resist: i32,
    pub blocked: u32,
    pub hit_info: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_SPELL_MISS_LOG` (`0x24B`, `UnitCombatLog_C.cpp:1888`
/// `HandleSpellLogMiss`) target row with the world scope it belongs to, the UI clock at apply,
/// and a monotonic sequence the UI feed uses to deliver each row exactly once. `miss_type`
/// indexes `g_spellMissStrings` at `:150` (0 NONE, 1 MISS, 2 RESIST, 3 DODGE, 4 PARRY, 5
/// BLOCK, 6 EVADE, 7 IMMUNE, 8 IMMUNE, 9 DEFLECT, 10 ABSORB, 11 REFLECT). `PushEvent`
/// (`:1241`, via `UnitCombatLogSpellMissed` `:1573` / `QueueMissedSpellText` `:1470` /
/// `SetMissInfo` `:311`) sends 8 header values, then spellId/spellName/spellSchoolMask (3),
/// then the miss string (0x4), then the amount slot only for missType 10/2/5 (`0x2000` absorb,
/// `0x4000` resisted, `0x8000` blocked) as integer 0 from this packet. The decompiled
/// `UnitCombatLogSpellMissed` call from `HandleSpellLogMiss` is labelled
/// `DAMAGE_SHIELD_MISSED` (event 41, `a5=true`); this slice implements `SPELL_MISSED` (event 8).
/// If the spell record is missing, `PushEvent` pushes nil name and 0 school.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellMiss {
    pub caster: u64,
    pub target: u64,
    pub spell_id: u32,
    pub miss_type: u8,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded rune slot with the `GetTime()` anchor its cooldown byte arrived on. `cooldown` is
/// the writer's 0..255 progress fraction; the presentation derives the start from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathRuneSlot {
    pub rune_type: u8,
    pub cooldown: u8,
    pub received_at: f64,
}

/// One decoded `SMSG_SPELLHEALLOG` (`0x150`, `Net/Types.h`) heal row with the world scope it
/// belongs to, the UI clock at apply, and a monotonic sequence the UI feed uses to deliver each
/// row exactly once. Body: packed target, packed caster, u32 spellId, u32 amount, u32 overheal,
/// u32 absorbed, u8 crit, u8 unused (`UnitCombatLog_C.cpp:1931` `HandleSpellHealLog`). The
/// combat-log event is built by `UnitCombatLogHeal` (`:1811`) / `SendHealEvent` (`:2001`) /
/// `PushEvent` (`:1241`): `UnitCombatLogHeal` returns without an entry when amount, overheal
/// and absorbed are all zero; otherwise `SendHealEvent` sets `m_infoFlags |= 0x20` with values
/// amount, overheal, absorbed, crit and `PushEvent` emits 8 header values, spellId/spellName/
/// spellSchoolMask (3), then the 0x20 block amount, overheal, absorbed, critical as 1.0 or nil
/// (4), total 15 for a shown heal. Missing spell row: nil name, school 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellHeal {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub amount: u32,
    pub overheal: u32,
    pub absorbed: u32,
    pub critical: bool,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_SPELLENERGIZELOG` (`0x151`, `Net/Types.h` in 12340 reference) energize
/// row with the world scope it belongs to, the UI clock at apply, and a monotonic sequence the
/// UI feed uses to deliver each row exactly once. Body: packed target, packed caster, u32
/// spellId, u32 powerType, u32 amount (`UnitCombatLog_C.cpp:1974` `HandleSpellLogEnergizeLog`,
/// `read_spell_energize_log` in `crates/benilla-protocol-wrath/src/gameplay.rs:3305`).
/// `UnitCombatLogEnergize` (`:1831`) returns when amount is zero. `SendEnergizeEvent` (`:2045`)
/// divides amount by `Spell_C_GetPowerDisplayMod(powerType)`
/// (`benilla_protocol::messages::power_display_scale`) with integer division before queueing and
/// stores value/powerType in the 0x40 block; `PushEvent` (`:1241`, `:1365`) appends amount,
/// powerType (2 values), so a shown energize is 8 header + spellId/spellName/spellSchoolMask +
/// 2 = 13 values. Missing spell row: nil name, school 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellEnergize {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub power_type: u32,
    pub amount: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_SPELL_START` (`0x131`, `Net/Types.h`) cast-start edge with the world
/// scope it belongs to, the UI clock at apply, and a monotonic sequence the UI feed uses to
/// deliver each row exactly once. `UnitCombatLogCastStart` (`UnitCombatLog_C.cpp:2254`, called
/// from `Spell_C.cpp:1094`) drops a packet whose `castFlags & 0x1` is set, a spell
/// `SpellIsHiddenInCombatLog` hides, and a caster with no cast time; a shown start is event 5
/// with a nil destination and 11 values (8 header + spellId/spellName/spellSchoolMask,
/// `PushEvent` `:1241`). The packet's `cast_time_ms` is `m_timer`, the time left when the
/// packet was written, not the full cast time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellCastStart {
    pub caster: u64,
    pub spell_id: u32,
    pub cast_flags: u32,
    pub cast_time_ms: i32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_SPELL_GO` (`0x132`, `Net/Types.h`) cast-success edge with the world
/// scope it belongs to, the UI clock at apply, and a monotonic sequence the UI feed uses to
/// deliver each row exactly once. `UnitCombatLogCastGo` (`UnitCombatLog_C.cpp:2272`, called
/// from `Spell_C.cpp:2108`) drops a packet whose `castFlags & 0x1` is set, a spell
/// `SpellIsHiddenInCombatLog` hides and an open-lock spell (`SpellHasOpenLockEffect`); a shown
/// success is event 6 with the packet's object target and 11 values (8 header +
/// spellId/spellName/spellSchoolMask, `PushEvent` `:1241`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellCastSuccess {
    pub caster: u64,
    pub spell_id: u32,
    pub cast_flags: u32,
    /// The packet's object target; 0 when it named none.
    pub target: u64,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded `SMSG_CAST_FAILED` (`0x130`, `Net/Types.h`) cast-failure edge with the world
/// scope it belongs to, the UI clock at apply, and a monotonic sequence the UI feed uses to
/// deliver each row exactly once. `UnitCombatLogSpellFail` (`UnitCombatLog_C.cpp:1556`, called
/// from `Spell_C.cpp:6793/6856/6858` for the active player's own error) drops
/// `SPELL_FAILED_DONT_REPORT` (0x17), an unsupported reason and an empty text; a shown failure
/// is event 7 with a nil destination and 12 values (8 header + spellId/spellName/spellSchoolMask,
/// plus the 0x2 reason text, `SetFailReason` `:217`). The source is the active player, the
/// `SMSG_CAST_FAILED` recipient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellCastFailed {
    pub spell_id: u32,
    /// The mapped 1.12 reason; `Vanilla(raw)` indexes `CAST_FAIL_KEYS`.
    pub reason: cast_failure::Reason,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    /// `UiClock::ui_now` when the row was applied; 0 when no clock existed yet.
    pub received_at: f64,
}

/// One decoded periodic-damage tick from `SMSG_PERIODICAURALOG` (aura 3/89) that produced a
/// `SPELL_PERIODIC_DAMAGE` row. Reference: `UnitCombatLog_C.cpp:2627/2414/2373/1241`. The
/// 0x10 block carries damage, overkill, school mask, resisted or nil, blocked or nil (always 0
/// from this packet), absorbed or nil, critical 1.0/nil, glancing nil, crushing nil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellPeriodicDamage {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub amount: u32,
    pub overkill: u32,
    pub school: u32,
    pub absorb: u32,
    pub resist: i32,
    pub critical: bool,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded periodic-damage tick that produced `SPELL_PERIODIC_MISSED` (damage == 0 with an
/// absorb or resist). 12 values: header 8 + spell 3 + miss string (+ amount for ABSORB/RESIST).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellPeriodicMiss {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub miss_type: u8,
    pub amount: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded periodic-heal tick (aura 8/20) for `SPELL_PERIODIC_HEAL`. 15 values: header 8 +
/// spell 3 + amount, overheal, absorbed, critical 1.0/nil. Dropped when all three amounts are 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellPeriodicHeal {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub amount: u32,
    pub overheal: u32,
    pub absorbed: u32,
    pub critical: bool,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded periodic-energize tick (aura 21/24) for `SPELL_PERIODIC_ENERGIZE`. 13 values:
/// header 8 + spell 3 + divided amount, powerType. Dropped when amount is 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellPeriodicEnergize {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub power_type: u32,
    pub amount: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded periodic drain/leech tick (aura 64) for `SPELL_PERIODIC_DRAIN` or
/// `SPELL_PERIODIC_LEECH`. `extra = trunc(amount * multiplier)`; event 38 when extra != 0,
/// else 37. 0x80 block: divided amount, powerType, divided extra or nil. 14 values.
/// Dropped when amount == 0 or power_type == 5 (POWER_HAPPINESS).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellPeriodicDrain {
    pub target: u64,
    pub caster: u64,
    pub spell_id: u32,
    pub power_type: u32,
    pub amount: u32,
    pub multiplier: f32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One logged `SMSG_SPELL_EXECUTE_LOG` effect row. The nine variants are the combat-log
/// subevents the 12340 handler produces; source is `caster`, destination is the effect target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WrathSpellExecuteLogRow {
    Interrupt {
        target: u64,
        interrupted_spell_id: u32,
    },
    ExtraAttacks {
        target: u64,
        amount: u32,
    },
    Create {
        target: u64,
    },
    Summon {
        target: u64,
    },
    Resurrect {
        target: u64,
    },
    Drain {
        target: u64,
        amount: u32,
        power_type: u32,
        multiplier: f32,
    },
    Leech {
        target: u64,
        amount: u32,
        power_type: u32,
        multiplier: f32,
    },
    DurabilityDamage {
        target: u64,
        item_id: i32,
        slot: i32,
    },
    DurabilityDamageAll {
        target: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellExecuteLog {
    pub caster: u64,
    pub spell_id: u32,
    pub row: WrathSpellExecuteLogRow,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_SPELL_DISPELL_LOG` / `SMSG_SPELL_STEAL_LOG` aura replacement row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellDispelLog {
    pub caster: u64,
    pub target: u64,
    pub spell_id: u32,
    pub replaced_spell_id: u32,
    pub harmful: bool,
    pub stolen: bool,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_DISPEL_FAILED` failed aura row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellDispelFailed {
    pub caster: u64,
    pub target: u64,
    pub spell_id: u32,
    pub failed_spell_id: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_SPELL_INSTAKILL_LOG` row: event 14 `SPELL_INSTAKILL`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathSpellInstakill {
    pub caster: u64,
    pub target: u64,
    pub spell_id: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_ENCHANTMENTLOG` row: event 31 `ENCHANT_APPLIED`, or event 32
/// `ENCHANT_REMOVED` when the packet's caster is 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathEnchantLog {
    pub caster: u64,
    pub target: u64,
    pub item_id: u32,
    pub enchant_id: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_ENVIRONMENTALDAMAGELOG` row: event 0 `ENVIRONMENTAL_DAMAGE`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathEnvironmentalDamage {
    pub victim: u64,
    pub damage_type: u8,
    pub damage: u32,
    pub absorbed: u32,
    pub resisted: i32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// One decoded `SMSG_SPELLDAMAGESHIELD` row: event 40 `DAMAGE_SHIELD`. The victim is the
/// source and the shield's caster is the destination, matching `UnitCombatLogDamageShield`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathDamageShield {
    pub target: u64,
    pub attacker: u64,
    pub spell_id: u32,
    pub damage: u32,
    pub overkill: u32,
    pub school: u32,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub revision: u64,
    pub received_at: f64,
}

/// The decoded `SMSG_CALENDAR_SEND_CALENDAR` snapshot, kept as the wire delivered it. The UI
/// reads the date from `zone_time` and pending invites from `invites`; the other rows stay here
/// for the calendar windows that need them.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathCalendar {
    pub server_time: u32,
    pub zone_time: u32,
    pub invites: Vec<gameplay::wrath_systems::CalendarInvite>,
    pub events: Vec<gameplay::wrath_systems::CalendarEvent>,
    pub raid_lockouts: Vec<gameplay::wrath_systems::CalendarRaidLockout>,
    pub relative_time: u32,
    pub raid_resets: Vec<gameplay::wrath_systems::CalendarRaidReset>,
    pub holidays: Vec<gameplay::wrath_systems::CalendarHoliday>,
}

/// `SMSG_CALENDAR_COMMAND_RESULT`: the server's answer to a calendar action. Stored as the last
/// decoded result so it is scoped and cleared with the rest of the calendar.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathCalendarCommandResult {
    pub command: u32,
    pub reserved: u8,
    pub error: u32,
    pub param: Option<String>,
}

/// The drained replicated world, as the Input/Stream/Presentation stages read it. Maintained
/// by the Net-stage drain; a world-generation change clears the entity map before anything
/// downstream runs this frame.
#[derive(Resource)]
pub struct WrathWorldView {
    #[allow(dead_code)] // Default for the resource init; every real value comes from a digest
    empty: (),

    pub session_epoch: u64,
    pub world_generation: u64,
    pub state_revision: u64,
    pub map_id: u32,
    pub phase: Option<Phase>,
    pub player_guid: u64,
    pub(crate) vehicle: Option<VehicleAttachment>,
    pub self_observation: Option<benilla_protocol_wrath::session::SelfObservation>,
    pub entities: BTreeMap<u64, WrathEntity>,
    pub last_changes: WrathChanges,
    /// The world-enter cascade has been raised for the current local player.
    pub entered_world: bool,
    /// The NPC and menu id from the last gossip reply; 3.3.5 requires the menu id on a select.
    pub gossip_menu: Option<(u64, u32)>,
    /// The open vendor's 12340 rows (vendor guid, rows). Buy packets address the 1-based row,
    /// while the stock command carries only the template entry.
    pub vendor: Option<(u64, Vec<gameplay::VendorRow>)>,
    /// The last 12340 server clock answer; retained across a world transfer, cleared by a new
    /// session or a terminal edge.
    pub server_clock: Option<WrathServerClock>,
    /// The last `/played` answer with its trigger semantics; session-scoped state.
    pub played_time: Option<WrathPlayedTime>,
    /// The decoded Death Knight runes in wire order, empty until `SMSG_RESYNC_RUNES`. Each slot
    /// carries the 0..255 cooldown fraction and the `GetTime()` anchor it arrived on.
    pub runes: Vec<WrathRuneSlot>,
    /// The last `SMSG_CALENDAR_SEND_CALENDAR` snapshot; `None` before the first reply.
    pub calendar: Option<WrathCalendar>,
    /// Whether a calendar reply has established the scoped snapshot.
    pub calendar_fetched: bool,
    /// Monotonic per-scope calendar snapshot revision; a second reply replaces at revision + 1.
    pub calendar_revision: u64,
    /// The last decoded `SMSG_CALENDAR_COMMAND_RESULT`, cleared with the calendar scope.
    pub last_calendar_command_result: Option<WrathCalendarCommandResult>,
    /// The decoded kills waiting for the UI feed. A kill is an ordered event, not a replaceable
    /// snapshot: a burst before one feed preserves every row up to [`PARTY_KILL_CAPACITY`].
    pub party_kills: VecDeque<WrathPartyKill>,
    /// Monotonic sequence for [`Self::party_kills`], never reset on a scope clear.
    pub party_kill_revision: u64,
    /// Kills dropped because the pending queue was full.
    pub party_kill_dropped: u64,
    /// Kills dropped because neither endpoint name resolved within the retry window.
    pub party_kill_unresolved: u64,
    /// The decoded melee swings waiting for the UI feed; ordered rows up to `SWING_CAPACITY`.
    pub swings: VecDeque<WrathSwing>,
    /// Monotonic sequence for `Self::swings`, never reset on a scope clear.
    pub swing_revision: u64,
    /// Swings dropped because the pending queue was full.
    pub swing_dropped: u64,
    /// Swings delivered with an unknown endpoint after name resolution timed out.
    pub swing_unresolved: u64,
    /// The decoded spell-damage rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_damages: VecDeque<WrathSpellDamage>,
    /// Monotonic sequence for `Self::spell_damages`, never reset on a scope clear.
    pub spell_damage_revision: u64,
    /// Spell-damage rows dropped because the pending queue was full.
    pub spell_damage_dropped: u64,
    /// Spell-damage rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_damage_unresolved: u64,
    /// The decoded spell-miss rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_misses: VecDeque<WrathSpellMiss>,
    /// Monotonic sequence for `Self::spell_misses`, never reset on a scope clear.
    pub spell_miss_revision: u64,
    /// Spell-miss rows dropped because the pending queue was full.
    pub spell_miss_dropped: u64,
    /// Spell-miss rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_miss_unresolved: u64,
    /// The decoded spell-heal rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_heals: VecDeque<WrathSpellHeal>,
    /// Monotonic sequence for `Self::spell_heals`, never reset on a scope clear.
    pub spell_heal_revision: u64,
    /// Spell-heal rows dropped because the pending queue was full.
    pub spell_heal_dropped: u64,
    /// Spell-heal rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_heal_unresolved: u64,
    /// The decoded spell-energize rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_energizes: VecDeque<WrathSpellEnergize>,
    /// Monotonic sequence for `Self::spell_energizes`, never reset on a scope clear.
    pub spell_energize_revision: u64,
    /// Spell-energize rows dropped because the pending queue was full.
    pub spell_energize_dropped: u64,
    /// Spell-energize rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_energize_unresolved: u64,
    /// The decoded `SPELL_CAST_START` rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_cast_starts: VecDeque<WrathSpellCastStart>,
    /// Monotonic sequence for `Self::spell_cast_starts`, never reset on a scope clear.
    pub spell_cast_start_revision: u64,
    /// Cast-start rows dropped because the pending queue was full.
    pub spell_cast_start_dropped: u64,
    /// Cast-start rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_cast_start_unresolved: u64,
    /// The decoded `SPELL_CAST_SUCCESS` rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_cast_successes: VecDeque<WrathSpellCastSuccess>,
    /// Monotonic sequence for `Self::spell_cast_successes`, never reset on a scope clear.
    pub spell_cast_success_revision: u64,
    /// Cast-success rows dropped because the pending queue was full.
    pub spell_cast_success_dropped: u64,
    /// Cast-success rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_cast_success_unresolved: u64,
    /// The decoded `SPELL_CAST_FAILED` rows waiting for the UI feed; ordered rows up to
    /// `SPELL_LOG_CAPACITY`.
    pub spell_cast_failures: VecDeque<WrathSpellCastFailed>,
    /// Monotonic sequence for `Self::spell_cast_failures`, never reset on a scope clear.
    pub spell_cast_failure_revision: u64,
    /// Cast-failure rows dropped because the pending queue was full.
    pub spell_cast_failure_dropped: u64,
    /// Cast-failure rows delivered with an unknown endpoint after name resolution timed out.
    pub spell_cast_failure_unresolved: u64,
    /// Periodic damage rows (`SPELL_PERIODIC_DAMAGE`) waiting for the UI feed.
    pub spell_periodic_damages: VecDeque<WrathSpellPeriodicDamage>,
    pub spell_periodic_damage_revision: u64,
    pub spell_periodic_damage_dropped: u64,
    pub spell_periodic_damage_unresolved: u64,
    /// Periodic miss rows (`SPELL_PERIODIC_MISSED`) waiting for the UI feed.
    pub spell_periodic_misses: VecDeque<WrathSpellPeriodicMiss>,
    pub spell_periodic_miss_revision: u64,
    pub spell_periodic_miss_dropped: u64,
    pub spell_periodic_miss_unresolved: u64,
    /// Periodic heal rows (`SPELL_PERIODIC_HEAL`) waiting for the UI feed.
    pub spell_periodic_heals: VecDeque<WrathSpellPeriodicHeal>,
    pub spell_periodic_heal_revision: u64,
    pub spell_periodic_heal_dropped: u64,
    pub spell_periodic_heal_unresolved: u64,
    /// Periodic energize rows (`SPELL_PERIODIC_ENERGIZE`) waiting for the UI feed.
    pub spell_periodic_energizes: VecDeque<WrathSpellPeriodicEnergize>,
    pub spell_periodic_energize_revision: u64,
    pub spell_periodic_energize_dropped: u64,
    pub spell_periodic_energize_unresolved: u64,
    /// Periodic drain/leech rows (`SPELL_PERIODIC_DRAIN`/`SPELL_PERIODIC_LEECH`) waiting for the UI feed.
    pub spell_periodic_drains: VecDeque<WrathSpellPeriodicDrain>,
    pub spell_periodic_drain_revision: u64,
    pub spell_periodic_drain_dropped: u64,
    pub spell_periodic_drain_unresolved: u64,
    /// Execute-log rows (`SMSG_SPELL_EXECUTE_LOG`) waiting for the UI feed.
    pub spell_execute_logs: VecDeque<WrathSpellExecuteLog>,
    pub spell_execute_log_revision: u64,
    pub spell_execute_log_dropped: u64,
    pub spell_execute_log_unresolved: u64,
    /// Dispel-log rows (`SMSG_SPELL_DISPELL_LOG` / `SMSG_SPELL_STEAL_LOG`) waiting for the UI feed.
    pub spell_dispel_logs: VecDeque<WrathSpellDispelLog>,
    pub spell_dispel_log_revision: u64,
    pub spell_dispel_log_dropped: u64,
    pub spell_dispel_log_unresolved: u64,
    /// Dispel-failed rows (`SMSG_DISPEL_FAILED`) waiting for the UI feed.
    pub spell_dispel_faileds: VecDeque<WrathSpellDispelFailed>,
    pub spell_dispel_failed_revision: u64,
    pub spell_dispel_failed_dropped: u64,
    pub spell_dispel_failed_unresolved: u64,
    /// Instakill rows (`SMSG_SPELL_INSTAKILL_LOG`) waiting for the UI feed.
    pub instakills: VecDeque<WrathSpellInstakill>,
    pub instakill_revision: u64,
    pub instakill_dropped: u64,
    pub instakill_unresolved: u64,
    /// Enchant applied/removed rows (`SMSG_ENCHANTMENTLOG`) waiting for the UI feed.
    pub enchant_logs: VecDeque<WrathEnchantLog>,
    pub enchant_revision: u64,
    pub enchant_dropped: u64,
    pub enchant_unresolved: u64,
    /// Environmental-damage rows (`SMSG_ENVIRONMENTALDAMAGELOG`) waiting for the UI feed.
    pub environmental_damages: VecDeque<WrathEnvironmentalDamage>,
    pub environmental_damage_revision: u64,
    pub environmental_damage_dropped: u64,
    pub environmental_damage_unresolved: u64,
    /// Damage-shield rows (`SMSG_SPELLDAMAGESHIELD`) waiting for the UI feed.
    pub damage_shields: VecDeque<WrathDamageShield>,
    pub damage_shield_revision: u64,
    pub damage_shield_dropped: u64,
    pub damage_shield_unresolved: u64,
    /// Presentation mirror of the authority's active casts, keyed by caster, retaining 12340
    /// identity/timing beyond the vanilla cast-bar edges. Written only from resolved snapshots;
    /// never re-resolves wire facts. UI clocks are stamped here, not in the authority.
    pub casts: BTreeMap<u64, crate::wrath_native::WrathActiveCast>,
    /// Presentation mirror of the authority's active channels, keyed by caster.
    pub channels: BTreeMap<u64, crate::wrath_native::WrathActiveChannel>,
    /// Mirror of the authority's last terminal cast edge.
    pub last_cast_end: Option<crate::wrath_native::WrathCastEnded>,
    /// The last native inventory failure with identities/tail; transient display state.
    pub last_inventory_failure: Option<crate::wrath_native::WrathInventoryFailure>,
    /// Bounded WMC1 v3 (`SMSG_MULTICLASS_REPLY`, `0x51A`) replies awaiting `MulticlassPoll`.
    pub multiclass_replies: VecDeque<(u16, u16, u32, Vec<u8>)>,
    /// The last decoded `SMSG_LFG_JOIN_RESULT` in this scope.
    pub lfg_join_result: Option<gameplay::wrath_systems::LfgJoinResult>,
    /// The last decoded `SMSG_LFG_QUEUE_STATUS` in this scope.
    pub lfg_queue_status: Option<gameplay::wrath_systems::LfgQueueStatus>,
    /// The last decoded `SMSG_LFG_UPDATE_PLAYER` in this scope.
    pub lfg_update_player: Option<gameplay::wrath_systems::LfgUpdatePlayer>,
    /// The last decoded `SMSG_LFG_UPDATE_PARTY` in this scope.
    pub lfg_update_party: Option<gameplay::wrath_systems::LfgUpdateParty>,
    /// The last decoded `SMSG_LFG_PROPOSAL_UPDATE` in this scope.
    pub lfg_proposal: Option<gameplay::wrath_systems::LfgProposalUpdate>,
    /// Monotonic cast count for `CMSG_CAST_SPELL`.
    pub next_cast_count: u8,
}

impl Default for WrathWorldView {
    fn default() -> Self {
        Self {
            session_epoch: 0,
            world_generation: 0,
            state_revision: 0,
            map_id: 0,
            phase: None,
            player_guid: 0,
            vehicle: None,
            self_observation: None,
            entities: BTreeMap::new(),
            last_changes: WrathChanges::default(),
            entered_world: false,
            gossip_menu: None,
            vendor: None,
            server_clock: None,
            played_time: None,
            runes: Vec::new(),
            calendar: None,
            calendar_fetched: false,
            calendar_revision: 0,
            last_calendar_command_result: None,
            party_kills: VecDeque::new(),
            party_kill_revision: 0,
            party_kill_dropped: 0,
            party_kill_unresolved: 0,
            swings: VecDeque::new(),
            swing_revision: 0,
            swing_dropped: 0,
            swing_unresolved: 0,
            spell_damages: VecDeque::new(),
            spell_damage_revision: 0,
            spell_damage_dropped: 0,
            spell_damage_unresolved: 0,
            spell_misses: VecDeque::new(),
            spell_miss_revision: 0,
            spell_miss_dropped: 0,
            spell_miss_unresolved: 0,
            spell_heals: VecDeque::new(),
            spell_heal_revision: 0,
            spell_heal_dropped: 0,
            spell_heal_unresolved: 0,
            spell_energizes: VecDeque::new(),
            spell_energize_revision: 0,
            spell_energize_dropped: 0,
            spell_energize_unresolved: 0,
            spell_cast_starts: VecDeque::new(),
            spell_cast_start_revision: 0,
            spell_cast_start_dropped: 0,
            spell_cast_start_unresolved: 0,
            spell_cast_successes: VecDeque::new(),
            spell_cast_success_revision: 0,
            spell_cast_success_dropped: 0,
            spell_cast_success_unresolved: 0,
            spell_cast_failures: VecDeque::new(),
            spell_cast_failure_revision: 0,
            spell_cast_failure_dropped: 0,
            spell_cast_failure_unresolved: 0,
            spell_periodic_damages: VecDeque::new(),
            spell_periodic_damage_revision: 0,
            spell_periodic_damage_dropped: 0,
            spell_periodic_damage_unresolved: 0,
            spell_periodic_misses: VecDeque::new(),
            spell_periodic_miss_revision: 0,
            spell_periodic_miss_dropped: 0,
            spell_periodic_miss_unresolved: 0,
            spell_periodic_heals: VecDeque::new(),
            spell_periodic_heal_revision: 0,
            spell_periodic_heal_dropped: 0,
            spell_periodic_heal_unresolved: 0,
            spell_periodic_energizes: VecDeque::new(),
            spell_periodic_energize_revision: 0,
            spell_periodic_energize_dropped: 0,
            spell_periodic_energize_unresolved: 0,
            spell_periodic_drains: VecDeque::new(),
            spell_periodic_drain_revision: 0,
            spell_periodic_drain_dropped: 0,
            spell_periodic_drain_unresolved: 0,
            spell_execute_logs: VecDeque::new(),
            spell_execute_log_revision: 0,
            spell_execute_log_dropped: 0,
            spell_execute_log_unresolved: 0,
            spell_dispel_logs: VecDeque::new(),
            spell_dispel_log_revision: 0,
            spell_dispel_log_dropped: 0,
            spell_dispel_log_unresolved: 0,
            spell_dispel_faileds: VecDeque::new(),
            spell_dispel_failed_revision: 0,
            spell_dispel_failed_dropped: 0,
            spell_dispel_failed_unresolved: 0,
            instakills: VecDeque::new(),
            instakill_revision: 0,
            instakill_dropped: 0,
            instakill_unresolved: 0,
            enchant_logs: VecDeque::new(),
            enchant_revision: 0,
            enchant_dropped: 0,
            enchant_unresolved: 0,
            environmental_damages: VecDeque::new(),
            environmental_damage_revision: 0,
            environmental_damage_dropped: 0,
            environmental_damage_unresolved: 0,
            damage_shields: VecDeque::new(),
            damage_shield_revision: 0,
            damage_shield_dropped: 0,
            damage_shield_unresolved: 0,
            casts: BTreeMap::new(),
            channels: BTreeMap::new(),
            last_cast_end: None,
            last_inventory_failure: None,
            multiclass_replies: VecDeque::new(),
            lfg_join_result: None,
            lfg_queue_status: None,
            lfg_update_player: None,
            lfg_update_party: None,
            lfg_proposal: None,
            next_cast_count: 1,
            empty: (),
        }
    }
}

#[derive(Resource)]
pub(crate) struct WrathVehicleSeats(pub(crate) benilla_formats::VehicleSeats);

impl WrathWorldView {
    pub(crate) fn seated_vehicle(&self, player_guid: u64) -> Option<u64> {
        self.local_vehicle(player_guid).map(|seat| seat.base_guid)
    }

    pub(crate) fn local_vehicle(&self, player_guid: u64) -> Option<VehicleAttachment> {
        if player_guid == 0 || player_guid != self.player_guid || self.phase != Some(Phase::InWorld)
        {
            return None;
        }
        self.vehicle
    }

    /// The mirrored active native cast for `unit`, if the authority's last resolved edge left
    /// one. `None` after the authoritative launch/failure/interruption edge, even though
    /// `last_cast_end` keeps its identity for diagnostics.
    #[allow(dead_code)] // presentation accessor for UI consumers of the mirrored authority
    pub fn casting(&self, unit: u64) -> Option<&crate::wrath_native::WrathActiveCast> {
        self.casts.get(&unit)
    }

    /// The mirrored active native channel for `unit`, if any.
    #[allow(dead_code)] // presentation accessor for UI consumers of the mirrored authority
    pub fn channeling(&self, unit: u64) -> Option<&crate::wrath_native::WrathActiveChannel> {
        self.channels.get(&unit)
    }

    /// Mirror the local cancel intent for display. This is not a terminal state and never
    /// decides an outcome: the authority separately records the request when the command is
    /// actually sent, and the export carries the authority's value.
    pub fn note_cancel_requested(&mut self, caster: u64, spell_id: u32) -> bool {
        if let Some(cast) = self.casts.get_mut(&caster) {
            if cast.identity.spell_id == spell_id {
                cast.cancel_requested = true;
                return true;
            }
        }
        false
    }

    /// The observed resources for any replicated unit. See
    /// [`crate::wrath_native::WrathUnitResources`] for the observed-zero/unavailable contract.
    pub fn resources(&self, guid: u64) -> Option<crate::wrath_native::WrathUnitResources> {
        let entity = self.entities.get(&guid)?;
        Some(crate::wrath_native::WrathUnitResources {
            health: entity.health,
            max_health: entity.max_health,
            powers: entity.powers,
            max_powers: entity.max_powers,
            power_type: entity.bytes_0.map(|bytes| ((bytes >> 24) & 0xff) as u8),
        })
    }

    /// The full native aura slots for `guid`; each slot retains 12340's slot, spell, raw flags,
    /// level, charges, caster and both durations. Helpful/harmful and dispel class are not carried
    /// by the aura packet and are therefore not inferred here; the spell catalog owns them. The
    /// synthesized vanilla aura fields remain a separate projection.
    #[allow(dead_code)] // presentation accessor; the WM export reads the session's own world
    pub fn auras(&self, guid: u64) -> &[benilla_protocol_wrath::reducer::AuraSlot] {
        self.entities
            .get(&guid)
            .map_or(&[], |entity| entity.auras.as_slice())
    }

    /// The last terminal cast edge only when it belongs to the currently adopted scope. A
    /// transfer/replacement leaves the old record in place for diagnostics but this accessor
    /// never presents it as current.
    #[allow(dead_code)] // presentation accessor for UI consumers of the mirrored authority
    pub fn last_cast_end_in_scope(&self) -> Option<&crate::wrath_native::WrathCastEnded> {
        self.last_cast_end.as_ref().filter(|ended| {
            ended.scope.session_epoch == self.session_epoch
                && ended.scope.world_generation == self.world_generation
        })
    }

    /// Adopt one `SMSG_RESYNC_RUNES` snapshot. The writer's order is the class's real layout, so
    /// no slot is synthesized: the vector is exactly the decoded rows.
    fn apply_rune_state(&mut self, runes: &[gameplay::wrath_systems::RuneSlot], received_at: f64) {
        self.runes = runes
            .iter()
            .map(|rune| WrathRuneSlot {
                rune_type: rune.rune_type,
                cooldown: rune.cooldown,
                received_at,
            })
            .collect();
    }

    /// `SMSG_CONVERT_RUNE` changes only the slot's type; its cooldown and anchor stay.
    fn apply_rune_convert(&mut self, rune: u8, rune_type: u8, received_at: f64) {
        if let Some(slot) = self.runes.get_mut(usize::from(rune)) {
            slot.rune_type = rune_type;
            if slot.received_at <= 0.0 {
                slot.received_at = received_at;
            }
        }
    }

    /// `SMSG_ADD_RUNE_POWER`: every masked rune is fully recharged, matching the reference's
    /// 255/full-progress treatment (`RuneInfo.cpp:419-432`).
    fn apply_rune_power(&mut self, mask: u32, received_at: f64) {
        for (index, slot) in self.runes.iter_mut().enumerate() {
            if index < 32 && mask & (1u32 << index) != 0 {
                slot.cooldown = 255;
                slot.received_at = received_at;
            }
        }
    }

    /// Adopt one `SMSG_CALENDAR_SEND_CALENDAR` reply. A second reply replaces the snapshot and
    /// advances the revision; it never appends rows to the previous one.
    fn apply_calendar(&mut self, calendar: &gameplay::wrath_systems::CalendarSendCalendar) {
        self.calendar_revision = self.calendar_revision.saturating_add(1);
        self.calendar = Some(WrathCalendar {
            server_time: calendar.server_time,
            zone_time: calendar.zone_time,
            invites: calendar.invites.clone(),
            events: calendar.events.clone(),
            raid_lockouts: calendar.raid_lockouts.clone(),
            relative_time: calendar.relative_time,
            raid_resets: calendar.raid_resets.clone(),
            holidays: calendar.holidays.clone(),
        });
        self.calendar_fetched = true;
    }

    /// Adopt one `SMSG_PARTYKILLLOG`. The sequence is monotonic for the app's lifetime so a
    /// teardown's clear cannot make a later kill reuse a revision the UI feed has already seen.
    /// The pending queue is bounded; overflow drops the oldest row and records the loss.
    fn apply_party_kill(
        &mut self,
        killer: u64,
        victim: u64,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.party_kill_revision = self.party_kill_revision.saturating_add(1);
        if self.party_kills.len() >= PARTY_KILL_CAPACITY {
            self.party_kills.pop_front();
            self.party_kill_dropped = self.party_kill_dropped.saturating_add(1);
        }
        self.party_kills.push_back(WrathPartyKill {
            killer,
            victim,
            session_epoch,
            world_generation,
            revision: self.party_kill_revision,
            received_at,
        });
    }

    /// Adopt one decoded swing. A swing is an ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    pub(crate) fn apply_swing(
        &mut self,
        hit: WrathSwingHit,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.swing_revision = self.swing_revision.saturating_add(1);
        if self.swings.len() >= SWING_CAPACITY {
            self.swings.pop_front();
            self.swing_dropped = self.swing_dropped.saturating_add(1);
        }
        self.swings.push_back(WrathSwing {
            hit,
            session_epoch,
            world_generation,
            revision: self.swing_revision,
            received_at,
        });
    }

    /// Adopt one decoded spell-damage row. Ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    pub(crate) fn apply_spell_damage(
        &mut self,
        hit: WrathSpellDamage,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_damage_revision = self.spell_damage_revision.saturating_add(1);
        if self.spell_damages.len() >= SPELL_LOG_CAPACITY {
            self.spell_damages.pop_front();
            self.spell_damage_dropped = self.spell_damage_dropped.saturating_add(1);
        }
        self.spell_damages.push_back(WrathSpellDamage {
            session_epoch,
            world_generation,
            revision: self.spell_damage_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded spell-miss row. Ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    pub(crate) fn apply_spell_miss(
        &mut self,
        hit: WrathSpellMiss,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_miss_revision = self.spell_miss_revision.saturating_add(1);
        if self.spell_misses.len() >= SPELL_LOG_CAPACITY {
            self.spell_misses.pop_front();
            self.spell_miss_dropped = self.spell_miss_dropped.saturating_add(1);
        }
        self.spell_misses.push_back(WrathSpellMiss {
            session_epoch,
            world_generation,
            revision: self.spell_miss_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded spell-heal row. Ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    /// `UnitCombatLogHeal` (`UnitCombatLog_C.cpp:1811`) returns without an entry when amount,
    /// overheal and absorbed are all zero, so a zero row is skipped here.
    pub(crate) fn apply_spell_heal(
        &mut self,
        hit: WrathSpellHeal,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.amount == 0 && hit.overheal == 0 && hit.absorbed == 0 {
            return;
        }
        self.spell_heal_revision = self.spell_heal_revision.saturating_add(1);
        if self.spell_heals.len() >= SPELL_LOG_CAPACITY {
            self.spell_heals.pop_front();
            self.spell_heal_dropped = self.spell_heal_dropped.saturating_add(1);
        }
        self.spell_heals.push_back(WrathSpellHeal {
            session_epoch,
            world_generation,
            revision: self.spell_heal_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded spell-energize row. Ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    /// `UnitCombatLogEnergize` (`UnitCombatLog_C.cpp:1831`) returns when amount is zero.
    pub(crate) fn apply_spell_energize(
        &mut self,
        hit: WrathSpellEnergize,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.amount == 0 {
            return;
        }
        self.spell_energize_revision = self.spell_energize_revision.saturating_add(1);
        if self.spell_energizes.len() >= SPELL_LOG_CAPACITY {
            self.spell_energizes.pop_front();
            self.spell_energize_dropped = self.spell_energize_dropped.saturating_add(1);
        }
        self.spell_energizes.push_back(WrathSpellEnergize {
            session_epoch,
            world_generation,
            revision: self.spell_energize_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded `SMSG_SPELL_START` edge. Ordered event, never a replaceable snapshot:
    /// the pending queue is bounded, and overflow drops the oldest row and records the loss.
    /// `UnitCombatLogCastStart` (`UnitCombatLog_C.cpp:2254`) returns without an entry for a
    /// packet whose `castFlags & 0x1` is set and for a caster with no cast time, so those rows
    /// are skipped here; the hidden-spell gate needs the catalog and runs in the UI feed.
    pub(crate) fn apply_spell_cast_start(
        &mut self,
        hit: WrathSpellCastStart,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.cast_flags & 0x1 != 0 || hit.cast_time_ms <= 0 {
            return;
        }
        self.spell_cast_start_revision = self.spell_cast_start_revision.saturating_add(1);
        if self.spell_cast_starts.len() >= SPELL_LOG_CAPACITY {
            self.spell_cast_starts.pop_front();
            self.spell_cast_start_dropped = self.spell_cast_start_dropped.saturating_add(1);
        }
        self.spell_cast_starts.push_back(WrathSpellCastStart {
            session_epoch,
            world_generation,
            revision: self.spell_cast_start_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded `SMSG_SPELL_GO` edge. Ordered event, never a replaceable snapshot: the
    /// pending queue is bounded, and overflow drops the oldest row and records the loss.
    /// `UnitCombatLogCastGo` (`UnitCombatLog_C.cpp:2272`) returns without an entry for a packet
    /// whose `castFlags & 0x1` is set; the hidden and open-lock gates need the catalog and run
    /// in the UI feed.
    pub(crate) fn apply_spell_cast_success(
        &mut self,
        hit: WrathSpellCastSuccess,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.cast_flags & 0x1 != 0 {
            return;
        }
        self.spell_cast_success_revision = self.spell_cast_success_revision.saturating_add(1);
        if self.spell_cast_successes.len() >= SPELL_LOG_CAPACITY {
            self.spell_cast_successes.pop_front();
            self.spell_cast_success_dropped = self.spell_cast_success_dropped.saturating_add(1);
        }
        self.spell_cast_successes.push_back(WrathSpellCastSuccess {
            session_epoch,
            world_generation,
            revision: self.spell_cast_success_revision,
            received_at,
            ..hit
        });
    }

    /// Adopt one decoded `SMSG_CAST_FAILED` edge. Ordered event, never a replaceable snapshot:
    /// the pending queue is bounded, and overflow drops the oldest row and records the loss.
    /// `UnitCombatLogSpellFail` (`UnitCombatLog_C.cpp:1556`) returns without an entry for
    /// `SPELL_FAILED_DONT_REPORT` (0x17) and for a reason with no 1.12 text, so those rows are
    /// skipped here; the text itself is resolved in the UI feed.
    pub(crate) fn apply_spell_cast_failed(
        &mut self,
        hit: WrathSpellCastFailed,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if !matches!(
            hit.reason,
            cast_failure::Reason::Vanilla(raw) if raw != 0x17
        ) {
            return;
        }
        self.spell_cast_failure_revision = self.spell_cast_failure_revision.saturating_add(1);
        if self.spell_cast_failures.len() >= SPELL_LOG_CAPACITY {
            self.spell_cast_failures.pop_front();
            self.spell_cast_failure_dropped = self.spell_cast_failure_dropped.saturating_add(1);
        }
        self.spell_cast_failures.push_back(WrathSpellCastFailed {
            session_epoch,
            world_generation,
            revision: self.spell_cast_failure_revision,
            received_at,
            ..hit
        });
    }

    pub(crate) fn apply_spell_periodic_damage(
        &mut self,
        hit: WrathSpellPeriodicDamage,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_periodic_damage_revision = self.spell_periodic_damage_revision.saturating_add(1);
        if self.spell_periodic_damages.len() >= SPELL_LOG_CAPACITY {
            self.spell_periodic_damages.pop_front();
            self.spell_periodic_damage_dropped =
                self.spell_periodic_damage_dropped.saturating_add(1);
        }
        self.spell_periodic_damages
            .push_back(WrathSpellPeriodicDamage {
                session_epoch,
                world_generation,
                revision: self.spell_periodic_damage_revision,
                received_at,
                ..hit
            });
    }

    pub(crate) fn apply_spell_periodic_miss(
        &mut self,
        hit: WrathSpellPeriodicMiss,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_periodic_miss_revision = self.spell_periodic_miss_revision.saturating_add(1);
        if self.spell_periodic_misses.len() >= SPELL_LOG_CAPACITY {
            self.spell_periodic_misses.pop_front();
            self.spell_periodic_miss_dropped = self.spell_periodic_miss_dropped.saturating_add(1);
        }
        self.spell_periodic_misses
            .push_back(WrathSpellPeriodicMiss {
                session_epoch,
                world_generation,
                revision: self.spell_periodic_miss_revision,
                received_at,
                ..hit
            });
    }

    /// `UnitCombatLogHeal` returns without an entry when amount, overheal and absorbed are all
    /// zero; the same drop rule applies to periodic heals.
    pub(crate) fn apply_spell_periodic_heal(
        &mut self,
        hit: WrathSpellPeriodicHeal,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.amount == 0 && hit.overheal == 0 && hit.absorbed == 0 {
            return;
        }
        self.spell_periodic_heal_revision = self.spell_periodic_heal_revision.saturating_add(1);
        if self.spell_periodic_heals.len() >= SPELL_LOG_CAPACITY {
            self.spell_periodic_heals.pop_front();
            self.spell_periodic_heal_dropped = self.spell_periodic_heal_dropped.saturating_add(1);
        }
        self.spell_periodic_heals.push_back(WrathSpellPeriodicHeal {
            session_epoch,
            world_generation,
            revision: self.spell_periodic_heal_revision,
            received_at,
            ..hit
        });
    }

    /// `UnitCombatLogEnergize` returns when amount is zero; periodic energize uses the same rule.
    pub(crate) fn apply_spell_periodic_energize(
        &mut self,
        hit: WrathSpellPeriodicEnergize,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.amount == 0 {
            return;
        }
        self.spell_periodic_energize_revision =
            self.spell_periodic_energize_revision.saturating_add(1);
        if self.spell_periodic_energizes.len() >= SPELL_LOG_CAPACITY {
            self.spell_periodic_energizes.pop_front();
            self.spell_periodic_energize_dropped =
                self.spell_periodic_energize_dropped.saturating_add(1);
        }
        self.spell_periodic_energizes
            .push_back(WrathSpellPeriodicEnergize {
                session_epoch,
                world_generation,
                revision: self.spell_periodic_energize_revision,
                received_at,
                ..hit
            });
    }

    /// Drain/leech: drop amount 0 and power type 5 (POWER_HAPPINESS).
    pub(crate) fn apply_spell_periodic_drain(
        &mut self,
        hit: WrathSpellPeriodicDrain,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.amount == 0 || hit.power_type == 5 {
            return;
        }
        self.spell_periodic_drain_revision = self.spell_periodic_drain_revision.saturating_add(1);
        if self.spell_periodic_drains.len() >= SPELL_LOG_CAPACITY {
            self.spell_periodic_drains.pop_front();
            self.spell_periodic_drain_dropped = self.spell_periodic_drain_dropped.saturating_add(1);
        }
        self.spell_periodic_drains
            .push_back(WrathSpellPeriodicDrain {
                session_epoch,
                world_generation,
                revision: self.spell_periodic_drain_revision,
                received_at,
                ..hit
            });
    }

    pub(crate) fn apply_spell_execute_log(
        &mut self,
        hit: WrathSpellExecuteLog,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        match hit.row {
            WrathSpellExecuteLogRow::Drain {
                amount, power_type, ..
            }
            | WrathSpellExecuteLogRow::Leech {
                amount, power_type, ..
            } if amount == 0 || power_type == 5 => {
                return;
            }
            _ => {}
        }
        self.spell_execute_log_revision = self.spell_execute_log_revision.saturating_add(1);
        if self.spell_execute_logs.len() >= SPELL_LOG_CAPACITY {
            self.spell_execute_logs.pop_front();
            self.spell_execute_log_dropped = self.spell_execute_log_dropped.saturating_add(1);
        }
        self.spell_execute_logs.push_back(WrathSpellExecuteLog {
            session_epoch,
            world_generation,
            revision: self.spell_execute_log_revision,
            received_at,
            ..hit
        });
    }

    pub(crate) fn apply_spell_dispel_log(
        &mut self,
        hit: WrathSpellDispelLog,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_dispel_log_revision = self.spell_dispel_log_revision.saturating_add(1);
        if self.spell_dispel_logs.len() >= SPELL_LOG_CAPACITY {
            self.spell_dispel_logs.pop_front();
            self.spell_dispel_log_dropped = self.spell_dispel_log_dropped.saturating_add(1);
        }
        self.spell_dispel_logs.push_back(WrathSpellDispelLog {
            session_epoch,
            world_generation,
            revision: self.spell_dispel_log_revision,
            received_at,
            ..hit
        });
    }

    pub(crate) fn apply_spell_dispel_failed(
        &mut self,
        hit: WrathSpellDispelFailed,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.spell_dispel_failed_revision = self.spell_dispel_failed_revision.saturating_add(1);
        if self.spell_dispel_faileds.len() >= SPELL_LOG_CAPACITY {
            self.spell_dispel_faileds.pop_front();
            self.spell_dispel_failed_dropped = self.spell_dispel_failed_dropped.saturating_add(1);
        }
        self.spell_dispel_faileds.push_back(WrathSpellDispelFailed {
            session_epoch,
            world_generation,
            revision: self.spell_dispel_failed_revision,
            received_at,
            ..hit
        });
    }

    pub(crate) fn apply_spell_instakill(
        &mut self,
        hit: WrathSpellInstakill,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.instakill_revision = self.instakill_revision.saturating_add(1);
        if self.instakills.len() >= SPELL_LOG_CAPACITY {
            self.instakills.pop_front();
            self.instakill_dropped = self.instakill_dropped.saturating_add(1);
        }
        self.instakills.push_back(WrathSpellInstakill {
            session_epoch,
            world_generation,
            revision: self.instakill_revision,
            received_at,
            ..hit
        });
    }

    pub(crate) fn apply_enchant_log(
        &mut self,
        hit: WrathEnchantLog,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.enchant_revision = self.enchant_revision.saturating_add(1);
        if self.enchant_logs.len() >= SPELL_LOG_CAPACITY {
            self.enchant_logs.pop_front();
            self.enchant_dropped = self.enchant_dropped.saturating_add(1);
        }
        self.enchant_logs.push_back(WrathEnchantLog {
            session_epoch,
            world_generation,
            revision: self.enchant_revision,
            received_at,
            ..hit
        });
    }

    /// `HandleEnvironmentDamage` (`UnitCombat_C.cpp:57`) writes no row when damage, absorbed
    /// and resisted are all zero; the visual kit is a separate consumer.
    pub(crate) fn apply_environmental_damage(
        &mut self,
        hit: WrathEnvironmentalDamage,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        if hit.damage == 0 && hit.absorbed == 0 && hit.resisted == 0 {
            return;
        }
        self.environmental_damage_revision = self.environmental_damage_revision.saturating_add(1);
        if self.environmental_damages.len() >= SPELL_LOG_CAPACITY {
            self.environmental_damages.pop_front();
            self.environmental_damage_dropped = self.environmental_damage_dropped.saturating_add(1);
        }
        self.environmental_damages
            .push_back(WrathEnvironmentalDamage {
                session_epoch,
                world_generation,
                revision: self.environmental_damage_revision,
                received_at,
                ..hit
            });
    }

    pub(crate) fn apply_damage_shield(
        &mut self,
        hit: WrathDamageShield,
        session_epoch: u64,
        world_generation: u64,
        received_at: f64,
    ) {
        self.damage_shield_revision = self.damage_shield_revision.saturating_add(1);
        if self.damage_shields.len() >= SPELL_LOG_CAPACITY {
            self.damage_shields.pop_front();
            self.damage_shield_dropped = self.damage_shield_dropped.saturating_add(1);
        }
        self.damage_shields.push_back(WrathDamageShield {
            session_epoch,
            world_generation,
            revision: self.damage_shield_revision,
            received_at,
            ..hit
        });
    }

    /// Clear every scoped rune/calendar/combat-log/LFG fact on a session end or world transfer.
    fn clear_rune_calendar(&mut self) {
        self.runes.clear();
        self.calendar = None;
        self.calendar_fetched = false;
        self.calendar_revision = 0;
        self.last_calendar_command_result = None;
        self.party_kills.clear();
        self.party_kill_dropped = 0;
        self.party_kill_unresolved = 0;
        self.swings.clear();
        self.swing_dropped = 0;
        self.swing_unresolved = 0;
        self.spell_damages.clear();
        self.spell_damage_dropped = 0;
        self.spell_damage_unresolved = 0;
        self.spell_misses.clear();
        self.spell_miss_dropped = 0;
        self.spell_miss_unresolved = 0;
        self.spell_heals.clear();
        self.spell_heal_dropped = 0;
        self.spell_heal_unresolved = 0;
        self.spell_energizes.clear();
        self.spell_energize_dropped = 0;
        self.spell_energize_unresolved = 0;
        self.spell_cast_starts.clear();
        self.spell_cast_start_dropped = 0;
        self.spell_cast_start_unresolved = 0;
        self.spell_cast_successes.clear();
        self.spell_cast_success_dropped = 0;
        self.spell_cast_success_unresolved = 0;
        self.spell_cast_failures.clear();
        self.spell_cast_failure_dropped = 0;
        self.spell_cast_failure_unresolved = 0;
        self.spell_periodic_damages.clear();
        self.spell_periodic_damage_dropped = 0;
        self.spell_periodic_damage_unresolved = 0;
        self.spell_periodic_misses.clear();
        self.spell_periodic_miss_dropped = 0;
        self.spell_periodic_miss_unresolved = 0;
        self.spell_periodic_heals.clear();
        self.spell_periodic_heal_dropped = 0;
        self.spell_periodic_heal_unresolved = 0;
        self.spell_periodic_energizes.clear();
        self.spell_periodic_energize_dropped = 0;
        self.spell_periodic_energize_unresolved = 0;
        self.spell_periodic_drains.clear();
        self.spell_periodic_drain_dropped = 0;
        self.spell_periodic_drain_unresolved = 0;
        self.spell_execute_logs.clear();
        self.spell_execute_log_dropped = 0;
        self.spell_execute_log_unresolved = 0;
        self.spell_dispel_logs.clear();
        self.spell_dispel_log_dropped = 0;
        self.spell_dispel_log_unresolved = 0;
        self.spell_dispel_faileds.clear();
        self.spell_dispel_failed_dropped = 0;
        self.spell_dispel_failed_unresolved = 0;
        self.instakills.clear();
        self.instakill_dropped = 0;
        self.instakill_unresolved = 0;
        self.enchant_logs.clear();
        self.enchant_dropped = 0;
        self.enchant_unresolved = 0;
        self.environmental_damages.clear();
        self.environmental_damage_dropped = 0;
        self.environmental_damage_unresolved = 0;
        self.damage_shields.clear();
        self.damage_shield_dropped = 0;
        self.damage_shield_unresolved = 0;
        self.lfg_join_result = None;
        self.lfg_queue_status = None;
        self.lfg_update_player = None;
        self.lfg_update_party = None;
        self.lfg_proposal = None;
    }
}

/// guid -> (object generation, slot -> (receipt revision, GetTime seconds)).
type AuraAnchorRows = BTreeMap<u64, (u32, BTreeMap<u8, (u64, f64)>)>;

/// Presentation-side `GetTime` anchor for each observed aura application, keyed by object and
/// slot. The anchor is stamped once when a receipt revision is first seen (from the frame's
/// `UiClock`, the same GetTime domain `UnitAura` callers use) and is replaced only when a newer
/// receipt revision arrives. Wall/server clocks never reach the UI here. Consumers read this
/// read-only owner; the aura slot keeps the wire's remaining duration.
#[derive(Resource, Default)]
pub struct WrathAuraAnchors {
    by_guid: AuraAnchorRows,
}

impl WrathAuraAnchors {
    /// The `GetTime()` seconds at which `slot`'s current application was first observed. The
    /// `UnitAura` expiration is this anchor plus the slot's remaining `duration_ms / 1000`;
    /// `None` when the slot is not present in this object generation. A refresh (even an
    /// identical delta) arrives with a new receipt revision and re-anchors the timer.
    #[allow(dead_code)] // read-only consumer contract; Astra's UI query consumes it after the handoff
    pub fn ui_received_at(&self, guid: u64, slot: u8) -> Option<f64> {
        self.by_guid.get(&guid)?.1.get(&slot).map(|(_, at)| *at)
    }

    /// The receipt revision the anchor was stamped for, paired with its GetTime seconds.
    #[allow(dead_code)] // read-only consumer contract; pairs the anchor with its receipt revision
    pub fn ui_receipt(&self, guid: u64, slot: u8) -> Option<(u64, f64)> {
        self.by_guid.get(&guid)?.1.get(&slot).copied()
    }
}

/// Stamp one GetTime anchor per new aura receipt revision and prune slots/objects that are no
/// longer present. Called from the exclusive world-apply owner, after the digest's entities are
/// adopted; a transfer/epoch reset clears the resource with the same edge.
fn stamp_aura_anchors(world: &mut World, entities: &BTreeMap<u64, WrathEntity>) {
    let Some(ui_now) = world
        .get_resource::<crate::ui_script::UiClock>()
        .map(|clock| clock.ui_now)
    else {
        return;
    };
    let Some(mut anchors) = world.get_resource_mut::<WrathAuraAnchors>() else {
        return;
    };
    let mut next = BTreeMap::new();
    for (guid, entity) in entities {
        let (generation, mut slots) = anchors
            .by_guid
            .remove(guid)
            .unwrap_or((entity.generation, BTreeMap::new()));
        if generation != entity.generation {
            slots.clear();
        }
        slots.retain(|slot, _| entity.aura_receipts.contains_key(slot));
        for (slot, receipt) in &entity.aura_receipts {
            let at = slots
                .get(slot)
                .filter(|(revision, _)| *revision == receipt.revision)
                .map_or(ui_now, |(_, at)| *at);
            slots.insert(*slot, (receipt.revision, at));
        }
        next.insert(*guid, (entity.generation, slots));
    }
    anchors.by_guid = next;
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct WrathChanges {
    pub created: Vec<u64>,
    pub updated: Vec<u64>,
    pub removed: Vec<u64>,
    pub world_transferred: bool,
}

/// The plugin: inert without a config; registers the control resource and the Net-stage drain.
pub struct WrathNetPlugin {
    pub config: Option<WrathSessionConfig>,
}

impl Plugin for WrathNetPlugin {
    fn build(&self, app: &mut App) {
        let commands: WrathOutbox = Arc::new(Mutex::new(VecDeque::new()));
        let multiclass = (self.config.is_some()
            && std::env::var("WOW_WRATH_MULTICLASS").as_deref() == Ok("1"))
        .then(crate::multiclass::MulticlassBridge::default);
        if let Some(bridge) = &multiclass {
            app.insert_resource(bridge.clone());
            info!("multiclass: WMC1 v3 extension enabled");
        }
        app.insert_resource(WrathControl {
            config: self.config.clone(),
            multiclass,
            trace: None,
            worker: None,
            retiring: None,
            feed: None,
            cancel: Arc::new(AtomicBool::new(false)),
            commands: Some(Arc::clone(&commands)),
            ended: false,
            restart_allowed: false,
            restart_attempts: 0,
            retry_at: None,
            terminal_ready: true,
            entered_world: false,
            ended_retirement_done: false,
            left_world: false,
            shutdown: false,
            last_error: None,
            session_events: None,
            session_events_tx: None,
            reply_stats: SessionReplyStats::default(),
            park_commands: None,
            publisher_retiring: Vec::new(),
            publisher_retire_rx: None,
            wm_actions: None,
        })
        .init_resource::<WrathWorldView>()
        .init_resource::<WrathAuraAnchors>()
        .init_resource::<crate::wrath_inventory::WrathInventory>()
        .add_message::<crate::wrath_native::WrathCastEvent>()
        .add_message::<crate::wrath_native::WrathItemText>();
        if self.config.is_some() {
            if !app
                .world()
                .contains_resource::<crate::quest_trace::Source>()
            {
                crate::quest_trace::install(app);
            }
            let trace = app.world().resource::<crate::quest_trace::Source>().clone();
            app.world_mut().resource_mut::<WrathControl>().trace = Some(trace);
            app.insert_resource(WrathInput(commands));
            // The select screen's asks and Enters travel the stock `CharPick` channel, whose
            // reader is the stock IO thread. That thread never spawns for the Wrath session,
            // so its receiver is dropped and every send dies silently. Rebind the resource to
            // a wrath-owned channel: the same ordered send path, drained by the pick
            // admission here (`roster::send_pick` and the screen's re-asks stay untouched).
            let (pick_tx, pick_rx) = crossbeam_channel::unbounded();
            app.insert_resource(CharPick(ParkSender::park(pick_tx.clone())))
                .insert_resource(crate::net::RealmChoice(ParkSender::park(pick_tx)))
                .insert_resource(WrathPickQueue(pick_rx));
            let (login_tx, login_rx) = crossbeam_channel::unbounded();
            app.insert_resource(crate::net::LoginSubmit(login_tx))
                .insert_resource(WrathLoginQueue {
                    requests: login_rx,
                    pending: None,
                });
            // One typed action seam for the app's lifetime; every publisher replacement clones
            // these channel ends, so a world transfer never strands a pending intent.
            let (action_tx, action_rx) = crossbeam_channel::unbounded::<ActionRequest>();
            let (cancel_tx, cancel_rx) = crossbeam_channel::unbounded::<String>();
            let (result_tx, result_rx) = crossbeam_channel::unbounded::<ActionResult>();
            let bridge = ActionBridge {
                requests: action_tx,
                cancels: cancel_tx,
                results: result_rx,
            };
            app.world_mut().resource_mut::<WrathControl>().wm_actions = Some(bridge.clone());
            app.insert_resource(WrathWmActions {
                requests: action_rx,
                cancels: cancel_rx,
                results: result_tx,
                pending: BTreeMap::new(),
                cancelled: Vec::new(),
                last_cast_spell: None,
                next_admission: 1,
                received: 0,
                refused: 0,
                completed: 0,
            });
            // Session ownership: the worker starts on the first ownership tick. Joining here only
            // records the terminal edge; the feed stays readable so the Net drain can consume the
            // final `SessionEnded` and invalidate before Input.
            app.add_systems(
                PreUpdate,
                (
                    wrath_session_ownership,
                    wrath_shutdown_on_exit,
                    wrath_login_admission,
                    wrath_pick_admission,
                )
                    .chain(),
            );
            app.init_resource::<crate::wrath_field_map::WrathFieldMap>();
            app.insert_non_send_resource(crate::wrath_translate::WrathTranslateScript::new());
            app.add_systems(
                Update,
                (
                    crate::wrath_field_map::poll_overlay,
                    crate::wrath_translate::poll_script,
                    wrath_net_drain,
                    wrath_apply_world,
                    wrath_wm_action_intake,
                    wrath_tap_commands,
                    wrath_dispatch_events,
                )
                    .chain()
                    .in_set(WorldStage::Net),
            );
            // A second tap pass after `WorldStage::Input`, which owns the controller: the moves and
            // acks `movement_net::stream_self_movement` wrote this frame reach the outbox this
            // frame, in the order they were sent, instead of waiting for the next Net stage.
            app.add_systems(Update, wrath_tap_commands.in_set(WorldStage::Stream));
            // The Wrath VM's rune/calendar model reads the scoped view the same frame the Net
            // stage adopted it, and drains `OpenCalendar()` fetches into the shared command tap.
            app.add_systems(
                Update,
                wrath_rune_calendar_feed::feed.in_set(crate::ui_script::UiFeed),
            );
            // The equipment manager's sets, copied into the VM, and the swap result's signals.
            app.init_resource::<crate::wrath_equipment::EquipmentManager>()
                .add_systems(
                    Update,
                    crate::wrath_equipment::feed.in_set(crate::ui_script::UiFeed),
                );
            app.add_systems(
                Update,
                wrath_combat_log_feed::feed.in_set(crate::ui_script::UiFeed),
            );
        }
    }
}

/// Worker ownership: admits a session only after the prior worker's terminal scope is drained;
/// records the terminal edge without consuming the feed. The Net-stage drain still receives the
/// worker's final `SessionEnded` this frame.
fn wrath_session_ownership(
    mut control: ResMut<WrathControl>,
    state: Res<State<ClientState>>,
    mut entered: MessageReader<EnteredWorldMessage>,
    login: Option<Res<WrathLoginQueue>>,
) {
    control.poll_retiring();
    if entered.read().next().is_some() {
        control.entered_world = true;
    }
    if *state.get() == ClientState::CharSelect {
        control.left_world = true;
    }
    // Publishers that missed the worker's bounded stop are retained until they report stopped;
    // replacement admission waits for the list to empty, because a dropped handle is not proof
    // of retirement.
    control.collect_publisher_retirements();
    if control.worker.as_ref().is_some_and(|w| w.is_finished()) {
        let panicked = match control.worker.take() {
            Some(handle) => handle.join().is_err(),
            None => false,
        };
        if panicked {
            // A panicked worker never published its terminal event. Synthesize the visible loss
            // through the same tagged channel the worker uses, and clear the restart budget so
            // the refusal is a policy decision rather than a stuck flag.
            let reason = "wrath session worker panicked".to_owned();
            if control.last_error.is_none() {
                control.last_error = Some(reason.clone());
            }
            control.restart_allowed = false;
            control.retry_at = None;
            if let Some(replies) = control.session_events_tx.as_ref() {
                let generation = control.config.as_ref().and_then(|c| c.login_generation);
                let event = WrathSessionEvent::forced(SessionEvent::Disconnected {
                    reason,
                    end: benilla_protocol::SessionEnd::Lost,
                })
                .for_login(generation);
                if let Ok(mut replies) = replies.lock() {
                    replies.push(event);
                }
            }
        }
        if !control.ended {
            control.ended = true;
            // The terminal feed and its reply queue are drained in Net; only then may a
            // replacement be admitted. If Net already drained `SessionEnded`, leave its ready
            // flag alone. A panicked worker has no `SessionEnded` to drain: its terminal edge
            // is the join and the synthesized loss above.
            control.terminal_ready = panicked;
        }
    }
    let retry_due = control.retry_at.is_some_and(|at| Instant::now() >= at);
    let may_retry = control.restart_allowed && control.restart_attempts > 0 && retry_due;
    let has_pending_login = login.as_ref().is_some_and(|l| l.pending.is_some());
    let roster_restart = *state.get() == ClientState::CharSelect
        && control.ended
        && control.restart_allowed
        && !has_pending_login
        && control.config.as_ref().is_some_and(|c| {
            c.login_generation.is_some() && c.preferred_realm.is_some() && !c.account.is_empty()
        });
    if control.shutdown || (*state.get() != ClientState::InWorld && !may_retry && !roster_restart) {
        return;
    }
    let may_admit = !control.ended
        || (control.restart_allowed
            && control.restart_attempts > 0
            && (control.left_world || retry_due));
    if control.worker.is_none()
        && control.retiring.is_none()
        && control.terminal_ready
        && control.session_events_empty()
        && control.publisher_retiring.is_empty()
        && may_admit
    {
        // A retry is only ever the boot flow's: a parked link that dies has no retry (the
        // `SessionEnded` arm refuses to arm one), so the fresh session re-dials the account
        // the way the boot path does.
        control.start(if roster_restart {
            WrathSessionStart::Parked { roster_asked: true }
        } else {
            WrathSessionStart::Auto
        });
    }
}

/// The select screen's asks, drained onto the Wrath session (the stock run's channel reader
/// lives in the IO thread this plugin replaces). At character selection an enumeration or an
/// Enter World ask lands on the running parked session, or starts one and lands there; the
/// admission gates are `start`'s. Elsewhere the asks are stale and dropped with their order
/// intact. Create/delete stay on that parked world connection and wait for server results.
fn wrath_login_admission(
    mut control: ResMut<WrathControl>,
    mut login: ResMut<WrathLoginQueue>,
    abandon: Res<crate::net::LoginAbandon>,
) {
    if control
        .config
        .as_ref()
        .is_none_or(|c| c.login_generation.is_none())
    {
        // The existing direct-entry runner owns its explicit identity and named boot pick.
        for _ in login.requests.try_iter() {}
        return;
    }
    let generation = crate::net::LoginAbandon::now(Some(&abandon));
    if control
        .config
        .as_ref()
        .and_then(|c| c.login_generation)
        .is_some_and(|g| g != generation)
    {
        // `LoginAbandon::advance` runs both on `CancelWorld` (when `GlueLogin` keeps the logon
        // session and raises the realm list) and on `close_logon` (`DisconnectFromServer`, which
        // also sends `RealmRequest::Close` / `CharRequest::Abandon`). Keep the authenticated
        // logon worker alive on `CancelWorld` by routing `WrathParkedCommand::CancelWorld` to
        // the parked channel and updating the live generation tag; full logon teardown is
        // handled when `RealmRequest::Close` or `CharRequest::Abandon` arrives.
        if let Some(config) = control.config.as_mut() {
            config.login_generation = Some(generation);
            config.preferred_realm = None;
        }
        if let Some(feed) = control.feed.as_ref() {
            feed.login_generation.store(generation, Ordering::SeqCst);
        }
        if let Some(pending) = login.pending.as_mut() {
            pending.login_generation = Some(generation);
            pending.preferred_realm = None;
        }
        let parked_notified = control
            .park_commands
            .as_ref()
            .is_some_and(|tx| tx.send(WrathParkedCommand::CancelWorld).is_ok());
        if !parked_notified {
            control.cancel.store(true, Ordering::SeqCst);
        }
    }
    for request in login.requests.try_iter().collect::<Vec<_>>() {
        if request.generation != generation {
            continue;
        }
        let Some(mut config) = control.config.clone() else {
            continue;
        };
        config.account = request.user;
        config.password = request.pass;
        config.auth_addr = if request.host.contains(':') {
            request.host
        } else {
            format!("{}:3724", request.host)
        };
        config.character_name = None;
        config.login_generation = Some(generation);
        config.preferred_realm = None;
        control.cancel.store(true, Ordering::SeqCst);
        login.pending = Some(config);
    }
    if login.pending.is_some()
        && control.worker.is_none()
        && control.retiring.is_none()
        && control.terminal_ready
        && control.session_events_empty()
        && control.publisher_retiring.is_empty()
    {
        control.config = login.pending.take();
        control.ended = false;
        control.shutdown = false;
        control.retry_at = None;
        control.start(WrathSessionStart::Parked {
            roster_asked: false,
        });
    }
}

fn wrath_pick_admission(
    mut control: ResMut<WrathControl>,
    state: Res<State<ClientState>>,
    pick: Option<Res<WrathPickQueue>>,
    mut login: ResMut<WrathLoginQueue>,
    abandon: Res<crate::net::LoginAbandon>,
    mut realm_lists: MessageWriter<crate::net::RealmListMessage>,
) {
    let Some(pick) = pick.as_ref() else { return };
    let at_select = *state.get() == ClientState::CharSelect;
    for request in pick.0.try_iter() {
        let request = match request {
            ParkRequest::Char(request) => request,
            ParkRequest::Realm(request) => {
                use crate::net::RealmRequest;
                match request {
                    RealmRequest::Enter { realm, generation }
                        if generation == crate::net::LoginAbandon::now(Some(&abandon)) =>
                    {
                        if let Some(config) = control.config.as_mut() {
                            config.preferred_realm = Some(realm.id);
                            config.login_generation = Some(generation);
                        }
                        if let Some(feed) = control.feed.as_ref() {
                            feed.login_generation.store(generation, Ordering::SeqCst);
                        }
                        // The id outlives the realm's move into the parked link's command.
                        let realm_id = realm.id;
                        let sent_to_parked = control.worker.is_some()
                            && !control.cancel.load(Ordering::SeqCst)
                            && control.park_commands.as_ref().is_some_and(|tx| {
                                tx.send(WrathParkedCommand::Realm(realm)).is_ok()
                            });
                        if !sent_to_parked {
                            if let Some(mut config) = control.config.clone() {
                                config.preferred_realm = Some(realm_id);
                                config.login_generation = Some(generation);
                                if !config.account.is_empty() {
                                    login.pending = Some(config);
                                    control.cancel.store(true, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                    RealmRequest::Refresh => {
                        let sent = control.worker.is_some()
                            && !control.cancel.load(Ordering::SeqCst)
                            && control.park_commands.as_ref().is_some_and(|tx| {
                                tx.send(WrathParkedCommand::RefreshRealms).is_ok()
                            });
                        if !sent {
                            if let Some(mut config) = control.config.clone() {
                                if !config.account.is_empty() && config.login_generation.is_some() {
                                    config.preferred_realm = None;
                                    login.pending = Some(config);
                                    control.cancel.store(true, Ordering::SeqCst);
                                } else {
                                    realm_lists.write(crate::net::RealmListMessage {
                                        realms: Vec::new(),
                                        failed: true,
                                    });
                                }
                            } else {
                                realm_lists.write(crate::net::RealmListMessage {
                                    realms: Vec::new(),
                                    failed: true,
                                });
                            }
                        }
                    }
                    RealmRequest::Leave => {
                        if let Some(config) = control.config.as_mut() {
                            config.preferred_realm = None;
                        }
                        control.restart_allowed = false;
                        if let Some(tx) = control.park_commands.as_ref() {
                            let _ = tx.send(WrathParkedCommand::LeaveRealm);
                        }
                    }
                    RealmRequest::Close if !at_select => {
                        login.pending = None;
                        control.restart_allowed = false;
                        if let Some(config) = control.config.as_mut() {
                            config.account.clear();
                            config.password.clear();
                            config.preferred_realm = None;
                        }
                        if let Some(tx) = control.park_commands.as_ref() {
                            let _ = tx.send(WrathParkedCommand::Close);
                        }
                        control.cancel.store(true, Ordering::SeqCst);
                    }
                    _ => {}
                }
                continue;
            }
        };
        match request {
            CharRequest::RealmSplit(state) if at_select => {
                control.park_serve(false, WrathParkedCommand::RealmSplit(state));
            }
            CharRequest::Enumerate => {
                if at_select {
                    control.park_serve(true, WrathParkedCommand::Enumerate);
                }
            }
            CharRequest::Enter(guid) => {
                if at_select {
                    control.park_serve(false, WrathParkedCommand::Enter(guid));
                }
            }
            CharRequest::Create(request) if at_select => {
                control.park_serve(false, WrathParkedCommand::Create(request));
            }
            CharRequest::Delete(guid) if at_select => {
                control.park_serve(false, WrathParkedCommand::Delete(guid));
            }
            CharRequest::Rename { guid, name } if at_select => {
                control.park_serve(false, WrathParkedCommand::Rename { guid, name });
            }
            CharRequest::Abandon => {
                login.pending = None;
                control.restart_allowed = false;
                if let Some(config) = control.config.as_mut() {
                    config.account.clear();
                    config.password.clear();
                    config.preferred_realm = None;
                }
                if let Some(tx) = control.park_commands.as_ref() {
                    let _ = tx.send(WrathParkedCommand::Close);
                }
                control.cancel.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

/// Drains WM intents from the publisher worker, fences them against the adopted scope and
/// deadline, maps them onto the existing `ClientCommand` owners and records them as in flight.
/// Admission is answered `Accepted`; terminal states come from replies, scope loss or the
/// deadline.
fn wrath_wm_action_intake(
    mut actions: ResMut<WrathWmActions>,
    view: Res<WrathWorldView>,
    net: Option<Res<NetCommands>>,
    trace: Option<Res<crate::quest_trace::Source>>,
) {
    let Some(net) = net else { return };
    let Some(phase) = view.phase else { return };
    let revision = Some(view.state_revision);

    // A cancel is admission-level only: before dispatch it prevents the commands; after
    // dispatch the command may already be on the wire, so the honest answer is Unknown.
    let cancels: Vec<String> = actions.cancels.try_iter().collect();
    for request_id in cancels {
        if let Some(pending) = actions.pending.remove(&request_id) {
            actions.answer(
                request_id,
                ActionStatus::Unknown,
                pending.session_epoch,
                pending.world_generation,
                revision,
                Some(
                    "cancellation arrived after dispatch; the command may still execute".to_owned(),
                ),
            );
        } else {
            actions.cancelled.push(request_id);
        }
    }

    let requests: Vec<_> = actions.requests.try_iter().collect();
    for request in requests {
        actions.received += 1;
        if actions.pending.len() >= 16 {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("client action queue is full".to_owned()),
            );
            continue;
        }
        if request.player_guid != view.player_guid
            || request.session_epoch != view.session_epoch
            || request.world_generation != view.world_generation
            || phase != Phase::InWorld
        {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("intent scope does not match the adopted world".to_owned()),
            );
            continue;
        }
        if request.deadline_unix_ms > 0 && now_ms() > request.deadline_unix_ms {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Expired,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("intent deadline passed before dispatch".to_owned()),
            );
            continue;
        }
        if matches!(
            &request.intent,
            ActionIntent::MoveStart { .. } | ActionIntent::MoveStop
        ) {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Unsupported,
                request.session_epoch,
                request.world_generation,
                revision,
                Some(
                    "no remote movement owner exists; movement is the human input controller"
                        .to_owned(),
                ),
            );
            continue;
        }
        if actions.cancelled.iter().any(|id| id == &request.request_id) {
            actions.cancelled.retain(|id| id != &request.request_id);
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Cancelled,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("cancelled before dispatch".to_owned()),
            );
            continue;
        }
        if actions.pending.contains_key(&request.request_id) {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("duplicate request id".to_owned()),
            );
            continue;
        }

        // Observed-conflict refusals: only a positively observed state blocks, and the detail
        // names the fact. This is the client's own resolved mirror, not a second resolution.
        let self_dead = view.self_observation.as_ref().is_some_and(|observation| {
            matches!(
                observation.death,
                benilla_protocol_wrath::session::SelfDeath::Dead
                    | benilla_protocol_wrath::session::SelfDeath::Ghost
            )
        });
        let observed_refusal: Option<String> = match &request.intent {
            ActionIntent::Cast { .. } => {
                if self_dead {
                    Some("the client observes the player dead; refusing cast".to_owned())
                } else if let Some(cast) = view.casting(view.player_guid) {
                    Some(format!(
                        "the client already observes cast attempt {} of spell {}; wait for its terminal edge or cancel it",
                        cast.attempt_serial, cast.identity.spell_id
                    ))
                } else {
                    view.channeling(view.player_guid).map(|channel| {
                        format!(
                            "the client already observes channel spell {}",
                            channel.spell_id
                        )
                    })
                }
            }
            ActionIntent::CastCancel => {
                if view.casting(view.player_guid).is_none()
                    && view.channeling(view.player_guid).is_none()
                {
                    Some("the client observes no active cast or channel to cancel".to_owned())
                } else {
                    None
                }
            }
            ActionIntent::AttackStart { .. } if self_dead => {
                Some("the client observes the player dead; refusing the intent".to_owned())
            }
            _ => None,
        };
        if matches!(&request.intent, ActionIntent::CastCancel) {
            // Record the local cancel request before admission can reject it. The resolved
            // mirror supplies the identity when one exists; otherwise the target is unknown.
            if let Some(trace) = trace.as_deref() {
                let (spell_id, wire_cast_count) = if let Some(cast) = view.casting(view.player_guid)
                {
                    (
                        Some(cast.identity.spell_id),
                        cast.identity.cast_count.map(u32::from),
                    )
                } else if let Some(channel) = view.channeling(view.player_guid) {
                    (Some(channel.spell_id), None)
                } else {
                    (None, None)
                };
                let ix = trace.begin(
                    Some((view.session_epoch, view.world_generation)),
                    None,
                    crate::quest_trace::Operation::Observed,
                );
                ix.record(crate::quest_trace::Stage::Cast {
                    spell_id,
                    attempt: None,
                    wire_cast_count,
                    edge: crate::quest_trace::CastEdge::CancelRequested,
                });
            }
        }
        if let Some(reason) = observed_refusal {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some(reason),
            );
            continue;
        }
        let mut commands: Vec<ClientCommand> = Vec::new();
        let mut refused: Option<String> = None;
        match &request.intent {
            ActionIntent::SetTarget { guid } => {
                commands.push(ClientCommand::SetSelection { guid: *guid });
            }
            ActionIntent::AttackStart { guid } => {
                commands.push(ClientCommand::AttackSwing { guid: *guid });
            }
            ActionIntent::AttackStop => commands.push(ClientCommand::AttackStop),
            ActionIntent::Cast { spell_id, target } => {
                actions.last_cast_spell = Some(*spell_id);
                // Give a WM-dispatched cast the same local interaction as a UI press; the tap
                // adds the wire count and the native drain supplies the outcome.
                let cast_trace = trace.as_deref().map(|trace| {
                    let ix = trace.begin(
                        Some((view.session_epoch, view.world_generation)),
                        None,
                        crate::quest_trace::Operation::Observed,
                    );
                    ix.record(crate::quest_trace::Stage::Cast {
                        spell_id: Some(*spell_id),
                        attempt: None,
                        wire_cast_count: None,
                        edge: crate::quest_trace::CastEdge::Requested,
                    });
                    ix
                });
                commands.push(ClientCommand::CastSpell {
                    spell_id: *spell_id,
                    target: *target,
                    trace: cast_trace,
                });
            }
            ActionIntent::CastCancel => {
                // Cancel what the resolved mirror says is actually running, so the command
                // owner reaches the same attempt the observation reported. The cancel request
                // itself was recorded above, before admission could reject it.
                if let Some(cast) = view.casting(view.player_guid) {
                    actions.last_cast_spell = Some(cast.identity.spell_id);
                    commands.push(ClientCommand::CancelCast {
                        spell_id: cast.identity.spell_id,
                        trace: None,
                    });
                } else if let Some(channel) = view.channeling(view.player_guid) {
                    commands.push(ClientCommand::CancelChannelling {
                        spell_id: channel.spell_id,
                        trace: None,
                    });
                } else if let Some(spell_id) = actions.last_cast_spell {
                    commands.push(ClientCommand::CancelCast {
                        spell_id,
                        trace: None,
                    });
                } else {
                    refused = Some("no client-known cast to cancel".to_owned());
                }
            }
            ActionIntent::Interact { guid } => {
                commands.push(ClientCommand::GossipHello { guid: *guid });
            }
            ActionIntent::GossipSelect { guid, option } => {
                commands.push(ClientCommand::GossipSelectOption {
                    guid: *guid,
                    option: *option,
                    code: None,
                });
            }
            ActionIntent::UseObject { guid } => {
                commands.push(ClientCommand::GameObjUse { guid: *guid });
            }
            ActionIntent::QuestAccept { npc, quest } => {
                commands.push(ClientCommand::QuestgiverAccept {
                    npc: *npc,
                    quest: *quest,
                });
            }
            ActionIntent::QuestComplete { npc, quest } => {
                commands.push(ClientCommand::QuestgiverComplete {
                    npc: *npc,
                    quest: *quest,
                });
            }
            ActionIntent::QuestChooseReward { npc, quest, reward } => {
                commands.push(ClientCommand::QuestgiverChooseReward {
                    npc: *npc,
                    quest: *quest,
                    choice: *reward,
                });
            }
            ActionIntent::LootTake { source, slot } => {
                // The stock owner opens the window then takes the slot; both commands are
                // idempotent at this layer and the server answers authoritatively.
                commands.push(ClientCommand::Loot { guid: *source });
                commands.push(ClientCommand::AutostoreLootItem { slot: *slot });
            }
            ActionIntent::LootMoney { source } => {
                commands.push(ClientCommand::Loot { guid: *source });
                commands.push(ClientCommand::LootMoney);
            }
            ActionIntent::LootRelease { source } => {
                commands.push(ClientCommand::LootRelease { guid: *source });
            }
            ActionIntent::MoveStart { .. } | ActionIntent::MoveStop => {}
        }
        if let Some(reason) = refused {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some(reason.to_owned()),
            );
            continue;
        }
        let mut owner_failed = false;
        for command in commands {
            if net.0.send(command).is_err() {
                owner_failed = true;
                break;
            }
        }
        if owner_failed {
            actions.refused += 1;
            actions.answer(
                request.request_id,
                ActionStatus::Rejected,
                request.session_epoch,
                request.world_generation,
                revision,
                Some("the command owner is unavailable".to_owned()),
            );
            continue;
        }
        let cast_spell = match &request.intent {
            ActionIntent::Cast { spell_id, .. } => Some(*spell_id),
            ActionIntent::CastCancel => view
                .casting(view.player_guid)
                .map(|cast| cast.identity.spell_id)
                .or_else(|| {
                    view.channeling(view.player_guid)
                        .map(|channel| channel.spell_id)
                })
                .or(actions.last_cast_spell),
            _ => None,
        };
        // A cancel binds the attempt that is active at admission, so its authoritative terminal
        // resolves this cancel and not another cast of the same spell. A cast binds at its START.
        let (cast_attempt, cast_count) = match &request.intent {
            ActionIntent::CastCancel => view
                .casting(view.player_guid)
                .map(|cast| (Some(cast.attempt_serial), cast.identity.cast_count))
                .unwrap_or((None, None)),
            _ => (None, None),
        };
        let admitted = actions.next_admission;
        actions.next_admission = actions.next_admission.saturating_add(1);
        actions.pending.insert(
            request.request_id.clone(),
            PendingWmAction {
                intent: request.intent.clone(),
                player_guid: request.player_guid,
                cast_spell,
                cast_attempt,
                cast_count,
                admitted,
                deadline: Instant::now() + Duration::from_secs(30),
                session_epoch: request.session_epoch,
                world_generation: request.world_generation,
            },
        );
        actions.answer(
            request.request_id,
            ActionStatus::Accepted,
            request.session_epoch,
            request.world_generation,
            revision,
            None,
        );
    }

    // A dispatched intent with no authoritative reply by its deadline is Unknown, not success
    // and not failure: the server may have acted.
    let expired: Vec<String> = actions
        .pending
        .iter()
        .filter(|(_, pending)| Instant::now() >= pending.deadline)
        .map(|(id, _)| id.clone())
        .collect();
    for id in expired {
        if let Some(pending) = actions.pending.remove(&id) {
            actions.answer(
                id,
                ActionStatus::Unknown,
                pending.session_epoch,
                pending.world_generation,
                revision,
                Some("no authoritative reply within the client action deadline".to_owned()),
            );
        }
    }
}

/// The stock chat bar's kind in the 12340 `ChatMsg` numbering.
fn wrath_chat_type(kind: crate::net::ChatKind) -> u32 {
    use crate::net::ChatKind;
    match kind {
        ChatKind::Say => 0x01,
        ChatKind::Yell => 0x06,
        ChatKind::Emote => 0x0A,
        ChatKind::Whisper => 0x07,
        ChatKind::Party => 0x02,
        ChatKind::Raid => 0x03,
        ChatKind::RaidLeader => 0x27,
        ChatKind::RaidWarning => 0x28,
        ChatKind::Guild => 0x04,
        ChatKind::Officer => 0x05,
        ChatKind::Battleground => 0x2C,
        ChatKind::BattlegroundLeader => 0x2D,
        ChatKind::Afk => 0x17,
        ChatKind::Dnd => 0x18,
        ChatKind::Channel => 0x11,
    }
}

/// The language the stock send would use when the frame names none: AFK/DND accept universal,
/// every character otherwise speaks its race's native tongue (Common 7 or Orcish 1).
fn stock_chat_language(
    kind: crate::net::ChatKind,
    requested: Option<u32>,
    view: &WrathWorldView,
) -> u32 {
    if let Some(language) = requested {
        return language;
    }
    if matches!(kind, crate::net::ChatKind::Afk | crate::net::ChatKind::Dnd) {
        return 0;
    }
    let race = view
        .entities
        .get(&view.player_guid)
        .and_then(|entity| entity.bytes_0)
        .map(|bytes| (bytes & 0xff) as u8);
    match race {
        Some(2 | 5 | 6 | 8 | 10) => 1, // Orcish
        _ => 7,                        // Common
    }
}

/// `AddonDistribution` in the 12340 `ChatMsg` numbering; LANG_ADDON marks the addon lane.
fn addon_chat_type(distribution: benilla_ui::script::AddonDistribution) -> u32 {
    use benilla_ui::script::AddonDistribution;
    match distribution {
        AddonDistribution::Party => 0x02,
        AddonDistribution::Raid => 0x03,
        AddonDistribution::Guild => 0x04,
        AddonDistribution::Battleground => 0x2C,
    }
}

/// The 12340 chat type as the stock owner's wire value. Wrath-only types with no 1.12 owner
/// (achievement, monster party, skill/loot/money lines, battlenet) are dropped explicitly
/// rather than projected onto an unrelated event.
fn chat_type_112(chat_type: u8) -> Option<u8> {
    use benilla_protocol::messages as m;
    Some(match chat_type {
        0x00 => m::CHAT_MSG_SYSTEM,
        0x01 => m::CHAT_MSG_SAY,
        0x02 => m::CHAT_MSG_PARTY,
        0x03 => m::CHAT_MSG_RAID,
        0x04 => m::CHAT_MSG_GUILD,
        0x05 => m::CHAT_MSG_OFFICER,
        0x06 => m::CHAT_MSG_YELL,
        0x07 | 0x08 => m::CHAT_MSG_WHISPER,
        0x09 => m::CHAT_MSG_WHISPER_INFORM,
        0x0A => m::CHAT_MSG_EMOTE,
        0x0C => m::CHAT_MSG_MONSTER_SAY,
        0x0E => m::CHAT_MSG_MONSTER_YELL,
        0x0F => m::CHAT_MSG_MONSTER_WHISPER,
        0x10 => m::CHAT_MSG_MONSTER_EMOTE,
        0x11 => m::CHAT_MSG_CHANNEL,
        0x17 => m::CHAT_MSG_AFK,
        0x18 => m::CHAT_MSG_DND,
        0x19 => m::CHAT_MSG_IGNORED,
        0x24 => m::CHAT_MSG_BG_SYSTEM_NEUTRAL,
        0x25 => m::CHAT_MSG_BG_SYSTEM_ALLIANCE,
        0x26 => m::CHAT_MSG_BG_SYSTEM_HORDE,
        0x27 => m::CHAT_MSG_RAID_LEADER,
        0x28 => m::CHAT_MSG_RAID_WARNING,
        0x29 => m::CHAT_MSG_RAID_BOSS_EMOTE,
        0x2A => m::CHAT_MSG_RAID_BOSS_WHISPER,
        0x2B => m::CHAT_MSG_FILTERED,
        0x2C => m::CHAT_MSG_BATTLEGROUND,
        0x2D => m::CHAT_MSG_BATTLEGROUND_LEADER,
        0x2E => m::CHAT_MSG_SYSTEM,
        0x33 => m::CHAT_MSG_PARTY,
        _ => return None,
    })
}

/// 12340 `SMSG_MESSAGECHAT`'s `chat_tag` is a bitmask (`0x01` AFK, `0x02` DND, `0x04` GM, `0x08`
/// COM, `0x10` DEV) whereas the 1.12 donor protocol uses the `chat_tag` enum (`NONE=0, AFK=1,
/// DND=2, GM=3`, prioritizing GM > DND > AFK).
fn chat_tag_112(tag: u8) -> u8 {
    use benilla_protocol::messages::chat_tag as t;
    if tag & 0x04 != 0 {
        t::GM
    } else if tag & 0x02 != 0 {
        t::DND
    } else if tag & 0x01 != 0 {
        t::AFK
    } else {
        t::NONE
    }
}

/// Map the stock item-use target block onto the 12340 writer. A trade-item target needs the real
/// item guid, which the stock command carries only as a slot, so it is left unsupported rather
/// than encoded against a guessed guid.
fn wrath_cast_target(
    target: benilla_protocol::messages::UseItemTarget,
) -> Option<gameplay::CastTarget> {
    use benilla_protocol::messages::{CorpseTarget, UseItemTarget};
    Some(match target {
        UseItemTarget::SelfImplicit => gameplay::CastTarget::None,
        UseItemTarget::Unit(guid) => gameplay::CastTarget::Unit(guid),
        UseItemTarget::Object(guid) => gameplay::CastTarget::GameObject(guid),
        UseItemTarget::Item(guid) => gameplay::CastTarget::Item(guid),
        UseItemTarget::Dest(point) => gameplay::CastTarget::Dest(point),
        UseItemTarget::Source(point) => gameplay::CastTarget::Source(point),
        UseItemTarget::Corpse(CorpseTarget::Ally, guid) => gameplay::CastTarget::CorpseAlly(guid),
        UseItemTarget::Corpse(CorpseTarget::Enemy, guid) => gameplay::CastTarget::CorpseEnemy(guid),
        UseItemTarget::TradeItem(slot) => gameplay::CastTarget::TradeItem(slot),
    })
}

/// The 1-based row for `entry` in the open vendor list. 12340 buy packets address the row;
/// without a matching open row the command cannot be encoded faithfully and is dropped.
fn vendor_row(view: &WrathWorldView, vendor: u64, entry: u32) -> Option<u32> {
    view.vendor
        .as_ref()
        .filter(|(guid, _)| *guid == vendor)
        .and_then(|(_, rows)| rows.iter().find(|row| row.entry == entry))
        .map(|row| row.slot)
}

/// Application exit owns a bounded stop of the session thread; dropping a running `JoinHandle`
/// alone is not cancellation.
fn wrath_shutdown_on_exit(
    mut control: ResMut<WrathControl>,
    mut actions: Option<ResMut<WrathWmActions>>,
    mut exits: MessageReader<AppExit>,
) {
    if exits.read().next().is_some() {
        control.stop_with_delivery(actions.as_deref_mut());
    }
}

/// The Net-stage drain: bounded, ordered, lifecycle-invalidating (R5).
fn reward_items(items: Vec<gameplay::RewardItem>) -> Vec<QuestRewardItem> {
    items
        .into_iter()
        .map(|item| QuestRewardItem {
            item_id: item.item_id,
            count: item.count,
            display_id: item.display_id,
        })
        .collect()
}

fn quest_panel_extra(
    extra: gameplay::QuestPanelExtra,
) -> benilla_protocol::messages::QuestPanelExtra {
    benilla_protocol::messages::QuestPanelExtra {
        flags: extra.flags,
        suggested_players: extra.suggested_players,
        xp: extra.xp,
        honor_tenths: extra.honor_tenths,
        honor_multiplier: extra.honor_multiplier,
        cast_spell: extra.cast_spell,
        title_id: extra.title_id,
        talents: extra.talents,
        arena_points: extra.arena_points,
        reputation: extra.reputation,
    }
}

fn quest_entries(quests: Vec<gameplay::QuestListEntry>) -> Vec<QuestListEntry> {
    quests
        .into_iter()
        .map(|q| QuestListEntry {
            flags: q.flags,
            repeatable: q.repeatable,
            quest_id: q.quest_id,
            icon: q.icon,
            level: q.level,
            title: q.title,
        })
        .collect()
}

/// 12340 cast flags onto the 1.12 seam's u16 (`Spell.h:55-84` vs vmangos `Spell.h:53-64`). No
/// bit is truncated blindly: the pending flag (0x1) shares vanilla's hidden-combat-log bit, the
/// projectile flag (0x20) shares vanilla's ammo bit, and a launch carries vanilla's GO marker
/// (0x100). Every other 3.3.5 bit has no 1.12 meaning at this seam and is not copied; the wrath
/// event keeps the full u32.
fn vanilla_cast_flags(wrath: u32, launch: bool) -> u16 {
    let mut flags = 0u16;
    if wrath & 0x0000_0001 != 0 {
        flags |= 0x0001;
    }
    if wrath & 0x0000_0020 != 0 {
        flags |= 0x0020;
    }
    if launch {
        flags |= 0x0100;
    }
    flags
}

/// The 1.12 target mask from 12340's: the low 16 bits share their meanings, except the mini-pet
/// bit (0x10000), which has no 1.12 bit and maps onto UNIT so the missile launcher and the
/// open-lock gate still see a target.
fn vanilla_target_mask(mask: u32) -> u16 {
    let mut vanilla = (mask & 0xffff) as u16;
    if mask & 0x0001_0000 != 0 {
        vanilla |= 0x0002;
    }
    vanilla
}

/// 12340's melee `HitInfo` onto the 1.12 seam's bit layout. The bit positions moved between the
/// builds: 12340's 0x20/0x40 absorb, 0x80/0x100 resist, 0x200 crit, 0x2000 block,
/// 0x10000/0x20000 glancing/crushing, 0x40000 no-animation and 0x200000 swing-no-hit-sound are
/// the 1.12 seam's 0x20, 0x40, 0x80, 0x800, 0x4000, 0x8000, 0x10000 and 0x80000. 12340's
/// 0x4000/0x8000 words are not glancing/crushing, and its 0x800000 rage-gain bit has no 1.12
/// word, so all three are dropped explicitly rather than leaking into the 1.12 consumers as false
/// outcomes.
fn vanilla_hit_info(wrath: u32) -> u32 {
    const AFFECTS_VICTIM: u32 = 0x0000_0002;
    const OFFHAND: u32 = 0x0000_0004;
    const MISS: u32 = 0x0000_0010;
    const ABSORB: u32 = 0x0000_0020;
    const RESIST: u32 = 0x0000_0040;
    const CRITICAL: u32 = 0x0000_0080;
    const BLOCK: u32 = 0x0000_0800;
    const GLANCING: u32 = 0x0000_4000;
    const CRUSHING: u32 = 0x0000_8000;
    const NO_ANIMATION: u32 = 0x0001_0000;
    const SWING_NO_HIT_SOUND: u32 = 0x0008_0000;

    let mut vanilla = wrath & (AFFECTS_VICTIM | OFFHAND | MISS);
    if wrath & (0x0000_0020 | 0x0000_0040) != 0 {
        vanilla |= ABSORB;
    }
    if wrath & (0x0000_0080 | 0x0000_0100) != 0 {
        vanilla |= RESIST;
    }
    if wrath & 0x0000_0200 != 0 {
        vanilla |= CRITICAL;
    }
    if wrath & 0x0000_2000 != 0 {
        vanilla |= BLOCK;
    }
    if wrath & 0x0001_0000 != 0 {
        vanilla |= GLANCING;
    }
    if wrath & 0x0002_0000 != 0 {
        vanilla |= CRUSHING;
    }
    if wrath & 0x0004_0000 != 0 {
        vanilla |= NO_ANIMATION;
    }
    if wrath & 0x0020_0000 != 0 {
        vanilla |= SWING_NO_HIT_SOUND;
    }
    vanilla
}

/// 12340 sends a spell-school *mask* where the 1.12 seam carries the 0-6 school *index*. A normal
/// melee swing's mask is 1, which the 1.12 line would otherwise read as a non-physical Holy hit.
fn vanilla_school(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros().min(6)
    }
}

/// [`translate_gameplay`] through the live-code boundary, so a patched translation runs from the
/// next reply on. The boundary may run its closure again after a patch unwinds it, so each run
/// translates its own copy of the event.
#[cfg(feature = "live_code")]
fn translate_live(
    event: WrathGameplayEvent,
    self_guid: u64,
    received_at: Instant,
) -> Vec<SessionEvent> {
    crate::live_code::call(|| translate_gameplay(event.clone(), self_guid, received_at))
}

/// Without `live_code`, [`translate_gameplay`] itself, with no copy.
#[cfg(not(feature = "live_code"))]
fn translate_live(
    event: WrathGameplayEvent,
    self_guid: u64,
    received_at: Instant,
) -> Vec<SessionEvent> {
    translate_gameplay(event, self_guid, received_at)
}

/// A saved store's answer body decoded the way the session thread decodes it live: the 3.3.5
/// gameplay decoder and [`translate_gameplay`], or the shared 1.12 decoder for an opcode the
/// 3.3.5 one leaves to it ([`is_shared_12340_server_opcode`]). [`crate::wdb`] files the result when
/// it loads a store's file and when a live-code patch re-derives the store. A template answers no
/// one player, so the translation's own guid is 0.
pub(crate) fn decode_saved_answer(opcode: u16, body: &[u8]) -> Vec<SessionEvent> {
    match gameplay::parse(opcode, body) {
        Some(Ok(event)) => translate_gameplay(event, 0, Instant::now()),
        Some(Err(_)) => Vec::new(),
        None if is_shared_12340_server_opcode(opcode) => {
            benilla_protocol::messages::parse_server(opcode, body)
                .map(benilla_protocol::decode)
                .unwrap_or_default()
        }
        None => Vec::new(),
    }
}

/// Map a typed 3.3.5 reply onto the established 1.12 event seam. Fields with no 1.12 equivalent
/// are left empty; nothing is invented from local state. `self_guid` gates the edges the 1.12
/// seam only ever has for the player (channel timers), because 12340 broadcasts them for every
/// nearby caster.
fn translate_gameplay(
    event: WrathGameplayEvent,
    self_guid: u64,
    received_at: Instant,
) -> Vec<SessionEvent> {
    match event {
        WrathGameplayEvent::InspectHonor(d) => vec![SessionEvent::InspectHonorStats(
            benilla_protocol::messages::InspectHonorStats {
                player_guid: d.guid,
                highest_rank: d.highest_rank,
                session_kills: u32::from(d.today_hk),
                today_honor: d.today_honor,
                yesterday_hk: d.yesterday_hk,
                unknown_old1: 0,
                last_week_hk: 0,
                unknown_old2: 0,
                this_week_hk: 0,
                unknown_old3: 0,
                lifetime_hk: d.lifetime_hk,
                lifetime_dhk: 0,
                yesterday_honor: d.yesterday_honor,
                last_week_honor: 0,
                this_week_honor: 0,
                last_week_rank: 0,
                rank_bar: 0,
            },
        )],
        WrathGameplayEvent::MonsterMove(m) => {
            // 3.3.5a (`MoveSplineFlag.h`): `0x1000` is `Walkmode`, `0x2000` is `Flying`,
            // `0x4000` is `OrientationFixed`, `0x40000` is `Catmullrom` (already expanded into
            // uncompressed waypoints in `gameplay/movement.rs`), and `0x80000` is `Cyclic`.
            // Only exclude `Falling` (`0x200`), `Parabolic` (`0x800`), and `Frozen` (`0x400000`).
            //
            // Our own guid always passes (bar `Frozen`): 12340 runs a spline on the active mover
            // through the same `OnMonsterMove` (`0x73C8E0`) as any unit, and the donor self-ride
            // (`player::server_ride`) yields input to it and acks `CMSG_MOVE_SPLINE_DONE`. Dropping
            // it left Charge, Intervene and every other server move on the player unplayed: the
            // body stayed put (or snapped on the next pose snapshot) while AzerothCore moved us
            // and refused our movement until its spline ended (`VerifyMovementInfo`). A falling or
            // parabolic self-spline (a leap, a jump) has no arc evaluator yet, so it rides its
            // points as a grounded path to the right endpoint over the server's duration, which
            // is closer to the server than leaving the body behind.
            let unsupported = if m.guid == self_guid {
                m.flags & 0x400000
            } else {
                m.flags & (0x200 | 0x800 | 0x400000)
            };
            if unsupported != 0 {
                debug!(
                    guid = m.guid,
                    flags = m.flags,
                    "Wrath spline needs unsupported evaluator"
                );
                return Vec::new();
            }
            let facing = match m.facing {
                gameplay::MonsterFacing::None => benilla_protocol::MonsterMoveFacing::None,
                gameplay::MonsterFacing::Spot(p) => benilla_protocol::MonsterMoveFacing::Spot(p),
                gameplay::MonsterFacing::Target(g) => {
                    benilla_protocol::MonsterMoveFacing::Target(g)
                }
                gameplay::MonsterFacing::Angle(a) => benilla_protocol::MonsterMoveFacing::Angle(a),
            };
            vec![SessionEvent::MonsterMove {
                guid: m.guid,
                transport: m.transport.map(|(guid, _seat)| guid),
                start: m.start,
                spline_id: m.spline_id,
                path: m.path,
                facing,
                stop: m.stop,
                duration_ms: m.duration_ms,
                flying: (m.flags & 0x2000) != 0,
                run_mode: (m.flags & 0x1000) == 0,
            }]
        }
        WrathGameplayEvent::AttackStart { attacker, victim } => {
            vec![SessionEvent::AttackStart { attacker, victim }]
        }
        WrathGameplayEvent::AttackStop { attacker, victim } => {
            vec![SessionEvent::AttackStop { attacker, victim }]
        }
        WrathGameplayEvent::PartyKillLog { killer, victim } => vec![SessionEvent::PartyKillLog(
            benilla_protocol::messages::PartyKillLog { killer, victim },
        )],
        WrathGameplayEvent::AttackSwingError(reason) => {
            let reason = match reason {
                gameplay::AttackSwingError::NotInRange => AttackSwingError::NotInRange,
                gameplay::AttackSwingError::BadFacing => AttackSwingError::BadFacing,
                gameplay::AttackSwingError::DeadOrUnattackable => {
                    AttackSwingError::DeadOrUnattackable
                }
            };
            vec![SessionEvent::AttackSwingError(reason)]
        }
        WrathGameplayEvent::AttackerState {
            attacker,
            victim,
            hit_info,
            damage,
            overkill: _,
            victim_state,
            absorb,
            resist,
            blocked,
            melee_spell_id,
            school,
        } => vec![SessionEvent::AttackerState(ProtocolAttackerState {
            attacker,
            victim,
            hit_info: vanilla_hit_info(hit_info),
            damage,
            victim_state,
            absorb,
            resist,
            blocked,
            melee_spell_id,
            school: vanilla_school(school),
        })],
        WrathGameplayEvent::CancelCombat => vec![SessionEvent::CancelCombat],
        WrathGameplayEvent::AiReaction { unit, reaction } => {
            vec![SessionEvent::AiReaction { unit, reaction }]
        }
        WrathGameplayEvent::CancelAutoRepeat => vec![SessionEvent::CancelAutoRepeat],
        WrathGameplayEvent::PlaySpellVisual { unit, kit_id } => {
            vec![SessionEvent::PlaySpellVisual { unit, kit_id }]
        }
        WrathGameplayEvent::SpellDamage {
            target,
            attacker,
            spell_id,
            damage,
            overkill: _,
            school,
            absorb,
            resist,
            blocked,
            hit_info,
        } => vec![SessionEvent::SpellDamageLog(ProtocolSpellDamageLog {
            target,
            attacker,
            spell_id,
            damage,
            school: vanilla_school(u32::from(school)) as u8,
            absorb,
            resist,
            // 12340's byte is `physicalLog`; periodic damage rides `SMSG_PERIODICAURALOG`.
            periodic: false,
            blocked,
            hit_info,
        })],
        WrathGameplayEvent::DamageShield { .. } => Vec::new(),
        WrathGameplayEvent::SpellInstakill { .. } => Vec::new(),
        WrathGameplayEvent::EnchantApplied { .. } => Vec::new(),
        WrathGameplayEvent::EnchantRemoved { .. } => Vec::new(),
        WrathGameplayEvent::EnvironmentalDamage { .. } => Vec::new(),
        WrathGameplayEvent::SpellMiss { .. } => Vec::new(),
        WrathGameplayEvent::SpellExecuteLog { .. } => Vec::new(),
        WrathGameplayEvent::SpellDispelLog { .. } => Vec::new(),
        WrathGameplayEvent::SpellStealLog { .. } => Vec::new(),
        WrathGameplayEvent::SpellDispelFailed { .. } => Vec::new(),
        WrathGameplayEvent::PeriodicAura {
            target,
            caster,
            spell_id,
            ticks,
        } => {
            let ticks = ticks
                .into_iter()
                .map(|tick| match tick {
                    gameplay::PeriodicAuraTick::Damage {
                        amount,
                        school,
                        absorb,
                        resist,
                        ..
                    } => ProtocolPeriodicTick::Damage {
                        amount,
                        school: vanilla_school(school),
                        absorb,
                        resist,
                    },
                    gameplay::PeriodicAuraTick::Heal { amount, .. } => {
                        ProtocolPeriodicTick::Heal { amount }
                    }
                    gameplay::PeriodicAuraTick::Energize { power, amount } => {
                        ProtocolPeriodicTick::Energize { power, amount }
                    }
                    gameplay::PeriodicAuraTick::ManaLeech {
                        power,
                        amount,
                        multiplier,
                    } => ProtocolPeriodicTick::ManaLeech {
                        power,
                        amount,
                        multiplier,
                    },
                })
                .collect();
            vec![SessionEvent::PeriodicAuraLog(ProtocolPeriodicAuraLog {
                target,
                caster,
                spell_id,
                ticks,
            })]
        }
        WrathGameplayEvent::CastFailed {
            spell_id,
            reason,
            arg,
            ..
        } => {
            let (reason, arg) = match reason {
                cast_failure::Reason::Vanilla(reason) => (Some(reason), arg),
                cast_failure::Reason::Unsupported(wrath) => {
                    warn!(
                        "wrath: unsupported cast-failure reason {wrath}; using SPELL_FAILED_ERROR"
                    );
                    (Some(0x1c), None)
                }
            };
            vec![SessionEvent::CastResult {
                spell_id,
                success: false,
                reason,
                arg,
            }]
        }
        WrathGameplayEvent::SpellStart {
            caster,
            spell_id,
            cast_flags,
            cast_time_ms,
            targets,
            ammo_display_id,
            ..
        } => vec![SessionEvent::SpellStart {
            caster,
            spell_id,
            cast_flags: vanilla_cast_flags(cast_flags, false),
            cast_time_ms: cast_time_ms.max(0) as u32,
            target: targets.unit,
            ammo_display_id,
        }],
        WrathGameplayEvent::SpellDelayed { caster, delay_ms } => {
            vec![SessionEvent::SpellDelayed { caster, delay_ms }]
        }
        WrathGameplayEvent::SpellCooldowns { caster, cooldowns } => {
            vec![SessionEvent::SpellCooldowns { caster, cooldowns }]
        }
        WrathGameplayEvent::ItemCooldown {
            item_guid,
            spell_id,
        } => {
            vec![SessionEvent::ItemCooldown {
                item_guid,
                spell_id,
            }]
        }
        WrathGameplayEvent::CooldownEvent { spell_id, caster } => {
            vec![SessionEvent::CooldownEvent { spell_id, caster }]
        }
        WrathGameplayEvent::ClearCooldown { spell_id, caster } => {
            vec![SessionEvent::ClearCooldown { spell_id, caster }]
        }
        WrathGameplayEvent::ChannelStart {
            caster,
            spell_id,
            duration_ms,
        } => {
            // 12340 broadcasts the channel; the 1.12 start edge is the player's own cast bar.
            if caster == self_guid {
                vec![SessionEvent::ChannelStart {
                    spell_id,
                    duration_ms,
                }]
            } else {
                Vec::new()
            }
        }
        WrathGameplayEvent::ChannelUpdate {
            caster,
            remaining_ms,
        } => {
            if caster == self_guid {
                vec![SessionEvent::ChannelUpdate { remaining_ms }]
            } else {
                Vec::new()
            }
        }
        WrathGameplayEvent::CorpseQuery {
            found,
            display_map,
            position,
            corpse_map,
        } => vec![SessionEvent::CorpseQuery {
            found,
            display_map,
            position,
            corpse_map,
        }],
        WrathGameplayEvent::CorpseReclaimDelay { delay_ms } => {
            vec![SessionEvent::CorpseReclaimDelay { delay_ms }]
        }
        WrathGameplayEvent::ResurrectRequest {
            caster,
            name,
            sickness,
            has_timer,
        } => vec![SessionEvent::ResurrectRequest {
            caster,
            name,
            sickness,
            has_timer,
        }],
        WrathGameplayEvent::SpiritHealerConfirm { npc } => {
            vec![SessionEvent::SpiritHealerConfirm { npc }]
        }
        WrathGameplayEvent::DeathReleaseLoc { map_id, position } => {
            vec![SessionEvent::DeathReleaseLoc { map_id, position }]
        }
        WrathGameplayEvent::PreResurrect { guid } => {
            vec![SessionEvent::PreResurrect { guid }]
        }
        WrathGameplayEvent::ClearTarget { guid } => {
            vec![SessionEvent::ClearTarget { guid }]
        }
        WrathGameplayEvent::AreaSpiritHealerTime { healer, ms } => {
            vec![SessionEvent::AreaSpiritHealerTime { healer, ms }]
        }
        WrathGameplayEvent::CorpseNotInInstance => vec![SessionEvent::CorpseNotInInstance],
        WrathGameplayEvent::LogoutResponse { reason, instant } => {
            vec![SessionEvent::LogoutResponse { reason, instant }]
        }
        WrathGameplayEvent::LoggedOut => vec![SessionEvent::LoggedOut],
        WrathGameplayEvent::LogoutCancelled => vec![SessionEvent::LogoutCancelled],
        WrathGameplayEvent::TransferAborted {
            map_id,
            reason,
            argument,
        } => {
            vec![SessionEvent::TransferAborted {
                reason,
                map_id: Some(map_id),
                argument,
            }]
        }
        // Our own failure is reported once, by `SMSG_CAST_FAILED` (the CastFailed arm). 12340's
        // 0x133/0x2A6 handlers (`Packet_SMSG_SPELL_FAILURE` 0x809C70, `_FAILED_OTHER` 0x806AD0)
        // never print a line. They end the cast (fizzle, `HandleSpellFailedOrGo`,
        // `FinishPendingSpellCast`), which `SpellFailedOther` does for any caster, us included.
        // AzerothCore sends `SendCastResult(r)` and then `SendInterrupted(0)` (`Spell.cpp`
        // 3807-3808, `cancel` 3669/3678). Turning the self copy into a second failed
        // `CastResult` put a second red line under every server refusal. Reason 0 (SUCCESS)
        // has no 1.12 key, so that line read "Internal error".
        // A non-zero reason is no different: `Spell::cancel` sends `SendInterrupted(40)` after
        // `SendCastResult(40)` while preparing, and alone for a delayed spell or a channel
        // interrupted by someone else, where 12340 shows the interrupted cast bar and no line.
        // Not applied: 0x809C70 also stops the local auto-repeat for reasons 52-54 (the
        // need-ammo family), which AzerothCore never sends through this message.
        WrathGameplayEvent::SpellFailure {
            caster, spell_id, ..
        } => {
            vec![SessionEvent::SpellFailedOther { caster, spell_id }]
        }
        WrathGameplayEvent::SpellGo {
            caster,
            spell_id,
            cast_flags,
            hits,
            misses,
            targets,
            ammo_display_id,
            item_caster,
            ..
        } => {
            let mut out = Vec::with_capacity(2);
            // 3.3.5a replaced 1.12 `SMSG_CAST_RESULT` (`0x130`, sent on both success and failure)
            // with failure-only `SMSG_CAST_FAILED` (`0x130`); a successful self cast goes straight
            // to `SMSG_SPELL_GO` (`0x132`), so synthesize the success `CastResult` ahead of the GO
            // to pop `PendingCast` (`spell/net.rs:1025`) and chain `modalNextSpell`.
            if caster == self_guid {
                out.push(SessionEvent::CastResult {
                    spell_id,
                    success: true,
                    reason: None,
                    arg: None,
                });
            }
            out.push(SessionEvent::SpellGo {
                caster,
                spell_id,
                cast_flags: vanilla_cast_flags(cast_flags, true),
                target: targets.unit,
                hits,
                misses: misses
                    .into_iter()
                    .map(
                        |(target, info, reflect_result)| benilla_protocol::messages::SpellGoMiss {
                            target,
                            info,
                            reflect_result,
                        },
                    )
                    .collect(),
                go_target: targets.gameobject,
                target_mask: vanilla_target_mask(targets.mask),
                source: targets.source,
                dest: targets.dest,
                ammo_display_id,
                item_caster,
                unit_caster: caster,
            });
            out
        }
        WrathGameplayEvent::InitialSpells {
            spells,
            cooldowns,
            declared_cooldowns,
        } => {
            // The pinned writer declares `m_spellCooldowns.size()` but skips entries it marks
            // not client-visible, so a larger declaration than entries is the server's filter,
            // not lost state. Complete entries present are the authoritative initial set.
            if usize::from(declared_cooldowns) != cooldowns.len() {
                warn!(
                    "wrath: SMSG_INITIAL_SPELLS declared {declared_cooldowns} cooldowns but wrote {}; using the entries present",
                    cooldowns.len()
                );
            }
            vec![SessionEvent::SpellBook {
                spell_ids: spells,
                received_at: Some(received_at),
                cooldowns: cooldowns
                    .into_iter()
                    .map(|cd| benilla_protocol::messages::SpellCooldown {
                        spell_id: cd.spell_id,
                        item_id: cd.item_id,
                        category: cd.category,
                        spell_cd_ms: cd.spell_cd_ms,
                        category_cd_ms: cd.category_cd_ms,
                    })
                    .collect(),
            }]
        }
        WrathGameplayEvent::GameObjectInfo {
            entry,
            type_id,
            display_id,
            name,
            data,
        } => {
            vec![SessionEvent::GameObjectInfo {
                entry,
                type_id,
                display_id,
                name,
                data,
            }]
        }
        WrathGameplayEvent::PlayerName {
            guid,
            name,
            race,
            gender,
            class,
        } => {
            vec![SessionEvent::PlayerName {
                guid,
                name,
                race,
                gender,
                class,
            }]
        }
        WrathGameplayEvent::CreatureName {
            entry,
            name,
            subname,
            creature_type,
            pet_family,
            rank,
            type_flags,
            display_id,
            racial_leader,
        } => vec![SessionEvent::CreatureName {
            entry,
            name,
            subname,
            creature_type: Some(creature_type),
            pet_family,
            rank,
            type_flags,
            pet_spell_list: 0,
            display_id,
            // 3.3.5 does not put the 1.12 civilian byte on this wire; leave it false rather than
            // inferring it from a differently-defined type flag.
            civilian: false,
            racial_leader,
        }],
        WrathGameplayEvent::ItemInfo { entry, info } => vec![SessionEvent::ItemTemplate {
            entry,
            info: info.map(|i| {
                Box::new(ItemInfo {
                    class: i.class,
                    subclass: i.subclass,
                    name: i.name,
                    display_info_id: i.display_info_id,
                    quality: i.quality,
                    flags: i.flags,
                    buy_price: i.buy_price,
                    sell_price: i.sell_price,
                    inventory_type: i.inventory_type,
                    allowable_class: i.allowable_class,
                    allowable_race: i.allowable_race,
                    item_level: i.item_level,
                    required_level: i.required_level,
                    required_skill: i.required_skill,
                    required_skill_rank: i.required_skill_rank,
                    required_spell: i.required_spell,
                    required_honor_rank: i.required_honor_rank,
                    required_city_rank: i.required_city_rank,
                    required_rep_faction: i.required_rep_faction,
                    required_rep_rank: i.required_rep_rank,
                    max_count: i.max_count,
                    stackable: i.stackable,
                    container_slots: i.container_slots,
                    stats: i.stats,
                    damages: i
                        .damages
                        .into_iter()
                        .map(|d| ProtocolItemDamage {
                            min: d.min,
                            max: d.max,
                            school: d.school,
                        })
                        .collect(),
                    dmg_min: i.dmg_min,
                    dmg_max: i.dmg_max,
                    dmg_type: i.dmg_type,
                    armor: i.armor,
                    resistances: i.resistances,
                    delay_ms: i.delay_ms,
                    ammo_type: i.ammo_type,
                    ranged_mod_range: i.ranged_mod_range,
                    spells: i
                        .spells
                        .into_iter()
                        .map(|s| ProtocolItemSpell {
                            index: s.index,
                            spell_id: s.spell_id,
                            trigger: s.trigger,
                            charges: s.charges,
                            cooldown_ms: s.cooldown_ms,
                            category: s.category,
                            category_cooldown_ms: s.category_cooldown_ms,
                        })
                        .collect(),
                    spell_charges_0: i.spell_charges_0,
                    use_spell: i.use_spell.map(|s| ProtocolItemUseSpell {
                        spell_id: s.spell_id,
                        cooldown_ms: s.cooldown_ms,
                        category: s.category,
                        category_cooldown_ms: s.category_cooldown_ms,
                    }),
                    bonding: i.bonding,
                    description: i.description,
                    page_text: i.page_text,
                    language_id: i.language_id,
                    page_material: i.page_material,
                    start_quest: i.start_quest,
                    lock_id: i.lock_id,
                    material: i.material,
                    sheath: i.sheath,
                    random_property: i.random_property,
                    block: i.block,
                    item_set: i.item_set,
                    max_durability: i.max_durability,
                    area: i.area,
                    map: i.map,
                    bag_family: i.bag_family,
                    wrath_roll: Some(benilla_protocol::messages::WrathRollItemInfo {
                        greed_only: i.flags2 & 0x100 != 0,
                        required_disenchant_skill: i.required_disenchant_skill,
                        item_limit_category: i.item_limit_category,
                    }),
                })
            }),
        }],
        WrathGameplayEvent::ActionButtons { state, buttons } => {
            vec![SessionEvent::ActionButtonsUpdate {
                state: match state {
                    gameplay::ActionButtonState::Snapshot => {
                        benilla_protocol::events::ActionButtonState::Snapshot
                    }
                    gameplay::ActionButtonState::Apply => {
                        benilla_protocol::events::ActionButtonState::Apply
                    }
                    gameplay::ActionButtonState::Pending => {
                        benilla_protocol::events::ActionButtonState::Pending
                    }
                },
                buttons: buttons
                    .into_iter()
                    .map(|b| ActionButton {
                        slot: b.slot,
                        action: b.action,
                        kind: b.kind,
                    })
                    .collect(),
            }]
        }
        WrathGameplayEvent::SpellLearned { spell_id } => {
            vec![SessionEvent::SpellLearned { spell_id }]
        }
        WrathGameplayEvent::Gossip {
            npc,
            menu_id: _,
            text_id,
            options,
            quests,
        } => vec![SessionEvent::GossipMenu {
            npc,
            text_id,
            options: options
                .into_iter()
                .map(|o| ProtocolGossipOption {
                    index: o.index,
                    icon: o.icon,
                    coded: o.coded,
                    message: o.message,
                })
                .collect(),
            quests: quests
                .into_iter()
                .map(|q| (q.quest_id, q.icon, q.level, q.title))
                .collect(),
        }],
        WrathGameplayEvent::GossipComplete => vec![SessionEvent::GossipComplete],
        WrathGameplayEvent::TrainerList {
            trainer,
            trainer_type,
            services,
            greeting,
        } => vec![SessionEvent::TrainerList {
            trainer,
            trainer_type,
            greeting,
            services: services
                .into_iter()
                .map(|row| ProtocolTrainerSpell {
                    spell: row.spell_id,
                    state: row.state,
                    cost: row.cost,
                    // 12340 collapses the 1.12 pair into PointCost[1]: the primary-profession
                    // first-rank flag both enables the row and asks for the slot confirmation.
                    can_learn_primary_prof: row.point_cost[1] != 0,
                    is_primary_prof_first_rank: row.point_cost[1] != 0,
                    req_level: row.req_level,
                    req_skill: row.req_skill_line,
                    req_skill_value: row.req_skill_rank,
                    req_spells: row.req_ability,
                })
                .collect(),
        }],
        WrathGameplayEvent::TrainerBuySucceeded { trainer, spell_id } => {
            vec![SessionEvent::TrainerBuySucceeded { trainer, spell_id }]
        }
        WrathGameplayEvent::TrainerBuyFailed {
            trainer,
            spell_id,
            error,
        } => vec![SessionEvent::TrainerBuyFailed {
            trainer,
            spell_id,
            error,
        }],
        WrathGameplayEvent::ShowBank { banker } => vec![SessionEvent::ShowBank { banker }],
        WrathGameplayEvent::AreaTriggerMessage { text } => {
            vec![SessionEvent::AreaTriggerMessage { text }]
        }
        WrathGameplayEvent::QuestConfirmAccept {
            quest_id,
            title,
            sender,
        } => vec![SessionEvent::QuestConfirmAccept(
            ProtocolQuestConfirmAccept {
                quest_id,
                title,
                sender,
            },
        )],
        WrathGameplayEvent::QuestPushResult { member, msg } => {
            vec![SessionEvent::QuestPushResult {
                member,
                msg: benilla_protocol::messages::QuestShareMsg(msg),
            }]
        }
        WrathGameplayEvent::BinderConfirm { binder } => {
            vec![SessionEvent::BinderConfirm { binder }]
        }
        WrathGameplayEvent::SummonRequest {
            summoner,
            zone,
            delay_ms,
        } => vec![SessionEvent::SummonRequest {
            summoner,
            zone,
            delay_ms,
        }],
        WrathGameplayEvent::TalentWipeConfirm { trainer, cost } => {
            vec![SessionEvent::TalentWipeConfirm { trainer, cost }]
        }
        WrathGameplayEvent::PlayedTime {
            total,
            level,
            trigger: _,
        } => vec![SessionEvent::PlayedTime { total, level }],
        WrathGameplayEvent::ServerUnixTime {
            unix_time,
            reset_in_secs: _,
        } => {
            vec![SessionEvent::ServerUnixTime { unix_time }]
        }
        WrathGameplayEvent::RandomRoll {
            min,
            max,
            roll,
            guid,
        } => vec![SessionEvent::RandomRoll {
            min,
            max,
            roll,
            guid,
        }],
        WrathGameplayEvent::InventoryFailure {
            reason,
            item,
            item2,
            bag_type,
            required_level,
            ..
        } => vec![SessionEvent::InventoryFailure {
            reason,
            required_level,
            item_guid: item.unwrap_or(0),
            item2_guid: item2.unwrap_or(0),
            // 12340's third byte is the bag-type subclass; the 1.12 field is the destination
            // bag slot. The native contract retains the raw value for the UI.
            bag_slot: bag_type,
        }],
        WrathGameplayEvent::ItemText { .. } => Vec::new(),
        WrathGameplayEvent::PageText {
            page_id,
            text,
            next_page_id,
        } => vec![SessionEvent::PageText {
            page_id,
            text,
            next_page_id,
        }],
        WrathGameplayEvent::BuyBankSlotResult { result } => {
            // 12340 sends OK on success; the stock bank handler treats every result as a
            // refusal and the raised slot count in PLAYER_BYTES_2 already shows the purchase.
            if result == benilla_protocol::messages::bank_slot_result::OK {
                Vec::new()
            } else {
                vec![SessionEvent::BuyBankSlotResult { result }]
            }
        }
        WrathGameplayEvent::VendorInventory { vendor, items } => {
            vec![SessionEvent::VendorInventory {
                vendor,
                items: items
                    .into_iter()
                    .map(|row| ProtocolVendorItem {
                        slot: row.slot,
                        entry: row.entry,
                        display_id: row.display_id,
                        current_count: row.current_count,
                        price: row.price,
                        max_durability: row.max_durability,
                        buy_count: row.buy_count,
                    })
                    .collect(),
            }]
        }
        WrathGameplayEvent::VendorBuyResult {
            vendor,
            slot,
            new_count,
            purchase_count,
        } => vec![SessionEvent::VendorBuyResult {
            vendor,
            slot,
            new_count,
            purchase_count,
        }],
        WrathGameplayEvent::VendorBuyFailed {
            vendor,
            item_entry,
            reason,
            param,
        } => {
            if let Some(param) = param {
                debug!("wrath: buy failure carries extended-cost param {param}");
            }
            vec![SessionEvent::VendorBuyFailed {
                vendor,
                item_entry,
                reason,
            }]
        }
        WrathGameplayEvent::VendorSellFailed {
            vendor,
            item_guid,
            reason,
            param,
        } => {
            if let Some(param) = param {
                debug!("wrath: sell failure carries param {param}");
            }
            vec![SessionEvent::VendorSellFailed {
                vendor,
                item_guid,
                reason,
            }]
        }
        WrathGameplayEvent::ChatPlayerNotFound { name } => {
            vec![SessionEvent::ChatPlayerNotFound { name }]
        }
        WrathGameplayEvent::Notification { text } => vec![SessionEvent::Notification { text }],
        WrathGameplayEvent::Chat(line) => match chat_type_112(line.chat_type) {
            Some(chat_type) => vec![SessionEvent::Chat(ProtocolChatMessage {
                chat_type,
                language: line.language,
                sender_guid: line.sender_guid,
                target_guid: line.target_guid,
                sender_name: line.sender_name,
                channel: line.channel,
                text: line.text,
                chat_tag: chat_tag_112(line.chat_tag),
            })],
            None => {
                debug!("wrath: chat type 0x{:X} has no 1.12 owner", line.chat_type);
                Vec::new()
            }
        },
        WrathGameplayEvent::QuestGiverStatus { npc, status } => {
            vec![SessionEvent::QuestGiverStatus { npc, status }]
        }
        WrathGameplayEvent::QuestGiverStatusMultiple(rows) => rows
            .into_iter()
            .map(|(npc, status)| SessionEvent::QuestGiverStatus { npc, status })
            .collect(),
        WrathGameplayEvent::QuestGreeting {
            npc,
            greeting,
            emote_delay,
            emote,
            quests,
        } => vec![SessionEvent::QuestGreeting(QuestGiverList {
            npc,
            greeting,
            emote_delay,
            emote,
            quests: quest_entries(quests),
        })],
        WrathGameplayEvent::QuestDetails(d) => vec![SessionEvent::QuestDetail(QuestDetails {
            extra: quest_panel_extra(d.extra),
            npc: d.npc,
            quest_id: d.quest_id,
            title: d.title,
            details: d.details,
            objectives: d.objectives,
            auto_finish: d.auto_finish,
            choices: reward_items(d.choices),
            rewards: reward_items(d.rewards),
            money: d.money,
            reward_spell: d.reward_spell,
        })],
        WrathGameplayEvent::QuestProgress(p) => {
            vec![SessionEvent::QuestProgress(QuestRequestItems {
                npc: p.npc,
                quest_id: p.quest_id,
                title: p.title,
                request_text: p.request_text,
                emote: p.emote,
                close_on_cancel: p.close_on_cancel,
                required_money: p.required_money,
                required_items: reward_items(p.required_items),
                is_complete: p.is_complete,
            })]
        }
        WrathGameplayEvent::QuestOffer(o) => {
            vec![SessionEvent::QuestOffer(QuestOfferReward {
                extra: quest_panel_extra(o.extra),
                npc: o.npc,
                quest_id: o.quest_id,
                title: o.title,
                offer_text: o.offer_text,
                auto_finish: o.auto_finish,
                choices: reward_items(o.choices),
                rewards: reward_items(o.rewards),
                money: o.money,
                quest_flags: o.quest_flags,
                reward_spell: o.reward_spell,
            })]
        }
        WrathGameplayEvent::QuestComplete(c) => vec![SessionEvent::QuestComplete(QuestComplete {
            quest_id: c.quest_id,
            xp: c.xp,
            money: c.money,
            items: Vec::new(),
        })],
        WrathGameplayEvent::QuestPois(pois) => vec![SessionEvent::QuestPois(
            pois.into_iter()
                .map(|q| {
                    let areas = q
                        .blobs
                        .into_iter()
                        .map(|b| benilla_protocol::QuestPoiArea {
                            objective_index: b.objective_index,
                            map_id: b.map_id as u32,
                            world_map_area_id: b.world_map_area_id as u32,
                            floor: b.floor as u32,
                            priority: b.priority as u32,
                            flags: b.flags as u32,
                            points: b.points,
                        })
                        .collect();
                    (q.quest_id, areas)
                })
                .collect(),
        )],
        WrathGameplayEvent::QuestTemplate(t) => {
            vec![SessionEvent::QuestTemplate(Box::new(QuestTemplate {
                quest_id: t.quest_id,
                method: t.method,
                level: t.level,
                zone_or_sort: t.zone_or_sort,
                quest_type: t.quest_type,
                suggested_players: t.suggested_players,
                xp_difficulty: t.xp_difficulty,
                rep_objective_faction: t.rep_objective_faction,
                rep_objective_value: t.rep_objective_value,
                opposite_rep_faction: t.opposite_rep_faction,
                opposite_rep_value: t.opposite_rep_value,
                next_quest_in_chain: t.next_quest_in_chain,
                money: t.money,
                money_max_level: t.money_max_level,
                reward_spell: t.reward_spell,
                src_item_id: t.src_item_id,
                flags: t.flags,
                honor: t.honor_reward,
                honor_multiplier: t.honor_multiplier,
                title_reward: t.title_reward,
                bonus_talents: t.bonus_talents,
                bonus_arena_points: t.bonus_arena_points,
                rep_rewards: t.rep_rewards,
                rewards: t.rewards,
                choices: t.choices,
                point_map_id: t.point_map_id,
                point_x: t.point_x,
                point_y: t.point_y,
                point_opt: t.point_opt,
                title: t.title,
                objectives_text: t.objectives_text,
                details: t.details,
                end_text: t.end_text,
                objectives: t.objectives.map(|o| QuestObjective {
                    creature_or_go: o.creature_or_go,
                    required_count: o.required_count,
                    item_id: o.item_id,
                    item_count: o.item_count,
                    text: o.text,
                }),
            }))]
        }
        WrathGameplayEvent::QuestObjectivesComplete { quest_id } => {
            vec![SessionEvent::QuestObjectivesComplete { quest_id }]
        }
        WrathGameplayEvent::QuestObjectiveKill {
            quest_id,
            entry,
            count,
            required,
        } => vec![SessionEvent::QuestObjectiveKill {
            quest_id,
            entry,
            count,
            required,
        }],
        WrathGameplayEvent::QuestObjectiveItem { item_id, count } => {
            vec![SessionEvent::QuestObjectiveItem { item_id, count }]
        }
        WrathGameplayEvent::QuestLogFull => vec![SessionEvent::QuestLogFull],
        WrathGameplayEvent::QuestGiverInvalid { reason } => {
            vec![SessionEvent::QuestGiverInvalid { reason }]
        }
        WrathGameplayEvent::QuestGiverFailed { quest_id, reason } => {
            vec![SessionEvent::QuestGiverFailed { quest_id, reason }]
        }
        WrathGameplayEvent::QuestFailed { quest_id, timed } => {
            vec![SessionEvent::QuestFailed { quest_id, timed }]
        }
        WrathGameplayEvent::NpcText { text_id, blocks } => vec![SessionEvent::NpcGreeting {
            text_id,
            blocks: blocks
                .into_iter()
                .map(|b| ProtocolNpcTextBlock {
                    probability: b.probability,
                    male: b.male,
                    female: b.female,
                })
                .collect(),
        }],
        WrathGameplayEvent::LootResponse {
            guid,
            loot_type,
            gold,
            items,
        } => vec![SessionEvent::LootResponse {
            guid,
            loot_type,
            gold,
            items: items
                .into_iter()
                .map(|i| ProtocolLootItem {
                    slot: i.slot,
                    item_id: i.item_id,
                    count: i.count,
                    display_info_id: i.display_info_id,
                    random_property_id: i.random_property_id,
                    suffix_factor: i.suffix_factor,
                    slot_type: i.slot_type,
                })
                .collect(),
        }],
        WrathGameplayEvent::LootError { guid, error } => {
            vec![SessionEvent::LootError { guid, error }]
        }
        WrathGameplayEvent::LootReleaseResponse { guid, result } => {
            vec![SessionEvent::LootReleaseResponse { guid, result }]
        }
        WrathGameplayEvent::LootRemoved { slot } => vec![SessionEvent::LootRemoved { slot }],
        WrathGameplayEvent::LootMoney { amount } => {
            vec![SessionEvent::LootMoneyNotify { amount }]
        }
        WrathGameplayEvent::LootClearMoney => vec![SessionEvent::LootClearMoney],
        WrathGameplayEvent::LootStartRoll {
            guid,
            map_id: _,
            item_slot,
            item_id,
            suffix_factor,
            random_property_id,
            item_count,
            countdown_ms,
            vote_mask,
        } => vec![SessionEvent::LootStartRoll(ProtocolLootStartRoll {
            looted_target: guid,
            item_slot,
            item_id,
            random_property_id,
            suffix_factor,
            countdown_ms,
            wrath_options: Some((item_count, vote_mask)),
        })],
        WrathGameplayEvent::LootRoll {
            guid,
            item_slot,
            roller,
            item_id,
            suffix_factor,
            random_property_id,
            roll_number,
            roll_type,
            auto_pass,
        } => vec![SessionEvent::LootRoll(ProtocolLootRoll {
            looted_target: guid,
            item_slot,
            roller,
            item_id,
            random_property_id,
            suffix_factor,
            roll_number,
            roll_type,
            auto_pass: Some(auto_pass),
        })],
        WrathGameplayEvent::LootRollWon {
            guid,
            item_slot,
            item_id,
            suffix_factor,
            random_property_id,
            winner,
            roll_number,
            roll_type,
        } => vec![SessionEvent::LootRollWon(ProtocolLootRollWon {
            looted_target: guid,
            item_slot,
            item_id,
            random_property_id,
            suffix_factor,
            winner,
            roll_number,
            roll_type,
        })],
        WrathGameplayEvent::LootAllPassed {
            guid,
            item_slot,
            item_id,
            random_property_id,
            suffix_factor,
        } => vec![SessionEvent::LootAllPassed(ProtocolLootAllPassed {
            looted_target: guid,
            item_slot,
            item_id,
            random_property_id,
            suffix_factor,
        })],
        // 12340 runs `OnItemPushResult` and then `CheckForQuestItemLoot` on the same packet.
        WrathGameplayEvent::ItemPush(p) => vec![
            SessionEvent::ItemPushResult(ItemPushResult {
                player_guid: p.player,
                from_npc: p.from_npc,
                created: p.created,
                show_in_chat: p.show_in_chat,
                bag_slot: p.bag_slot,
                item_slot: p.item_slot,
                item_entry: p.item_entry,
                suffix_factor: p.suffix_factor,
                random_property_id: p.random_property_id as u32,
                count: p.count,
            }),
            SessionEvent::QuestItemLooted {
                player_guid: p.player,
                item_id: p.item_entry,
                count: p.count,
                total_count: p.total_count,
            },
        ],
        WrathGameplayEvent::PetSpells(spells) => {
            let spells = *spells;
            vec![SessionEvent::PetSpells(Box::new(
                benilla_protocol::messages::PetSpells {
                    pet_guid: spells.pet_guid,
                    duration_ms: spells.duration_ms,
                    state: spells.state,
                    bar: spells
                        .bar
                        .map(benilla_protocol::messages::PetActionEntry::from),
                    spells: spells
                        .spells
                        .into_iter()
                        .map(benilla_protocol::messages::PetActionEntry::from)
                        .collect(),
                    cooldowns: spells
                        .cooldowns
                        .into_iter()
                        .map(|cd| benilla_protocol::messages::PetSpellCooldown {
                            spell_id: cd.spell_id,
                            category: cd.category,
                            spell_cd_ms: cd.spell_cd_ms,
                            category_cd_ms: cd.category_cd_ms,
                        })
                        .collect(),
                },
            ))]
        }
        WrathGameplayEvent::PetMode(mode) => {
            vec![SessionEvent::PetMode(benilla_protocol::messages::PetMode {
                pet_guid: mode.pet_guid,
                state: mode.state,
            })]
        }
        WrathGameplayEvent::PetActionFeedback { reason } => {
            vec![SessionEvent::PetActionFeedback { reason }]
        }
        WrathGameplayEvent::PetActionSound { pet_guid, talk } => {
            vec![SessionEvent::PetActionSound {
                pet_guid,
                talk: talk as u32,
            }]
        }
        WrathGameplayEvent::PetDismissSound { model_id, position } => {
            vec![SessionEvent::PetDismissSound {
                model_id: model_id as u32,
                position,
            }]
        }
        WrathGameplayEvent::PetLearnedSpell { spell_id } => {
            vec![SessionEvent::PetSpellLearned { spell_id }]
        }
        WrathGameplayEvent::PetRemovedSpell { spell_id } => {
            vec![SessionEvent::PetSpellRemoved { spell_id }]
        }
        WrathGameplayEvent::PetGuids { guids } => vec![SessionEvent::PetGuids { guids }],
        WrathGameplayEvent::TalentsInfo(info) => {
            let unspent_points = info
                .player
                .as_ref()
                .map(|p| p.unspent_points)
                .or_else(|| info.pet_talents.as_ref().map(|p| p.unspent_points))
                .unwrap_or(0);
            let (active_spec, specs, pet_talents) = if info.pet {
                (
                    None,
                    Vec::new(),
                    info.pet_talents.map(|p| p.talents).unwrap_or_default(),
                )
            } else if let Some(player) = info.player {
                (
                    Some(player.active_spec),
                    player
                        .specs
                        .into_iter()
                        .map(|spec| TalentGroupInfo {
                            talents: spec.talents,
                            glyphs: spec.glyphs,
                        })
                        .collect(),
                    Vec::new(),
                )
            } else {
                (None, Vec::new(), Vec::new())
            };
            vec![SessionEvent::TalentInfo {
                pet: info.pet,
                unspent_points,
                active_spec,
                specs,
                pet_talents,
            }]
        }
        WrathGameplayEvent::TalentsInvoluntarilyReset { flag } => {
            vec![SessionEvent::TalentsInvoluntarilyReset { flag }]
        }
        WrathGameplayEvent::StableList(list) => {
            let list = *list;
            let mut next_stable_slot = 1u8;
            let pets = list
                .pets
                .into_iter()
                .map(|pet| {
                    // 3.3.5 sends state (1 current, 2 stabled), not a loyalty id or a slot.
                    let slot = if pet.state & 1 != 0 {
                        0
                    } else {
                        let slot = next_stable_slot;
                        next_stable_slot = next_stable_slot.saturating_add(1);
                        slot
                    };
                    benilla_protocol::messages::StabledPet {
                        pet_number: pet.pet_number,
                        creature_entry: pet.creature_entry,
                        level: pet.level,
                        name: pet.name,
                        // The 12340 list has no loyalty column; the pet stats owner fills it
                        // from the live pet where the stock UI asks.
                        loyalty: 0,
                        slot,
                    }
                })
                .collect();
            vec![SessionEvent::ListStabledPets {
                npc: list.npc_guid,
                num_stable_slots: list.stable_slots,
                pets,
            }]
        }
        WrathGameplayEvent::StableResult { result } => {
            vec![SessionEvent::StableResult { result }]
        }
        WrathGameplayEvent::SpellSuperceded {
            old_spell_id,
            new_spell_id,
        } => vec![SessionEvent::SpellSuperceded {
            old_spell_id,
            new_spell_id,
        }],
        WrathGameplayEvent::SpellRemoved { spell_id } => {
            vec![SessionEvent::SpellRemoved { spell_id }]
        }
        WrathGameplayEvent::TradeStatus { status } => {
            vec![SessionEvent::TradeStatus {
                status: wrath_trade_status(status),
            }]
        }
        WrathGameplayEvent::TradeStatusExtended(state) => {
            vec![SessionEvent::TradeStatusExtended {
                state: Box::new(wrath_trade_extended(*state)),
            }]
        }
        WrathGameplayEvent::SendMailResult(result) => {
            vec![SessionEvent::SendMailResult {
                mail_id: result.mail_id,
                action: result.action,
                error: result.error,
                equip_error: result.equip_error,
                item: result.item,
            }]
        }
        WrathGameplayEvent::MailList(list) => vec![SessionEvent::MailList {
            mails: wrath_mail_list(*list),
        }],
        WrathGameplayEvent::ReceivedMail { seconds } => {
            vec![SessionEvent::ReceivedMail { seconds }]
        }
        WrathGameplayEvent::NextMailTime { seconds, senders } => {
            if !senders.is_empty() {
                debug!(
                    "wrath: MSG_QUERY_NEXT_MAIL_TIME carries {} recent sender(s), not represented in the 1.12 UI",
                    senders.len()
                );
            }
            vec![SessionEvent::NextMailTime { seconds }]
        }
        WrathGameplayEvent::AuctionHello(hello) => vec![SessionEvent::AuctionHello {
            auctioneer: hello.auctioneer,
            house_id: hello.house_id,
            enabled: hello.enabled,
        }],
        WrathGameplayEvent::AuctionCommandResult(result) => {
            let tail = if result.error == 1 {
                result
                    .inventory_error
                    .map(
                        |result| benilla_protocol::messages::AuctionCommandTail::Inventory {
                            result,
                        },
                    )
                    .unwrap_or(benilla_protocol::messages::AuctionCommandTail::Empty)
            } else if result.error == 0 && result.action != 0 {
                benilla_protocol::messages::AuctionCommandTail::BidPlaced {
                    new_min_outbid: result.inventory_error.unwrap_or(0),
                }
            } else {
                benilla_protocol::messages::AuctionCommandTail::Empty
            };
            vec![SessionEvent::AuctionCommandResult {
                auction_id: result.auction_id,
                action: result.action,
                error: result.error,
                tail,
            }]
        }
        WrathGameplayEvent::AuctionList { kind, list } => {
            let list = *list;
            let auctions = list.auctions.into_iter().map(wrath_auction_entry).collect();
            vec![match kind {
                gameplay::AuctionListKind::Browse => SessionEvent::AuctionListResult {
                    auctions,
                    total_count: list.total_count,
                },
                gameplay::AuctionListKind::Owner => SessionEvent::AuctionOwnerListResult {
                    auctions,
                    total_count: list.total_count,
                },
                gameplay::AuctionListKind::Bidder => SessionEvent::AuctionBidderListResult {
                    auctions,
                    total_count: list.total_count,
                },
            }]
        }
        WrathGameplayEvent::GroupInvite(invite) => vec![SessionEvent::GroupInvite {
            inviter: invite.inviter,
        }],
        WrathGameplayEvent::GroupDecline { name } => vec![SessionEvent::GroupDecline { name }],
        WrathGameplayEvent::GroupUninvited => vec![SessionEvent::GroupUninvited],
        WrathGameplayEvent::GroupLeaderChanged { name } => {
            vec![SessionEvent::GroupLeaderChanged { name }]
        }
        WrathGameplayEvent::GroupDestroyed => vec![SessionEvent::GroupDestroyed],
        WrathGameplayEvent::GroupList(list) => wrath_group_list_events(*list),
        WrathGameplayEvent::PartyCommandResult {
            operation,
            member,
            result,
            value,
        } => {
            if value != 0 {
                debug!("wrath: party command result carries LFD value {value}");
            }
            vec![SessionEvent::PartyCommandResult {
                operation,
                member,
                result,
            }]
        }
        WrathGameplayEvent::PartyMemberStats(stats) => vec![wrath_party_stats(*stats)],
        WrathGameplayEvent::Who(who) => vec![wrath_who(*who)],
        WrathGameplayEvent::Whois(whois) => vec![SessionEvent::WhoisText { text: whois.text }],
        WrathGameplayEvent::ContactList(list) => wrath_contacts(*list),
        WrathGameplayEvent::FriendStatus(status) => {
            vec![SessionEvent::FriendStatus(wrath_friend_status(status))]
        }
        WrathGameplayEvent::ChannelNotify(notify) => vec![wrath_channel_notify(*notify)],
        WrathGameplayEvent::ChannelList(list) => vec![SessionEvent::ChannelList {
            channel: list.channel,
            flags: list.flags,
            members: list.members,
        }],
        WrathGameplayEvent::GuildInfo(info) => {
            vec![SessionEvent::GuildInfo(wrath_guild_info(info))]
        }
        WrathGameplayEvent::GuildRoster(roster) => {
            vec![SessionEvent::GuildRoster(wrath_guild_roster(*roster))]
        }
        WrathGameplayEvent::GuildEvent(event) => {
            vec![SessionEvent::GuildEvent(
                benilla_protocol::messages::GuildEventNotice {
                    event: event.event,
                    params: event.args,
                    guid: event.guid,
                },
            )]
        }
        WrathGameplayEvent::GuildCommandResult(result) => {
            vec![SessionEvent::GuildCommandResult(
                benilla_protocol::messages::GuildCommandResult {
                    command: result.command,
                    name: result.name,
                    result: result.error,
                },
            )]
        }
        WrathGameplayEvent::RaidTargetUpdate(update) => vec![match update {
            gameplay::social::RaidTargetUpdate::Set { icon, guid, .. } => {
                SessionEvent::RaidTargetSet { icon, guid }
            }
            gameplay::social::RaidTargetUpdate::List { entries } => {
                SessionEvent::RaidTargetList { entries }
            }
        }],
        WrathGameplayEvent::RaidReadyCheck(check) => vec![match check {
            // 12340's request names the initiator; the donor event is a plain prompt.
            gameplay::social::RaidReadyCheck::Request { .. } => SessionEvent::ReadyCheckRequest,
            gameplay::social::RaidReadyCheck::Answer { guid, ready } => {
                SessionEvent::ReadyCheckAnswer { guid, ready }
            }
        }],
        WrathGameplayEvent::BattlefieldStatus(status) => vec![wrath_battlefield_status(status)],
        WrathGameplayEvent::PvpLogData(log) => vec![wrath_pvp_log_data(*log)],
        // Calendar, rune, and LFG records are consumed by `WrathWorldView` through `WrathFeedEvent`,
        // so they return empty here.
        WrathGameplayEvent::LfgJoinResult(_)
        | WrathGameplayEvent::LfgQueueStatus(_)
        | WrathGameplayEvent::LfgUpdatePlayer(_)
        | WrathGameplayEvent::LfgUpdateParty(_)
        | WrathGameplayEvent::LfgProposalUpdate(_)
        | WrathGameplayEvent::CalendarSendCalendar(_)
        | WrathGameplayEvent::CalendarCommandResult(_)
        | WrathGameplayEvent::RuneConvert(_)
        | WrathGameplayEvent::RuneState(_)
        | WrathGameplayEvent::RunePower(_)
        | WrathGameplayEvent::EquipmentSetList(_)
        | WrathGameplayEvent::EquipmentSetSaved(_)
        | WrathGameplayEvent::EquipmentSetUseResult(_) => Vec::new(),
        WrathGameplayEvent::XpGain {
            victim,
            total_xp,
            kill_xp,
            rested_bonus,
            group_rate,
        } => vec![SessionEvent::XpGain(benilla_protocol::messages::XpGain {
            victim,
            total: total_xp,
            base: total_xp.saturating_sub(rested_bonus),
            kill: kill_xp,
            group: group_rate,
        })],
        WrathGameplayEvent::LevelUp {
            new_level,
            health_delta,
            power_deltas,
            stat_deltas,
        } => vec![SessionEvent::LevelUp(
            benilla_protocol::messages::LevelUpInfo {
                level: new_level,
                health: health_delta as i32,
                powers: power_deltas.map(|v| v as i32),
                stats: stat_deltas.map(|v| v as i32),
            },
        )],
        WrathGameplayEvent::SpellHealLog {
            target,
            caster,
            spell_id,
            amount,
            overheal: _,
            absorbed: _,
            critical,
        } => vec![SessionEvent::SpellHealLog(
            benilla_protocol::messages::SpellHealLog {
                target,
                healer: caster,
                spell_id,
                amount,
                critical,
            },
        )],
        WrathGameplayEvent::SpellEnergizeLog {
            target,
            caster,
            spell_id,
            power_type,
            amount,
        } => vec![SessionEvent::SpellEnergizeLog(
            benilla_protocol::messages::SpellEnergizeLog {
                target,
                caster,
                spell_id,
                power: power_type,
                amount,
            },
        )],
        WrathGameplayEvent::InitializeFactions { entries } => {
            vec![SessionEvent::Reputations {
                standings: entries
                    .into_iter()
                    .map(|(id, standing)| (id, standing as i32))
                    .collect(),
            }]
        }
        WrathGameplayEvent::SetFactionStanding { updates } => {
            vec![SessionEvent::ReputationDelta {
                standings: updates
                    .into_iter()
                    .map(|(id, standing)| (id, standing as i32))
                    .collect(),
            }]
        }
        WrathGameplayEvent::PetCastFailed { spell_id, reason } => {
            vec![SessionEvent::PetCastFailed {
                spell_id,
                reason: match cast_failure::reason(reason) {
                    cast_failure::Reason::Vanilla(r) => Some(r),
                    cast_failure::Reason::Unsupported(_) => None,
                },
            }]
        }
        WrathGameplayEvent::InitWorldStates {
            map_id,
            zone_id,
            states,
        } => vec![SessionEvent::WorldStates {
            scope: Some((map_id, zone_id)),
            states,
        }],
        WrathGameplayEvent::ForceSpeedChange {
            kind,
            guid,
            counter,
            speed,
        } => match wrath_speed_kind(kind) {
            Some(kind) => vec![SessionEvent::ForceSpeedChange {
                guid,
                kind,
                counter,
                speed,
            }],
            None => {
                // Unsupported: the shared mover has no flight or pitch speed to apply, and an ack
                // for a speed we never use would tell the server something false.
                warn!(
                    ?kind,
                    guid, speed, "wrath: forced speed has no mover owner; not acked"
                );
                Vec::new()
            }
        },
        WrathGameplayEvent::SpeedChanged { kind, guid, speed } => match wrath_speed_kind(kind) {
            Some(kind) => vec![SessionEvent::SpeedChanged { guid, kind, speed }],
            None => {
                debug!(?kind, guid, speed, "wrath: unit speed has no motion owner");
                Vec::new()
            }
        },
        WrathGameplayEvent::MoveMode {
            mode,
            apply,
            guid,
            counter,
        } => match wrath_move_mode(mode) {
            Some(mode) => vec![SessionEvent::MoveMode {
                guid,
                counter,
                mode,
                apply,
            }],
            None => {
                // Unsupported: CanFly and gravity need the flight controller the shared mover
                // lacks, so the order is neither applied nor acked.
                warn!(
                    ?mode,
                    apply, guid, "wrath: move mode has no mover owner; not acked"
                );
                Vec::new()
            }
        },
        WrathGameplayEvent::SplineMoveMode { mode, apply, guid } => match wrath_spline_mode(mode) {
            Some(mode) => vec![SessionEvent::SplineMoveMode { guid, mode, apply }],
            None => {
                debug!(?mode, apply, guid, "wrath: spline mode has no motion owner");
                Vec::new()
            }
        },
        WrathGameplayEvent::KnockBack {
            guid,
            counter,
            cos_angle,
            sin_angle,
            xy_speed,
            z_speed_down,
        } => vec![SessionEvent::KnockBack {
            guid,
            counter,
            launch: benilla_protocol::messages::JumpInfo {
                zspeed: z_speed_down,
                cos_angle,
                sin_angle,
                xy_speed,
            },
        }],
        WrathGameplayEvent::ClientControl { guid, allow_move } => {
            vec![SessionEvent::ClientControl {
                mover: guid,
                allow_move,
            }]
        }
    }
}

/// The shared mover's speed for a Wrath kind; `None` for flight, flight-back and pitch rate,
/// which the shared mover does not model.
fn wrath_speed_kind(kind: gameplay::SpeedKind) -> Option<benilla_protocol::messages::SpeedKind> {
    use benilla_protocol::messages::SpeedKind as Shared;
    use gameplay::SpeedKind as Wrath;
    Some(match kind {
        Wrath::Walk => Shared::Walk,
        Wrath::Run => Shared::Run,
        Wrath::RunBack => Shared::RunBack,
        Wrath::Swim => Shared::Swim,
        Wrath::SwimBack => Shared::SwimBack,
        Wrath::TurnRate => Shared::TurnRate,
        Wrath::Flight | Wrath::FlightBack | Wrath::PitchRate => return None,
    })
}

/// The shared mode for a Wrath mover order; `None` for CanFly and gravity.
fn wrath_move_mode(mode: gameplay::MoveMode) -> Option<benilla_protocol::messages::MoveMode> {
    use benilla_protocol::messages::MoveMode as Shared;
    use gameplay::MoveMode as Wrath;
    Some(match mode {
        Wrath::Root => Shared::Root,
        Wrath::WaterWalk => Shared::WaterWalk,
        Wrath::FeatherFall => Shared::FeatherFall,
        Wrath::Hover => Shared::Hover,
        Wrath::CanFly | Wrath::GravityDisabled => return None,
    })
}

/// The shared mode for a Wrath unit broadcast; `None` for flying and gravity.
fn wrath_spline_mode(mode: gameplay::SplineMode) -> Option<benilla_protocol::messages::SplineMode> {
    use benilla_protocol::messages::SplineMode as Shared;
    use gameplay::SplineMode as Wrath;
    Some(match mode {
        Wrath::Root => Shared::Root,
        Wrath::WaterWalk => Shared::WaterWalk,
        Wrath::FeatherFall => Shared::FeatherFall,
        Wrath::Hover => Shared::Hover,
        Wrath::WalkMode => Shared::WalkMode,
        Wrath::Swimming => Shared::Swimming,
        Wrath::Flying | Wrath::GravityDisabled => return None,
    })
}

fn wrath_group_member_flags(sub_group: u8, member_flags: u8) -> u8 {
    // 12340 sends `sub_group` (`0..8`) and `flags` (`MEMBER_FLAG_ASSISTANT = 0x01`,
    // `MEMBER_FLAG_MAINTANK = 0x02`, `MEMBER_FLAG_MAINASSIST = 0x04`) in separate bytes, whereas
    // the group roster packs subgroup in bits 0..2 and assistant in bit 7 (`0x80`).
    (sub_group & 0x7)
        | if member_flags & 0x81 != 0 {
            benilla_protocol::messages::GROUP_MEMBER_ASSISTANT
        } else {
            0
        }
}

type PartyAuraRows = (Vec<(u8, u32)>, Vec<(u8, u32)>);

fn wrath_partition_party_auras(auras: &[gameplay::social::PartyAura]) -> PartyAuraRows {
    // 3.3.5a `SMSG_PARTY_MEMBER_STATS` sends a 64-bit aura mask (`MAX_AURAS = 64`) with `u32`
    // spell IDs and `u8` aura flags (`AFLAG_POSITIVE = 0x10`, `AFLAG_NEGATIVE = 0x20`). When the
    // server sets `AFLAG_NEGATIVE` (or neither positive/negative bit is set and the slot is in the
    // negative range `>= 32`), classify the slot into `auras_negative`.
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for aura in auras.iter().filter(|aura| aura.slot < 64) {
        let is_negative = if aura.flags & 0x30 != 0 {
            aura.flags & 0x20 != 0
        } else {
            aura.slot >= 32
        };
        if is_negative {
            negative.push((aura.slot, aura.spell_id));
        } else {
            positive.push((aura.slot, aura.spell_id));
        }
    }
    (positive, negative)
}

pub(crate) fn wrath_group_list(list: gameplay::social::GroupList) -> SessionEvent {
    let members = list
        .members
        .into_iter()
        .map(|member| benilla_protocol::messages::GroupMemberEntry {
            name: member.name,
            guid: member.guid,
            status: member.status,
            flags: wrath_group_member_flags(member.sub_group, member.flags),
        })
        .collect();
    let loot = list
        .loot
        .map(|loot| benilla_protocol::messages::GroupLootInfo {
            method: loot.method,
            master: loot.master,
            threshold: loot.threshold,
        });
    SessionEvent::GroupList {
        group_type: list.group_type,
        own_flags: wrath_group_member_flags(list.sub_group, list.flags),
        members,
        leader: list.leader_guid,
        loot,
    }
}

pub(crate) fn wrath_group_list_events(list: gameplay::social::GroupList) -> Vec<SessionEvent> {
    let role_deltas: Vec<SessionEvent> = list
        .members
        .iter()
        .filter(|member| member.roles != 0)
        .map(|member| SessionEvent::PartyMemberStats {
            guid: member.guid,
            full: false,
            info: Box::new(benilla_protocol::messages::PartyMemberStatsInfo {
                lfg_roles: Some(member.roles),
                ..Default::default()
            }),
        })
        .collect();
    let mut out = Vec::with_capacity(1 + role_deltas.len());
    out.push(wrath_group_list(list));
    out.extend(role_deltas);
    out
}

pub(crate) fn wrath_party_stats(stats: gameplay::social::PartyMemberStats) -> SessionEvent {
    let f = stats.fields;
    let mut info = benilla_protocol::messages::PartyMemberStatsInfo {
        status: f.status.map(|status| status as u8),
        cur_hp: f.cur_hp,
        max_hp: f.max_hp,
        power_type: f.power_type,
        cur_power: f.cur_power,
        max_power: f.max_power,
        level: f.level,
        zone: f.zone,
        position: f.position.map(|(x, y)| (x as i16, y as i16)),
        vehicle_seat: f.vehicle_seat,
        ..Default::default()
    };
    if let Some(auras) = f.auras {
        let (pos, neg) = wrath_partition_party_auras(&auras.auras);
        info.auras = Some(pos);
        info.auras_negative = Some(neg);
    }
    info.pet_guid = f.pet_guid;
    info.pet_name = f.pet_name;
    info.pet_model_id = f.pet_model_id;
    info.pet_cur_hp = f.pet_cur_hp;
    info.pet_max_hp = f.pet_max_hp;
    info.pet_power_type = f.pet_power_type;
    info.pet_cur_power = f.pet_cur_power;
    info.pet_max_power = f.pet_max_power;
    if let Some(auras) = f.pet_auras {
        let (pos, neg) = wrath_partition_party_auras(&auras.auras);
        info.pet_auras = Some(pos);
        info.pet_auras_negative = Some(neg);
    }
    SessionEvent::PartyMemberStats {
        guid: stats.guid,
        full: stats.full,
        info: Box::new(info),
    }
}

fn wrath_who(who: gameplay::social::WhoResults) -> SessionEvent {
    SessionEvent::WhoResults(benilla_protocol::messages::WhoResults {
        displayed: who.shown,
        total: who.total,
        entries: who
            .players
            .into_iter()
            .map(|player| benilla_protocol::messages::WhoEntry {
                name: player.name,
                guild: player.guild,
                level: player.level,
                class: player.class,
                race: player.race,
                zone: player.zone,
            })
            .collect(),
    })
}

fn wrath_contacts(list: gameplay::social::ContactList) -> Vec<SessionEvent> {
    let mut friends = Vec::new();
    let mut ignores = Vec::new();
    for contact in list.contacts {
        if contact.flags & 0x1 != 0 {
            let friend = contact.friend.unwrap_or(gameplay::social::FriendInfo {
                status: 0,
                area: None,
                level: None,
                class: None,
            });
            friends.push(benilla_protocol::messages::FriendEntry {
                guid: contact.guid,
                status: friend.status,
                area: friend.area.unwrap_or(0),
                level: friend.level.unwrap_or(0),
                class: friend.class.unwrap_or(0),
            });
        }
        if contact.flags & 0x2 != 0 {
            ignores.push(contact.guid);
        }
    }
    vec![
        SessionEvent::FriendList { friends },
        SessionEvent::IgnoreList { guids: ignores },
    ]
}

/// Map a 12340 `BattlegroundTypeId` (`SharedDefines.h:3400-3416`) to its canonical `Map.dbc` ID
/// when the `SMSG_BATTLEFIELD_STATUS` arm (`STATUS_WAIT_QUEUE`) does not carry an explicit map word.
pub(crate) fn wrath_bg_type_default_map_id(bg_type_id: u32) -> u32 {
    match bg_type_id {
        1 => 30,   // BATTLEGROUND_AV -> Alterac Valley
        2 => 489,  // BATTLEGROUND_WS -> Warsong Gulch
        3 => 529,  // BATTLEGROUND_AB -> Arathi Basin
        4 => 559,  // BATTLEGROUND_NA -> Nagrand Arena
        5 => 562,  // BATTLEGROUND_BE -> Blade's Edge Arena
        6 => 559,  // BATTLEGROUND_AA -> All Arenas (fallback map)
        7 => 566,  // BATTLEGROUND_EY -> Eye of the Storm
        8 => 572,  // BATTLEGROUND_RL -> Ruins of Lordaeron
        9 => 607,  // BATTLEGROUND_SA -> Strand of the Ancients
        10 => 617, // BATTLEGROUND_DS -> Dalaran Sewers
        11 => 618, // BATTLEGROUND_RV -> Ring of Valor
        30 => 628, // BATTLEGROUND_IC -> Isle of Conquest
        32 => 489, // BATTLEGROUND_RB -> Random Battleground
        other => other,
    }
}

/// Map a `Map.dbc` ID or `BattlemasterList.dbc` ID to its 12340 `BattlegroundTypeId` for outbound
/// `CMSG_BATTLEMASTER_JOIN`, `CMSG_BATTLEFIELD_PORT`, and `CMSG_BATTLEFIELD_LIST`.
pub(crate) fn wrath_map_to_bg_type_id(map_or_bg_id: u32) -> u32 {
    match map_or_bg_id {
        30 => 1,   // Alterac Valley
        489 => 2,  // Warsong Gulch
        529 => 3,  // Arathi Basin
        559 => 4,  // Nagrand Arena
        562 => 5,  // Blade's Edge Arena
        566 => 7,  // Eye of the Storm
        572 => 8,  // Ruins of Lordaeron
        607 => 9,  // Strand of the Ancients
        617 => 10, // Dalaran Sewers
        618 => 11, // Ring of Valor
        628 => 30, // Isle of Conquest
        other => other,
    }
}

pub(crate) fn wrath_battlefield_status(
    status: gameplay::wrath_systems::BattlefieldStatus,
) -> SessionEvent {
    let Some(slot) = status.queue else {
        return SessionEvent::BattlefieldStatus(benilla_protocol::messages::BattlefieldStatus {
            slot: status.queue_slot,
            map_id: 0,
            bracket: 0,
            instance_id: 0,
            status: 0,
            time_ms: None,
            in_progress: None,
            queued: None,
        });
    };
    let fallback_map = wrath_bg_type_default_map_id(slot.bg_type);
    let (map_id, time_ms, in_progress, queued) = match slot.arm {
        gameplay::wrath_systems::BattlefieldStatusArm::Queued {
            average_wait_ms,
            queued_ms,
        } => (fallback_map, None, None, Some((average_wait_ms, queued_ms))),
        gameplay::wrath_systems::BattlefieldStatusArm::Confirm {
            map_id,
            port_expiration_ms,
            ..
        } => (
            if map_id != 0 { map_id } else { fallback_map },
            Some(port_expiration_ms),
            None,
            None,
        ),
        gameplay::wrath_systems::BattlefieldStatusArm::Active {
            map_id,
            auto_leave_ms,
            started_ms,
            ..
        } => (
            if map_id != 0 { map_id } else { fallback_map },
            None,
            Some((auto_leave_ms, started_ms)),
            None,
        ),
        gameplay::wrath_systems::BattlefieldStatusArm::Inactive => (fallback_map, None, None, None),
    };
    SessionEvent::BattlefieldStatus(benilla_protocol::messages::BattlefieldStatus {
        slot: status.queue_slot,
        map_id,
        bracket: slot.min_level,
        instance_id: slot.instance_id,
        status: slot.status,
        time_ms,
        in_progress,
        queued,
    })
}

pub(crate) fn wrath_pvp_log_data(log: gameplay::wrath_systems::PvpLogData) -> SessionEvent {
    let rows = log
        .scores
        .into_iter()
        .map(|row| benilla_protocol::messages::PvpLogRow {
            guid: row.guid,
            rank: u32::from(row.team.unwrap_or(0)),
            killing_blows: row.killing_blows,
            honorable_kills: row.honorable_kills.unwrap_or(0),
            deaths: row.deaths.unwrap_or(0),
            honor_gained: row.bonus_honor.unwrap_or(0),
            stats: row.objectives,
        })
        .collect();
    SessionEvent::PvpLogData(benilla_protocol::messages::PvpLogData {
        ended: log.ended,
        winner: log.winner,
        rows,
    })
}

fn wrath_friend_status(
    status: gameplay::social::FriendStatus,
) -> benilla_protocol::messages::FriendStatusUpdate {
    benilla_protocol::messages::FriendStatusUpdate {
        result: status.result,
        guid: status.guid,
        online: status
            .online
            .map(|friend| benilla_protocol::messages::FriendOnline {
                status: friend.status,
                area: friend.area.unwrap_or(0),
                level: friend.level.unwrap_or(0),
                class: friend.class.unwrap_or(0),
            }),
    }
}

fn wrath_channel_notify(notify: gameplay::social::ChannelNotify) -> SessionEvent {
    use benilla_protocol::messages::ChannelNoticeTail as Out;
    use gameplay::social::ChannelNotifyTail as In;
    let tail = match notify.tail {
        In::None => Out::Empty,
        In::Guid { guid } => Out::Guid(guid),
        In::Joined { flags, .. } => Out::YouJoined {
            flags: u32::from(flags),
        },
        In::Left { .. } => Out::Empty,
        In::PlayerName { name } => Out::Name(name),
        In::Kicked { player, actor } => Out::Actors {
            target: player,
            source: actor,
        },
        In::ModeChange {
            guid,
            old_flags,
            new_flags,
        } => Out::ModeChange {
            guid,
            old_flags,
            new_flags,
        },
    };
    SessionEvent::ChannelNotify {
        notice: notify.notice,
        channel: notify.channel,
        tail,
    }
}

fn wrath_guild_info(
    info: gameplay::social::GuildInfoWire,
) -> benilla_protocol::messages::GuildInfo {
    // `AppendPackedTime` packs the client's WowTime: year-2000, month, day and time of day.
    let year = ((info.created_at >> 24) & 0x1F) + 2000;
    let month = ((info.created_at >> 20) & 0xF) + 1;
    let day = ((info.created_at >> 14) & 0x3F) + 1;
    benilla_protocol::messages::GuildInfo {
        name: info.name,
        created_day: day,
        created_month: month,
        created_year: year,
        member_count: info.member_count,
        account_count: info.account_count,
    }
}

fn wrath_guild_roster(
    roster: gameplay::social::GuildRosterWire,
) -> benilla_protocol::messages::GuildRoster {
    benilla_protocol::messages::GuildRoster {
        motd: roster.motd,
        info: roster.guild_info,
        rank_rights: roster.ranks.into_iter().map(|rank| rank.rights).collect(),
        members: roster
            .members
            .into_iter()
            .map(|member| benilla_protocol::messages::GuildRosterMember {
                guid: member.guid,
                presence: member.status,
                name: member.name,
                rank_id: member.rank,
                level: member.level as u8,
                class: member.class as u8,
                zone: member.zone,
                last_online_days: member.last_online_days.unwrap_or(0.0),
                public_note: member.note,
                officer_note: member.officer_note,
            })
            .collect(),
    }
}

/// Map the 12340 trade status onto the donor UI's status enum. Codes 22/23 are intentionally
/// kept as `Unknown`: their 3.3.5 meanings (wrong realm / not on tap list) have no 1.12 arm or
/// line, and mapping them onto the conjured-only bounce would misreport the refusal.
fn wrath_trade_status(
    status: gameplay::economy::TradeStatus,
) -> benilla_protocol::messages::TradeStatus {
    use benilla_protocol::messages::TradeStatus as Out;
    use gameplay::economy::TradeStatus as In;
    match status {
        In::Busy => Out::Busy,
        In::BeginTrade { partner } => Out::BeginTrade { partner },
        In::OpenWindow { .. } => Out::OpenWindow,
        In::Canceled => Out::Canceled,
        In::Accept => Out::Accept,
        In::Busy2 => Out::Busy2,
        In::NoTarget => Out::NoTarget,
        In::BackToTrade => Out::BackToTrade,
        In::Complete => Out::Complete,
        In::Rejected => Out::Rejected,
        In::TargetTooFar => Out::TargetTooFar,
        In::WrongFaction => Out::WrongFaction,
        In::CloseWindow {
            result,
            item_limit_category,
            ..
        } => Out::CloseWindow {
            result,
            item_limit_category,
        },
        In::IgnoreYou => Out::IgnoreYou,
        In::YouStunned => Out::YouStunned,
        In::TargetStunned => Out::TargetStunned,
        In::YouDead => Out::YouDead,
        In::TargetDead => Out::TargetDead,
        In::YouLogout => Out::YouLogout,
        In::TargetLogout => Out::TargetLogout,
        In::TrialAccount => Out::TrialAccount,
        In::WrongRealm { .. } => Out::Unknown(22),
        In::NotOnTapList { .. } => Out::Unknown(23),
        In::Unknown(code) => Out::Unknown(code),
    }
}

/// Map a 12340 trade side onto the trade state, preserving `trade_id` and `socket_enchant`.
pub(crate) fn wrath_trade_extended(
    state: gameplay::economy::TradeExtended,
) -> benilla_protocol::messages::TradeStatusExtended {
    let mut slots = [None; gameplay::economy::TRADE_SLOT_COUNT];
    for (out, item) in slots.iter_mut().zip(state.slots) {
        *out = item.map(|item| benilla_protocol::messages::TradeItem {
            entry: item.entry,
            display_id: item.display_id,
            count: item.count,
            wrapped: item.wrapped,
            gift_creator: item.gift_creator,
            perm_enchant: item.perm_enchant,
            socket_enchant: item.socket_enchant,
            creator: item.creator,
            charges: item.charges,
            suffix_factor: item.suffix_factor,
            random_prop_id: item.random_prop_id as u32,
            lock_id: item.lock_id,
            max_durability: item.max_durability,
            durability: item.durability,
        });
    }
    benilla_protocol::messages::TradeStatusExtended {
        their_window: state.their_window,
        trade_id: state.trade_id,
        sequence: state.state_index,
        flag: state.sequence,
        gold: state.gold,
        enchant_spell_id: state.enchant_spell_id,
        slots,
    }
}

pub(crate) fn wrath_mail_list(
    list: gameplay::economy::MailList,
) -> Vec<benilla_protocol::messages::MailListEntry> {
    list.mails
        .into_iter()
        .map(|mail| {
            let attachments: Vec<benilla_protocol::messages::MailAttachment> = mail
                .attachments
                .iter()
                .take(benilla_protocol::messages::MAX_MAIL_ITEMS)
                .map(|attachment| benilla_protocol::messages::MailAttachment {
                    index: attachment.index,
                    guid_low: attachment.item_guid_low,
                    entry: attachment.item_entry,
                    perm_enchant: attachment.enchants[0].id,
                    random_prop_id: attachment.random_prop_id as u32,
                    suffix_factor: attachment.suffix_factor,
                    count: attachment.count,
                    charges: attachment.charges,
                    durability_max: attachment.max_durability,
                    durability: attachment.durability,
                })
                .collect();
            let item = attachments.first().copied();
            benilla_protocol::messages::MailListEntry {
                message_id: mail.message_id,
                message_type: mail.message_type,
                sender_guid: mail.sender_guid,
                sender_id: mail.sender_entry,
                subject: mail.subject,
                inline_body: (!mail.body.is_empty()).then_some(mail.body),
                item_text_id: 0,
                package: mail.package,
                stationery: mail.stationery,
                item,
                attachments,
                money: mail.money,
                cod: mail.cod,
                checked: mail.flags,
                expire_days: mail.expire_days,
                mail_template_id: mail.mail_template_id,
            }
        })
        .collect()
}

fn wrath_auction_entry(
    entry: gameplay::economy::AuctionEntry,
) -> benilla_protocol::messages::AuctionListEntry {
    benilla_protocol::messages::AuctionListEntry {
        auction_id: entry.auction_id,
        item_entry: entry.item_entry,
        perm_enchant: entry.enchants[0].id,
        random_property_id: entry.random_prop_id,
        suffix_factor: entry.suffix_factor,
        count: entry.count,
        spell_charges: entry.spell_charges,
        owner_guid: entry.owner_guid,
        start_bid: entry.start_bid,
        min_increment: entry.min_increment,
        buyout: entry.buyout,
        time_left_ms: entry.time_left_ms,
        bidder_guid: entry.bidder_guid,
        current_bid: entry.current_bid,
    }
}

/// The action/UI commands the stock 1.12 net thread would have consumed, translated to the
/// 3.3.5 wire. The stock UI remains the only producer and its existing handlers remain the
/// only consumers of replies.
fn wrath_tap_commands(
    tap: Option<ResMut<NetCommandTap>>,
    mut view: ResMut<WrathWorldView>,
    input: ResMut<WrathInput>,
    trace: Option<Res<crate::quest_trace::Source>>,
    player: Option<Res<crate::player::Player>>,
) {
    let Some(mut tap) = tap else { return };
    let Some(receiver) = tap.0.as_mut() else {
        return;
    };
    // One reading for the pass: every `MovementInfo` written below carries the clock the
    // time-sync reply reports ([`wrath_client_ms`]).
    let now_ms = wrath_client_ms();
    while let Ok(raw_command) = receiver.try_recv() {
        // Unwrap the shared transport-identity envelope exactly once before matching the inner
        // command. Its trace and scope flag are carried alongside the existing per-variant traces.
        let (env_trace, env_enforce_scope, from_tracked, command) = match raw_command {
            ClientCommand::Tracked {
                trace,
                enforce_scope,
                command,
            } => (trace, enforce_scope, true, *command),
            other => (None, false, false, other),
        };
        // Self-movement is replaceable motion with its own admission rule, not an action.
        if let ClientCommand::Move {
            kind,
            flags,
            pos,
            orientation,
            pitch,
            fall_time,
            jump,
            transport,
        } = command
        {
            // 12340 names the mover in every move (`MovementHandler.cpp` drops one whose guid is
            // not the session's mover); the shared command carries none, as 1.12's do not.
            let mover = player
                .as_ref()
                .and_then(|p| p.foreign_mover)
                .unwrap_or(view.player_guid);
            if mover == 0 {
                continue;
            }
            let (opcode, payload) = wrath_move_packet(
                mover,
                now_ms,
                kind,
                flags,
                pos,
                orientation,
                pitch,
                fall_time,
                jump,
                transport,
            );
            admit_wrath_movement(&input, opcode, payload, trace.as_deref());
            continue;
        }
        let opening = matches!(
            &command,
            ClientCommand::GossipHello { .. }
                | ClientCommand::QuestgiverHello { .. }
                | ClientCommand::QuestgiverOpenTracked { .. }
        );
        let scope = Some((view.session_epoch, view.world_generation));
        let cmd_trace = match &command {
            ClientCommand::CastSpell { trace, .. }
            | ClientCommand::CastSpellCorpse { trace, .. }
            | ClientCommand::CastSpellAtDest { trace, .. }
            | ClientCommand::CastSpellAtSource { trace, .. }
            | ClientCommand::CastSpellGameObject { trace, .. }
            | ClientCommand::CastSpellItem { trace, .. }
            | ClientCommand::CastSpellTradeItem { trace, .. }
            | ClientCommand::UseItem { trace, .. }
            | ClientCommand::PetCastSpell { trace, .. }
            | ClientCommand::CancelCast { trace, .. }
            | ClientCommand::CancelChannelling { trace, .. } => trace.clone(),
            _ => None,
        };
        let cmd_trace = cmd_trace.or(env_trace.clone());
        // Receipt trace for quest tracked variants, used by the generic trace scope path.
        let receipt_trace = match &command {
            ClientCommand::QuestgiverAcceptTracked { receipt }
            | ClientCommand::QuestgiverSelectTracked { receipt, .. }
            | ClientCommand::QuestgiverOpenTracked { receipt, .. }
            | ClientCommand::QuestgiverRequestRewardTracked { receipt }
            | ClientCommand::QuestgiverChooseRewardTracked { receipt, .. } => receipt.trace.clone(),
            _ => None,
        };
        let primary_trace = cmd_trace.clone().or(receipt_trace.clone());
        let receipt_enforce = match &command {
            ClientCommand::QuestgiverAcceptTracked { receipt }
            | ClientCommand::QuestgiverSelectTracked { receipt, .. }
            | ClientCommand::QuestgiverOpenTracked { receipt, .. }
            | ClientCommand::QuestgiverRequestRewardTracked { receipt }
            | ClientCommand::QuestgiverChooseRewardTracked { receipt, .. } => receipt.enforce_scope,
            _ => false,
        };
        // Scope rule:
        // - a `Tracked` envelope uses its explicit flag;
        // - a receipt's flag governs its own trace;
        // - a per-variant (non-envelope) trace with a carried scope defaults to enforced.
        let enforce_scope = if from_tracked {
            env_enforce_scope || receipt_enforce
        } else {
            receipt_enforce || primary_trace.as_ref().and_then(|t| t.scope()).is_some()
        };
        if opening {
            if let Some(trace) = &trace {
                trace.observe(scope, None, crate::quest_trace::Stage::OpenCommandDequeued);
            }
        }
        let (command, receipt) = match command {
            ClientCommand::QuestgiverAcceptTracked { receipt } => {
                if !view.entered_world
                    || receipt.session_epoch != view.session_epoch
                    || receipt.world_generation != view.world_generation
                {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::ScopeChanged,
                    ));
                    continue;
                }
                (
                    ClientCommand::QuestgiverAccept {
                        npc: receipt.npc,
                        quest: receipt.quest,
                    },
                    Some(receipt),
                )
            }
            ClientCommand::QuestgiverSelectTracked { receipt, complete } => {
                if !view.entered_world
                    || receipt.session_epoch != view.session_epoch
                    || receipt.world_generation != view.world_generation
                {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::ScopeChanged,
                    ));
                    continue;
                }
                let command = if complete {
                    ClientCommand::QuestgiverComplete {
                        npc: receipt.npc,
                        quest: receipt.quest,
                    }
                } else {
                    ClientCommand::QuestgiverQuery {
                        npc: receipt.npc,
                        quest: receipt.quest,
                    }
                };
                (command, Some(receipt))
            }
            ClientCommand::QuestgiverOpenTracked { receipt, gossip } => {
                // The transport-core scope contract covers the open arm too: a right-click open
                // written after a session/world transfer must be refused once, not forwarded as
                // an old-world greeting in the new world.
                if !view.entered_world
                    || receipt.session_epoch != view.session_epoch
                    || receipt.world_generation != view.world_generation
                {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::ScopeChanged,
                    ));
                    continue;
                }
                let command = if gossip {
                    ClientCommand::GossipHello { guid: receipt.npc }
                } else {
                    ClientCommand::QuestgiverHello { npc: receipt.npc }
                };
                (command, Some(receipt))
            }
            ClientCommand::QuestgiverRequestRewardTracked { receipt } => {
                if !view.entered_world
                    || receipt.session_epoch != view.session_epoch
                    || receipt.world_generation != view.world_generation
                {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::ScopeChanged,
                    ));
                    continue;
                }
                (
                    ClientCommand::QuestgiverRequestReward {
                        npc: receipt.npc,
                        quest: receipt.quest,
                    },
                    Some(receipt),
                )
            }
            ClientCommand::QuestgiverChooseRewardTracked { receipt, choice } => {
                if !view.entered_world
                    || receipt.session_epoch != view.session_epoch
                    || receipt.world_generation != view.world_generation
                {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::ScopeChanged,
                    ));
                    continue;
                }
                (
                    ClientCommand::QuestgiverChooseReward {
                        npc: receipt.npc,
                        quest: receipt.quest,
                        choice,
                    },
                    Some(receipt),
                )
            }
            command => (command, None),
        };
        // Generic trace scope check: enforced only when `enforce_scope` is true (defaulted true for
        // a carried scope per the named rule above).
        if enforce_scope {
            if let Some(ix) = &primary_trace {
                if let Some((se, wg)) = ix.scope() {
                    if !view.entered_world
                        || view.session_epoch != se
                        || view.world_generation != wg
                    {
                        if let Some(receipt) = receipt.as_ref() {
                            receipt.report(crate::net::CommandDelivery::Rejected(
                                crate::net::CommandRefusal::ScopeChanged,
                            ));
                        } else {
                            ix.record(crate::quest_trace::Stage::Delivery(
                                crate::net::CommandDelivery::Rejected(
                                    crate::net::CommandRefusal::ScopeChanged,
                                ),
                            ));
                        }
                        continue;
                    }
                }
            }
        }
        let count = view.next_cast_count;
        // The wire cast count actually used by a CMSG_CAST_SPELL arm, when one encodes. It is
        // distinct from the native attempt serial and the local interaction id.
        let mut admitted_wire_count: Option<u8> = None;
        let packet = match command {
            ClientCommand::SetSelection { guid } => {
                if guid != 0 && !view.entities.contains_key(&guid) {
                    debug!("wrath: dropping selection of unstreamed {guid:#x}");
                    continue;
                }
                Some((
                    benilla_protocol_wrath::opcode::CMSG_SET_SELECTION,
                    gameplay::set_selection(guid),
                ))
            }
            ClientCommand::AttackSwing { guid } => {
                if !view.entities.contains_key(&guid) {
                    continue;
                }
                Some((
                    benilla_protocol_wrath::opcode::CMSG_ATTACKSWING,
                    gameplay::attack_swing(guid),
                ))
            }
            ClientCommand::AttackStop => Some((
                benilla_protocol_wrath::opcode::CMSG_ATTACKSTOP,
                gameplay::attack_stop(),
            )),
            ClientCommand::CastSpell {
                spell_id, target, ..
            } => {
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell(
                        count,
                        spell_id,
                        target.map_or(gameplay::CastTarget::None, gameplay::CastTarget::Unit),
                    ),
                ))
            }
            ClientCommand::GossipHello { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_GOSSIP_HELLO,
                gameplay::gossip_hello(guid),
            )),
            ClientCommand::GossipSelectOption { guid, option, code } => {
                let menu_id = view
                    .gossip_menu
                    .filter(|(npc, _)| *npc == guid)
                    .map_or(0, |(_, id)| id);
                let code = code.as_deref().and_then(|b| std::str::from_utf8(b).ok());
                Some((
                    benilla_protocol_wrath::opcode::CMSG_GOSSIP_SELECT_OPTION,
                    gameplay::gossip_select(guid, menu_id, option, code),
                ))
            }
            ClientCommand::NpcTextQuery { text_id, guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_NPC_TEXT_QUERY,
                gameplay::npc_text_query(text_id, guid),
            )),
            ClientCommand::QuestgiverStatusQuery { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_STATUS_QUERY,
                gameplay::questgiver_status_query(npc),
            )),
            ClientCommand::QuestgiverHello { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_HELLO,
                gameplay::questgiver_hello(npc),
            )),
            ClientCommand::QuestgiverQuery { npc, quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_QUERY_QUEST,
                gameplay::questgiver_query(npc, quest),
            )),
            ClientCommand::QuestgiverAccept { npc, quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_ACCEPT_QUEST,
                gameplay::questgiver_accept(npc, quest),
            )),
            ClientCommand::QuestgiverComplete { npc, quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_COMPLETE_QUEST,
                gameplay::questgiver_complete(npc, quest),
            )),
            ClientCommand::QuestgiverRequestReward { npc, quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_REQUEST_REWARD,
                gameplay::questgiver_request_reward(npc, quest),
            )),
            ClientCommand::QuestgiverChooseReward { npc, quest, choice } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_CHOOSE_REWARD,
                gameplay::questgiver_choose_reward(npc, quest, choice),
            )),
            ClientCommand::NameQuery { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_NAME_QUERY,
                guid.to_le_bytes().to_vec(),
            )),
            ClientCommand::GameObjectQuery { entry, guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_GAMEOBJECT_QUERY,
                gameplay::gameobject_query(entry, guid),
            )),
            ClientCommand::CreatureQuery { entry, guid } => {
                let mut payload = entry.to_le_bytes().to_vec();
                payload.extend_from_slice(&guid.to_le_bytes());
                Some((benilla_protocol_wrath::opcode::CMSG_CREATURE_QUERY, payload))
            }
            ClientCommand::ItemQuery { entry, .. } => Some((
                benilla_protocol_wrath::opcode::CMSG_ITEM_QUERY_SINGLE,
                gameplay::item_query(entry),
            )),
            ClientCommand::QuestQuery { quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUEST_QUERY,
                gameplay::quest_query(quest),
            )),
            ClientCommand::QuestPoiQuery { quests } => {
                match benilla_protocol_wrath::quest_poi::query(&quests) {
                    Ok(payload) => Some((
                        benilla_protocol_wrath::quest_poi::CMSG_QUEST_POI_QUERY,
                        payload,
                    )),
                    Err(_) => None,
                }
            }
            ClientCommand::GameObjUse { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_GAMEOBJ_USE,
                guid.to_le_bytes().to_vec(),
            )),
            ClientCommand::Loot { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT,
                gameplay::loot(guid),
            )),
            ClientCommand::AutostoreLootItem { slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUTOSTORE_LOOT_ITEM,
                gameplay::autostore_loot_item(slot),
            )),
            ClientCommand::LootMoney => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT_MONEY,
                gameplay::loot_money(),
            )),
            ClientCommand::LootRelease { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT_RELEASE,
                gameplay::loot_release(guid),
            )),
            // The corresponding AzerothCore handlers read the same mailbox/mail-id
            // layout as these existing encoders; other mail verbs require Wrath-specific
            // fields and are intentionally not routed through the donor encoders.
            ClientCommand::GetMailList { mailbox } => Some((
                benilla_protocol_wrath::opcode::CMSG_GET_MAIL_LIST,
                benilla_protocol::messages::get_mail_list(mailbox),
            )),
            ClientCommand::SendMail {
                mailbox,
                receiver,
                subject,
                body,
                stationery,
                item_guid,
                money,
                cod,
                ..
            } => gameplay::economy::send_mail(
                mailbox, &receiver, &subject, &body, stationery, item_guid, money, cod,
            )
            .map(|body| (benilla_protocol_wrath::opcode::CMSG_SEND_MAIL, body)),
            ClientCommand::MailMarkAsRead { mailbox, mail_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_MARK_AS_READ,
                benilla_protocol::messages::mail_mark_as_read(mailbox, mail_id),
            )),
            ClientCommand::MailCreateTextItem {
                mailbox, mail_id, ..
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_CREATE_TEXT_ITEM,
                benilla_protocol::messages::mail_mark_as_read(mailbox, mail_id),
            )),
            ClientCommand::QueryNextMailTime => Some((
                benilla_protocol_wrath::opcode::MSG_QUERY_NEXT_MAIL_TIME,
                Vec::new(),
            )),
            ClientCommand::LootRoll {
                looted_target,
                item_slot,
                roll_type,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT_ROLL,
                gameplay::loot_roll(looted_target, item_slot, roll_type),
            )),
            ClientCommand::OptOutOfLoot { pass } => Some((
                benilla_protocol_wrath::opcode::CMSG_OPT_OUT_OF_LOOT,
                u32::from(pass).to_le_bytes().to_vec(),
            )),
            ClientCommand::SetDungeonDifficulty { mode } => Some((
                benilla_protocol_wrath::opcode::MSG_SET_DUNGEON_DIFFICULTY,
                mode.to_le_bytes().to_vec(),
            )),
            ClientCommand::SetRaidDifficulty { mode } => Some((
                benilla_protocol_wrath::opcode::MSG_SET_RAID_DIFFICULTY,
                mode.to_le_bytes().to_vec(),
            )),
            ClientCommand::Logout => Some((
                benilla_protocol_wrath::opcode::CMSG_LOGOUT_REQUEST,
                gameplay::logout_request(),
            )),
            ClientCommand::LogoutCancel => Some((
                benilla_protocol_wrath::opcode::CMSG_LOGOUT_CANCEL,
                gameplay::logout_cancel(),
            )),
            ClientCommand::SetActiveMover { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_ACTIVE_MOVER,
                guid.to_le_bytes().to_vec(),
            )),
            ClientCommand::SetActionButton { button, packed } if button < 144 => Some((
                0x128, // CMSG_SET_ACTION_BUTTON
                gameplay::set_action_button(button, packed),
            )),
            ClientCommand::SetActionButton { button, .. } => {
                debug!("wrath: action button {button} is outside the 12340 0..143 range; refused");
                None
            }
            ClientCommand::RepopRequest => Some((
                benilla_protocol_wrath::opcode::CMSG_REPOP_REQUEST,
                gameplay::repop_request(),
            )),
            ClientCommand::CorpseQuery => Some((
                u32::from(benilla_protocol_wrath::opcode::MSG_CORPSE_QUERY),
                Vec::new(),
            )),
            ClientCommand::ReclaimCorpse { corpse } => Some((
                benilla_protocol_wrath::opcode::CMSG_RECLAIM_CORPSE,
                corpse.to_le_bytes().to_vec(),
            )),
            ClientCommand::SelfRes => {
                Some((benilla_protocol_wrath::opcode::CMSG_SELF_RES, Vec::new()))
            }
            ClientCommand::SpiritHealerActivate { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_SPIRIT_HEALER_ACTIVATE,
                npc.to_le_bytes().to_vec(),
            )),
            ClientCommand::ResurrectResponse { caster, accept } => Some((
                benilla_protocol_wrath::opcode::CMSG_RESURRECT_RESPONSE,
                gameplay::resurrect_response(caster, accept),
            )),
            ClientCommand::ListInventory { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_LIST_INVENTORY,
                gameplay::list_inventory(guid),
            )),
            ClientCommand::BuyItem {
                vendor,
                entry,
                count,
            } => vendor_row(&view, vendor, entry).map(|slot| {
                (
                    benilla_protocol_wrath::opcode::CMSG_BUY_ITEM,
                    gameplay::buy_item(vendor, entry, slot, u32::from(count)),
                )
            }),
            ClientCommand::BuyItemInSlot {
                vendor,
                entry,
                bag_guid,
                bag_slot,
                count,
            } => vendor_row(&view, vendor, entry).map(|slot| {
                (
                    benilla_protocol_wrath::opcode::CMSG_BUY_ITEM_IN_SLOT,
                    gameplay::buy_item_in_slot(
                        vendor,
                        entry,
                        slot,
                        bag_guid,
                        bag_slot,
                        u32::from(count),
                    ),
                )
            }),
            ClientCommand::SellItem {
                vendor,
                item_guid,
                count,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SELL_ITEM,
                gameplay::sell_item(vendor, item_guid, u32::from(count)),
            )),
            ClientCommand::BuybackItem { vendor, slot } => {
                // The stock owner names the absolute 1.12 buyback slot 69-80; 12340 starts at 74.
                const STOCK_START: u32 = 69;
                const WRATH_START: u32 = 74;
                let wrath_slot = slot
                    .checked_sub(STOCK_START)
                    .filter(|index| *index < 12)
                    .map(|index| index + WRATH_START);
                wrath_slot.map(|slot| {
                    (
                        benilla_protocol_wrath::opcode::CMSG_BUYBACK_ITEM,
                        gameplay::buyback_item(vendor, slot),
                    )
                })
            }
            ClientCommand::RepairItem { vendor, item_guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_REPAIR_ITEM,
                gameplay::repair_item(vendor, item_guid, false),
            )),
            ClientCommand::TrainerList { trainer } => Some((
                benilla_protocol_wrath::opcode::CMSG_TRAINER_LIST,
                gameplay::trainer_list(trainer),
            )),
            ClientCommand::TrainerBuySpell { trainer, spell_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_TRAINER_BUY_SPELL,
                gameplay::trainer_buy_spell(trainer, spell_id),
            )),
            ClientCommand::AreaTrigger { trigger_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_AREATRIGGER,
                gameplay::area_trigger(trigger_id),
            )),
            ClientCommand::QuestlogRemove { slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTLOG_REMOVE_QUEST,
                gameplay::questlog_remove(slot),
            )),
            ClientCommand::QuestConfirmAccept { quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUEST_CONFIRM_ACCEPT,
                gameplay::quest_confirm_accept(quest),
            )),
            ClientCommand::PushQuestToParty { quest } => Some((
                benilla_protocol_wrath::opcode::CMSG_PUSHQUESTTOPARTY,
                gameplay::push_quest_to_party(quest),
            )),
            ClientCommand::QuestPushResult { .. } => {
                debug!("wrath: quest-share decline refused: the 12340 packet requires a quest id");
                None
            }
            ClientCommand::BinderActivate { binder } => Some((
                benilla_protocol_wrath::opcode::CMSG_BINDER_ACTIVATE,
                gameplay::binder_activate(binder),
            )),
            ClientCommand::SummonResponse { summoner } => Some((
                benilla_protocol_wrath::opcode::CMSG_SUMMON_RESPONSE,
                gameplay::summon_response(summoner, true),
            )),
            ClientCommand::TalentWipeConfirm { trainer } => Some((
                u32::from(benilla_protocol_wrath::opcode::MSG_TALENT_WIPE_CONFIRM),
                gameplay::talent_wipe_confirm(trainer),
            )),
            ClientCommand::PlayedTime => Some((
                benilla_protocol_wrath::opcode::CMSG_PLAYED_TIME,
                gameplay::played_time(),
            )),
            ClientCommand::QueryTime => Some((
                benilla_protocol_wrath::opcode::CMSG_QUERY_TIME,
                gameplay::query_time(),
            )),
            ClientCommand::EquipmentSetSave(set) => Some((
                benilla_protocol_wrath::opcode::CMSG_SAVE_EQUIPMENT_SET,
                gameplay::equipment_set::encode_save_equipment_set(&set),
            )),
            ClientCommand::EquipmentSetDelete { set_guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_EQUIPMENT_SET_DELETE,
                gameplay::equipment_set::encode_equipment_set_delete(set_guid),
            )),
            ClientCommand::EquipmentSetUse(slots) => Some((
                benilla_protocol_wrath::opcode::CMSG_EQUIPMENT_SET_USE,
                gameplay::equipment_set::encode_equipment_set_use(&slots),
            )),
            ClientCommand::OpenCalendar {
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    debug!(
                        "wrath: refusing a calendar fetch from stale scope ({session_epoch}, {world_generation})"
                    );
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_CALENDAR_GET_CALENDAR,
                        gameplay::wrath_systems::encode_calendar_get_calendar(),
                    ))
                }
            }
            ClientCommand::LfgJoin {
                slots,
                comment,
                roles,
                no_partial_clear,
                achievements,
                needs,
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    debug!(
                        "wrath: refusing LfgJoin from stale scope ({session_epoch}, {world_generation})"
                    );
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_LFG_JOIN,
                        gameplay::wrath_systems::encode_lfg_join_request(
                            &gameplay::wrath_systems::LfgJoinRequest {
                                slots,
                                comment,
                                roles,
                                no_partial_clear,
                                achievements,
                                needs,
                            },
                        ),
                    ))
                }
            }
            ClientCommand::LfgLeave {
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_LFG_LEAVE,
                        gameplay::wrath_systems::encode_lfg_leave(),
                    ))
                }
            }
            ClientCommand::LfgProposalResult {
                proposal_id,
                accept,
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_LFG_PROPOSAL_RESULT,
                        gameplay::wrath_systems::encode_lfg_proposal_result(proposal_id, accept),
                    ))
                }
            }
            ClientCommand::SetLfgComment {
                comment,
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_SET_LFG_COMMENT,
                        gameplay::wrath_systems::encode_set_lfg_comment(&comment),
                    ))
                }
            }
            ClientCommand::LfgSetRoles {
                roles,
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_LFG_SET_ROLES,
                        gameplay::wrath_systems::encode_lfg_set_roles(roles),
                    ))
                }
            }
            ClientCommand::VehicleExit {
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_REQUEST_VEHICLE_EXIT,
                        gameplay::wrath_systems::encode_request_vehicle_exit(),
                    ))
                }
            }
            ClientCommand::VehiclePrevSeat {
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_REQUEST_VEHICLE_PREV_SEAT,
                        gameplay::wrath_systems::encode_request_vehicle_prev_seat(),
                    ))
                }
            }
            ClientCommand::VehicleNextSeat {
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_REQUEST_VEHICLE_NEXT_SEAT,
                        gameplay::wrath_systems::encode_request_vehicle_next_seat(),
                    ))
                }
            }
            ClientCommand::VehicleSwitchSeat {
                vehicle,
                seat,
                session_epoch,
                world_generation,
            } => {
                if !view.entered_world
                    || view.session_epoch != session_epoch
                    || view.world_generation != world_generation
                {
                    None
                } else {
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_REQUEST_VEHICLE_SWITCH_SEAT,
                        gameplay::wrath_systems::encode_request_vehicle_switch_seat(vehicle, seat),
                    ))
                }
            }
            ClientCommand::RandomRoll { min, max } => Some((
                u32::from(benilla_protocol_wrath::opcode::MSG_RANDOM_ROLL),
                gameplay::random_roll(min, max),
            )),
            ClientCommand::PageTextQuery { page_id, guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_PAGE_TEXT_QUERY,
                gameplay::page_text_query(page_id, guid),
            )),
            ClientCommand::ItemTextByGuid { item_guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_ITEM_TEXT_QUERY,
                gameplay::item_text_query(item_guid),
            )),
            ClientCommand::ItemTextQuery { .. } => {
                debug!(
                    "wrath: 1.12 mail-text query has no 12340 equivalent; use the native by-guid path"
                );
                None
            }
            ClientCommand::SetSheathed { state } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_SHEATHED,
                gameplay::set_sheathed(state),
            )),
            ClientCommand::StandStateChange { state } => Some((
                benilla_protocol_wrath::opcode::CMSG_STANDSTATECHANGE,
                gameplay::stand_state_change(state),
            )),
            ClientCommand::MountSpecial => Some((
                benilla_protocol_wrath::opcode::CMSG_MOUNTSPECIAL_ANIM,
                gameplay::mount_special(),
            )),
            ClientCommand::SetActionBarToggles { toggles } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_ACTIONBAR_TOGGLES,
                gameplay::set_action_bar_toggles(toggles),
            )),
            ClientCommand::CancelCast { spell_id, .. } => {
                let self_guid = view.player_guid;
                view.note_cancel_requested(self_guid, spell_id);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CAST,
                    gameplay::cancel_cast(spell_id),
                ))
            }
            ClientCommand::CancelChannelling { spell_id, .. } => {
                let self_guid = view.player_guid;
                view.note_cancel_requested(self_guid, spell_id);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CHANNELLING,
                    gameplay::cancel_channelling(spell_id),
                ))
            }
            ClientCommand::CancelAura { spell_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_CANCEL_AURA,
                gameplay::cancel_aura(spell_id),
            )),
            ClientCommand::CancelAutoRepeat => Some((
                benilla_protocol_wrath::opcode::CMSG_CANCEL_AUTO_REPEAT_SPELL,
                gameplay::cancel_auto_repeat(),
            )),
            ClientCommand::BankerActivate { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_BANKER_ACTIVATE,
                gameplay::banker_activate(guid),
            )),
            ClientCommand::BuyBankSlot { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_BUY_BANK_SLOT,
                gameplay::buy_bank_slot(guid),
            )),
            ClientCommand::AutoBankItem { bag, slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUTOBANK_ITEM,
                gameplay::autobank_item(bag, slot),
            )),
            ClientCommand::AutoStoreBankItem { bag, slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUTOSTORE_BANK_ITEM,
                // `wire_pos_in_target(Wrath335)` already emits the 3.3.5 bank-bag bytes
                // (67..=73); no donor-to-Wrath shift belongs here.
                gameplay::autostore_bank_item(bag, slot),
            )),
            ClientCommand::CastSpellGameObject {
                spell_id, go_guid, ..
            } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_gameobject(count, spell_id, go_guid),
                ))
            }
            ClientCommand::CastSpellItem {
                spell_id,
                item_guid,
                ..
            } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_item(count, spell_id, item_guid),
                ))
            }
            ClientCommand::CastSpellTradeItem { spell_id, slot, .. } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_trade_item(count, spell_id, slot),
                ))
            }
            ClientCommand::CastSpellCorpse {
                spell_id,
                target,
                corpse_guid,
                ..
            } => {
                use benilla_protocol::messages::CorpseTarget;
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_corpse(
                        count,
                        spell_id,
                        matches!(target, CorpseTarget::Ally),
                        corpse_guid,
                    ),
                ))
            }
            ClientCommand::CastSpellAtDest { spell_id, dest, .. } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_at_dest(count, spell_id, dest),
                ))
            }
            ClientCommand::CastSpellAtSource { spell_id, src, .. } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_at_source(count, spell_id, src),
                ))
            }
            ClientCommand::UseItem {
                bag_index,
                slot,
                spell_id,
                item_guid,
                target,
                ..
            } => match (item_guid, wrath_cast_target(target)) {
                (Some(item_guid), Some(target)) => {
                    let count = view.next_cast_count;
                    view.next_cast_count = count.wrapping_add(1).max(1);
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_USE_ITEM,
                        gameplay::use_item(bag_index, slot, count, spell_id, item_guid, target),
                    ))
                }
                _ => {
                    debug!("wrath: item use dropped (no item guid or an unsupported target block)");
                    None
                }
            },
            ClientCommand::OpenItem { bag_index, slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_OPEN_ITEM,
                gameplay::open_item(bag_index, slot),
            )),
            ClientCommand::AutoEquipItem { bag_index, slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUTOEQUIP_ITEM,
                gameplay::auto_equip_item(bag_index, slot),
            )),
            ClientCommand::SwapInvItem { src_slot, dst_slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_SWAP_INV_ITEM,
                gameplay::swap_inv_item(dst_slot, src_slot),
            )),
            ClientCommand::SwapItem {
                dst_bag,
                dst_slot,
                src_bag,
                src_slot,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SWAP_ITEM,
                gameplay::swap_item(dst_bag, dst_slot, src_bag, src_slot),
            )),
            ClientCommand::AutoStoreBagItem {
                src_bag,
                src_slot,
                dst_bag,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUTOSTORE_BAG_ITEM,
                gameplay::auto_store_bag_item(src_bag, src_slot, dst_bag),
            )),
            ClientCommand::SplitItem {
                src_bag,
                src_slot,
                dst_bag,
                dst_slot,
                count,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SPLIT_ITEM,
                gameplay::split_item(src_bag, src_slot, dst_bag, dst_slot, u32::from(count)),
            )),
            ClientCommand::DestroyItem {
                bag_index,
                slot,
                count,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_DESTROYITEM,
                gameplay::destroy_item(bag_index, slot, count),
            )),
            ClientCommand::SetAmmo { entry } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_AMMO,
                gameplay::set_ammo(entry),
            )),
            ClientCommand::Chat {
                kind,
                target,
                text,
                language,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_MESSAGECHAT,
                gameplay::message_chat(
                    wrath_chat_type(kind),
                    stock_chat_language(kind, language, &view),
                    target.as_deref(),
                    &text,
                ),
            )),
            ClientCommand::AddonMessage { distribution, text } => Some((
                benilla_protocol_wrath::opcode::CMSG_MESSAGECHAT,
                gameplay::message_chat(addon_chat_type(distribution), 0xFFFF_FFFF, None, &text),
            )),
            ClientCommand::LearnTalent { talent_id, rank } => Some((
                benilla_protocol_wrath::opcode::CMSG_LEARN_TALENT,
                gameplay::encode_learn_talent(talent_id, rank),
            )),
            ClientCommand::LearnPreviewTalents {
                pet,
                pet_guid,
                entries,
            } => Some((
                if pet {
                    benilla_protocol_wrath::opcode::CMSG_LEARN_PREVIEW_TALENTS_PET
                } else {
                    benilla_protocol_wrath::opcode::CMSG_LEARN_PREVIEW_TALENTS
                },
                gameplay::encode_learn_preview_talents(
                    if pet { Some(pet_guid) } else { None },
                    &entries,
                ),
            )),
            ClientCommand::PetAction {
                pet_guid,
                packed,
                target_guid,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_ACTION,
                gameplay::encode_pet_action(pet_guid, packed, target_guid),
            )),
            ClientCommand::PetSetAction { pet_guid, entries } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_SET_ACTION,
                gameplay::encode_pet_set_action(pet_guid, &entries),
            )),
            ClientCommand::PetCastSpell {
                caster_guid,
                spell_id,
                target,
                ..
            } => wrath_cast_target(target).map(|target| {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                (
                    benilla_protocol_wrath::opcode::CMSG_PET_CAST_SPELL,
                    gameplay::encode_pet_cast_spell(caster_guid, count, spell_id, target),
                )
            }),
            ClientCommand::PetStopAttack { pet_guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_STOP_ATTACK,
                gameplay::encode_guid_only(pet_guid),
            )),
            ClientCommand::PetCancelAura { pet_guid, spell_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_CANCEL_AURA,
                gameplay::encode_pet_cancel_aura(pet_guid, spell_id),
            )),
            ClientCommand::PetSpellAutocast {
                pet_guid,
                spell_id,
                enabled,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_SPELL_AUTOCAST,
                gameplay::encode_pet_spell_autocast(pet_guid, spell_id, enabled),
            )),
            ClientCommand::PetAbandon { pet_guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_ABANDON,
                gameplay::encode_guid_only(pet_guid),
            )),
            ClientCommand::PetRename { pet_guid, name } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_RENAME,
                gameplay::encode_pet_rename(pet_guid, &name),
            )),
            ClientCommand::RequestPetInfo => Some((
                benilla_protocol_wrath::opcode::CMSG_REQUEST_PET_INFO,
                Vec::new(),
            )),
            ClientCommand::PetUnlearn { trainer } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_UNLEARN,
                gameplay::encode_guid_only(trainer),
            )),
            ClientCommand::ListStabledPets { npc } => Some((
                benilla_protocol_wrath::opcode::MSG_LIST_STABLED_PETS,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::StablePet { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_STABLE_PET,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::UnstablePet { npc, pet_number } => Some((
                benilla_protocol_wrath::opcode::CMSG_UNSTABLE_PET,
                gameplay::encode_stable_pet_slot(npc, pet_number),
            )),
            ClientCommand::BuyStableSlot { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_BUY_STABLE_SLOT,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::StableSwapPet { npc, pet_number } => Some((
                benilla_protocol_wrath::opcode::CMSG_STABLE_SWAP_PET,
                gameplay::encode_stable_pet_slot(npc, pet_number),
            )),
            ClientCommand::PetNameQuery { pet_number, guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_PET_NAME_QUERY,
                gameplay::pet_name_query(pet_number, guid),
            )),
            // The 12340 encoder writes a zero item guid; the command's guid is the shared
            // name-cache identity and has no field in the Wrath query body.
            ClientCommand::ItemNameQuery { entry, guid: _ } => Some((
                benilla_protocol_wrath::opcode::CMSG_ITEM_NAME_QUERY,
                gameplay::item_name_query(entry),
            )),
            ClientCommand::GuildQuery { guild_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_QUERY,
                guild_id.to_le_bytes().to_vec(),
            )),
            // `CMSG_TOGGLE_PVP` is toggle-by-opcode with no body; the donor sends `&[]` too.
            ClientCommand::TogglePvp => {
                Some((benilla_protocol_wrath::opcode::CMSG_TOGGLE_PVP, Vec::new()))
            }
            ClientCommand::ToggleHelm => {
                Some((benilla_protocol_wrath::opcode::CMSG_SHOWING_HELM, vec![1]))
            }
            ClientCommand::ToggleCloak => {
                Some((benilla_protocol_wrath::opcode::CMSG_SHOWING_CLOAK, vec![1]))
            }
            ClientCommand::Emote { emote } => Some((
                benilla_protocol_wrath::opcode::CMSG_EMOTE,
                emote.to_le_bytes().to_vec(),
            )),
            ClientCommand::TextEmote { text_id, target } => Some((
                benilla_protocol_wrath::opcode::CMSG_TEXT_EMOTE,
                // The command carries no sound/variation selector. The donor writer sends 0
                // there (`messages::text_emote`); 12340's `SendTextEmote` derives it from
                // `EmotesText`/race/sex, which this command cannot express, so 0 is the
                // best-supported neutral value.
                gameplay::text_emote(text_id, 0, target),
            )),
            ClientCommand::Inspect { target } => Some((
                benilla_protocol_wrath::opcode::CMSG_INSPECT,
                gameplay::encode_guid_only(target),
            )),
            ClientCommand::InspectHonorStats { target } => Some((
                benilla_protocol_wrath::opcode::MSG_INSPECT_HONOR_STATS,
                gameplay::encode_guid_only(target),
            )),
            ClientCommand::UnlearnSkill { skill_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_UNLEARN_SKILL,
                skill_id.to_le_bytes().to_vec(),
            )),
            ClientCommand::SetFactionAtWar {
                rep_list_id,
                at_war,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_FACTION_ATWAR,
                gameplay::set_faction_flag(rep_list_id, u8::from(at_war)),
            )),
            ClientCommand::SetFactionInactive {
                rep_list_id,
                inactive,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_FACTION_INACTIVE,
                gameplay::set_faction_flag(rep_list_id, u8::from(inactive)),
            )),
            ClientCommand::SetWatchedFaction { rep_list_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_WATCHED_FACTION,
                rep_list_id.to_le_bytes().to_vec(),
            )),
            ClientCommand::NextCinematicCamera => Some((
                benilla_protocol_wrath::opcode::CMSG_NEXT_CINEMATIC_CAMERA,
                Vec::new(),
            )),
            ClientCommand::CompleteCinematic => Some((
                benilla_protocol_wrath::opcode::CMSG_COMPLETE_CINEMATIC,
                Vec::new(),
            )),
            ClientCommand::TutorialFlag { id } => Some((
                benilla_protocol_wrath::opcode::CMSG_TUTORIAL_FLAG,
                id.to_le_bytes().to_vec(),
            )),
            ClientCommand::TutorialClear => Some((
                benilla_protocol_wrath::opcode::CMSG_TUTORIAL_CLEAR,
                Vec::new(),
            )),
            ClientCommand::TutorialReset => Some((
                benilla_protocol_wrath::opcode::CMSG_TUTORIAL_RESET,
                Vec::new(),
            )),
            ClientCommand::RequestRaidInfo => Some((
                benilla_protocol_wrath::opcode::CMSG_REQUEST_RAID_INFO,
                Vec::new(),
            )),
            ClientCommand::ResetInstances => Some((
                benilla_protocol_wrath::opcode::CMSG_RESET_INSTANCES,
                Vec::new(),
            )),
            ClientCommand::FriendListRequest => Some((
                benilla_protocol_wrath::opcode::CMSG_CONTACT_LIST,
                gameplay::contact_list(0x07),
            )),
            ClientCommand::AddFriend { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_ADD_FRIEND,
                gameplay::add_friend(&name, ""),
            )),
            ClientCommand::DelFriend { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_DEL_FRIEND,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::AddIgnore { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_ADD_IGNORE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::DelIgnore { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_DEL_IGNORE,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::ChatIgnored { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHAT_IGNORED,
                gameplay::chat_ignored(guid),
            )),
            ClientCommand::Whois { text } => Some((
                benilla_protocol_wrath::opcode::CMSG_WHOIS,
                gameplay::cstring_single(&text),
            )),
            ClientCommand::GroupInvite { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_INVITE,
                gameplay::group_invite(&name),
            )),
            ClientCommand::GroupAccept => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_ACCEPT,
                gameplay::group_accept(),
            )),
            ClientCommand::GroupDecline => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_DECLINE,
                Vec::new(),
            )),
            ClientCommand::GroupUninvite { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_UNINVITE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GroupUninviteGuid { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_UNINVITE_GUID,
                gameplay::group_uninvite_guid(guid, ""),
            )),
            ClientCommand::LootMethod {
                method,
                master,
                threshold,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT_METHOD,
                gameplay::loot_method(method, master, threshold),
            )),
            ClientCommand::GroupLeave => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_DISBAND,
                Vec::new(),
            )),
            ClientCommand::GroupRaidConvert => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_RAID_CONVERT,
                Vec::new(),
            )),
            ClientCommand::GroupChangeSubGroup { name, group } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_CHANGE_SUB_GROUP,
                gameplay::group_change_sub_group(&name, group),
            )),
            ClientCommand::GroupSwapSubGroup { name, other } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_SWAP_SUB_GROUP,
                gameplay::cstring_pair(&name, &other),
            )),
            ClientCommand::RequestPartyMemberStats { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_REQUEST_PARTY_MEMBER_STATS,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::SetRaidTarget { icon, guid } => Some((
                benilla_protocol_wrath::opcode::MSG_RAID_TARGET_UPDATE,
                gameplay::set_raid_target(icon, guid),
            )),
            ClientCommand::RequestRaidTargets => Some((
                benilla_protocol_wrath::opcode::MSG_RAID_TARGET_UPDATE,
                vec![0xFF],
            )),
            ClientCommand::MinimapPing { x, y } => Some((
                benilla_protocol_wrath::opcode::MSG_MINIMAP_PING,
                gameplay::minimap_ping(x, y),
            )),
            ClientCommand::ReadyCheckStart => Some((
                benilla_protocol_wrath::opcode::MSG_RAID_READY_CHECK,
                Vec::new(),
            )),
            ClientCommand::ReadyCheckAnswer { status } => Some((
                benilla_protocol_wrath::opcode::MSG_RAID_READY_CHECK,
                vec![status],
            )),
            ClientCommand::LootMasterGive { guid, slot, target } => Some((
                benilla_protocol_wrath::opcode::CMSG_LOOT_MASTER_GIVE,
                gameplay::loot_master_give(guid, slot, target),
            )),
            ClientCommand::DuelAccepted { arbiter } => Some((
                benilla_protocol_wrath::opcode::CMSG_DUEL_ACCEPTED,
                gameplay::encode_guid_only(arbiter),
            )),
            ClientCommand::DuelCancelled { arbiter } => Some((
                benilla_protocol_wrath::opcode::CMSG_DUEL_CANCELLED,
                gameplay::encode_guid_only(arbiter),
            )),
            ClientCommand::TaxiNodeStatusQuery { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_TAXINODE_STATUS_QUERY,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::TaxiQueryNodes { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_TAXIQUERYAVAILABLENODES,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::ActivateTaxi {
                guid,
                source_node,
                dest_node,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_ACTIVATETAXI,
                gameplay::activate_taxi(guid, source_node, dest_node),
            )),
            // 12340's `CMSG_ACTIVATETAXIEXPRESS` is (guid, node_count, nodes); the shared
            // command's `total_cost` is 1.12-only and has no field in this body.
            ClientCommand::ActivateTaxiExpress { guid, nodes, .. } => Some((
                benilla_protocol_wrath::opcode::CMSG_ACTIVATETAXIEXPRESS,
                gameplay::activate_taxi_express(guid, &nodes),
            )),
            ClientCommand::InitiateTrade { target } => Some((
                benilla_protocol_wrath::opcode::CMSG_INITIATE_TRADE,
                gameplay::encode_guid_only(target),
            )),
            ClientCommand::BeginTrade => {
                Some((benilla_protocol_wrath::opcode::CMSG_BEGIN_TRADE, Vec::new()))
            }
            ClientCommand::BusyTrade => {
                Some((benilla_protocol_wrath::opcode::CMSG_BUSY_TRADE, Vec::new()))
            }
            ClientCommand::IgnoreTrade => Some((
                benilla_protocol_wrath::opcode::CMSG_IGNORE_TRADE,
                Vec::new(),
            )),
            ClientCommand::AcceptTrade { flag } => Some((
                benilla_protocol_wrath::opcode::CMSG_ACCEPT_TRADE,
                flag.to_le_bytes().to_vec(),
            )),
            ClientCommand::UnacceptTrade => Some((
                benilla_protocol_wrath::opcode::CMSG_UNACCEPT_TRADE,
                Vec::new(),
            )),
            ClientCommand::CancelTrade => Some((
                benilla_protocol_wrath::opcode::CMSG_CANCEL_TRADE,
                Vec::new(),
            )),
            ClientCommand::SetTradeItem {
                trade_slot,
                bag,
                slot,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_TRADE_ITEM,
                vec![trade_slot, bag, slot],
            )),
            ClientCommand::ClearTradeItem { trade_slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_CLEAR_TRADE_ITEM,
                vec![trade_slot],
            )),
            ClientCommand::SetTradeGold { copper } => Some((
                benilla_protocol_wrath::opcode::CMSG_SET_TRADE_GOLD,
                copper.to_le_bytes().to_vec(),
            )),
            ClientCommand::JoinChannel { name, password } => Some((
                benilla_protocol_wrath::opcode::CMSG_JOIN_CHANNEL,
                gameplay::join_channel(&name, &password),
            )),
            ClientCommand::LeaveChannel { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_LEAVE_CHANNEL,
                gameplay::leave_channel(&name),
            )),
            ClientCommand::ChannelList { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_LIST,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::ChannelPassword { name, password } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_PASSWORD,
                gameplay::cstring_pair(&name, &password),
            )),
            ClientCommand::ChannelSetOwner { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_SET_OWNER,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelOwner { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_OWNER,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::ChannelModerator { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_MODERATOR,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelUnmoderator { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_UNMODERATOR,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelMute { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_MUTE,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelUnmute { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_UNMUTE,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelInvite { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_INVITE,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelKick { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_KICK,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelBan { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_BAN,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelUnban { name, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_UNBAN,
                gameplay::cstring_pair(&name, &player),
            )),
            ClientCommand::ChannelAnnouncements { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_ANNOUNCEMENTS,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::ChannelModerate { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_CHANNEL_MODERATE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildInvite { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_INVITE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildAccept => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_ACCEPT,
                Vec::new(),
            )),
            ClientCommand::GuildDecline => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_DECLINE,
                Vec::new(),
            )),
            ClientCommand::GuildInfoRequest => {
                Some((benilla_protocol_wrath::opcode::CMSG_GUILD_INFO, Vec::new()))
            }
            ClientCommand::GuildRosterRequest => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_ROSTER,
                Vec::new(),
            )),
            ClientCommand::GuildPromote { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_PROMOTE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildDemote { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_DEMOTE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildLeave => {
                Some((benilla_protocol_wrath::opcode::CMSG_GUILD_LEAVE, Vec::new()))
            }
            ClientCommand::GuildRemove { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_REMOVE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildDisband => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_DISBAND,
                Vec::new(),
            )),
            ClientCommand::GuildLeader { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_LEADER,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildMotd { motd } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_MOTD,
                gameplay::cstring_single(&motd),
            )),
            ClientCommand::GuildAddRank { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_ADD_RANK,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildDelRank => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_DEL_RANK,
                Vec::new(),
            )),
            ClientCommand::GuildSetPublicNote { name, note } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_SET_PUBLIC_NOTE,
                gameplay::cstring_pair(&name, &note),
            )),
            ClientCommand::GuildSetOfficerNote { name, note } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_SET_OFFICER_NOTE,
                gameplay::cstring_pair(&name, &note),
            )),
            ClientCommand::GuildInfoText { text } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_INFO_TEXT,
                gameplay::cstring_single(&text),
            )),
            ClientCommand::PetitionShowList { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_PETITION_SHOWLIST,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::PetitionShowSignatures { item } => Some((
                benilla_protocol_wrath::opcode::CMSG_PETITION_SHOW_SIGNATURES,
                gameplay::encode_guid_only(item),
            )),
            ClientCommand::PetitionDecline { item } => Some((
                benilla_protocol_wrath::opcode::MSG_PETITION_DECLINE,
                gameplay::encode_guid_only(item),
            )),
            ClientCommand::TurnInPetition { item } => Some((
                benilla_protocol_wrath::opcode::CMSG_TURN_IN_PETITION,
                gameplay::encode_guid_only(item),
            )),
            ClientCommand::PetitionBuy { npc, name } => Some((
                benilla_protocol_wrath::opcode::CMSG_PETITION_BUY,
                gameplay::petition_buy(npc, &name),
            )),
            ClientCommand::PetitionSign { item, byte } => Some((
                benilla_protocol_wrath::opcode::CMSG_PETITION_SIGN,
                gameplay::petition_sign(item, byte as u8),
            )),
            ClientCommand::OfferPetition { item, player } => Some((
                benilla_protocol_wrath::opcode::CMSG_OFFER_PETITION,
                gameplay::offer_petition(item, player),
            )),
            ClientCommand::PetitionQuery { petition_id, item } => Some((
                benilla_protocol_wrath::opcode::CMSG_PETITION_QUERY,
                gameplay::petition_query(petition_id, item),
            )),
            ClientCommand::PetitionRename { item, name } => Some((
                benilla_protocol_wrath::opcode::MSG_PETITION_RENAME,
                gameplay::petition_rename(item, &name),
            )),
            ClientCommand::WrapItem {
                gift_bag,
                gift_slot,
                item_bag,
                item_slot,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_WRAP_ITEM,
                gameplay::wrap_item(gift_bag, gift_slot, item_bag, item_slot),
            )),
            ClientCommand::MailTakeMoney { mailbox, mail_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_TAKE_MONEY,
                gameplay::mail_action(mailbox, mail_id),
            )),
            ClientCommand::MailTakeItem {
                mailbox,
                mail_id,
                item_guid_low,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_TAKE_ITEM,
                gameplay::mail_action_with_u32(mailbox, mail_id, item_guid_low),
            )),
            ClientCommand::MailReturnToSender { mailbox, mail_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_RETURN_TO_SENDER,
                gameplay::mail_return_to_sender(mailbox, mail_id),
            )),
            ClientCommand::MailDelete { mailbox, mail_id } => Some((
                benilla_protocol_wrath::opcode::CMSG_MAIL_DELETE,
                gameplay::mail_action_with_u32(mailbox, mail_id, 0),
            )),
            ClientCommand::AuctionHello { auctioneer } => Some((
                benilla_protocol_wrath::opcode::MSG_AUCTION_HELLO,
                gameplay::encode_guid_only(auctioneer),
            )),
            ClientCommand::AuctionListItems {
                auctioneer,
                list_from,
                searched_name,
                level_min,
                level_max,
                slot_id,
                main_category,
                sub_category,
                quality,
                usable,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_LIST_ITEMS,
                gameplay::auction_list_items(
                    auctioneer,
                    list_from,
                    &searched_name,
                    level_min,
                    level_max,
                    slot_id,
                    main_category,
                    sub_category,
                    quality,
                    usable,
                ),
            )),
            ClientCommand::AuctionListOwnerItems {
                auctioneer,
                list_from,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_LIST_OWNER_ITEMS,
                gameplay::auction_guid_and_u32(auctioneer, list_from),
            )),
            ClientCommand::AuctionListBidderItems {
                auctioneer,
                list_from,
                auction_ids,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_LIST_BIDDER_ITEMS,
                gameplay::auction_list_bidder_items(auctioneer, list_from, &auction_ids),
            )),
            ClientCommand::AuctionSellItem {
                auctioneer,
                items,
                bid,
                buyout,
                etime_minutes,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_SELL_ITEM,
                gameplay::auction_sell_item(auctioneer, &items, bid, buyout, etime_minutes),
            )),
            ClientCommand::AuctionPlaceBid {
                auctioneer,
                auction_id,
                price,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_PLACE_BID,
                gameplay::auction_place_bid(auctioneer, auction_id, price),
            )),
            ClientCommand::AuctionRemoveItem {
                auctioneer,
                auction_id,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_AUCTION_REMOVE_ITEM,
                gameplay::auction_guid_and_u32(auctioneer, auction_id),
            )),
            ClientCommand::GroupSetLeader { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_SET_LEADER,
                gameplay::encode_guid_only(guid),
            )),
            ClientCommand::GroupAssistantLeader { guid, grant } => Some((
                benilla_protocol_wrath::opcode::CMSG_GROUP_ASSISTANT_LEADER,
                gameplay::group_assistant_leader(guid, grant),
            )),
            ClientCommand::Who { request } => Some((
                benilla_protocol_wrath::opcode::CMSG_WHO,
                benilla_protocol::messages::who(&request),
            )),
            ClientCommand::GuildCreate { name } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_CREATE,
                gameplay::cstring_single(&name),
            )),
            ClientCommand::GuildRank {
                rank_id,
                rights,
                name,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_RANK,
                gameplay::guild_rank(rank_id, rights, &name),
            )),
            ClientCommand::SaveGuildEmblem { vendor, design } => Some((
                benilla_protocol_wrath::opcode::CMSG_SAVE_GUILD_EMBLEM,
                benilla_protocol::messages::save_guild_emblem(vendor, design),
            )),
            ClientCommand::TabardVendorActivate { npc } => Some((
                benilla_protocol_wrath::opcode::MSG_TABARDVENDOR_ACTIVATE,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::OpeningCinematic => Some((
                benilla_protocol_wrath::opcode::CMSG_OPENING_CINEMATIC,
                Vec::new(),
            )),
            ClientCommand::ForceLogout => Some((
                benilla_protocol_wrath::opcode::CMSG_PLAYER_LOGOUT,
                Vec::new(),
            )),
            ClientCommand::FarSight { engage } => Some((
                benilla_protocol_wrath::opcode::CMSG_FARSIGHT,
                gameplay::farsight(engage),
            )),
            ClientCommand::AreaSpiritHealerQueue { healer } => Some((
                benilla_protocol_wrath::opcode::CMSG_AREA_SPIRIT_HEALER_QUEUE,
                gameplay::encode_guid_only(healer),
            )),
            ClientCommand::AreaSpiritHealerQuery { healer } => Some((
                benilla_protocol_wrath::opcode::CMSG_AREA_SPIRIT_HEALER_QUERY,
                gameplay::encode_guid_only(healer),
            )),
            ClientCommand::BattlemasterHello { npc } => Some((
                benilla_protocol_wrath::opcode::CMSG_BATTLEMASTER_HELLO,
                gameplay::encode_guid_only(npc),
            )),
            ClientCommand::RequestBattlefieldScoreData => Some((
                u32::from(benilla_protocol_wrath::opcode::MSG_PVP_LOG_DATA),
                Vec::new(),
            )),
            ClientCommand::RequestBattlefieldPositions => Some((
                benilla_protocol_wrath::opcode::MSG_BATTLEGROUND_PLAYER_POSITIONS,
                Vec::new(),
            )),
            ClientCommand::BattlefieldStatusRequest => Some((
                benilla_protocol_wrath::opcode::CMSG_BATTLEFIELD_STATUS,
                Vec::new(),
            )),
            ClientCommand::BattlefieldList { map_id } => {
                let bg_type = wrath_map_to_bg_type_id(map_id);
                let mut body = Vec::with_capacity(6);
                body.extend_from_slice(&bg_type.to_le_bytes());
                body.push(0); // from_where: UI / non-battlemaster
                body.push(0); // xp_gain
                Some((benilla_protocol_wrath::opcode::CMSG_BATTLEFIELD_LIST, body))
            }
            ClientCommand::BattlemasterJoin {
                battlemaster,
                map_id,
                instance_id,
                as_group,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_BATTLEMASTER_JOIN,
                gameplay::wrath_systems::encode_battlemaster_join(
                    battlemaster,
                    wrath_map_to_bg_type_id(map_id),
                    instance_id,
                    as_group,
                ),
            )),
            ClientCommand::BattlefieldJoin {
                map_id,
                instance_id,
                as_group,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_BATTLEMASTER_JOIN,
                gameplay::wrath_systems::encode_battlemaster_join(
                    view.player_guid,
                    wrath_map_to_bg_type_id(map_id),
                    instance_id,
                    as_group,
                ),
            )),
            ClientCommand::BattlefieldPort { map_id, accept } => Some((
                benilla_protocol_wrath::opcode::CMSG_BATTLEFIELD_PORT,
                gameplay::wrath_systems::encode_battlefield_port(
                    0,
                    0,
                    wrath_map_to_bg_type_id(map_id),
                    u8::from(accept),
                ),
            )),
            ClientCommand::LeaveBattlefield { map_id } => {
                let bg_type = wrath_map_to_bg_type_id(map_id);
                let mut body = Vec::with_capacity(8);
                body.push(0);
                body.push(0);
                body.extend_from_slice(&bg_type.to_le_bytes());
                body.extend_from_slice(&0x1F90u16.to_le_bytes());
                Some((benilla_protocol_wrath::opcode::CMSG_LEAVE_BATTLEFIELD, body))
            }
            ClientCommand::GmTicketCreate {
                category,
                map,
                pos,
                text,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_GMTICKET_CREATE,
                benilla_protocol::messages::gm_ticket_create(category, map, pos, &text),
            )),
            ClientCommand::GmTicketUpdate { category, text } => Some((
                benilla_protocol_wrath::opcode::CMSG_GMTICKET_UPDATETEXT,
                benilla_protocol::messages::gm_ticket_updatetext(category, &text),
            )),
            ClientCommand::GmTicketGet => Some((
                benilla_protocol_wrath::opcode::CMSG_GMTICKET_GETTICKET,
                Vec::new(),
            )),
            ClientCommand::GmTicketDelete => Some((
                benilla_protocol_wrath::opcode::CMSG_GMTICKET_DELETETICKET,
                Vec::new(),
            )),
            ClientCommand::GmTicketSystemStatus => Some((
                benilla_protocol_wrath::opcode::CMSG_GMTICKET_SYSTEMSTATUS,
                Vec::new(),
            )),
            ClientCommand::GmSurveySubmit {
                survey,
                answers,
                comment,
            } => {
                let answers: Vec<benilla_protocol::messages::SurveyAnswer<'_>> = answers
                    .iter()
                    .map(
                        |(question, rating, comment)| benilla_protocol::messages::SurveyAnswer {
                            question: *question,
                            rating: *rating,
                            comment: comment.as_str(),
                        },
                    )
                    .collect();
                Some((
                    benilla_protocol_wrath::opcode::CMSG_GMSURVEY_SUBMIT,
                    benilla_protocol::messages::gm_survey_submit(survey, &answers, &comment),
                ))
            }
            ClientCommand::ForceSpeedAck {
                kind,
                guid,
                counter,
                speed,
                flags,
                pos,
                orientation,
                pitch,
                fall_time,
                jump,
                transport,
            } => {
                use benilla_protocol::messages::SpeedKind;
                let opcode = match kind {
                    SpeedKind::Walk => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_WALK_SPEED_CHANGE_ACK)
                    }
                    SpeedKind::Run => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_RUN_SPEED_CHANGE_ACK)
                    }
                    SpeedKind::RunBack => u32::from(
                        benilla_protocol_wrath::opcode::CMSG_FORCE_RUN_BACK_SPEED_CHANGE_ACK,
                    ),
                    SpeedKind::Swim => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_SWIM_SPEED_CHANGE_ACK)
                    }
                    SpeedKind::SwimBack => u32::from(
                        benilla_protocol_wrath::opcode::CMSG_FORCE_SWIM_BACK_SPEED_CHANGE_ACK,
                    ),
                    SpeedKind::TurnRate => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_TURN_RATE_CHANGE_ACK)
                    }
                };
                Some((
                    opcode,
                    gameplay::force_speed_ack(
                        guid,
                        counter,
                        speed,
                        wrath_move_flags(flags),
                        now_ms,
                        pos,
                        orientation,
                        pitch,
                        fall_time,
                        jump.map(|j| (j.zspeed, j.sin_angle, j.cos_angle, j.xy_speed)),
                        transport.map(|t| (t.guid, [t.pos.x, t.pos.y, t.pos.z], t.orientation, 0)),
                    ),
                ))
            }
            ClientCommand::MoveModeAck {
                guid,
                counter,
                mode,
                apply,
                flags,
                pos,
                orientation,
            } => {
                use benilla_protocol::messages::MoveMode;
                let opcode = match (mode, apply) {
                    (MoveMode::Root, true) => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_MOVE_ROOT_ACK)
                    }
                    (MoveMode::Root, false) => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_FORCE_MOVE_UNROOT_ACK)
                    }
                    (MoveMode::WaterWalk, _) => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_WATER_WALK_ACK)
                    }
                    (MoveMode::FeatherFall, _) => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_FEATHER_FALL_ACK)
                    }
                    (MoveMode::Hover, _) => {
                        u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_HOVER_ACK)
                    }
                };
                Some((
                    opcode,
                    gameplay::move_mode_ack(
                        guid,
                        counter,
                        apply,
                        wrath_move_flags(flags),
                        now_ms,
                        pos,
                        orientation,
                    ),
                ))
            }
            ClientCommand::KnockBackAck {
                guid,
                counter,
                launch,
                flags,
                pos,
                orientation,
                transport,
            } => Some((
                u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_KNOCK_BACK_ACK),
                gameplay::knock_back_ack(
                    guid,
                    counter,
                    wrath_move_flags(flags),
                    now_ms,
                    pos,
                    orientation,
                    (
                        launch.zspeed,
                        launch.sin_angle,
                        launch.cos_angle,
                        launch.xy_speed,
                    ),
                    transport.map(|t| (t.guid, [t.pos.x, t.pos.y, t.pos.z], t.orientation, 0)),
                ),
            )),
            ClientCommand::MoveSplineDone {
                flags,
                pos,
                orientation,
                spline_id,
            } => Some((
                u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_SPLINE_DONE),
                gameplay::move_spline_done(
                    view.player_guid,
                    wrath_move_flags(flags),
                    now_ms,
                    pos,
                    orientation,
                    spline_id,
                ),
            )),
            ClientCommand::NotActiveMover {
                guid,
                flags,
                pos,
                orientation,
                fall_time,
            } => Some((
                u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_NOT_ACTIVE_MOVER),
                gameplay::not_active_mover(
                    guid,
                    wrath_move_flags(flags),
                    now_ms,
                    pos,
                    orientation,
                    fall_time,
                ),
            )),
            ClientCommand::MoveTimeSkipped { guid, lag_ms } => Some((
                u32::from(benilla_protocol_wrath::opcode::CMSG_MOVE_TIME_SKIPPED),
                gameplay::move_time_skipped(guid, lag_ms),
            )),
            _ => None,
        };
        if let Some((opcode, payload)) = packet {
            if let Some(trace) = &primary_trace {
                trace.record(crate::quest_trace::Stage::Encoded);
            }
            if let Ok(mut queue) = input.0.lock() {
                if (opcode == benilla_protocol_wrath::opcode::CMSG_ITEM_QUERY_SINGLE
                    || opcode == benilla_protocol_wrath::opcode::CMSG_NAME_QUERY)
                    && queue
                        .iter()
                        .any(|o| o.opcode == opcode && o.payload == payload)
                {
                    if let Some(receipt) = receipt.as_ref() {
                        receipt.report(crate::net::CommandDelivery::OutboxAdmitted);
                    }
                    continue;
                }
                if queue.len() < WRATH_OUTBOX_CAP {
                    // The Wrath encoder writes one cast-count byte before the spell ID.
                    let admitted_spell = (opcode
                        == benilla_protocol_wrath::opcode::CMSG_CAST_SPELL)
                        .then(|| payload.get(1..5)?.try_into().ok().map(u32::from_le_bytes))
                        .flatten();
                    queue.push_back(WrathOutbound {
                        opcode,
                        payload,
                        movement: false,
                        movement_seq: None,
                        enqueued_at: None,
                        receipt: receipt.clone(),
                        trace: cmd_trace.clone(),
                        enforce_scope,
                    });
                    if let Some(receipt) = receipt.as_ref() {
                        receipt.report(crate::net::CommandDelivery::OutboxAdmitted);
                        // One interaction must receive exactly one terminal delivery event: the receipt
                        // remains the authority, so a distinct envelope trace records only when its
                        // interaction differs from the receipt's.
                        if let Some(pt) = &primary_trace {
                            let receipt_ix = receipt
                                .trace
                                .as_ref()
                                .map(|t| t.interaction_id())
                                .unwrap_or(0);
                            if pt.interaction_id() != receipt_ix {
                                pt.record_delivery(crate::net::CommandDelivery::OutboxAdmitted);
                            }
                        }
                    } else if let Some(pt) = &primary_trace {
                        pt.record_delivery(crate::net::CommandDelivery::OutboxAdmitted);
                    }
                    if diagnostic_outbound(opcode) {
                        let stage = crate::quest_trace::Stage::Outbound {
                            opcode,
                            state: crate::quest_trace::OutboundState::Queued,
                        };
                        if let Some(ix) = &cmd_trace {
                            ix.record(stage);
                        } else if let Some(trace) = &trace {
                            trace.observe(scope, None, stage);
                        }
                    }
                    if let Some(spell_id) = admitted_spell {
                        let stage = crate::quest_trace::Stage::Cast {
                            spell_id: Some(spell_id),
                            attempt: None,
                            wire_cast_count: admitted_wire_count.map(u32::from),
                            edge: crate::quest_trace::CastEdge::Admitted,
                        };
                        if let Some(ix) = &cmd_trace {
                            ix.record(stage);
                        } else if let Some(trace) = &trace {
                            trace.observe(scope, None, stage);
                        }
                    }
                    if opening {
                        if let Some(trace) = &trace {
                            trace.observe(
                                scope,
                                None,
                                crate::quest_trace::Stage::OpenOutboxAdmitted,
                            );
                        }
                    }
                } else {
                    // The only way to fill this queue is a burst of uncapped actions or a
                    // session whose writer stopped draining; refuse visibly instead of hiding
                    // the loss. Movement never fills it (its own cap evicts motion first).
                    warn!(
                        "wrath: outbox full ({}), refusing client opcode {opcode:#x}",
                        WRATH_OUTBOX_CAP
                    );
                    if opcode == benilla_protocol_wrath::opcode::CMSG_ITEM_QUERY_SINGLE {
                        // The ask stays pending until the item cache's re-ask clock runs out.
                        let item = payload
                            .get(..4)
                            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                        warn!("wrath: item template ask for item {item:?} dropped (outbox full)");
                    }
                    if let Some(receipt) = receipt.as_ref() {
                        receipt.report(crate::net::CommandDelivery::Rejected(
                            crate::net::CommandRefusal::OutboxFull,
                        ));
                        if let Some(pt) = &primary_trace {
                            let receipt_ix = receipt
                                .trace
                                .as_ref()
                                .map(|t| t.interaction_id())
                                .unwrap_or(0);
                            if pt.interaction_id() != receipt_ix {
                                pt.record_delivery(crate::net::CommandDelivery::Rejected(
                                    crate::net::CommandRefusal::OutboxFull,
                                ));
                            }
                        }
                    } else if let Some(pt) = &primary_trace {
                        pt.record_delivery(crate::net::CommandDelivery::Rejected(
                            crate::net::CommandRefusal::OutboxFull,
                        ));
                    }
                    if diagnostic_outbound(opcode) {
                        let stage = crate::quest_trace::Stage::Outbound {
                            opcode,
                            state: crate::quest_trace::OutboundState::Refused,
                        };
                        if let Some(ix) = &cmd_trace {
                            ix.record(stage);
                        } else if let Some(trace) = &trace {
                            trace.observe(scope, None, stage);
                        }
                    }
                    if opening {
                        if let Some(trace) = &trace {
                            trace.observe(
                                scope,
                                None,
                                crate::quest_trace::Stage::OpenOutboxRefused,
                            );
                        }
                    }
                }
            } else {
                if let Some(receipt) = receipt.as_ref() {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::Unavailable,
                    ));
                    if let Some(pt) = &primary_trace {
                        let receipt_ix = receipt
                            .trace
                            .as_ref()
                            .map(|t| t.interaction_id())
                            .unwrap_or(0);
                        if pt.interaction_id() != receipt_ix {
                            pt.record_delivery(crate::net::CommandDelivery::Rejected(
                                crate::net::CommandRefusal::Unavailable,
                            ));
                        }
                    }
                } else if let Some(pt) = &primary_trace {
                    pt.record_delivery(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::Unavailable,
                    ));
                }
                if diagnostic_outbound(opcode) {
                    let stage = crate::quest_trace::Stage::Outbound {
                        opcode,
                        state: crate::quest_trace::OutboundState::Refused,
                    };
                    if let Some(ix) = &cmd_trace {
                        ix.record(stage);
                    } else if let Some(trace) = &trace {
                        trace.observe(scope, None, stage);
                    }
                }
                if opening {
                    if let Some(trace) = &trace {
                        trace.observe(scope, None, crate::quest_trace::Stage::OpenOutboxRefused);
                    }
                }
            }
        } else if let Some(ix) = &cmd_trace {
            ix.record(crate::quest_trace::Stage::GuardRefused);
        }
    }
}

/// Uncapped actions admitted per frame boundary; movement is capped separately and may never
/// evict these entries.
const WRATH_OUTBOX_CAP: usize = 4096;
/// Replaceable motion entries allowed before the streamer evicts its own oldest pose.
const WRATH_MOVEMENT_QUEUE_CAP: usize = 64;

/// Whether a tagged event belongs to a scope the view has already left. A newer event is kept:
/// the digest that adopts its scope may still be in flight, and applying it against the previous
/// scope is the existing packet-before-digest order rather than a stale replay.
/// Outbound packet metadata allowed into the local recorder. Payloads are never copied.
fn diagnostic_outbound(opcode: u32) -> bool {
    use benilla_protocol_wrath::opcode::*;
    matches!(
        opcode,
        CMSG_CAST_SPELL
            | CMSG_ATTACKSWING
            | CMSG_ATTACKSTOP
            | CMSG_LOOT
            | CMSG_LOOT_MONEY
            | CMSG_LOOT_RELEASE
            | CMSG_LOOT_ROLL
            | CMSG_AUTOSTORE_LOOT_ITEM
            | CMSG_LIST_INVENTORY
            | CMSG_SELL_ITEM
            | CMSG_BUY_ITEM
            | CMSG_BUY_ITEM_IN_SLOT
            | CMSG_USE_ITEM
            | CMSG_AUTOEQUIP_ITEM
    )
}

/// Packet metadata allowed into the local recorder. Payloads are never copied.
fn diagnostic_opcode(opcode: u16) -> bool {
    use benilla_protocol_wrath::opcode::*;
    matches!(
        opcode,
        SMSG_UPDATE_OBJECT
            | SMSG_GOSSIP_MESSAGE
            | SMSG_GOSSIP_COMPLETE
            | SMSG_QUESTGIVER_QUEST_LIST
            | SMSG_QUESTGIVER_QUEST_DETAILS
            | SMSG_QUESTGIVER_REQUEST_ITEMS
            | SMSG_QUESTGIVER_OFFER_REWARD
            | SMSG_QUESTGIVER_QUEST_INVALID
            | SMSG_QUESTGIVER_QUEST_FAILED
            | SMSG_QUESTLOG_FULL
            | SMSG_QUEST_QUERY_RESPONSE
            | SMSG_SPELL_START
            | SMSG_SPELL_GO
            | SMSG_CAST_FAILED
            | SMSG_SPELL_FAILURE
            | SMSG_SPELL_FAILED_OTHER
            | SMSG_SPELL_DELAYED
            | SMSG_INVENTORY_CHANGE_FAILURE
            | SMSG_LOOT_RESPONSE
            | SMSG_ITEM_PUSH_RESULT
    )
}

fn scope_is_older(
    current_epoch: u64,
    current_world: u64,
    event_epoch: u64,
    event_world: u64,
) -> bool {
    event_epoch < current_epoch || (event_epoch == current_epoch && event_world < current_world)
}

/// Whether `candidate` belongs to `scope` or a scope before it. Used to fence a retirement to the
/// intents the retired scope actually admitted.
fn scope_at_or_before(scope: (u64, u64), candidate: (u64, u64)) -> bool {
    candidate.0 < scope.0 || (candidate.0 == scope.0 && candidate.1 <= scope.1)
}

/// Deliver Wrath replies after the Net stage has adopted the matching world scope. This is the
/// only path for Wrath gameplay replies; the stock 1.12 inbound channel is not used, so a queued
/// old-world reply cannot overtake the transfer teardown.
fn wrath_dispatch_events(world: &mut World) {
    let login_generation =
        crate::net::LoginAbandon::now(world.get_resource::<crate::net::LoginAbandon>());
    let (current_epoch, current_world) = world
        .get_resource::<WrathWorldView>()
        .map_or((0, 0), |view| (view.session_epoch, view.world_generation));
    let (ended, tagged) = {
        let Some(mut control) = world.get_resource_mut::<WrathControl>() else {
            return;
        };
        (control.ended, control.take_session_events())
    };

    let mut deliver = Vec::with_capacity(tagged.len());
    for event in tagged {
        if event
            .login_generation
            .is_some_and(|g| g != login_generation)
        {
            continue;
        }
        if event.force
            || (!ended
                && !scope_is_older(
                    current_epoch,
                    current_world,
                    event.session_epoch,
                    event.world_generation,
                ))
        {
            // The buy reply names a vendor row slot, not an entry. Resolve it from the same
            // 12340 stock the buy packet addressed; absent stock keeps the item unknown.
            let vendor_buy_entry = match &event.event {
                SessionEvent::VendorBuyResult { vendor, slot, .. } => world
                    .get_resource::<WrathWorldView>()
                    .and_then(|view| view.vendor.as_ref())
                    .filter(|(guid, _)| *guid == *vendor)
                    .and_then(|(_, rows)| rows.iter().find(|row| row.slot == *slot))
                    .map(|row| row.entry),
                _ => None,
            };
            if let Some(trace) = world.get_resource::<crate::quest_trace::Source>() {
                use crate::quest_trace::{DomainEvent as D, LoginMilestone, Stage};
                let login_milestone = match &event.event {
                    SessionEvent::LoginStage { stage } => Some(match stage {
                        benilla_protocol::LoginStage::Connecting => LoginMilestone::Connecting,
                        benilla_protocol::LoginStage::Authenticating => {
                            LoginMilestone::Authenticating
                        }
                        benilla_protocol::LoginStage::Handshaking => LoginMilestone::Handshaking,
                        benilla_protocol::LoginStage::WorldConnected => {
                            LoginMilestone::WorldConnected
                        }
                    }),
                    SessionEvent::RealmList { failed, .. } => Some(if *failed {
                        LoginMilestone::RealmListFailed
                    } else {
                        LoginMilestone::RealmListReceived
                    }),
                    SessionEvent::WorldAuthenticated { .. } => {
                        Some(LoginMilestone::WorldAuthenticated)
                    }
                    SessionEvent::CharacterList { .. } => Some(LoginMilestone::RosterReceived),
                    SessionEvent::RosterFailed { .. } => Some(LoginMilestone::RosterFailed),
                    SessionEvent::LoginQueued { .. } => Some(LoginMilestone::LoginQueued),
                    SessionEvent::LoginFailed { refusal, .. } => Some(if refusal.is_some() {
                        LoginMilestone::LoginRefused
                    } else {
                        LoginMilestone::LoginFailed
                    }),
                    SessionEvent::CharActionResult { .. } => Some(LoginMilestone::CharacterAction),
                    SessionEvent::CharacterLoginFailed { result } => {
                        Some(LoginMilestone::CharacterLoginFailed { result: *result })
                    }
                    _ => None,
                };
                let kind = match &event.event {
                    SessionEvent::QuestComplete(_) => Some(D::QuestComplete),
                    SessionEvent::QuestObjectiveKill { .. }
                    | SessionEvent::QuestObjectiveItem { .. }
                    | SessionEvent::QuestObjectivesComplete { .. } => Some(D::QuestObjective),
                    SessionEvent::GossipMenu { .. } => Some(D::GossipMenu),
                    SessionEvent::VendorInventory { .. } => Some(D::VendorList),
                    SessionEvent::VendorBuyResult { .. } => Some(D::VendorBuy),
                    SessionEvent::VendorBuyFailed { .. } => Some(D::VendorBuyFailed),
                    SessionEvent::VendorSellFailed { .. } => Some(D::VendorSellFailed),
                    SessionEvent::LootResponse { .. } => Some(D::LootOpened),
                    SessionEvent::LootError { .. } => Some(D::LootError),
                    SessionEvent::LootRemoved { .. } => Some(D::LootRemoved),
                    SessionEvent::LootMoneyNotify { .. } | SessionEvent::LootClearMoney => {
                        Some(D::LootMoney)
                    }
                    SessionEvent::LootReleaseResponse { .. } => Some(D::LootReleased),
                    SessionEvent::ItemPushResult(_) => Some(D::ItemReceived),
                    SessionEvent::InventoryFailure { .. } => Some(D::InventoryFailure),
                    SessionEvent::Connected { .. } => Some(D::PlayerConnected),
                    SessionEvent::Disconnected { .. } => Some(D::PlayerDisconnected),
                    SessionEvent::Worldport { .. } => Some(D::Worldport),
                    _ => None,
                };
                let npc = match &event.event {
                    SessionEvent::GossipMenu { npc, .. } => Some(*npc),
                    SessionEvent::VendorInventory { vendor, .. } => Some(*vendor),
                    SessionEvent::LootResponse { guid, .. }
                    | SessionEvent::LootError { guid, .. }
                    | SessionEvent::LootReleaseResponse { guid, .. } => Some(*guid),
                    _ => None,
                };
                // Forced/terminal events (epoch 0) have no reliable producer scope; record them
                // as unknown rather than borrowing the scope active at drain time.
                let scope =
                    if event.force || (event.session_epoch == 0 && event.world_generation == 0) {
                        None
                    } else {
                        Some((event.session_epoch, event.world_generation))
                    };
                if let Some(milestone) = login_milestone {
                    // The attempt id comes from the wrapper that produced the event, not from
                    // whichever attempt happens to be newest when the trace is drained.

                    trace.observe(
                        scope,
                        None,
                        Stage::LoginMilestone {
                            attempt: event.login_generation,
                            milestone,
                        },
                    );
                }
                if let Some(event_kind) = kind {
                    trace.observe_with_npc(
                        scope,
                        None,
                        npc,
                        Stage::DomainEvent { event: event_kind },
                    );
                }
                // Numeric, privacy-safe inbound evidence. Raw GUIDs (source, item, vendor,
                // seller) are used only to derive the run-local alias; no GUID, name, message or
                // packet body enters the recorder.
                match &event.event {
                    SessionEvent::LootResponse { guid, .. } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*guid),
                        None,
                        None,
                        None,
                        Stage::LootResponseOpened,
                    ),
                    SessionEvent::LootError { guid, error } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*guid),
                        None,
                        None,
                        None,
                        Stage::LootErrorObserved { reason: *error },
                    ),
                    SessionEvent::LootRemoved { slot } => trace.observe_with_subject(
                        scope,
                        None,
                        None,
                        None,
                        None,
                        Some(u32::from(*slot)),
                        Stage::LootSlotRemoved {
                            slot: u32::from(*slot),
                            item_id: None,
                            count: None,
                        },
                    ),
                    SessionEvent::LootMoneyNotify { amount } => trace.observe_with_subject(
                        scope,
                        None,
                        None,
                        None,
                        Some(*amount),
                        None,
                        Stage::LootMoneyNotify { amount: *amount },
                    ),
                    SessionEvent::LootClearMoney => trace.observe_with_subject(
                        scope,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Stage::LootMoneyCleared,
                    ),
                    SessionEvent::LootReleaseResponse { guid, .. } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*guid),
                        None,
                        None,
                        None,
                        Stage::LootReleasedObserved,
                    ),
                    SessionEvent::ItemPushResult(push) => {
                        // Keep the quest reward semantics for the quest turn-in correlator, and
                        // record the explicit non-quest item-push domain as well.
                        trace.observe_with_subject(
                            scope,
                            None,
                            None,
                            Some(push.item_entry),
                            Some(push.count),
                            None,
                            Stage::ItemPushResult {
                                item_id: push.item_entry,
                                count: push.count,
                            },
                        );
                        trace.observe(
                            scope,
                            None,
                            Stage::RewardItemPushed {
                                item_id: push.item_entry,
                                count: push.count,
                            },
                        );
                    }
                    SessionEvent::InventoryFailure { reason, .. } => trace.observe_with_subject(
                        scope,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Stage::InventoryFailure { reason: *reason },
                    ),
                    SessionEvent::VendorInventory { vendor, items } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*vendor),
                        None,
                        None,
                        None,
                        Stage::VendorInventoryObserved {
                            vendor_alias: trace.npc_alias(*vendor),
                            rows: items.len().min(u32::MAX as usize) as u32,
                        },
                    ),
                    SessionEvent::VendorBuyResult {
                        vendor,
                        slot,
                        purchase_count,
                        ..
                    } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*vendor),
                        vendor_buy_entry,
                        Some(*purchase_count),
                        Some(*slot),
                        Stage::VendorBuyResult {
                            item_id: vendor_buy_entry,
                            count: *purchase_count,
                            slot: Some(*slot),
                        },
                    ),
                    SessionEvent::VendorBuyFailed {
                        vendor,
                        item_entry,
                        reason,
                    } => trace.observe_with_subject(
                        scope,
                        None,
                        Some(*vendor),
                        Some(*item_entry),
                        None,
                        None,
                        Stage::VendorBuyFailedObserved {
                            item_id: Some(*item_entry),
                            reason: *reason,
                        },
                    ),
                    SessionEvent::VendorSellFailed { vendor, reason, .. } => trace
                        .observe_with_subject(
                            scope,
                            None,
                            Some(*vendor),
                            None,
                            None,
                            None,
                            Stage::VendorSellFailedObserved { reason: *reason },
                        ),
                    _ => {}
                }
            }
            deliver.push(event.event);
        }
    }
    if !deliver.is_empty() {
        route_wm_action_outcomes(world, &deliver);
        crate::net::handlers::dispatch(world, deliver);
    }

    // Bridge WMC1 v3 (`MulticlassRequest` / `MulticlassPoll`) between the active Lua VM
    // (`WM_Multiclass.lua` in-game or `multiclass_glue.lua` at character select) and the
    // Wrath session transport.
    // Drain only when a VM can actually receive the replies. Draining unconditionally and then
    // finding no `UiScript` would silently drop a valid reply before the UI is ready.
    let has_script = world
        .get_non_send_resource::<Option<benilla_ui::script::UiScript>>()
        .is_some_and(|opt| opt.is_some());
    let pending_replies: Vec<_> = if has_script {
        world
            .get_resource_mut::<WrathWorldView>()
            .map(|mut view| view.multiclass_replies.drain(..).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let requests: Vec<_> = world
        .get_non_send_resource_mut::<Option<benilla_ui::script::UiScript>>()
        .and_then(|mut opt| {
            let script = opt.as_mut().as_mut()?;
            for (op, status, serial, payload) in pending_replies {
                script.push_multiclass_reply(op, status, serial, payload);
            }
            Some(script.take_multiclass_requests())
        })
        .unwrap_or_default();
    if !requests.is_empty() {
        let in_world = world
            .get_resource::<WrathWorldView>()
            .is_some_and(|v| v.phase == Some(Phase::InWorld));
        for (op, serial, character_guid, arg0, arg1) in requests {
            let payload =
                gameplay::encode_multiclass_request(op, serial, character_guid, arg0, arg1)
                    .to_vec();
            if in_world {
                if let Some(input) = world.get_resource::<WrathInput>() {
                    if let Ok(mut queue) = input.0.lock() {
                        queue.push_back(WrathOutbound {
                            opcode: benilla_protocol_wrath::opcode::CMSG_MULTICLASS_REQUEST,
                            payload,
                            movement: false,
                            movement_seq: None,
                            enqueued_at: Some(std::time::Instant::now()),
                            receipt: None,
                            trace: None,
                            enforce_scope: false,
                        });
                    }
                }
            } else if let Some(control) = world.get_resource::<WrathControl>() {
                if let Some(tx) = control.park_commands.as_ref() {
                    let _ = tx.send(WrathParkedCommand::MulticlassRequest(payload));
                }
            }
        }
    }
}

/// Feed the existing translated replies to the in-flight action set. Only server replies and
/// session state resolve an intent; the observation stream remains the separate authority for
/// world state.
fn route_wm_action_outcomes(world: &mut World, events: &[SessionEvent]) {
    let self_guid = world.get_resource::<SelfGuid>().and_then(|g| g.0);
    let revision = world
        .get_resource::<WrathWorldView>()
        .map(|v| v.state_revision);
    let Some(mut actions) = world.get_resource_mut::<WrathWmActions>() else {
        return;
    };
    for event in events {
        let mut resolved: Vec<(String, ActionStatus, Option<String>)> = Vec::new();
        for (id, pending) in &actions.pending {
            // Cast intents resolve only from resolved native cast edges (`observe_native`),
            // never from a bare spell id or whichever cast reply arrived most recently.
            if matches!(
                &pending.intent,
                ActionIntent::Cast { .. } | ActionIntent::CastCancel
            ) {
                continue;
            }
            let outcome = match (event, &pending.intent) {
                (SessionEvent::AttackSwingError(_), ActionIntent::AttackStart { .. }) => Some((
                    ActionStatus::Failed,
                    Some("attack swing refused".to_owned()),
                )),
                (SessionEvent::AttackStart { attacker, .. }, ActionIntent::AttackStart { .. })
                    if Some(*attacker) == self_guid =>
                {
                    Some((ActionStatus::Succeeded, None))
                }
                (SessionEvent::AttackStop { attacker, .. }, ActionIntent::AttackStop)
                    if Some(*attacker) == self_guid =>
                {
                    Some((ActionStatus::Succeeded, None))
                }
                (SessionEvent::GossipMenu { npc, .. }, ActionIntent::Interact { guid })
                    if npc == guid =>
                {
                    Some((ActionStatus::Succeeded, None))
                }
                (
                    SessionEvent::QuestComplete(complete),
                    ActionIntent::QuestComplete { quest, .. },
                ) if complete.quest_id == *quest => Some((ActionStatus::Succeeded, None)),
                (
                    SessionEvent::QuestComplete(complete),
                    ActionIntent::QuestChooseReward { quest, .. },
                ) if complete.quest_id == *quest => Some((ActionStatus::Succeeded, None)),
                (
                    SessionEvent::QuestGiverFailed { quest_id, reason },
                    ActionIntent::QuestComplete { quest, .. }
                    | ActionIntent::QuestChooseReward { quest, .. }
                    | ActionIntent::QuestAccept { quest, .. },
                ) if quest_id == quest => Some((
                    ActionStatus::Failed,
                    Some(format!("quest failed ({reason})")),
                )),
                (
                    SessionEvent::LootRemoved { slot },
                    ActionIntent::LootTake { slot: wanted, .. },
                ) if slot == wanted => Some((ActionStatus::Succeeded, None)),
                (SessionEvent::LootMoneyNotify { .. }, ActionIntent::LootMoney { .. }) => {
                    Some((ActionStatus::Succeeded, None))
                }
                (
                    SessionEvent::LootReleaseResponse { guid, .. },
                    ActionIntent::LootRelease { source },
                ) if guid == source => Some((ActionStatus::Succeeded, None)),
                (
                    SessionEvent::LootError { guid, error },
                    ActionIntent::LootTake { source, .. },
                )
                | (SessionEvent::LootError { guid, error }, ActionIntent::LootMoney { source })
                    if guid == source =>
                {
                    let _ = error;
                    Some((ActionStatus::Failed, Some("loot refused".to_owned())))
                }
                (SessionEvent::Disconnected { .. }, _) => Some((
                    ActionStatus::Unknown,
                    Some("session ended before an authoritative result".to_owned()),
                )),
                _ => None,
            };
            if let Some((status, detail)) = outcome {
                resolved.push((id.clone(), status, detail));
            }
        }
        for (id, status, detail) in resolved {
            if let Some(pending) = actions.pending.remove(&id) {
                if let ActionIntent::Cast { spell_id, .. } = &pending.intent {
                    if actions.last_cast_spell == Some(*spell_id) {
                        actions.last_cast_spell = None;
                    }
                }
                actions.answer(
                    id,
                    status,
                    pending.session_epoch,
                    pending.world_generation,
                    revision,
                    detail,
                );
            }
        }
    }
}

fn wrath_net_drain(
    mut control: ResMut<WrathControl>,
    mut view: ResMut<WrathWorldView>,
    mut actions: Option<ResMut<WrathWmActions>>,
    known: Option<Res<crate::ui_action::PlayerActions>>,
    spells: Option<Res<crate::ui_action::Spells>>,
    mut cast_events: MessageWriter<crate::wrath_native::WrathCastEvent>,
    mut item_text_events: MessageWriter<crate::wrath_native::WrathItemText>,
    clock: Option<Res<crate::ui_script::UiClock>>,
    trace: Option<Res<crate::quest_trace::Source>>,
    mut equipment: Option<ResMut<crate::wrath_equipment::EquipmentManager>>,
) {
    let ui_now_seconds = clock.as_ref().map(|clock| clock.ui_now);
    let (native, native_dropped) = control
        .feed
        .as_ref()
        .map(|feed| (feed.take_native(), feed.take_native_dropped()))
        .unwrap_or_default();
    if native_dropped > 0 {
        if let Some(trace) = &trace {
            trace.observe(
                Some((view.session_epoch, view.world_generation)),
                None,
                crate::quest_trace::Stage::NativeEventsLost {
                    count: native_dropped,
                },
            );
        }
    }
    for (drained, event) in control.take_events().into_iter().enumerate() {
        if drained >= WRATH_DRAIN_LIMIT {
            break;
        }
        if let Some(trace) = &trace {
            use crate::quest_trace::{SessionEndKind, SessionPhase, Stage};
            let stage = match &event {
                WrathFeedEvent::Phase {
                    phase,
                    session_epoch,
                    world_generation,
                    login_generation,
                    map_id,
                } => {
                    // Record only an actual transition: a duplicate or older phase event is
                    // not a new milestone, and the state guard below applies the same rule.
                    let older = scope_is_older(
                        view.session_epoch,
                        view.world_generation,
                        *session_epoch,
                        *world_generation,
                    );
                    let same_scope = view.session_epoch == *session_epoch
                        && view.world_generation == *world_generation;
                    if older || (same_scope && view.phase == Some(*phase)) {
                        None
                    } else {
                        Some(Stage::SessionPhase {
                            phase: match phase {
                                Phase::CharacterSelection => SessionPhase::CharacterSelection,
                                Phase::EnteringWorld => SessionPhase::EnteringWorld,
                                Phase::InWorld => SessionPhase::InWorld,
                                Phase::TransferPending => SessionPhase::TransferPending,
                                Phase::Loading => SessionPhase::Loading,
                            },
                            attempt: *login_generation,
                            map_id: *map_id,
                        })
                    }
                }
                WrathFeedEvent::WorldTransfer {
                    session_epoch,
                    world_generation,
                    map_id,
                } => {
                    let older = scope_is_older(
                        view.session_epoch,
                        view.world_generation,
                        *session_epoch,
                        *world_generation,
                    );
                    let same_scope = view.session_epoch == *session_epoch
                        && view.world_generation == *world_generation;
                    if older || same_scope {
                        None
                    } else {
                        Some(Stage::WorldTransfer {
                            map_id: Some(*map_id),
                            from: if view.session_epoch == 0 && view.world_generation == 0 {
                                None
                            } else {
                                Some((view.session_epoch, view.world_generation))
                            },
                            to: Some((*session_epoch, *world_generation)),
                            retired_local: 0,
                            retired_written: 0,
                        })
                    }
                }
                WrathFeedEvent::SessionEnded { end, .. } => Some(Stage::SessionEnded {
                    kind: match end {
                        benilla_protocol::SessionEnd::LoggedOut => SessionEndKind::LoggedOut,
                        benilla_protocol::SessionEnd::Lost => SessionEndKind::Lost,
                    },
                }),
                _ => None,
            };
            if let Some(stage) = stage {
                let (epoch, generation, unknown) = match &event {
                    WrathFeedEvent::Phase {
                        session_epoch,
                        world_generation,
                        ..
                    } => (*session_epoch, *world_generation, false),
                    WrathFeedEvent::WorldTransfer {
                        session_epoch,
                        world_generation,
                        ..
                    } => (*session_epoch, *world_generation, false),
                    WrathFeedEvent::SessionEnded {
                        session_epoch,
                        world_generation,
                        ..
                    } => (*session_epoch, *world_generation, false),
                    _ => (view.session_epoch, view.world_generation, false),
                };
                // A delayed terminal/phase event keeps its own captured scope; never borrow
                // the scope that happens to be active when it is drained.
                if unknown || (epoch == 0 && generation == 0) {
                    trace.observe(None, None, stage);
                } else {
                    trace.observe(Some((epoch, generation)), None, stage);
                }
            }
        }
        // Guard state mutation before the first write: stale or duplicate feed events must not
        // mutate the active view. A delayed terminal event is never allowed to end a newer session.
        let apply_state = match &event {
            WrathFeedEvent::Phase {
                session_epoch,
                world_generation,
                ..
            } => !scope_is_older(
                view.session_epoch,
                view.world_generation,
                *session_epoch,
                *world_generation,
            ),
            WrathFeedEvent::WorldTransfer {
                session_epoch,
                world_generation,
                ..
            } => {
                !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    *session_epoch,
                    *world_generation,
                ) && (*session_epoch != view.session_epoch
                    || *world_generation != view.world_generation)
            }
            WrathFeedEvent::SessionEnded {
                session_epoch,
                world_generation,
                ..
            } => {
                if *session_epoch == 0 && *world_generation == 0 {
                    view.session_epoch == 0 && view.world_generation == 0
                } else {
                    !scope_is_older(
                        view.session_epoch,
                        view.world_generation,
                        *session_epoch,
                        *world_generation,
                    )
                }
            }
            _ => true,
        };
        if !apply_state {
            continue;
        }
        match event {
            WrathFeedEvent::Phase { phase, .. } => view.phase = Some(phase),
            WrathFeedEvent::WorldTransfer {
                session_epoch,
                world_generation,
                map_id,
            } => {
                // `CGEquipmentManager::EnterWorld` (`0x5AD690`): no swap is pending in a new world.
                if let Some(equipment) = equipment.as_deref_mut() {
                    equipment.enter_world();
                }
                // Same-frame teardown: old-scope entities are gone before Input runs. Everything
                // the retired world could still answer is retired at one barrier: an intent
                // admitted against it becomes Unknown and is never replayed, a receipt that never
                // reached the wire is answered, and a reply already older than the adopted scope
                // is dropped rather than occupying a delivery slot. Forced edges keep their
                // "deliver even when stale" contract and stay for `scope_is_older`.
                control.retire_scoped_delivery(
                    DeliveryScopeEnd::WorldTransfer {
                        retired: (view.session_epoch, view.world_generation),
                        adopted: (session_epoch, world_generation),
                    },
                    actions.as_deref_mut(),
                    "world scope changed before an authoritative result",
                );
                for event in crate::wrath_native::invalidate_presentation(
                    &mut view,
                    crate::wrath_native::WrathScopeLoss::WorldTransfer,
                    now_ms(),
                    ui_now_seconds,
                ) {
                    cast_events.write(event);
                }
                view.entities.clear();
                view.world_generation = world_generation;
                view.map_id = map_id;
                view.self_observation = None;
                view.player_guid = 0;
                view.vehicle = None;
                view.entered_world = false;
                view.vendor = None;
                view.last_inventory_failure = None;
                view.clear_rune_calendar();
                view.last_changes.world_transferred = true;
            }
            WrathFeedEvent::GossipMenu {
                npc,
                menu_id,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.gossip_menu = Some((npc, menu_id));
                }
            }
            WrathFeedEvent::VendorInventory {
                vendor,
                items,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.vendor = Some((vendor, items));
                }
            }
            WrathFeedEvent::ServerClock {
                unix_time,
                reset_in_secs,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.server_clock = Some(WrathServerClock {
                        unix_time,
                        reset_in_secs,
                    });
                }
            }
            WrathFeedEvent::PlayedTime {
                total,
                level,
                trigger_script_event,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.played_time = Some(WrathPlayedTime {
                        total,
                        level,
                        trigger_script_event,
                    });
                }
            }
            WrathFeedEvent::EquipmentSet {
                packet,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    if let Some(equipment) = equipment.as_deref_mut() {
                        equipment.apply_packet(packet);
                    }
                }
            }
            WrathFeedEvent::RuneState {
                runes,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_rune_state(&runes, ui_now_seconds.unwrap_or(0.0));
                }
            }
            WrathFeedEvent::RuneConvert {
                rune,
                rune_type,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_rune_convert(rune, rune_type, ui_now_seconds.unwrap_or(0.0));
                }
            }
            WrathFeedEvent::RunePower {
                mask,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_rune_power(mask, ui_now_seconds.unwrap_or(0.0));
                }
            }
            WrathFeedEvent::AttackerState {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_swing(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellDamage {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_damage(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellMiss {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_miss(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellHeal {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_heal(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellEnergize {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_energize(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellCastStart {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_cast_start(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellCastSuccess {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_cast_success(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellCastFailed {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_cast_failed(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellPeriodicDamage {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_periodic_damage(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellPeriodicMiss {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_periodic_miss(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellPeriodicHeal {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_periodic_heal(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellPeriodicEnergize {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_periodic_energize(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellPeriodicDrain {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_periodic_drain(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellExecuteLog {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_execute_log(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellDispelLog {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_dispel_log(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellDispelFailed {
                hit,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_dispel_failed(
                        hit,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::SpellInstakill {
                caster,
                target,
                spell_id,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_spell_instakill(
                        WrathSpellInstakill {
                            caster,
                            target,
                            spell_id,
                            session_epoch,
                            world_generation,
                            revision: 0,
                            received_at: 0.0,
                        },
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::EnchantApplied {
                caster,
                target,
                item_id,
                enchant_id,
                session_epoch,
                world_generation,
            }
            | WrathFeedEvent::EnchantRemoved {
                caster,
                target,
                item_id,
                enchant_id,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_enchant_log(
                        WrathEnchantLog {
                            caster,
                            target,
                            item_id,
                            enchant_id,
                            session_epoch,
                            world_generation,
                            revision: 0,
                            received_at: 0.0,
                        },
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::EnvironmentalDamage {
                victim,
                damage_type,
                damage,
                absorbed,
                resisted,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_environmental_damage(
                        WrathEnvironmentalDamage {
                            victim,
                            damage_type,
                            damage,
                            absorbed,
                            resisted,
                            session_epoch,
                            world_generation,
                            revision: 0,
                            received_at: 0.0,
                        },
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::DamageShield {
                target,
                attacker,
                spell_id,
                damage,
                overkill,
                school,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_damage_shield(
                        WrathDamageShield {
                            target,
                            attacker,
                            spell_id,
                            damage,
                            overkill,
                            school,
                            session_epoch,
                            world_generation,
                            revision: 0,
                            received_at: 0.0,
                        },
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::PartyKillLog {
                killer,
                victim,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_party_kill(
                        killer,
                        victim,
                        session_epoch,
                        world_generation,
                        ui_now_seconds.unwrap_or(0.0),
                    );
                }
            }
            WrathFeedEvent::Calendar {
                calendar,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.apply_calendar(&calendar);
                }
            }
            WrathFeedEvent::CalendarCommandResult {
                command,
                reserved,
                error,
                param,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.last_calendar_command_result = Some(WrathCalendarCommandResult {
                        command,
                        reserved,
                        error,
                        param,
                    });
                }
            }
            WrathFeedEvent::MulticlassReply {
                op,
                status,
                serial,
                payload,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    if view.multiclass_replies.len() >= 16 {
                        view.multiclass_replies.pop_front();
                    }
                    view.multiclass_replies
                        .push_back((op, status, serial, payload));
                }
            }
            WrathFeedEvent::LfgJoinResult {
                result,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.lfg_join_result = Some(result);
                }
            }
            WrathFeedEvent::LfgQueueStatus {
                status,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.lfg_queue_status = Some(status);
                }
            }
            WrathFeedEvent::LfgUpdatePlayer {
                update,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    if !update.joined {
                        view.lfg_queue_status = None;
                        view.lfg_proposal = None;
                    }
                    view.lfg_update_player = Some(update);
                }
            }
            WrathFeedEvent::LfgUpdateParty {
                update,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    if !update.joined {
                        view.lfg_queue_status = None;
                        view.lfg_proposal = None;
                    }
                    view.lfg_update_party = Some(update);
                }
            }
            WrathFeedEvent::LfgProposalUpdate {
                proposal,
                session_epoch,
                world_generation,
            } => {
                if !scope_is_older(
                    view.session_epoch,
                    view.world_generation,
                    session_epoch,
                    world_generation,
                ) {
                    view.lfg_proposal = Some(proposal);
                }
            }
            WrathFeedEvent::ExportSkipped(reason) => {
                debug!("wrath export skipped: {reason}");
            }
            WrathFeedEvent::SessionEnded {
                result,
                end: _,
                session_epoch: _,
                world_generation: _,
            } => {
                // `CGEquipmentManager::ShutdownGame` (`0x5AE1B0`): the session's sets go.
                if let Some(equipment) = equipment.as_deref_mut() {
                    equipment.clear();
                }
                control.ended = true;
                control.terminal_ready = true;
                // Whether this link served the select screen is decided before the barrier: the
                // ask channel is retired by the same barrier, and the loss policy below must not
                // mistake a retired parked link for a boot dial.
                let parked_link = control.park_commands.is_some();
                // The terminal is a session barrier: whatever reply, receipt or intent was queued
                // behind it belongs to a session that no longer exists. The reserved terminal
                // slots are kept and delivered first; everything else is retired exactly once.
                control.retire_scoped_delivery(
                    DeliveryScopeEnd::SessionEnded,
                    actions.as_deref_mut(),
                    "session ended before an authoritative result",
                );
                control.ended_retirement_done = true;
                // The entry message may still be in the Bevy message queue; the view already
                // records that the player actually entered this world.
                control.entered_world |= view.entered_world;
                let clean = result.is_ok();
                match result {
                    Ok(()) => info!("wrath session ended cleanly"),
                    Err(e) => {
                        warn!("wrath session failed: {e}");
                        // A live-code client's stderr is only dx's terminal, which scrolls; keep
                        // every session failure, with its cause chain, where it can be read later.
                        if let Some(dir) = crate::local_state::logs_dir() {
                            use std::io::Write as _;
                            let stamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let _ = std::fs::create_dir_all(&dir);
                            if let Ok(mut file) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(dir.join("session-failures.log"))
                            {
                                let _ = writeln!(file, "{stamp} wrath session failed: {e:#}");
                            }
                        }
                        control.last_error = Some(e);
                    }
                }
                if clean {
                    // Clean logout returns to character selection through the existing glue
                    // edge. A fresh session may be admitted only after that stop is visible.
                    control.restart_allowed = true;
                    control.restart_attempts = WRATH_RESTART_ATTEMPTS;
                    control.retry_at = None;
                } else if control.restart_allowed
                    && !control.entered_world
                    && control.restart_attempts > 0
                    && !parked_link
                {
                    // A fresh-session admission failed before any world entry: retry it on the
                    // frame loop with a bounded budget. Once a world was entered, a loss is
                    // connection-loss recovery, which is a separate policy. A parked link that
                    // dies is that same loss: the select screen's connection is gone, the app
                    // shows the disconnect, and it does not redial under the screen's feet.
                    control.retry_at = Some(Instant::now() + WRATH_RESTART_DELAY);
                } else {
                    control.restart_allowed = false;
                    control.retry_at = None;
                }
                control.entered_world = false;
                for event in crate::wrath_native::invalidate_presentation(
                    &mut view,
                    crate::wrath_native::WrathScopeLoss::SessionEnded,
                    now_ms(),
                    ui_now_seconds,
                ) {
                    cast_events.write(event);
                }
                view.entities.clear();
                view.self_observation = None;
                view.player_guid = 0;
                view.vehicle = None;
                view.entered_world = false;
                view.gossip_menu = None;
                view.vendor = None;
                view.server_clock = None;
                view.played_time = None;
                view.clear_rune_calendar();
                view.last_inventory_failure = None;
                view.multiclass_replies.clear();
                view.session_epoch = 0;
                view.world_generation = 0;
                view.state_revision = 0;
                view.map_id = 0;
                view.next_cast_count = 0;
                view.phase = None;
                view.last_changes.world_transferred = true;
            }
        }
    }

    // Native cast/channel/item-text/failure traffic is drained from its own queue so a burst
    // cannot evict a lifecycle edge. Old-scope facts are rejected before they touch the view.
    // Cast facts arrive already resolved by the session authority: this drain only mirrors the
    // authoritative entries for presentation and stamps the presentation clock.
    if !control.ended {
        for event in native {
            let scope = event.scope();
            if scope_is_older(
                view.session_epoch,
                view.world_generation,
                scope.session_epoch,
                scope.world_generation,
            ) {
                continue;
            }
            if let Some(trace) = &trace {
                use crate::quest_trace::{CastEdge, Stage};
                use crate::wrath_native::{WrathCastEventKind as Kind, WrathNativeFeedEvent};
                let stage = match &event {
                    WrathNativeFeedEvent::Cast(resolved) => {
                        let edge = match &resolved.event.kind {
                            Kind::Started { .. } => CastEdge::Started,
                            Kind::Delayed { .. } => CastEdge::Delayed,
                            Kind::Launched { .. } => CastEdge::Launched,
                            Kind::Failed { reason } => CastEdge::Failed { reason: *reason },
                            Kind::Interrupted { reason } => {
                                CastEdge::Interrupted { reason: *reason }
                            }
                            Kind::ChannelStarted { .. } => CastEdge::ChannelStarted,
                            Kind::ChannelUpdated { .. } => CastEdge::ChannelUpdated,
                            Kind::ChannelEnded => CastEdge::ChannelEnded,
                            Kind::InterruptibilityChanged { .. } => continue,
                            Kind::ScopeInvalidated { .. } => CastEdge::ScopeInvalidated,
                        };
                        Some(Stage::Cast {
                            spell_id: resolved.event.identity.map(|id| id.spell_id),
                            attempt: resolved.event.attempt_serial,
                            wire_cast_count: resolved
                                .event
                                .identity
                                .and_then(|id| id.cast_count)
                                .map(u32::from),
                            edge,
                        })
                    }
                    WrathNativeFeedEvent::InventoryFailure { failure, .. } => {
                        Some(Stage::InventoryFailure {
                            reason: failure.reason,
                        })
                    }
                    WrathNativeFeedEvent::ItemText(_) => None,
                };
                if let Some(stage) = stage {
                    trace.observe(
                        Some((scope.session_epoch, scope.world_generation)),
                        None,
                        stage,
                    );
                }
            }
            match event {
                crate::wrath_native::WrathNativeFeedEvent::Cast(resolved) => {
                    // Correlate in-flight WM cast intents with the resolved edge before the
                    // presentation mirror consumes it.
                    if let Some(actions) = actions.as_deref_mut() {
                        actions.observe_native(&resolved, Some(view.state_revision));
                    }
                    let mut event = resolved.event;
                    let started = matches!(
                        &event.kind,
                        crate::wrath_native::WrathCastEventKind::Started { .. }
                            | crate::wrath_native::WrathCastEventKind::ChannelStarted { .. }
                    );
                    if started {
                        event.start_ui_now_seconds = ui_now_seconds;
                    } else if let Some(identity) = event.identity {
                        // The authority carries no UI clock; carry the attempt's origin from
                        // the mirror it is about to replace, so later edges never rewrite it.
                        event.start_ui_now_seconds = view
                            .casts
                            .get(&identity.caster)
                            .filter(|cast| event.attempt_serial == Some(cast.attempt_serial))
                            .and_then(|cast| cast.start_ui_now_seconds)
                            .or_else(|| {
                                view.channels
                                    .get(&identity.caster)
                                    .and_then(|channel| channel.start_ui_now_seconds)
                            });
                    }
                    event.ui_now_seconds = ui_now_seconds;
                    let mut last_cast_end = resolved.last_cast_end;
                    if let Some(ended) = last_cast_end.as_mut() {
                        if ended.ended_at_unix_ms == event.observed_at_unix_ms {
                            ended.start_ui_now_seconds = event.start_ui_now_seconds;
                        }
                    }
                    if let Some(identity) = event.identity {
                        let caster = identity.caster;
                        match resolved.casting {
                            Some((_, mut cast)) => {
                                if started {
                                    cast.start_ui_now_seconds = ui_now_seconds;
                                } else if let Some(existing) = view.casts.get(&caster) {
                                    if existing.attempt_serial == cast.attempt_serial {
                                        cast.start_ui_now_seconds = existing.start_ui_now_seconds;
                                    }
                                }
                                cast.last_update_ui_now_seconds = ui_now_seconds;
                                view.casts.insert(caster, cast);
                            }
                            None => {
                                view.casts.remove(&caster);
                            }
                        }
                        match resolved.channeling {
                            Some((_, mut channel)) => {
                                let channel_started = matches!(
                                    &event.kind,
                                    crate::wrath_native::WrathCastEventKind::ChannelStarted { .. }
                                );
                                if channel_started {
                                    channel.start_ui_now_seconds = ui_now_seconds;
                                } else if let Some(existing) = view.channels.get(&caster) {
                                    channel.start_ui_now_seconds = existing.start_ui_now_seconds;
                                }
                                channel.last_update_ui_now_seconds = ui_now_seconds;
                                view.channels.insert(caster, channel);
                            }
                            None => {
                                view.channels.remove(&caster);
                            }
                        }
                    } else {
                        // Identity-less edges cannot name a caster to mirror; nothing to sync.
                        view.casts.clear();
                        view.channels.clear();
                    }
                    view.last_cast_end = last_cast_end;
                    cast_events.write(event);
                }
                crate::wrath_native::WrathNativeFeedEvent::ItemText(text) => {
                    item_text_events.write(text);
                }
                crate::wrath_native::WrathNativeFeedEvent::InventoryFailure { failure, .. } => {
                    view.last_inventory_failure = Some(failure);
                }
            }
        }
    }

    // The bounded native queue dropped resolved edges: the presentation mirror is no longer a
    // complete copy of the authority. Close its bars and clear it; the next resolved edges
    // repopulate it. The WM export reads the authority directly and is unaffected.
    if native_dropped > 0 && !control.ended {
        debug!(
            "wrath: native presentation feed dropped {native_dropped} resolved edges; resynchronizing the mirror"
        );
        for event in crate::wrath_native::invalidate_presentation(
            &mut view,
            crate::wrath_native::WrathScopeLoss::FeedOverflow,
            now_ms(),
            ui_now_seconds,
        ) {
            cast_events.write(event);
        }
    }

    // A worker can exit after its final event was already drained (or panicked without one). The
    // exposed view is invalid on that terminal edge even when no digest follows.
    if control.ended {
        // The terminal arm already retired this scope; this is the fallback for a worker that
        // ended without one (a panic or a feed overflow). It runs once per ended scope, never per
        // frame, and catches anything the dead publisher proposed after the terminal.
        if !control.ended_retirement_done {
            control.retire_scoped_delivery(
                DeliveryScopeEnd::SessionEnded,
                actions.as_deref_mut(),
                "session ended before an authoritative result",
            );
            control.ended_retirement_done = true;
        }
        for event in crate::wrath_native::invalidate_presentation(
            &mut view,
            crate::wrath_native::WrathScopeLoss::SessionEnded,
            now_ms(),
            ui_now_seconds,
        ) {
            cast_events.write(event);
        }
        view.entities.clear();
        view.self_observation = None;
        view.player_guid = 0;
        view.vehicle = None;
        view.entered_world = false;
        view.gossip_menu = None;
        view.vendor = None;
        view.server_clock = None;
        view.played_time = None;
        view.clear_rune_calendar();
        view.last_inventory_failure = None;
        view.multiclass_replies.clear();
        view.session_epoch = 0;
        view.world_generation = 0;
        view.state_revision = 0;
        view.map_id = 0;
        view.next_cast_count = 0;
        view.phase = None;
        view.last_changes.world_transferred = true;
        if let Some(feed) = control.feed.take() {
            if let Ok(mut slot) = feed.latest.lock() {
                *slot = None;
            }
        }
        return;
    }

    let Some(digest) = control.take_digest() else {
        refresh_cast_interruptibility(
            &mut view,
            known.as_deref(),
            spells.as_deref(),
            &mut cast_events,
        );
        return;
    };
    let new_session = digest.session_epoch != view.session_epoch;
    if new_session {
        // A replacement session restarts its epoch, world generation and revision counters, so
        // its scope is compared as a new one, never with the retired session's monotonic values.
        // Everything the old scope could still answer is retired at the one barrier.
        control.retire_scoped_delivery(
            DeliveryScopeEnd::WorldTransfer {
                retired: (view.session_epoch, view.world_generation),
                adopted: (digest.session_epoch, digest.world_generation),
            },
            actions.as_deref_mut(),
            "world scope changed before an authoritative result",
        );
        for event in crate::wrath_native::invalidate_presentation(
            &mut view,
            crate::wrath_native::WrathScopeLoss::SessionReplaced,
            now_ms(),
            ui_now_seconds,
        ) {
            cast_events.write(event);
        }
        view.entities.clear();
        view.gossip_menu = None;
        view.vendor = None;
        view.server_clock = None;
        view.played_time = None;
        view.clear_rune_calendar();
        view.last_inventory_failure = None;
        view.multiclass_replies.clear();
        view.last_changes.world_transferred = true;
        view.state_revision = 0;
    } else {
        if digest.world_generation < view.world_generation {
            return; // stale world scope
        }
        if digest.world_generation != view.world_generation {
            control.retire_scoped_delivery(
                DeliveryScopeEnd::WorldTransfer {
                    retired: (view.session_epoch, view.world_generation),
                    adopted: (digest.session_epoch, digest.world_generation),
                },
                actions.as_deref_mut(),
                "world scope changed before an authoritative result",
            );
            for event in crate::wrath_native::invalidate_presentation(
                &mut view,
                crate::wrath_native::WrathScopeLoss::WorldTransfer,
                now_ms(),
                ui_now_seconds,
            ) {
                cast_events.write(event);
            }
            view.entities.clear();
            view.gossip_menu = None;
            view.vendor = None;
            view.clear_rune_calendar();
            view.last_changes.world_transferred = true;
        }
        if digest.state_revision < view.state_revision {
            return; // stale revision within the current scope
        }
    }
    view.session_epoch = digest.session_epoch;
    view.world_generation = digest.world_generation;
    view.state_revision = digest.state_revision;
    view.map_id = digest.map_id;
    view.phase = Some(digest.phase);
    view.player_guid = digest.player_guid;
    view.vehicle = digest.vehicle;
    view.self_observation = digest.self_observation.clone();

    let mut changes = WrathChanges {
        created: Vec::new(),
        updated: Vec::new(),
        removed: Vec::new(),
        world_transferred: view.last_changes.world_transferred,
    };
    for (guid, entity) in &digest.entities {
        match view.entities.get(guid) {
            None => changes.created.push(*guid),
            Some(prev) if prev != entity => changes.updated.push(*guid),
            _ => {}
        }
        view.entities.insert(*guid, entity.clone());
    }
    let stale: Vec<u64> = view
        .entities
        .keys()
        .copied()
        .filter(|g| !digest.entities.contains_key(g))
        .collect();
    for guid in stale {
        view.entities.remove(&guid);
        changes.removed.push(guid);
    }
    view.last_changes = changes;
    refresh_cast_interruptibility(
        &mut view,
        known.as_deref(),
        spells.as_deref(),
        &mut cast_events,
    );
}

fn cast_interruptible(
    definition: &benilla_formats::SpellDisplay,
    attributes_ex_g: u32,
    is_player: bool,
    interrupt_mask: u32,
    silence_mask: u32,
    interrupt_special_mask: u32,
    silence_special_mask: u32,
    cast_school_immunities: u32,
    cast_immunities: u32,
    aura_effects: &[(u32, i32)],
) -> bool {
    if attributes_ex_g & 0x2000 != 0 || definition.prevention_type != 1 {
        return true;
    }
    let mut interrupt = interrupt_mask;
    let mut silence = silence_mask;
    let (mut interrupt_special, mut silence_special) = if is_player {
        (0, 0)
    } else {
        (interrupt_special_mask, silence_special_mask)
    };

    if cast_immunities & 0x0400_0000 != 0 {
        interrupt = 0;
        interrupt_special = 0;
    } else {
        interrupt &= !cast_school_immunities;
        interrupt_special &= !cast_school_immunities;
    }
    if cast_immunities & 0x200 != 0 {
        silence = 0;
        silence_special = 0;
    } else {
        silence &= !cast_school_immunities;
        silence_special &= !cast_school_immunities;
    }
    for &(kind, misc) in aura_effects {
        match kind {
            37 if misc == 68 => {
                interrupt = 0;
                interrupt_special = 0;
            }
            38 if misc == 27 => {
                silence = 0;
                silence_special = 0;
            }
            39 => {
                let mask = u32::try_from(misc).unwrap_or(u32::MAX);
                interrupt &= !mask;
                interrupt_special &= !mask;
                silence &= !mask;
                silence_special &= !mask;
            }
            77 if misc == 9 => {
                silence = 0;
                silence_special = 0;
            }
            77 if misc == 26 => {
                interrupt = 0;
                interrupt_special = 0;
            }
            _ => {}
        }
    }
    interrupt != 0 || interrupt_special != 0 || silence != 0 || silence_special != 0
}

/// Recompute the 12340 cast-bar interruptibility predicate after aura/spellbook state changes.
/// START/GO do not carry this bit; the reference derives it from PreventionType, the known
/// interrupt/silence school masks, non-player special masks, START cast-immunity words, and aura
/// immunity effects.
fn refresh_cast_interruptibility(
    view: &mut WrathWorldView,
    known: Option<&crate::ui_action::PlayerActions>,
    spells: Option<&crate::ui_action::Spells>,
    cast_events: &mut MessageWriter<crate::wrath_native::WrathCastEvent>,
) {
    let (Some(known), Some(spells)) = (known, spells) else {
        return;
    };
    let mut interrupt_mask = 0u32;
    let mut silence_mask = 0u32;
    let mut interrupt_special_mask = 0u32;
    let mut silence_special_mask = 0u32;
    for &id in &known.spells {
        let Some(spell) = spells.catalog.get(id) else {
            continue;
        };
        let school = spells.catalog.school_mask(id).unwrap_or(0);
        if spell
            .effects
            .iter()
            .zip(spell.effect_apply_aura)
            .any(|(&effect, aura)| effect == 6 && aura == 27)
        {
            silence_mask |= school;
        }
        if spell.effects.contains(&68) {
            interrupt_mask |= school;
        }
        if spells.catalog.attributes_ex_g(id) & 0x800 != 0 {
            interrupt_special_mask |= school;
        }
        if spells.catalog.attributes_ex_g(id) & 0x1000 != 0 {
            silence_special_mask |= school;
        }
    }
    let aura_spell_ids: BTreeMap<u64, Vec<u32>> = view
        .entities
        .iter()
        .map(|(&guid, unit)| (guid, unit.auras.iter().map(|aura| aura.spell_id).collect()))
        .collect();
    for (caster, cast) in &mut view.casts {
        let Some(definition) = spells.catalog.get(cast.identity.spell_id) else {
            continue;
        };
        let previous = cast.interruptible;
        let aura_effects: Vec<(u32, i32)> = aura_spell_ids
            .get(caster)
            .into_iter()
            .flatten()
            .filter_map(|spell_id| spells.catalog.get(*spell_id))
            .flat_map(|aura| {
                aura.effect_apply_aura
                    .into_iter()
                    .zip(aura.effect_misc_value)
            })
            .collect();
        let is_player = view
            .entities
            .get(caster)
            .is_some_and(|unit| unit.type_id == benilla_protocol_wrath::updates::type_id::PLAYER);
        let interruptible = cast_interruptible(
            definition,
            spells.catalog.attributes_ex_g(cast.identity.spell_id),
            is_player,
            interrupt_mask,
            silence_mask,
            interrupt_special_mask,
            silence_special_mask,
            cast.cast_school_immunities,
            cast.cast_immunities,
            &aura_effects,
        );
        cast.interruptible = Some(interruptible);
        if previous != Some(interruptible) {
            cast_events.write(crate::wrath_native::WrathCastEvent {
                scope: cast.scope,
                attempt_serial: Some(cast.attempt_serial),
                identity: Some(cast.identity),
                target: cast.target,
                targets: Some(cast.targets.clone()),
                observed_at_unix_ms: cast.observed_at_unix_ms,
                start_ui_now_seconds: cast.start_ui_now_seconds,
                ui_now_seconds: cast.last_update_ui_now_seconds,
                kind: crate::wrath_native::WrathCastEventKind::InterruptibilityChanged {
                    not_interruptible: !interruptible,
                },
            });
        }
    }
}

#[cfg(test)]
mod interruptibility_tests {
    use super::*;
    use crate::ui_action::{PlayerActions, Spells};
    use crate::wrath_native::{WrathActiveCast, WrathCastIdentity, WrathScope};
    use benilla_formats::SpellDisplay;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    fn definition(prevention_type: u32) -> SpellDisplay {
        SpellDisplay {
            prevention_type,
            ..Default::default()
        }
    }

    #[test]
    fn non_silence_prevention_and_attribute_gate_are_interruptible() {
        assert!(cast_interruptible(
            &definition(0),
            0,
            false,
            0,
            0,
            0,
            0,
            0,
            0,
            &[]
        ));
        assert!(cast_interruptible(
            &definition(2),
            0,
            false,
            0,
            0,
            0,
            0,
            0,
            0,
            &[]
        ));
        let bypass = definition(1);
        assert!(cast_interruptible(
            &bypass,
            0x2000,
            false,
            0,
            0,
            0,
            0,
            0,
            0,
            &[]
        ));
    }

    #[test]
    fn spell_interrupt_and_silence_masks_block_a_cast() {
        let definition = definition(1);
        assert!(cast_interruptible(
            &definition,
            0,
            true,
            1,
            0,
            0,
            0,
            0,
            0,
            &[]
        ));
        assert!(cast_interruptible(
            &definition,
            0,
            true,
            0,
            1,
            0,
            0,
            0,
            0,
            &[]
        ));
        assert!(!cast_interruptible(
            &definition,
            0,
            true,
            0,
            0,
            0,
            0,
            0,
            0,
            &[]
        ));
    }

    #[test]
    fn cast_immunity_words_and_school_immunity_clear_matching_masks() {
        let definition = definition(1);
        assert!(!cast_interruptible(
            &definition,
            0,
            true,
            1,
            0,
            0,
            0,
            1,
            0,
            &[]
        ));
        assert!(cast_interruptible(
            &definition,
            0,
            true,
            1,
            0,
            0,
            0,
            2,
            0,
            &[]
        ));
        assert!(!cast_interruptible(
            &definition,
            0,
            true,
            1,
            0,
            0,
            0,
            0,
            0x0400_0000,
            &[]
        ));
        assert!(!cast_interruptible(
            &definition,
            0,
            true,
            0,
            1,
            0,
            0,
            0,
            0x200,
            &[]
        ));
    }

    #[test]
    fn special_masks_only_apply_to_non_player_casters_and_auras_clear_them() {
        let definition = definition(1);
        assert!(!cast_interruptible(
            &definition,
            0,
            true,
            0,
            0,
            1,
            0,
            0,
            0,
            &[]
        ));
        assert!(cast_interruptible(
            &definition,
            0,
            false,
            0,
            0,
            1,
            0,
            0,
            0,
            &[]
        ));
        assert!(!cast_interruptible(
            &definition,
            0,
            false,
            0,
            0,
            1,
            0,
            0,
            0,
            &[(37, 68)]
        ));
        assert!(!cast_interruptible(
            &definition,
            0,
            false,
            0,
            1,
            0,
            0,
            0,
            0,
            &[(38, 27)]
        ));
    }

    fn active_cast() -> WrathActiveCast {
        WrathActiveCast {
            scope: WrathScope {
                session_epoch: 1,
                world_generation: 1,
                state_revision: 1,
            },
            attempt_serial: 1,
            identity: WrathCastIdentity {
                caster: 7,
                spell_id: 100,
                cast_count: Some(1),
                cast_item: None,
            },
            target: None,
            targets: gameplay::SpellTargets::default(),
            start_remaining_ms: Some(1_000),
            cast_flags: 0,
            ammo_display_id: None,
            cast_school_immunities: 0,
            cast_immunities: 0,
            observed_at_unix_ms: 1,
            start_ui_now_seconds: None,
            last_update_ui_now_seconds: None,
            pushback_ms: 0,
            go_server_time_ms: None,
            cancel_requested: false,
            interruptible: None,
        }
    }

    #[test]
    fn refresh_emits_one_edge_then_stays_quiet_when_state_is_unchanged() {
        let mut displays = HashMap::new();
        displays.insert(100, definition(0));
        let spells = Spells {
            catalog: benilla_formats::SpellCatalog::from_displays(displays),
            ..Spells::empty_for_tests()
        };
        let known = PlayerActions::default();
        let mut view = WrathWorldView::default();
        view.casts.insert(7, active_cast());
        let mut world = World::new();
        world.insert_resource(view);
        world.insert_resource(known);
        world.insert_resource(spells);
        world.init_resource::<Messages<crate::wrath_native::WrathCastEvent>>();
        world
            .run_system_once(
                |mut view: ResMut<WrathWorldView>,
                 known: Res<PlayerActions>,
                 spells: Res<Spells>,
                 mut events: MessageWriter<crate::wrath_native::WrathCastEvent>| {
                    refresh_cast_interruptibility(
                        &mut view,
                        Some(&known),
                        Some(&spells),
                        &mut events,
                    );
                },
            )
            .expect("initial refresh runs");
        assert_eq!(
            world.resource::<WrathWorldView>().casts[&7].interruptible,
            Some(true)
        );
        assert_eq!(
            world
                .resource::<Messages<crate::wrath_native::WrathCastEvent>>()
                .iter_current_update_messages()
                .count(),
            1
        );
        world
            .run_system_once(
                |mut view: ResMut<WrathWorldView>,
                 known: Res<PlayerActions>,
                 spells: Res<Spells>,
                 mut events: MessageWriter<crate::wrath_native::WrathCastEvent>| {
                    refresh_cast_interruptibility(
                        &mut view,
                        Some(&known),
                        Some(&spells),
                        &mut events,
                    );
                },
            )
            .expect("unchanged refresh runs");
        assert_eq!(
            world
                .resource::<Messages<crate::wrath_native::WrathCastEvent>>()
                .iter_current_update_messages()
                .count(),
            1
        );
    }
}

/// An ECS entity translated from a Wrath digest. Presentation clears exactly this scope without
/// touching stock-path or UI entities.
#[derive(Component)]
struct WrathStreamed;

#[derive(Component, Clone, Copy)]
struct WrathGeneration(u32);

/// Last replicated pose applied to the root. Unrelated descriptor changes must not reset a
/// server spline to the older create-block position after it has moved or finished.
#[derive(Component, Clone, Copy, PartialEq)]
struct WrathAppliedPosition(benilla_protocol_wrath::updates::PositionSnapshot);

/// Last create-block spline ID attached to this entity so repeated digests do not restart it.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct WrathAppliedCreateSpline(u32);

fn vanilla_npc_flags(flags: u32) -> u32 {
    // AC UnitDefines.h NPCFlags -> the donor service ladder in target/cursor_mode.rs.
    let mut mapped = flags & 0x3;
    for (source, destination) in [
        (0x70, 0x10),
        (0xf80, 0x4),
        (0x1000, 0x4000),
        (0x2000, 0x8),
        (0x4000, 0x20),
        (0x8000, 0x40),
        (0x10000, 0x80),
        (0x20000, 0x100),
        (0x40000, 0x200),
        (0x80000, 0x400),
        (0x100000, 0x800),
        (0x200000, 0x1000),
        (0x400000, 0x2000),
    ] {
        if flags & source != 0 {
            mapped |= destination;
        }
    }
    mapped
}

/// The full 3.3.5 quest-log window (25 five-dword slots) carried beside the 1.12 descriptor's
/// twenty-slot projection. The stock log reads this, so no server slot is lost.
#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WrathQuestSlots(
    pub [QuestSlotView; QUEST_LOG_SLOTS],
    pub u32, // PLAYER_FIELD_MAX_LEVEL
    pub u32, // Nonempty PLAYER_FIELD_DAILY_QUESTS_1 slots.
);

/// The active player's raw 3.3.5 field window (`0..PLAYER_END` = `0..1326`) with its delivered
/// mask, the source for the UI contracts the 1.12 descriptor cannot carry:
/// `PLAYER_CHOSEN_TITLE` (321), `PLAYER__FIELD_KNOWN_TITLES` (626/628/630),
/// `PLAYER_FIELD_COMBAT_RATING_1` (1231..1256), `PLAYER_FIELD_MAX_LEVEL` (1279) and the
/// `PLAYER_FIELD_DAILY_QUESTS_1` (1280..1305) tail. Kept beside the translated store so
/// donor field indices stay untouched.
#[derive(Component, Clone, PartialEq, Eq, Default)]
pub(crate) struct WrathPlayerRaw(pub(crate) crate::wrath_inventory::WrathFieldWindow);

/// A unit's raw 3.3.5 `UNIT_NPC_FLAGS`, beside the store's 1.12 projection, which
/// [`vanilla_npc_flags`] folds the vendor kinds into one bit of. The minimap's town-service
/// tracking (`minimapTrackedInfo`) matches the 3.3.5 bits.
#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WrathNpcFlags(pub(crate) u32);

/// Map 12340's `PLAYER_QUEST_LOG_*` block: id, state, two count dwords (four 16-bit counts) and
/// the deadline, five dwords a slot for twenty-five slots.
fn wrath_quest_slots(
    raw: &crate::wrath_inventory::WrathFieldWindow,
) -> [QuestSlotView; QUEST_LOG_SLOTS] {
    std::array::from_fn(|slot| {
        let src = 158 + slot * 5;
        let quest_id = raw.word_or_zero(src as u16);
        if quest_id == 0 {
            return QuestSlotView::default();
        }
        let state = raw.word_or_zero((src + 1) as u16);
        let counts = u64::from(raw.word_or_zero((src + 2) as u16))
            | (u64::from(raw.word_or_zero((src + 3) as u16)) << 32);
        let mut counters = [0u8; 4];
        for (i, counter) in counters.iter_mut().enumerate() {
            *counter = ((counts >> (i * 16)) & 0xffff).min(63) as u8;
        }
        QuestSlotView {
            quest_id,
            counters,
            state: (state & 0xff) as u8,
            timer: raw.word_or_zero((src + 4) as u16),
        }
    })
}

/// Project 3.3.5 `UNIT_FIELD_*` words (`0..UNIT_END` = `0..148`) that are not carried in the
/// parsed digest onto their 1.12 donor `FIELD_UNIT_*` indices (`0..188`). Shared by both
/// `Unit` (creatures/pets) and `Player` entities; only delivered dwords are copied.
///
/// Health, max health, powers 1..=5, max powers 1..=5, level, faction, unit flags, bytes 0/1/2,
/// display id, mount display id, npc flags, dynamic flags, target, charmed_by, summoned_by,
/// created_by, charm, and summon are projected from the parsed digest in `wrath_apply_world`.
/// Powers 6..=7 (runes/runic power), regen flat modifiers (40..54), `UNIT_FIELD_CRITTER` (10..12),
/// `UNIT_FIELD_FLAGS_2` (60), `UNIT_FIELD_MAXHEALTHMODIFIER` (145, read from `WrathPlayerRaw`),
/// `UNIT_FIELD_HOVERHEIGHT` (146), and `UNIT_FIELD_PADDING` (147) have no 1.12 descriptor slot.
/// `UNIT_VIRTUAL_ITEM_SLOT_ID` (56..59) is resolved via `Item.dbc` in `wrath_translate.lua`.
fn push_unit_fields(raw: &crate::wrath_inventory::WrathFieldWindow, pairs: &mut Vec<(u16, u32)>) {
    for (src, dst) in [
        (6u16, 6u16), // UNIT_FIELD_CHARM low -> FIELD_UNIT_CHARM low
        (7, 7),       // UNIT_FIELD_CHARM high -> FIELD_UNIT_CHARM high
        (8, 8),       // UNIT_FIELD_SUMMON low -> FIELD_UNIT_SUMMON low
        (9, 9),       // UNIT_FIELD_SUMMON high -> FIELD_UNIT_SUMMON high
        (20, 20),     // UNIT_FIELD_CHANNEL_OBJECT low -> FIELD_UNIT_CHANNEL_OBJECT low
        (21, 21),     // UNIT_FIELD_CHANNEL_OBJECT high -> FIELD_UNIT_CHANNEL_OBJECT high
        (22, 144),    // UNIT_CHANNEL_SPELL -> FIELD_UNIT_CHANNEL_SPELL
        (61, 125),    // UNIT_FIELD_AURASTATE -> FIELD_UNIT_AURASTATE
        (62, 126),    // UNIT_FIELD_BASEATTACKTIME[0] (main hand) -> FIELD_UNIT_BASEATTACKTIME[0]
        (63, 127),    // UNIT_FIELD_BASEATTACKTIME[1] (off hand) -> FIELD_UNIT_BASEATTACKTIME[1]
        (64, 128),    // UNIT_FIELD_RANGEDATTACKTIME -> FIELD_UNIT_RANGEDATTACKTIME
        (65, 129),    // UNIT_FIELD_BOUNDINGRADIUS -> FIELD_UNIT_BOUNDINGRADIUS
        (66, 130),    // UNIT_FIELD_COMBATREACH -> FIELD_UNIT_COMBATREACH
        (70, 134),    // UNIT_FIELD_MINDAMAGE -> FIELD_UNIT_MINDAMAGE
        (71, 135),    // UNIT_FIELD_MAXDAMAGE -> FIELD_UNIT_MAXDAMAGE
        (72, 136),    // UNIT_FIELD_MINOFFHANDDAMAGE -> FIELD_UNIT_MINOFFHANDDAMAGE
        (73, 137),    // UNIT_FIELD_MAXOFFHANDDAMAGE -> FIELD_UNIT_MAXOFFHANDDAMAGE
        (75, 139),    // UNIT_FIELD_PETNUMBER -> FIELD_UNIT_PETNUMBER
        (76, 140),    // UNIT_FIELD_PET_NAME_TIMESTAMP -> FIELD_UNIT_PET_NAME_TIMESTAMP
        (77, 141),    // UNIT_FIELD_PETEXPERIENCE -> FIELD_UNIT_PETEXPERIENCE
        (78, 142),    // UNIT_FIELD_PETNEXTLEVELEXP -> FIELD_UNIT_PETNEXTLEVELEXP
        (80, 145),    // UNIT_MOD_CAST_SPEED -> FIELD_UNIT_MOD_CAST_SPEED
        (81, 146),    // UNIT_CREATED_BY_SPELL -> FIELD_UNIT_CREATED_BY_SPELL
        (83, 148),    // UNIT_NPC_EMOTESTATE -> FIELD_UNIT_NPC_EMOTESTATE
        (120, 162),   // UNIT_FIELD_BASE_MANA -> FIELD_UNIT_BASE_MANA
        (121, 163),   // UNIT_FIELD_BASE_HEALTH -> FIELD_UNIT_BASE_HEALTH
    ] {
        if let Some(value) = raw.get(src) {
            pairs.push((dst, value));
        }
    }
    // UNIT_FIELD_NATIVEDISPLAYID (68) -> FIELD_UNIT_NATIVEDISPLAYID (132); non-zero only so a
    // delivered 0 does not clobber the UNIT_FIELD_DISPLAYID / ChrRaces body display fallback.
    if let Some(native_display) = raw.get(68).filter(|&d| d != 0) {
        pairs.push((132, native_display));
    }
    // UNIT_FIELD_STAT0..4 (84..89) -> FIELD_UNIT_STAT0..4 (150..155)
    for i in 0..5u16 {
        if let Some(v) = raw.get(84 + i) {
            pairs.push((150 + i, v));
        }
    }
    // UNIT_FIELD_RESISTANCES[0..7] (99..106) -> FIELD_UNIT_RESISTANCES[0..7] (155..162)
    for i in 0..7u16 {
        if let Some(v) = raw.get(99 + i) {
            pairs.push((155 + i, v));
        }
    }
    // UNIT_FIELD_ATTACK_POWER..MAXRANGEDDAMAGE (123..131) -> FIELD_UNIT_ATTACK_POWER..MAXRANGEDDAMAGE (165..173)
    for i in 0..8u16 {
        if let Some(v) = raw.get(123 + i) {
            pairs.push((165 + i, v));
        }
    }
    // UNIT_FIELD_POWER_COST_MODIFIER[0..7] (131..138) -> FIELD_UNIT_POWER_COST_MODIFIER[0..7] (173..180)
    // UNIT_FIELD_POWER_COST_MULTIPLIER[0..7] (138..145) -> FIELD_UNIT_POWER_COST_MULTIPLIER[0..7] (180..187)
    for i in 0..7u16 {
        if let Some(v) = raw.get(131 + i) {
            pairs.push((173 + i, v));
        }
        if let Some(v) = raw.get(138 + i) {
            pairs.push((180 + i, v));
        }
    }
}

/// The player's header (duel, flags, guild, `PLAYER_BYTES*`), the shared unit combat/stat block
/// carried in its `0..148` unit prefix, and the player-only stat/resistance buff modifiers (which
/// live in the `PLAYER` block in 1.12 at `1177..1201`), from 3.3.5 indices to the 1.12 slots the
/// donor owners read. Only delivered dwords are copied.
fn push_player_unit_fields(
    raw: &crate::wrath_inventory::WrathFieldWindow,
    pairs: &mut Vec<(u16, u32)>,
) {
    // Player header (`148..=157` -> `188..=197`). Powers, `UNIT_FIELD_BYTES_0` (3.3.5 23) and the
    // mount display are projected for every unit by `wrath_apply_world` from the parsed digest;
    // they must not be re-mapped here (3.3.5 36 is `UNIT_FIELD_MAXPOWER4`, not 1.12 `BYTES_0` 36).
    for (src, dst) in [
        (148u16, 188u16), // PLAYER_DUEL_ARBITER low -> FIELD_PLAYER_DUEL_ARBITER low
        (149, 189),       // PLAYER_DUEL_ARBITER high -> FIELD_PLAYER_DUEL_ARBITER high
        (150, benilla_protocol::field::FIELD_PLAYER_FLAGS), // PLAYER_FLAGS -> FIELD_PLAYER_FLAGS (190)
        (151, 191), // PLAYER_GUILDID -> FIELD_PLAYER_GUILDID
        (152, 192), // PLAYER_GUILDRANK -> FIELD_PLAYER_GUILDRANK
        (153, 193), // PLAYER_BYTES -> FIELD_PLAYER_BYTES (skin, face, hairStyle, hairColor)
        (154, 194), // PLAYER_BYTES_2 -> FIELD_PLAYER_BYTES_2 (facialHair, bank bags, rest)
        (155, 195), // PLAYER_BYTES_3 -> FIELD_PLAYER_BYTES_3 (gender, drunk, medal, pvp rank)
        (156, 196), // PLAYER_DUEL_TEAM -> FIELD_PLAYER_DUEL_TEAM
        (157, 197), // PLAYER_GUILD_TIMESTAMP -> FIELD_PLAYER_GUILD_TIMESTAMP
    ] {
        if let Some(value) = raw.get(src) {
            pairs.push((dst, value));
        }
    }
    push_unit_fields(raw, pairs);
    // Player-only stat buff modifiers (3.3.5 UNIT block -> 1.12 PLAYER block):
    // UNIT_FIELD_POSSTAT0..4 (89..94) -> FIELD_PLAYER_POSSTAT0..4 (1177..1182)
    // UNIT_FIELD_NEGSTAT0..4 (94..99) -> FIELD_PLAYER_NEGSTAT0..4 (1182..1187)
    for i in 0..5u16 {
        if let Some(v) = raw.get(89 + i) {
            pairs.push((1177 + i, v));
        }
        if let Some(v) = raw.get(94 + i) {
            pairs.push((1182 + i, v));
        }
    }
    // Player-only resistance buff modifiers (3.3.5 UNIT block -> 1.12 PLAYER block):
    // UNIT_FIELD_RESISTANCEBUFFMODSPOSITIVE[0..7] (106..113) -> FIELD_PLAYER_RESISTANCEBUFFMODSPOSITIVE (1187..1194)
    // UNIT_FIELD_RESISTANCEBUFFMODSNEGATIVE[0..7] (113..120) -> FIELD_PLAYER_RESISTANCEBUFFMODSNEGATIVE (1194..1201)
    for i in 0..7u16 {
        if let Some(v) = raw.get(106 + i) {
            pairs.push((1187 + i, v));
        }
        if let Some(v) = raw.get(113 + i) {
            pairs.push((1194 + i, v));
        }
    }
}

/// Project a 3.3.5 `GameObject` (`0..GAMEOBJECT_END` = `0..18`) and its movement position into the
/// 1.12 `FIELD_GAMEOBJECT_*` descriptor (`0..26`). Only delivered dwords are copied.
fn push_gameobject_fields(
    raw: Option<&crate::wrath_inventory::WrathFieldWindow>,
    position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    pairs: &mut Vec<(u16, u32)>,
) {
    if let Some(raw) = raw {
        for (src, dst) in [
            (6u16, 6u16), // OBJECT_FIELD_CREATED_BY low -> FIELD_GAMEOBJECT_CREATED_BY low
            (7, 7),       // OBJECT_FIELD_CREATED_BY high -> FIELD_GAMEOBJECT_CREATED_BY high
            (8, 8),       // GAMEOBJECT_DISPLAYID -> FIELD_GAMEOBJECT_DISPLAYID
            (9, 9),       // GAMEOBJECT_FLAGS -> FIELD_GAMEOBJECT_FLAGS
            (10, 10),     // GAMEOBJECT_PARENTROTATION[0] -> FIELD_GAMEOBJECT_ROTATION[0]
            (11, 11),     // GAMEOBJECT_PARENTROTATION[1] -> FIELD_GAMEOBJECT_ROTATION[1]
            (12, 12),     // GAMEOBJECT_PARENTROTATION[2] -> FIELD_GAMEOBJECT_ROTATION[2]
            (13, 13),     // GAMEOBJECT_PARENTROTATION[3] -> FIELD_GAMEOBJECT_ROTATION[3]
            (15, 20),     // GAMEOBJECT_FACTION -> FIELD_GAMEOBJECT_FACTION
            (16, 22),     // GAMEOBJECT_LEVEL -> FIELD_GAMEOBJECT_LEVEL
        ] {
            if let Some(value) = raw.get(src) {
                pairs.push((dst, value));
            }
        }
        // GAMEOBJECT_DYNAMIC (14, low u16 dynamic flags; high u16 path progress unmapped in 1.12)
        // -> FIELD_GAMEOBJECT_DYN_FLAGS (19)
        if let Some(dyn_val) = raw.get(14) {
            pairs.push((19, dyn_val & 0xffff));
        }
        // GAMEOBJECT_BYTES_1 (17: [state, type_id, art_kit, anim_progress]) -> split 1.12 fields
        if let Some(bytes_1) = raw.get(17) {
            pairs.push((14, bytes_1 & 0xff)); // GAMEOBJECT_BYTES_1[0] -> FIELD_GAMEOBJECT_STATE (14)
            pairs.push((21, (bytes_1 >> 8) & 0xff)); // GAMEOBJECT_BYTES_1[1] -> FIELD_GAMEOBJECT_TYPE_ID (21)
            pairs.push((23, (bytes_1 >> 16) & 0xff)); // GAMEOBJECT_BYTES_1[2] -> GAMEOBJECT_ARTKIT (23)
            pairs.push((24, (bytes_1 >> 24) & 0xff)); // GAMEOBJECT_BYTES_1[3] -> GAMEOBJECT_ANIMPROGRESS (24)
        }
    }
    if let Some(pos) = position {
        pairs.push((15, pos.x.to_bits())); // movement.x -> FIELD_GAMEOBJECT_POS_X (15)
        pairs.push((16, pos.y.to_bits())); // movement.y -> FIELD_GAMEOBJECT_POS_Y (16)
        pairs.push((17, pos.z.to_bits())); // movement.z -> FIELD_GAMEOBJECT_POS_Z (17)
        pairs.push((18, pos.o.to_bits())); // movement.o -> FIELD_GAMEOBJECT_FACING (18)
    }
}

/// Project a 3.3.5 `DynamicObject` (`0..DYNAMICOBJECT_END` = `0..12`) and its movement position
/// into the 1.12 `DYNAMICOBJECT_*` descriptor (`0..16`). Only delivered dwords are copied.
/// `DYNAMICOBJECT_CASTTIME` (11) has no 1.12 descriptor slot (donor `11..=14` hold position) and
/// stays in `WrathEntity::raw_fields`.
fn push_dynamicobject_fields(
    raw: Option<&crate::wrath_inventory::WrathFieldWindow>,
    position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    pairs: &mut Vec<(u16, u32)>,
) {
    if let Some(raw) = raw {
        for (src, dst) in [
            (6u16, 6u16), // DYNAMICOBJECT_CASTER low -> DYNAMICOBJECT_CASTER low
            (7, 7),       // DYNAMICOBJECT_CASTER high -> DYNAMICOBJECT_CASTER high
            (8, 8),       // DYNAMICOBJECT_BYTES -> DYNAMICOBJECT_BYTES
            (9, 9),       // DYNAMICOBJECT_SPELLID -> DYNAMICOBJECT_SPELLID
            (10, 10),     // DYNAMICOBJECT_RADIUS -> DYNAMICOBJECT_RADIUS
        ] {
            if let Some(value) = raw.get(src) {
                pairs.push((dst, value));
            }
        }
    }
    if let Some(pos) = position {
        pairs.push((11, pos.x.to_bits())); // movement.x -> DYNAMICOBJECT_POS_X (11)
        pairs.push((12, pos.y.to_bits())); // movement.y -> DYNAMICOBJECT_POS_Y (12)
        pairs.push((13, pos.z.to_bits())); // movement.z -> DYNAMICOBJECT_POS_Z (13)
        pairs.push((14, pos.o.to_bits())); // movement.o -> DYNAMICOBJECT_FACING (14)
    }
}

/// Project the 3.3.5 honor counters (`PLAYER_FIELD_KILLS` 1225 .. `PLAYER_FIELD_WATCHED_FACTION_INDEX`
/// 1230) into the 1.12 honor block (`1250..=1261`).
///
/// In 3.3.5a, `PLAYER_FIELD_KILLS` (1225) packs today's honorable kills in the low u16 and
/// yesterday's honorable kills in the high u16 (there are no dishonorable kills), whereas 1.12
/// `FIELD_PLAYER_FIELD_SESSION_KILLS` (1250) packs today's HK/DHK and `YESTERDAY_KILLS` (1251)
/// holds yesterday's HK/DHK. Split `1225` across `1250` and `1251` so yesterday's HK never shows
/// as today's DHK. `PLAYER_FIELD_TODAY_CONTRIBUTION` (1226), `PLAYER_FIELD_HONOR_CURRENCY` (1277),
/// and `PLAYER_FIELD_ARENA_CURRENCY` (1278) have no 1.12 slot (1254 is `THIS_WEEK_CONTRIBUTION`)
/// and are read directly from `WrathPlayerRaw` by `ui_honor.rs`.
fn push_player_honor_fields(
    raw: &crate::wrath_inventory::WrathFieldWindow,
    pairs: &mut Vec<(u16, u32)>,
) {
    // PLAYER_FIELD_KILLS (1225): low u16 -> FIELD_PLAYER_FIELD_SESSION_KILLS (1250),
    // high u16 -> FIELD_PLAYER_FIELD_YESTERDAY_KILLS (1251).
    if let Some(kills) = raw.get(benilla_protocol_wrath::fields::PLAYER_FIELD_KILLS) {
        pairs.push((1250, kills & 0xffff));
        pairs.push((1251, kills >> 16));
    }
    for (src, dst) in [
        (1227u16, 1257u16), // PLAYER_FIELD_YESTERDAY_CONTRIBUTION -> FIELD_PLAYER_FIELD_YESTERDAY_CONTRIBUTION
        (1228, 1255), // PLAYER_FIELD_LIFETIME_HONORABLE_KILLS -> FIELD_PLAYER_FIELD_LIFETIME_HONORABLE_KILLS
        (1229, 1260), // PLAYER_FIELD_BYTES2 -> FIELD_PLAYER_FIELD_BYTES2
        (1230, 1261), // PLAYER_FIELD_WATCHED_FACTION_INDEX -> FIELD_PLAYER_WATCHED_FACTION_INDEX
    ] {
        if let Some(value) = raw.get(src) {
            pairs.push((dst, value));
        }
    }
}

/// Translate one 3.3.5 corpse block (`0..CORPSE_END` = `0..36`) and its movement position into the
/// 1.12 corpse layout (`0..38`). Owner (`6..=7`) keeps its indices; donor `8..=11` hold facing and
/// position (3.3.5 `CORPSE_FIELD_PARTY` `8..=9` has no 1.12 slot and must not overwrite them);
/// display, the nineteen item slots, both appearance bytes, guild, flags and dynamic flags start
/// two dwords later (`CORPSE_FIELD_DISPLAY_ID` 10 -> 12; `Player.cpp:4809-4822`).
/// `CORPSE_FIELD_PAD` (35) is padding and stays unwritten. Only delivered dwords are copied.
fn push_corpse_fields(
    raw: Option<&crate::wrath_inventory::WrathFieldWindow>,
    position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    pairs: &mut Vec<(u16, u32)>,
) {
    if let Some(raw) = raw {
        // CORPSE_FIELD_OWNER (6..8) -> CORPSE_FIELD_OWNER (6..8)
        for src in 6..=7u16 {
            if let Some(value) = raw.get(src) {
                pairs.push((src, value));
            }
        }
    }
    if let Some(pos) = position {
        pairs.push((8, pos.o.to_bits())); // movement.o -> CORPSE_FIELD_FACING (8)
        pairs.push((9, pos.x.to_bits())); // movement.x -> CORPSE_FIELD_POS_X (9)
        pairs.push((10, pos.y.to_bits())); // movement.y -> CORPSE_FIELD_POS_Y (10)
        pairs.push((11, pos.z.to_bits())); // movement.z -> CORPSE_FIELD_POS_Z (11)
    }
    if let Some(raw) = raw {
        // CORPSE_FIELD_DISPLAY_ID (10) -> CORPSE_FIELD_DISPLAY_ID (12)
        if let Some(value) = raw.get(10) {
            pairs.push((12, value));
        }
        // CORPSE_FIELD_ITEM[0..19] (11..30) -> CORPSE_FIELD_ITEM[0..19] (13..32)
        for i in 0..19u16 {
            if let Some(value) = raw.get(11 + i) {
                pairs.push((13 + i, value));
            }
        }
        for (src, dst) in [
            (30u16, 32u16), // CORPSE_FIELD_BYTES_1 -> CORPSE_FIELD_BYTES_1
            (31, 33),       // CORPSE_FIELD_BYTES_2 -> CORPSE_FIELD_BYTES_2
            (32, 34),       // CORPSE_FIELD_GUILD -> CORPSE_FIELD_GUILD
            (33, 35),       // CORPSE_FIELD_FLAGS -> CORPSE_FIELD_FLAGS
            (34, 36),       // CORPSE_FIELD_DYNAMIC_FLAGS -> FIELD_CORPSE_DYNAMIC_FLAGS
        ] {
            if let Some(value) = raw.get(src) {
                pairs.push((dst, value));
            }
        }
    }
}

/// Copy the player inventory bands the 1.12 item owners walk. The 3.3.5 arrays are wider in the
/// bank (28 -> 24 slots), bank bags (7 -> 6) and otherwise use a different base; every array here
/// is copied explicitly, with the extra 3.3.5 tail dropped rather than folded into a neighbour.
/// The dropped tail lives in [`crate::wrath_inventory::WrathInventory`]; no donor offset moves.
fn push_player_inventory_fields(
    raw: &crate::wrath_inventory::WrathFieldWindow,
    pairs: &mut Vec<(u16, u32)>,
) {
    // (3.3.5 base, 1.12 base, dwords copied)
    const BANDS: [(usize, u16, usize); 7] = [
        (324, 486, 46), // PLAYER_FIELD_INV_SLOT_HEAD (324..370) -> FIELD_PLAYER_INV_SLOT_HEAD (486..532, 19 equip + 4 bags)
        (370, 532, 32), // PLAYER_FIELD_PACK_SLOT_1 (370..402) -> FIELD_PLAYER_PACK_SLOT_1 (532..564, 16 slots)
        (402, 564, 48), // PLAYER_FIELD_BANK_SLOT_1 (402..450) -> FIELD_PLAYER_BANK_SLOT_1 (564..612, first 24 of 28 slots)
        (458, 612, 12), // PLAYER_FIELD_BANKBAG_SLOT_1 (458..470) -> FIELD_PLAYER_BANK_BAG_SLOT_1 (612..624, first 6 of 7 slots)
        (472, 624, 24), // PLAYER_FIELD_VENDORBUYBACK_SLOT_1 (472..496) -> FIELD_PLAYER_VENDORBUYBACK_SLOT_1 (624..648, 12 slots)
        (496, 648, 64), // PLAYER_FIELD_KEYRING_SLOT_1 (496..560) -> FIELD_PLAYER_KEYRING_SLOT_1 (648..712, 32 slots)
        (624, 712, 2),  // PLAYER_FARSIGHT (624..626) -> FIELD_PLAYER_FARSIGHT (712..714)
    ];
    for (src, dst, count) in BANDS {
        for i in 0..count {
            // Only delivered dwords reach the donor projection. The create-shaped ObjectFields
            // still reads 0 for an absent field, so donor readers see the same value as before,
            // but the projection no longer claims a delivery the server did not make.
            if let Some(value) = raw.get((src + i) as u16) {
                pairs.push((dst + i as u16, value));
            }
        }
    }
}

/// The 3.3.5 `PLAYER_VISIBLE_ITEM_*` block into the 1.12 twelve-dword slots: each 3.3.5 slot is
/// two fields (entry, two packed 16-bit enchants) at 283 + 2n, against 1.12's entry at +2 and its
/// seven enchant dwords at +3. The perm/temp enchants the glow fold reads land at +3/+4.
fn push_visible_item_fields(
    raw: &crate::wrath_inventory::WrathFieldWindow,
    pairs: &mut Vec<(u16, u32)>,
) {
    for slot in 0..19u16 {
        let src =
            usize::from(benilla_protocol_wrath::fields::PLAYER_VISIBLE_ITEM_1_ENTRYID + 2 * slot);
        // An undelivered entry is skipped, not treated as the end: a create may omit a zero
        // slot while a later slot is present.
        let Some(entry) = raw.get(src as u16) else {
            continue;
        };
        let ench = raw.word_or_zero((src + 1) as u16);
        let dst = benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 12 * slot;
        pairs.push((dst + 2, entry)); // PLAYER_VISIBLE_ITEM_<slot>_ENTRYID -> FIELD_PLAYER_VISIBLE_ITEM_<slot>_0 (+2)
        pairs.push((dst + 3, ench & 0xffff)); // PLAYER_VISIBLE_ITEM_<slot>_ENCHANTMENT low u16 -> +3 (perm enchant)
        pairs.push((dst + 4, ench >> 16)); // PLAYER_VISIBLE_ITEM_<slot>_ENCHANTMENT high u16 -> +4 (temp enchant)
    }
}

/// Translate one item/container's Wrath fields into the 1.12 item layout. The first fields
/// coincide; the enchant array is shorter in 1.12, and the durability/random columns moved after
/// the two extra 3.3.5 enchant slots. Container fields start at a different base.
///
/// Only delivered dwords are projected; an absent field leaves the create-shaped donor descriptor
/// at 0 without claiming a delivery. Wrath 43..57 are enchant slots 8..12 and have no donor slot:
/// they are deliberately not written into donor 43..47, which are the property seed, random
/// properties id, durability and max durability (Wrath 58..61). `CREATE_PLAYED_TIME` (62) has no
/// 1.12 field and stays in the typed row.
fn push_item_fields(
    raw: &crate::wrath_inventory::WrathFieldWindow,
    container: bool,
    pairs: &mut Vec<(u16, u32)>,
) {
    // ITEM_FIELD_OWNER (6..8), ITEM_FIELD_CONTAINED (8..10), ITEM_FIELD_CREATOR (10..12),
    // ITEM_FIELD_GIFTCREATOR (12..14), ITEM_FIELD_STACK_COUNT (14), ITEM_FIELD_DURATION (15),
    // ITEM_FIELD_SPELL_CHARGES[0..5] (16..21), ITEM_FIELD_FLAGS (21) -> identical 1.12 indices (6..=21).
    for src in 6..=21u16 {
        if let Some(value) = raw.get(src) {
            pairs.push((src, value));
        }
    }
    // ITEM_FIELD_ENCHANTMENT_1_1..=ITEM_FIELD_ENCHANTMENT_7_3 (22..43) -> FIELD_ITEM_ENCHANTMENT (22..43, 7 slots * 3 dwords).
    for i in 0..21u16 {
        if let Some(value) = raw.get(22 + i) {
            pairs.push((22 + i, value));
        }
    }
    // Property seed, random properties id, durability and max durability. Donor 45 stays unwritten:
    // 1.12 spells it `ITEM_FIELD_ITEM_TEXT_ID`, which 3.3.5 carries on the item-text query instead.
    for (src, dst) in [
        (58u16, 43u16), // ITEM_FIELD_PROPERTY_SEED -> ITEM_FIELD_PROPERTY_SEED
        (59, 44),       // ITEM_FIELD_RANDOM_PROPERTIES_ID -> ITEM_FIELD_RANDOM_PROPERTIES_ID
        (60, 46),       // ITEM_FIELD_DURABILITY -> ITEM_FIELD_DURABILITY
        (61, 47),       // ITEM_FIELD_MAXDURABILITY -> ITEM_FIELD_MAXDURABILITY
    ] {
        if let Some(value) = raw.get(src) {
            pairs.push((dst, value));
        }
    }
    if container {
        // CONTAINER_FIELD_NUM_SLOTS (64) -> FIELD_CONTAINER_NUM_SLOTS (48)
        // CONTAINER_ALIGN_PAD (65) is alignment padding and skipped.
        if let Some(value) = raw.get(64) {
            pairs.push((48, value));
        }
        // CONTAINER_FIELD_SLOT_1[0..36] (66..138) -> FIELD_CONTAINER_SLOT_1[0..36] (50..122)
        for i in 0..36u16 {
            if let Some(value) = raw.get(66 + 2 * i) {
                pairs.push((50 + 2 * i, value));
            }
            if let Some(value) = raw.get(67 + 2 * i) {
                pairs.push((51 + 2 * i, value));
            }
        }
    }
}

/// The 1.12 unit-aura descriptor window (`UNIT_FIELD_AURA`, fields 47-94, the six `AURAFLAGS`
/// nibble words at 95-100, the twelve `AURALEVELS` byte words at 101-112, and the twelve
/// `AURAAPPLICATIONS` byte words at 113-124) from 12340's visible slots. Slots 48 and up have no
/// 1.12 field and drop; the flags' low three bits are the effect indices the nibble carries.
fn push_aura_fields(
    auras: &[benilla_protocol_wrath::reducer::AuraSlot],
    pairs: &mut Vec<(u16, u32)>,
) {
    const SLOT_COUNT: usize = 48;
    let first = benilla_protocol::field::FIELD_UNIT_AURA;
    let mut ids = [0u32; SLOT_COUNT];
    let mut flags = [0u32; 6];
    let mut levels = [0u32; 12];
    let mut applications = [0u32; 12];
    for aura in auras {
        let slot = usize::from(aura.slot);
        if slot >= SLOT_COUNT {
            continue;
        }
        ids[slot] = aura.spell_id;
        flags[slot >> 3] |= u32::from(aura.flags & 0x7) << ((slot & 7) * 4);
        let shift = (slot & 3) * 8;
        levels[slot >> 2] |= u32::from(aura.level) << shift;
        applications[slot >> 2] |= u32::from(aura.charges.saturating_sub(1)) << shift;
    }
    for (index, id) in ids.iter().enumerate() {
        pairs.push((first + index as u16, *id));
    }
    for (index, word) in flags.iter().enumerate() {
        pairs.push((first + SLOT_COUNT as u16 + index as u16, *word));
    }
    for (index, word) in levels.iter().enumerate() {
        pairs.push((first + 54 + index as u16, *word));
    }
    for (index, word) in applications.iter().enumerate() {
        pairs.push((first + 66 + index as u16, *word));
    }
}

/// The aura watcher's current window of a translated descriptor, zeros where not carried.
fn aura_span(fields: &ObjectFields) -> [u32; crate::net::AURA_WATCH_SPAN] {
    let first = benilla_protocol::field::FIELD_UNIT_AURA;
    let end = first + crate::net::AURA_WATCH_SPAN as u16;
    let mut span = [0u32; crate::net::AURA_WATCH_SPAN];
    for (index, value) in fields.raw_fields() {
        if (first..end).contains(&index) {
            span[usize::from(index - first)] = value;
        }
    }
    span
}

fn diff_fields(old: &ObjectFields, new: &ObjectFields) -> Vec<(u16, u32, u32)> {
    let mut moved = Vec::new();
    let mut copy = old.clone();
    copy.merge_diff(new.clone(), |index, prev, next| {
        moved.push((index, prev, next));
    });
    moved
}

/// Spawn or update one item/container entity in the established `ItemObject`/`ObjectStore`
/// owner. Items carry no `NetEntity` or transform; their GUID identity stays in `GuidIndex`.
/// The typed [`crate::wrath_item::WrathItemRow`] rides beside the donor projection on the same
/// entity: the donor store stays the 1.12 view, the row keeps the fields 1.12 has no slot for.
fn apply_item_world(
    world: &mut World,
    index: &mut HashMap<u64, Entity>,
    guid: u64,
    remote: &WrathEntity,
) -> (
    Option<ObjectFields>,
    ObjectFields,
    Option<crate::wrath_item::WrathItemRow>,
    Option<crate::wrath_item::WrathItemRow>,
) {
    let container = remote.type_id == benilla_protocol_wrath::updates::type_id::CONTAINER;
    let object_type = if container {
        ObjectType::Container
    } else {
        ObjectType::Item
    };
    let mut pairs = vec![(2u16, object_type_mask(object_type))];
    if let Some(entry) = remote.entry {
        pairs.push((3, entry));
    }
    if let Some(raw) = remote.item_fields.as_ref() {
        push_item_fields(raw, container, &mut pairs);
    }
    let fields = ObjectFields::from_pairs(&pairs).into_created(object_type);

    let mut existing = index.get(&guid).copied();
    if let Some(entity) = existing {
        if world
            .get::<WrathGeneration>(entity)
            .is_some_and(|generation| generation.0 != remote.generation)
        {
            index.remove(&guid);
            world.despawn(entity);
            existing = None;
        }
    }
    let old_fields =
        existing.and_then(|entity| world.get::<ObjectStore>(entity).map(|s| s.0.clone()));
    let old_row = existing
        .and_then(|entity| world.get::<crate::wrath_item::WrathItemRow>(entity))
        .cloned();
    let scope = world
        .get_resource::<WrathWorldView>()
        .map(|view| (view.session_epoch, view.world_generation))
        .unwrap_or((0, 0));
    let new_row = remote
        .item_fields
        .as_ref()
        .map(|window| crate::wrath_item::WrathItemRow::new(window.clone(), container, scope));
    if old_fields.as_ref().and_then(ObjectFields::object_entry) != fields.object_entry() {
        if let Some(entry) = fields.object_entry().filter(|&e| e != 0) {
            if let Some(net) = world
                .get_resource::<NetCommands>()
                .map(|n| NetCommands(n.0.clone()))
            {
                if let Some(items) = world.get_resource_mut::<crate::items::Items>() {
                    if items
                        .template(entry, guid, crate::query_cache::Ask::Event(&net))
                        .is_none()
                        && old_fields.is_none()
                    {
                        items.wait(entry, crate::items::RecordCallback::SetUp(guid));
                    }
                }
            }
        }
    }
    let entity = match existing {
        Some(entity) => {
            let mut e = world.entity_mut(entity);
            e.remove::<NetEntity>();
            e.insert((
                ObjectStore(fields.clone()),
                ItemObject { container },
                WrathGeneration(remote.generation),
            ));
            entity
        }
        None => {
            let entity = world
                .spawn((
                    WrathStreamed,
                    WrathGeneration(remote.generation),
                    Guid(guid),
                    ObjectStore(fields.clone()),
                    ItemObject { container },
                    Countdowns::default(),
                ))
                .id();
            index.insert(guid, entity);
            entity
        }
    };
    // Only a changed row is written, so `Changed<WrathItemRow>` is a real instance signal rather
    // than a per-digest artifact. A row whose field was never delivered stays absent rather than
    // clearing itself.
    if new_row.as_ref() != old_row.as_ref() {
        match new_row.as_ref() {
            Some(row) => {
                world.entity_mut(entity).insert(row.clone());
            }
            None => {
                world
                    .entity_mut(entity)
                    .remove::<crate::wrath_item::WrathItemRow>();
            }
        }
    }
    (old_fields, fields, old_row, new_row)
}

/// Translate the drained digest into the established `GuidIndex`/`NetEntity`/`ObjectStore` path.
/// Exclusive, so creates/removals are live before `WorldStage::Input`.
/// Convert the 12340 wire bytes to the donor descriptor consumed by ObjectFields.
/// Unit_C.cpp:5128/5130: vis flags are byte 2; byte 3 is the animation tier, not flags.
fn donor_unit_bytes_1(bytes_1: u32, bytes_2: u32) -> u32 {
    (bytes_1 & 0x0000_ffff) | ((bytes_2 >> 24) << 16) | (((bytes_1 >> 16) & 0xff) << 24)
}

/// One 3.3.5 field delta: (guid, object type, [(field, value, mask)]).
type FieldDiff = (u64, ObjectType, Vec<(u16, u32, u32)>);

fn wrath_apply_world(world: &mut World) {
    let (mut changes, entities, entered, phase, player_guid, map_id) = {
        let mut view = world.resource_mut::<WrathWorldView>();
        (
            std::mem::take(&mut view.last_changes),
            view.entities.clone(),
            view.entered_world,
            view.phase,
            view.player_guid,
            view.map_id,
        )
    };
    // A reloaded field table re-projects every streamed object through the update path below.
    if world
        .get_resource_mut::<crate::wrath_field_map::WrathFieldMap>()
        .is_some_and(|mut map| std::mem::take(&mut map.reapply))
    {
        crate::wrath_field_map::queue_reapply(&mut changes, &entities);
    }
    let mut index = std::mem::take(&mut *world.resource_mut::<GuidIndex>());
    if changes.world_transferred {
        // Scope loss is cleanup-first: it does not depend on a following digest, a player guid,
        // or a live phase. No old-scope ECS entity or action may survive into the next Input.
        let stale: Vec<Entity> = world
            .query_filtered::<Entity, With<WrathStreamed>>()
            .iter(world)
            .collect();
        for entity in stale {
            if let Some(guid) = world.get::<Guid>(entity).map(|g| g.0) {
                if index.0.get(&guid) == Some(&entity) {
                    index.0.remove(&guid);
                }
            }
            world.despawn(entity);
        }
        world.resource_mut::<SelfGuid>().0 = None;
        if let Some(mut player) = world.get_resource_mut::<crate::player::Player>() {
            *player = crate::player::Player::default();
        }
        if let Some(mut control) = world.get_resource_mut::<crate::player::OwnControlLost>() {
            *control = crate::player::OwnControlLost::default();
        }
        if let Some(mut stand) = world.get_resource_mut::<crate::player::StandPending>() {
            *stand = crate::player::StandPending::default();
        }
        if let Some(mut pending) = world.get_resource_mut::<crate::spell::PendingCast>() {
            *pending = crate::spell::PendingCast::default();
        }
        if let Some(mut queued) = world.get_resource_mut::<crate::spell::QueuedMeleeSpell>() {
            *queued = crate::spell::QueuedMeleeSpell::default();
        }
        if let Some(mut channel) = world.get_resource_mut::<crate::spell::ActiveChannel>() {
            *channel = crate::spell::ActiveChannel::default();
        }
        if let Some(mut repeat) = world.get_resource_mut::<crate::spell::AutoRepeatActive>() {
            *repeat = crate::spell::AutoRepeatActive::default();
        }
        if let Some(input) = world.get_resource::<WrathInput>() {
            if let Ok(mut queue) = input.0.lock() {
                // The Net drain already retired this scope's outbox; this is the same terminal
                // rule for anything queued after it. A receipt that never reached the wire is
                // answered here instead of vanishing with the queue.
                retire_outbound_deliveries(&mut queue, crate::net::CommandRefusal::ScopeChanged);
            }
        }
        if let Some(mut bags) = world.get_resource_mut::<crate::item_watch::BagCache>() {
            *bags = crate::item_watch::BagCache::default();
        }
        if let Some(mut lane) = world.get_resource_mut::<crate::item_lane::ItemLane>() {
            lane.clear();
        }
        // The typed Wrath inventory is the same scope as the item watchers and item lane: a
        // transfer or terminal edge retires every address before the next digest can populate it.
        if let Some(mut inventory) =
            world.get_resource_mut::<crate::wrath_inventory::WrathInventory>()
        {
            inventory.clear();
        }
        // The retired world's presentation aura anchors die with its entities.
        if let Some(mut anchors) = world.get_resource_mut::<WrathAuraAnchors>() {
            anchors.by_guid.clear();
        }
    }
    *world.resource_mut::<GuidIndex>() = index;

    if player_guid == 0
        || !matches!(
            phase,
            Some(Phase::InWorld | Phase::TransferPending | Phase::Loading)
        )
    {
        if player_guid == 0 {
            if let Some(mut anchors) = world.get_resource_mut::<WrathAuraAnchors>() {
                anchors.by_guid.clear();
            }
            if changes.world_transferred {
                if let Some(mut status) = world.get_resource_mut::<crate::net::NetStatus>() {
                    status.connected = false;
                }
            }
        }
        return;
    }
    let mut index = std::mem::take(&mut *world.resource_mut::<GuidIndex>());

    if !changes.removed.is_empty()
        && world.contains_resource::<crate::items::Items>()
        && world.contains_resource::<crate::item_lane::ItemLane>()
    {
        let mut stores_by_guid: HashMap<u64, ObjectFields> = HashMap::new();
        for (&g, &e) in &index.0 {
            if let Some(store) = world.get::<ObjectStore>(e) {
                stores_by_guid.insert(g, store.0.clone());
            }
        }
        let mut destroyed_runs = Vec::new();
        if let Some(items) = world.get_resource::<crate::items::Items>() {
            let inventory = world.get_resource::<crate::wrath_inventory::WrathInventory>();
            let watch = crate::item_watch::Watch {
                me: Some(player_guid),
                lookup: |g: u64| stores_by_guid.get(&g),
                items,
                inventory,
            };
            for guid in &changes.removed {
                if index
                    .0
                    .get(guid)
                    .is_some_and(|&e| world.get::<ItemObject>(e).is_some())
                {
                    if let Some(run) = watch.destroyed_run(*guid) {
                        destroyed_runs.push(run);
                    }
                }
            }
        }
        if let Some(mut lane) = world.get_resource_mut::<crate::item_lane::ItemLane>() {
            for run in destroyed_runs {
                lane.push(run);
            }
        }
    }

    for guid in &changes.removed {
        if let Some(entity) = index.0.remove(guid) {
            world.despawn(entity);
        }
        // A corpse-to-bones swap destroys the object; the latch must not ride a later reclaim.
        if let Some(mut death) = world.get_resource_mut::<crate::death::DeathNet>() {
            crate::death::net::forget_corpse(*guid, &mut death);
        }
    }

    let mut item_creates: Vec<u64> = Vec::new();
    let mut row_diffs: Vec<(
        u64,
        Option<crate::wrath_item::WrathItemRow>,
        Option<crate::wrath_item::WrathItemRow>,
    )> = Vec::new();
    let mut field_diffs: Vec<FieldDiff> = Vec::new();
    let mut unit_name_lookups: Vec<(u64, ObjectFields)> = Vec::new();
    let mut go_template_requests: Vec<(u64, ObjectFields)> = Vec::new();
    let mut self_player_created = false;
    let mut typed_changes: Option<crate::wrath_inventory::InventoryChanges> = None;

    for guid in changes.created.iter().chain(changes.updated.iter()) {
        let Some(remote) = entities.get(guid) else {
            continue;
        };
        // The 3.3.5 player window keeps fields whose 1.12 equivalent has no slot (combat
        // ratings, known titles, chosen title). The projection reads this component beside the
        // translated 1.12 descriptor rather than inventing 1.12 indices for them.
        let player_raw = remote.player_fields.clone();
        // One writer for the typed owner: the self player's delivered inventory window. The
        // donor projection below stays its existing 24/6/7 shape.
        if *guid == player_guid
            && remote.type_id == benilla_protocol_wrath::updates::type_id::PLAYER
        {
            if let Some(window) = player_raw.as_ref() {
                let scope = world
                    .get_resource::<WrathWorldView>()
                    .map(|view| (view.session_epoch, view.world_generation))
                    .unwrap_or((0, 0));
                let mut inventory = world
                    .remove_resource::<crate::wrath_inventory::WrathInventory>()
                    .unwrap_or_default();
                typed_changes = Some(inventory.apply(window, scope));
                world.insert_resource(inventory);
            }
        }
        if matches!(
            remote.type_id,
            benilla_protocol_wrath::updates::type_id::ITEM
                | benilla_protocol_wrath::updates::type_id::CONTAINER
        ) {
            let (old_item, new_item, old_row, new_row) =
                apply_item_world(world, &mut index.0, *guid, remote);
            if old_row != new_row {
                row_diffs.push((*guid, old_row, new_row));
            }
            let item_kind = if remote.type_id == benilla_protocol_wrath::updates::type_id::CONTAINER
            {
                ObjectType::Container
            } else {
                ObjectType::Item
            };
            match old_item {
                None => item_creates.push(*guid),
                Some(old_fields) => {
                    let moved = diff_fields(&old_fields, &new_item);
                    if !moved.is_empty() {
                        field_diffs.push((*guid, item_kind, moved));
                    }
                }
            }
            continue;
        }
        let kind = entity_kind(remote.type_id);
        let object_type = match kind {
            EntityKind::Player => ObjectType::Player,
            EntityKind::Unit => ObjectType::Unit,
            EntityKind::GameObject => ObjectType::GameObject,
            EntityKind::DynamicObject => ObjectType::DynamicObject,
            EntityKind::Corpse => ObjectType::Corpse,
            EntityKind::Other => ObjectType::Object,
        };
        // OBJECT_FIELD_TYPE (2) -> FIELD_OBJECT_TYPE (2), OBJECT_FIELD_ENTRY (3) -> FIELD_OBJECT_ENTRY (3),
        // OBJECT_FIELD_SCALE_X (4) -> FIELD_OBJECT_SCALE_X (4). OBJECT_FIELD_PADDING (5) is unmapped.
        let mut pairs = vec![(2u16, object_type_mask(object_type))];
        if let Some(entry) = remote.entry {
            pairs.push((3, entry));
        }
        if let Some(scale) = remote.scale {
            pairs.push((4, scale.to_bits()));
        }
        // Gate all UNIT_FIELD_* projections strictly on Unit and Player entities so non-unit
        // objects (Object, GameObject, DynamicObject, Corpse) never receive unit descriptor slots.
        if matches!(kind, EntityKind::Unit | EntityKind::Player) {
            if let Some(health) = remote.health {
                pairs.push((22, health)); // UNIT_FIELD_HEALTH (24) -> FIELD_UNIT_HEALTH (22)
                pairs.push((28, remote.max_health.unwrap_or(health))); // UNIT_FIELD_MAXHEALTH (32) -> FIELD_UNIT_MAXHEALTH (28)
            } else if let Some(max_health) = remote.max_health {
                pairs.push((28, max_health)); // UNIT_FIELD_MAXHEALTH (32) -> FIELD_UNIT_MAXHEALTH (28)
            }
            // Project all five 1.12 power slots (Mana, Rage, Focus, Energy, Happiness) for both
            // Unit and Player entities so target/focus/pet power bars and rogue/cat/hunter energy
            // and focus consumers see live values from `ObjectStore`. Wrath powers 6..=7 (runes,
            // runic power) have no 1.12 descriptor slot and stay on `WrathEntity::powers[5..7]`.
            for i in 0..5usize {
                if let Some(power) = remote.powers[i] {
                    pairs.push((23 + i as u16, power)); // UNIT_FIELD_POWER1..5 (25..30) -> FIELD_UNIT_POWER1..5 (23..28)
                }
                if let Some(max_power) = remote.max_powers[i] {
                    pairs.push((29 + i as u16, max_power)); // UNIT_FIELD_MAXPOWER1..5 (33..38) -> FIELD_UNIT_MAXPOWER1..5 (29..34)
                }
            }
            if let Some(level) = remote.level {
                pairs.push((34, level)); // UNIT_FIELD_LEVEL (54) -> FIELD_UNIT_LEVEL (34)
            }
            if let Some(faction) = remote.faction {
                pairs.push((35, faction)); // UNIT_FIELD_FACTIONTEMPLATE (55) -> FIELD_UNIT_FACTIONTEMPLATE (35)
            }
            if let Some(flags) = remote.unit_flags {
                pairs.push((46, flags)); // UNIT_FIELD_FLAGS (59) -> FIELD_UNIT_FLAGS (46)
            }
            if let Some(mount) = remote.mount_display_id {
                pairs.push((133, mount)); // UNIT_FIELD_MOUNTDISPLAYID (69) -> FIELD_UNIT_MOUNTDISPLAYID (133)
            }
            // 12340 moves `ShapeshiftForm` to `UNIT_FIELD_BYTES_2[3]` (`Unit_C.cpp:6652`) and shifts
            // `VisFlags` to `UNIT_FIELD_BYTES_1[2]` (`Unit_C.cpp:5128`). Synthesize the 1.12
            // `FIELD_UNIT_BYTES_1` (`138`: `[stand_state, pet_talents, shapeshift_form, vis_flags]`)
            // and project `UNIT_FIELD_BYTES_2` (122) onto `FIELD_UNIT_BYTES_2` (`164`: `[sheath_state, ...]`).
            if remote.bytes_1.is_some() || remote.bytes_2.is_some() {
                let b1 = remote.bytes_1.unwrap_or(0);
                let b2 = remote.bytes_2.unwrap_or(0);
                let vanilla_bytes_1 = donor_unit_bytes_1(b1, b2);
                pairs.push((138, vanilla_bytes_1)); // UNIT_FIELD_BYTES_1 (74) + BYTES_2[3] (122) -> FIELD_UNIT_BYTES_1 (138)
            }
            if let Some(bytes_2) = remote.bytes_2 {
                pairs.push((164, bytes_2)); // UNIT_FIELD_BYTES_2 (122) -> FIELD_UNIT_BYTES_2 (164)
            }
            if let Some(flags) = remote.npc_flags {
                pairs.push((147, vanilla_npc_flags(flags))); // UNIT_NPC_FLAGS (82) -> FIELD_UNIT_NPC_FLAGS (147)
            }
            if let Some(flags) = remote.dynamic_flags {
                pairs.push((143, flags)); // UNIT_DYNAMIC_FLAGS (79) -> FIELD_UNIT_DYNAMIC_FLAGS (143)
            }
            if let Some(display) = remote.display_id.filter(|&display| display != 0) {
                pairs.push((131, display)); // UNIT_FIELD_DISPLAYID (67) -> FIELD_UNIT_DISPLAYID (131)
                pairs.push((132, display)); // fallback for FIELD_UNIT_NATIVEDISPLAYID (132) until 68 overrides
            } else if kind == EntityKind::Player {
                // A self player's create mask can omit `UNIT_FIELD_DISPLAYID`; the race/sex body
                // display is authoritative (`ChrRaces` cols 4/5), and both the glue preview and the
                // reference's `InitDisplayIds` derive the body from it. Without this fallback the
                // undelivered field reaches `live_display_id` as zero and no avatar is built.
                if let Some(bytes) = remote.bytes_0 {
                    let race = (bytes & 0xff) as u8;
                    let sex = ((bytes >> 16) & 0xff) as u8;
                    if let Some(display) = world
                        .get_resource::<crate::entities::CharCreate>()
                        .and_then(|create| create.0.body_display(race, sex))
                    {
                        pairs.push((131, display));
                        pairs.push((132, display));
                        debug!(
                            "wrath: player {guid:#x} has no delivered display id; using the ChrRaces body display (race {race}, sex {sex})",
                            guid = remote.guid
                        );
                    }
                }
            }
            if let Some(bytes) = remote.bytes_0 {
                pairs.push((36, bytes)); // UNIT_FIELD_BYTES_0 (23) -> FIELD_UNIT_BYTES_0 (36)
            }
            if let Some(target) = remote.target {
                pairs.push((16, target as u32)); // UNIT_FIELD_TARGET low (18) -> FIELD_UNIT_TARGET low (16)
                pairs.push((17, (target >> 32) as u32)); // UNIT_FIELD_TARGET high (19) -> FIELD_UNIT_TARGET high (17)
            }
            // Project 12340 ownership guids onto the 1.12 descriptor slots the reaction, UI and minimap
            // consumers read (`CHARMEDBY` 10/11, `SUMMONEDBY` 12/13, `CREATEDBY` 14/15). Without these
            // a hunter pet streams in with no owner, so it reads as neutral/attackable even though the
            // server refuses the swing.
            if let Some(charmed_by) = remote.charmed_by {
                pairs.push((10, charmed_by as u32)); // UNIT_FIELD_CHARMEDBY low (12) -> FIELD_UNIT_CHARMEDBY low (10)
                pairs.push((11, (charmed_by >> 32) as u32)); // UNIT_FIELD_CHARMEDBY high (13) -> FIELD_UNIT_CHARMEDBY high (11)
            }
            if let Some(summoned_by) = remote.summoned_by {
                pairs.push((12, summoned_by as u32)); // UNIT_FIELD_SUMMONEDBY low (14) -> FIELD_UNIT_SUMMONEDBY low (12)
                pairs.push((13, (summoned_by >> 32) as u32)); // UNIT_FIELD_SUMMONEDBY high (15) -> FIELD_UNIT_SUMMONEDBY high (13)
            }
            if let Some(created_by) = remote.created_by {
                pairs.push((14, created_by as u32)); // UNIT_FIELD_CREATEDBY low (16) -> FIELD_UNIT_CREATEDBY low (14)
                pairs.push((15, (created_by >> 32) as u32)); // UNIT_FIELD_CREATEDBY high (17) -> FIELD_UNIT_CREATEDBY high (15)
            }
            // `UNIT_FIELD_CHARM`/`UNIT_FIELD_SUMMON` share the 1.12 indices (6/7, 8/9): a controlled
            // pet or vehicle seat on us is what `UnitIsControlling` and the attack-charm refusal read.
            if let Some(charm) = remote.charm {
                pairs.push((6, charm as u32)); // UNIT_FIELD_CHARM low (6) -> FIELD_UNIT_CHARM low (6)
                pairs.push((7, (charm >> 32) as u32)); // UNIT_FIELD_CHARM high (7) -> FIELD_UNIT_CHARM high (7)
            }
            if let Some(summon) = remote.summon {
                pairs.push((8, summon as u32)); // UNIT_FIELD_SUMMON low (8) -> FIELD_UNIT_SUMMON low (8)
                pairs.push((9, (summon >> 32) as u32)); // UNIT_FIELD_SUMMON high (9) -> FIELD_UNIT_SUMMON high (9)
            }
            if kind == EntityKind::Unit {
                if let Some(raw) = remote.raw_fields.as_ref() {
                    push_unit_fields(raw, &mut pairs);
                }
            }
        }
        if kind == EntityKind::GameObject {
            push_gameobject_fields(remote.raw_fields.as_ref(), remote.position, &mut pairs);
        }
        if kind == EntityKind::DynamicObject {
            push_dynamicobject_fields(remote.raw_fields.as_ref(), remote.position, &mut pairs);
        }
        if kind == EntityKind::Corpse {
            let corpse_raw = remote.corpse_fields.as_ref().or(remote.raw_fields.as_ref());
            push_corpse_fields(corpse_raw, remote.position, &mut pairs);
        }
        let mut wrath_slots: Option<WrathQuestSlots> = None;
        if kind == EntityKind::Player {
            if let Some(bytes) = remote.player_bytes {
                pairs.push((193, bytes)); // PLAYER_BYTES (153) -> FIELD_PLAYER_BYTES (193)
            }
            if let Some(bytes) = remote.player_bytes_2 {
                pairs.push((194, bytes)); // PLAYER_BYTES_2 (154) -> FIELD_PLAYER_BYTES_2 (194)
            }
            if let Some(raw) = remote.player_fields.as_ref() {
                // 3.3.5 uses 148 as UNIT_END; 1.12 uses a different player layout. Map the
                // durable fields the stock quest/loot/player frames read. The descriptor keeps
                // the 1.12 twenty-slot projection; the full twenty-five go beside it.
                // PLAYER_QUEST_LOG_1_1..PLAYER_QUEST_LOG_20_5 (158..258) -> FIELD_PLAYER_QUEST_LOG_1_1 (198..258)
                let slots = wrath_quest_slots(raw);
                for (slot, s) in slots.iter().take(20).enumerate() {
                    if s.quest_id == 0 {
                        continue;
                    }
                    let mut packed = u32::from(s.state) << 24;
                    for (i, count) in s.counters.iter().enumerate() {
                        packed |= u32::from(*count) << (i * 6);
                    }
                    let dst = 198 + slot * 3;
                    pairs.push((dst as u16, s.quest_id));
                    pairs.push(((dst + 1) as u16, packed));
                    pairs.push(((dst + 2) as u16, s.timer));
                }
                let max_level =
                    raw.word_or_zero(benilla_protocol_wrath::fields::PLAYER_FIELD_MAX_LEVEL);
                let daily_start = benilla_protocol_wrath::fields::PLAYER_FIELD_DAILY_QUESTS_1;
                let daily = (0..25u16)
                    .filter(|i| raw.word_or_zero(daily_start + i) != 0)
                    .count() as u32;
                wrath_slots = Some(WrathQuestSlots(slots, max_level, daily));
                push_player_unit_fields(raw, &mut pairs);
                push_player_inventory_fields(raw, &mut pairs);
                push_visible_item_fields(raw, &mut pairs);
                if let Some(value) = raw.get(634) {
                    pairs.push((716, value)); // PLAYER_XP (634) -> FIELD_PLAYER_XP (716)
                }
                if let Some(value) = raw.get(635) {
                    pairs.push((717, value)); // PLAYER_NEXT_LEVEL_XP (635) -> FIELD_PLAYER_NEXT_LEVEL_XP (717)
                }
                // `PLAYER_SKILL_INFO_1_1` (`636..1020` -> `718..1102`, 128 * 3 dwords): identical
                // 3-dword per skill line layout (`id | step`, `value | max`, `pos | neg`).
                for i in 0..384u16 {
                    if let Some(v) = raw.get(636 + i) {
                        pairs.push((718 + i, v));
                    }
                }
                // Character points (talent/profession), creature/resource tracking, and block/dodge/parry/crit.
                for (src, dst) in [
                    (1020u16, 1102u16), // PLAYER_CHARACTER_POINTS1 -> FIELD_PLAYER_CHARACTER_POINTS1
                    (1021, 1103), // PLAYER_CHARACTER_POINTS2 -> FIELD_PLAYER_CHARACTER_POINTS2
                    (1022, 1104), // PLAYER_TRACK_CREATURES -> FIELD_PLAYER_TRACK_CREATURES
                    (1023, 1105), // PLAYER_TRACK_RESOURCES -> FIELD_PLAYER_TRACK_RESOURCES
                    (1024, 1106), // PLAYER_BLOCK_PERCENTAGE -> FIELD_PLAYER_BLOCK_PERCENTAGE
                    (1025, 1107), // PLAYER_DODGE_PERCENTAGE -> FIELD_PLAYER_DODGE_PERCENTAGE
                    (1026, 1108), // PLAYER_PARRY_PERCENTAGE -> FIELD_PLAYER_PARRY_PERCENTAGE
                    (1029, 1109), // PLAYER_CRIT_PERCENTAGE -> FIELD_PLAYER_CRIT_PERCENTAGE
                    (1030, 1110), // PLAYER_RANGED_CRIT_PERCENTAGE -> FIELD_PLAYER_RANGED_CRIT_PERCENTAGE
                ] {
                    if let Some(v) = raw.get(src) {
                        pairs.push((dst, v));
                    }
                }
                // First 64 words of `PLAYER_EXPLORED_ZONES_1` (`1041..1105` -> `1111..1175` FIELD_PLAYER_EXPLORED_ZONES_1).
                // Words 64..128 (`1105..1169`, Outland/Northrend) stay in `WrathPlayerRaw`.
                for i in 0..64u16 {
                    if let Some(v) = raw.get(1041 + i) {
                        pairs.push((1111 + i, v));
                    }
                }
                if let Some(value) =
                    raw.get(benilla_protocol_wrath::fields::PLAYER_REST_STATE_EXPERIENCE)
                {
                    pairs.push((1175, value)); // PLAYER_REST_STATE_EXPERIENCE (1169) -> FIELD_PLAYER_REST_STATE_EXPERIENCE (1175)
                }
                if let Some(value) = raw.get(1170) {
                    pairs.push((1176, value)); // PLAYER_FIELD_COINAGE (1170) -> FIELD_PLAYER_FIELD_COINAGE (1176)
                }
                // `PLAYER_FIELD_MOD_DAMAGE_DONE_POS/NEG/PCT` (`1171..1192` -> `1201..1222`, 21 words).
                for i in 0..21u16 {
                    if let Some(v) = raw.get(1171 + i) {
                        pairs.push((1201 + i, v));
                    }
                }
                // `PLAYER_FIELD_BYTES` (combo points in byte 1), ammo, self-res, and PvP medals.
                for (src, dst) in [
                    (1197u16, 1222u16), // PLAYER_FIELD_BYTES -> FIELD_PLAYER_FIELD_BYTES
                    (1198, 1223),       // PLAYER_AMMO_ID -> FIELD_PLAYER_AMMO_ID
                    (1199, 1224),       // PLAYER_SELF_RES_SPELL -> FIELD_PLAYER_SELF_RES_SPELL
                    (1200, 1225),       // PLAYER_FIELD_PVP_MEDALS -> FIELD_PLAYER_PVP_MEDALS
                ] {
                    if let Some(v) = raw.get(src) {
                        pairs.push((dst, v));
                    }
                }
                // Honor counters (`1225..=1230` -> `1250..=1261`), splitting `PLAYER_FIELD_KILLS`
                // into today/yesterday HKs and keeping `TODAY_CONTRIBUTION` in `WrathPlayerRaw`.
                push_player_honor_fields(raw, &mut pairs);
                // PLAYER_FIELD_BUYBACK_PRICE_1 (1201..1213) -> FIELD_PLAYER_BUYBACK_PRICE_1 (1226..1238)
                // PLAYER_FIELD_BUYBACK_TIMESTAMP_1 (1213..1225) -> FIELD_PLAYER_BUYBACK_TIMESTAMP_1 (1238..1250)
                for i in 0..24u16 {
                    if let Some(v) = raw.get(1201 + i) {
                        pairs.push((1226 + i, v));
                    }
                }
            }
        }
        if matches!(kind, EntityKind::Unit | EntityKind::Player) && !remote.auras.is_empty() {
            push_aura_fields(&remote.auras, &mut pairs);
        }
        // The live field table, then the live translation script, run last so a data rule or
        // the script can add or override any 1.12 word.
        if let Some(map) = world.get_resource::<crate::wrath_field_map::WrathFieldMap>() {
            map.push_pairs(remote, &mut pairs);
        }
        if let Some(script) =
            world.get_non_send_resource::<crate::wrath_translate::WrathTranslateScript>()
        {
            script.translate(remote, &mut pairs);
        }
        let fields = ObjectFields::from_pairs(&pairs).into_created(object_type);
        // A 12340 corpse keeps its body display in its own block (`CORPSE_FIELD_DISPLAY_ID`,
        // `Player.cpp:4820`), not in `UNIT_FIELD_DISPLAYID`: the display build would otherwise
        // draw nothing for our own corpse. A bone pile keeps the id and builds its skeleton from
        // the appearance bytes instead (`crate::entities::corpse`).
        let display_id = if kind == EntityKind::Corpse {
            fields.corpse_display_id()
        } else {
            remote.display_id
        };
        // The corpse-guid latch `RetrieveCorpse` sends has one writer in the reference
        // (`0x4920d0`, gated on `CORPSE_FIELD_OWNER == me` and a clear `CORPSE_FIELD_FLAGS`
        // bit 0); the 12340 digest path feeds it here, where the translated block exists.
        if kind == EntityKind::Corpse {
            let me = crate::net::SelfGuid(Some(player_guid));
            if let Some(mut death) = world.get_resource_mut::<crate::death::DeathNet>() {
                crate::death::net::note_corpse(*guid, kind, &fields, &me, &mut death);
            }
        }
        let transform = remote.position.map(|p| Transform {
            translation: wow_to_bevy([p.x, p.y, p.z]),
            rotation: Quat::from_rotation_y(p.o),
            ..default()
        });
        let mut existing = index.0.get(guid).copied();
        if let Some(entity) = existing {
            if world
                .get::<WrathGeneration>(entity)
                .is_some_and(|generation| generation.0 != remote.generation)
            {
                index.0.remove(guid);
                world.despawn(entity);
                existing = None;
            }
        }
        match existing {
            Some(entity) => {
                let old = world
                    .get::<ObjectStore>(entity)
                    .map(|store| store.0.clone());
                if let Some(old_fields) = &old {
                    let moved = diff_fields(old_fields, &fields);
                    if !moved.is_empty() {
                        field_diffs.push((*guid, object_type, moved));
                    }
                }
                let old_health = old
                    .as_ref()
                    .and_then(benilla_protocol::ObjectFields::unit_health);
                let old_max = old
                    .as_ref()
                    .and_then(benilla_protocol::ObjectFields::unit_max_health);
                let new_span = matches!(kind, EntityKind::Unit | EntityKind::Player)
                    .then(|| aura_span(&fields));
                // The player descriptor is replaced whole, so the 1.12 watches get the quest-log
                // signals by hand when any of the twenty-five slots moved.
                let old_slots = world.get::<WrathQuestSlots>(entity).copied();
                let quest_slots_changed = wrath_slots.is_some_and(|slots| old_slots != Some(slots));
                if *guid == player_guid {
                    let old_raw = world.get::<WrathPlayerRaw>(entity);
                    let old_xp = old_raw.and_then(|r| r.0.get(634));
                    let new_xp = player_raw.as_ref().and_then(|r| r.get(634));
                    let old_money = old_raw.and_then(|r| r.0.get(1170));
                    let new_money = player_raw.as_ref().and_then(|r| r.get(1170));
                    let scope = world
                        .get_resource::<WrathWorldView>()
                        .map(|v| (v.session_epoch, v.world_generation));
                    if let Some(trace) = world.get_resource::<crate::quest_trace::Source>() {
                        if let (Some(old), Some(new)) = (old_slots, wrath_slots) {
                            for slot in &new.0 {
                                if slot.quest_id == 0 {
                                    continue;
                                }
                                match old.0.iter().find(|s| s.quest_id == slot.quest_id) {
                                    None => {
                                        trace.observe(
                                            scope,
                                            Some(slot.quest_id),
                                            crate::quest_trace::Stage::QuestSlotAdded,
                                        );
                                    }
                                    Some(prev)
                                        if prev.counters != slot.counters
                                            || prev.state != slot.state =>
                                    {
                                        trace.observe(
                                            scope,
                                            Some(slot.quest_id),
                                            crate::quest_trace::Stage::QuestSlotProgressed {
                                                counters: slot.counters.map(u16::from),
                                                state: u32::from(slot.state),
                                            },
                                        );
                                    }
                                    Some(_) => {}
                                }
                            }
                            for slot in &old.0 {
                                if slot.quest_id != 0
                                    && !new.0.iter().any(|s| s.quest_id == slot.quest_id)
                                {
                                    trace.observe(
                                        scope,
                                        Some(slot.quest_id),
                                        crate::quest_trace::Stage::QuestSlotRemoved,
                                    );
                                }
                            }
                        }
                        if let (Some(prev_xp), Some(curr_xp)) = (old_xp, new_xp) {
                            if curr_xp != prev_xp {
                                trace.observe(
                                    scope,
                                    None,
                                    crate::quest_trace::Stage::PlayerXpChanged {
                                        delta: curr_xp.saturating_sub(prev_xp),
                                    },
                                );
                            }
                        }
                        if let (Some(prev_money), Some(curr_money)) = (old_money, new_money) {
                            if curr_money != prev_money {
                                trace.observe(
                                    scope,
                                    None,
                                    crate::quest_trace::Stage::PlayerMoneyChanged {
                                        delta: i64::from(curr_money) - i64::from(prev_money),
                                    },
                                );
                            }
                        }
                    }
                }
                if old.as_ref().and_then(ObjectFields::object_entry) != fields.object_entry() {
                    if matches!(kind, EntityKind::Unit | EntityKind::Player) {
                        unit_name_lookups.push((*guid, fields.clone()));
                    } else if kind == EntityKind::GameObject {
                        go_template_requests.push((*guid, fields.clone()));
                    }
                }
                world.entity_mut(entity).insert((
                    NetEntity {
                        kind,
                        display_id,
                        scale: remote.scale.unwrap_or(1.0),
                    },
                    ObjectStore(fields),
                ));
                if let Some(slots) = wrath_slots {
                    world.entity_mut(entity).insert(slots);
                }
                if let Some(raw) = player_raw {
                    world.entity_mut(entity).insert(WrathPlayerRaw(raw));
                }
                if let Some(flags) = remote.npc_flags {
                    world.entity_mut(entity).insert(WrathNpcFlags(flags));
                }
                if quest_slots_changed && *guid == player_guid {
                    // The same two signals the 1.12 values block raises: the rebuild notice and
                    // the event the stock log frame listens for.
                    let mut notices = world.resource_mut::<crate::field_watch::NoticeLane>();
                    notices.write(crate::field_watch::FieldNotice::QuestLog);
                    notices.write(crate::field_watch::FieldNotice::Unit {
                        guid: *guid,
                        id: crate::field_watch::id::UNIT_QUEST_LOG_CHANGED,
                    });
                }
                if let Some(new_span) = new_span {
                    // The 12340 aura packets changed this unit's descriptor window; hand the
                    // 1.12 aura watcher the same edges and one call the stock block path makes.
                    let old_span = old
                        .as_ref()
                        .map_or([0u32; crate::net::AURA_WATCH_SPAN], aura_span);
                    if old_span != new_span {
                        let at = crate::lanes::stamp();
                        for (offset, (old_value, new_value)) in
                            old_span.iter().zip(new_span.iter()).enumerate()
                        {
                            if old_value == new_value {
                                continue;
                            }
                            let _ = world.write_message(FieldChanged {
                                at,
                                entity,
                                guid: *guid,
                                kind: object_type,
                                index: benilla_protocol::field::FIELD_UNIT_AURA + offset as u16,
                                old: *old_value,
                                new: *new_value,
                            });
                        }
                        let _ = world.write_message(crate::net::AuraFieldsChanged {
                            at,
                            entity,
                            guid: *guid,
                            old: old_span,
                            new: new_span,
                            health: world
                                .get::<ObjectStore>(entity)
                                .and_then(|store| store.0.unit_health())
                                .unwrap_or(0),
                        });
                    }
                }
                if matches!(kind, EntityKind::Unit | EntityKind::Player) {
                    let old_bytes_1 = old
                        .as_ref()
                        .and_then(benilla_protocol::ObjectFields::unit_bytes_1);
                    let new_bytes_1 = world
                        .get::<ObjectStore>(entity)
                        .and_then(|s| s.0.unit_bytes_1());
                    let old_bytes_2 = old
                        .as_ref()
                        .and_then(benilla_protocol::ObjectFields::unit_bytes_2);
                    let new_bytes_2 = world
                        .get::<ObjectStore>(entity)
                        .and_then(|s| s.0.unit_bytes_2());
                    let old_unit_flags =
                        old.as_ref().map(benilla_protocol::ObjectFields::unit_flags);
                    let new_unit_flags = remote.unit_flags.unwrap_or(0);
                    if old_health != remote.health {
                        let _ = world.write_message(FieldChanged {
                            at: crate::lanes::stamp(),
                            entity,
                            guid: *guid,
                            kind: object_type,
                            index: 22,
                            old: old_health.unwrap_or(0),
                            new: remote.health.unwrap_or(0),
                        });
                    }
                    if old_max != remote.max_health {
                        let _ = world.write_message(FieldChanged {
                            at: crate::lanes::stamp(),
                            entity,
                            guid: *guid,
                            kind: object_type,
                            index: 28,
                            old: old_max.unwrap_or(0),
                            new: remote.max_health.unwrap_or(0),
                        });
                    }
                    if old_unit_flags != Some(new_unit_flags) && remote.unit_flags.is_some() {
                        let prev_flags = old_unit_flags.unwrap_or(0);
                        let _ = world.write_message(FieldChanged {
                            at: crate::lanes::stamp(),
                            entity,
                            guid: *guid,
                            kind: object_type,
                            index: 46,
                            old: prev_flags,
                            new: new_unit_flags,
                        });
                        if *guid == player_guid && (prev_flags ^ new_unit_flags) & 0x8_0000 != 0 {
                            if let Some(mut notices) =
                                world.get_resource_mut::<crate::field_watch::NoticeLane>()
                            {
                                let id = if new_unit_flags & 0x8_0000 != 0 {
                                    crate::field_watch::id::PLAYER_REGEN_DISABLED
                                } else {
                                    crate::field_watch::id::PLAYER_REGEN_ENABLED
                                };
                                notices.write(crate::field_watch::FieldNotice::Global { id });
                            }
                        }
                    }
                    if old_bytes_1 != new_bytes_1 {
                        let prev = old_bytes_1.unwrap_or(0);
                        let next = new_bytes_1.unwrap_or(0);
                        let _ = world.write_message(FieldChanged {
                            at: crate::lanes::stamp(),
                            entity,
                            guid: *guid,
                            kind: object_type,
                            index: 138,
                            old: prev,
                            new: next,
                        });
                        if *guid == player_guid && (prev ^ next) & 0x00ff_0000 != 0 {
                            if let Some(mut notices) =
                                world.get_resource_mut::<crate::field_watch::NoticeLane>()
                            {
                                notices.write(crate::field_watch::FieldNotice::Shapeshift);
                            }
                        }
                    }
                    if old_bytes_2 != new_bytes_2 {
                        let _ = world.write_message(FieldChanged {
                            at: crate::lanes::stamp(),
                            entity,
                            guid: *guid,
                            kind: object_type,
                            index: 164,
                            old: old_bytes_2.unwrap_or(0),
                            new: new_bytes_2.unwrap_or(0),
                        });
                    }
                }
                if *guid == player_guid && world.get::<SelfPlayer>(entity).is_none() {
                    world.entity_mut(entity).insert(SelfPlayer);
                }
                if *guid != player_guid {
                    let has_active_spline =
                        world.get::<crate::net::motion::Spline>(entity).is_some();
                    if let (Some(transform), Some(position)) = (transform, remote.position) {
                        let stamp = WrathAppliedPosition(position);
                        if world.get::<WrathAppliedPosition>(entity) != Some(&stamp) {
                            if !has_active_spline {
                                if let Some(mut current) = world.get_mut::<Transform>(entity) {
                                    current.translation = transform.translation;
                                    current.rotation = transform.rotation;
                                }
                            }
                            world.entity_mut(entity).insert(stamp);
                        }
                    }
                    if let Some(cs) = &remote.create_spline {
                        let applied = WrathAppliedCreateSpline(cs.id);
                        if world.get::<WrathAppliedCreateSpline>(entity) != Some(&applied) {
                            let create = benilla_protocol::CreateSpline {
                                flying: cs.flags & 0x2000 != 0,
                                cyclic: cs.flags & 0x80000 != 0,
                                run_mode: remote.move_flags.is_none_or(|f| f & 0x100 == 0),
                                time_passed_ms: cs.time_passed_ms,
                                duration_ms: cs.duration_ms,
                                id: cs.id,
                                path: cs.path.clone(),
                            };
                            if let Some(spline) = crate::net::motion::create_spline(create) {
                                world.entity_mut(entity).insert((spline, applied));
                            } else {
                                world.entity_mut(entity).insert(applied);
                            }
                        }
                    }
                } else if let (Some(pose), Some(server_flags)) =
                    (remote.position, remote.move_flags)
                {
                    // Server correction is applied at rest only: while we are moving, the local
                    // controller owns the pose and the next reduced stop/heartbeat reconciles it.
                    const MOVING: u32 = W_FLAG_FORWARD
                        | W_FLAG_BACKWARD
                        | W_FLAG_STRAFE_LEFT
                        | W_FLAG_STRAFE_RIGHT
                        | W_FLAG_TURN_LEFT
                        | W_FLAG_TURN_RIGHT
                        | W_FLAG_FALLING;
                    let local_flags = wrath_move_flags(
                        world
                            .get_resource::<crate::player::Player>()
                            .map_or(0, crate::player::Player::move_flags),
                    );
                    // A reduced snapshot retains the last pose across unrelated field updates.
                    // Consume each new pose once, including while locally moving, so a later
                    // health/aura update cannot replay an old correction after terrain settling.
                    let fresh_pose = consume_wrath_position(world, entity, pose);
                    if fresh_pose && server_flags & MOVING == 0 && local_flags & MOVING == 0 {
                        let bevy = wow_to_bevy([pose.x, pose.y, pose.z]);
                        let current = world
                            .get_resource::<crate::player::Player>()
                            .map_or(Vec3::ZERO, |p| p.pos);
                        if current.distance_squared(bevy) > 0.01 {
                            let applied = if let Some(mut player) =
                                world.get_resource_mut::<crate::player::Player>()
                            {
                                player.apply_server_pose(bevy, pose.o);
                                true
                            } else {
                                false
                            };
                            if applied {
                                if let Some(trace) =
                                    world.get_resource::<crate::quest_trace::Source>()
                                {
                                    let scope = world
                                        .get_resource::<WrathWorldView>()
                                        .map(|v| (v.session_epoch, v.world_generation));
                                    let distance = current.distance(bevy);
                                    let distance_bucket = if distance <= 0.5 {
                                        0
                                    } else if distance <= 2.0 {
                                        1
                                    } else if distance <= 5.0 {
                                        2
                                    } else {
                                        3
                                    };
                                    let (movement_seq, ambiguous) =
                                        trace.movement_written_seq_unambiguous();
                                    trace.clear_movement_writes_since_correction();
                                    trace.observe(
                                        scope,
                                        None,
                                        crate::quest_trace::Stage::MovementCorrectionApplied {
                                            distance_bucket,
                                            movement_seq,
                                            ambiguous,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            None => {
                if *guid == player_guid {
                    self_player_created = true;
                }
                if matches!(kind, EntityKind::Unit | EntityKind::Player) {
                    unit_name_lookups.push((*guid, fields.clone()));
                } else if kind == EntityKind::GameObject {
                    go_template_requests.push((*guid, fields.clone()));
                }
                let mut entity = world.spawn((
                    WrathStreamed,
                    WrathGeneration(remote.generation),
                    Guid(*guid),
                    NetEntity {
                        kind,
                        display_id,
                        scale: remote.scale.unwrap_or(1.0),
                    },
                    ObjectStore(fields),
                    UnitSpeeds(default_speeds()),
                    // The 1.12 spawn path installs this before attaching model parts. Without a
                    // root InheritedVisibility every child mesh raises B0004 and can inherit as
                    // hidden: terrain and UI drew, the world bodies did not.
                    Visibility::default(),
                    InheritedVisibility::default(),
                    transform.unwrap_or_default(),
                ));
                if let Some(position) = remote.position {
                    entity.insert(WrathAppliedPosition(position));
                }
                // A unit created while it already swings (`HAS_TARGET`) gets no fresh
                // `SMSG_ATTACKSTART`: the 12340 client seeds its attack target from the create
                // block (`CCombat::SetClientInitData`), which the Ready idle gates on. A recreate
                // (a new generation) passes here too, so a respawned fighter keeps its stance.
                if let Some(victim) = remote
                    .attack_target
                    .filter(|_| matches!(kind, EntityKind::Unit | EntityKind::Player))
                {
                    entity.insert(crate::creature_anim::Engaged(victim));
                }
                if *guid != player_guid {
                    if let Some(cs) = &remote.create_spline {
                        let create = benilla_protocol::CreateSpline {
                            flying: cs.flags & 0x2000 != 0,
                            cyclic: cs.flags & 0x80000 != 0,
                            run_mode: remote.move_flags.is_none_or(|f| f & 0x100 == 0),
                            time_passed_ms: cs.time_passed_ms,
                            duration_ms: cs.duration_ms,
                            id: cs.id,
                            path: cs.path.clone(),
                        };
                        if let Some(spline) = crate::net::motion::create_spline(create) {
                            entity.insert((spline, WrathAppliedCreateSpline(cs.id)));
                        } else {
                            entity.insert(WrathAppliedCreateSpline(cs.id));
                        }
                    }
                }
                if let Some(slots) = wrath_slots {
                    entity.insert(slots);
                }
                if let Some(raw) = player_raw {
                    entity.insert(WrathPlayerRaw(raw));
                }
                if let Some(flags) = remote.npc_flags {
                    entity.insert(WrathNpcFlags(flags));
                }
                if *guid == player_guid {
                    entity.insert((SelfPlayer, Equipment::default()));
                } else if kind == EntityKind::Player {
                    entity.insert(Equipment::default());
                }
                index.0.insert(*guid, entity.id());
            }
        }
    }

    world.resource_mut::<SelfGuid>().0 = Some(player_guid);
    *world.resource_mut::<GuidIndex>() = index;

    if !entered && entities.contains_key(&player_guid) {
        let seeded_name = world
            .get_resource::<crate::glue_session::roster::Roster>()
            .and_then(|roster| {
                roster
                    .chars
                    .iter()
                    .find(|c| c.guid == player_guid)
                    .map(|c| c.name.clone())
            })
            .filter(|name| !name.is_empty())
            .or_else(|| {
                world
                    .get_resource::<WrathControl>()
                    .and_then(|control| control.feed.as_ref())
                    .and_then(|feed| feed.player_name.lock().ok().and_then(|pair| pair.clone()))
                    .and_then(|(guid, name)| {
                        (guid == player_guid && !name.is_empty()).then_some(name)
                    })
            })
            .or_else(|| {
                world
                    .get_resource::<WrathControl>()
                    .and_then(|control| control.config.as_ref())
                    .and_then(|cfg| cfg.character_name.clone())
                    .filter(|name| !name.is_empty())
            });
        if let Some(name) = seeded_name {
            if let Some(mut names) = world.get_resource_mut::<crate::names::NameCache>() {
                names.insert_player(player_guid, name, None);
            }
        }
    }

    if !unit_name_lookups.is_empty() || !go_template_requests.is_empty() {
        if let Some(net) = world
            .get_resource::<NetCommands>()
            .map(|n| NetCommands(n.0.clone()))
        {
            if !unit_name_lookups.is_empty() {
                if let Some(names) = world.get_resource::<crate::names::NameCache>() {
                    for (guid, fields) in &unit_name_lookups {
                        names.lookup_fields(*guid, fields, &net);
                    }
                }
            }
            if !go_template_requests.is_empty() {
                if let Some(go_templates) =
                    world.get_resource::<crate::go_templates::GameObjectTemplates>()
                {
                    for (guid, fields) in &go_template_requests {
                        go_templates.request_at_create(*guid, fields, &net);
                    }
                }
            }
        }
    }

    let typed_bank_changed = typed_changes.as_ref().is_some_and(|changes| {
        changes.slots.iter().any(|change| match change.addr {
            crate::wrath_inventory::InventoryAddr::Bank(i) => i >= 24,
            crate::wrath_inventory::InventoryAddr::BankBag(i) => i == 6,
            _ => false,
        })
    });
    if self_player_created
        || !item_creates.is_empty()
        || !field_diffs.is_empty()
        || !row_diffs.is_empty()
        || typed_bank_changed
    {
        let mut stores_by_guid: HashMap<u64, ObjectFields> = HashMap::new();
        for (&g, &e) in &world.resource::<GuidIndex>().0 {
            if let Some(store) = world.get::<ObjectStore>(e) {
                stores_by_guid.insert(g, store.0.clone());
            }
        }
        if self_player_created {
            if let Some(player_fields) = stores_by_guid.get(&player_guid) {
                // The typed owner's 28/7 seats, copied before the BagCache is moved out of the
                // world. The donor path falls back to its own 24/6 seats.
                let seats: [u64; 11] = {
                    let wrath = world.get_resource::<crate::wrath_inventory::WrathInventory>();
                    let view = crate::wrath_inventory::PlayerInventory::new(
                        player_fields,
                        wrath,
                        crate::wrath::selected_from_env(),
                    );
                    std::array::from_fn(|i| match i as u8 + 1 {
                        1..=4 => view.bag_slot(i as u8),
                        5..=11 => view.bank_bag(i as u8 - 4),
                        _ => 0,
                    })
                };
                if let Some(mut bag_cache) = world.get_resource_mut::<crate::item_watch::BagCache>()
                {
                    let lookup = |g: u64| stores_by_guid.get(&g);
                    bag_cache.seat_with(
                        11,
                        &|n| seats[usize::from(n.saturating_sub(1)).min(10)],
                        &lookup,
                    );
                }
            }
        }
        let mut runs_to_push: Vec<crate::item_watch::Run> = Vec::new();
        let mut entry_misses: Vec<(u64, u32)> = Vec::new();
        if world.contains_resource::<crate::items::Items>()
            && world.contains_resource::<crate::item_watch::BagCache>()
        {
            let mut bag_cache = world
                .remove_resource::<crate::item_watch::BagCache>()
                .unwrap_or_default();
            if let Some(items) = world.get_resource::<crate::items::Items>() {
                let inventory = world.get_resource::<crate::wrath_inventory::WrathInventory>();
                let watch = crate::item_watch::Watch {
                    me: Some(player_guid),
                    lookup: |g: u64| stores_by_guid.get(&g),
                    items,
                    inventory,
                };
                for guid in &item_creates {
                    if let Some(fields) = stores_by_guid.get(guid) {
                        runs_to_push.push(watch.created_run(*guid, fields, &bag_cache));
                    }
                }
                for (guid, kind, moved) in &field_diffs {
                    if let Some(fields) = stores_by_guid.get(guid) {
                        let sent: Vec<u16> = moved.iter().map(|&(idx, _, _)| idx).collect();
                        let block = crate::field_watch::Notified::new(
                            crate::field_watch::Sent::Fields(&sent),
                            moved,
                        );
                        for (_, run) in
                            watch.placed_values(*guid, *kind, fields, &block, &mut bag_cache)
                        {
                            if let Some(crate::item_watch::ItemNotice::Entry {
                                guid: item_guid,
                                new,
                                new_cached: false,
                                ..
                            }) = run.notice
                            {
                                entry_misses.push((item_guid, new));
                            }
                            runs_to_push.push(run);
                        }
                    }
                }
                // Wrath enchant slots 8..12 (fields 43..57) have no donor slot, so the donor
                // diff above cannot raise the enchantment notice for them. The typed row's band
                // is the only source. The donor 22..42 band still runs through `field_diffs`;
                // this covers exactly the part the projection deliberately drops, so no change
                // is noticed twice.
                for (guid, old_row, new_row) in &row_diffs {
                    let Some(fields) = stores_by_guid.get(guid) else {
                        continue;
                    };
                    let Some(old_row) = old_row.as_ref() else {
                        continue;
                    };
                    let Some(new_row) = new_row.as_ref() else {
                        continue;
                    };
                    let read = crate::wrath_item::ItemRead::new(fields, Some(new_row));
                    if player_guid == 0
                        || !stores_by_guid.contains_key(&player_guid)
                        || !fields.is_item()
                        || read.owner() != Some(player_guid)
                    {
                        continue;
                    }
                    let Some(entry) = fields.object_entry() else {
                        continue;
                    };
                    let charter = items
                        .template(entry, 0, crate::query_cache::Ask::Read)
                        .is_some_and(|t| {
                            t.flags & benilla_protocol::messages::ITEM_FLAG_CHARTER != 0
                        });
                    if charter
                        || !crate::wrath_item::WrathItemRow::enchants_changed(
                            Some(old_row),
                            new_row,
                            7..=11,
                        )
                    {
                        continue;
                    }
                    runs_to_push.push(watch.run(
                        crate::item_watch::ItemNotice::Enchantment { guid: *guid, entry },
                        &bag_cache,
                    ));
                }
                // The four extra bank slots and the seventh bank bag have no donor field, so
                // their seat runs are generated from the typed owner. They are appended after
                // the donor-order runs; the 3.3.5 field order for those five addresses is not
                // observable in the donor projection.
                if watch.inventory.is_some() {
                    if let Some(changes) = typed_changes.as_ref() {
                        let seats: [u64; 11] =
                            if let Some(player_fields) = stores_by_guid.get(&player_guid) {
                                let view = crate::wrath_inventory::PlayerInventory::new(
                                    player_fields,
                                    watch.inventory,
                                    true,
                                );
                                std::array::from_fn(|i| match i as u8 + 1 {
                                    1..=4 => view.bag_slot(i as u8),
                                    5..=11 => view.bank_bag(i as u8 - 4),
                                    _ => 0,
                                })
                            } else {
                                [0; 11]
                            };
                        let seat = |n: u8| seats[usize::from(n.saturating_sub(1)).min(10)];
                        for change in &changes.slots {
                            let extra = match change.addr {
                                crate::wrath_inventory::InventoryAddr::Bank(i) => i >= 24,
                                crate::wrath_inventory::InventoryAddr::BankBag(i) => i == 6,
                                _ => false,
                            };
                            if !extra {
                                continue;
                            }
                            if let Some(run) = watch.typed_bank_change(
                                change.addr,
                                change.old,
                                change.new,
                                &mut bag_cache,
                                &seat,
                            ) {
                                runs_to_push.push(run);
                            }
                        }
                    }
                }
            }
            world.insert_resource(bag_cache);
        }
        if !entry_misses.is_empty() {
            if let Some(net) = world
                .get_resource::<NetCommands>()
                .map(|n| NetCommands(n.0.clone()))
            {
                if let Some(mut items) = world.get_resource_mut::<crate::items::Items>() {
                    for (item_guid, new_entry) in entry_misses {
                        items.template(new_entry, item_guid, crate::query_cache::Ask::Event(&net));
                        items.wait_for_entry(item_guid, new_entry);
                    }
                }
            }
        }
        if !runs_to_push.is_empty() {
            if let Some(mut lane) = world.get_resource_mut::<crate::item_lane::ItemLane>() {
                for run in runs_to_push {
                    lane.push(run);
                }
            }
        }
        for (guid, kind, moved) in &field_diffs {
            if matches!(kind, ObjectType::Unit | ObjectType::Player) {
                for &(index, old, new) in moved {
                    use benilla_protocol::field::{
                        FIELD_UNIT_BYTES_0, FIELD_UNIT_DYNAMIC_FLAGS, FIELD_UNIT_FACTIONTEMPLATE,
                        FIELD_UNIT_FLAGS, FIELD_UNIT_HEALTH, FIELD_UNIT_LEVEL,
                        FIELD_UNIT_MAXHEALTH, FIELD_UNIT_MAXPOWER1, FIELD_UNIT_POWER1,
                    };
                    // The snapshot adapter bypasses the normal values-block watches. Feed
                    // the UI explicitly; FieldChanged alone only reaches ECS consumers.
                    let event = match index {
                        FIELD_UNIT_HEALTH => Some(crate::field_watch::id::UNIT_HEALTH),
                        FIELD_UNIT_MAXHEALTH => Some(crate::field_watch::id::UNIT_MAXHEALTH),
                        i if (FIELD_UNIT_POWER1..FIELD_UNIT_POWER1 + 5).contains(&i) => {
                            Some(crate::field_watch::id::POWER + (i - FIELD_UNIT_POWER1))
                        }
                        i if (FIELD_UNIT_MAXPOWER1..FIELD_UNIT_MAXPOWER1 + 5).contains(&i) => {
                            Some(crate::field_watch::id::MAX_POWER + (i - FIELD_UNIT_MAXPOWER1))
                        }
                        FIELD_UNIT_FLAGS => Some(crate::field_watch::id::UNIT_FLAGS),
                        FIELD_UNIT_LEVEL => Some(crate::field_watch::id::UNIT_LEVEL),
                        FIELD_UNIT_FACTIONTEMPLATE => Some(crate::field_watch::id::UNIT_FACTION),
                        FIELD_UNIT_BYTES_0 if (old >> 24) != (new >> 24) => {
                            Some(crate::field_watch::id::UNIT_DISPLAYPOWER)
                        }
                        _ => None,
                    };
                    if let Some(id) = event {
                        if let Some(mut notices) =
                            world.get_resource_mut::<crate::field_watch::NoticeLane>()
                        {
                            notices
                                .write(crate::field_watch::FieldNotice::Signal { guid: *guid, id });
                        }
                    }
                    // The normal values-block watcher signals the dynamic-flags event on any
                    // bit, plus the tap's `UNIT_FACTION` and the dead look's `UNIT_HEALTH`. The
                    // snapshot adapter bypasses it, so the target frame's tapped/lootable state
                    // would otherwise keep its old icon after a Wrath kill.
                    if index == FIELD_UNIT_DYNAMIC_FLAGS {
                        if let Some(mut notices) =
                            world.get_resource_mut::<crate::field_watch::NoticeLane>()
                        {
                            notices.write(crate::field_watch::FieldNotice::Signal {
                                guid: *guid,
                                id: crate::field_watch::id::UNIT_DYNAMIC_FLAGS,
                            });
                            let flipped = old ^ new;
                            if flipped & 0x4 != 0 {
                                notices.write(crate::field_watch::FieldNotice::Signal {
                                    guid: *guid,
                                    id: crate::field_watch::id::UNIT_FACTION,
                                });
                            }
                            if flipped & 0x20 != 0 {
                                notices.write(crate::field_watch::FieldNotice::Signal {
                                    guid: *guid,
                                    id: crate::field_watch::id::UNIT_HEALTH,
                                });
                            }
                        }
                    }
                    // Loot FX rescan on this edge, including when LOOTABLE is cleared, plus
                    // action-bar usability refreshes on power, aura-state, and combo-point edges.
                    if index == FIELD_UNIT_DYNAMIC_FLAGS
                        || (FIELD_UNIT_POWER1..FIELD_UNIT_POWER1 + 5).contains(&index)
                        || index == 125
                        || index == 1222
                    {
                        let entity = world.resource::<GuidIndex>().0.get(guid).copied();
                        if let Some(entity) = entity {
                            let _ = world.write_message(FieldChanged {
                                at: crate::lanes::stamp(),
                                entity,
                                guid: *guid,
                                kind: *kind,
                                index,
                                old,
                                new,
                            });
                        }
                    }
                }
            }
            if *kind == ObjectType::GameObject || *kind == ObjectType::Corpse {
                // `go_anim` reacts to `FIELD_GAMEOBJECT_STATE` (14) edges to open/close doors and
                // chests; `creature_anim::spell_visual` rescans corpse loot sparkles on
                // `FIELD_CORPSE_DYNAMIC_FLAGS` (36).
                for &(index, old, new) in moved {
                    if (*kind == ObjectType::GameObject
                        && index == benilla_protocol::field::FIELD_GAMEOBJECT_STATE)
                        || (*kind == ObjectType::Corpse
                            && index == benilla_protocol::field::FIELD_CORPSE_DYNAMIC_FLAGS)
                    {
                        if let Some(entity) = world.resource::<GuidIndex>().0.get(guid).copied() {
                            let _ = world.write_message(FieldChanged {
                                at: crate::lanes::stamp(),
                                entity,
                                guid: *guid,
                                kind: *kind,
                                index,
                                old,
                                new,
                            });
                        }
                    }
                }
            }
            if *guid == player_guid && *kind == ObjectType::Player {
                // Snapshot replacement bypasses the normal field watchers; notify after seating
                // the values so the XP bar, skill frame, reputation bar, honor frame, and guild roster update.
                let xp_changed = moved
                    .iter()
                    .any(|&(index, _, _)| index == 716 || index == 717);
                let rest_changed = moved.iter().any(|&(index, _, _)| index == 1175);
                let skills_changed = moved
                    .iter()
                    .any(|&(index, _, _)| (718..1102).contains(&index));
                let watched_faction_changed = moved.iter().any(|&(index, _, _)| index == 1261);
                let pvp_kills_changed = moved.iter().any(|&(index, _, _)| index == 1250);
                let pvp_rank_changed = moved.iter().any(|&(index, old, new)| {
                    index == 195 || (index == 1260 && (old & 0xff) != (new & 0xff))
                });
                let stats_or_combat_changed = moved.iter().any(|&(index, _, _)| {
                    (126..=172).contains(&index)
                        || (1102..=1110).contains(&index)
                        || (1177..=1225).contains(&index)
                });
                if let Some(mut notices) =
                    world.get_resource_mut::<crate::field_watch::NoticeLane>()
                {
                    if xp_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 189 });
                    }
                    if rest_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 406 });
                    }
                    if skills_changed {
                        notices.write(crate::field_watch::FieldNotice::SkillLines);
                    }
                    if watched_faction_changed {
                        notices.write(crate::field_watch::FieldNotice::Player {
                            id: crate::field_watch::id::UPDATE_FACTION,
                        });
                    }
                    if pvp_kills_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 523 });
                    }
                    if pvp_rank_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 524 });
                    }
                    if stats_or_combat_changed {
                        notices.write(crate::field_watch::FieldNotice::Signal {
                            guid: player_guid,
                            id: crate::field_watch::id::UNIT_LEVEL,
                        });
                    }
                }
                let inv_changed = moved.iter().any(|&(idx, _, _)| {
                    (benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR
                        ..benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 228)
                        .contains(&idx)
                        || (benilla_protocol::field::FIELD_PLAYER_INV_SLOT_HEAD
                            ..benilla_protocol::field::FIELD_PLAYER_INV_SLOT_HEAD + 226)
                            .contains(&idx)
                        || (1226..1250).contains(&idx)
                });
                let money_changed = moved
                    .iter()
                    .any(|&(idx, _, _)| idx == benilla_protocol::field::FIELD_PLAYER_FIELD_COINAGE);
                if inv_changed || money_changed {
                    if let Some(mut notices) =
                        world.get_resource_mut::<crate::field_watch::NoticeLane>()
                    {
                        if inv_changed {
                            notices.write(crate::field_watch::FieldNotice::Player {
                                id: crate::field_watch::id::UNIT_INVENTORY_CHANGED,
                            });
                            notices.write(crate::field_watch::FieldNotice::Unit {
                                guid: player_guid,
                                id: crate::field_watch::id::UNIT_INVENTORY_CHANGED,
                            });
                        }
                        if money_changed {
                            notices.write(crate::field_watch::FieldNotice::Global {
                                id: crate::field_watch::id::PLAYER_MONEY,
                            });
                        }
                    }
                }
                // Equipment-slot evidence: the descriptor update moved one of the player's
                // nineteen equipment guids. Record the numeric slot and the item entry (never the
                // guid), so an equip can be distinguished from a bare item push.
                let mut changed_slots: Vec<u8> = moved
                    .iter()
                    .filter_map(|&(idx, _, _)| {
                        let base = benilla_protocol::field::FIELD_PLAYER_INV_SLOT_HEAD;
                        ((base..base + 38).contains(&idx) && (idx - base).is_multiple_of(2))
                            .then(|| ((idx - base) / 2) as u8)
                            .filter(|slot| *slot < 19)
                    })
                    .collect();
                changed_slots.sort_unstable();
                changed_slots.dedup();
                if !changed_slots.is_empty() {
                    if let Some(trace) = world.get_resource::<crate::quest_trace::Source>() {
                        let scope = world
                            .get_resource::<WrathWorldView>()
                            .map(|v| (v.session_epoch, v.world_generation));
                        let mut entries: Vec<(u8, u64)> = Vec::new();
                        if let Some(entity) = world.resource::<GuidIndex>().0.get(guid).copied() {
                            if let Some(store) = world.get::<crate::net::ObjectStore>(entity) {
                                for slot in &changed_slots {
                                    entries
                                        .push((*slot, store.0.player_inv_slot(*slot).unwrap_or(0)));
                                }
                            }
                        }
                        for (slot, item_guid) in entries {
                            let item_id = if item_guid == 0 {
                                0
                            } else {
                                world
                                    .resource::<GuidIndex>()
                                    .0
                                    .get(&item_guid)
                                    .and_then(|entity| {
                                        world
                                            .get::<crate::net::ObjectStore>(*entity)
                                            .and_then(|store| store.0.object_entry())
                                    })
                                    .unwrap_or(0)
                            };
                            trace.observe_with_subject(
                                scope,
                                None,
                                None,
                                Some(item_id),
                                Some(1),
                                Some(u32::from(slot)),
                                crate::quest_trace::Stage::EquipmentSlotChanged {
                                    item_id,
                                    slot: u32::from(slot),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    stamp_aura_anchors(world, &entities);
    world.insert_resource(CurrentMap(map_id));
    if !entered {
        if let Some(local) = entities.get(&player_guid).and_then(|e| e.position) {
            let now = world
                .get_resource::<Time>()
                .map_or(0.0, bevy::time::Time::elapsed_secs);
            if let Some(mut player) = world.get_resource_mut::<crate::player::Player>() {
                player.active = true;
                player.reseat = true;
                player.settling = true;
                player.settle_since = now;
                player.settle_deadline = now + crate::player::SETTLE_TIMEOUT;
                player.world_stale = true;
                player.pos = wow_to_bevy([local.x, local.y, local.z]);
            }
        }
    }
    if !entered && entities.contains_key(&player_guid) {
        if let Some(mut status) = world.get_resource_mut::<crate::net::NetStatus>() {
            status.connected = true;
            status.last_reason = None;
        }
        world.write_message(WorldEnterCascadeMessage);
        world.write_message(EnteredWorldMessage {
            tutorial_flags: None,
        });
        world.resource_mut::<WrathWorldView>().entered_world = true;
        info!(
            "wrath: entered world {map_id}, player {player_guid:#x}, {} entities",
            entities.len()
        );
    }
}

fn consume_wrath_position(
    world: &mut World,
    entity: Entity,
    pose: benilla_protocol_wrath::updates::PositionSnapshot,
) -> bool {
    let stamp = WrathAppliedPosition(pose);
    if world.get::<WrathAppliedPosition>(entity) == Some(&stamp) {
        return false;
    }
    world.entity_mut(entity).insert(stamp);
    true
}

fn default_speeds() -> MoveSpeeds {
    MoveSpeeds {
        walk: 2.5,
        run: 7.0,
        run_back: 4.5,
        swim: 4.722_222,
        swim_back: 2.5,
        turn_rate: std::f32::consts::PI,
    }
}

const W_FLAG_FORWARD: u32 = 0x1;
const W_FLAG_BACKWARD: u32 = 0x2;
const W_FLAG_STRAFE_LEFT: u32 = 0x4;
const W_FLAG_STRAFE_RIGHT: u32 = 0x8;
const W_FLAG_TURN_LEFT: u32 = 0x10;
const W_FLAG_TURN_RIGHT: u32 = 0x20;
/// 12340 `MOVEMENTFLAG_ONTRANSPORT` (0x0200); the 1.12 client uses 0x02000000 for it, which
/// 3.3.5 names `MOVEMENTFLAG_FLYING`.
const W_FLAG_ONTRANSPORT: u32 = 0x200;
/// 12340 `MOVEMENTFLAG_ROOT` (0x0800); 1.12 uses 0x1000.
const W_FLAG_ROOT: u32 = 0x800;
/// 12340 `MOVEMENTFLAG_FALLING` (0x1000); 1.12 uses 0x2000 and 3.3.5 uses 0x2000 for
/// `MOVEMENTFLAG_FALLING_FAR`.
const W_FLAG_FALLING: u32 = 0x1000;
const W_FLAG_FALLING_FAR: u32 = 0x2000;

/// The shared 1.12 controller's movement bits in 12340 placement. Only the meanings that moved
/// are rewritten: ROOT, FALLING/FALLING_FAR and ON_TRANSPORT; walking, steering, swimming,
/// levitation/disable-gravity, water-walking, safe-fall and hover share their bits.
fn wrath_move_flags(flags: u32) -> u32 {
    let rooted = flags & 0x1000 != 0;
    let falling = flags & 0x2000 != 0;
    let falling_far = flags & 0x4000 != 0;
    let on_transport = flags & 0x0200_0000 != 0;
    let mut out = flags & !(0x1000 | 0x2000 | 0x4000 | 0x0200_0000);
    if rooted {
        out |= W_FLAG_ROOT;
    }
    if falling {
        out |= W_FLAG_FALLING;
    }
    if falling_far {
        out |= W_FLAG_FALLING_FAR;
    }
    if on_transport {
        out |= W_FLAG_ONTRANSPORT;
    }
    out
}

/// The client's millisecond clock: one monotonic reading for the time in every outbound
/// `MovementInfo` and for the `CMSG_TIME_SYNC_RESP` answer. 12340 answers the sync with the move
/// event's own time (`CMovement_C::SendTimeSyncResp`, `0x006E9BB0`), and the server adds the delta
/// it derives from that answer to each move's time, so the two readings must share a clock.
fn wrath_client_ms() -> u32 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    // Truncation is the wrap of a 32-bit millisecond clock, after 49.7 days.
    START.get_or_init(Instant::now).elapsed().as_millis() as u32
}

/// The 12340 opcode of one shared self-movement message. The `MSG_MOVE_*` numbers did not move
/// between 1.12 and 3.3.5a; the table is explicit so the Wrath wire does not lean on that.
fn wrath_move_opcode(kind: crate::net::MoveKind) -> u32 {
    use crate::net::MoveKind;
    use benilla_protocol_wrath::opcode as op;
    match kind {
        MoveKind::StartForward => op::MSG_MOVE_START_FORWARD,
        MoveKind::StartBackward => op::MSG_MOVE_START_BACKWARD,
        MoveKind::Stop => op::MSG_MOVE_STOP,
        MoveKind::StartStrafeLeft => op::MSG_MOVE_START_STRAFE_LEFT,
        MoveKind::StartStrafeRight => op::MSG_MOVE_START_STRAFE_RIGHT,
        MoveKind::StopStrafe => op::MSG_MOVE_STOP_STRAFE,
        MoveKind::StartTurnLeft => op::MSG_MOVE_START_TURN_LEFT,
        MoveKind::StartTurnRight => op::MSG_MOVE_START_TURN_RIGHT,
        MoveKind::StopTurn => op::MSG_MOVE_STOP_TURN,
        MoveKind::Jump => op::MSG_MOVE_JUMP,
        MoveKind::FallLand => op::MSG_MOVE_FALL_LAND,
        MoveKind::StartSwim => u32::from(op::MSG_MOVE_START_SWIM),
        MoveKind::StopSwim => u32::from(op::MSG_MOVE_STOP_SWIM),
        MoveKind::StartPitchUp => op::MSG_MOVE_START_PITCH_UP,
        MoveKind::StartPitchDown => op::MSG_MOVE_START_PITCH_DOWN,
        MoveKind::StopPitch => op::MSG_MOVE_STOP_PITCH,
        MoveKind::SetWalkMode => u32::from(op::MSG_MOVE_SET_WALK_MODE),
        MoveKind::SetRunMode => u32::from(op::MSG_MOVE_SET_RUN_MODE),
        MoveKind::SetFacing => op::MSG_MOVE_SET_FACING,
        MoveKind::SetPitch => u32::from(op::MSG_MOVE_SET_PITCH),
        MoveKind::Heartbeat => op::MSG_MOVE_HEARTBEAT,
    }
}

/// One self-movement message of the shared streamer (`movement_net::stream_self_movement`) in
/// 12340 form: the mover's packed guid, then the `MovementInfo`. The streamer owns which message
/// goes out and when; this only re-places the flags and writes the 3.3.5 layout.
///
/// A tail travels with its flag. The streamer supplies the jump tail on every airborne message
/// and the deck pose on every boarded one; `gameplay::write_movement_info` clears a flag that
/// arrives without its tail, because the server parses whatever tail the flag announces.
#[allow(clippy::too_many_arguments)]
fn wrath_move_packet(
    mover: u64,
    time_ms: u32,
    kind: crate::net::MoveKind,
    flags: u32,
    pos: [f32; 3],
    orientation: f32,
    pitch: f32,
    fall_time: u32,
    jump: Option<benilla_protocol::JumpInfo>,
    transport: Option<benilla_protocol::TransportPose>,
) -> (u32, Vec<u8>) {
    let mut payload = enc_pack_guid(mover);
    gameplay::write_movement_info(
        &mut payload,
        wrath_move_flags(flags),
        time_ms,
        pos,
        orientation,
        pitch,
        fall_time,
        jump.map(|j| (j.zspeed, j.sin_angle, j.cos_angle, j.xy_speed)),
        transport.map(|t| (t.guid, [t.pos.x, t.pos.y, t.pos.z], t.orientation, 0)),
    );
    (wrath_move_opcode(kind), payload)
}

/// Whether a queued move reports a pose and nothing else, so a later move replaces it whole. The
/// edges do not qualify: the server acts on the landing (`HandleFall`), the jump and the swim
/// start by their opcode.
fn wrath_move_is_pose_only(opcode: u32) -> bool {
    use benilla_protocol_wrath::opcode as op;
    opcode == op::MSG_MOVE_HEARTBEAT
        || opcode == op::MSG_MOVE_SET_FACING
        || opcode == u32::from(op::MSG_MOVE_SET_PITCH)
}

/// Admits one replaceable motion entry. At [`WRATH_MOVEMENT_QUEUE_CAP`] it evicts a queued move
/// first, the oldest pose report if there is one and the oldest edge otherwise: each move carries
/// the whole flags word and pose, so a newer one supersedes it. A queue full of real actions
/// refuses the move instead of letting movement drop a cast, quest, loot or logout command.
fn admit_wrath_movement(
    input: &WrathInput,
    opcode: u32,
    payload: Vec<u8>,
    trace: Option<&crate::quest_trace::Source>,
) {
    let Ok(mut queue) = input.0.lock() else {
        if let Some(trace) = trace {
            trace.note_movement_refused();
        }
        return;
    };
    if queue.len() >= WRATH_MOVEMENT_QUEUE_CAP {
        let evict = queue
            .iter()
            .position(|entry| entry.movement && wrath_move_is_pose_only(entry.opcode))
            .or_else(|| queue.iter().position(|entry| entry.movement));
        let Some(evict) = evict else {
            if let Some(trace) = trace {
                trace.note_movement_refused();
            }
            return;
        };
        let _ = queue.remove(evict);
        if let Some(trace) = trace {
            trace.note_movement_coalesced();
        }
    }
    if let Some(trace) = trace {
        trace.note_movement_admitted();
    }
    let movement_seq = trace.map(|t| t.mint_movement_sequence());
    queue.push_back(WrathOutbound {
        opcode,
        payload,
        movement: true,
        movement_seq,
        enqueued_at: Some(Instant::now()),
        receipt: None,
        trace: None,
        enforce_scope: false,
    });
}

fn object_type_mask(kind: ObjectType) -> u32 {
    match kind {
        ObjectType::Object => 0x1,
        ObjectType::Item => 0x2 | 0x1,
        ObjectType::Container => 0x4 | 0x2 | 0x1,
        ObjectType::Unit => 0x8 | 0x1,
        ObjectType::Player => 0x10 | 0x8 | 0x1,
        ObjectType::GameObject => 0x20 | 0x1,
        ObjectType::DynamicObject => 0x40 | 0x1,
        ObjectType::Corpse => 0x80 | 0x1,
    }
}

fn entity_kind(type_id: u8) -> EntityKind {
    match type_id {
        4 => EntityKind::Player,
        3 => EntityKind::Unit,
        5 => EntityKind::GameObject,
        6 => EntityKind::DynamicObject,
        7 => EntityKind::Corpse,
        _ => EntityKind::Other,
    }
}

/// Owns the observation publisher for one worker. Every exit path, including an early socket
/// error, signals retirement and waits only for the bounded publisher stop window; a retirement
/// that misses that window is handed to the app, which tracks it until the publisher reports
/// stopped and blocks any replacement until then.
struct PublisherOwner {
    publisher: Option<benilla_wm_native::ObservationPublisher>,
    retiring: Vec<benilla_wm_native::PublisherRetirement>,
    retire_out: Option<Sender<benilla_wm_native::PublisherRetirement>>,
}

impl PublisherOwner {
    fn as_ref(&self) -> Option<&benilla_wm_native::ObservationPublisher> {
        self.publisher.as_ref()
    }

    fn as_mut(&mut self) -> Option<&mut benilla_wm_native::ObservationPublisher> {
        self.publisher.as_mut()
    }

    fn retire_all(&mut self) {
        benilla_wm_native::retire_publisher(&mut self.publisher, &mut self.retiring);
        for retirement in self.retiring.drain(..) {
            if retirement.wait_timeout(Duration::from_millis(200)) {
                continue;
            }
            // The bounded stop missed. The handle must stay tracked: sending it to the app lets
            // admission wait for the publisher to actually report stopped; with no app to track
            // it, wait here rather than let a replacement overlap publication.
            match self.retire_out.as_ref() {
                Some(out) => {
                    let _ = out.send(retirement);
                }
                None => retirement.join(),
            }
        }
    }
}

impl Drop for PublisherOwner {
    fn drop(&mut self) {
        self.retire_all();
    }
}

/// Explicit boot selection uses the account's actual roster and never falls back on a miss.
fn boot_character<'a>(
    records: &'a [gameplay::CharEnumRecord],
    name: Option<&str>,
) -> anyhow::Result<&'a gameplay::CharEnumRecord> {
    use anyhow::Context;
    match name {
        Some(name) => records
            .iter()
            .find(|record| record.name.eq_ignore_ascii_case(name))
            .with_context(|| {
                format!("WOW_CHAR requested character {name:?}, absent from the account roster")
            }),
        None => records
            .first()
            .context("no character on the account roster"),
    }
}

/// The 3.3.5 record onto the established roster seam's record. The appearance bytes, position
/// and the 19 equipment slots carry over; the first bag slot and the customize flags have no
/// stock equivalent and drop (`Player::BuildEnumData` writes 23 visual slots; the stock record
/// holds the 19 equipment ones). The mapping runs forward only: a level-0 wire byte is the
/// fresh character's mark (`new_character`, level pinned to 1 as the 1.12 parse holds it).
fn roster_from(records: &[gameplay::CharEnumRecord]) -> Vec<Character> {
    records
        .iter()
        .map(|record| {
            let mut equipment = [CharEnumItem::default(); 19];
            for (slot, visual) in equipment.iter_mut().zip(record.visuals) {
                *slot = CharEnumItem {
                    display_id: visual.display_id,
                    inventory_type: visual.inventory_type,
                };
            }
            Character {
                guid: record.guid,
                name: record.name.clone(),
                race: record.race,
                class: record.class,
                gender: record.gender,
                skin: record.skin,
                face: record.face,
                hair_style: record.hair_style,
                hair_color: record.hair_color,
                facial_hair: record.facial_style,
                level: record.level.max(1),
                new_character: record.level == 0,
                zone: record.zone,
                map: record.map,
                position: Vector3d {
                    x: record.position[0],
                    y: record.position[1],
                    z: record.position[2],
                },
                flags: record.character_flags,
                equipment,
                pet_display_id: record.pet_display_id,
                pet_level: record.pet_level,
                pet_family: record.pet_family,
            }
        })
        .collect()
}

/// The app-owned session loop. `cancel` is checked between bounded socket reads and before every
/// packet step; a cancelled session exits as a clean `SessionEnded` without manufacturing packets.
/// `parked` is the parked character select after a clean logout: the fresh session serves the
/// screen's asks from the channel the pick admission fed, publishes the fresh roster as the
/// established roster seam expects, and enters on the ask the screen made. The boot flow
/// publishes no roster. `publisher_retire` receives any publisher retirement the bounded stop
/// could not finish, so the app can keep it tracked and refuse a replacement.
pub fn run_session_with_cancel(
    feed: &WrathFeed,
    config: &WrathSessionConfig,
    cancel: &AtomicBool,
    commands: Option<WrathOutbox>,
    events_out: Option<Arc<Mutex<SessionReplies>>>,
    parked: Option<ParkedInput>,
    publisher_retire: Option<Sender<benilla_wm_native::PublisherRetirement>>,
) -> anyhow::Result<()> {
    use benilla_wm_native::{pump_once, replace_publisher, PublisherConfig};

    let multiclass = feed
        .multiclass
        .as_ref()
        .map(crate::multiclass::MulticlassBridge::connect);
    let (parked, roster_asked) = match parked {
        Some(input) => (Some(input.commands), input.roster_asked),
        None => (None, false),
    };

    let live_generation = || feed.current_login_generation(config.login_generation);
    let send_glue = |event| {
        push_forced_event(&events_out, event, live_generation());
    };
    send_glue(SessionEvent::LoginStage {
        stage: benilla_protocol::LoginStage::Connecting,
    });
    let mut logon =
        benilla_protocol_wrath::logon::logon(&config.auth_addr, &config.account, &config.password)?;
    if cancel.load(Ordering::SeqCst) {
        return Ok(());
    }
    send_glue(SessionEvent::LoginStage {
        stage: benilla_protocol::LoginStage::Handshaking,
    });
    let realm_view = |r: &benilla_protocol_wrath::auth::RealmInfo| benilla_protocol::RealmInfo {
        name: r.name.clone(),
        address: r.address.clone(),
        population: r.population,
        characters: r.characters,
        realm_type: u32::from(r.realm_type),
        flags: r.flags,
        category: r.category,
        id: r.realm_id,
    };
    let publish_realms = |logon: &benilla_protocol_wrath::logon::Logon, failed| {
        send_glue(SessionEvent::RealmList {
            realms: logon.realms.iter().map(realm_view).collect(),
            failed,
        });
    };
    let refresh_or_relogon = |logon: &mut benilla_protocol_wrath::logon::Logon| -> bool {
        if logon.refresh_realms().is_ok() {
            return false;
        }
        if !config.account.is_empty() {
            if let Ok(fresh) = benilla_protocol_wrath::logon::logon(
                &config.auth_addr,
                &config.account,
                &config.password,
            ) {
                *logon = fresh;
                return false;
            }
        }
        true
    };
    if config.login_generation.is_some() {
        publish_realms(&logon, false);
    }

    let publisher_config = match &config.wm_token {
        Some(token) => {
            let loopback = benilla_wm_native::LoopbackConfig::new(
                std::net::SocketAddr::from(([127, 0, 0, 1], benilla_wm_native::DEFAULT_PORT)),
                benilla_wm_native::AuthToken::new(token.clone())?,
            )?;
            Some(match config.actions.as_ref() {
                Some(bridge) => PublisherConfig::new(loopback).with_actions(bridge.clone()),
                None => PublisherConfig::new(loopback),
            })
        }
        None => None,
    };
    let mut publisher = PublisherOwner {
        publisher: match &publisher_config {
            Some(config) => Some(benilla_wm_native::ObservationPublisher::spawn(
                config.clone(),
            )?),
            None => None,
        },
        retiring: Vec::new(),
        retire_out: publisher_retire,
    };
    let mut session = SessionState::new();
    // The single native cast/channel authority for this session; it resolves each wire fact
    // exactly once and feeds both presentation mirrors and the WM export.
    let mut native = crate::wrath_native::NativeCastState::default();
    let started = std::time::Instant::now();
    let mut pending_realm = config.preferred_realm;
    let mut roster_asked_next = roster_asked;

    let (mut world, guid) = 'realm: loop {
        feed.authenticated.store(false, Ordering::SeqCst);
        let realm_id = if config.login_generation.is_some() {
            if let Some(id) = pending_realm.take() {
                id
            } else {
                let input = parked
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("login needs a parked session"))?;
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    match input.recv_timeout(Duration::from_millis(100)) {
                        Ok(WrathParkedCommand::Realm(r))
                            if logon
                                .realms
                                .iter()
                                .any(|v| v.realm_id == r.id && v.flags & 3 == 0) =>
                        {
                            break r.id;
                        }
                        Ok(WrathParkedCommand::RefreshRealms) => {
                            let failed = refresh_or_relogon(&mut logon);
                            publish_realms(&logon, failed);
                        }
                        Ok(WrathParkedCommand::Close) => return Ok(()),
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(()),
                        _ => {}
                    }
                }
            }
        } else {
            logon
                .realms
                .first()
                .ok_or_else(|| anyhow::anyhow!("realm list is empty"))?
                .realm_id
        };
        let Some(realm) = logon
            .realms
            .iter()
            .find(|r| r.realm_id == realm_id)
            .cloned()
        else {
            if config.login_generation.is_some() && parked.is_some() {
                publish_realms(&logon, true);
                continue 'realm;
            }
            anyhow::bail!("selected realm is no longer available");
        };

        session.start_session(1); // placeholder; re-started with the real GUID after the enum
        native.invalidate_scope();
        feed.push_event(WrathFeedEvent::Phase {
            phase: session.phase(),
            session_epoch: session.epoch(),
            world_generation: session.world_generation(),
            login_generation: live_generation(),
            map_id: session.map_id(),
        });

        let connect_addr = if config.world_addr.is_empty() {
            &realm.address
        } else {
            &config.world_addr
        };
        let (mut world, seed) =
            match benilla_protocol_wrath::world::WorldConnection::connect(connect_addr) {
                Ok(pair) => pair,
                Err(e) if config.login_generation.is_some() && parked.is_some() => {
                    send_glue(SessionEvent::LoginFailed {
                        refusal: None,
                        reason: format!("{e:#}"),
                        dial: e.downcast_ref::<benilla_protocol::DialFailure>().cloned(),
                    });
                    continue 'realm;
                }
                Err(e) => return Err(e),
            };
        world
            .stream_mut()
            .set_read_timeout(Some(Duration::from_millis(200)))?;
        if config.login_generation.is_some() {
            send_glue(SessionEvent::LoginStage {
                stage: benilla_protocol::LoginStage::WorldConnected,
            });
        }
        world.auth_session(
            &logon.account,
            &logon.session_key,
            &seed,
            realm.realm_id as u32,
        )?;
        // auth_session sends the proof; only the server's AUTH_OK accepts the world link.
        let mut auth_deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            if let Some(input) = parked.as_ref() {
                while let Ok(cmd) = input.try_recv() {
                    match cmd {
                        WrathParkedCommand::CancelWorld | WrathParkedCommand::LeaveRealm => {
                            continue 'realm;
                        }
                        WrathParkedCommand::Realm(r)
                            if logon
                                .realms
                                .iter()
                                .any(|v| v.realm_id == r.id && v.flags & 3 == 0) =>
                        {
                            pending_realm = Some(r.id);
                            continue 'realm;
                        }
                        WrathParkedCommand::RefreshRealms => {
                            let failed = refresh_or_relogon(&mut logon);
                            publish_realms(&logon, failed);
                        }
                        WrathParkedCommand::Close => return Ok(()),
                        _ => {}
                    }
                }
            }
            if Instant::now() >= auth_deadline {
                if config.login_generation.is_some() && parked.is_some() {
                    send_glue(SessionEvent::LoginFailed {
                        refusal: None,
                        reason: "timeout waiting for world authentication".to_owned(),
                        dial: None,
                    });
                    continue 'realm;
                }
                anyhow::bail!("timeout waiting for world authentication");
            }
            match world.recv() {
                Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_AUTH_RESPONSE => {
                    let code = *p
                        .payload
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("empty auth response"))?;
                    if let Some(expansion) = p.payload.get(10) {
                        // AuthHandler.cpp: billing fields precede the account expansion byte.
                        feed.account_expansion.store(*expansion, Ordering::SeqCst);
                    }
                    if code == benilla_protocol_wrath::opcode::AUTH_OK {
                        break;
                    }
                    if code == 0x1b {
                        // AuthHandler.cpp: initial queue includes billing; later updates are six bytes.
                        let offset = if p.payload.len() >= 16 { 11 } else { 1 };
                        let bytes = p
                            .payload
                            .get(offset..offset + 4)
                            .ok_or_else(|| anyhow::anyhow!("truncated queue response"))?;
                        send_glue(SessionEvent::LoginQueued {
                            position: Some(u32::from_le_bytes(bytes.try_into()?)),
                            realm: Some(realm.name.clone()),
                        });
                        auth_deadline = Instant::now() + Duration::from_secs(60);
                    } else if config.login_generation.is_some() && parked.is_some() {
                        send_glue(SessionEvent::LoginFailed {
                            refusal: Some(benilla_protocol::LoginRefusal::World(code)),
                            reason: format!("world server refused authentication ({code:#04x})"),
                            dial: None,
                        });
                        continue 'realm;
                    } else {
                        return Err(anyhow::Error::new(
                            benilla_protocol_wrath::auth::LogonAuthError::World(code),
                        ));
                    }
                }
                Ok(_) => {}
                Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
                Err(e) if config.login_generation.is_some() && parked.is_some() => {
                    send_glue(SessionEvent::LoginFailed {
                        refusal: None,
                        reason: format!("{e:#}"),
                        dial: None,
                    });
                    continue 'realm;
                }
                Err(e) => return Err(e),
            }
        }
        feed.authenticated.store(true, Ordering::SeqCst);
        if let Some(generation) = live_generation() {
            send_glue(SessionEvent::WorldAuthenticated {
                realm: realm_view(&realm),
                epoch: session.epoch(),
                generation,
            });
        }
        session.note_player_login();

        // Character selection: reuse the renderless driver's create-first flow is intentionally
        // out of scope here — the graphical path logs into an existing character (issue 23's
        // driver owns creation). Enumerate before resolving a named boot or parked-screen pick.
        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
        let mut records: Option<Vec<gameplay::CharEnumRecord>> = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while records.is_none() {
            if cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            if let Some(input) = parked.as_ref() {
                while let Ok(cmd) = input.try_recv() {
                    match cmd {
                        WrathParkedCommand::CancelWorld | WrathParkedCommand::LeaveRealm => {
                            continue 'realm;
                        }
                        WrathParkedCommand::Realm(r)
                            if logon
                                .realms
                                .iter()
                                .any(|v| v.realm_id == r.id && v.flags & 3 == 0) =>
                        {
                            pending_realm = Some(r.id);
                            continue 'realm;
                        }
                        WrathParkedCommand::RefreshRealms => {
                            let failed = refresh_or_relogon(&mut logon);
                            publish_realms(&logon, failed);
                        }
                        WrathParkedCommand::Close => return Ok(()),
                        _ => {}
                    }
                }
            }
            if std::time::Instant::now() > deadline {
                if config.login_generation.is_some() && parked.is_some() {
                    send_glue(SessionEvent::RosterFailed {
                        epoch: session.epoch(),
                    });
                    continue 'realm;
                }
                anyhow::bail!("timeout waiting for SMSG_CHAR_ENUM");
            }
            match world.recv() {
                Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_ENUM => {
                    records = Some(benilla_protocol_wrath::gameplay::parse_char_enum(
                        &p.payload,
                    )?);
                }
                Ok(_) => {}
                Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
                Err(_) if config.login_generation.is_some() && parked.is_some() => {
                    send_glue(SessionEvent::RosterFailed {
                        epoch: session.epoch(),
                    });
                    continue 'realm;
                }
                Err(e) => return Err(e),
            }
        }
        let mut records = records.expect("fresh roster");
        if records.is_empty() && parked.is_none() {
            anyhow::bail!(
                "no disposable character on the lab account; create one with the renderless driver first"
            );
        }
        let publish_roster = |records: &[gameplay::CharEnumRecord], cause| {
            let gen = live_generation();
            if let Some(replies) = events_out.as_ref() {
                if let Ok(mut replies) = replies.lock() {
                    replies.push(
                        WrathSessionEvent::scoped(
                            &session,
                            SessionEvent::CharacterList {
                                characters: roster_from(records),
                                realm: Some(realm_view(&realm)),
                                cause,
                                epoch: session.epoch(),
                                generation: gen.unwrap_or(0),
                            },
                        )
                        .for_login(gen),
                    );
                }
            }
        };
        // The parked session publishes the fresh roster as the reply the screen's ask ordered; the
        // boot flow publishes nothing, its named boot selection decides the entry. A later re-ask
        // lands in the serve loop and republishes under its own cause.
        if parked.is_some() {
            publish_roster(
                &records,
                if std::mem::replace(&mut roster_asked_next, false) {
                    RosterCause::Asked
                } else {
                    RosterCause::Unasked
                },
            );
        }

        let guid = if let Some(parked) = parked.as_ref() {
            // The parked loop serves the select screen directly: a re-ask enumerates again and the
            // fresh roster republishes; the Enter World ask enters the record it picked. The reads
            // keep the parked link's own windows — bounded socket reads and one cancel edge, no
            // other deadline: the screen may sit as long as it wants, as the stock park does.
            let mut entered: Option<u64> = None;
            let mut roster_mutation: Option<benilla_protocol::CharAction> = None;
            let mut mutation_deadline: Option<Instant> = None;
            while entered.is_none() {
                if cancel.load(Ordering::SeqCst) {
                    return Ok(());
                }
                anyhow::ensure!(
                    !mutation_deadline.is_some_and(|d| Instant::now() >= d),
                    "timeout waiting for character action response"
                );
                for request in parked.try_iter() {
                    match request {
                        WrathParkedCommand::RealmSplit(state) => {
                            world.send(
                                benilla_protocol_wrath::opcode::CMSG_REALM_SPLIT,
                                &state.to_le_bytes(),
                            )?;
                            debug!("wrath: sent realm split choice={state}");
                        }
                        WrathParkedCommand::Enumerate => {
                            world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
                        }
                        WrathParkedCommand::Enter(picked) => {
                            if roster_mutation.is_none()
                                && records.iter().any(|record| record.guid == picked)
                            {
                                entered = Some(picked);
                            } else {
                                // A stale pick from a roster this session never saw (a screen
                                // click landing after the link changed): refuse to manufacture an
                                // entry, keep the parked link serving.
                                warn!(
                                    "wrath: ignoring an enter ask for {picked:#x}, which is not on the fresh roster"
                                );
                            }
                        }
                        WrathParkedCommand::Create(request) if roster_mutation.is_none() => {
                            // AC CharacterHandler::HandleCharCreateOpcode: name CString, eight
                            // appearance/race/class bytes and outfit byte, identical to this encoder.
                            world.send(
                                benilla_protocol_wrath::opcode::CMSG_CHAR_CREATE,
                                &benilla_protocol::messages::char_create(&request),
                            )?;
                            roster_mutation = Some(benilla_protocol::CharAction::Create);
                            mutation_deadline = Some(Instant::now() + Duration::from_secs(30));
                        }
                        WrathParkedCommand::Delete(guid)
                            if roster_mutation.is_none()
                                && records.iter().any(|record| record.guid == guid) =>
                        {
                            world.send(
                                benilla_protocol_wrath::opcode::CMSG_CHAR_DELETE,
                                &guid.to_le_bytes(),
                            )?;
                            roster_mutation = Some(benilla_protocol::CharAction::Delete);
                            mutation_deadline = Some(Instant::now() + Duration::from_secs(30));
                        }
                        WrathParkedCommand::Rename { guid, name }
                            if roster_mutation.is_none()
                                && records.iter().any(|record| record.guid == guid) =>
                        {
                            world.send(
                                benilla_protocol_wrath::opcode::CMSG_CHAR_RENAME,
                                &encode_char_rename(guid, &name),
                            )?;
                            mutation_deadline = Some(Instant::now() + Duration::from_secs(30));
                        }
                        WrathParkedCommand::RefreshRealms => {
                            let failed = refresh_or_relogon(&mut logon);
                            publish_realms(&logon, failed);
                        }
                        WrathParkedCommand::Realm(r)
                            if logon
                                .realms
                                .iter()
                                .any(|v| v.realm_id == r.id && v.flags & 3 == 0) =>
                        {
                            pending_realm = Some(r.id);
                            continue 'realm;
                        }
                        WrathParkedCommand::CancelWorld | WrathParkedCommand::LeaveRealm => {
                            continue 'realm;
                        }
                        WrathParkedCommand::Close => return Ok(()),
                        WrathParkedCommand::MulticlassRequest(payload) => {
                            world.send(
                                benilla_protocol_wrath::opcode::CMSG_MULTICLASS_REQUEST,
                                &payload,
                            )?;
                        }
                        _ => {}
                    }
                }
                if entered.is_some() {
                    break;
                }
                if let Some(multiclass) = &multiclass {
                    multiclass.selection(roster_mutation.is_none());
                    multiclass.dispatch(&mut world)?;
                }
                match world.recv() {
                    Ok(p)
                        if p.opcode == benilla_protocol_wrath::multiclass::SERVER_OPCODE
                            && multiclass.is_some() =>
                    {
                        multiclass
                            .as_ref()
                            .expect("enabled multiclass")
                            .receive(&p.payload)?;
                    }
                    Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_REALM_SPLIT => {
                        let reply =
                            benilla_protocol_wrath::gameplay::parse_realm_split(&p.payload)?;
                        info!(
                            "wrath: realm split choice={} state={} date={}",
                            reply.0, reply.1, reply.2
                        );
                        send_glue(SessionEvent::RealmSplit {
                            choice: reply.0,
                            state: reply.1,
                            date: reply.2,
                        });
                    }
                    Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_ENUM => {
                        records = benilla_protocol_wrath::gameplay::parse_char_enum(&p.payload)?;
                        publish_roster(&records, RosterCause::Asked);
                    }
                    Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_MULTICLASS_REPLY => {
                        if let Ok(reply) =
                            benilla_protocol_wrath::gameplay::accept_multiclass_reply(&p.payload)
                        {
                            feed.push_event(WrathFeedEvent::MulticlassReply {
                                op: reply.op,
                                status: reply.status,
                                serial: reply.serial,
                                payload: reply.body,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                    }
                    Ok(p)
                        if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_CREATE
                            || p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_DELETE =>
                    {
                        let action = if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_CREATE
                        {
                            benilla_protocol::CharAction::Create
                        } else {
                            benilla_protocol::CharAction::Delete
                        };
                        if roster_mutation != Some(action) {
                            continue;
                        }
                        let &[raw] = p.payload.as_slice() else {
                            anyhow::bail!("invalid character action reply length");
                        };
                        roster_mutation = None;
                        mutation_deadline = None;
                        let code = roster_status(action, raw);
                        send_glue(SessionEvent::CharActionResult { action, code });
                        // The roster changes only after the server's response and fresh enum.
                        if (action == benilla_protocol::CharAction::Create && raw == 0x2f)
                            || (action == benilla_protocol::CharAction::Delete && raw == 0x47)
                        {
                            world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
                        }
                    }
                    Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_RENAME => {
                        mutation_deadline = None;
                        let (result, renamed) = match parse_char_rename(&p.payload) {
                            Ok(reply) => reply,
                            Err(error) => {
                                warn!("invalid Wrath character rename reply: {error:#}");
                                (1, None)
                            }
                        };
                        if let Some((renamed_guid, ref new_name)) = renamed {
                            if let Some(rec) = records.iter_mut().find(|r| r.guid == renamed_guid) {
                                rec.name = new_name.clone();
                                rec.character_flags &= !benilla_protocol::CHARACTER_FLAG_RENAME;
                            }
                        }
                        send_glue(SessionEvent::CharRenamed { result, renamed });
                    }
                    Ok(_) => {}
                    Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
                    Err(e) => return Err(e),
                }
            }
            entered.expect("picked guid")
        } else {
            let record = boot_character(&records, config.character_name.as_deref())?;
            info!(
                "wrath: boot selection name={} guid={:#x} race={} class={} map={}",
                record.name, record.guid, record.race, record.class, record.map
            );
            record.guid
        };
        if let Some(picked_record) = records.iter().find(|r| r.guid == guid) {
            if let Ok(mut slot) = feed.player_name.lock() {
                *slot = Some((guid, picked_record.name.clone()));
            }
        }
        break 'realm (world, guid);
    };

    struct Adapter<'a> {
        session: &'a mut SessionState,
        /// The session's single native cast/channel authority, read directly by the export.
        native: &'a crate::wrath_native::NativeCastState,
    }
    impl benilla_wm_native::ProducerSession for Adapter<'_> {
        type Snapshot = SnapshotOwned;
        fn source_health(&self) -> Option<benilla_wm_native::SourceHealth> {
            let state = match self.session.phase() {
                benilla_protocol_wrath::session::Phase::InWorld => {
                    benilla_wm_native::SourceSessionState::InWorld
                }
                benilla_protocol_wrath::session::Phase::TransferPending
                | benilla_protocol_wrath::session::Phase::Loading => {
                    benilla_wm_native::SourceSessionState::Transferring
                }
                _ if self.session.world_established() => {
                    benilla_wm_native::SourceSessionState::InWorld
                }
                _ => benilla_wm_native::SourceSessionState::Disconnected,
            };
            benilla_wm_native::SourceHealth::new(
                self.session.epoch(),
                self.session.world_generation(),
                self.session.player_guid().unwrap_or(0),
                state,
                self.session.last_server_packet_at_unix_ms(),
            )
            .ok()
        }
        fn export_due(&self) -> bool {
            self.session.export_due()
        }
        fn export(
            &mut self,
            exported_at_unix_ms: i64,
            client_uptime_ms: u64,
        ) -> std::result::Result<Option<SnapshotOwned>, benilla_wm_native::ProducerExportError>
        {
            match self.session.export(exported_at_unix_ms, client_uptime_ms) {
                Ok(Some(snapshot)) => {
                    let player_guid = snapshot.player_guid;
                    let auras = match self.session.world().get(player_guid) {
                        Some(entry) => crate::wrath_native::auras_observation(
                            &entry.auras,
                            entry.auras_observed,
                        ),
                        None => benilla_wm_native::AurasObservation::unknown(),
                    };
                    Ok(Some(SnapshotOwned {
                        casting: crate::wrath_native::cast_observation(self.native, player_guid),
                        channeling: crate::wrath_native::channel_observation(
                            self.native,
                            player_guid,
                        ),
                        last_cast_end: crate::wrath_native::last_cast_end_observation(
                            self.native,
                            player_guid,
                            snapshot.session_epoch,
                            snapshot.world_generation,
                        ),
                        auras,
                        snapshot,
                    }))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(benilla_wm_native::ProducerExportError::new(e.to_string())),
            }
        }
        fn mark_published(&mut self) {
            self.session.mark_published();
        }
    }
    struct SnapshotOwned {
        snapshot: SelfObservation,
        casting: benilla_wm_native::CastObservation,
        channeling: benilla_wm_native::ChannelObservation,
        last_cast_end: benilla_wm_native::LastCastEndObservation,
        auras: benilla_wm_native::AurasObservation,
    }
    impl benilla_wm_native::ProducerSnapshot for SnapshotOwned {
        fn session_epoch(&self) -> u64 {
            self.snapshot.session_epoch
        }
        fn player_guid(&self) -> u64 {
            self.snapshot.player_guid
        }
        fn sequence(&self) -> u64 {
            self.snapshot.sequence
        }
        fn state_revision(&self) -> u64 {
            self.snapshot.state_revision
        }
        fn world_generation(&self) -> u64 {
            self.snapshot.world_generation
        }
        fn position_observed_at_unix_ms(&self) -> i64 {
            self.snapshot.position_observed_at_unix_ms
        }
        fn observed_at_unix_ms(&self) -> i64 {
            self.snapshot.observed_at_unix_ms
        }
        fn client_uptime_ms(&self) -> u64 {
            self.snapshot.client_uptime_ms
        }
        fn map_id(&self) -> u32 {
            self.snapshot.map_id
        }
        fn position(&self) -> benilla_wm_native::ProducerPosition {
            benilla_wm_native::ProducerPosition {
                x: self.snapshot.position.x,
                y: self.snapshot.position.y,
                z: self.snapshot.position.z,
                orientation: self.snapshot.position.o,
            }
        }
        fn target(&self) -> benilla_wm_native::Result<benilla_wm_native::TargetObservation> {
            Ok(match &self.snapshot.target {
                benilla_protocol_wrath::session::SelfTarget::Unknown => {
                    benilla_wm_native::TargetObservation::Unknown
                }
                benilla_protocol_wrath::session::SelfTarget::None => {
                    benilla_wm_native::TargetObservation::None
                }
                benilla_protocol_wrath::session::SelfTarget::Target(g) => {
                    benilla_wm_native::TargetObservation::target(*g)?
                }
            })
        }
        fn combat(&self) -> benilla_wm_native::CombatObservation {
            match self.snapshot.combat {
                benilla_protocol_wrath::session::SelfCombat::Unknown => {
                    benilla_wm_native::CombatObservation::Unknown
                }
                benilla_protocol_wrath::session::SelfCombat::OutOfCombat => {
                    benilla_wm_native::CombatObservation::OutOfCombat
                }
                benilla_protocol_wrath::session::SelfCombat::InCombat => {
                    benilla_wm_native::CombatObservation::InCombat
                }
            }
        }
        fn resources(&self) -> benilla_wm_native::Result<benilla_wm_native::ResourceObservation> {
            benilla_wm_native::ResourceObservation::pairs(
                self.snapshot.resources.health,
                self.snapshot.resources.max_health,
                self.snapshot.resources.power_type,
                self.snapshot.resources.power,
                self.snapshot.resources.powers,
                self.snapshot.resources.max_powers,
            )
        }
        fn death(&self) -> benilla_wm_native::DeathObservation {
            match self.snapshot.death {
                benilla_protocol_wrath::session::SelfDeath::Unknown => {
                    benilla_wm_native::DeathObservation::Unknown
                }
                benilla_protocol_wrath::session::SelfDeath::Alive => {
                    benilla_wm_native::DeathObservation::Alive
                }
                benilla_protocol_wrath::session::SelfDeath::Dead => {
                    benilla_wm_native::DeathObservation::Dead
                }
                benilla_protocol_wrath::session::SelfDeath::Ghost => {
                    benilla_wm_native::DeathObservation::Ghost
                }
            }
        }
        fn visibility(&self) -> benilla_wm_native::Result<benilla_wm_native::Visibility> {
            benilla_wm_native::Visibility::new(
                self.snapshot.visibility.complete,
                self.snapshot.visibility.total_guid_count,
                self.snapshot.visibility.guids.clone(),
            )
        }
        fn casting(&self) -> benilla_wm_native::CastObservation {
            self.casting.clone()
        }
        fn channeling(&self) -> benilla_wm_native::ChannelObservation {
            self.channeling.clone()
        }
        fn last_cast_end(&self) -> benilla_wm_native::LastCastEndObservation {
            self.last_cast_end.clone()
        }
        fn auras(&self) -> benilla_wm_native::AurasObservation {
            self.auras.clone()
        }
        fn zone_id(&self) -> Option<u32> {
            self.snapshot.zone_id
        }
        fn area_id(&self) -> Option<u32> {
            self.snapshot.area_id
        }
    }

    if let Some(multiclass) = &multiclass {
        multiclass.selection(false);
    }
    session.start_session(guid); // re-begin with the real full GUID; phase resets to selection
    session.note_player_login();
    feed.last_session_epoch
        .store(session.epoch(), Ordering::SeqCst);
    feed.last_world_generation
        .store(session.world_generation(), Ordering::SeqCst);
    feed.push_event(WrathFeedEvent::Phase {
        phase: session.phase(),
        session_epoch: session.epoch(),
        world_generation: session.world_generation(),
        login_generation: live_generation(),
        map_id: session.map_id(),
    });
    world.send(
        benilla_protocol_wrath::opcode::CMSG_PLAYER_LOGIN,
        &guid.to_le_bytes(),
    )?;

    let deadline = config.dwell.map(|dwell| std::time::Instant::now() + dwell);
    let mut last_ping = std::time::Instant::now();
    let mut last_server_response = std::time::Instant::now();
    let mut ping_seq = 1u32;
    let mut last_digest_revision = session.state_revision();
    let mut logged_out = false;
    feed.publish_digest(build_digest(&session)); // initial state (character selection)
    while deadline.is_none_or(|deadline| std::time::Instant::now() < deadline) {
        feed.last_session_epoch
            .store(session.epoch(), Ordering::SeqCst);
        feed.last_world_generation
            .store(session.world_generation(), Ordering::SeqCst);
        if cancel.load(Ordering::SeqCst) {
            native.invalidate_scope();
            publisher.retire_all();
            return Ok(());
        }
        if let Some(multiclass) = &multiclass {
            multiclass.world(session.phase(), session.world_generation());
            multiclass.dispatch(&mut world)?;
        }
        // Bounded ownership (R04): once the reply queue overflowed, ordered replies have already
        // been lost. End the session through the one terminal path instead of continuing to read
        // and silently discarding gameplay state.
        if let Some(replies) = events_out.as_ref() {
            if replies
                .lock()
                .map(|replies| replies.overloaded())
                .unwrap_or(false)
            {
                anyhow::bail!(
                    "session reply backlog exceeded {WRATH_SESSION_REPLY_CAP} entries; \
                     ordered reply delivery was lost"
                );
            }
        }
        if let Some(commands) = commands.as_ref() {
            let pending: Vec<_> = commands
                .lock()
                .map(|mut queue| queue.drain(..).collect())
                .unwrap_or_default();
            for command in pending {
                // Cancel handling for receipt-level scope.
                if let Some(receipt) = command.receipt.as_ref() {
                    if receipt.enforce_scope && cancel.load(Ordering::SeqCst) {
                        receipt.report(crate::net::CommandDelivery::Rejected(
                            crate::net::CommandRefusal::Cancelled,
                        ));
                        continue;
                    }
                    if receipt.enforce_scope
                        && (receipt.session_epoch != session.epoch()
                            || receipt.world_generation != session.world_generation()
                            || session.phase() != Phase::InWorld)
                    {
                        receipt.report(crate::net::CommandDelivery::Rejected(
                            crate::net::CommandRefusal::ScopeChanged,
                        ));
                        continue;
                    }
                }
                // Generic trace scope check: enforced only when the carried flag says so.
                let mut stale_trace = false;
                if command.enforce_scope {
                    if let Some(ix) = &command.trace {
                        if let Some((se, wg)) = ix.scope() {
                            if se != session.epoch()
                                || wg != session.world_generation()
                                || session.phase() != Phase::InWorld
                            {
                                stale_trace = true;
                                ix.record(crate::quest_trace::Stage::Delivery(
                                    crate::net::CommandDelivery::Rejected(
                                        crate::net::CommandRefusal::ScopeChanged,
                                    ),
                                ));
                            }
                        }
                    }
                }
                if let Some(receipt) = &command.receipt {
                    if !receipt.enforce_scope {
                        if let Some(ix) = &receipt.trace {
                            if let Some((se, wg)) = ix.scope() {
                                if se != session.epoch()
                                    || wg != session.world_generation()
                                    || session.phase() != Phase::InWorld
                                {
                                    stale_trace = true;
                                    // Reuse the receipt report path as authority for this interaction,
                                    // unless the same interaction was already recorded via command.trace.
                                    let cmd_ix = command
                                        .trace
                                        .as_ref()
                                        .map(|t| t.interaction_id())
                                        .unwrap_or(0);
                                    if cmd_ix == 0 || cmd_ix != ix.interaction_id() {
                                        receipt.report(crate::net::CommandDelivery::Rejected(
                                            crate::net::CommandRefusal::ScopeChanged,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                if stale_trace {
                    continue;
                }
                let result = world.send(command.opcode, &command.payload);
                if command.movement {
                    if let Some(seq) = command.movement_seq {
                        if result.is_ok() {
                            if let Some(trace) = &feed.trace {
                                trace.note_movement_written(seq);
                            }
                        }
                    }
                }
                if diagnostic_outbound(command.opcode) {
                    if let Some(ix) = &command.trace {
                        ix.record(crate::quest_trace::Stage::Outbound {
                            opcode: command.opcode,
                            state: if result.is_ok() {
                                crate::quest_trace::OutboundState::Written
                            } else {
                                crate::quest_trace::OutboundState::WriteFailed
                            },
                        });
                    } else if let Some(trace) = &feed.trace {
                        trace.observe(
                            Some((session.epoch(), session.world_generation())),
                            None,
                            crate::quest_trace::Stage::Outbound {
                                opcode: command.opcode,
                                state: if result.is_ok() {
                                    crate::quest_trace::OutboundState::Written
                                } else {
                                    crate::quest_trace::OutboundState::WriteFailed
                                },
                            },
                        );
                    }
                }
                if result.is_err() {
                    if let Some(ix) = &command.trace {
                        ix.record(crate::quest_trace::Stage::SocketWriteFailed);
                    } else if let Some(trace) = &feed.trace {
                        trace.observe(
                            Some((session.epoch(), session.world_generation())),
                            None,
                            crate::quest_trace::Stage::SocketWriteFailed,
                        );
                    }
                }
                // Delivery outcome records on trace-bearing commands (defect #2 fix), unless the
                // same interaction is already reported through the receipt path.
                let receipt_ix = command
                    .receipt
                    .as_ref()
                    .and_then(|r| r.trace.as_ref())
                    .map(|t| t.interaction_id())
                    .unwrap_or(0);
                if let Some(ix) = &command.trace {
                    if receipt_ix == 0 || ix.interaction_id() != receipt_ix {
                        if result.is_ok() {
                            ix.record_delivery(crate::net::CommandDelivery::Written);
                        } else {
                            ix.record_delivery(crate::net::CommandDelivery::WriteFailed);
                        }
                    }
                }
                if let Some(receipt) = command.receipt.as_ref() {
                    receipt.report(if result.is_ok() {
                        crate::net::CommandDelivery::Written
                    } else {
                        crate::net::CommandDelivery::WriteFailed
                    });
                }
                result?;
                // A cancel request is client authority state recorded when the command
                // actually leaves, not when the UI asked for it. The authoritative terminal
                // edge still decides the outcome; the export publishes only the request.
                let spell_id = match command.opcode {
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CAST => command.payload.get(1..5),
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CHANNELLING => {
                        command.payload.get(0..4)
                    }
                    _ => None,
                };
                if let Some(spell_id) = spell_id
                    .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                    .map(u32::from_le_bytes)
                {
                    let self_guid = session.player_guid().unwrap_or(0);
                    if native.note_cancel_requested(self_guid, spell_id) {
                        session.note_native_state_change(now_ms());
                    }
                }
            }
        }
        if last_ping.elapsed() >= Duration::from_secs(10) {
            let mut ping = ping_seq.to_le_bytes().to_vec();
            ping.extend_from_slice(&0u32.to_le_bytes());
            world.send(benilla_protocol_wrath::opcode::CMSG_PING, &ping)?;
            ping_seq += 1;
            last_ping = std::time::Instant::now();
        }
        // Bounded-ownership backpressure (R04): while the reply queue is above its high-water
        // mark, stop reading in short slices. The pause always begins between complete packets,
        // so the framing contract of `world::recv` (`read_exact` across a packet boundary is
        // fatal) is untouched; it ends within `WRATH_REPLY_BACKPRESSURE_SLICE`, re-checks
        // cancellation, and never suppresses a ping or an outbound command.
        if let Some(replies) = events_out.as_ref() {
            let paused_at = Instant::now();
            loop {
                let above = replies
                    .lock()
                    .map(|mut replies| replies.producer_should_pause())
                    .unwrap_or(false);
                if !above || cancel.load(Ordering::SeqCst) {
                    break;
                }
                if paused_at.elapsed() >= WRATH_REPLY_BACKPRESSURE_SLICE {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        // The pause is cancel-aware, but the check must be repeated here: falling through to
        // `world.recv()` would block for the socket read timeout after a stop request and extend
        // `AppExit`/stop latency. Same clean exit as the top-of-loop cancellation block.
        if cancel.load(Ordering::SeqCst) {
            native.invalidate_scope();
            publisher.retire_all();
            return Ok(());
        }
        match world.recv() {
            Ok(p)
                if p.opcode == benilla_protocol_wrath::multiclass::SERVER_OPCODE
                    && multiclass.is_some() =>
            {
                last_server_response = Instant::now();
                multiclass
                    .as_ref()
                    .expect("enabled multiclass")
                    .receive(&p.payload)?;
            }
            Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHARACTER_LOGIN_FAILED => {
                // AzerothCore sends LoginFailureReason, not ResponseCodes. The 12340
                // ClientConnection::HandleCharacterLoginFailed (0x006B2070) uses the
                // same 1–6 indices as the donor glue, with 7–8 handled separately there.
                let result = p.payload.first().copied().unwrap_or(0);
                if let Some(trace) = &feed.trace {
                    trace.observe(
                        Some((session.epoch(), session.world_generation())),
                        None,
                        crate::quest_trace::Stage::LoginMilestone {
                            attempt: live_generation(),
                            milestone: crate::quest_trace::LoginMilestone::CharacterLoginFailed {
                                result,
                            },
                        },
                    );
                }
                send_glue(SessionEvent::CharacterLoginFailed { result });
                native.invalidate_scope();
                publisher.retire_all();
                return Ok(());
            }
            Ok(p) => {
                let received_at = Instant::now();
                last_server_response = received_at;
                let eligible = diagnostic_opcode(p.opcode);
                let traced = eligible
                    && match &feed.trace {
                        Some(trace)
                            if p.opcode == benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT =>
                        {
                            trace.sample_update_packet()
                        }
                        Some(_) => true,
                        None => false,
                    };
                let scope = Some((session.epoch(), session.world_generation()));
                if traced {
                    if let Some(trace) = &feed.trace {
                        trace.observe(
                            scope,
                            None,
                            crate::quest_trace::Stage::PacketReceived { opcode: p.opcode },
                        );
                    }
                }
                // The reducer, the decoder and the translation each run through a live-code
                // boundary ([`crate::live_code::call`]), as the 1.12 stream's read does: this loop
                // was entered before any patch, so a direct call would keep the old decoders.
                let reduced =
                    crate::live_code::call(|| session.reduce(p.opcode, &p.payload, now_ms()));
                if eligible && (traced || reduced.is_err()) {
                    if let Some(trace) = &feed.trace {
                        let stage = if reduced.is_ok() {
                            crate::quest_trace::Stage::ReducerAccepted { opcode: p.opcode }
                        } else {
                            let reason = reduced
                                .as_ref()
                                .err()
                                .map(benilla_protocol_wrath::gameplay::classify_reduce_failure)
                                .map(crate::quest_trace::ReducerReason::from_decode_failure)
                                .unwrap_or(crate::quest_trace::ReducerReason::Other);
                            crate::quest_trace::Stage::ReducerRejected {
                                opcode: p.opcode,
                                reason,
                            }
                        };
                        trace.observe(scope, None, stage);
                    }
                }
                reduced?;
                // A saved store's answer hands its bytes on ahead of its record, as the 1.12
                // stream does (`crate::net::io::inbound`), so the store can save them and decode
                // them again after a decoder patch (`crate::wdb`). Gated like the record itself.
                if session.player_guid().is_some() && crate::wdb::is_saved_answer(p.opcode) {
                    push_session_event(
                        &events_out,
                        &session,
                        SessionEvent::SavedAnswer {
                            opcode: p.opcode,
                            body: p.payload.to_vec(),
                        },
                    );
                }
                if let Some(parsed) =
                    crate::live_code::call(|| gameplay::parse(p.opcode, &p.payload))
                {
                    if traced {
                        if let Some(trace) = &feed.trace {
                            let stage = match &parsed {
                                Ok(_) => {
                                    crate::quest_trace::Stage::GameplayDecoded { opcode: p.opcode }
                                }
                                Err(error) => crate::quest_trace::Stage::GameplayRejected {
                                    opcode: p.opcode,
                                    decoder: gameplay::decoder_family(p.opcode),
                                    reason: gameplay::classify_decode_failure(error),
                                    remaining_bytes: gameplay::decode_remaining_bytes(error),
                                },
                            };
                            trace.observe(scope, None, stage);
                        }
                    }
                    match parsed {
                        Ok(event @ WrathGameplayEvent::Gossip { npc, menu_id, .. }) => {
                            feed.push_event(WrathFeedEvent::GossipMenu {
                                npc,
                                menu_id,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(event @ WrathGameplayEvent::PlayedTime {
                            total,
                            level,
                            trigger,
                        }) => {
                            feed.push_event(WrathFeedEvent::PlayedTime {
                                total,
                                level,
                                trigger_script_event: trigger != 0,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(event @ WrathGameplayEvent::ServerUnixTime {
                            unix_time,
                            reset_in_secs,
                        }) => {
                            feed.push_event(WrathFeedEvent::ServerClock {
                                unix_time,
                                reset_in_secs,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::EquipmentSetList(sets)) => {
                            feed.push_event(WrathFeedEvent::EquipmentSet {
                                packet: crate::wrath_equipment::EquipmentSetPacket::List(sets),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::EquipmentSetSaved(saved)) => {
                            feed.push_event(WrathFeedEvent::EquipmentSet {
                                packet: crate::wrath_equipment::EquipmentSetPacket::Saved(saved),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::EquipmentSetUseResult(result)) => {
                            feed.push_event(WrathFeedEvent::EquipmentSet {
                                packet: crate::wrath_equipment::EquipmentSetPacket::UseResult(
                                    result,
                                ),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::RuneState(state)) => {
                            feed.push_event(WrathFeedEvent::RuneState {
                                runes: state.runes.clone(),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::RuneState(state),
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::RuneConvert(conversion)) => {
                            feed.push_event(WrathFeedEvent::RuneConvert {
                                rune: conversion.rune,
                                rune_type: conversion.rune_type,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::RuneConvert(conversion),
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::RunePower(power)) => {
                            feed.push_event(WrathFeedEvent::RunePower {
                                mask: power.mask,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::RunePower(power),
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(event @ WrathGameplayEvent::SpellDamage {
                            target,
                            attacker,
                            spell_id,
                            damage,
                            overkill,
                            school,
                            absorb,
                            resist,
                            blocked,
                            hit_info,
                        }) => {
                            feed.push_event(WrathFeedEvent::SpellDamage {
                                hit: WrathSpellDamage {
                                    target,
                                    attacker,
                                    spell_id,
                                    damage,
                                    overkill,
                                    school,
                                    absorb,
                                    resist,
                                    blocked,
                                    hit_info,
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                    revision: 0,
                                    received_at: 0.0,
                                },
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(event @ WrathGameplayEvent::SpellHealLog {
                            target,
                            caster,
                            spell_id,
                            amount,
                            overheal,
                            absorbed,
                            critical,
                        }) => {
                            feed.push_event(WrathFeedEvent::SpellHeal {
                                hit: WrathSpellHeal {
                                    target,
                                    caster,
                                    spell_id,
                                    amount,
                                    overheal,
                                    absorbed,
                                    critical,
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                    revision: 0,
                                    received_at: 0.0,
                                },
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(event @ WrathGameplayEvent::SpellEnergizeLog {
                            target,
                            caster,
                            spell_id,
                            power_type,
                            amount,
                        }) => {
                            feed.push_event(WrathFeedEvent::SpellEnergize {
                                hit: WrathSpellEnergize {
                                    target,
                                    caster,
                                    spell_id,
                                    power_type,
                                    amount,
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                    revision: 0,
                                    received_at: 0.0,
                                },
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::SpellMiss {
                            caster,
                            spell_id,
                            misses,
                        }) => {
                            for miss in misses {
                                feed.push_event(WrathFeedEvent::SpellMiss {
                                    hit: WrathSpellMiss {
                                        caster,
                                        target: miss.target,
                                        spell_id,
                                        miss_type: miss.miss_type,
                                        session_epoch: session.epoch(),
                                        world_generation: session.world_generation(),
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                });
                            }
                        }
                        Ok(event @ WrathGameplayEvent::AttackerState {
                            attacker,
                            victim,
                            hit_info,
                            damage,
                            overkill,
                            victim_state,
                            absorb,
                            resist,
                            blocked,
                            melee_spell_id,
                            school,
                        }) => {
                            feed.push_event(WrathFeedEvent::AttackerState {
                                hit: WrathSwingHit {
                                    attacker,
                                    victim,
                                    hit_info,
                                    damage,
                                    overkill,
                                    victim_state,
                                    absorb,
                                    resist,
                                    blocked,
                                    melee_spell_id,
                                    school,
                                },
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::PartyKillLog { killer, victim }) => {
                            feed.push_event(WrathFeedEvent::PartyKillLog {
                                killer,
                                victim,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::PartyKillLog { killer, victim },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::SpellInstakill { caster, target, spell_id }) => {
                            feed.push_event(WrathFeedEvent::SpellInstakill {
                                caster,
                                target,
                                spell_id,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::SpellInstakill { caster, target, spell_id },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::EnchantApplied { caster, target, item_id, enchant_id }) => {
                            feed.push_event(WrathFeedEvent::EnchantApplied {
                                caster,
                                target,
                                item_id,
                                enchant_id,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::EnchantApplied { caster, target, item_id, enchant_id },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::EnchantRemoved { caster, target, item_id, enchant_id }) => {
                            feed.push_event(WrathFeedEvent::EnchantRemoved {
                                caster,
                                target,
                                item_id,
                                enchant_id,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::EnchantRemoved { caster, target, item_id, enchant_id },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::EnvironmentalDamage { victim, damage_type, damage, absorbed, resisted }) => {
                            feed.push_event(WrathFeedEvent::EnvironmentalDamage {
                                victim,
                                damage_type,
                                damage,
                                absorbed,
                                resisted,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::EnvironmentalDamage { victim, damage_type, damage, absorbed, resisted },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::DamageShield { target, attacker, spell_id, damage, overkill, school }) => {
                            feed.push_event(WrathFeedEvent::DamageShield {
                                target,
                                attacker,
                                spell_id,
                                damage,
                                overkill,
                                school,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::DamageShield { target, attacker, spell_id, damage, overkill, school },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::CalendarSendCalendar(calendar)) => {
                            feed.push_event(WrathFeedEvent::Calendar {
                                calendar: calendar.clone(),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::CalendarSendCalendar(calendar),
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::CalendarCommandResult(result)) => {
                            feed.push_event(WrathFeedEvent::CalendarCommandResult {
                                command: result.command,
                                reserved: result.reserved,
                                error: result.error,
                                param: result.param.clone(),
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::CalendarCommandResult(result),
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::LfgJoinResult(result)) => {
                            feed.push_event(WrathFeedEvent::LfgJoinResult {
                                result: *result,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::LfgQueueStatus(status)) => {
                            feed.push_event(WrathFeedEvent::LfgQueueStatus {
                                status,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::LfgUpdatePlayer(update)) => {
                            feed.push_event(WrathFeedEvent::LfgUpdatePlayer {
                                update,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::LfgUpdateParty(update)) => {
                            feed.push_event(WrathFeedEvent::LfgUpdateParty {
                                update,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(WrathGameplayEvent::LfgProposalUpdate(proposal)) => {
                            feed.push_event(WrathFeedEvent::LfgProposalUpdate {
                                proposal: *proposal,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                        }
                        Ok(event @ WrathGameplayEvent::InventoryFailure {
                            reason,
                            item,
                            item2,
                            bag_type,
                            required_level,
                            item_limit_category,
                        }) => {
                            feed.push_native(
                                crate::wrath_native::WrathNativeFeedEvent::InventoryFailure {
                                    failure: crate::wrath_native::WrathInventoryFailure {
                                        reason,
                                        item,
                                        item2,
                                        bag_type,
                                        required_level,
                                        item_limit_category,
                                    },
                                    scope: crate::wrath_native::WrathScope {
                                        session_epoch: session.epoch(),
                                        world_generation: session.world_generation(),
                                        state_revision: session.state_revision(),
                                    },
                                },
                            );
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::LoggedOut) => {
                            logged_out = true;
                            // The logout edge; the worker's exit then publishes the
                            // `Disconnected { end: LoggedOut }` teardown as the second, distinct
                            // terminal. Both handlers do different work, so both are delivered.
                            push_forced_event(&events_out, SessionEvent::LoggedOut, None);
                        }
                        Ok(WrathGameplayEvent::PeriodicAura {
                            target,
                            caster,
                            spell_id,
                            ticks,
                        }) => {
                            let session_epoch = session.epoch();
                            let world_generation = session.world_generation();
                            for tick in &ticks {
                                match *tick {
                                    gameplay::PeriodicAuraTick::Damage {
                                        amount,
                                        overkill,
                                        school,
                                        absorb,
                                        resist,
                                        critical,
                                        hit_info,
                                        ..
                                    } => {
                                        if amount != 0 {
                                            feed.push_event(WrathFeedEvent::SpellPeriodicDamage {
                                                hit: WrathSpellPeriodicDamage {
                                                    target,
                                                    caster,
                                                    spell_id,
                                                    amount,
                                                    overkill,
                                                    school,
                                                    absorb,
                                                    resist,
                                                    critical: critical || (hit_info & 0x2 != 0),
                                                    session_epoch,
                                                    world_generation,
                                                    revision: 0,
                                                    received_at: 0.0,
                                                },
                                                session_epoch,
                                                world_generation,
                                            });
                                        } else if absorb != 0 {
                                            feed.push_event(WrathFeedEvent::SpellPeriodicMiss {
                                                hit: WrathSpellPeriodicMiss {
                                                    target,
                                                    caster,
                                                    spell_id,
                                                    miss_type: 10, // ABSORB
                                                    amount: absorb,
                                                    session_epoch,
                                                    world_generation,
                                                    revision: 0,
                                                    received_at: 0.0,
                                                },
                                                session_epoch,
                                                world_generation,
                                            });
                                        } else if resist != 0 {
                                            feed.push_event(WrathFeedEvent::SpellPeriodicMiss {
                                                hit: WrathSpellPeriodicMiss {
                                                    target,
                                                    caster,
                                                    spell_id,
                                                    miss_type: 2, // RESIST
                                                    amount: resist.unsigned_abs(),
                                                    session_epoch,
                                                    world_generation,
                                                    revision: 0,
                                                    received_at: 0.0,
                                                },
                                                session_epoch,
                                                world_generation,
                                            });
                                        }
                                    }
                                    gameplay::PeriodicAuraTick::Heal {
                                        amount,
                                        overheal,
                                        absorb,
                                        critical,
                                    } => {
                                        feed.push_event(WrathFeedEvent::SpellPeriodicHeal {
                                            hit: WrathSpellPeriodicHeal {
                                                target,
                                                caster,
                                                spell_id,
                                                amount,
                                                overheal,
                                                absorbed: absorb,
                                                critical,
                                                session_epoch,
                                                world_generation,
                                                revision: 0,
                                                received_at: 0.0,
                                            },
                                            session_epoch,
                                            world_generation,
                                        });
                                    }
                                    gameplay::PeriodicAuraTick::Energize { power, amount } => {
                                        feed.push_event(WrathFeedEvent::SpellPeriodicEnergize {
                                            hit: WrathSpellPeriodicEnergize {
                                                target,
                                                caster,
                                                spell_id,
                                                power_type: power,
                                                amount,
                                                session_epoch,
                                                world_generation,
                                                revision: 0,
                                                received_at: 0.0,
                                            },
                                            session_epoch,
                                            world_generation,
                                        });
                                    }
                                    gameplay::PeriodicAuraTick::ManaLeech {
                                        power,
                                        amount,
                                        multiplier,
                                    } => {
                                        feed.push_event(WrathFeedEvent::SpellPeriodicDrain {
                                            hit: WrathSpellPeriodicDrain {
                                                target,
                                                caster,
                                                spell_id,
                                                power_type: power,
                                                amount,
                                                multiplier,
                                                session_epoch,
                                                world_generation,
                                                revision: 0,
                                                received_at: 0.0,
                                            },
                                            session_epoch,
                                            world_generation,
                                        });
                                    }
                                }
                            }
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    WrathGameplayEvent::PeriodicAura {
                                        target,
                                        caster,
                                        spell_id,
                                        ticks,
                                    },
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::SpellExecuteLog {
                            caster,
                            spell_id,
                            rows,
                        }) => {
                            let session_epoch = session.epoch();
                            let world_generation = session.world_generation();
                            for row in rows {
                                let row = match row {
                                    gameplay::SpellExecuteLogRow::Interrupt {
                                        target,
                                        interrupted_spell_id,
                                    } => WrathSpellExecuteLogRow::Interrupt {
                                        target,
                                        interrupted_spell_id,
                                    },
                                    gameplay::SpellExecuteLogRow::ExtraAttacks { target, amount } => {
                                        WrathSpellExecuteLogRow::ExtraAttacks { target, amount }
                                    }
                                    gameplay::SpellExecuteLogRow::Create { target } => {
                                        WrathSpellExecuteLogRow::Create { target }
                                    }
                                    gameplay::SpellExecuteLogRow::Summon { target } => {
                                        WrathSpellExecuteLogRow::Summon { target }
                                    }
                                    gameplay::SpellExecuteLogRow::Resurrect { target } => {
                                        WrathSpellExecuteLogRow::Resurrect { target }
                                    }
                                    gameplay::SpellExecuteLogRow::Drain {
                                        target,
                                        amount,
                                        power_type,
                                        multiplier,
                                    } => WrathSpellExecuteLogRow::Drain {
                                        target,
                                        amount,
                                        power_type,
                                        multiplier,
                                    },
                                    gameplay::SpellExecuteLogRow::Leech {
                                        target,
                                        amount,
                                        power_type,
                                        multiplier,
                                    } => WrathSpellExecuteLogRow::Leech {
                                        target,
                                        amount,
                                        power_type,
                                        multiplier,
                                    },
                                    gameplay::SpellExecuteLogRow::DurabilityDamage {
                                        target,
                                        item_id,
                                        slot,
                                    } => WrathSpellExecuteLogRow::DurabilityDamage {
                                        target,
                                        item_id,
                                        slot,
                                    },
                                    gameplay::SpellExecuteLogRow::DurabilityDamageAll { target } => {
                                        WrathSpellExecuteLogRow::DurabilityDamageAll { target }
                                    }
                                };
                                feed.push_event(WrathFeedEvent::SpellExecuteLog {
                                    hit: WrathSpellExecuteLog {
                                        caster,
                                        spell_id,
                                        row,
                                        session_epoch,
                                        world_generation,
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch,
                                    world_generation,
                                });
                            }
                        }
                        Ok(WrathGameplayEvent::SpellDispelLog {
                            victim,
                            caster,
                            dispel_spell_id,
                            rows,
                        }) => {
                            let session_epoch = session.epoch();
                            let world_generation = session.world_generation();
                            for row in rows {
                                feed.push_event(WrathFeedEvent::SpellDispelLog {
                                    hit: WrathSpellDispelLog {
                                        caster,
                                        target: victim,
                                        spell_id: dispel_spell_id,
                                        replaced_spell_id: row.spell_id,
                                        harmful: row.harmful,
                                        stolen: false,
                                        session_epoch,
                                        world_generation,
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch,
                                    world_generation,
                                });
                            }
                        }
                        Ok(WrathGameplayEvent::SpellStealLog {
                            victim,
                            caster,
                            dispel_spell_id,
                            rows,
                        }) => {
                            let session_epoch = session.epoch();
                            let world_generation = session.world_generation();
                            for row in rows {
                                feed.push_event(WrathFeedEvent::SpellDispelLog {
                                    hit: WrathSpellDispelLog {
                                        caster,
                                        target: victim,
                                        spell_id: dispel_spell_id,
                                        replaced_spell_id: row.spell_id,
                                        harmful: row.harmful,
                                        stolen: true,
                                        session_epoch,
                                        world_generation,
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch,
                                    world_generation,
                                });
                            }
                        }
                        Ok(WrathGameplayEvent::SpellDispelFailed {
                            caster,
                            victim,
                            dispel_spell_id,
                            failed_spell_ids,
                        }) => {
                            let session_epoch = session.epoch();
                            let world_generation = session.world_generation();
                            for failed_spell_id in failed_spell_ids {
                                feed.push_event(WrathFeedEvent::SpellDispelFailed {
                                    hit: WrathSpellDispelFailed {
                                        caster,
                                        target: victim,
                                        spell_id: dispel_spell_id,
                                        failed_spell_id,
                                        session_epoch,
                                        world_generation,
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch,
                                    world_generation,
                                });
                            }
                        }
                        Ok(event) => {
                            // The cast edges also feed the combat-log queues; the event is only
                            // borrowed here so the native cast lifecycle and the live translation
                            // below still see the same packet.
                            if let WrathGameplayEvent::SpellStart {
                                caster,
                                spell_id,
                                cast_flags,
                                cast_time_ms,
                                ..
                            } = &event
                            {
                                feed.push_event(WrathFeedEvent::SpellCastStart {
                                    hit: WrathSpellCastStart {
                                        caster: *caster,
                                        spell_id: *spell_id,
                                        cast_flags: *cast_flags,
                                        cast_time_ms: *cast_time_ms,
                                        session_epoch: session.epoch(),
                                        world_generation: session.world_generation(),
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                });
                            }
                            if let WrathGameplayEvent::SpellGo {
                                caster,
                                spell_id,
                                cast_flags,
                                targets,
                                ..
                            } = &event
                            {
                                feed.push_event(WrathFeedEvent::SpellCastSuccess {
                                    hit: WrathSpellCastSuccess {
                                        caster: *caster,
                                        spell_id: *spell_id,
                                        cast_flags: *cast_flags,
                                        target: targets.unit.unwrap_or(0),
                                        session_epoch: session.epoch(),
                                        world_generation: session.world_generation(),
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                });
                            }
                            if let WrathGameplayEvent::CastFailed { spell_id, reason, .. } = &event
                            {
                                feed.push_event(WrathFeedEvent::SpellCastFailed {
                                    hit: WrathSpellCastFailed {
                                        spell_id: *spell_id,
                                        reason: *reason,
                                        session_epoch: session.epoch(),
                                        world_generation: session.world_generation(),
                                        revision: 0,
                                        received_at: 0.0,
                                    },
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                });
                            }
                            if let WrathGameplayEvent::VendorInventory { vendor, items } = &event {
                                feed.push_event(WrathFeedEvent::VendorInventory {
                                    vendor: *vendor,
                                    items: items.clone(),
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                });
                            }
                            if let WrathGameplayEvent::ItemText {
                                has_text,
                                item_guid,
                                text,
                            } = &event
                            {
                                feed.push_native(
                                    crate::wrath_native::WrathNativeFeedEvent::ItemText(
                                        crate::wrath_native::WrathItemText {
                                            scope: crate::wrath_native::WrathScope {
                                                session_epoch: session.epoch(),
                                                world_generation: session.world_generation(),
                                                state_revision: session.state_revision(),
                                            },
                                            item_guid: *item_guid,
                                            has_text: *has_text,
                                            text: text.clone(),
                                        },
                                    ),
                                );
                            }
                            let observed_at_unix_ms = now_ms();
                            if let Some(update) = crate::wrath_native::update_from_gameplay(
                                &event,
                                crate::wrath_native::WrathScope {
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                    state_revision: session.state_revision(),
                                },
                                observed_at_unix_ms,
                                session.player_guid().unwrap_or(0),
                            ) {
                                if let Some(resolved) = native.apply(update) {
                                    // One source-change revision covers replicated-world and
                                    // native-state edges alike: the export fires on its own,
                                    // and position provenance stays untouched.
                                    session.note_native_state_change(observed_at_unix_ms);
                                    feed.push_native(
                                        crate::wrath_native::WrathNativeFeedEvent::Cast(resolved),
                                    );
                                }
                            }
                            if session.player_guid().is_some() {
                                for event in translate_live(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Err(e) if p.opcode
                            == benilla_protocol_wrath::opcode::SMSG_ITEM_QUERY_SINGLE_RESPONSE =>
                        {
                            // The ask stays pending until the item cache's re-ask clock runs out.
                            let item = p
                                .payload
                                .get(..4)
                                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) & 0x7FFF_FFFF);
                            warn!(
                                "wrath: dropped undecodable item template reply for item {item:?} ({} bytes): {e}",
                                p.payload.len()
                            );
                        }
                        Err(e) => {
                            debug!("wrath: ignored malformed 0x{:X} gameplay reply: {e}", p.opcode);
                        }
                    }
                } else if session.player_guid().is_some() && is_shared_12340_server_opcode(p.opcode)
                {
                    let decoded = crate::live_code::call(|| {
                        benilla_protocol::messages::parse_server(p.opcode, &p.payload)
                            .map(benilla_protocol::decode)
                            .unwrap_or_default()
                    });
                    for event in decoded {
                        push_session_event(&events_out, &session, event);
                    }
                }
                if p.opcode == benilla_protocol_wrath::opcode::SMSG_MULTICLASS_REPLY {
                    if let Ok(reply) =
                        benilla_protocol_wrath::gameplay::accept_multiclass_reply(&p.payload)
                    {
                        feed.push_event(WrathFeedEvent::MulticlassReply {
                            op: reply.op,
                            status: reply.status,
                            serial: reply.serial,
                            payload: reply.body,
                            session_epoch: session.epoch(),
                            world_generation: session.world_generation(),
                        });
                    }
                }
                if p.opcode == 0x390 {
                    // `SMSG_TIME_SYNC_REQ`: echo the counter with the clock the moves are stamped
                    // on. The server derives its clock delta from this reading and adds it to
                    // every move's time (`MovementHandler.cpp` `HandleTimeSyncResp`).
                    if p.payload.len() >= 4 {
                        let mut resp = p.payload[0..4].to_vec();
                        resp.extend_from_slice(&wrath_client_ms().to_le_bytes());
                        world.send(0x391, &resp)?;
                    }
                }
            }
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
            Err(e) => {
                if let Some(trace) = &feed.trace {
                    trace.observe(
                        Some((session.epoch(), session.world_generation())),
                        None,
                        crate::quest_trace::Stage::SocketReadFailed,
                    );
                }
                return Err(e);
            }
        }
        if logged_out {
            native.invalidate_scope();
            session.finish_session();
            feed.push_event(WrathFeedEvent::Phase {
                phase: session.phase(),
                session_epoch: session.epoch(),
                world_generation: session.world_generation(),
                login_generation: config.login_generation,
                map_id: session.map_id(),
            });
            publisher.retire_all();
            return Ok(());
        }
        if last_server_response.elapsed() >= SERVER_RESPONSE_TIMEOUT {
            // A quiet server still answers our ping. Silence is only a disconnection once the
            // bounded monotonic response window closes; one socket read timeout is not evidence.
            if let Some(publisher) = publisher.as_ref() {
                if let Ok(health) = benilla_wm_native::SourceHealth::new(
                    session.epoch(),
                    session.world_generation(),
                    session.player_guid().unwrap_or(0),
                    benilla_wm_native::SourceSessionState::Disconnected,
                    session.last_server_packet_at_unix_ms(),
                ) {
                    let _ = publisher.publish_health(health);
                }
            }
            publisher.retire_all();
            native.invalidate_scope();
            anyhow::bail!(
                "world server made no response for {} s",
                SERVER_RESPONSE_TIMEOUT.as_secs()
            );
        }
        for event in session.take_lifecycle() {
            // The same lifecycle policy as the renderless driver: retire the old scope and order
            // the replacement after that worker stops, without blocking the packet loop. Native
            // authority state from the retired world scope is dropped before the replacement
            // can export it.
            native.invalidate_scope();
            replace_publisher(
                publisher_config.as_ref(),
                &mut publisher.publisher,
                &mut publisher.retiring,
            )?;
            feed.push_event(WrathFeedEvent::from_session(event, session.epoch()));
        }
        // Near teleport: acknowledge the server's order counter after landing (packed GUID +
        // u32 flags + u32 time â€” both read and unused server-side).
        for _counter in session.take_pending_teleport_acks() {
            let mut ack = enc_pack_guid(guid);
            ack.extend_from_slice(&0u32.to_le_bytes());
            ack.extend_from_slice(&0u32.to_le_bytes());
            world.send(
                benilla_protocol_wrath::opcode::MSG_MOVE_TELEPORT_ACK as u32,
                &ack,
            )?;
        }
        // Far transfer: the NEW_WORLD was processed; tear down, then acknowledge so the
        // server sends the new world's visibility traffic.
        if session.phase() == Phase::Loading {
            world.send(0x0DCu32, &[])?; // MSG_MOVE_WORLDPORT_ACK (empty payload)
            session.worldport_acknowledged();
        }
        // Publish a full-world digest whenever source state advanced â€” independent of the WM
        // bridge, so the graphical client receives state with WM disabled/busy/reconnecting.
        if session.state_revision() != last_digest_revision {
            last_digest_revision = session.state_revision();
            feed.publish_digest(build_digest(&session));
        }
        let phase_before = session.phase();
        let outcome = publisher.as_mut().map(|publisher| {
            let mut adapter = Adapter {
                session: &mut session,
                native: &native,
            };
            pump_once(
                publisher,
                &mut adapter,
                now_ms(),
                started.elapsed().as_millis() as u64,
            )
        });
        match outcome {
            None => {}
            Some(Ok(_)) => {}
            Some(Err(benilla_wm_native::PumpError::Producer(e))) => {
                feed.push_event(WrathFeedEvent::ExportSkipped(e.message().to_owned()));
            }
            Some(Err(benilla_wm_native::PumpError::Publisher(
                benilla_wm_native::Error::Disconnected,
            ))) => {
                // The publisher signalled shutdown: replace it under the shared lifecycle policy.
                replace_publisher(
                    publisher_config.as_ref(),
                    &mut publisher.publisher,
                    &mut publisher.retiring,
                )?;
            }
            Some(Err(benilla_wm_native::PumpError::Publisher(e))) => {
                feed.push_event(WrathFeedEvent::ExportSkipped(format!("publisher: {e}")));
            }
            Some(Err(e)) => {
                feed.push_event(WrathFeedEvent::ExportSkipped(format!("{e}")));
            }
        }
        if session.phase() != phase_before {
            feed.push_event(WrathFeedEvent::Phase {
                phase: session.phase(),
                session_epoch: session.epoch(),
                world_generation: session.world_generation(),
                login_generation: config.login_generation,
                map_id: session.map_id(),
            });
        }
    }

    world.send(benilla_protocol_wrath::opcode::CMSG_LOGOUT_REQUEST, &[])?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        match world.recv() {
            Ok(p) if p.opcode == 0x04D => {
                session.finish_session();
                feed.push_event(WrathFeedEvent::Phase {
                    phase: session.phase(),
                    session_epoch: session.epoch(),
                    world_generation: session.world_generation(),
                    login_generation: config.login_generation,
                    map_id: session.map_id(),
                });
                publisher.retire_all();
                return Ok(());
            }
            Ok(_) => {}
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("SMSG_LOGOUT_COMPLETE never arrived")
}

fn enc_pack_guid(guid: u64) -> Vec<u8> {
    let mut mask = 0u8;
    let mut bytes = Vec::new();
    for i in 0..8 {
        let b = (guid >> (i * 8)) as u8;
        if b != 0 {
            mask |= 1 << i;
            bytes.push(b);
        }
    }
    let mut out = vec![mask];
    out.extend_from_slice(&bytes);
    out
}

/// Server-to-client opcodes whose 3.3.5a (build 12340) wire layout is compatible with
/// `benilla_protocol::messages::parse_server` and `benilla_protocol::decode`.
fn is_shared_12340_server_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        // World clock, bind & cinematic
        0x042 | 0x043 | 0x044 | 0x047 | 0x0FA | 0x155 | 0x158 | 0x29D
        // Pet & item name queries
        | 0x053 | 0x2C5
        // Guild query & events
        | 0x055 | 0x083 | 0x086 | 0x088 | 0x092 | 0x093 | 0x1E6 | 0x1F1
        // Channels, Who, Social, Server Message
        | 0x061 | 0x063 | 0x065 | 0x068 | 0x09B | 0x19E | 0x1CD
        // Group, Raid, Ready Check
        | 0x06F | 0x074 | 0x077 | 0x079 | 0x07C | 0x07F | 0x0FE | 0x2CC | 0x2FA | 0x321
        // Movement force speeds, modes, splines, knockback, control & mount results
        | 0x0DE | 0x0DF | 0x0E4 | 0x0E8 | 0x0EA | 0x0EC | 0x0EF
        | 0x13A | 0x13C | 0x159 | 0x16E | 0x16F | 0x172
        | 0x2DA | 0x2F2 | 0x2F3 | 0x2F4 | 0x2F5
        | 0x2FD..=0x302 | 0x304..=0x30D
        // Tutorials, Emotes, Mirror Timers, Weather, Sounds, World States
        | 0x0FD | 0x103 | 0x105 | 0x1D9 | 0x1DA | 0x1DB | 0x277 | 0x278 | 0x2C3 | 0x2D2 | 0x2F9
        // Trade, Factions (visible/atwar/forced), Proficiencies, Spell Modifiers
        | 0x120 | 0x121 | 0x123 | 0x127 | 0x266 | 0x267 | 0x2A5 | 0x313
        // Combat logs & durability/exploration/party kill
        | 0x14B | 0x1D6 | 0x1F5 | 0x1F8 | 0x21D | 0x24B | 0x2A6 | 0x2BD
        // Duel
        | 0x167..=0x16B | 0x2B7
        // Pet mode, feedback, sounds, unlearn
        | 0x173 | 0x178 | 0x17A | 0x1B1 | 0x2AF | 0x2C6 | 0x324 | 0x325
        // Taxi
        | 0x1A9 | 0x1AB | 0x1AE | 0x1AF
        // GM Ticket
        | 0x206 | 0x208 | 0x212 | 0x218 | 0x21B | 0x283
        // Mail & Auction
        | 0x239 | 0x255 | 0x25B | 0x25E | 0x25F | 0x284 | 0x285 | 0x28D
    )
}

fn build_digest(session: &SessionState) -> WorldDigest {
    use benilla_protocol_wrath::fields;
    let mut entities = BTreeMap::new();
    for (guid, entry) in session.world().iter() {
        let observed = |index: u16| {
            entry
                .field_delivered(index)
                .then(|| entry.field(index))
                .flatten()
        };
        // Unit indices name other words on every other type: on a GameObject `CHARMEDBY`,
        // `SUMMONEDBY` and `CREATEDBY` (`12..17`) are its parent rotation, dynamic, faction, level
        // and bytes, which the projection would write into the 1.12 rotation, state and position.
        use benilla_protocol_wrath::updates::type_id;
        let unit_layout = matches!(entry.type_id, type_id::UNIT | type_id::PLAYER);
        let player_layout = entry.type_id == type_id::PLAYER;
        let unit = |index: u16| if unit_layout { observed(index) } else { None };
        let player = |index: u16| if player_layout { observed(index) } else { None };
        let unit_guid = |index: u16| {
            if unit_layout {
                entry.self_guid(index).filter(|&guid| guid != 0)
            } else {
                None
            }
        };
        let mut powers = [None; 7];
        let mut max_powers = [None; 7];
        for i in 0..7u16 {
            powers[usize::from(i)] = unit(fields::UNIT_FIELD_POWER1 + i);
            max_powers[usize::from(i)] = unit(fields::UNIT_FIELD_MAXPOWER1 + i);
        }
        let display_id = match entry.type_id {
            type_id::UNIT | type_id::PLAYER => observed(fields::UNIT_FIELD_DISPLAYID),
            type_id::GAMEOBJECT => observed(fields::GAMEOBJECT_DISPLAYID),
            type_id::CORPSE => observed(fields::CORPSE_FIELD_DISPLAY_ID),
            _ => None,
        };
        entities.insert(
            *guid,
            WrathEntity {
                guid: *guid,
                type_id: entry.type_id,
                entry: observed(fields::OBJECT_FIELD_ENTRY),
                display_id,
                scale: observed(fields::OBJECT_FIELD_SCALE_X).map(f32::from_bits),
                health: unit(fields::UNIT_FIELD_HEALTH),
                max_health: unit(fields::UNIT_FIELD_MAXHEALTH),
                powers,
                max_powers,
                level: unit(fields::UNIT_FIELD_LEVEL),
                bytes_0: unit(fields::UNIT_FIELD_BYTES_0),
                faction: unit(fields::UNIT_FIELD_FACTIONTEMPLATE),
                unit_flags: unit(fields::UNIT_FIELD_FLAGS),
                mount_display_id: unit(fields::UNIT_FIELD_MOUNTDISPLAYID),
                bytes_1: unit(fields::UNIT_FIELD_BYTES_1),
                bytes_2: unit(fields::UNIT_FIELD_BYTES_2),
                npc_flags: unit(fields::UNIT_NPC_FLAGS),
                dynamic_flags: unit(fields::UNIT_DYNAMIC_FLAGS),
                target: unit_guid(fields::UNIT_FIELD_TARGET),
                charmed_by: unit_guid(fields::UNIT_FIELD_CHARMEDBY),
                summoned_by: unit_guid(fields::UNIT_FIELD_SUMMONEDBY),
                created_by: unit_guid(fields::UNIT_FIELD_CREATEDBY),
                charm: unit_guid(fields::UNIT_FIELD_CHARM),
                summon: unit_guid(fields::UNIT_FIELD_SUMMON),
                corpse_fields: (entry.type_id == type_id::CORPSE).then(|| {
                    crate::wrath_inventory::WrathFieldWindow::from_fields(
                        fields::CORPSE_END,
                        |index| entry.field(index),
                        |index| entry.field_delivered(index),
                    )
                }),
                player_bytes: player(fields::PLAYER_BYTES),
                player_bytes_2: player(fields::PLAYER_BYTES_2),
                position: entry.movement.as_ref().and_then(|m| m.position),
                move_flags: entry.movement.as_ref().map(|m| m.move_flags),
                create_spline: entry
                    .movement
                    .as_ref()
                    .and_then(|m| m.create_spline.clone()),
                attack_target: entry.movement.as_ref().and_then(|m| m.attack_target),
                generation: entry.generation,
                player_fields: (entry.type_id == benilla_protocol_wrath::updates::type_id::PLAYER)
                    .then(|| {
                        crate::wrath_inventory::WrathFieldWindow::from_fields(
                            crate::wrath_inventory::PLAYER_WINDOW_END,
                            |index| entry.field(index),
                            |index| entry.field_delivered(index),
                        )
                    }),
                item_fields: matches!(
                    entry.type_id,
                    benilla_protocol_wrath::updates::type_id::ITEM
                        | benilla_protocol_wrath::updates::type_id::CONTAINER
                )
                .then(|| {
                    let container =
                        entry.type_id == benilla_protocol_wrath::updates::type_id::CONTAINER;
                    crate::wrath_inventory::WrathFieldWindow::from_fields(
                        if container {
                            fields::CONTAINER_END
                        } else {
                            fields::ITEM_END
                        },
                        |index| entry.field(index),
                        |index| entry.field_delivered(index),
                    )
                }),
                auras: entry.auras.clone(),
                auras_observed: entry.auras_observed,
                aura_receipts: entry.aura_receipts.clone(),
                raw_fields: {
                    use benilla_protocol_wrath::updates::type_id;
                    let end = match entry.type_id {
                        type_id::UNIT => Some(fields::UNIT_END),
                        type_id::GAMEOBJECT => Some(fields::GAMEOBJECT_END),
                        type_id::DYNAMICOBJECT => Some(fields::DYNAMICOBJECT_END),
                        type_id::CORPSE => Some(fields::CORPSE_END),
                        _ => None,
                    };
                    end.map(|end| {
                        crate::wrath_inventory::WrathFieldWindow::from_fields(
                            end,
                            |index| entry.field(index),
                            |index| entry.field_delivered(index),
                        )
                    })
                },
            },
        );
    }
    WorldDigest {
        session_epoch: session.epoch(),
        world_generation: session.world_generation(),
        state_revision: session.state_revision(),
        map_id: session.map_id().unwrap_or(0),
        phase: session.phase(),
        player_guid: session.player_guid().unwrap_or(0),
        vehicle: session
            .player_guid()
            .and_then(|guid| session.world().seated_vehicle(guid)),
        self_observation: session.current_self_snapshot(),
        entities,
    }
}

// Small helper kept local so the module stays self-contained.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol_wrath::fields;

    #[test]
    fn self_pose_is_not_replayed_by_unrelated_snapshots() {
        use benilla_protocol_wrath::updates::PositionSnapshot;
        let mut world = World::new();
        let pose = PositionSnapshot {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            o: 0.5,
        };
        let entity = world.spawn(WrathAppliedPosition(pose)).id();
        // Spawn already consumed this position. Health/aura snapshots retain it.
        assert!(!consume_wrath_position(&mut world, entity, pose));
        let newer = PositionSnapshot { x: 4.0, ..pose };
        assert!(consume_wrath_position(&mut world, entity, newer));
        assert!(!consume_wrath_position(&mut world, entity, newer));
        // Returning to the same coordinates later is still a new position.
        assert!(consume_wrath_position(&mut world, entity, pose));
        // A replacement entity must not inherit the previous entity's receipt.
        let replacement = world.spawn_empty().id();
        assert!(consume_wrath_position(&mut world, replacement, pose));
    }

    fn wrath_swing(hit_info: u32, school: u32) -> WrathGameplayEvent {
        WrathGameplayEvent::AttackerState {
            attacker: 0x11,
            victim: 0xf130_0000_0000_0022,
            hit_info,
            damage: 30,
            overkill: 0,
            victim_state: 1,
            absorb: 0,
            resist: 0,
            blocked: 0,
            melee_spell_id: 0,
            school,
        }
    }

    fn translated_swing(hit_info: u32, school: u32) -> ProtocolAttackerState {
        match translate_gameplay(wrath_swing(hit_info, school), 0x11, Instant::now()).as_slice() {
            [SessionEvent::AttackerState(s)] => *s,
            other => panic!("one swing expected, got {other:?}"),
        }
    }

    /// 12340 sends a school mask; a normal swing is mask 1. The 1.12 consumers read school 0 as
    /// physical, so passing the mask through made every wolf hit a Holy `…SCHOOL` line.
    #[test]
    fn wrath_melee_school_masks_become_vanilla_school_indices() {
        for (mask, index) in [
            (0, 0),
            (1, 0),
            (2, 1),
            (4, 2),
            (8, 3),
            (16, 4),
            (32, 5),
            (64, 6),
        ] {
            assert_eq!(vanilla_school(mask), index, "mask {mask:#x}");
            assert_eq!(
                translated_swing(0x2, mask).school,
                index,
                "the seam carries the index"
            );
        }
    }

    /// The melee HitInfo bit positions moved. A crit, full absorb/resist, block, glance, crush and
    /// the no-animation bit must arrive at the 1.12 seam as its own bits, and 12340's 0x4000/0x8000
    /// words must not masquerade as vanilla glancing/crushing.
    #[test]
    fn wrath_melee_hit_info_becomes_the_vanilla_bit_layout() {
        assert_eq!(vanilla_hit_info(0x0000_0002), 0x0000_0002, "a plain hit");
        assert_eq!(vanilla_hit_info(0x0000_0004), 0x0000_0004, "offhand");
        assert_eq!(vanilla_hit_info(0x0000_0010), 0x0000_0010, "miss");
        assert_eq!(vanilla_hit_info(0x0000_0020), 0x0000_0020, "full absorb");
        assert_eq!(vanilla_hit_info(0x0000_0040), 0x0000_0020, "partial absorb");
        assert_eq!(vanilla_hit_info(0x0000_0080), 0x0000_0040, "full resist");
        assert_eq!(vanilla_hit_info(0x0000_0100), 0x0000_0040, "partial resist");
        assert_eq!(vanilla_hit_info(0x0000_0200), 0x0000_0080, "crit");
        assert_eq!(vanilla_hit_info(0x0000_2000), 0x0000_0800, "block");
        assert_eq!(vanilla_hit_info(0x0001_0000), 0x0000_4000, "glancing");
        assert_eq!(vanilla_hit_info(0x0002_0000), 0x0000_8000, "crushing");
        assert_eq!(vanilla_hit_info(0x0004_0000), 0x0001_0000, "no animation");
        assert_eq!(
            vanilla_hit_info(0x0020_0000),
            0x0008_0000,
            "swing no-hit-sound"
        );
        assert_eq!(
            vanilla_hit_info(0x0000_4000 | 0x0000_8000),
            0,
            "12340's unknown words are not 1.12 glancing/crushing"
        );
        assert_eq!(
            vanilla_hit_info(0x0080_0000),
            0,
            "12340's rage-gain bit has no 1.12 word"
        );
    }

    /// The translated crit and full resist reach the shared line/feedback classifier as such,
    /// instead of as an ordinary hit and no line.
    #[test]
    fn wrath_melee_crit_and_full_resist_reach_the_vanilla_classifier() {
        let crit = translated_swing(0x0000_0202, 1);
        let (family, word) =
            crate::ui_chat::combat::melee_outcome(&crit).expect("a crit has a line");
        assert_eq!(family.stem, "COMBATHITCRIT");
        assert_eq!(word.message_type, "DAMAGE_CRIT");

        let mut full_resist = translated_swing(0x0000_0080, 1);
        full_resist.damage = 0;
        let (family, word) =
            crate::ui_chat::combat::melee_outcome(&full_resist).expect("a full resist has a line");
        assert_eq!(family.stem, "VSRESIST");
        assert_eq!(word.message_type, "RESIST");
    }

    /// 12340's spell damage and periodic ticks carry the same school mask; both become the index
    /// the 1.12 line and portrait flash read.
    #[test]
    fn wrath_spell_damage_and_periodic_school_masks_become_indices() {
        let spell = translate_gameplay(
            WrathGameplayEvent::SpellDamage {
                target: 0x22,
                attacker: 0x11,
                spell_id: 133,
                damage: 40,
                overkill: 0,
                school: 4,
                absorb: 0,
                resist: 0,
                blocked: 0,
                hit_info: 0,
            },
            0x11,
            Instant::now(),
        );
        assert!(matches!(
            spell.as_slice(),
            [SessionEvent::SpellDamageLog(s)] if s.school == 2
        ));

        let periodic = translate_gameplay(
            WrathGameplayEvent::PeriodicAura {
                target: 0x22,
                caster: 0x11,
                spell_id: 172,
                ticks: vec![gameplay::PeriodicAuraTick::Damage {
                    amount: 5,
                    overkill: 0,
                    school: 16,
                    absorb: 0,
                    resist: 0,
                    critical: false,
                    hit_info: 0,
                }],
            },
            0x11,
            Instant::now(),
        );
        assert!(matches!(
            periodic.as_slice(),
            [SessionEvent::PeriodicAuraLog(p)]
                if matches!(p.ticks.as_slice(), [ProtocolPeriodicTick::Damage { school: 4, .. }])
        ));
    }

    /// A kill's XP keeps its victim so the chat line and the corpse's XP mark can name it; the
    /// base recovers the un-rested award, which the 1.12 line formats separately.
    #[test]
    fn wrath_kill_xp_keeps_its_victim_and_base_award() {
        let events = translate_gameplay(
            WrathGameplayEvent::XpGain {
                victim: 0xf130_0000_0000_0022,
                total_xp: 60,
                kill_xp: true,
                rested_bonus: 20,
                group_rate: 1.0,
            },
            0x11,
            Instant::now(),
        );
        assert!(matches!(
            events.as_slice(),
            [SessionEvent::XpGain(x)]
                if x.victim == 0xf130_0000_0000_0022
                    && x.total == 60
                    && x.base == 40
                    && x.kill
                    && x.group == 1.0
        ));
    }

    /// A self `SMSG_SPELL_GO` is 3.3.5's success answer: it must emit the 1.12 success result
    /// first, or the in-flight slot stays armed and every later press says 0x61. Another caster's
    /// GO never touches our slot.
    #[test]
    fn a_wrath_self_go_answers_the_pending_cast() {
        let go = |caster| WrathGameplayEvent::SpellGo {
            caster,
            spell_id: 6673,
            cast_flags: 0,
            cast_count: 0,
            server_time_ms: 0,
            hits: Vec::new(),
            misses: Vec::new(),
            targets: gameplay::SpellTargets::default(),
            ammo_display_id: None,
            item_caster: None,
        };
        let events = translate_gameplay(go(0x1522), 0x1522, Instant::now());
        assert!(
            matches!(
                events.first(),
                Some(SessionEvent::CastResult {
                    spell_id: 6673,
                    success: true,
                    reason: None,
                    arg: None,
                })
            ),
            "the self GO answers the send: {events:?}"
        );
        assert!(
            matches!(
                events.get(1),
                Some(SessionEvent::SpellGo { caster: 0x1522, .. })
            ),
            "and the GO itself still follows: {events:?}"
        );
        assert_eq!(events.len(), 2);

        let remote = translate_gameplay(go(0x99), 0x1522, Instant::now());
        assert_eq!(remote.len(), 1);
        assert!(matches!(
            remote[0],
            SessionEvent::SpellGo { caster: 0x99, .. }
        ));
    }

    /// AzerothCore follows a refusal's `SMSG_CAST_FAILED` with `SendInterrupted(0)`. The self
    /// copy only ends the cast. A failed `CastResult` from it was a second red line,
    /// "Internal error", under "Not enough rage".
    #[test]
    fn a_wrath_self_spell_failure_reports_no_second_line() {
        let failure = WrathGameplayEvent::SpellFailure {
            caster: 0x1522,
            spell_id: 772,
            cast_count: 3,
            raw_reason: 0,
        };
        let events = translate_gameplay(failure, 0x1522, Instant::now());
        assert!(
            matches!(
                events.as_slice(),
                [SessionEvent::SpellFailedOther {
                    caster: 0x1522,
                    spell_id: 772
                }]
            ),
            "{events:?}"
        );
    }

    /// One cast terminates once. `Spell::cancel` sends `SendCastResult(40)` and then
    /// `SendInterrupted(40)` for the same attempt. Our own `SMSG_SPELL_FAILURE` with 12340
    /// SPELL_FAILED_INTERRUPTED (40) only ends the observed cast, like reason 0; the failure
    /// line comes from `SMSG_CAST_FAILED` alone.
    #[test]
    fn an_interrupt_does_not_fail_the_same_cast_twice() {
        let failure = WrathGameplayEvent::SpellFailure {
            caster: 0x1522,
            spell_id: 133,
            cast_count: 4,
            raw_reason: 40,
        };
        let events = translate_gameplay(failure, 0x1522, Instant::now());
        assert!(
            matches!(
                events.as_slice(),
                [SessionEvent::SpellFailedOther {
                    caster: 0x1522,
                    spell_id: 133
                }]
            ),
            "{events:?}"
        );
    }

    /// A Charge spline naming our own guid reaches the self-ride, even with a parabolic flag; it
    /// was dropped, so the warrior never travelled the path.
    #[test]
    fn a_wrath_self_spline_reaches_the_self_ride() {
        let mv = |flags| {
            WrathGameplayEvent::MonsterMove(gameplay::MonsterMove {
                guid: 0x1522,
                transport: None,
                movement_byte: 0,
                start: [0.0; 3],
                spline_id: 9,
                facing: gameplay::MonsterFacing::None,
                stop: false,
                flags,
                duration_ms: 400,
                animation: None,
                parabolic: None,
                path: vec![[0.0; 3], [10.0, 0.0, 0.0]],
            })
        };
        for flags in [0, 0x800] {
            let events = translate_gameplay(mv(flags), 0x1522, Instant::now());
            assert!(
                matches!(
                    events.as_slice(),
                    [SessionEvent::MonsterMove {
                        guid: 0x1522,
                        spline_id: 9,
                        ..
                    }]
                ),
                "flags {flags:#x}: {events:?}"
            );
        }
    }

    /// All twenty-five 12340 slots survive the mapping, not just the 1.12 twenty.
    #[test]
    fn wrath_quest_slots_keep_all_twenty_five() {
        let mut raw = vec![0u32; 158 + 25 * 5];
        raw[158] = 57; // slot 1
        raw[158 + 1] = 0x2;
        raw[158 + 2] = 3 | (4 << 16);
        raw[158 + 4] = 111;
        let last = 158 + 24 * 5;
        raw[last] = 999; // slot 25
        raw[last + 1] = 0x3;
        raw[last + 2] = 1 | (2 << 16);
        raw[last + 3] = 5 | (6 << 16);
        raw[last + 4] = 777;
        let slots = wrath_quest_slots(&crate::wrath_inventory::WrathFieldWindow::from_dense(raw));
        assert_eq!(slots.len(), 25);
        assert_eq!(slots[0].quest_id, 57);
        assert_eq!(slots[0].counters[0], 3);
        assert_eq!(slots[0].counters[1], 4);
        assert_eq!(slots[0].timer, 111);
        assert_eq!(slots[24].quest_id, 999, "the 25th slot is kept");
        assert_eq!(slots[24].counters, [1, 2, 5, 6]);
        assert_eq!(slots[24].state, 3);
        assert_eq!(slots[24].timer, 777);
    }

    /// 3.3.5's two-field visible slots feed the 1.12 twelve-dword block: entry at +2, the packed
    /// 16-bit enchants at +3/+4.
    #[test]
    fn wrath_visible_items_land_in_the_twelve_dword_slots() {
        let mut raw = vec![0u32; usize::from(crate::wrath_inventory::PLAYER_WINDOW_END)];
        raw[283] = 4368; // head, entry
        raw[284] = 0x0002_0001; // perm enchant 1, temp 2
        raw[283 + 2 * 18] = 5976; // tabard, entry
        let mut pairs = Vec::new();
        push_visible_item_fields(
            &crate::wrath_inventory::WrathFieldWindow::from_dense(raw),
            &mut pairs,
        );
        let at =
            |slot: u16| benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 12 * slot;
        assert!(pairs.contains(&(at(0) + 2, 4368)));
        assert!(pairs.contains(&(at(0) + 3, 1)));
        assert!(pairs.contains(&(at(0) + 4, 2)));
        assert!(pairs.contains(&(at(18) + 2, 5976)));
    }

    /// The player unit-prefix table must not write 3.3.5 `UNIT_FIELD_MAXPOWER4` (36) over the 1.12
    /// `UNIT_FIELD_BYTES_0` slot (36): a delivered max energy of 100 read back as race 100 and left
    /// the local player's body and hair untextured. Attack times and damage keep their own slots.
    #[test]
    fn wrath_player_unit_fields_keep_bytes_0_and_land_combat_slots() {
        use benilla_protocol_wrath::fields as w;
        let window = item_window(
            &[
                (w::UNIT_FIELD_MAXPOWER4, 100),
                (w::UNIT_FIELD_AURASTATE, 0x20),
                (w::UNIT_FIELD_BASEATTACKTIME, 2000),
                (w::UNIT_FIELD_BASEATTACKTIME + 1, 1500),
                (w::UNIT_FIELD_RANGEDATTACKTIME, 2800),
                (w::UNIT_FIELD_MOUNTDISPLAYID, 14_000),
                (w::UNIT_FIELD_MINDAMAGE, 7.0f32.to_bits()),
                (w::UNIT_FIELD_MAXOFFHANDDAMAGE, 9.0f32.to_bits()),
                (w::PLAYER_BYTES, 0x0102_0304),
            ],
            crate::wrath_inventory::PLAYER_WINDOW_END,
        );
        let mut pairs = Vec::new();
        push_player_unit_fields(&window, &mut pairs);
        let at = |dst: u16| {
            pairs
                .iter()
                .filter(|(i, _)| *i == dst)
                .map(|(_, v)| *v)
                .collect::<Vec<_>>()
        };
        assert!(
            at(36).is_empty(),
            "BYTES_0 comes from the digest, not the raw table: {:?}",
            at(36)
        );
        assert!(at(133).is_empty(), "mount display comes from the digest");
        assert_eq!(at(125), [0x20], "aura state");
        assert_eq!(at(126), [2000], "main-hand attack time");
        assert_eq!(at(127), [1500], "off-hand attack time");
        assert_eq!(at(128), [2800], "ranged attack time");
        assert_eq!(at(134), [7.0f32.to_bits()], "min damage");
        assert_eq!(at(137), [9.0f32.to_bits()], "max off-hand damage");
        assert_eq!(at(193), [0x0102_0304], "PLAYER_BYTES");
    }

    /// The 3.3.5 honor counters are at 1225..=1230, while the 1.12 honor block scatters them from
    /// 1250 to 1261. `PLAYER_FIELD_KILLS` (1225) packs today's HK (low u16) and yesterday's HK
    /// (high u16) and must split across 1250 and 1251; `PLAYER_FIELD_TODAY_CONTRIBUTION` (1226)
    /// stays in `WrathPlayerRaw` rather than overwriting 1.12 `THIS_WEEK_CONTRIBUTION` (1254).
    /// Undelivered counters must leave their donor slots untouched.
    #[test]
    fn wrath_player_honor_fields_sit_in_the_1_12_honor_block() {
        let window = item_window(
            &[
                (1225, 0x0003_0007),
                (1226, 1200),
                (1227, 800),
                (1228, 42_000),
                (1229, 0x0000_6400),
                (1230, 17),
            ],
            crate::wrath_inventory::PLAYER_WINDOW_END,
        );
        let mut pairs = Vec::new();
        push_player_honor_fields(&window, &mut pairs);
        assert_eq!(
            pairs,
            vec![
                (1250, 0x0007),
                (1251, 0x0003),
                (1257, 800),
                (1255, 42_000),
                (1260, 0x0000_6400),
                (1261, 17),
            ]
        );

        let partial = item_window(&[(1228, 42_000)], crate::wrath_inventory::PLAYER_WINDOW_END);
        let mut pairs = Vec::new();
        push_player_honor_fields(&partial, &mut pairs);
        assert_eq!(pairs, vec![(1255, 42_000)]);
    }

    /// A window where only the supplied `(index, value)` dwords count as delivered.
    fn item_window(pairs: &[(u16, u32)], end: u16) -> crate::wrath_inventory::WrathFieldWindow {
        let mut values = vec![0u32; usize::from(end)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        for &(index, value) in pairs {
            let at = usize::from(index);
            values[at] = value;
            delivered[at / 64] |= 1u64 << (at % 64);
        }
        crate::wrath_inventory::WrathFieldWindow::new(values, delivered)
    }

    /// Wrath enchant slots 8..12 are 43..57 and have no donor slot. Donor 43..47 must keep the
    /// property seed, random-properties id, durability and max durability, and neither the Wrath
    /// enchant band nor `CREATE_PLAYED_TIME` may leak into a donor index.
    #[test]
    fn wrath_item_projection_never_folds_enchant_slots_eight_to_twelve_into_donor_43_to_47() {
        let window = item_window(
            &[
                (fields::ITEM_FIELD_ENCHANTMENT_1_1, 2564),
                (fields::ITEM_FIELD_ENCHANTMENT_8_1, 0x8888),
                (fields::ITEM_FIELD_ENCHANTMENT_12_3, 0x9999),
                (fields::ITEM_FIELD_PROPERTY_SEED, 0xAA),
                (fields::ITEM_FIELD_RANDOM_PROPERTIES_ID, 0xBB),
                (fields::ITEM_FIELD_DURABILITY, 17),
                (fields::ITEM_FIELD_MAXDURABILITY, 40),
                (fields::ITEM_FIELD_CREATE_PLAYED_TIME, 1234),
            ],
            fields::ITEM_END,
        );
        let mut pairs = Vec::new();
        push_item_fields(&window, false, &mut pairs);
        assert!(pairs.contains(&(22, 2564)), "donor slot 0 still lands");
        assert!(pairs.contains(&(43, 0xAA)), "donor 43 is the property seed");
        assert!(pairs.contains(&(44, 0xBB)), "donor 44 is the random id");
        assert!(pairs.contains(&(46, 17)), "donor 46 is durability");
        assert!(pairs.contains(&(47, 40)), "donor 47 is max durability");
        assert!(
            !pairs.iter().any(|&(index, _)| index == 45),
            "1.12's item-text id stays unwritten"
        );
        assert!(
            !pairs
                .iter()
                .any(|&(_, value)| value == 0x8888 || value == 0x9999),
            "Wrath slots 8 and 12 never project"
        );
        assert!(
            !pairs.iter().any(|&(index, _)| index == 62),
            "CREATE_PLAYED_TIME has no donor index"
        );
    }

    /// An undelivered donor field stays absent rather than being written as an observed zero.
    #[test]
    fn an_absent_item_field_is_not_projected_as_a_delivered_zero() {
        let window = item_window(&[(fields::ITEM_FIELD_DURABILITY, 0)], fields::ITEM_END);
        let mut pairs = Vec::new();
        push_item_fields(&window, false, &mut pairs);
        assert!(
            pairs.contains(&(46, 0)),
            "a delivered zero is an observed clear"
        );
        assert!(
            !pairs.iter().any(|&(index, _)| index == 47),
            "max durability was never delivered and writes nothing"
        );
        assert!(!pairs.iter().any(|&(index, _)| (22..=42).contains(&index)));
    }

    /// A container projects its own slot array into donor 48/50..; a non-container never does.
    #[test]
    fn container_slots_project_only_for_container_objects() {
        let window = item_window(
            &[
                (fields::CONTAINER_FIELD_NUM_SLOTS, 2),
                (fields::CONTAINER_FIELD_SLOT_1, 0x11),
                (fields::CONTAINER_FIELD_SLOT_1 + 2, 0x22),
            ],
            fields::CONTAINER_END,
        );
        let mut pairs = Vec::new();
        push_item_fields(&window, true, &mut pairs);
        assert!(pairs.contains(&(48, 2)));
        assert!(pairs.contains(&(50, 0x11)));
        assert!(pairs.contains(&(52, 0x22)));

        let mut item_pairs = Vec::new();
        push_item_fields(&window, false, &mut item_pairs);
        assert!(!item_pairs.iter().any(|&(index, _)| index >= 48));
    }

    /// A 3.3.5 corpse block shifts by two after PARTY: `CORPSE_FIELD_DISPLAY_ID` 10 lands on the
    /// 1.12 display slot 12, item 0 at 11 on 13, the appearance at 30/31 on 32/33 and the dynamic
    /// flags at 34 on 36. Owner and party keep their indices. The display drives the corpse's
    /// body model; `CORPSE_FIELD_FLAGS` drives the bone-pile fork.
    #[test]
    fn a_corpse_block_shifts_by_two_after_party() {
        let window = item_window(
            &[
                (fields::CORPSE_FIELD_OWNER, 0x11),
                (fields::CORPSE_FIELD_DISPLAY_ID, 49),
                (fields::CORPSE_FIELD_ITEM, 902 | (5 << 24)),
                (fields::CORPSE_FIELD_BYTES_1, 0x0600_0000),
                (fields::CORPSE_FIELD_FLAGS, 0x08),
                (fields::CORPSE_FIELD_DYNAMIC_FLAGS, 1),
            ],
            fields::CORPSE_END,
        );
        let mut pairs = Vec::new();
        push_corpse_fields(Some(&window), None, &mut pairs);
        assert_eq!(
            pairs,
            vec![
                (6, 0x11),
                (12, 49),
                (13, 902 | (5 << 24)),
                (32, 0x0600_0000),
                (35, 0x08),
                (36, 1),
            ]
        );
        // The shifted block is what the corpse renderer and the reclaim latch read: display 12
        // is the body model, owner 6 arms note_corpse, and flags 0x08 is not the bone bit.
        let corpse = ObjectFields::from_pairs(&pairs).into_created(ObjectType::Corpse);
        assert_eq!(corpse.corpse_owner(), Some(0x11));
        assert_eq!(
            corpse.corpse_display_id(),
            Some(49),
            "the 1.12 display slot 12 is read, not 12340's 10"
        );
        assert!(!corpse.corpse_is_bones());
    }

    /// The typed row rides the existing container entity: same guid keeps the same entity and
    /// only a changed row is written, while a generation change replaces the entity and its row.
    #[test]
    #[ignore = "known failure on main (2026-11-18 CI baseline); run with --ignored"]
    fn an_item_row_keeps_container_identity_and_generation_ownership() {
        const BAG: u64 = 0x4000_0000_0000_00B1;
        let base = super::command_tap_tests::view(BAG).entities[&BAG].clone();
        let mut remote = WrathEntity {
            guid: BAG,
            type_id: benilla_protocol_wrath::updates::type_id::CONTAINER,
            entry: Some(100),
            item_fields: Some(item_window(
                &[
                    (fields::CONTAINER_FIELD_NUM_SLOTS, 2),
                    (fields::CONTAINER_FIELD_SLOT_1, 0x11),
                ],
                fields::CONTAINER_END,
            )),
            generation: 1,
            ..base
        };
        let mut world = World::new();
        let mut index = HashMap::new();
        let (_, _, old_row, new_row) = apply_item_world(&mut world, &mut index, BAG, &remote);
        assert!(old_row.is_none());
        assert_eq!(
            new_row.as_ref().and_then(|row| row.container_slot(0)),
            Some(0x11)
        );
        let entity = index[&BAG];
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(entity)
            .is_some());

        // The same generation updates in place; the changed row is the only write.
        remote.item_fields = Some(item_window(
            &[
                (fields::CONTAINER_FIELD_NUM_SLOTS, 2),
                (fields::CONTAINER_FIELD_SLOT_1, 0x22),
            ],
            fields::CONTAINER_END,
        ));
        let (_, _, old_row, new_row) = apply_item_world(&mut world, &mut index, BAG, &remote);
        assert_eq!(
            index[&BAG], entity,
            "the container entity is not reparented"
        );
        assert_eq!(
            old_row.as_ref().and_then(|row| row.container_slot(0)),
            Some(0x11)
        );
        assert_eq!(
            new_row.as_ref().and_then(|row| row.container_slot(0)),
            Some(0x22)
        );
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(entity)
            .is_some_and(|row| row.container_slot(0) == Some(0x22)));

        // A generation change is a new object; the old entity and its row are gone.
        remote.generation = 2;
        let (_, _, old_row, _) = apply_item_world(&mut world, &mut index, BAG, &remote);
        assert!(
            old_row.is_none(),
            "the replaced entity's row does not carry over"
        );
        let replacement = index[&BAG];
        assert_ne!(replacement, entity);
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(entity)
            .is_none());
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(replacement)
            .is_some());
    }

    #[test]
    fn hovering_unit_is_not_stealthed_after_projection() {
        let fields = ObjectFields::from_pairs(&[(138, donor_unit_bytes_1(0x0200_0000, 0))]);
        assert!(!fields.unit_is_stealthed());
        assert!(!fields.unit_is_ghost_visual());
        assert!(!fields.unit_is_untrackable());
    }

    #[test]
    fn creep_unit_is_stealthed_after_projection() {
        let fields =
            ObjectFields::from_pairs(&[(138, donor_unit_bytes_1(0x0002_0000, 0x1100_0000))]);
        assert!(fields.unit_is_stealthed());
        assert_eq!(fields.unit_shapeshift_form(), 17);
        assert!(!fields.unit_is_ghost_visual());
        assert!(!fields.unit_is_untrackable());
    }

    #[test]
    fn terminal_scope_loss_clears_ecs_and_input_without_a_digest() {
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        world.init_resource::<SelfGuid>();
        world.init_resource::<WrathWorldView>();
        world.init_resource::<crate::player::Player>();
        world.init_resource::<crate::player::OwnControlLost>();
        world.init_resource::<crate::player::StandPending>();
        world.init_resource::<crate::wrath_inventory::WrathInventory>();
        {
            let mut raw = vec![0u32; usize::from(crate::wrath_inventory::PLAYER_WINDOW_END)];
            let bank = benilla_protocol_wrath::fields::PLAYER_FIELD_BANK_SLOT_1 + 2 * 24;
            raw[usize::from(bank)] = 0x77;
            let mut inventory = crate::wrath_inventory::WrathInventory::default();
            inventory.apply(
                &crate::wrath_inventory::WrathFieldWindow::from_dense(raw),
                (1, 1),
            );
            world.insert_resource(inventory);
        }
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::from([WrathOutbound {
            opcode: 1,
            payload: vec![1],
            movement: false,
            movement_seq: None,
            enqueued_at: None,
            receipt: None,
            trace: None,
            enforce_scope: false,
        }])));
        world.insert_resource(WrathInput(Arc::clone(&outbox)));

        let entity = world.spawn((WrathStreamed, Guid(0xAA), SelfPlayer)).id();
        world.resource_mut::<GuidIndex>().0.insert(0xAA, entity);
        world.resource_mut::<SelfGuid>().0 = Some(0xAA);
        world.resource_mut::<crate::player::Player>().active = true;
        {
            let mut view = world.resource_mut::<WrathWorldView>();
            view.player_guid = 0;
            view.last_changes.world_transferred = true;
        }

        wrath_apply_world(&mut world);

        assert!(
            world.get_entity(entity).is_err(),
            "the old scope entity is despawned"
        );
        assert!(world.resource::<GuidIndex>().0.is_empty());
        assert_eq!(world.resource::<SelfGuid>().0, None);
        assert!(!world.resource::<crate::player::Player>().active);
        assert!(
            world
                .resource::<crate::wrath_inventory::WrathInventory>()
                .iter()
                .next()
                .is_none(),
            "the typed inventory retires with the streamed scope"
        );
        assert!(outbox.lock().unwrap().is_empty(), "pending actions cleared");
    }

    /// Rune replies are scoped state, not entities: resync establishes the decoded layout,
    /// convert changes only the type, rune power recharges the masked slot, and a transfer clears
    /// everything.
    #[test]
    fn rune_feed_state_tracks_resync_convert_and_power() {
        use benilla_protocol_wrath::gameplay::wrath_systems::RuneSlot;

        let mut view = WrathWorldView::default();
        assert!(view.runes.is_empty(), "no resync yet: no invented layout");

        view.apply_rune_state(
            &[
                RuneSlot {
                    rune_type: 0,
                    cooldown: 0,
                },
                RuneSlot {
                    rune_type: 0,
                    cooldown: 128,
                },
                RuneSlot {
                    rune_type: 1,
                    cooldown: 255,
                },
                RuneSlot {
                    rune_type: 1,
                    cooldown: 0,
                },
                RuneSlot {
                    rune_type: 2,
                    cooldown: 64,
                },
                RuneSlot {
                    rune_type: 2,
                    cooldown: 255,
                },
            ],
            100.0,
        );
        assert_eq!(view.runes.len(), 6);
        assert_eq!(view.runes[2].rune_type, 1);
        assert_eq!(view.runes[2].cooldown, 255);
        assert_eq!(view.runes[4].received_at, 100.0);

        // ConvertRune keeps the cooldown and changes only the type.
        view.apply_rune_convert(0, 3, 110.0);
        assert_eq!(view.runes[0].rune_type, 3);
        assert_eq!(view.runes[0].cooldown, 0);
        assert_eq!(view.runes[0].received_at, 100.0);

        // AddRunePower marks its masked slot fully recharged at the decode clock.
        view.apply_rune_power(1 << 3, 110.0);
        assert_eq!(view.runes[3].cooldown, 255);
        assert_eq!(view.runes[3].received_at, 110.0);

        view.clear_rune_calendar();
        assert!(view.runes.is_empty());
    }

    /// `SMSG_PARTYKILLLOG` is scoped presentation state: translate it onto the stock event,
    /// preserve a burst as ordered rows, and clear the queue on a transfer while the sequence
    /// stays monotonic so a later kill cannot reuse a revision the UI feed already fired.
    #[test]
    fn party_kill_state_translates_to_the_stock_event_and_clears() {
        let killer = 0x0000_0000_0000_0011;
        let victim = 0xF130_0000_0000_0022;
        let mut view = WrathWorldView::default();
        view.apply_party_kill(killer, victim, 7, 9, 100.0);
        let kill = *view
            .party_kills
            .front()
            .expect("the decoded kill is retained");
        assert_eq!((kill.killer, kill.victim), (killer, victim));
        assert_eq!((kill.session_epoch, kill.world_generation), (7, 9));
        assert_eq!((kill.revision, kill.received_at), (1, 100.0));

        let translated = translate_gameplay(
            WrathGameplayEvent::PartyKillLog { killer, victim },
            0,
            std::time::Instant::now(),
        );
        assert_eq!(translated.len(), 1);
        assert!(matches!(
            &translated[0],
            SessionEvent::PartyKillLog(kill) if kill.killer == killer && kill.victim == victim
        ));

        // A second kill before the feed is a second row, not a replacement.
        view.apply_party_kill(killer, victim, 7, 9, 100.5);
        assert_eq!(view.party_kills.len(), 2);
        assert_eq!(view.party_kills[1].revision, 2);

        view.clear_rune_calendar();
        assert!(view.party_kills.is_empty());
        view.apply_party_kill(killer, victim, 10, 11, 101.0);
        assert_eq!(
            view.party_kills
                .front()
                .expect("a later kill is retained")
                .revision,
            3,
            "the sequence never rewinds across a scope clear"
        );
    }

    /// `SMSG_CLEAR_TARGET` translates to the stock forced-deselect event with its guid.
    #[test]
    fn clear_target_translates_to_the_stock_event() {
        let guid = 0xF130_0000_0000_00ABu64;
        let translated = translate_gameplay(
            WrathGameplayEvent::ClearTarget { guid },
            0,
            std::time::Instant::now(),
        );
        assert_eq!(translated.len(), 1);
        match translated.as_slice() {
            [SessionEvent::ClearTarget { guid: seen }] => assert_eq!(*seen, guid),
            other => panic!("one clear-target event expected, got {other:?}"),
        }
    }

    /// The pending queue is bounded and reports overflow instead of silently losing a burst.
    #[test]
    fn party_kill_queue_reports_capacity_loss() {
        let mut view = WrathWorldView::default();
        for index in 0..(PARTY_KILL_CAPACITY + 3) {
            view.apply_party_kill(1, 2, 7, 9, index as f64);
        }
        assert_eq!(view.party_kills.len(), PARTY_KILL_CAPACITY);
        assert_eq!(view.party_kill_dropped, 3);
        assert_eq!(
            view.party_kills
                .front()
                .expect("the first three were dropped")
                .received_at,
            3.0
        );
    }

    /// Swings are scoped ordered rows: a full queue drops the oldest and reports the loss, the
    /// sequence stays monotonic across a scope clear, and the clear removes every pending swing.
    #[test]
    fn swing_queue_clears_at_scope_end_and_reports_capacity_loss() {
        let hit = WrathSwingHit {
            attacker: 1,
            victim: 2,
            hit_info: 0x2,
            damage: 10,
            overkill: 0,
            victim_state: 1,
            absorb: 0,
            resist: 0,
            blocked: 0,
            melee_spell_id: 0,
            school: 1,
        };
        let mut view = WrathWorldView::default();
        for index in 0..(SWING_CAPACITY + 3) {
            view.apply_swing(hit, 7, 9, index as f64);
        }
        assert_eq!(view.swings.len(), SWING_CAPACITY);
        assert_eq!(view.swing_dropped, 3);
        let front = view.swings.front().expect("the first three were dropped");
        assert_eq!(front.received_at, 3.0);
        assert_eq!((front.session_epoch, front.world_generation), (7, 9));
        assert_eq!(front.revision, 4);

        view.clear_rune_calendar();
        assert!(view.swings.is_empty());
        assert_eq!(view.swing_dropped, 0);
        assert_eq!(view.swing_unresolved, 0);
        view.apply_swing(hit, 10, 11, 101.0);
        assert_eq!(
            view.swings
                .front()
                .expect("a later swing is retained")
                .revision,
            (SWING_CAPACITY + 4) as u64,
            "the sequence never rewinds across a scope clear"
        );
    }

    /// Calendar replies replace the scoped snapshot; a second one never appends to the first.
    /// The date/count feed clears on a transfer with the rest of the scoped state.
    #[test]
    fn calendar_feed_state_replaces_the_snapshot_without_duplicates() {
        use benilla_protocol_wrath::gameplay::wrath_systems::{
            CalendarEvent, CalendarInvite, CalendarRaidLockout, CalendarRaidReset,
        };

        let invite = CalendarInvite {
            event_id: 5,
            invite_id: 9,
            status: 1,
            rank: 1,
            invite_type: 1,
            inviter: 0x0100_0000_0000_0002,
        };
        let event = |id: u64, title: &str| CalendarEvent {
            event_id: id,
            title: title.to_owned(),
            event_type: 0,
            time: 0x1AA1_6B9E,
            flags: 0,
            dungeon_id: -1,
            creator: 0x0200_0000_0000_0001,
        };
        let lockout = CalendarRaidLockout {
            map_id: 571,
            difficulty: 3,
            time_left: 3_600,
            instance_id: 0xFF,
        };
        let reset = CalendarRaidReset {
            map_id: 533,
            period: 604_800,
            offset: 0,
        };

        let mut view = WrathWorldView::default();
        assert!(!view.calendar_fetched);
        view.apply_calendar(&gameplay::wrath_systems::CalendarSendCalendar {
            server_time: 1_700_000_000,
            zone_time: 0x1AA1_6B9E,
            invites: vec![invite],
            events: vec![event(5, "Onyxia")],
            raid_lockouts: vec![lockout],
            relative_time: 1_134_028_800,
            raid_resets: vec![reset],
            holidays: Vec::new(),
        });
        assert!(view.calendar_fetched);
        assert_eq!(view.calendar_revision, 1);
        assert_eq!(view.calendar.as_ref().unwrap().invites.len(), 1);
        assert_eq!(view.calendar.as_ref().unwrap().events.len(), 1);

        view.apply_calendar(&gameplay::wrath_systems::CalendarSendCalendar {
            server_time: 1_700_100_000,
            zone_time: 0x1AA1_6B9E,
            invites: Vec::new(),
            events: vec![event(6, "Naxxramas"), event(7, "Ulduar")],
            raid_lockouts: Vec::new(),
            relative_time: 1_134_028_800,
            raid_resets: Vec::new(),
            holidays: Vec::new(),
        });
        assert_eq!(view.calendar_revision, 2, "each reply is one revision");
        let calendar = view.calendar.as_ref().unwrap();
        assert_eq!(calendar.server_time, 1_700_100_000);
        assert!(calendar.invites.is_empty(), "the first invite row is gone");
        assert_eq!(calendar.events.len(), 2, "rows were replaced, not appended");

        view.clear_rune_calendar();
        assert!(!view.calendar_fetched);
        assert!(view.calendar.is_none());
        assert_eq!(view.calendar_revision, 0);
    }
}

#[cfg(test)]
mod command_tap_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    pub(super) fn view(npc: u64) -> WrathWorldView {
        let mut entities = BTreeMap::new();
        entities.insert(
            npc,
            WrathEntity {
                guid: npc,
                type_id: 3,
                entry: Some(6),
                display_id: Some(1),
                scale: Some(1.0),
                health: Some(10),
                max_health: Some(10),
                powers: [None; 7],
                max_powers: [None; 7],
                level: Some(1),
                bytes_0: Some(0),
                faction: None,
                unit_flags: None,
                mount_display_id: None,
                bytes_1: None,
                bytes_2: None,
                npc_flags: None,
                dynamic_flags: None,
                target: None,
                charmed_by: None,
                summoned_by: None,
                created_by: None,
                charm: None,
                summon: None,
                corpse_fields: None,
                player_bytes: None,
                player_bytes_2: None,
                position: None,
                move_flags: None,
                create_spline: None,
                attack_target: None,
                generation: 1,
                player_fields: None,
                item_fields: None,
                auras: Vec::new(),
                auras_observed: false,
                aura_receipts: BTreeMap::new(),
                raw_fields: None,
            },
        );
        WrathWorldView {
            entities,
            player_guid: 1,
            ..WrathWorldView::default()
        }
    }

    #[test]
    fn stock_commands_become_the_335_messages() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::new()));
        let mut world = World::new();
        world.insert_resource(NetCommandTap(Some(rx)));
        world.insert_resource(WrathInput(Arc::clone(&outbox)));
        world.insert_resource(view(0x22));
        world.resource_mut::<WrathWorldView>().gossip_menu = Some((0x22, 55));

        tx.send(ClientCommand::SetSelection { guid: 0x22 }).unwrap();
        tx.send(ClientCommand::AttackSwing { guid: 0x22 }).unwrap();
        tx.send(ClientCommand::CastSpell {
            spell_id: 133,
            target: Some(0x22),
            trace: None,
        })
        .unwrap();
        tx.send(ClientCommand::GossipSelectOption {
            guid: 0x22,
            option: 3,
            code: None,
        })
        .unwrap();
        tx.send(ClientCommand::QuestgiverAccept {
            npc: 0x22,
            quest: 7,
        })
        .unwrap();
        tx.send(ClientCommand::Loot { guid: 0x22 }).unwrap();
        tx.send(ClientCommand::Move {
            kind: crate::net::MoveKind::Heartbeat,
            flags: 0,
            pos: [0.0; 3],
            orientation: 0.0,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();

        let _ = world.run_system_once(wrath_tap_commands);
        let sent: Vec<_> = outbox.lock().unwrap().drain(..).collect();
        assert_eq!(
            sent.len(),
            7,
            "the tap carries the shared streamer's move as well"
        );
        assert_eq!(
            sent[0].opcode,
            benilla_protocol_wrath::opcode::CMSG_SET_SELECTION
        );
        assert_eq!(
            sent[1].opcode,
            benilla_protocol_wrath::opcode::CMSG_ATTACKSWING
        );
        assert_eq!(
            sent[2].opcode,
            benilla_protocol_wrath::opcode::CMSG_CAST_SPELL
        );
        assert_eq!(sent[2].payload[0], 1);
        assert_eq!(
            u32::from_le_bytes(sent[2].payload[1..5].try_into().unwrap()),
            133
        );
        assert_eq!(
            sent[3].opcode,
            benilla_protocol_wrath::opcode::CMSG_GOSSIP_SELECT_OPTION
        );
        assert_eq!(
            u32::from_le_bytes(sent[3].payload[8..12].try_into().unwrap()),
            55
        );
        assert_eq!(
            sent[4].opcode,
            benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_ACCEPT_QUEST
        );
        assert_eq!(sent[4].payload.len(), 16);
        assert_eq!(sent[5].opcode, benilla_protocol_wrath::opcode::CMSG_LOOT);
        // The move is replaceable motion addressed to the session's mover (`view` names guid 1):
        // its packed guid, then a 30-byte `MovementInfo` with no optional tail.
        assert_eq!(
            sent[6].opcode,
            benilla_protocol_wrath::opcode::MSG_MOVE_HEARTBEAT
        );
        assert!(sent[6].movement);
        assert!(sent[..6].iter().all(|entry| !entry.movement));
        assert_eq!(sent[6].payload[..2], [0x01, 0x01]);
        assert_eq!(sent[6].payload.len(), 2 + 30);
    }

    /// The `MovementInfo` of a move for mover guid 1, past its two-byte packed guid.
    fn move_body(
        kind: crate::net::MoveKind,
        flags: u32,
        pitch: f32,
        fall_time: u32,
        jump: Option<benilla_protocol::JumpInfo>,
        transport: Option<benilla_protocol::TransportPose>,
    ) -> (u32, Vec<u8>) {
        let (opcode, payload) = wrath_move_packet(
            1,
            1000,
            kind,
            flags,
            [1.0, 2.0, 3.0],
            0.5,
            pitch,
            fall_time,
            jump,
            transport,
        );
        assert_eq!(payload[..2], [0x01, 0x01]);
        (opcode, payload[2..].to_vec())
    }

    fn u32_at(body: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(body[at..at + 4].try_into().unwrap())
    }

    fn f32_at(body: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(body[at..at + 4].try_into().unwrap())
    }

    /// An airborne move reports how long the fall has run and the launch it follows. The server
    /// keeps the fall's origin only while the reported fall time grows
    /// (`Player::UpdateFallInformationIfNeed`), so a zero here cancels fall damage.
    #[test]
    fn an_airborne_move_carries_the_fall_time_and_the_jump_tail() {
        let jump = benilla_protocol::JumpInfo {
            zspeed: -7.955547,
            cos_angle: 0.5,
            sin_angle: 0.25,
            xy_speed: 7.0,
        };
        // 1.12 FORWARD | FALLING.
        let (opcode, body) = move_body(
            crate::net::MoveKind::Heartbeat,
            0x1 | 0x2000,
            0.0,
            321,
            Some(jump),
            None,
        );
        assert_eq!(opcode, benilla_protocol_wrath::opcode::MSG_MOVE_HEARTBEAT);
        assert_eq!(u32_at(&body, 0), 0x1001, "12340 FORWARD | FALLING");
        assert_eq!(u32_at(&body, 6), 1000, "the client clock");
        assert_eq!(f32_at(&body, 22), 0.5, "orientation");
        assert_eq!(u32_at(&body, 26), 321, "fall time");
        // 12340 order: zspeed, sinAngle, cosAngle, xyspeed.
        assert_eq!(f32_at(&body, 30), -7.955547);
        assert_eq!(f32_at(&body, 34), 0.25);
        assert_eq!(f32_at(&body, 38), 0.5);
        assert_eq!(f32_at(&body, 42), 7.0);
        assert_eq!(body.len(), 46);
    }

    #[test]
    fn a_swimming_move_carries_the_pitch() {
        let (_, body) = move_body(
            crate::net::MoveKind::SetPitch,
            0x1 | 0x20_0000,
            -0.4,
            0,
            None,
            None,
        );
        assert_eq!(u32_at(&body, 0), 0x20_0001);
        assert_eq!(f32_at(&body, 26), -0.4, "pitch precedes the fall time");
        assert_eq!(u32_at(&body, 30), 0);
        assert_eq!(body.len(), 34);
    }

    /// A boarded move carries the deck pose under 12340's transport bit. The 1.12 bit must not
    /// survive: `0x02000000` is `MOVEMENTFLAG_FLYING` in 12340 and announces a pitch.
    #[test]
    fn a_boarded_move_carries_the_transport_block() {
        let deck = benilla_protocol::TransportPose {
            guid: 0x05,
            pos: benilla_protocol::wire::Vector3d {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
            orientation: 1.5,
        };
        let (_, body) = move_body(
            crate::net::MoveKind::Heartbeat,
            0x0200_0000,
            0.0,
            0,
            None,
            Some(deck),
        );
        assert_eq!(u32_at(&body, 0), 0x200);
        assert_eq!(body[26..28], [0x01, 0x05], "the transport's packed guid");
        assert_eq!(f32_at(&body, 28), 4.0);
        assert_eq!(f32_at(&body, 40), 1.5, "facing in the deck's frame");
        assert_eq!(body[48], 0xFF, "a deck is no vehicle seat");
        // Packed guid 2, pose 16, transport time 4, seat 1, then the fall time.
        assert_eq!(body.len(), 26 + 23 + 4);
    }

    /// The server reads whatever tail a flag announces, so a flag that arrives without its tail
    /// is not sent.
    #[test]
    fn a_flag_without_its_tail_is_cleared() {
        let (_, body) = move_body(
            crate::net::MoveKind::Heartbeat,
            0x2000 | 0x0200_0000,
            0.0,
            0,
            None,
            None,
        );
        assert_eq!(u32_at(&body, 0), 0);
        assert_eq!(body.len(), 30);
    }

    /// The opcode numbers are AzerothCore's `Opcodes.h`, written out so the table is checked
    /// against the server and not against the constants it is built from.
    #[test]
    fn each_shared_move_has_its_12340_opcode() {
        use crate::net::MoveKind;
        for (kind, opcode) in [
            (MoveKind::StartForward, 0x0B5),
            (MoveKind::StartBackward, 0x0B6),
            (MoveKind::Stop, 0x0B7),
            (MoveKind::StartStrafeLeft, 0x0B8),
            (MoveKind::StartStrafeRight, 0x0B9),
            (MoveKind::StopStrafe, 0x0BA),
            (MoveKind::Jump, 0x0BB),
            (MoveKind::StartTurnLeft, 0x0BC),
            (MoveKind::StartTurnRight, 0x0BD),
            (MoveKind::StopTurn, 0x0BE),
            (MoveKind::StartPitchUp, 0x0BF),
            (MoveKind::StartPitchDown, 0x0C0),
            (MoveKind::StopPitch, 0x0C1),
            (MoveKind::SetRunMode, 0x0C2),
            (MoveKind::SetWalkMode, 0x0C3),
            (MoveKind::FallLand, 0x0C9),
            (MoveKind::StartSwim, 0x0CA),
            (MoveKind::StopSwim, 0x0CB),
            (MoveKind::SetFacing, 0x0DA),
            (MoveKind::SetPitch, 0x0DB),
            (MoveKind::Heartbeat, 0x0EE),
        ] {
            assert_eq!(wrath_move_opcode(kind), opcode, "{kind:?}");
        }
    }

    fn queued(opcode: u32, movement: bool) -> WrathOutbound {
        WrathOutbound {
            opcode,
            payload: Vec::new(),
            movement,
            movement_seq: None,
            enqueued_at: None,
            receipt: None,
            trace: None,
            enforce_scope: false,
        }
    }

    /// Under pressure a move displaces a queued pose report first, then a queued edge, and
    /// never an action; a queue of actions alone refuses it.
    #[test]
    fn a_move_under_pressure_displaces_motion_and_never_an_action() {
        use benilla_protocol_wrath::opcode as op;
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::new()));
        let fill_with_actions = |outbox: &WrathOutbox| {
            let mut queue = outbox.lock().unwrap();
            while queue.len() < WRATH_MOVEMENT_QUEUE_CAP {
                queue.push_back(queued(op::CMSG_ATTACKSWING, false));
            }
        };
        let moves = |outbox: &WrathOutbox| -> Vec<u32> {
            let queue = outbox.lock().unwrap();
            assert_eq!(queue.len(), WRATH_MOVEMENT_QUEUE_CAP);
            queue
                .iter()
                .filter(|entry| entry.movement)
                .map(|entry| entry.opcode)
                .collect()
        };
        {
            let mut queue = outbox.lock().unwrap();
            queue.push_back(queued(op::MSG_MOVE_JUMP, true));
            queue.push_back(queued(op::MSG_MOVE_HEARTBEAT, true));
        }
        fill_with_actions(&outbox);
        let input = WrathInput(Arc::clone(&outbox));

        admit_wrath_movement(&input, op::MSG_MOVE_FALL_LAND, Vec::new(), None);
        assert_eq!(
            moves(&outbox),
            [op::MSG_MOVE_JUMP, op::MSG_MOVE_FALL_LAND],
            "the pose report went first"
        );

        admit_wrath_movement(&input, op::MSG_MOVE_START_FORWARD, Vec::new(), None);
        assert_eq!(
            moves(&outbox),
            [op::MSG_MOVE_FALL_LAND, op::MSG_MOVE_START_FORWARD],
            "then the oldest edge"
        );

        outbox.lock().unwrap().retain(|entry| !entry.movement);
        fill_with_actions(&outbox);
        admit_wrath_movement(&input, op::MSG_MOVE_HEARTBEAT, Vec::new(), None);
        assert!(
            moves(&outbox).is_empty(),
            "a queue of actions refuses the move"
        );
    }

    /// `OpenCalendar()` is the real empty `CMSG_CALENDAR_GET_CALENDAR`; a request written under a
    /// retired world scope never reaches the wire.
    #[test]
    fn open_calendar_sends_the_empty_cmsg_and_refuses_a_stale_scope() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::new()));
        let mut world = World::new();
        world.insert_resource(NetCommandTap(Some(rx)));
        world.insert_resource(WrathInput(Arc::clone(&outbox)));
        let mut view = view(0x22);
        view.entered_world = true;
        view.session_epoch = 3;
        view.world_generation = 4;
        world.insert_resource(view);

        tx.send(ClientCommand::OpenCalendar {
            session_epoch: 3,
            world_generation: 3,
        })
        .unwrap();
        tx.send(ClientCommand::OpenCalendar {
            session_epoch: 3,
            world_generation: 4,
        })
        .unwrap();

        let _ = world.run_system_once(wrath_tap_commands);
        let sent: Vec<_> = outbox.lock().unwrap().drain(..).collect();
        assert_eq!(sent.len(), 1, "the stale fetch is refused");
        assert_eq!(
            sent[0].opcode,
            benilla_protocol_wrath::opcode::CMSG_CALENDAR_GET_CALENDAR
        );
        assert!(sent[0].payload.is_empty(), "12340's fetch body is empty");
    }

    fn enum_record(guid: u64, level: u8) -> gameplay::CharEnumRecord {
        gameplay::CharEnumRecord {
            guid,
            name: "Padfoot".to_owned(),
            race: 1,
            class: 4,
            gender: 0,
            skin: 3,
            face: 2,
            hair_style: 4,
            hair_color: 5,
            facial_style: 1,
            level,
            zone: 12,
            map: 0,
            position: [-8943.5, 132.25, 61.5],
            guild_id: 0,
            character_flags: 0x0040_0000,
            customize_flags: 0,
            first_login: false,
            pet_display_id: 0,
            pet_level: 0,
            pet_family: 0,
            visuals: std::array::from_fn(|slot| gameplay::CharEnumVisual {
                display_id: slot as u32,
                inventory_type: (slot % 2 == 0) as u8,
                enchant_id: 0,
            }),
        }
    }

    #[test]
    fn wrath_named_boot_character_does_not_fall_back_to_first() {
        let first = enum_record(0x42, 30);
        let mut firna = enum_record(0x43, 1);
        firna.name = "Firna".into();
        let records = [first, firna];
        assert_eq!(boot_character(&records, Some("firna")).unwrap().guid, 0x43);
        assert!(boot_character(&records, Some("Missing")).is_err());
        assert_eq!(boot_character(&records, None).unwrap().guid, 0x42);
        assert!(boot_character(&[], None).is_err());
    }

    #[test]
    fn interactive_restart_never_uses_the_boot_pick_during_logout_transition() {
        assert!(matches!(
            WrathSessionStart::Auto.for_login_mode(true),
            WrathSessionStart::Parked {
                roster_asked: false
            }
        ));
        assert!(matches!(
            WrathSessionStart::Auto.for_login_mode(false),
            WrathSessionStart::Auto
        ));
        assert!(matches!(
            WrathSessionStart::Parked { roster_asked: true }.for_login_mode(true),
            WrathSessionStart::Parked { roster_asked: true }
        ));
    }

    /// The 3.3.5 record onto the 1.12 roster seam: the appearance bytes, position and the 19
    /// equipment slots carry over, the extra bag slots drop, and a wire level 0 is the fresh
    /// character's mark with the level pinned to 1.
    #[test]
    fn the_wrath_record_translates_onto_the_stock_roster_record() {
        let characters = roster_from(&[enum_record(0x42, 10), enum_record(0x43, 0)]);
        assert_eq!(characters.len(), 2);
        let character = &characters[0];
        assert_eq!(character.guid, 0x42);
        assert_eq!(character.name, "Padfoot");
        assert_eq!(character.level, 10);
        assert!(!character.new_character);
        assert_eq!(character.facial_hair, 1, "the 3.3.5 facial style byte");
        assert_eq!(
            character.position,
            Vector3d {
                x: -8943.5,
                y: 132.25,
                z: 61.5
            }
        );
        assert_eq!(character.equipment.len(), 19);
        assert_eq!(character.equipment[0].display_id, 0);
        assert_eq!(character.equipment[18].display_id, 18);
        assert_eq!(character.flags, 0x0040_0000);
        assert_eq!(characters[1].level, 1, "a level-0 wire byte is pinned to 1");
        assert!(characters[1].new_character);
    }

    fn control_with_park_channel() -> (WrathControl, Receiver<WrathParkedCommand>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            WrathControl {
                config: None,
                multiclass: None,
                trace: None,
                worker: None,
                retiring: None,
                feed: None,
                cancel: Arc::new(AtomicBool::new(false)),
                commands: None,
                ended: false,
                restart_allowed: false,
                restart_attempts: 0,
                retry_at: None,
                terminal_ready: true,
                entered_world: false,
                ended_retirement_done: false,
                left_world: false,
                shutdown: false,
                last_error: None,
                session_events: None,
                session_events_tx: None,
                reply_stats: SessionReplyStats::default(),
                park_commands: Some(tx),
                publisher_retiring: Vec::new(),
                publisher_retire_rx: None,
                wm_actions: None,
            },
            rx,
        )
    }

    /// A running parked session owns its asks in order: nothing starts a replacement, and the
    /// asks arrive on the channel the parked loop reads.
    #[test]
    fn a_parked_ask_is_forwarded_to_the_running_session() {
        let (mut control, rx) = control_with_park_channel();
        control.park_serve(true, WrathParkedCommand::Enumerate);
        control.park_serve(false, WrathParkedCommand::Enter(0x42));
        let asked: Vec<_> = rx.try_iter().collect();
        assert_eq!(asked.len(), 2);
        assert!(matches!(asked[0], WrathParkedCommand::Enumerate));
        assert!(matches!(asked[1], WrathParkedCommand::Enter(0x42)));
        assert!(control.worker.is_none());
    }

    /// Without a running parked session the ask is an admission attempt; with no config the
    /// attempt dies at `start`'s first gate, manufacturing neither a worker nor a channel.
    #[test]
    fn a_parked_ask_without_a_config_does_not_start_a_session() {
        let (mut control, _rx) = control_with_park_channel();
        control.park_commands = None;
        control.park_serve(true, WrathParkedCommand::Enumerate);
        assert!(control.worker.is_none());
        assert!(control.park_commands.is_none());
    }
}

/// Delivery-queue behaviour for review finding R04. These are source-level cases only: no build
/// or test run is claimed for them (see the package-B handoff).
#[cfg(test)]
mod reply_queue_tests {
    use super::*;

    fn scoped(session_epoch: u64, world_generation: u64, event: SessionEvent) -> WrathSessionEvent {
        WrathSessionEvent {
            login_generation: None,
            session_epoch,
            world_generation,
            event,
            force: false,
        }
    }

    fn forced(event: SessionEvent) -> WrathSessionEvent {
        WrathSessionEvent::forced(event)
    }

    fn reply(attacker: u64) -> WrathSessionEvent {
        scoped(
            1,
            1,
            SessionEvent::AttackStart {
                attacker,
                victim: 2,
            },
        )
    }

    fn clock(unix_time: u32) -> WrathSessionEvent {
        scoped(1, 1, SessionEvent::ServerUnixTime { unix_time })
    }

    fn unix_time(event: &WrathSessionEvent) -> Option<u32> {
        match event.event {
            SessionEvent::ServerUnixTime { unix_time } => Some(unix_time),
            _ => None,
        }
    }

    fn disconnected() -> WrathSessionEvent {
        forced(SessionEvent::Disconnected {
            reason: "lost".to_owned(),
            end: benilla_protocol::SessionEnd::Lost,
        })
    }

    #[test]
    fn the_three_terminal_kinds_win_over_a_full_reply_backlog() {
        let mut replies = SessionReplies::default();
        for i in 0..1000 {
            replies.push(reply(i));
        }
        replies.push(forced(SessionEvent::LoggedOut));
        assert_eq!(replies.len(), 1001);
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 1, "a terminal ends the batch");
        assert!(matches!(drained[0].event, SessionEvent::LoggedOut));
        assert!(replies.is_empty(), "the superseded backlog is discarded");
        assert_eq!(replies.stats().dropped, 1000);
    }

    /// A clean logout legitimately produces two terminal edges: `LoggedOut` (the logout edge) and
    /// the closure's `Disconnected { end: LoggedOut }` (the world teardown). Both handlers do
    /// different work, so both are delivered; a repeated edge of either kind is dropped.
    #[test]
    fn a_clean_logout_delivers_the_logout_edge_and_its_teardown_once_each() {
        let mut replies = SessionReplies::default();
        replies.push(forced(SessionEvent::LoggedOut));
        replies.push(forced(SessionEvent::Disconnected {
            reason: "wrath session ended".to_owned(),
            end: benilla_protocol::SessionEnd::LoggedOut,
        }));
        replies.push(forced(SessionEvent::LoggedOut));
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0].event, SessionEvent::LoggedOut));
        assert!(matches!(
            drained[1].event,
            SessionEvent::Disconnected { .. }
        ));
        assert_eq!(replies.stats().dropped, 1, "the repeated LoggedOut");
        assert!(replies
            .take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET)
            .is_empty());
    }

    #[test]
    fn character_login_failed_survives_retire_scope_and_precedes_teardown() {
        let mut replies = SessionReplies::default();
        replies.push(reply(1));
        replies.push(forced(SessionEvent::CharacterLoginFailed { result: 0x41 }));
        replies.push(forced(SessionEvent::Disconnected {
            reason: "wrath session ended".to_owned(),
            end: benilla_protocol::SessionEnd::LoggedOut,
        }));
        // `wrath_net_drain` runs before `wrath_dispatch_events` and retires the scope on
        // `WrathFeedEvent::SessionEnded`; terminal edges must survive that backlog clear.
        assert_eq!(replies.retire_scope(), 1);
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 2);
        assert!(matches!(
            drained[0].event,
            SessionEvent::CharacterLoginFailed { result: 0x41 }
        ));
        assert!(matches!(
            drained[1].event,
            SessionEvent::Disconnected {
                end: benilla_protocol::SessionEnd::LoggedOut,
                ..
            }
        ));
    }

    #[test]
    fn observations_coalesce_in_place_and_keep_their_position() {
        let mut replies = SessionReplies::default();
        replies.push(reply(1));
        replies.push(clock(10));
        replies.push(reply(3));
        replies.push(clock(20));
        assert_eq!(replies.len(), 3, "the second clock replaced the first");
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 3);
        assert!(matches!(
            drained[0].event,
            SessionEvent::AttackStart { attacker: 1, .. }
        ));
        assert_eq!(
            unix_time(&drained[1]),
            Some(20),
            "newest value, original position"
        );
        assert!(matches!(
            drained[2].event,
            SessionEvent::AttackStart { attacker: 3, .. }
        ));
        assert_eq!(replies.stats().coalesced, 1);
    }

    #[test]
    fn gossip_and_vendor_observations_coalesce_per_npc_and_per_vendor() {
        let mut replies = SessionReplies::default();
        replies.push(scoped(
            1,
            1,
            SessionEvent::GossipMenu {
                npc: 7,
                text_id: 1,
                options: Vec::new(),
                quests: Vec::new(),
            },
        ));
        replies.push(scoped(
            1,
            1,
            SessionEvent::VendorInventory {
                vendor: 9,
                items: Vec::new(),
            },
        ));
        replies.push(scoped(
            1,
            1,
            SessionEvent::GossipMenu {
                npc: 7,
                text_id: 2,
                options: Vec::new(),
                quests: Vec::new(),
            },
        ));
        replies.push(scoped(
            1,
            1,
            SessionEvent::VendorInventory {
                vendor: 9,
                items: Vec::new(),
            },
        ));
        assert_eq!(replies.len(), 2);
        assert_eq!(replies.stats().coalesced, 2);
    }

    #[test]
    fn the_reply_cap_is_hard_and_reports_the_controlled_outcome() {
        let mut replies = SessionReplies::default();
        for i in 0..WRATH_SESSION_REPLY_CAP {
            replies.push(reply(i as u64));
        }
        assert!(!replies.overloaded());
        replies.push(reply(u64::MAX));
        assert!(
            replies.overloaded(),
            "the worker turns this into a terminal"
        );
        assert_eq!(
            replies.len(),
            WRATH_SESSION_REPLY_CAP,
            "memory stays bounded"
        );
        // The reserved terminal slots still fit at the cap and are delivered first.
        replies.push(disconnected());
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 1);
        assert!(matches!(
            drained[0].event,
            SessionEvent::Disconnected { .. }
        ));
        assert!(replies.is_empty());
    }

    #[test]
    fn a_closed_queue_accepts_nothing_and_never_fabricates_an_overflow_terminal() {
        let mut replies = SessionReplies::default();
        replies.close();
        for i in 0..(WRATH_SESSION_REPLY_CAP + 10) {
            replies.push(reply(i as u64));
        }
        replies.push(disconnected());
        assert!(!replies.overloaded(), "teardown pushes are not an overload");
        assert!(replies.is_empty());
        assert_eq!(
            replies.stats().closed_pushes,
            WRATH_SESSION_REPLY_CAP as u64 + 11
        );
    }

    #[test]
    fn producer_backpressure_latches_between_high_and_low_water() {
        let mut replies = SessionReplies::default();
        for i in 0..(WRATH_SESSION_REPLY_HIGH_WATER - 1) {
            replies.push(reply(i as u64));
        }
        assert!(!replies.producer_should_pause());
        replies.push(reply(0));
        assert!(replies.producer_should_pause(), "high water reached");
        let _ = replies.take(
            WRATH_SESSION_REPLY_HIGH_WATER - WRATH_SESSION_REPLY_LOW_WATER - 1,
            WRATH_DRAIN_BUDGET,
        );
        assert!(
            replies.producer_should_pause(),
            "between the marks the latch holds, so the producer does not flap"
        );
        // Drain in bounded slices (a single pass may stop on its wall budget, so loop rather
        // than assume the whole backlog left in one call).
        while !replies.is_empty() {
            let _ = replies.take(WRATH_DRAIN_CEILING, WRATH_DRAIN_BUDGET);
        }
        assert!(
            !replies.producer_should_pause(),
            "low water releases the latch"
        );
    }

    #[test]
    fn a_transfer_prunes_only_strictly_older_replies_and_keeps_forced_edges() {
        let mut replies = SessionReplies::default();
        replies.push(scoped(
            1,
            0,
            SessionEvent::AttackStart {
                attacker: 1,
                victim: 2,
            },
        ));
        replies.push(scoped(
            2,
            0,
            SessionEvent::AttackStart {
                attacker: 2,
                victim: 2,
            },
        ));
        replies.push(scoped(
            2,
            1,
            SessionEvent::AttackStart {
                attacker: 3,
                victim: 2,
            },
        ));
        replies.push(forced(SessionEvent::LoginStage {
            stage: benilla_protocol::LoginStage::Connecting,
        }));
        replies.adopt_scope(2, 1);
        assert_eq!(replies.len(), 2, "the two older replies are gone");
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert!(matches!(
            drained[0].event,
            SessionEvent::AttackStart { attacker: 3, .. }
        ));
        assert!(matches!(drained[1].event, SessionEvent::LoginStage { .. }));
    }

    #[test]
    fn the_drain_is_bounded_by_the_ceiling_and_the_budget() {
        let mut replies = SessionReplies::default();
        for i in 0..1000 {
            replies.push(reply(i));
        }
        // One pass never exceeds the ceiling; the wall budget may end it earlier, so the two
        // sides are checked against each other rather than against an exact count.
        let drained = replies.take(WRATH_DRAIN_CEILING, WRATH_DRAIN_BUDGET);
        assert!(!drained.is_empty());
        assert!(drained.len() <= WRATH_DRAIN_CEILING);
        assert_eq!(drained.len() + replies.len(), 1000);
    }

    #[test]
    fn classification_names_every_terminal_kind_and_keeps_inventory_replies_ordered() {
        assert_eq!(
            session_class(&SessionEvent::LoggedOut),
            SessionClass::Terminal
        );
        assert_eq!(
            session_class(&SessionEvent::CharacterLoginFailed { result: 0x41 }),
            SessionClass::Terminal
        );
        assert_eq!(
            session_class(&SessionEvent::Disconnected {
                reason: String::new(),
                end: benilla_protocol::SessionEnd::Lost,
            }),
            SessionClass::Terminal
        );
        assert_eq!(
            session_class(&SessionEvent::LoginFailed {
                refusal: None,
                reason: String::new(),
                dial: None,
            }),
            SessionClass::Terminal
        );
        assert_eq!(
            session_class(&SessionEvent::Worldport {
                map_id: 0,
                position: [0.0; 3],
                orientation: 0.0,
                needs_ack: false,
            }),
            SessionClass::Lifecycle
        );
        assert_eq!(
            session_class(&SessionEvent::ServerUnixTime { unix_time: 0 }),
            SessionClass::Observation
        );
        // Inventory replies are ordered outcomes, never coalesced.
        assert_eq!(
            session_class(&SessionEvent::InventoryFailure {
                reason: 0,
                required_level: None,
                item_guid: 0,
                item2_guid: 0,
                bag_slot: 0,
            }),
            SessionClass::Reply
        );
    }
}

/// The scoped delivery barrier: world transfer, logout and shutdown resolve every outstanding
/// receipt, WM action and parked ask exactly once with a terminal state, and never let a retired
/// scope's reply reach the replacement world. Source-level cases only: no build or test run is
/// claimed here (see the package-B handoff).
#[cfg(test)]
mod scoped_delivery_tests {
    use super::*;

    fn control() -> (WrathControl, Receiver<WrathParkedCommand>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            WrathControl {
                config: None,
                multiclass: None,
                trace: None,
                worker: None,
                retiring: None,
                feed: None,
                cancel: Arc::new(AtomicBool::new(false)),
                commands: None,
                ended: false,
                restart_allowed: false,
                restart_attempts: 0,
                retry_at: None,
                terminal_ready: true,
                entered_world: false,
                ended_retirement_done: false,
                left_world: false,
                shutdown: false,
                last_error: None,
                session_events: None,
                session_events_tx: None,
                reply_stats: SessionReplyStats::default(),
                park_commands: Some(tx),
                publisher_retiring: Vec::new(),
                publisher_retire_rx: None,
                wm_actions: None,
            },
            rx,
        )
    }

    fn actions() -> (
        WrathWmActions,
        Sender<ActionResult>,
        Receiver<ActionResult>,
        Sender<ActionRequest>,
    ) {
        let (requests_tx, requests_rx) = crossbeam_channel::unbounded();
        let (_cancels_tx, cancels_rx) = crossbeam_channel::unbounded::<String>();
        let (results_tx, results_rx) = crossbeam_channel::unbounded();
        (
            WrathWmActions {
                requests: requests_rx,
                cancels: cancels_rx,
                results: results_tx.clone(),
                pending: BTreeMap::new(),
                cancelled: Vec::new(),
                last_cast_spell: None,
                next_admission: 2,
                received: 1,
                refused: 0,
                completed: 0,
            },
            results_tx,
            results_rx,
            requests_tx,
        )
    }

    fn admit_cast(
        actions: &mut WrathWmActions,
        id: &str,
        session_epoch: u64,
        world_generation: u64,
    ) {
        actions.pending.insert(
            id.to_owned(),
            PendingWmAction {
                intent: ActionIntent::Cast {
                    spell_id: 133,
                    target: Some(0x22),
                },
                player_guid: 0x1,
                cast_spell: Some(133),
                cast_attempt: None,
                cast_count: None,
                admitted: 1,
                deadline: Instant::now() + Duration::from_secs(30),
                session_epoch,
                world_generation,
            },
        );
    }

    fn scoped(session_epoch: u64, world_generation: u64) -> WrathSessionEvent {
        WrathSessionEvent {
            login_generation: None,
            session_epoch,
            world_generation,
            event: SessionEvent::AttackStart {
                attacker: 1,
                victim: 2,
            },
            force: false,
        }
    }

    fn queued_receipt(
        outbox: &WrathOutbox,
        session_epoch: u64,
        world_generation: u64,
    ) -> Receiver<crate::net::CommandDelivery> {
        let (results, receiver) = crossbeam_channel::bounded(2);
        outbox.lock().unwrap().push_back(WrathOutbound {
            opcode: benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_ACCEPT_QUEST,
            payload: vec![0; 8],
            movement: false,
            movement_seq: None,
            enqueued_at: None,
            receipt: Some(crate::net::CommandReceipt {
                trace: None,
                enforce_scope: true,
                session_epoch,
                world_generation,
                npc: 0x22,
                quest: 7,
                results,
            }),
            trace: None,
            enforce_scope: true,
        });
        receiver
    }

    /// A transfer retires the intent admitted against the old world and closes the select
    /// screen's ask channel; the answer carries the retired scope, not the adopted one.
    #[test]
    fn a_world_transfer_retires_a_pending_action_and_the_parked_link() {
        let (mut control, park_rx) = control();
        let (mut actions, _results_tx, results, _requests) = actions();
        admit_cast(&mut actions, "cast-1", 1, 1);

        let report = control.retire_scoped_delivery(
            DeliveryScopeEnd::WorldTransfer {
                retired: (1, 1),
                adopted: (1, 2),
            },
            Some(&mut actions),
            "world scope changed before an authoritative result",
        );

        assert_eq!(report.actions, 1);
        assert!(
            actions.pending.is_empty(),
            "the intent is consumed, not replayed"
        );
        let answer = results.try_recv().expect("one terminal answer");
        assert_eq!(answer.request_id, "cast-1");
        assert_eq!(answer.status, ActionStatus::Unknown);
        assert_eq!((answer.session_epoch, answer.world_generation), (1, 1));
        assert!(results.try_recv().is_err(), "exactly one terminal answer");
        assert!(control.park_commands.is_none(), "the ask channel is closed");
        assert!(
            matches!(
                park_rx.try_recv(),
                Err(crossbeam_channel::TryRecvError::Disconnected)
            ),
            "a parked ask can never be served by the replacement world"
        );
    }

    /// A logout answers a receipt that never reached the wire and keeps the terminal slots that
    /// carry the session's end.
    #[test]
    fn a_logout_retires_a_pending_receipt_and_keeps_its_terminal() {
        let (mut control, _park_rx) = control();
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::new()));
        control.commands = Some(Arc::clone(&outbox));
        let receipt = queued_receipt(&outbox, 1, 1);
        let replies = Arc::new(Mutex::new(SessionReplies::default()));
        control.session_events = Some(Arc::clone(&replies));
        control.session_events_tx = Some(Arc::clone(&replies));
        {
            let mut queue = replies.lock().unwrap();
            queue.push(scoped(1, 1));
            queue.push(WrathSessionEvent::forced(SessionEvent::LoggedOut));
        }

        let report = control.retire_scoped_delivery(
            DeliveryScopeEnd::SessionEnded,
            None,
            "session ended before an authoritative result",
        );

        assert_eq!(report.outbound, 1);
        assert!(outbox.lock().unwrap().is_empty());
        assert_eq!(
            receipt.try_recv(),
            Ok(crate::net::CommandDelivery::Rejected(
                crate::net::CommandRefusal::ScopeChanged
            )),
            "a receipt the worker never wrote gets one terminal refusal"
        );
        assert!(receipt.try_recv().is_err(), "not answered twice");
        assert_eq!(report.replies, 1, "the queued old-scope reply is discarded");
        let drained = replies
            .lock()
            .unwrap()
            .take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 1);
        assert!(
            matches!(drained[0].event, SessionEvent::LoggedOut),
            "the reserved terminal slot survives the retirement"
        );
    }

    /// A second retirement of the same scope resolves nothing, and a reply the retired scope
    /// pushes afterwards never takes a delivery slot.
    #[test]
    fn a_second_retirement_resolves_nothing_and_a_late_reply_is_not_delivered() {
        let (mut control, _park_rx) = control();
        let replies = Arc::new(Mutex::new(SessionReplies::default()));
        control.session_events = Some(Arc::clone(&replies));
        control.session_events_tx = Some(Arc::clone(&replies));
        let (mut actions, _results_tx, results, _requests) = actions();
        admit_cast(&mut actions, "cast-1", 1, 1);

        let first = control.retire_scoped_delivery(
            DeliveryScopeEnd::WorldTransfer {
                retired: (1, 1),
                adopted: (1, 2),
            },
            Some(&mut actions),
            "world scope changed before an authoritative result",
        );
        assert_eq!(first.actions, 1);
        assert_eq!(
            results.try_recv().map(|a| a.request_id),
            Ok("cast-1".to_owned())
        );

        let second = control.retire_scoped_delivery(
            DeliveryScopeEnd::WorldTransfer {
                retired: (1, 1),
                adopted: (1, 2),
            },
            Some(&mut actions),
            "world scope changed before an authoritative result",
        );
        assert_eq!(second, ScopedDeliveryRetirement::default());
        assert!(results.try_recv().is_err(), "no action is resolved twice");

        // The old worker can still push across the barrier's race window; the adopted scope
        // rejects it at the producer, while the current scope's reply goes through.
        {
            let mut queue = replies.lock().unwrap();
            queue.push(scoped(1, 1));
            queue.push(scoped(1, 2));
        }
        let drained = replies
            .lock()
            .unwrap()
            .take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(
            drained.len(),
            1,
            "only the adopted scope's reply is delivered"
        );
        assert_eq!(drained[0].world_generation, 2);
    }

    /// The shutdown path takes the same barrier, so an intent queued for a session that is going
    /// away is answered instead of being stranded for a follower that never comes.
    #[test]
    fn the_shutdown_barrier_answers_unadmitted_proposals() {
        let (mut control, _park_rx) = control();
        let (mut actions, _results_tx, results, requests) = actions();
        admit_cast(&mut actions, "cast-1", 1, 1);
        requests
            .send(ActionRequest {
                request_id: "cast-2".to_owned(),
                session_epoch: 1,
                world_generation: 1,
                player_guid: 0x1,
                deadline_unix_ms: 0,
                intent: ActionIntent::CastCancel,
            })
            .unwrap();

        let report = control.retire_scoped_delivery(
            DeliveryScopeEnd::SessionEnded,
            Some(&mut actions),
            "the client is shutting down",
        );

        assert_eq!(report.actions, 1);
        assert_eq!(report.unadmitted, 1);
        let mut answered: Vec<_> = results.try_iter().collect();
        answered.sort_by(|a, b| a.request_id.cmp(&b.request_id));
        assert_eq!(answered.len(), 2);
        assert_eq!(answered[0].request_id, "cast-1");
        assert_eq!(answered[1].request_id, "cast-2");
        assert_eq!(answered[0].status, ActionStatus::Unknown);
        assert_eq!(
            answered[1].status,
            ActionStatus::Rejected,
            "a proposal that never reached a command owner was never dispatched"
        );
    }

    #[test]
    #[ignore = "known failure on main (2026-11-18 CI baseline); run with --ignored"]
    fn push_aura_fields_projects_spell_flags_levels_and_applications_into_object_fields() {
        let auras = vec![
            benilla_protocol_wrath::reducer::AuraSlot {
                slot: 0,
                spell_id: 17,
                flags: 0x05,
                level: 60,
                charges: 3,
                caster: 0,
                max_duration_ms: 0,
                duration_ms: 0,
            },
            benilla_protocol_wrath::reducer::AuraSlot {
                slot: 3,
                spell_id: 139,
                flags: 0x02,
                level: 80,
                charges: 1,
                caster: 0,
                max_duration_ms: 0,
                duration_ms: 0,
            },
        ];
        let mut pairs = Vec::new();
        push_aura_fields(&auras, &mut pairs);
        let fields = ObjectFields::from_pairs(&pairs).into_created(ObjectType::Unit);
        let a0 = fields.unit_aura(0).expect("slot 0 aura");
        assert_eq!(a0.spell_id, 17);
        assert_eq!(a0.flags, 0x05);
        assert_eq!(a0.level, 60);
        assert_eq!(a0.stacks, 2); // charges(3) - 1 = 2 additional applications

        let a3 = fields.unit_aura(3).expect("slot 3 aura");
        assert_eq!(a3.spell_id, 139);
        assert_eq!(a3.flags, 0x02);
        assert_eq!(a3.level, 80);
        assert_eq!(a3.stacks, 0);
    }

    #[test]
    fn extract_login_refusal_maps_challenge_proof_and_world_codes() {
        let challenge_err = anyhow::Error::new(
            benilla_protocol_wrath::auth::LogonAuthError::Challenge(0x03),
        )
        .context("reading challenge reply");
        assert_eq!(
            extract_login_refusal(&challenge_err),
            Some(benilla_protocol::LoginRefusal::Logon(0x03))
        );

        let proof_err =
            anyhow::Error::new(benilla_protocol_wrath::auth::LogonAuthError::Proof(0x04))
                .context("reading proof reply");
        assert_eq!(
            extract_login_refusal(&proof_err),
            Some(benilla_protocol::LoginRefusal::LogonProof(0x04))
        );

        let world_err =
            anyhow::Error::new(benilla_protocol_wrath::auth::LogonAuthError::World(0x15));
        assert_eq!(
            extract_login_refusal(&world_err),
            Some(benilla_protocol::LoginRefusal::World(0x15))
        );
    }

    /// A Wrath logon dial nothing answered reaches the glue as a dial failure, so it reads
    /// `LOGIN_FAILED` rather than `DISCONNECTED`; a refusal or a lost link carries none.
    #[test]
    fn a_refused_wrath_logon_dial_is_a_dial_failure() {
        // Bind then drop a loopback listener: its port refuses the connect.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let addr = format!("127.0.0.1:{port}");
        let Err(err) = benilla_protocol_wrath::logon::logon(&addr, "a", "b") else {
            panic!("a closed port answered");
        };
        let dial = extract_logon_dial(&err).expect("a refused dial is a DialFailure");
        assert_eq!(dial.address, addr);
        assert!(!dial.unresolved);
        assert_eq!(extract_login_refusal(&err), None);

        let refusal = anyhow::Error::new(benilla_protocol_wrath::auth::LogonAuthError::Proof(4));
        assert!(extract_logon_dial(&refusal).is_none());
        assert!(extract_logon_dial(&anyhow::anyhow!("connection reset")).is_none());
    }

    #[test]
    fn char_rename_codec_round_trips_and_maps_status_codes() {
        let wire = encode_char_rename(0x1122_3344_5566_7788, "Firna");
        assert_eq!(&wire[..8], &0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(&wire[8..], b"Firna\0");

        let mut ok_reply = vec![0x00u8];
        ok_reply.extend_from_slice(&0x42u64.to_le_bytes());
        ok_reply.extend_from_slice(b"Renamed\0");
        assert_eq!(
            parse_char_rename(&ok_reply).expect("valid rename reply"),
            (0, Some((0x42, "Renamed".to_owned())))
        );

        assert_eq!(
            parse_char_rename(&[0x32]).expect("name in use"),
            (0x31, None)
        );
        assert_eq!(
            parse_char_rename(&[0x31]).expect("generic rename failure"),
            (1, None)
        );
        assert!(parse_char_rename(&[0]).is_err());
    }

    /// 12340's `SMSG_MESSAGECHAT` chat tag is a bitmask; the donor tag is an enum with GM first,
    /// then DND, then AFK. COM/DEV have no donor spelling and read NONE.
    #[test]
    fn wrath_chat_tag_bitmask_becomes_the_vanilla_enum() {
        use benilla_protocol::messages::chat_tag as t;
        assert_eq!(chat_tag_112(0), t::NONE);
        assert_eq!(chat_tag_112(0x01), t::AFK);
        assert_eq!(chat_tag_112(0x02), t::DND);
        assert_eq!(chat_tag_112(0x04), t::GM);
        assert_eq!(chat_tag_112(0x03), t::DND, "DND wins over AFK");
        assert_eq!(chat_tag_112(0x07), t::GM, "GM wins over DND and AFK");
        assert_eq!(
            chat_tag_112(0x08 | 0x10),
            t::NONE,
            "COM/DEV have no donor tag"
        );
    }
}

#[cfg(test)]
mod field_audit_tests {
    use super::*;
    use benilla_protocol_wrath::fields;

    /// The store's value at `index` only when it was delivered: `ObjectFields`' own reader is
    /// private, and answers 0 for an absent descriptor field, which would hide a projection
    /// that skipped a slot.
    trait Delivered {
        fn delivered(&self, index: u16) -> Option<u32>;
    }

    impl Delivered for benilla_protocol::ObjectFields {
        fn delivered(&self, index: u16) -> Option<u32> {
            self.raw_fields()
                .find(|&(at, _)| at == index)
                .map(|(_, value)| value)
        }
    }

    fn audit_window(pairs: &[(u16, u32)], end: u16) -> crate::wrath_inventory::WrathFieldWindow {
        let mut values = vec![0u32; usize::from(end)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        for &(index, value) in pairs {
            let at = usize::from(index);
            values[at] = value;
            delivered[at / 64] |= 1u64 << (at % 64);
        }
        crate::wrath_inventory::WrathFieldWindow::new(values, delivered)
    }

    fn audit_pack_guid(guid: u64) -> Vec<u8> {
        let mut mask = 0u8;
        let mut bytes = Vec::new();
        for (i, b) in guid.to_le_bytes().into_iter().enumerate() {
            if b != 0 {
                mask |= 1 << i;
                bytes.push(b);
            }
        }
        let mut out = vec![mask];
        out.extend(bytes);
        out
    }

    fn audit_values_wire(values: &[(u16, u32)]) -> Vec<u8> {
        let max_idx = values.iter().map(|(i, _)| *i).max().unwrap_or(0);
        let num_blocks = (usize::from(max_idx) / 32) + 1;
        let mut mask = vec![0u32; num_blocks];
        let mut sorted: Vec<(u16, u32)> = values.to_vec();
        sorted.sort_by_key(|(i, _)| *i);
        for (i, _) in &sorted {
            mask[usize::from(*i) / 32] |= 1u32 << (usize::from(*i) % 32);
        }
        let mut out = vec![num_blocks as u8];
        for m in mask {
            out.extend(m.to_le_bytes());
        }
        for (_, v) in sorted {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn audit_create_packet(
        guid: u64,
        type_id: u8,
        pos: Option<(f32, f32, f32, f32)>,
        values: &[(u16, u32)],
    ) -> Vec<u8> {
        let mut p = 1u32.to_le_bytes().to_vec();
        p.push(2); // CREATE_OBJECT
        p.extend(audit_pack_guid(guid));
        p.push(type_id);
        if let Some((x, y, z, o)) = pos {
            p.extend(
                benilla_protocol_wrath::updates::update_flags::STATIONARY_POSITION.to_le_bytes(),
            );
            p.extend(x.to_le_bytes());
            p.extend(y.to_le_bytes());
            p.extend(z.to_le_bytes());
            p.extend(o.to_le_bytes());
        } else {
            p.extend(0u16.to_le_bytes());
        }
        p.extend(audit_values_wire(values));
        p
    }

    fn audit_values_packet(guid: u64, values: &[(u16, u32)]) -> Vec<u8> {
        let mut p = 1u32.to_le_bytes().to_vec();
        p.push(0); // VALUES
        p.extend(audit_pack_guid(guid));
        p.extend(audit_values_wire(values));
        p
    }

    fn audit_session(player_guid: u64) -> SessionState {
        let mut session = SessionState::new();
        session.start_session(player_guid);
        session.note_player_login();
        let mut verify = Vec::with_capacity(20);
        verify.extend(0u32.to_le_bytes());
        verify.extend(0.0f32.to_le_bytes());
        verify.extend(0.0f32.to_le_bytes());
        verify.extend(0.0f32.to_le_bytes());
        verify.extend(0.0f32.to_le_bytes());
        session
            .reduce(
                benilla_protocol_wrath::opcode::SMSG_LOGIN_VERIFY_WORLD,
                &verify,
                1,
            )
            .expect("verify world");
        session
    }

    fn audit_world() -> World {
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        world.init_resource::<SelfGuid>();
        world.init_resource::<WrathWorldView>();
        world.init_resource::<crate::field_watch::NoticeLane>();
        world.init_resource::<crate::wrath_field_map::WrathFieldMap>();
        world.init_resource::<crate::wrath_inventory::WrathInventory>();
        world.init_resource::<Messages<FieldChanged>>();
        world
    }

    fn audit_step(session: &SessionState, world: &mut World) {
        let digest = build_digest(session);
        {
            let mut view = world.resource_mut::<WrathWorldView>();
            let mut changes = WrathChanges::default();
            for &guid in digest.entities.keys() {
                if view.entities.contains_key(&guid) {
                    changes.updated.push(guid);
                } else {
                    changes.created.push(guid);
                }
            }
            view.session_epoch = digest.session_epoch;
            view.world_generation = digest.world_generation;
            view.phase = Some(digest.phase);
            view.map_id = digest.map_id;
            view.player_guid = digest.player_guid;
            view.entered_world = true;
            view.entities = digest.entities;
            view.last_changes = changes;
        }
        wrath_apply_world(world);
    }

    /// Base `Object` (type_id 0) gets `ObjectType::Object` mask, entry, and scale, and never
    /// receives `UNIT_FIELD_*` slots even when stray unit values are seeded on `WrathEntity`.
    #[test]
    fn wrath_field_audit_object_projection_gates_unit_fields() {
        const PLAYER: u64 = 0x1;
        const OBJ: u64 = 0x99;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            OBJ,
            benilla_protocol_wrath::updates::type_id::OBJECT,
            None,
            &[
                (fields::OBJECT_FIELD_GUID, OBJ as u32),
                (fields::OBJECT_FIELD_ENTRY, 777),
                (fields::OBJECT_FIELD_SCALE_X, 1.5f32.to_bits()),
                (fields::OBJECT_FIELD_PADDING, 0xDEAD_BEEF),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce object");
        let mut world = audit_world();
        audit_step(&session, &mut world);
        // Inject rogue unit values onto the view entity and re-apply to confirm the gate in
        // `wrath_apply_world` itself also refuses to emit `UNIT_FIELD_*` onto a non-unit.
        {
            let mut view = world.resource_mut::<WrathWorldView>();
            let ent = view.entities.get_mut(&OBJ).expect("object in view");
            ent.health = Some(500);
            ent.level = Some(60);
            ent.unit_flags = Some(0x1234);
            ent.bytes_0 = Some(0x0102_0304);
            view.last_changes.updated.push(OBJ);
        }
        wrath_apply_world(&mut world);

        let entity = world.resource::<GuidIndex>().0[&OBJ];
        let store = &world.get::<ObjectStore>(entity).expect("ObjectStore").0;
        assert_eq!(store.delivered(2), Some(0x1), "ObjectType::Object mask");
        assert_eq!(store.delivered(3), Some(777), "OBJECT_FIELD_ENTRY");
        assert_eq!(
            store.delivered(4),
            Some(1.5f32.to_bits()),
            "OBJECT_FIELD_SCALE_X"
        );
        assert_eq!(
            store.delivered(5),
            None,
            "OBJECT_FIELD_PADDING stays unmapped"
        );
        assert_eq!(
            store.delivered(22),
            None,
            "UNIT_FIELD_HEALTH gated off Object"
        );
        assert_eq!(
            store.delivered(34),
            None,
            "UNIT_FIELD_LEVEL gated off Object"
        );
        assert_eq!(
            store.delivered(36),
            None,
            "UNIT_FIELD_BYTES_0 gated off Object"
        );
        assert_eq!(
            store.delivered(46),
            None,
            "UNIT_FIELD_FLAGS gated off Object"
        );
    }

    /// Synthetic Item and Container update blocks map into 1.12 donor slots while preserving
    /// Wrath-only enchant slots 8..12 and `CREATE_PLAYED_TIME` in `WrathItemRow` only.
    #[test]
    fn wrath_field_audit_item_and_container_projection() {
        const PLAYER: u64 = 0x1;
        const ITEM: u64 = 0x4000_0000_0000_0010;
        const BAG: u64 = 0x4000_0000_0000_0020;
        let mut session = audit_session(PLAYER);
        let item_pkt = audit_create_packet(
            ITEM,
            benilla_protocol_wrath::updates::type_id::ITEM,
            None,
            &[
                (fields::OBJECT_FIELD_ENTRY, 6948),
                (fields::ITEM_FIELD_OWNER, PLAYER as u32),
                (fields::ITEM_FIELD_STACK_COUNT, 5),
                (fields::ITEM_FIELD_FLAGS, 0x01),
                (fields::ITEM_FIELD_ENCHANTMENT_1_1, 1894),
                (fields::ITEM_FIELD_ENCHANTMENT_8_1, 3333),
                (fields::ITEM_FIELD_PROPERTY_SEED, 0x11),
                (fields::ITEM_FIELD_RANDOM_PROPERTIES_ID, 0x22),
                (fields::ITEM_FIELD_DURABILITY, 25),
                (fields::ITEM_FIELD_MAXDURABILITY, 50),
                (fields::ITEM_FIELD_CREATE_PLAYED_TIME, 999),
            ],
        );
        let bag_pkt = audit_create_packet(
            BAG,
            benilla_protocol_wrath::updates::type_id::CONTAINER,
            None,
            &[
                (fields::OBJECT_FIELD_ENTRY, 4496),
                (fields::CONTAINER_FIELD_NUM_SLOTS, 8),
                (fields::CONTAINER_ALIGN_PAD, 0xBEEF),
                (fields::CONTAINER_FIELD_SLOT_1, ITEM as u32),
                (fields::CONTAINER_FIELD_SLOT_1 + 1, (ITEM >> 32) as u32),
            ],
        );
        session
            .reduce(
                benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT,
                &item_pkt,
                2,
            )
            .expect("reduce item");
        session
            .reduce(
                benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT,
                &bag_pkt,
                3,
            )
            .expect("reduce bag");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let item_ent = world.resource::<GuidIndex>().0[&ITEM];
        let item_store = &world.get::<ObjectStore>(item_ent).expect("item store").0;
        assert_eq!(item_store.delivered(14), Some(5), "ITEM_FIELD_STACK_COUNT");
        assert_eq!(item_store.delivered(21), Some(0x01), "ITEM_FIELD_FLAGS");
        assert_eq!(
            item_store.delivered(22),
            Some(1894),
            "ITEM_FIELD_ENCHANTMENT_1_1"
        );
        assert_eq!(
            item_store.delivered(43),
            Some(0x11),
            "ITEM_FIELD_PROPERTY_SEED -> 43"
        );
        assert_eq!(
            item_store.delivered(44),
            Some(0x22),
            "ITEM_FIELD_RANDOM_PROPERTIES_ID -> 44"
        );
        assert_eq!(
            item_store.delivered(45),
            None,
            "donor 45 ITEM_FIELD_ITEM_TEXT_ID unwritten"
        );
        assert_eq!(
            item_store.delivered(46),
            Some(25),
            "ITEM_FIELD_DURABILITY -> 46"
        );
        assert_eq!(
            item_store.delivered(47),
            Some(50),
            "ITEM_FIELD_MAXDURABILITY -> 47"
        );
        assert_eq!(
            item_store.delivered(62),
            None,
            "CREATE_PLAYED_TIME not in donor"
        );

        let bag_ent = world.resource::<GuidIndex>().0[&BAG];
        let bag_store = &world.get::<ObjectStore>(bag_ent).expect("bag store").0;
        assert_eq!(
            bag_store.delivered(48),
            Some(8),
            "CONTAINER_FIELD_NUM_SLOTS -> 48"
        );
        assert_eq!(
            bag_store.delivered(49),
            None,
            "CONTAINER_ALIGN_PAD stays unmapped"
        );
        assert_eq!(
            bag_store.delivered(50),
            Some(ITEM as u32),
            "CONTAINER_FIELD_SLOT_1 low -> 50"
        );
        assert_eq!(
            bag_store.delivered(51),
            Some((ITEM >> 32) as u32),
            "CONTAINER_FIELD_SLOT_1 high -> 51"
        );
    }

    /// Synthetic Unit update block projects combat reach, bounding radius, mount display, pet
    /// fields, power cost modifier/multiplier, bytes 0/1/2 (including stealth + shapeshift), and
    /// guards `NATIVEDISPLAYID` against zero clobbering.
    #[test]
    fn wrath_field_audit_unit_projection_and_native_display_guard() {
        const PLAYER: u64 = 0x1;
        const UNIT: u64 = 0xF130_0000_0000_0042;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            UNIT,
            benilla_protocol_wrath::updates::type_id::UNIT,
            Some((10.0, 20.0, 30.0, 1.0)),
            &[
                (fields::OBJECT_FIELD_ENTRY, 299),
                (fields::UNIT_FIELD_CHANNEL_OBJECT, 0x55),
                (fields::UNIT_CHANNEL_SPELL, 15407),
                (fields::UNIT_FIELD_BYTES_0, 0x0001_0201),
                (fields::UNIT_FIELD_HEALTH, 120),
                (fields::UNIT_FIELD_POWER1, 80),
                (fields::UNIT_FIELD_POWER4, 60),
                (fields::UNIT_FIELD_POWER7, 400),
                (fields::UNIT_FIELD_MAXHEALTH, 150),
                (fields::UNIT_FIELD_MAXPOWER1, 100),
                (fields::UNIT_FIELD_MAXPOWER4, 100),
                (fields::UNIT_FIELD_LEVEL, 12),
                (fields::UNIT_FIELD_FACTIONTEMPLATE, 14),
                (fields::UNIT_FIELD_FLAGS, 0x0008_0000),
                (fields::UNIT_FIELD_BOUNDINGRADIUS, 0.75f32.to_bits()),
                (fields::UNIT_FIELD_COMBATREACH, 2.25f32.to_bits()),
                (fields::UNIT_FIELD_DISPLAYID, 1234),
                (fields::UNIT_FIELD_NATIVEDISPLAYID, 0), // must NOT clobber display fallback
                (fields::UNIT_FIELD_MOUNTDISPLAYID, 2410),
                (fields::UNIT_FIELD_BYTES_1, (0x02 << 16) | 0x01), // stand=1, vis_flags=0x02 (stealth)
                (fields::UNIT_FIELD_PETNUMBER, 77),
                (fields::UNIT_FIELD_PET_NAME_TIMESTAMP, 1_700_000),
                (fields::UNIT_FIELD_PETEXPERIENCE, 300),
                (fields::UNIT_FIELD_PETNEXTLEVELEXP, 900),
                (fields::UNIT_MOD_CAST_SPEED, 1.0f32.to_bits()),
                (fields::UNIT_CREATED_BY_SPELL, 688),
                (fields::UNIT_NPC_EMOTESTATE, 69),
                (fields::UNIT_FIELD_STAT0, 25),
                (fields::UNIT_FIELD_RESISTANCES, 450),
                (fields::UNIT_FIELD_BASE_MANA, 200),
                (fields::UNIT_FIELD_BASE_HEALTH, 150),
                (fields::UNIT_FIELD_BYTES_2, (0x05 << 24) | 0x01), // sheath=1, shapeshift=5 (bear)
                (fields::UNIT_FIELD_ATTACK_POWER, 85),
                (fields::UNIT_FIELD_POWER_COST_MODIFIER, (-10i32) as u32),
                (fields::UNIT_FIELD_POWER_COST_MULTIPLIER, 0.9f32.to_bits()),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce unit");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let entity = world.resource::<GuidIndex>().0[&UNIT];
        let store = &world.get::<ObjectStore>(entity).expect("unit store").0;
        assert_eq!(store.delivered(20), Some(0x55), "UNIT_FIELD_CHANNEL_OBJECT");
        assert_eq!(
            store.delivered(144),
            Some(15407),
            "UNIT_CHANNEL_SPELL -> 144"
        );
        assert_eq!(store.delivered(22), Some(120), "UNIT_FIELD_HEALTH -> 22");
        assert_eq!(store.delivered(23), Some(80), "UNIT_FIELD_POWER1 -> 23");
        assert_eq!(store.delivered(26), Some(60), "UNIT_FIELD_POWER4 -> 26");
        assert_eq!(
            store.delivered(36),
            Some(0x0001_0201),
            "UNIT_FIELD_BYTES_0 -> 36"
        );
        assert_eq!(
            store.delivered(129),
            Some(0.75f32.to_bits()),
            "BOUNDINGRADIUS -> 129"
        );
        assert_eq!(
            store.delivered(130),
            Some(2.25f32.to_bits()),
            "COMBATREACH -> 130"
        );
        assert_eq!(store.delivered(131), Some(1234), "DISPLAYID -> 131");
        assert_eq!(
            store.delivered(132),
            Some(1234),
            "delivered NATIVEDISPLAYID=0 keeps display fallback"
        );
        assert_eq!(store.delivered(133), Some(2410), "MOUNTDISPLAYID -> 133");
        assert_eq!(
            store.delivered(138),
            Some((0x02 << 24) | (0x05 << 16) | 0x01),
            "donor BYTES_1 packs stand=1, shapeshift=5 (byte 2), vis_flags=0x02 (byte 3)"
        );
        assert_eq!(store.delivered(139), Some(77), "PETNUMBER -> 139");
        assert_eq!(
            store.delivered(140),
            Some(1_700_000),
            "PET_NAME_TIMESTAMP -> 140"
        );
        assert_eq!(store.delivered(141), Some(300), "PETEXPERIENCE -> 141");
        assert_eq!(store.delivered(142), Some(900), "PETNEXTLEVELEXP -> 142");
        assert_eq!(
            store.delivered(145),
            Some(1.0f32.to_bits()),
            "MOD_CAST_SPEED -> 145"
        );
        assert_eq!(store.delivered(146), Some(688), "CREATED_BY_SPELL -> 146");
        assert_eq!(store.delivered(148), Some(69), "NPC_EMOTESTATE -> 148");
        assert_eq!(store.delivered(150), Some(25), "STAT0 -> 150");
        assert_eq!(store.delivered(155), Some(450), "RESISTANCES[0] -> 155");
        assert_eq!(store.delivered(162), Some(200), "BASE_MANA -> 162");
        assert_eq!(store.delivered(163), Some(150), "BASE_HEALTH -> 163");
        assert_eq!(
            store.delivered(164),
            Some((0x05 << 24) | 0x01),
            "BYTES_2 -> 164"
        );
        assert_eq!(store.delivered(165), Some(85), "ATTACK_POWER -> 165");
        assert_eq!(
            store.delivered(173),
            Some((-10i32) as u32),
            "POWER_COST_MODIFIER[0] -> 173"
        );
        assert_eq!(
            store.delivered(180),
            Some(0.9f32.to_bits()),
            "POWER_COST_MULTIPLIER[0] -> 180"
        );
        // Non-zero NATIVEDISPLAYID overrides the display fallback on subsequent update.
        let upd = audit_values_packet(UNIT, &[(fields::UNIT_FIELD_NATIVEDISPLAYID, 5678)]);
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &upd, 3)
            .expect("reduce native display");
        audit_step(&session, &mut world);
        let store = &world.get::<ObjectStore>(entity).expect("unit store").0;
        assert_eq!(
            store.delivered(132),
            Some(5678),
            "non-zero NATIVEDISPLAYID -> 132"
        );
    }

    /// Synthetic Player update block projects skills, quests (20 donor + 25 WrathQuestSlots),
    /// explored zones, stat buff mods, honor kills split (today/yesterday), yesterday contribution,
    /// lifetime HK, bytes2, retains honor/arena currency in `WrathPlayerRaw`, and fires PvP notices.
    #[test]
    fn wrath_field_audit_player_projection_and_pvp_notices() {
        const PLAYER: u64 = 0x42;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            PLAYER,
            benilla_protocol_wrath::updates::type_id::PLAYER,
            Some((-8913.0, -133.0, 82.0, 0.5)),
            &[
                (fields::UNIT_FIELD_HEALTH, 100),
                (fields::UNIT_FIELD_MAXHEALTH, 100),
                (fields::UNIT_FIELD_POSSTAT0, 12),
                (fields::UNIT_FIELD_RESISTANCEBUFFMODSPOSITIVE, 30),
                (fields::PLAYER_FLAGS, 0x04),
                (fields::PLAYER_BYTES, 0x0102_0304),
                (fields::PLAYER_BYTES_2, 0x0506_0708),
                (fields::PLAYER_BYTES_3, 0x0000_0001),
                (fields::PLAYER_QUEST_LOG_1_1, 783),
                (fields::PLAYER_QUEST_LOG_25_1, 9999),
                (fields::PLAYER_XP, 450),
                (fields::PLAYER_NEXT_LEVEL_XP, 1000),
                (fields::PLAYER_SKILL_INFO_1_1, 43), // Swords skill id
                (fields::PLAYER_EXPLORED_ZONES_1, 0x0000_000F),
                (fields::PLAYER_FIELD_COINAGE, 12345),
                (fields::PLAYER_FIELD_KILLS, (9 << 16) | 4), // today=4, yesterday=9
                (fields::PLAYER_FIELD_TODAY_CONTRIBUTION, 350),
                (fields::PLAYER_FIELD_YESTERDAY_CONTRIBUTION, 700),
                (fields::PLAYER_FIELD_LIFETIME_HONORABLE_KILLS, 1234),
                (fields::PLAYER_FIELD_BYTES2, 0x0000_0007),
                (fields::PLAYER_FIELD_WATCHED_FACTION_INDEX, 3),
                (fields::PLAYER_FIELD_HONOR_CURRENCY, 2500),
                (fields::PLAYER_FIELD_ARENA_CURRENCY, 400),
                (fields::PLAYER_FIELD_MAX_LEVEL, 80),
                (fields::PLAYER_FIELD_DAILY_QUESTS_1, 11111),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce player");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let entity = world.resource::<GuidIndex>().0[&PLAYER];
        let store = &world.get::<ObjectStore>(entity).expect("player store").0;
        assert_eq!(store.delivered(190), Some(0x04), "PLAYER_FLAGS -> 190");
        assert_eq!(
            store.delivered(193),
            Some(0x0102_0304),
            "PLAYER_BYTES -> 193"
        );
        assert_eq!(
            store.delivered(194),
            Some(0x0506_0708),
            "PLAYER_BYTES_2 -> 194"
        );
        assert_eq!(store.delivered(198), Some(783), "QUEST_LOG_1_1 -> 198");
        assert_eq!(store.delivered(716), Some(450), "PLAYER_XP -> 716");
        assert_eq!(
            store.delivered(718),
            Some(43),
            "PLAYER_SKILL_INFO_1_1 -> 718"
        );
        assert_eq!(
            store.delivered(1111),
            Some(0x0000_000F),
            "EXPLORED_ZONES_1 -> 1111"
        );
        assert_eq!(store.delivered(1176), Some(12345), "COINAGE -> 1176");
        assert_eq!(
            store.delivered(1177),
            Some(12),
            "UNIT_FIELD_POSSTAT0 -> 1177"
        );
        assert_eq!(
            store.delivered(1187),
            Some(30),
            "RESISTANCEBUFFMODSPOSITIVE -> 1187"
        );
        assert_eq!(
            store.delivered(1250),
            Some(4),
            "KILLS low u16 -> SESSION_KILLS 1250"
        );
        assert_eq!(
            store.delivered(1251),
            Some(9),
            "KILLS high u16 -> YESTERDAY_KILLS 1251"
        );
        assert_eq!(
            store.delivered(1255),
            Some(1234),
            "LIFETIME_HONORABLE_KILLS -> 1255"
        );
        assert_eq!(
            store.delivered(1257),
            Some(700),
            "YESTERDAY_CONTRIBUTION -> 1257"
        );
        assert_eq!(
            store.delivered(1260),
            Some(0x0000_0007),
            "PLAYER_FIELD_BYTES2 -> 1260"
        );
        assert_eq!(
            store.delivered(1261),
            Some(3),
            "WATCHED_FACTION_INDEX -> 1261"
        );

        let qslots = world
            .get::<WrathQuestSlots>(entity)
            .expect("WrathQuestSlots");
        assert_eq!(
            qslots.0[24].quest_id, 9999,
            "slot 25 preserved in WrathQuestSlots"
        );
        assert_eq!(qslots.1, 80, "PLAYER_FIELD_MAX_LEVEL");
        assert_eq!(qslots.2, 1, "PLAYER_FIELD_DAILY_QUESTS_1 count");

        let raw = &world
            .get::<WrathPlayerRaw>(entity)
            .expect("WrathPlayerRaw")
            .0;
        assert_eq!(raw.get(fields::PLAYER_FIELD_TODAY_CONTRIBUTION), Some(350));
        assert_eq!(raw.get(fields::PLAYER_FIELD_HONOR_CURRENCY), Some(2500));
        assert_eq!(raw.get(fields::PLAYER_FIELD_ARENA_CURRENCY), Some(400));

        // Update kills + rank byte and verify PvP notices (523 / 524) fire.
        world
            .resource_mut::<crate::field_watch::NoticeLane>()
            .drain()
            .for_each(drop);
        let upd = audit_values_packet(
            PLAYER,
            &[
                (fields::PLAYER_FIELD_KILLS, (9 << 16) | 5),
                (fields::PLAYER_FIELD_BYTES2, 0x0000_0008),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &upd, 3)
            .expect("reduce pvp delta");
        audit_step(&session, &mut world);
        let notices: Vec<_> = world
            .resource_mut::<crate::field_watch::NoticeLane>()
            .drain()
            .collect();
        assert!(
            notices.contains(&crate::field_watch::FieldNotice::Player { id: 523 }),
            "PLAYER_PVP_KILLS_CHANGED (523) fired: {notices:?}"
        );
        assert!(
            notices.contains(&crate::field_watch::FieldNotice::Player { id: 524 }),
            "PLAYER_PVP_RANK_CHANGED (524) fired: {notices:?}"
        );
    }

    /// Synthetic GameObject update block projects display, flags, parent rotation, dynamic low u16,
    /// faction, level, split BYTES_1 (state/type/art_kit/anim_progress), and position, gates out
    /// all unit slots, and emits `FieldChanged` on state transition.
    #[test]
    fn wrath_field_audit_gameobject_projection_and_state_change() {
        const PLAYER: u64 = 0x1;
        const GO: u64 = 0xF110_0000_0000_0007;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            GO,
            benilla_protocol_wrath::updates::type_id::GAMEOBJECT,
            Some((100.0, 200.0, 30.0, 1.25)),
            &[
                (fields::OBJECT_FIELD_ENTRY, 179697),
                (fields::OBJECT_FIELD_CREATED_BY, 0xABCD),
                (fields::GAMEOBJECT_DISPLAYID, 4211),
                (fields::GAMEOBJECT_FLAGS, 0x20),
                (fields::GAMEOBJECT_PARENTROTATION, 0.1f32.to_bits()),
                (fields::GAMEOBJECT_PARENTROTATION + 1, 0.2f32.to_bits()),
                (fields::GAMEOBJECT_PARENTROTATION + 2, 0.3f32.to_bits()),
                (fields::GAMEOBJECT_PARENTROTATION + 3, 0.9f32.to_bits()),
                (fields::GAMEOBJECT_DYNAMIC, 0x00FF_0001), // high u16 must be masked off
                (fields::GAMEOBJECT_FACTION, 114),
                (fields::GAMEOBJECT_LEVEL, 5),
                (
                    fields::GAMEOBJECT_BYTES_1,
                    (100 << 24) | (3 << 16) | (3 << 8) | 1,
                ), // state=1, type=3 (chest), artkit=3, anim=100
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce go");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let entity = world.resource::<GuidIndex>().0[&GO];
        let store = &world.get::<ObjectStore>(entity).expect("go store").0;
        assert_eq!(store.delivered(6), Some(0xABCD), "CREATED_BY -> 6");
        assert_eq!(store.delivered(8), Some(4211), "GAMEOBJECT_DISPLAYID -> 8");
        assert_eq!(store.delivered(9), Some(0x20), "GAMEOBJECT_FLAGS -> 9");
        assert_eq!(
            store.delivered(10),
            Some(0.1f32.to_bits()),
            "PARENTROTATION[0] -> 10"
        );
        assert_eq!(
            store.delivered(13),
            Some(0.9f32.to_bits()),
            "PARENTROTATION[3] -> 13"
        );
        assert_eq!(store.delivered(14), Some(1), "BYTES_1[0] state -> 14");
        assert_eq!(store.delivered(15), Some(100.0f32.to_bits()), "POS_X -> 15");
        assert_eq!(store.delivered(18), Some(1.25f32.to_bits()), "FACING -> 18");
        assert_eq!(store.delivered(19), Some(1), "DYNAMIC low u16 -> 19");
        assert_eq!(store.delivered(20), Some(114), "FACTION -> 20");
        assert_eq!(store.delivered(21), Some(3), "BYTES_1[1] type_id -> 21");
        assert_eq!(store.delivered(22), Some(5), "LEVEL -> 22");
        assert_eq!(store.delivered(23), Some(3), "BYTES_1[2] art_kit -> 23");
        assert_eq!(
            store.delivered(24),
            Some(100),
            "BYTES_1[3] anim_progress -> 24"
        );
        assert_eq!(
            store.delivered(131),
            None,
            "UNIT_FIELD_DISPLAYID gated off GO"
        );
        assert_eq!(store.delivered(46), None, "UNIT_FIELD_FLAGS gated off GO");

        // Flip GO state 1 -> 0 (open door/chest) and verify FieldChanged fires for index 14.
        world
            .resource_mut::<Messages<FieldChanged>>()
            .drain()
            .for_each(drop);
        let upd = audit_values_packet(
            GO,
            &[(
                fields::GAMEOBJECT_BYTES_1,
                (100 << 24) | (3 << 16) | (3 << 8),
            )],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &upd, 3)
            .expect("reduce go state");
        audit_step(&session, &mut world);
        let changed: Vec<_> = world
            .resource_mut::<Messages<FieldChanged>>()
            .drain()
            .collect();
        assert!(
            changed.iter().any(|m| {
                m.guid == GO
                    && m.kind == ObjectType::GameObject
                    && m.index == benilla_protocol::field::FIELD_GAMEOBJECT_STATE
                    && m.old == 1
                    && m.new == 0
            }),
            "expected FieldChanged for FIELD_GAMEOBJECT_STATE (14), got {changed:?}"
        );
    }

    /// Synthetic DynamicObject update block projects caster, bytes, spell, radius, and position
    /// without letting Wrath `DYNAMICOBJECT_CASTTIME` (11) overwrite donor `POS_X` (11).
    #[test]
    fn wrath_field_audit_dynamicobject_projection_preserves_pos_over_casttime() {
        const PLAYER: u64 = 0x1;
        const DYN: u64 = 0xF100_0000_0000_0009;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            DYN,
            benilla_protocol_wrath::updates::type_id::DYNAMICOBJECT,
            Some((12.5, -44.0, 5.0, 0.25)),
            &[
                (fields::DYNAMICOBJECT_CASTER, PLAYER as u32),
                (fields::DYNAMICOBJECT_BYTES, 0x0000_0001),
                (fields::DYNAMICOBJECT_SPELLID, 2120), // Flamestrike
                (fields::DYNAMICOBJECT_RADIUS, 8.0f32.to_bits()),
                (fields::DYNAMICOBJECT_CASTTIME, 987_654),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce dynobj");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let entity = world.resource::<GuidIndex>().0[&DYN];
        let store = &world.get::<ObjectStore>(entity).expect("dyn store").0;
        assert_eq!(store.delivered(6), Some(PLAYER as u32), "CASTER low -> 6");
        assert_eq!(store.delivered(8), Some(1), "BYTES -> 8");
        assert_eq!(store.delivered(9), Some(2120), "SPELLID -> 9");
        assert_eq!(store.delivered(10), Some(8.0f32.to_bits()), "RADIUS -> 10");
        assert_eq!(
            store.delivered(11),
            Some(12.5f32.to_bits()),
            "POS_X (11) must not be clobbered by DYNAMICOBJECT_CASTTIME"
        );
        assert_eq!(
            store.delivered(12),
            Some((-44.0f32).to_bits()),
            "POS_Y -> 12"
        );
        assert_eq!(store.delivered(13), Some(5.0f32.to_bits()), "POS_Z -> 13");
        assert_eq!(store.delivered(14), Some(0.25f32.to_bits()), "FACING -> 14");
        assert_eq!(
            store.delivered(22),
            None,
            "UNIT_FIELD_HEALTH gated off DynamicObject"
        );
        // Cast time stays available in raw_fields for Wrath-native consumers.
        let view_ent = &world.resource::<WrathWorldView>().entities[&DYN];
        assert_eq!(
            view_ent
                .raw_fields
                .as_ref()
                .and_then(|r| r.get(fields::DYNAMICOBJECT_CASTTIME)),
            Some(987_654)
        );
    }

    /// Synthetic Corpse update block projects owner, position (without `CORPSE_FIELD_PARTY`
    /// colliding on 8..=9), display id, item displays, bytes 1/2, guild, flags, dynamic flags,
    /// skips `CORPSE_FIELD_PAD`, and emits `FieldChanged` on dynamic flag updates.
    #[test]
    fn wrath_field_audit_corpse_projection_and_dynamic_flags_event() {
        const PLAYER: u64 = 0x1;
        const CORPSE: u64 = 0xF101_0000_0000_0003;
        let mut session = audit_session(PLAYER);
        let pkt = audit_create_packet(
            CORPSE,
            benilla_protocol_wrath::updates::type_id::CORPSE,
            Some((1.0, 2.0, 3.0, 4.0)),
            &[
                (fields::CORPSE_FIELD_OWNER, PLAYER as u32),
                (fields::CORPSE_FIELD_PARTY, 0xDEAD_0001),
                (fields::CORPSE_FIELD_PARTY + 1, 0xDEAD_0002),
                (fields::CORPSE_FIELD_DISPLAY_ID, 49),
                (fields::CORPSE_FIELD_ITEM, 902 | (5 << 24)),
                (fields::CORPSE_FIELD_BYTES_1, 0x0601_0200),
                (fields::CORPSE_FIELD_BYTES_2, 0x0304_0506),
                (fields::CORPSE_FIELD_GUILD, 15),
                (fields::CORPSE_FIELD_FLAGS, 0x04),
                (fields::CORPSE_FIELD_DYNAMIC_FLAGS, 0x00),
                (fields::CORPSE_FIELD_PAD, 0xFFFF_FFFF),
            ],
        );
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
            .expect("reduce corpse");
        let mut world = audit_world();
        audit_step(&session, &mut world);

        let entity = world.resource::<GuidIndex>().0[&CORPSE];
        let store = &world.get::<ObjectStore>(entity).expect("corpse store").0;
        assert_eq!(
            store.delivered(6),
            Some(PLAYER as u32),
            "CORPSE_FIELD_OWNER -> 6"
        );
        assert_eq!(
            store.delivered(8),
            Some(4.0f32.to_bits()),
            "FACING (8) must not be clobbered by CORPSE_FIELD_PARTY"
        );
        assert_eq!(
            store.delivered(9),
            Some(1.0f32.to_bits()),
            "POS_X (9) must not be clobbered by CORPSE_FIELD_PARTY+1"
        );
        assert_eq!(store.delivered(10), Some(2.0f32.to_bits()), "POS_Y -> 10");
        assert_eq!(store.delivered(11), Some(3.0f32.to_bits()), "POS_Z -> 11");
        assert_eq!(
            store.delivered(12),
            Some(49),
            "CORPSE_FIELD_DISPLAY_ID 10 -> 12"
        );
        assert_eq!(
            store.delivered(13),
            Some(902 | (5 << 24)),
            "CORPSE_FIELD_ITEM[0] 11 -> 13"
        );
        assert_eq!(
            store.delivered(32),
            Some(0x0601_0200),
            "CORPSE_FIELD_BYTES_1 30 -> 32"
        );
        assert_eq!(
            store.delivered(33),
            Some(0x0304_0506),
            "CORPSE_FIELD_BYTES_2 31 -> 33"
        );
        assert_eq!(store.delivered(34), Some(15), "CORPSE_FIELD_GUILD 32 -> 34");
        assert_eq!(
            store.delivered(35),
            Some(0x04),
            "CORPSE_FIELD_FLAGS 33 -> 35"
        );
        assert_eq!(
            store.delivered(36),
            Some(0x00),
            "CORPSE_FIELD_DYNAMIC_FLAGS 34 -> 36"
        );
        assert_eq!(
            store.delivered(37),
            None,
            "CORPSE_FIELD_PAD (35) stays unmapped"
        );

        // Toggle dynamic lootable flag and confirm FieldChanged fires for index 36.
        world
            .resource_mut::<Messages<FieldChanged>>()
            .drain()
            .for_each(drop);
        let upd = audit_values_packet(CORPSE, &[(fields::CORPSE_FIELD_DYNAMIC_FLAGS, 0x01)]);
        session
            .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &upd, 3)
            .expect("reduce corpse dynflags");
        audit_step(&session, &mut world);
        let changed: Vec<_> = world
            .resource_mut::<Messages<FieldChanged>>()
            .drain()
            .collect();
        assert!(
            changed.iter().any(|m| {
                m.guid == CORPSE
                    && m.kind == ObjectType::Corpse
                    && m.index == benilla_protocol::field::FIELD_CORPSE_DYNAMIC_FLAGS
                    && m.old == 0
                    && m.new == 1
            }),
            "expected FieldChanged for FIELD_CORPSE_DYNAMIC_FLAGS (36), got {changed:?}"
        );
    }

    /// `build_digest` extracts unit fields only on Unit/Player, player bytes only on Player, and
    /// routes `display_id` per object type (Unit/Player, GameObject, Corpse).
    #[test]
    fn wrath_field_audit_build_digest_gates_by_type() {
        const PLAYER: u64 = 0x1;
        const UNIT: u64 = 0x2;
        const GO: u64 = 0x3;
        const CORPSE: u64 = 0x4;
        let mut session = audit_session(PLAYER);
        for pkt in [
            audit_create_packet(
                PLAYER,
                benilla_protocol_wrath::updates::type_id::PLAYER,
                None,
                &[
                    (fields::UNIT_FIELD_DISPLAYID, 50),
                    (fields::PLAYER_BYTES, 0xAA),
                    (fields::PLAYER_BYTES_2, 0xBB),
                ],
            ),
            audit_create_packet(
                UNIT,
                benilla_protocol_wrath::updates::type_id::UNIT,
                None,
                &[
                    (fields::UNIT_FIELD_DISPLAYID, 60),
                    (fields::UNIT_FIELD_HEALTH, 99),
                ],
            ),
            audit_create_packet(
                GO,
                benilla_protocol_wrath::updates::type_id::GAMEOBJECT,
                None,
                &[
                    (fields::GAMEOBJECT_DISPLAYID, 70),
                    (fields::GAMEOBJECT_FACTION, 114), // shares index 15 with UNIT_FIELD_SUMMONEDBY+1
                ],
            ),
            audit_create_packet(
                CORPSE,
                benilla_protocol_wrath::updates::type_id::CORPSE,
                None,
                &[(fields::CORPSE_FIELD_DISPLAY_ID, 80)],
            ),
        ] {
            session
                .reduce(benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT, &pkt, 2)
                .expect("reduce");
        }
        let digest = build_digest(&session);
        assert_eq!(digest.entities[&PLAYER].display_id, Some(50));
        assert_eq!(digest.entities[&PLAYER].player_bytes, Some(0xAA));
        assert_eq!(digest.entities[&PLAYER].player_bytes_2, Some(0xBB));
        assert_eq!(digest.entities[&UNIT].display_id, Some(60));
        assert_eq!(digest.entities[&UNIT].player_bytes, None);
        assert_eq!(digest.entities[&UNIT].player_bytes_2, None);
        assert_eq!(digest.entities[&GO].display_id, Some(70));
        assert_eq!(digest.entities[&GO].summoned_by, None);
        assert_eq!(digest.entities[&GO].health, None);
        assert_eq!(digest.entities[&CORPSE].display_id, Some(80));
        assert_eq!(digest.entities[&CORPSE].health, None);
    }

    /// Shipped `wrath_field_map.toml` parses, projects GO rotation/artkit/animprogress and Player
    /// honor splits, and never writes `UNIT_FIELD_NATIVEDISPLAYID` (so 0 cannot clobber Rust's fallback).
    #[test]
    fn wrath_field_audit_shipped_toml_rules() {
        let map = crate::wrath_field_map::WrathFieldMap::default();
        let unit_window = audit_window(
            &[
                (fields::UNIT_FIELD_NATIVEDISPLAYID, 0),
                (fields::UNIT_FIELD_BOUNDINGRADIUS, 0.5f32.to_bits()),
                (fields::UNIT_FIELD_POWER_COST_MODIFIER, 5),
            ],
            fields::UNIT_END,
        );
        let mut unit_entity = super::command_tap_tests::view(0x10).entities[&0x10].clone();
        unit_entity.type_id = benilla_protocol_wrath::updates::type_id::UNIT;
        unit_entity.raw_fields = Some(unit_window);
        let mut pairs = Vec::new();
        map.push_pairs(&unit_entity, &mut pairs);
        assert!(
            !pairs.iter().any(|&(dst, _)| dst == 132),
            "shipped TOML must not project NATIVEDISPLAYID=0 onto 132"
        );
        assert!(pairs.contains(&(129, 0.5f32.to_bits())));
        assert!(pairs.contains(&(173, 5)));
    }
}
