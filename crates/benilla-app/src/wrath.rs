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

use bevy::prelude::*;
use benilla_world::schedule::WorldStage;
use benilla_world::world_map::CurrentMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use benilla_protocol::{
    messages::ObjectType, wire::Vector3d, Character, CharEnumItem, EntityKind, MoveSpeeds,
    ObjectFields, RosterCause, SessionEvent, TalentGroupInfo,
};
use crate::client_state::ClientState;
use benilla_protocol_wrath::cast_failure;
use benilla_wm_native::{
    ActionBridge, ActionCorrelation, ActionEvidence, ActionIntent, ActionRequest, ActionResult,
    ActionStatus,
};
use benilla_protocol_wrath::gameplay::{self, ServerEvent as WrathGameplayEvent};
use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
use benilla_ui::script::{QuestSlotView, QUEST_LOG_SLOTS};
use crate::entities::Equipment;
use crate::items::{Countdowns, ItemObject};
use crate::net::{
    CharPick, CharRequest, ClientCommand, Guid, GuidIndex, NetCommandTap, NetCommands, NetEntity,
    ObjectStore, ParkRequest, ParkSender, SelfGuid, SelfPlayer, UnitSpeeds,
    WorldEnterCascadeMessage, EnteredWorldMessage, FieldChanged,
};
use benilla_protocol::messages::{
    ActionButton, AttackSwingError, AttackerState as ProtocolAttackerState,
    GossipOption as ProtocolGossipOption, ItemDamage as ProtocolItemDamage, ItemInfo,
    ItemPushResult, ItemSpellEntry as ProtocolItemSpell,
    ItemUseSpell as ProtocolItemUseSpell, LootAllPassed as ProtocolLootAllPassed,
    LootItem as ProtocolLootItem, LootRoll as ProtocolLootRoll,
    LootRollWon as ProtocolLootRollWon, LootStartRoll as ProtocolLootStartRoll,
    NpcTextBlock as ProtocolNpcTextBlock, PeriodicAuraLog as ProtocolPeriodicAuraLog,
    PeriodicTick as ProtocolPeriodicTick, QuestComplete, QuestDetails, QuestGiverList,
    QuestListEntry, QuestObjective, QuestOfferReward, QuestRequestItems, QuestRewardItem,
    QuestConfirmAccept as ProtocolQuestConfirmAccept, QuestTemplate,
    SpellDamageLog as ProtocolSpellDamageLog,
    ChatMessage as ProtocolChatMessage, TrainerSpell as ProtocolTrainerSpell,
    VendorItem as ProtocolVendorItem,
};

use benilla_protocol_wrath::session::{LifeEvent, Phase, SelfObservation, SessionState};

/// Bounded lifecycle queue depth: lifetimes of interest are short (one world transfer is one
/// event); 16 gives many frames of headroom at one-event-per-transfer rates.
pub const WRATH_EVENT_CAPACITY: usize = 16;
/// Native cast/channel/item-text queue depth. Separate from the lifecycle queue so game traffic
/// cannot evict a terminal lifecycle edge.
pub const WRATH_NATIVE_EVENT_CAPACITY: usize = 64;

/// Per-frame drain bound for the Net-stage system: a burst longer than this leaves the rest
/// for the next frame instead of starving input (R5 budget; measured, not tuned, per frame).
pub const WRATH_DRAIN_LIMIT: usize = 64;

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
        SessionEvent::LoggedOut => Some(0),
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
            self.dropped += 1;
            return;
        }
        if let Some(scope) = self.adopted {
            if class != SessionClass::Terminal && SessionReplies::superseded(scope, &event) {
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
        self.fifo
            .retain(|queued| !SessionReplies::superseded(scope, queued));
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
    WorldTransfer { session_epoch: u64, world_generation: u64, map_id: u32 },
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
    /// Validated `SMSG_MULTICLASS_REPLY` (`0x51A`, WMC1 v3) frame for `MulticlassPoll`.
    MulticlassReply {
        op: u16,
        status: u16,
        serial: u32,
        payload: Vec<u8>,
        session_epoch: u64,
        world_generation: u64,
    },
}

impl WrathFeedEvent {
    fn from_session(event: LifeEvent, session_epoch: u64) -> Self {
        match event {
            LifeEvent::WorldTransfer { world_generation, map_id } => {
                Self::WorldTransfer {
                    session_epoch,
                    world_generation,
                    map_id,
                }
            }
        }
    }
}

/// Shared handles between the IO thread and the app.
#[derive(Clone)]
pub struct WrathFeed {
    authenticated: Arc<AtomicBool>,
    trace: Option<crate::quest_trace::Source>,
    account_expansion: Arc<std::sync::atomic::AtomicU8>,
    last_session_epoch: Arc<AtomicU64>,
    last_world_generation: Arc<AtomicU64>,
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
            authenticated: Arc::new(AtomicBool::new(false)),
            trace: None,
            account_expansion: Arc::new(std::sync::atomic::AtomicU8::new(0)),
            last_session_epoch: Arc::new(AtomicU64::new(0)),
            last_world_generation: Arc::new(AtomicU64::new(0)),
            latest: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(WRATH_EVENT_CAPACITY))),
            native: Arc::new(Mutex::new(VecDeque::with_capacity(
                WRATH_NATIVE_EVENT_CAPACITY,
            ))),
            native_dropped: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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

impl WrathSessionConfig {
    /// Build the selected graphical path from the same protected environment the rest of the
    /// client uses. Credentials never come from committed source or command-line arguments.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        use anyhow::Context;
        if !selected_from_env() {
            return Ok(None);
        }
        let interactive = interactive_from_env();
        let account = if interactive { String::new() } else {
            std::env::var("WOW_USER").context("WOW_WRATH needs WOW_USER")?
        };
        let password = if interactive { String::new() } else {
            std::env::var("WOW_PASS").context("WOW_WRATH needs WOW_PASS")?
        };
        let auth_addr =
            std::env::var("WOW_WRATH_AUTH").unwrap_or_else(|_| "127.0.0.1:3724".to_owned());
        let world_addr =
            std::env::var("WOW_WRATH_WORLD").unwrap_or_else(|_| {
                if interactive { String::new() } else { "127.0.0.1:8095".to_owned() }
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
            character_name: std::env::var("WOW_CHAR").ok()
                .map(|name| name.trim().to_owned()).filter(|name| !name.is_empty()),
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
    /// Wrath `PLAYER_BYTES`: skin, face, hair style, hair colour.
    pub player_bytes: Option<u32>,
    /// Wrath `PLAYER_BYTES_2`: facial hair, bank slots, rest state, arena faction.
    pub player_bytes_2: Option<u32>,
    pub position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    /// The last reduced movement flags for this object.
    pub move_flags: Option<u32>,
    /// Active spline carried by the create/movement update block (`SPLINE_ENABLED`).
    pub create_spline: Option<benilla_protocol_wrath::updates::CreateSplineSnapshot>,
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
            Self::Auto if interactive => Self::Parked { roster_asked: false },
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
    Realm(benilla_protocol::RealmInfo),
    RefreshRealms,
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
        let Some(pending) = self.pending.remove(&id) else { return };
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
        let Some(identity) = &event.identity else { return };
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
            let Some(pending) = self.pending.remove(&id) else { continue };
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
                (true, _) => (
                    ActionStatus::Succeeded,
                    Some("the cast ended".to_owned()),
                ),
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
        self.feed.as_ref().map_or(0, |f| f.account_expansion.load(Ordering::SeqCst))
    }
    /// Whether a worker is currently running.
    pub fn running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
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
        let Some(mut config) = self.config.clone() else { return };
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
        if self.ended {
            if !self.restart_allowed || self.restart_attempts == 0 {
                return;
            }
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
                    if config.login_generation.is_some() && result.is_err()
                        && !feed.authenticated.load(Ordering::SeqCst)
                    {
                        push_forced_event(
                            &events_out,
                            SessionEvent::LoginFailed {
                                refusal: None,
                                reason: result.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default(),
                                dial: None,
                            },
                            config.login_generation,
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

    /// Bounded app-owned stop: signal the loop and join for a short grace period. A worker still
    /// inside a blocking socket read remains in `retiring`; no replacement can overlap it.
    pub fn stop(&mut self) {
        self.stop_with_delivery(None);
    }

    /// [`Self::stop`] with the typed action seam, so a shutdown retires pending intents and
    /// proposals instead of leaving them for a follower that never comes.
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
                DeliveryScopeEnd::SessionEnded => {
                    actions.retire_all(ActionStatus::Unknown, reason)
                }
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
            Some(replies) => replies.lock().map(|replies| replies.is_empty()).unwrap_or(true),
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
            self_observation: None,
            entities: BTreeMap::new(),
            last_changes: WrathChanges::default(),
            entered_world: false,
            gossip_menu: None,
            vendor: None,
            server_clock: None,
            played_time: None,
            casts: BTreeMap::new(),
            channels: BTreeMap::new(),
            last_cast_end: None,
            last_inventory_failure: None,
            multiclass_replies: VecDeque::new(),
            next_cast_count: 1,
            empty: (),
        }
    }
}

impl WrathWorldView {
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
            power_type: entity
                .bytes_0
                .map(|bytes| ((bytes >> 24) & 0xff) as u8),
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
}

/// Presentation-side `GetTime` anchor for each observed aura application, keyed by object and
/// slot. The anchor is stamped once when a receipt revision is first seen (from the frame's
/// `UiClock`, the same GetTime domain `UnitAura` callers use) and is replaced only when a newer
/// receipt revision arrives. Wall/server clocks never reach the UI here. Consumers read this
/// read-only owner; the aura slot keeps the wire's remaining duration.
#[derive(Resource, Default)]
pub struct WrathAuraAnchors {
    /// guid -> (object generation, slot -> (receipt revision, GetTime seconds)).
    by_guid: BTreeMap<u64, (u32, BTreeMap<u8, (u64, f64)>)>,
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
pub struct WrathChanges {    pub created: Vec<u64>,
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
        app.insert_resource(WrathControl {
            config: self.config.clone(),
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
            if !app.world().contains_resource::<crate::quest_trace::Source>() {
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
                .insert_resource(WrathLoginQueue { requests: login_rx, pending: None });
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
            app.world_mut()
                .resource_mut::<WrathControl>()
                .wm_actions = Some(bridge.clone());
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
            app.add_systems(
                Update,
                (
                    wrath_net_drain,
                    wrath_apply_world,
                    wrath_wm_action_intake,
                    wrath_tap_commands,
                    wrath_dispatch_events,
                )
                    .chain()
                    .in_set(WorldStage::Net),
            );
            // After `WorldStage::Input`, which owns the controller that writes `Player`.
            app.add_systems(Update, wrath_stream_movement.in_set(WorldStage::Stream));
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
    let roster_restart = *state.get() == ClientState::CharSelect && control.ended
        && control.restart_allowed && control.config.as_ref().is_some_and(|c|
            c.login_generation.is_some() && c.preferred_realm.is_some());
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
        } else { WrathSessionStart::Auto });
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
    if control.config.as_ref().is_none_or(|c| c.login_generation.is_none()) {
        // The existing direct-entry runner owns its explicit identity and named boot pick.
        for _ in login.requests.try_iter() {}
        return;
    }
    let generation = crate::net::LoginAbandon::now(Some(&abandon));
    if control.config.as_ref().and_then(|c| c.login_generation)
        .is_some_and(|g| g != generation)
    {
        control.cancel.store(true, Ordering::SeqCst);
        login.pending = None;
        if let Some(config) = control.config.as_mut() {
            config.account.clear();
            config.password.clear();
            config.login_generation = Some(generation);
        }
    }
    for request in login.requests.try_iter().collect::<Vec<_>>() {
        if request.generation != generation { continue; }
        let Some(mut config) = control.config.clone() else { continue };
        config.account = request.user;
        config.password = request.pass;
        config.auth_addr = if request.host.contains(':') { request.host }
            else { format!("{}:3724", request.host) };
        config.character_name = None;
        config.login_generation = Some(generation);
        config.preferred_realm = None;
        control.cancel.store(true, Ordering::SeqCst);
        login.pending = Some(config);
    }
    if login.pending.is_some() && control.worker.is_none() && control.retiring.is_none()
        && control.terminal_ready && control.session_events_empty()
        && control.publisher_retiring.is_empty()
    {
        control.config = login.pending.take();
        control.ended = false;
        control.shutdown = false;
        control.retry_at = None;
        control.start(WrathSessionStart::Parked { roster_asked: false });
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
                        if generation == crate::net::LoginAbandon::now(Some(&abandon)) => {
                        if let Some(config) = control.config.as_mut() {
                            config.preferred_realm = Some(realm.id);
                        }
                        if at_select && control.feed.as_ref()
                            .is_some_and(|f| f.authenticated.load(Ordering::SeqCst)) {
                            if let Some(mut config) = control.config.clone() {
                                config.preferred_realm = Some(realm.id);
                                config.login_generation = Some(generation);
                                login.pending = Some(config);
                                control.cancel.store(true, Ordering::SeqCst);
                            }
                        } else if let Some(tx) = control.park_commands.as_ref() {
                            let _ = tx.send(WrathParkedCommand::Realm(realm));
                        }
                    }
                    RealmRequest::Refresh => {
                        let sent = control.park_commands.as_ref().is_some_and(|tx|
                            tx.send(WrathParkedCommand::RefreshRealms).is_ok());
                        if !sent {
                            realm_lists.write(crate::net::RealmListMessage {
                                realms: Vec::new(), failed: true,
                            });
                        }
                    }
                    RealmRequest::Close if !at_select => {
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
            CharRequest::Abandon => {
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
                Some("cancellation arrived after dispatch; the command may still execute".to_owned()),
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
                Some("no remote movement owner exists; movement is the human input controller".to_owned()),
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
                } else if let Some(channel) = view.channeling(view.player_guid) {
                    Some(format!(
                        "the client already observes channel spell {}",
                        channel.spell_id
                    ))
                } else {
                    None
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
                let (spell_id, wire_cast_count) =
                    if let Some(cast) = view.casting(view.player_guid) {
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
                    commands.push(ClientCommand::CancelCast { spell_id, trace: None });
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
                .or_else(|| view.channeling(view.player_guid).map(|channel| channel.spell_id))
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
        _ => 7,                         // Common
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

fn quest_panel_extra(extra: gameplay::QuestPanelExtra) -> benilla_protocol::messages::QuestPanelExtra {
    benilla_protocol::messages::QuestPanelExtra {
        flags: extra.flags, suggested_players: extra.suggested_players, xp: extra.xp,
        honor_tenths: extra.honor_tenths, honor_multiplier: extra.honor_multiplier,
        cast_spell: extra.cast_spell, title_id: extra.title_id, talents: extra.talents,
        arena_points: extra.arena_points, reputation: extra.reputation,
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
        WrathGameplayEvent::MonsterMove(m) => {
            // 3.3.5a (`MoveSplineFlag.h`): `0x1000` is `Walkmode`, `0x2000` is `Flying`,
            // `0x4000` is `OrientationFixed`, `0x40000` is `Catmullrom` (already expanded into
            // uncompressed waypoints in `gameplay/movement.rs`), and `0x80000` is `Cyclic`.
            // Only exclude `Falling` (`0x200`), `Parabolic` (`0x800`), and `Frozen` (`0x400000`).
            let unsupported = m.flags & (0x200 | 0x800 | 0x400000);
            if m.guid == self_guid || unsupported != 0 {
                debug!(guid = m.guid, flags = m.flags, "Wrath spline needs unsupported evaluator");
                return Vec::new();
            }
            let facing = match m.facing {
                gameplay::MonsterFacing::None => benilla_protocol::MonsterMoveFacing::None,
                gameplay::MonsterFacing::Spot(p) => benilla_protocol::MonsterMoveFacing::Spot(p),
                gameplay::MonsterFacing::Target(g) => benilla_protocol::MonsterMoveFacing::Target(g),
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
            victim_state,
            absorb,
            resist,
            blocked,
            melee_spell_id,
            school,
        } => vec![SessionEvent::AttackerState(ProtocolAttackerState {
            attacker,
            victim,
            hit_info,
            damage,
            victim_state,
            absorb,
            resist,
            blocked,
            melee_spell_id,
            school,
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
            school,
            absorb,
            resist,
            // 12340's byte is `physicalLog`; periodic damage rides `SMSG_PERIODICAURALOG`.
            periodic: false,
            blocked,
            hit_info,
        })],
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
                    } => ProtocolPeriodicTick::Damage {
                        amount,
                        school,
                        absorb,
                        resist,
                    },
                    gameplay::PeriodicAuraTick::Heal { amount } => {
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
        WrathGameplayEvent::ItemCooldown { item_guid, spell_id } => {
            vec![SessionEvent::ItemCooldown { item_guid, spell_id }]
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
        WrathGameplayEvent::LogoutResponse { reason, instant } => {
            vec![SessionEvent::LogoutResponse { reason, instant }]
        }
        WrathGameplayEvent::LoggedOut => vec![SessionEvent::LoggedOut],
        WrathGameplayEvent::LogoutCancelled => vec![SessionEvent::LogoutCancelled],
        WrathGameplayEvent::SpellFailure {
            caster,
            spell_id,
            raw_reason,
            ..
        } => {
            let mut out = Vec::with_capacity(2);
            if caster == self_guid {
                let reason = match cast_failure::reason(raw_reason) {
                    cast_failure::Reason::Vanilla(r) => Some(r),
                    cast_failure::Reason::Unsupported(_) => Some(0x1c),
                };
                out.push(SessionEvent::CastResult {
                    spell_id,
                    success: false,
                    reason,
                    arg: None,
                });
            }
            out.push(SessionEvent::SpellFailedOther { caster, spell_id });
            out
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
                    .map(|(target, info, reflect_result)| {
                        benilla_protocol::messages::SpellGoMiss {
                            target,
                            info,
                            reflect_result,
                        }
                    })
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
        WrathGameplayEvent::GameObjectInfo { entry, type_id, display_id, name, data } => {
            vec![SessionEvent::GameObjectInfo { entry, type_id, display_id, name, data }]
        }
        WrathGameplayEvent::PlayerName { guid, name, race, gender, class } => {
            vec![SessionEvent::PlayerName { guid, name, race, gender, class }]
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
        } => vec![SessionEvent::QuestConfirmAccept(ProtocolQuestConfirmAccept {
            quest_id,
            title,
            sender,
        })],
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
                chat_tag: line.chat_tag,
            })],
            None => {
                debug!("wrath: chat type 0x{:X} has no 1.12 owner", line.chat_type);
                Vec::new()
            }
        },
        WrathGameplayEvent::QuestGiverStatus { npc, status } => {
            vec![SessionEvent::QuestGiverStatus { npc, status }]
        }
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
        WrathGameplayEvent::QuestTemplate(t) => vec![SessionEvent::QuestTemplate(Box::new(
            QuestTemplate {
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
            },
        ))],
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
        // 12340's map id, stack count and vote mask have no donor field. They stay on the
        // protocol event for the owner extension recorded in the group-loot checkpoint; the
        // donor roll owner consumes the identity, item, countdown and both property words.
        WrathGameplayEvent::LootStartRoll {
            guid,
            map_id: _,
            item_slot,
            item_id,
            suffix_factor,
            random_property_id,
            item_count: _,
            countdown_ms,
            vote_mask: _,
        } => vec![SessionEvent::LootStartRoll(ProtocolLootStartRoll {
            looted_target: guid,
            item_slot,
            item_id,
            random_property_id,
            suffix_factor,
            countdown_ms,
        })],
        // `auto_pass` has no donor field; it is retained on the protocol event for the chat-line
        // extension recorded in the checkpoint.
        WrathGameplayEvent::LootRoll {
            guid,
            item_slot,
            roller,
            item_id,
            suffix_factor,
            random_property_id,
            roll_number,
            roll_type,
            auto_pass: _,
        } => vec![SessionEvent::LootRoll(ProtocolLootRoll {
            looted_target: guid,
            item_slot,
            roller,
            item_id,
            random_property_id,
            suffix_factor,
            roll_number,
            roll_type,
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
        WrathGameplayEvent::ItemPush(p) => vec![SessionEvent::ItemPushResult(ItemPushResult {
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
        })],
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
        WrathGameplayEvent::PetDismissSound {
            model_id,
            position,
        } => vec![SessionEvent::PetDismissSound {
            model_id: model_id as u32,
            position,
        }],
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
                (None, Vec::new(), info.pet_talents.map(|p| p.talents).unwrap_or_default())
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
                    .map(|result| benilla_protocol::messages::AuctionCommandTail::Inventory { result })
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
            let auctions = list
                .auctions
                .into_iter()
                .map(wrath_auction_entry)
                .collect();
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
        WrathGameplayEvent::GroupList(list) => vec![wrath_group_list(*list)],
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
        WrathGameplayEvent::GuildInfo(info) => vec![SessionEvent::GuildInfo(wrath_guild_info(info))],
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
        // The 12340 systems below decode exactly and are ready for their app state owners, but
        // the current 1.12 UI has no LFG, battlefield, calendar or rune view to receive them.
        // Keep the decoded records in the worker's trace path rather than projecting them into
        // an unrelated SessionEvent (a notification or chat line would misreport the wire).
        WrathGameplayEvent::LfgJoinResult(_)
        | WrathGameplayEvent::LfgQueueStatus(_)
        | WrathGameplayEvent::LfgUpdatePlayer(_)
        | WrathGameplayEvent::LfgUpdateParty(_)
        | WrathGameplayEvent::LfgProposalUpdate(_)
        | WrathGameplayEvent::BattlefieldStatus(_)
        | WrathGameplayEvent::PvpLogData(_)
        | WrathGameplayEvent::CalendarSendCalendar(_)
        | WrathGameplayEvent::CalendarCommandResult(_)
        | WrathGameplayEvent::RuneConvert(_)
        | WrathGameplayEvent::RuneState(_)
        | WrathGameplayEvent::RunePower(_) => {
            debug!("wrath: decoded 12340 LFG/battlefield/calendar/rune reply; no app view owner yet");
            Vec::new()
        }
    }
}

fn wrath_group_list(list: gameplay::social::GroupList) -> SessionEvent {
    let members = list
        .members
        .into_iter()
        .map(|member| benilla_protocol::messages::GroupMemberEntry {
            name: member.name,
            guid: member.guid,
            status: member.status,
            // The donor row packs the subgroup and assistant bit into one byte; 12340 sends
            // subgroup, member flags and LFG roles separately.
            flags: (member.sub_group & 0x7) | (member.flags & 0x80),
        })
        .collect();
    let loot = list.loot.map(|loot| benilla_protocol::messages::GroupLootInfo {
        method: loot.method,
        master: loot.master,
        threshold: loot.threshold,
    });
    SessionEvent::GroupList {
        group_type: list.group_type,
        own_flags: list.flags,
        members,
        leader: list.leader_guid,
        loot,
    }
}

fn wrath_party_stats(stats: gameplay::social::PartyMemberStats) -> SessionEvent {
    let f = stats.fields;
    let mut info = benilla_protocol::messages::PartyMemberStatsInfo {
        status: f.status.map(|status| status as u8),
        cur_hp: f.cur_hp.map(|value| value.min(u32::from(u16::MAX)) as u16),
        max_hp: f.max_hp.map(|value| value.min(u32::from(u16::MAX)) as u16),
        power_type: f.power_type,
        cur_power: f.cur_power,
        max_power: f.max_power,
        level: f.level,
        zone: f.zone,
        position: f.position.map(|(x, y)| (x as i16, y as i16)),
        ..Default::default()
    };
    if let Some(auras) = f.auras {
        info.auras = Some(
            auras
                .auras
                .iter()
                .filter(|aura| aura.slot < 32)
                .map(|aura| (aura.slot, aura.spell_id.min(u32::from(u16::MAX)) as u16))
                .collect(),
        );
        info.auras_negative = Some(
            auras
                .auras
                .iter()
                .filter(|aura| (32..48).contains(&aura.slot))
                .map(|aura| (aura.slot, aura.spell_id.min(u32::from(u16::MAX)) as u16))
                .collect(),
        );
    }
    info.pet_guid = f.pet_guid;
    info.pet_name = f.pet_name;
    info.pet_model_id = f.pet_model_id;
    info.pet_cur_hp = f.pet_cur_hp.map(|value| value.min(u32::from(u16::MAX)) as u16);
    info.pet_max_hp = f.pet_max_hp.map(|value| value.min(u32::from(u16::MAX)) as u16);
    info.pet_power_type = f.pet_power_type;
    info.pet_cur_power = f.pet_cur_power;
    info.pet_max_power = f.pet_max_power;
    if let Some(auras) = f.pet_auras {
        info.pet_auras = Some(
            auras
                .auras
                .iter()
                .filter(|aura| aura.slot < 32)
                .map(|aura| (aura.slot, aura.spell_id.min(u32::from(u16::MAX)) as u16))
                .collect(),
        );
        info.pet_auras_negative = Some(
            auras
                .auras
                .iter()
                .filter(|aura| (32..48).contains(&aura.slot))
                .map(|aura| (aura.slot, aura.spell_id.min(u32::from(u16::MAX)) as u16))
                .collect(),
        );
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

fn wrath_channel_notify(
    notify: gameplay::social::ChannelNotify,
) -> SessionEvent {
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

fn wrath_guild_info(info: gameplay::social::GuildInfoWire) -> benilla_protocol::messages::GuildInfo {
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
fn wrath_trade_status(status: gameplay::economy::TradeStatus) -> benilla_protocol::messages::TradeStatus {
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

/// Map a 12340 trade side onto the donor state. The 3.3.5 `stateIndex` is the stale-offer guard
/// the donor calls `sequence`; its per-side state word is the donor's `flag`. Socket-enchant
/// words are retained by the Wrath decoder but have no donor field, so they are not fabricated
/// into an enchant.
fn wrath_trade_extended(state: gameplay::economy::TradeExtended) -> benilla_protocol::messages::TradeStatusExtended {
    let mut slots = [None; gameplay::economy::TRADE_SLOT_COUNT];
    for (out, item) in slots.iter_mut().zip(state.slots) {
        *out = item.map(|item| benilla_protocol::messages::TradeItem {
            entry: item.entry,
            display_id: item.display_id,
            count: item.count,
            wrapped: item.wrapped,
            gift_creator: item.gift_creator,
            perm_enchant: item.perm_enchant,
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
        sequence: state.state_index,
        flag: state.sequence,
        gold: state.gold,
        enchant_spell_id: state.enchant_spell_id,
        slots,
    }
}

fn wrath_mail_list(list: gameplay::economy::MailList) -> Vec<benilla_protocol::messages::MailListEntry> {
    list.mails
        .into_iter()
        .map(|mail| {
            if !mail.body.is_empty() {
                debug!(
                    "wrath: mail {} carries its body inline; the donor inbox fetches bodies separately",
                    mail.message_id
                );
            }
            let item = mail.attachments.first().map(|attachment| {
                benilla_protocol::messages::MailAttachment {
                    entry: attachment.item_entry,
                    perm_enchant: attachment.enchants[0].id,
                    random_prop_id: attachment.random_prop_id as u32,
                    suffix_factor: attachment.suffix_factor,
                    count: u8::try_from(attachment.count).unwrap_or(u8::MAX),
                    charges: attachment.charges,
                    durability_max: attachment.max_durability,
                    durability: attachment.durability,
                }
            });
            if mail.attachments.len() > 1 {
                debug!(
                    "wrath: mail {} carries {} attachments; the donor inbox shows the first",
                    mail.message_id,
                    mail.attachments.len()
                );
            }
            benilla_protocol::messages::MailListEntry {
                message_id: mail.message_id,
                message_type: mail.message_type,
                sender_guid: mail.sender_guid,
                sender_id: mail.sender_entry,
                subject: mail.subject,
                item_text_id: 0,
                package: mail.package,
                stationery: mail.stationery,
                item,
                money: mail.money,
                cod: mail.cod,
                checked: mail.flags,
                expire_days: mail.expire_days,
                mail_template_id: mail.mail_template_id,
            }
        })
        .collect()
}

fn wrath_auction_entry(entry: gameplay::economy::AuctionEntry) -> benilla_protocol::messages::AuctionListEntry {
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
) {
    let Some(mut tap) = tap else { return };
    let Some(receiver) = tap.0.as_mut() else { return };
    while let Ok(raw_command) = receiver.try_recv() {
        // Unwrap the shared transport-identity envelope exactly once before matching the inner
        // command. Its trace and scope flag are carried alongside the existing per-variant traces.
        let (env_trace, env_enforce_scope, from_tracked, command) = match raw_command {
            ClientCommand::Tracked { trace, enforce_scope, command } => (trace, enforce_scope, true, *command),
            other => (None, false, false, other),
        };
        let opening = matches!(&command, ClientCommand::GossipHello { .. } | ClientCommand::QuestgiverHello { .. } | ClientCommand::QuestgiverOpenTracked { .. });
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
                    ClientCommand::QuestgiverComplete { npc: receipt.npc, quest: receipt.quest }
                } else {
                    ClientCommand::QuestgiverQuery { npc: receipt.npc, quest: receipt.quest }
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
                Some((benilla_protocol_wrath::opcode::CMSG_SET_SELECTION, gameplay::set_selection(guid)))
            }
            ClientCommand::AttackSwing { guid } => {
                if !view.entities.contains_key(&guid) {
                    continue;
                }
                Some((benilla_protocol_wrath::opcode::CMSG_ATTACKSWING, gameplay::attack_swing(guid)))
            }
            ClientCommand::AttackStop => Some((
                benilla_protocol_wrath::opcode::CMSG_ATTACKSTOP,
                gameplay::attack_stop(),
            )),
            ClientCommand::CastSpell { spell_id, target, .. } => {
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
            ClientCommand::GossipSelectOption {
                guid,
                option,
                code,
            } => {
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
            ClientCommand::QuestgiverChooseReward {
                npc,
                quest,
                choice,
            } => Some((
                benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_CHOOSE_REWARD,
                gameplay::questgiver_choose_reward(npc, quest, choice),
            )),
            ClientCommand::NameQuery { guid } => Some((
                benilla_protocol_wrath::opcode::CMSG_NAME_QUERY,
                guid.to_le_bytes().to_vec(),
            )),
            ClientCommand::GameObjectQuery { entry, guid } => {
                Some((
                    benilla_protocol_wrath::opcode::CMSG_GAMEOBJECT_QUERY,
                    gameplay::gameobject_query(entry, guid),
                ))
            }
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
                debug!(
                    "wrath: action button {button} is outside the 12340 0..143 range; refused"
                );
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
            ClientCommand::SelfRes => Some((
                benilla_protocol_wrath::opcode::CMSG_SELF_RES,
                Vec::new(),
            )),
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
            ClientCommand::QuestPushResultForQuest {
                sharer,
                quest,
                msg,
            } => Some((
                u32::from(benilla_protocol_wrath::opcode::MSG_QUEST_PUSH_RESULT),
                gameplay::quest_push_result(sharer, quest, msg.0),
            )),
            ClientCommand::QuestPushResult { .. } => {
                debug!(
                    "wrath: quest-share decline refused: the 12340 packet requires a quest id"
                );
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
            ClientCommand::CastSpellGameObject { spell_id, go_guid, .. } => {
                let count = view.next_cast_count;
                admitted_wire_count = Some(count);
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_gameobject(count, spell_id, go_guid),
                ))
            }
            ClientCommand::CastSpellItem { spell_id, item_guid, .. } => {
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
                    debug!(
                        "wrath: item use dropped (no item guid or an unsupported target block)"
                    );
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
                gameplay::message_chat(
                    addon_chat_type(distribution),
                    0xFFFF_FFFF,
                    None,
                    &text,
                ),
            )),
            ClientCommand::LearnTalent { talent_id, rank } => Some((
                benilla_protocol_wrath::opcode::CMSG_LEARN_TALENT,
                gameplay::encode_learn_talent(talent_id, rank),
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
            ClientCommand::TogglePvp => Some((
                benilla_protocol_wrath::opcode::CMSG_TOGGLE_PVP,
                Vec::new(),
            )),
            ClientCommand::ToggleHelm => Some((
                benilla_protocol_wrath::opcode::CMSG_SHOWING_HELM,
                vec![1],
            )),
            ClientCommand::ToggleCloak => Some((
                benilla_protocol_wrath::opcode::CMSG_SHOWING_CLOAK,
                vec![1],
            )),
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
            ClientCommand::SetFactionAtWar { rep_list_id, at_war } => Some((
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
            ClientCommand::BeginTrade => Some((
                benilla_protocol_wrath::opcode::CMSG_BEGIN_TRADE,
                Vec::new(),
            )),
            ClientCommand::BusyTrade => Some((
                benilla_protocol_wrath::opcode::CMSG_BUSY_TRADE,
                Vec::new(),
            )),
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
            ClientCommand::GuildInfoRequest => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_INFO,
                Vec::new(),
            )),
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
            ClientCommand::GuildLeave => Some((
                benilla_protocol_wrath::opcode::CMSG_GUILD_LEAVE,
                Vec::new(),
            )),
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
            _ => None,
        };
        if let Some((opcode, payload)) = packet {
            if let Some(trace) = &primary_trace {
                trace.record(crate::quest_trace::Stage::Encoded);
            }
            if let Ok(mut queue) = input.0.lock() {
                if queue.len() < WRATH_OUTBOX_CAP {
                    // The Wrath encoder writes one cast-count byte before the spell ID.
                    let admitted_spell = (opcode == benilla_protocol_wrath::opcode::CMSG_CAST_SPELL)
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
                            let receipt_ix = receipt.trace.as_ref().map(|t| t.interaction_id()).unwrap_or(0);
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
                            trace.observe(scope, None, crate::quest_trace::Stage::OpenOutboxAdmitted);
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
                    if let Some(receipt) = receipt.as_ref() {
                        receipt.report(crate::net::CommandDelivery::Rejected(
                            crate::net::CommandRefusal::OutboxFull,
                        ));
                        if let Some(pt) = &primary_trace {
                            let receipt_ix = receipt.trace.as_ref().map(|t| t.interaction_id()).unwrap_or(0);
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
                            trace.observe(scope, None, crate::quest_trace::Stage::OpenOutboxRefused);
                        }
                    }
                }
            } else {
                if let Some(receipt) = receipt.as_ref() {
                    receipt.report(crate::net::CommandDelivery::Rejected(
                        crate::net::CommandRefusal::Unavailable,
                    ));
                    if let Some(pt) = &primary_trace {
                        let receipt_ix = receipt.trace.as_ref().map(|t| t.interaction_id()).unwrap_or(0);
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
const WRATH_OUTBOX_CAP: usize = 256;
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
        CMSG_CAST_SPELL | CMSG_ATTACKSWING | CMSG_ATTACKSTOP | CMSG_LOOT
            | CMSG_LOOT_MONEY | CMSG_LOOT_RELEASE | CMSG_LOOT_ROLL
            | CMSG_AUTOSTORE_LOOT_ITEM | CMSG_LIST_INVENTORY | CMSG_SELL_ITEM
            | CMSG_BUY_ITEM | CMSG_BUY_ITEM_IN_SLOT | CMSG_USE_ITEM | CMSG_AUTOEQUIP_ITEM
    )
}

/// Packet metadata allowed into the local recorder. Payloads are never copied.
fn diagnostic_opcode(opcode: u16) -> bool {
    use benilla_protocol_wrath::opcode::*;
    matches!(
        opcode,
        SMSG_UPDATE_OBJECT | SMSG_GOSSIP_MESSAGE | SMSG_GOSSIP_COMPLETE
            | SMSG_QUESTGIVER_QUEST_LIST | SMSG_QUESTGIVER_QUEST_DETAILS
            | SMSG_QUESTGIVER_REQUEST_ITEMS | SMSG_QUESTGIVER_OFFER_REWARD
            | SMSG_QUESTGIVER_QUEST_INVALID | SMSG_QUESTGIVER_QUEST_FAILED
            | SMSG_QUESTLOG_FULL | SMSG_QUEST_QUERY_RESPONSE
            | SMSG_SPELL_START | SMSG_SPELL_GO | SMSG_CAST_FAILED
            | SMSG_SPELL_FAILURE | SMSG_SPELL_FAILED_OTHER | SMSG_SPELL_DELAYED
            | SMSG_INVENTORY_CHANGE_FAILURE | SMSG_LOOT_RESPONSE | SMSG_ITEM_PUSH_RESULT
    )
}

fn scope_is_older(
    current_epoch: u64,
    current_world: u64,
    event_epoch: u64,
    event_world: u64,
) -> bool {
    event_epoch < current_epoch
        || (event_epoch == current_epoch && event_world < current_world)
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
    let login_generation = crate::net::LoginAbandon::now(world.get_resource::<crate::net::LoginAbandon>());
    let (current_epoch, current_world) = world
        .get_resource::<WrathWorldView>()
        .map_or((0, 0), |view| (view.session_epoch, view.world_generation));
    let Some(mut control) = world.get_resource_mut::<WrathControl>() else {
        return;
    };
    let ended = control.ended;
    let tagged = control.take_session_events();
    drop(control);

    let mut deliver = Vec::with_capacity(tagged.len());
    for event in tagged {
        if event.login_generation.is_some_and(|g| g != login_generation) { continue; }
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
                    SessionEvent::QuestObjectiveKill { .. } | SessionEvent::QuestObjectiveItem { .. }
                    | SessionEvent::QuestObjectivesComplete { .. } => Some(D::QuestObjective),
                    SessionEvent::GossipMenu { .. } => Some(D::GossipMenu),
                    SessionEvent::VendorInventory { .. } => Some(D::VendorList),
                    SessionEvent::VendorBuyResult { .. } => Some(D::VendorBuy),
                    SessionEvent::VendorBuyFailed { .. } => Some(D::VendorBuyFailed),
                    SessionEvent::VendorSellFailed { .. } => Some(D::VendorSellFailed),
                    SessionEvent::LootResponse { .. } => Some(D::LootOpened),
                    SessionEvent::LootError { .. } => Some(D::LootError),
                    SessionEvent::LootRemoved { .. } => Some(D::LootRemoved),
                    SessionEvent::LootMoneyNotify { .. } | SessionEvent::LootClearMoney => Some(D::LootMoney),
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
                let scope = if event.force
                    || (event.session_epoch == 0 && event.world_generation == 0)
                {
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
                    SessionEvent::LootClearMoney => {
                        trace.observe_with_subject(scope, None, None, None, None, None, Stage::LootMoneyCleared)
                    }
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
                    SessionEvent::VendorSellFailed { vendor, reason, .. } => {
                        trace.observe_with_subject(
                            scope,
                            None,
                            Some(*vendor),
                            None,
                            None,
                            None,
                            Stage::VendorSellFailedObserved { reason: *reason },
                        )
                    }
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
            let payload = gameplay::encode_multiclass_request(
                op,
                serial,
                character_guid,
                arg0,
                arg1,
            )
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
    let revision = world.get_resource::<WrathWorldView>().map(|v| v.state_revision);
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
                (SessionEvent::AttackSwingError(_), ActionIntent::AttackStart { .. }) => {
                    Some((ActionStatus::Failed, Some("attack swing refused".to_owned())))
                }
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
                ) if quest_id == quest => {
                    Some((ActionStatus::Failed, Some(format!("quest failed ({reason})"))))
                }
                (SessionEvent::LootRemoved { slot }, ActionIntent::LootTake { slot: wanted, .. })
                    if slot == wanted =>
                {
                    Some((ActionStatus::Succeeded, None))
                }
                (SessionEvent::LootMoneyNotify { .. }, ActionIntent::LootMoney { .. }) => {
                    Some((ActionStatus::Succeeded, None))
                }
                (
                    SessionEvent::LootReleaseResponse { guid, .. },
                    ActionIntent::LootRelease { source },
                ) if guid == source => Some((ActionStatus::Succeeded, None)),
                (SessionEvent::LootError { guid, error }, ActionIntent::LootTake { source, .. })
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
    mut cast_events: MessageWriter<crate::wrath_native::WrathCastEvent>,
    mut item_text_events: MessageWriter<crate::wrath_native::WrathItemText>,
    clock: Option<Res<crate::ui_script::UiClock>>,
    trace: Option<Res<crate::quest_trace::Source>>,
) {
    let ui_now_seconds = clock.as_ref().map(|clock| clock.ui_now);
    let (native, native_dropped) = control
        .feed
        .as_ref()
        .map(|feed| (feed.take_native(), feed.take_native_dropped()))
        .unwrap_or_default();
    let mut drained = 0usize;
    if native_dropped > 0 {
        if let Some(trace) = &trace {
            trace.observe(Some((view.session_epoch, view.world_generation)), None,
                crate::quest_trace::Stage::NativeEventsLost { count: native_dropped });
        }
    }
    for event in control.take_events() {
        if drained >= WRATH_DRAIN_LIMIT {
            break;
        }
        drained += 1;
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
            WrathFeedEvent::WorldTransfer { session_epoch, world_generation, map_id } => {
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
                    &mut *view,
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
                view.entered_world = false;
                view.vendor = None;
                view.last_inventory_failure = None;
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
            WrathFeedEvent::ExportSkipped(reason) => {
                debug!("wrath export skipped: {reason}");
            }
            WrathFeedEvent::SessionEnded { result, end: _, session_epoch, world_generation } => {
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
                    &mut *view,
                    crate::wrath_native::WrathScopeLoss::SessionEnded,
                    now_ms(),
                    ui_now_seconds,
                ) {
                    cast_events.write(event);
                }
                view.entities.clear();
                view.self_observation = None;
                view.player_guid = 0;
                view.entered_world = false;
                view.gossip_menu = None;
                view.vendor = None;
                view.server_clock = None;
                view.played_time = None;
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
                            Kind::Interrupted { reason } => CastEdge::Interrupted { reason: *reason },
                            Kind::ChannelStarted { .. } => CastEdge::ChannelStarted,
                            Kind::ChannelUpdated { .. } => CastEdge::ChannelUpdated,
                            Kind::ChannelEnded => CastEdge::ChannelEnded,
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
                    WrathNativeFeedEvent::InventoryFailure { failure, .. } =>
                        Some(Stage::InventoryFailure { reason: failure.reason }),
                    WrathNativeFeedEvent::ItemText(_) => None,
                };
                if let Some(stage) = stage {
                    trace.observe(Some((scope.session_epoch, scope.world_generation)), None, stage);
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
                                        cast.start_ui_now_seconds =
                                            existing.start_ui_now_seconds;
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
                                    channel.start_ui_now_seconds =
                                        existing.start_ui_now_seconds;
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
            &mut *view,
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
            &mut *view,
            crate::wrath_native::WrathScopeLoss::SessionEnded,
            now_ms(),
            ui_now_seconds,
        ) {
            cast_events.write(event);
        }
        view.entities.clear();
        view.self_observation = None;
        view.player_guid = 0;
        view.entered_world = false;
        view.gossip_menu = None;
        view.vendor = None;
        view.server_clock = None;
        view.played_time = None;
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

    let Some(digest) = control.take_digest() else { return };
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
            &mut *view,
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
                &mut *view,
                crate::wrath_native::WrathScopeLoss::WorldTransfer,
                now_ms(),
                ui_now_seconds,
            ) {
                cast_events.write(event);
            }
            view.entities.clear();
            view.gossip_menu = None;
            view.vendor = None;
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
        (0x70, 0x10), (0xf80, 0x4), (0x1000, 0x4000), (0x2000, 0x8),
        (0x4000, 0x20), (0x8000, 0x40), (0x10000, 0x80), (0x20000, 0x100),
        (0x40000, 0x200), (0x80000, 0x400), (0x100000, 0x800),
        (0x200000, 0x1000), (0x400000, 0x2000),
    ] {
        if flags & source != 0 { mapped |= destination; }
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

/// Map 12340's `PLAYER_QUEST_LOG_*` block: id, state, two count dwords (four 16-bit counts) and
/// the deadline, five dwords a slot for twenty-five slots.
fn wrath_quest_slots(raw: &crate::wrath_inventory::WrathFieldWindow) -> [QuestSlotView; QUEST_LOG_SLOTS] {
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
        (324, 486, 46), // equipment + four equipped bags
        (370, 532, 32), // backpack, 16 slots
        (402, 564, 48), // bank, first 24 of 28 slots
        (458, 612, 12), // bank bags, first six of seven slots
        (472, 624, 24), // vendor buyback, 12 slots
        (496, 648, 64), // keyring, 32 slots
        (624, 712, 2),  // farsight guid
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
        let src = usize::from(
            benilla_protocol_wrath::fields::PLAYER_VISIBLE_ITEM_1_ENTRYID + 2 * slot,
        );
        // An undelivered entry is skipped, not treated as the end: a create may omit a zero
        // slot while a later slot is present.
        let Some(entry) = raw.get(src as u16) else {
            continue;
        };
        let ench = raw.word_or_zero((src + 1) as u16);
        let dst = benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 12 * slot;
        pairs.push((dst + 2, entry));
        pairs.push((dst + 3, ench & 0xffff));
        pairs.push((dst + 4, ench >> 16));
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
    for src in 6..=21u16 {
        if let Some(value) = raw.get(src) {
            pairs.push((src, value));
        }
    }
    // The seven donor enchant slots: Wrath 22..42, unchanged.
    for i in 0..21u16 {
        if let Some(value) = raw.get(22 + i) {
            pairs.push((22 + i, value));
        }
    }
    // Property seed, random properties id, durability and max durability. Donor 45 stays unwritten:
    // 1.12 spells it `ITEM_FIELD_ITEM_TEXT_ID`, which 3.3.5 carries on the item-text query instead.
    for (src, dst) in [(58u16, 43u16), (59, 44), (60, 46), (61, 47)] {
        if let Some(value) = raw.get(src) {
            pairs.push((dst, value));
        }
    }
    if container {
        if let Some(value) = raw.get(64) {
            pairs.push((48, value));
        }
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

/// The 1.12 unit-aura descriptor window (`UNIT_FIELD_AURA`, fields 47-94, plus the six
/// `AURAFLAGS` nibble words at 95-100) from 12340's visible slots. Slots 48 and up have no 1.12
/// field and drop; the flags' low three bits are the effect indices the nibble carries.
fn push_aura_fields(
    auras: &[benilla_protocol_wrath::reducer::AuraSlot],
    pairs: &mut Vec<(u16, u32)>,
) {
    const SLOT_COUNT: usize = 48;
    let first = benilla_protocol::field::FIELD_UNIT_AURA;
    let mut ids = [0u32; SLOT_COUNT];
    let mut flags = [0u32; 6];
    for aura in auras {
        let slot = usize::from(aura.slot);
        if slot >= SLOT_COUNT {
            continue;
        }
        ids[slot] = aura.spell_id;
        flags[slot >> 3] |= u32::from(aura.flags & 0x7) << ((slot & 7) * 4);
    }
    for (index, id) in ids.iter().enumerate() {
        pairs.push((first + index as u16, *id));
    }
    for (index, word) in flags.iter().enumerate() {
        pairs.push((first + SLOT_COUNT as u16 + index as u16, *word));
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
    let old_fields = existing.and_then(|entity| world.get::<ObjectStore>(entity).map(|s| s.0.clone()));
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
                if let Some(mut items) = world.get_resource_mut::<crate::items::Items>() {
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
                world.entity_mut(entity).remove::<crate::wrath_item::WrathItemRow>();
            }
        }
    }
    (old_fields, fields, old_row, new_row)
}

/// Translate the drained digest into the established `GuidIndex`/`NetEntity`/`ObjectStore` path.
/// Exclusive, so creates/removals are live before `WorldStage::Input`.
fn wrath_apply_world(world: &mut World) {
    let (changes, entities, entered, phase, player_guid, map_id) = {
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
                retire_outbound_deliveries(
                    &mut queue,
                    crate::net::CommandRefusal::ScopeChanged,
                );
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
    }

    let mut item_creates: Vec<u64> = Vec::new();
    let mut row_diffs: Vec<(
        u64,
        Option<crate::wrath_item::WrathItemRow>,
        Option<crate::wrath_item::WrathItemRow>,
    )> = Vec::new();
    let mut field_diffs: Vec<(u64, ObjectType, Vec<(u16, u32, u32)>)> = Vec::new();
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
            EntityKind::GameObject => ObjectType::GameObject,
            EntityKind::DynamicObject => ObjectType::DynamicObject,
            EntityKind::Corpse => ObjectType::Corpse,
            _ => ObjectType::Unit,
        };
        let mut pairs = vec![(2u16, object_type_mask(object_type))];
        if let Some(entry) = remote.entry {
            pairs.push((3, entry));
        }
        if let Some(scale) = remote.scale {
            pairs.push((4, scale.to_bits()));
        }
        if let Some(health) = remote.health {
            pairs.push((22, health));
            pairs.push((28, remote.max_health.unwrap_or(health)));
        } else if let Some(max_health) = remote.max_health {
            pairs.push((28, max_health));
        }
        if let Some(level) = remote.level {
            pairs.push((34, level));
        }
        if let Some(faction) = remote.faction {
            pairs.push((35, faction));
        }
        if let Some(flags) = remote.unit_flags {
            pairs.push((46, flags));
        }
        if let Some(mount) = remote.mount_display_id {
            pairs.push((133, mount));
        }
        // 12340 moves `ShapeshiftForm` to `UNIT_FIELD_BYTES_2[3]` (`Unit_C.cpp:6652`) and shifts
        // `VisFlags` to `UNIT_FIELD_BYTES_1[2]` (`Unit_C.cpp:5128`). Synthesize the 1.12
        // `FIELD_UNIT_BYTES_1` (`138`: `[stand_state, pet_talents, shapeshift_form, vis_flags]`)
        // and project `UNIT_FIELD_BYTES_2` onto `FIELD_UNIT_BYTES_2` (`164`: `[sheath_state, ...]`).
        if remote.bytes_1.is_some() || remote.bytes_2.is_some() {
            let b1 = remote.bytes_1.unwrap_or(0);
            let b2 = remote.bytes_2.unwrap_or(0);
            let vanilla_bytes_1 =
                (b1 & 0x0000_ffff) | ((b2 >> 24) << 16) | (((b1 >> 16) & 0xff) << 24);
            pairs.push((138, vanilla_bytes_1));
        }
        if let Some(bytes_2) = remote.bytes_2 {
            pairs.push((164, bytes_2));
        }
        if let Some(flags) = remote.npc_flags {
            pairs.push((147, vanilla_npc_flags(flags)));
        }
        if let Some(flags) = remote.dynamic_flags {
            pairs.push((143, flags));
        }
        if let Some(display) = remote.display_id.filter(|&display| display != 0) {
            pairs.push((131, display));
            pairs.push((132, display));
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
            pairs.push((36, bytes));
        }
        if let Some(target) = remote.target {
            pairs.push((16, target as u32));
            pairs.push((17, (target >> 32) as u32));
        }
        // Project 12340 ownership guids onto the 1.12 descriptor slots the reaction, UI and minimap
        // consumers read (`CHARMEDBY` 10/11, `SUMMONEDBY` 12/13, `CREATEDBY` 14/15). Without these
        // a hunter pet streams in with no owner, so it reads as neutral/attackable even though the
        // server refuses the swing.
        if let Some(charmed_by) = remote.charmed_by {
            pairs.push((10, charmed_by as u32));
            pairs.push((11, (charmed_by >> 32) as u32));
        }
        if let Some(summoned_by) = remote.summoned_by {
            pairs.push((12, summoned_by as u32));
            pairs.push((13, (summoned_by >> 32) as u32));
        }
        if let Some(created_by) = remote.created_by {
            pairs.push((14, created_by as u32));
            pairs.push((15, (created_by >> 32) as u32));
        }
        let mut wrath_slots: Option<WrathQuestSlots> = None;
        if kind == EntityKind::Player {
            if let Some(bytes) = remote.player_bytes {
                pairs.push((193, bytes));
            }
            if let Some(bytes) = remote.player_bytes_2 {
                pairs.push((194, bytes));
            }
            if let Some(raw) = remote.player_fields.as_ref() {
                // 3.3.5 uses 148 as UNIT_END; 1.12 uses a different player layout. Map the
                // durable fields the stock quest/loot/player frames read. The descriptor keeps
                // the 1.12 twenty-slot projection; the full twenty-five go beside it.
                let slots = wrath_quest_slots(raw);
                for slot in 0..20usize {
                    let s = slots[slot];
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
                let max_level = raw.word_or_zero(benilla_protocol_wrath::fields::PLAYER_FIELD_MAX_LEVEL);
                let daily_start = benilla_protocol_wrath::fields::PLAYER_FIELD_DAILY_QUESTS_1;
                let daily = (0..25u16)
                    .filter(|i| raw.word_or_zero(daily_start + i) != 0)
                    .count() as u32;
                wrath_slots = Some(WrathQuestSlots(slots, max_level, daily));
                for (src, dst) in [(25u16, 23u16), (26, 24), (27, 25), (33, 29), (34, 30), (35, 31)] {
                    if let Some(value) = raw.get(src) {
                        pairs.push((dst, value));
                    }
                }
                // `PLAYER_FLAGS` (3.3.5 150 -> 1.12 190): the ghost bit 0x10 is what the death
                // owner reads off the self descriptor.
                if let Some(value) = raw.get(150) {
                    pairs.push((benilla_protocol::field::FIELD_PLAYER_FLAGS, value));
                }
                push_player_inventory_fields(raw, &mut pairs);
                push_visible_item_fields(raw, &mut pairs);
                if let Some(value) = raw.get(634) {
                    pairs.push((716, value));
                }
                if let Some(value) = raw.get(635) {
                    pairs.push((717, value));
                }
                if let Some(value) = raw.get(benilla_protocol_wrath::fields::PLAYER_REST_STATE_EXPERIENCE) {
                    pairs.push((1175, value));
                }
                if let Some(value) = raw.get(1170) {
                    pairs.push((1176, value));
                }
            }
        }
        if matches!(kind, EntityKind::Unit | EntityKind::Player) && !remote.auras.is_empty() {
            push_aura_fields(&remote.auras, &mut pairs);
        }
        let fields = ObjectFields::from_pairs(&pairs).into_created(object_type);
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
                let old = world.get::<ObjectStore>(entity).map(|store| store.0.clone());
                if let Some(old_fields) = &old {
                    let moved = diff_fields(old_fields, &fields);
                    if !moved.is_empty() {
                        field_diffs.push((*guid, object_type, moved));
                    }
                }
                let old_health = old.as_ref().and_then(benilla_protocol::ObjectFields::unit_health);
                let old_max = old
                    .as_ref()
                    .and_then(benilla_protocol::ObjectFields::unit_max_health);
                let new_span = matches!(kind, EntityKind::Unit | EntityKind::Player)
                    .then(|| aura_span(&fields));
                // The player descriptor is replaced whole, so the 1.12 watches get the quest-log
                // signals by hand when any of the twenty-five slots moved.
                let old_slots = world.get::<WrathQuestSlots>(entity).copied();
                let quest_slots_changed =
                    wrath_slots.is_some_and(|slots| old_slots != Some(slots));
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
                        display_id: remote.display_id,
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
                if quest_slots_changed && *guid == player_guid {
                    // The same two signals the 1.12 values block raises: the rebuild notice and
                    // the event the stock log frame listens for.
                    let mut notices =
                        world.resource_mut::<crate::field_watch::NoticeLane>();
                    notices.write(crate::field_watch::FieldNotice::QuestLog);
                    notices.write(crate::field_watch::FieldNotice::Unit {
                        guid: *guid,
                        id: crate::field_watch::id::UNIT_QUEST_LOG_CHANGED,
                    });
                }
                if let Some(new_span) = new_span {
                    // The 12340 aura packets changed this unit's descriptor window; hand the
                    // 1.12 aura watcher the same edges and one call the stock block path makes.
                    let old_span = old.as_ref().map_or([0u32; crate::net::AURA_WATCH_SPAN], aura_span);
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
                let old_bytes_1 = old.as_ref().and_then(benilla_protocol::ObjectFields::unit_bytes_1);
                let new_bytes_1 = world
                    .get::<ObjectStore>(entity)
                    .and_then(|s| s.0.unit_bytes_1());
                let old_bytes_2 = old.as_ref().and_then(benilla_protocol::ObjectFields::unit_bytes_2);
                let new_bytes_2 = world
                    .get::<ObjectStore>(entity)
                    .and_then(|s| s.0.unit_bytes_2());
                let old_unit_flags = old.as_ref().map(benilla_protocol::ObjectFields::unit_flags);
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
                if kind == EntityKind::Player {
                    world.entity_mut(entity).insert(Equipment {
                        settled: true,
                        ..default()
                    });
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
                            let applied = if let Some(mut player) = world
                                .get_resource_mut::<crate::player::Player>()
                            {
                                player.apply_server_pose(bevy, pose.o);
                                true
                            } else {
                                false
                            };
                            if applied {
                                if let Some(trace) = world.get_resource::<crate::quest_trace::Source>() {
                                    let scope = world.get_resource::<WrathWorldView>()
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
                                    let (movement_seq, ambiguous) = trace.movement_written_seq_unambiguous();
                                    trace.clear_movement_writes_since_correction();
                                    trace.observe(scope, None, crate::quest_trace::Stage::MovementCorrectionApplied {
                                        distance_bucket,
                                        movement_seq,
                                        ambiguous,
                                    });
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
                        display_id: remote.display_id,
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
                if *guid == player_guid {
                    entity.insert((
                        SelfPlayer,
                        Equipment {
                            settled: true,
                            ..default()
                        },
                    ));
                }
                index.0.insert(*guid, entity.id());
            }
        }
    }

    world.resource_mut::<SelfGuid>().0 = Some(player_guid);
    *world.resource_mut::<GuidIndex>() = index;

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
                if let Some(mut bag_cache) =
                    world.get_resource_mut::<crate::item_watch::BagCache>()
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
                let inventory =
                    world.get_resource::<crate::wrath_inventory::WrathInventory>();
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
                        let seat =
                            |n: u8| seats[usize::from(n.saturating_sub(1)).min(10)];
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
                        FIELD_UNIT_DYNAMIC_FLAGS, FIELD_UNIT_HEALTH, FIELD_UNIT_MAXHEALTH,
                    };
                    // The snapshot adapter bypasses the normal values-block watches. Feed
                    // the UI explicitly; FieldChanged alone only reaches ECS consumers.
                    let event = match index {
                        FIELD_UNIT_HEALTH => Some(crate::field_watch::id::UNIT_HEALTH),
                        FIELD_UNIT_MAXHEALTH => Some(crate::field_watch::id::UNIT_MAXHEALTH),
                        _ => None,
                    };
                    if let Some(id) = event {
                        if let Some(mut notices) =
                            world.get_resource_mut::<crate::field_watch::NoticeLane>()
                        {
                            notices.write(crate::field_watch::FieldNotice::Signal {
                                guid: *guid,
                                id,
                            });
                        }
                    }
                    // Loot FX rescan on this edge, including when LOOTABLE is cleared.
                    if index == FIELD_UNIT_DYNAMIC_FLAGS {
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
            if *guid == player_guid && *kind == ObjectType::Player {
                // Snapshot replacement bypasses the normal field watchers; notify after seating
                // the values so the XP bar reads the updated XP and rested pool.
                let xp_changed = moved.iter().any(|&(index, _, _)| index == 716 || index == 717);
                let rest_changed = moved.iter().any(|&(index, _, _)| index == 1175);
                if let Some(mut notices) = world.get_resource_mut::<crate::field_watch::NoticeLane>() {
                    if xp_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 189 });
                    }
                    if rest_changed {
                        notices.write(crate::field_watch::FieldNotice::Player { id: 406 });
                    }
                }
                let inv_changed = moved.iter().any(|&(idx, _, _)| {
                    (benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR
                        ..benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 228)
                        .contains(&idx)
                        || (benilla_protocol::field::FIELD_PLAYER_INV_SLOT_HEAD
                            ..benilla_protocol::field::FIELD_PLAYER_INV_SLOT_HEAD + 226)
                            .contains(&idx)
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
                        ((base..base + 38).contains(&idx) && (idx - base) % 2 == 0)
                            .then(|| ((idx - base) / 2) as u8)
                            .filter(|slot| *slot < 19)
                    })
                    .collect();
                changed_slots.sort_unstable();
                changed_slots.dedup();
                if !changed_slots.is_empty() {
                    if let Some(trace) =
                        world.get_resource::<crate::quest_trace::Source>()
                    {
                        let scope = world
                            .get_resource::<WrathWorldView>()
                            .map(|v| (v.session_epoch, v.world_generation));
                        let mut entries: Vec<(u8, u64)> = Vec::new();
                        if let Some(entity) =
                            world.resource::<GuidIndex>().0.get(guid).copied()
                        {
                            if let Some(store) = world.get::<crate::net::ObjectStore>(entity)
                            {
                                for slot in &changed_slots {
                                    entries.push((
                                        *slot,
                                        store.0.player_inv_slot(*slot).unwrap_or(0),
                                    ));
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

#[derive(Default)]
struct WrathMoveLocal {
    started: Option<std::time::Instant>,
    flags: Option<u32>,
    pos: Option<[f32; 3]>,
    facing: Option<f32>,
    last_send: Option<std::time::Instant>,
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
/// 12340 `MOVEMENTFLAG_SWIMMING`, the same numeric bit as 1.12's.
const W_FLAG_SWIMMING: u32 = 0x20_0000;

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

/// Stream the shared `Player` controller's pose onto the Wrath session. The existing action/input
/// path remains the source of motion; this adapter only chooses the Wrath opcode for the edge and
/// writes the 3.3.5 `MovementInfo` fields the server reads.
fn wrath_stream_movement(
    player: Option<Res<crate::player::Player>>,
    self_guid: Option<Res<SelfGuid>>,
    input: Option<Res<WrathInput>>,
    mut local: Local<WrathMoveLocal>,
    trace: Option<Res<crate::quest_trace::Source>>,
) {
    let Some(player) = player else {
        local.flags = None;
        return;
    };
    let Some(input) = input else { return };
    let Some(guid) = self_guid.as_ref().and_then(|g| g.0) else {
        local.flags = None;
        return;
    };
    if !player.active || player.detached || player.control_lost || player.settling {
        return;
    }
    let flags = wrath_move_flags(player.move_flags());
    if flags & W_FLAG_ONTRANSPORT != 0 {
        // Transport-local movement needs the transport block; the first route is on foot.
        return;
    }
    let pos = bevy_to_wow(player.pos);
    let facing = player.facing().rem_euclid(std::f32::consts::TAU);
    let now = std::time::Instant::now();
    let started = *local.started.get_or_insert(now);
    let prev_flags = local.flags.replace(flags);
    let changed = prev_flags != Some(flags);
    let moved = local.pos.replace(pos) != Some(pos);
    let turned = local.facing.replace(facing) != Some(facing);
    // The pose does not stream per frame: the reference's mover sends a `MSG_MOVE_*` per axis
    // transition, a `SET_FACING` for a standing turn, and a 500 ms heartbeat while moving (the
    // 1.12 `stream_self_movement` cadence). A per-frame pose packet floods the server with
    // heartbeats, whose rate check then rejects the stream.
    let in_motion = flags
        & (W_FLAG_FORWARD
            | W_FLAG_BACKWARD
            | W_FLAG_STRAFE_LEFT
            | W_FLAG_STRAFE_RIGHT
            | W_FLAG_TURN_LEFT
            | W_FLAG_TURN_RIGHT
            | W_FLAG_FALLING)
        != 0;
    let heartbeat_due = local
        .last_send
        .is_none_or(|at| now.duration_since(at) >= Duration::from_millis(500));
    if !changed && !(in_motion && heartbeat_due) && !(!in_motion && (moved || turned)) {
        return;
    }
    let fall_edge = |was: Option<u32>, now: u32| {
        let was = was.unwrap_or(0) & W_FLAG_FALLING != 0;
        let is = now & W_FLAG_FALLING != 0;
        (was, is)
    };
    let (was_falling, is_falling) = fall_edge(prev_flags, flags);
    let opcode = if !was_falling && is_falling {
        Some(benilla_protocol_wrath::opcode::MSG_MOVE_JUMP)
    } else if was_falling && !is_falling {
        Some(benilla_protocol_wrath::opcode::MSG_MOVE_FALL_LAND)
    } else {
        let delta = prev_flags.unwrap_or(0) ^ flags;
        Some(if delta & (W_FLAG_STRAFE_LEFT | W_FLAG_STRAFE_RIGHT) != 0 {
            if flags & W_FLAG_STRAFE_LEFT != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_STRAFE_LEFT
            } else if flags & W_FLAG_STRAFE_RIGHT != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_STRAFE_RIGHT
            } else {
                benilla_protocol_wrath::opcode::MSG_MOVE_STOP_STRAFE
            }
        } else if delta & (W_FLAG_TURN_LEFT | W_FLAG_TURN_RIGHT) != 0 {
            if flags & W_FLAG_TURN_LEFT != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_TURN_LEFT
            } else if flags & W_FLAG_TURN_RIGHT != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_TURN_RIGHT
            } else {
                benilla_protocol_wrath::opcode::MSG_MOVE_STOP_TURN
            }
        } else if delta & (W_FLAG_FORWARD | W_FLAG_BACKWARD) != 0 {
            if flags & W_FLAG_FORWARD != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_FORWARD
            } else if flags & W_FLAG_BACKWARD != 0 {
                benilla_protocol_wrath::opcode::MSG_MOVE_START_BACKWARD
            } else {
                benilla_protocol_wrath::opcode::MSG_MOVE_STOP
            }
        } else if turned && !in_motion {
            // A standing turn rides `SET_FACING`, as the reference's broadcaster sends it.
            benilla_protocol_wrath::opcode::MSG_MOVE_SET_FACING
        } else {
            benilla_protocol_wrath::opcode::MSG_MOVE_HEARTBEAT
        })
    };
    let Some(opcode) = opcode else { return };
    let mut payload = enc_pack_guid(guid);
    payload.extend_from_slice(&flags.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes()); // movement flags 2
    let time = now.duration_since(started).as_millis() as u32;
    payload.extend_from_slice(&time.to_le_bytes());
    payload.extend_from_slice(&pos[0].to_le_bytes());
    payload.extend_from_slice(&pos[1].to_le_bytes());
    payload.extend_from_slice(&pos[2].to_le_bytes());
    payload.extend_from_slice(&facing.to_le_bytes());
    if flags & W_FLAG_SWIMMING != 0 {
        payload.extend_from_slice(&0f32.to_le_bytes()); // pitch
    }
    payload.extend_from_slice(&0u32.to_le_bytes()); // fall time
    if flags & W_FLAG_FALLING != 0 {
        payload.extend_from_slice(&[0u8; 16]); // zspeed, sin, cos, xyspeed
    }
    if let Ok(mut queue) = input.0.lock() {
        if queue.len() >= WRATH_MOVEMENT_QUEUE_CAP {
            // Evict only replaceable motion. A queue full of real actions refuses this pose
            // instead of letting movement drop a cast, quest, loot or logout command.
            let Some(oldest_motion) = queue.iter().position(|entry| entry.movement) else {
                if let Some(trace) = &trace {
                    trace.note_movement_refused();
                }
                return;
            };
            let _ = queue.remove(oldest_motion);
            if let Some(trace) = &trace {
                trace.note_movement_coalesced();
            }
        }
        if let Some(trace) = &trace {
            trace.note_movement_admitted();
        }
        let movement_seq = trace.as_ref().map(|t| t.mint_movement_sequence());
        queue.push_back(WrathOutbound {
            opcode,
            payload,
            movement: true,
            movement_seq,
            enqueued_at: Some(std::time::Instant::now()),
            receipt: None,
            trace: None,
            enforce_scope: false,
        });
    } else if let Some(trace) = &trace {
        trace.note_movement_refused();
    }
    local.last_send = Some(now);
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
        Some(name) => records.iter().find(|record| record.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("WOW_CHAR requested character {name:?}, absent from the account roster")),
        None => records.first().context("no character on the account roster"),
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

    let (parked, roster_asked) = match parked {
        Some(input) => (Some(input.commands), input.roster_asked),
        None => (None, false),
    };

    let send_glue = |event| {
        push_forced_event(&events_out, event, config.login_generation);
    };
    send_glue(SessionEvent::LoginStage { stage: benilla_protocol::LoginStage::Connecting });
    let mut logon = benilla_protocol_wrath::logon::logon(&config.auth_addr, &config.account, &config.password)?;
    if cancel.load(Ordering::SeqCst) { return Ok(()); }
    send_glue(SessionEvent::LoginStage { stage: benilla_protocol::LoginStage::Handshaking });
    let realm_view = |r: &benilla_protocol_wrath::auth::RealmInfo| benilla_protocol::RealmInfo {
        name: r.name.clone(), address: r.address.clone(), population: r.population,
        characters: r.characters, realm_type: u32::from(r.realm_type), flags: r.flags,
        category: r.category, id: r.realm_id,
    };
    let publish_realms = |logon: &benilla_protocol_wrath::logon::Logon, failed| {
        send_glue(SessionEvent::RealmList {
            realms: logon.realms.iter().map(realm_view).collect(), failed,
        });
    };
    let realm_id = if config.login_generation.is_some() {
        publish_realms(&logon, false);
        if let Some(id) = config.preferred_realm { id } else {
            let input = parked.as_ref().ok_or_else(|| anyhow::anyhow!("login needs a parked session"))?;
            loop {
                if cancel.load(Ordering::SeqCst) { return Ok(()); }
                match input.recv_timeout(Duration::from_millis(100)) {
                    Ok(WrathParkedCommand::Realm(r)) if logon.realms.iter()
                        .any(|v| v.realm_id == r.id && v.flags & 3 == 0) => break r.id,
                    Ok(WrathParkedCommand::RefreshRealms) => {
                        let failed = logon.refresh_realms().is_err();
                        publish_realms(&logon, failed);
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(()),
                    _ => {}
                }
            }
        }
    } else {
        logon.realms.first().ok_or_else(|| anyhow::anyhow!("realm list is empty"))?.realm_id
    };
    let realm = logon.realms.iter().find(|r| r.realm_id == realm_id)
        .ok_or_else(|| anyhow::anyhow!("selected realm is no longer available"))?.clone();

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
    session.start_session(1); // placeholder; re-started with the real GUID after the enum
    // The single native cast/channel authority for this session; it resolves each wire fact
    // exactly once and feeds both presentation mirrors and the WM export.
    let mut native = crate::wrath_native::NativeCastState::default();
    feed.push_event(WrathFeedEvent::Phase {
        phase: session.phase(),
        session_epoch: session.epoch(),
        world_generation: session.world_generation(),
        login_generation: config.login_generation,
        map_id: session.map_id(),
    });
    let started = std::time::Instant::now();

    let (mut world, seed) =
        benilla_protocol_wrath::world::WorldConnection::connect(
            if config.world_addr.is_empty() { &realm.address } else { &config.world_addr }
        )?;
    world.stream_mut().set_read_timeout(Some(Duration::from_millis(200)))?;
    if config.login_generation.is_some() {
        send_glue(SessionEvent::LoginStage { stage: benilla_protocol::LoginStage::WorldConnected });
    }
    world.auth_session(&logon.account, &logon.session_key, &seed, realm.realm_id as u32)?;
    // auth_session sends the proof; only the server's AUTH_OK accepts the world link.
    let mut auth_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if cancel.load(Ordering::SeqCst) { return Ok(()); }
        anyhow::ensure!(Instant::now() < auth_deadline, "timeout waiting for world authentication");
        match world.recv() {
            Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_AUTH_RESPONSE => {
                let code = *p.payload.first().ok_or_else(|| anyhow::anyhow!("empty auth response"))?;
                if let Some(expansion) = p.payload.get(10) {
                    // AuthHandler.cpp: billing fields precede the account expansion byte.
                    feed.account_expansion.store(*expansion, Ordering::SeqCst);
                }
                if code == benilla_protocol_wrath::opcode::AUTH_OK { break; }
                if code == 0x1b {
                    // AuthHandler.cpp: initial queue includes billing; later updates are six bytes.
                    let offset = if p.payload.len() >= 16 { 11 } else { 1 };
                    let bytes = p.payload.get(offset..offset + 4)
                        .ok_or_else(|| anyhow::anyhow!("truncated queue response"))?;
                    send_glue(SessionEvent::LoginQueued {
                        position: Some(u32::from_le_bytes(bytes.try_into()?)), realm: Some(realm.name.clone()),
                    });
                    auth_deadline = Instant::now() + Duration::from_secs(60);
                } else {
                    anyhow::bail!("world server refused authentication ({code:#04x})");
                }
            }
            Ok(_) => {}
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }
    feed.authenticated.store(true, Ordering::SeqCst);
    if let Some(generation) = config.login_generation {
        send_glue(SessionEvent::WorldAuthenticated {
            realm: realm_view(&realm), epoch: session.epoch(), generation,
        });
    }
    session.note_player_login();

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
            Some(benilla_wm_native::SourceHealth::new(
                self.session.epoch(),
                self.session.world_generation(),
                self.session.player_guid().unwrap_or(0),
                state,
                self.session.last_server_packet_at_unix_ms(),
            ).ok()?)
        }
        fn export_due(&self) -> bool {
            self.session.export_due()
        }
        fn export(
            &mut self,
            exported_at_unix_ms: i64,
            client_uptime_ms: u64,
        ) -> std::result::Result<Option<SnapshotOwned>, benilla_wm_native::ProducerExportError> {
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
        fn resources(
            &self,
        ) -> benilla_wm_native::Result<benilla_wm_native::ResourceObservation> {
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

    // Character selection: reuse the renderless driver's create-first flow is intentionally
    // out of scope here â€” the graphical path logs into an existing character (issue 23's
    // driver owns creation). Enumerate before resolving a named boot or parked-screen pick.
    world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
    let mut records: Option<Vec<gameplay::CharEnumRecord>> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while records.is_none() {
        if cancel.load(Ordering::SeqCst) {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timeout waiting for SMSG_CHAR_ENUM");
        }
        match world.recv() {
            Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_ENUM => {
                records = Some(benilla_protocol_wrath::gameplay::parse_char_enum(&p.payload)?);
            }
            Ok(_) => {}
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
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
                            generation: config.login_generation.unwrap_or(0),
                        },
                    )
                    .for_login(config.login_generation),
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
            if roster_asked {
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
            anyhow::ensure!(!mutation_deadline.is_some_and(|d| Instant::now() >= d),
                "timeout waiting for character action response");
            for request in parked.try_iter() {
                match request {
                    WrathParkedCommand::RealmSplit(state) => {
                        world.send(benilla_protocol_wrath::opcode::CMSG_REALM_SPLIT,
                            &state.to_le_bytes())?;
                        debug!("wrath: sent realm split choice={state}");
                    }
                    WrathParkedCommand::Enumerate => {
                        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
                    }
                    WrathParkedCommand::Enter(picked) => {
                        if roster_mutation.is_none() && records.iter().any(|record| record.guid == picked) {
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
                        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_CREATE,
                            &benilla_protocol::messages::char_create(&request))?;
                        roster_mutation = Some(benilla_protocol::CharAction::Create);
                        mutation_deadline = Some(Instant::now() + Duration::from_secs(30));
                    }
                    WrathParkedCommand::Delete(guid) if roster_mutation.is_none()
                        && records.iter().any(|record| record.guid == guid) => {
                        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_DELETE, &guid.to_le_bytes())?;
                        roster_mutation = Some(benilla_protocol::CharAction::Delete);
                        mutation_deadline = Some(Instant::now() + Duration::from_secs(30));
                    }
                    WrathParkedCommand::RefreshRealms => {
                        let failed = logon.refresh_realms().is_err();
                        publish_realms(&logon, failed);
                    }
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
            match world.recv() {
                Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_REALM_SPLIT => {
                    let reply = benilla_protocol_wrath::gameplay::parse_realm_split(&p.payload)?;
                    info!("wrath: realm split choice={} state={} date={}", reply.0, reply.1, reply.2);
                    send_glue(SessionEvent::RealmSplit { choice: reply.0, state: reply.1, date: reply.2 });
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
                Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_CREATE
                    || p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_DELETE => {
                    let action = if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_CREATE {
                        benilla_protocol::CharAction::Create
                    } else { benilla_protocol::CharAction::Delete };
                    if roster_mutation != Some(action) { continue; }
                    let &[raw] = p.payload.as_slice() else {
                        anyhow::bail!("invalid character action reply length");
                    };
                    roster_mutation = None;
                    mutation_deadline = None;
                    let code = roster_status(action, raw);
                    send_glue(SessionEvent::CharActionResult { action, code });
                    // The roster changes only after the server's response and fresh enum.
                    if (action == benilla_protocol::CharAction::Create && raw == 0x2f)
                        || (action == benilla_protocol::CharAction::Delete && raw == 0x47) {
                        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
                    }
                }
                Ok(_) => {}
                Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
                Err(e) => return Err(e),
            }
        }
        entered.expect("picked guid")
    } else {
        let record = boot_character(&records, config.character_name.as_deref())?;
        info!("wrath: boot selection name={} guid={:#x} race={} class={} map={}",
            record.name, record.guid, record.race, record.class, record.map);
        record.guid
    };
    session.start_session(guid); // re-begin with the real full GUID; phase resets to selection
    session.note_player_login();
    feed.last_session_epoch.store(session.epoch(), Ordering::SeqCst);
    feed.last_world_generation.store(session.world_generation(), Ordering::SeqCst);
    feed.push_event(WrathFeedEvent::Phase {
        phase: session.phase(),
        session_epoch: session.epoch(),
        world_generation: session.world_generation(),
        login_generation: config.login_generation,
        map_id: session.map_id(),
    });
    world.send(benilla_protocol_wrath::opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;

    let deadline = config
        .dwell
        .map(|dwell| std::time::Instant::now() + dwell);
    let mut last_ping = std::time::Instant::now();
    let mut last_server_response = std::time::Instant::now();
    let mut ping_seq = 1u32;
    let mut last_digest_revision = session.state_revision();
    let mut logged_out = false;
    feed.publish_digest(build_digest(&session)); // initial state (character selection)
    while deadline.is_none_or(|deadline| std::time::Instant::now() < deadline) {
        feed.last_session_epoch.store(session.epoch(), Ordering::SeqCst);
        feed.last_world_generation.store(session.world_generation(), Ordering::SeqCst);
        if cancel.load(Ordering::SeqCst) {
            native.invalidate_scope();
            publisher.retire_all();
            return Ok(());
        }
        // Bounded ownership (R04): once the reply queue overflowed, ordered replies have already
        // been lost. End the session through the one terminal path instead of continuing to read
        // and silently discarding gameplay state.
        if let Some(replies) = events_out.as_ref() {
            if replies.lock().map(|replies| replies.overloaded()).unwrap_or(false) {
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
                    if receipt.enforce_scope && (receipt.session_epoch != session.epoch()
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
                                    let cmd_ix = command.trace.as_ref().map(|t| t.interaction_id()).unwrap_or(0);
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
                        trace.observe(Some((session.epoch(), session.world_generation())), None,
                            crate::quest_trace::Stage::Outbound {
                                opcode: command.opcode,
                                state: if result.is_ok() {
                                    crate::quest_trace::OutboundState::Written
                                } else {
                                    crate::quest_trace::OutboundState::WriteFailed
                                },
                            });
                    }
                }
                if result.is_err() {
                    if let Some(ix) = &command.trace {
                        ix.record(crate::quest_trace::Stage::SocketWriteFailed);
                    } else if let Some(trace) = &feed.trace {
                        trace.observe(Some((session.epoch(), session.world_generation())), None,
                            crate::quest_trace::Stage::SocketWriteFailed);
                    }
                }
                // Delivery outcome records on trace-bearing commands (defect #2 fix), unless the
                // same interaction is already reported through the receipt path.
                let receipt_ix = command.receipt.as_ref().and_then(|r| r.trace.as_ref()).map(|t| t.interaction_id()).unwrap_or(0);
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
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CAST => {
                        command.payload.get(1..5)
                    }
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
            Ok(p) => {
                let received_at = Instant::now();
                last_server_response = received_at;
                let eligible = diagnostic_opcode(p.opcode);
                let traced = eligible
                    && match &feed.trace {
                        Some(trace) if p.opcode == benilla_protocol_wrath::opcode::SMSG_UPDATE_OBJECT =>
                            trace.sample_update_packet(),
                        Some(_) => true,
                        None => false,
                    };
                let scope = Some((session.epoch(), session.world_generation()));
                if traced {
                    if let Some(trace) = &feed.trace {
                        trace.observe(scope, None, crate::quest_trace::Stage::PacketReceived { opcode: p.opcode });
                    }
                }
                let reduced = session.reduce(p.opcode, &p.payload, now_ms());
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
                            crate::quest_trace::Stage::ReducerRejected { opcode: p.opcode, reason }
                        };
                        trace.observe(scope, None, stage);
                    }
                }
                reduced?;
                if let Some(parsed) = gameplay::parse(p.opcode, &p.payload) {
                    if traced {
                        if let Some(trace) = &feed.trace {
                            let stage = match &parsed {
                                Ok(_) => crate::quest_trace::Stage::GameplayDecoded {
                                    opcode: p.opcode,
                                },
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
                                for event in translate_gameplay(
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
                                for event in translate_gameplay(
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
                                for event in translate_gameplay(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
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
                                for event in translate_gameplay(
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
                        Ok(event) => {
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
                                for event in translate_gameplay(
                                    event,
                                    session.player_guid().unwrap_or(0),
                                    received_at,
                                ) {
                                    push_session_event(&events_out, &session, event);
                                }
                            }
                        }
                        Err(e) => {
                            debug!("wrath: ignored malformed 0x{:X} gameplay reply: {e}", p.opcode);
                        }
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
                    // time sync: answer to keep the session healthy
                    if p.payload.len() >= 4 {
                        let mut resp = p.payload[0..4].to_vec();
                        resp.extend_from_slice(&0u32.to_le_bytes());
                        world.send(0x391, &resp)?;
                    }
                }
            }
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
            Err(e) => {
                if let Some(trace) = &feed.trace {
                    trace.observe(Some((session.epoch(), session.world_generation())), None,
                        crate::quest_trace::Stage::SocketReadFailed);
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
            world.send(benilla_protocol_wrath::opcode::MSG_MOVE_TELEPORT_ACK as u32, &ack)?;
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
            pump_once(publisher, &mut adapter, now_ms(), started.elapsed().as_millis() as u64)
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
        if b != 0 { mask |= 1 << i; bytes.push(b); }
    }
    let mut out = vec![mask];
    out.extend_from_slice(&bytes);
    out
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
        let mut powers = [None; 7];
        let mut max_powers = [None; 7];
        for i in 0..7u16 {
            powers[usize::from(i)] = observed(fields::UNIT_FIELD_POWER1 + i);
            max_powers[usize::from(i)] = observed(fields::UNIT_FIELD_MAXPOWER1 + i);
        }
        entities.insert(
            *guid,
            WrathEntity {
                guid: *guid,
                type_id: entry.type_id,
                entry: observed(fields::OBJECT_FIELD_ENTRY),
                display_id: observed(fields::UNIT_FIELD_DISPLAYID),
                scale: observed(fields::OBJECT_FIELD_SCALE_X).map(f32::from_bits),
                health: observed(fields::UNIT_FIELD_HEALTH),
                max_health: observed(fields::UNIT_FIELD_MAXHEALTH),
                powers,
                max_powers,
                level: observed(fields::UNIT_FIELD_LEVEL),
                bytes_0: observed(fields::UNIT_FIELD_BYTES_0),
                faction: observed(fields::UNIT_FIELD_FACTIONTEMPLATE),
                unit_flags: observed(fields::UNIT_FIELD_FLAGS),
                mount_display_id: observed(fields::UNIT_FIELD_MOUNTDISPLAYID),
                bytes_1: observed(fields::UNIT_FIELD_BYTES_1),
                bytes_2: observed(fields::UNIT_FIELD_BYTES_2),
                npc_flags: observed(fields::UNIT_NPC_FLAGS),
                dynamic_flags: observed(fields::UNIT_DYNAMIC_FLAGS),
                target: entry.self_guid(fields::UNIT_FIELD_TARGET).filter(|&guid| guid != 0),
                charmed_by: entry.self_guid(fields::UNIT_FIELD_CHARMEDBY).filter(|&guid| guid != 0),
                summoned_by: entry.self_guid(fields::UNIT_FIELD_SUMMONEDBY).filter(|&guid| guid != 0),
                created_by: entry.self_guid(fields::UNIT_FIELD_CREATEDBY).filter(|&guid| guid != 0),
                player_bytes: observed(fields::PLAYER_BYTES),
                player_bytes_2: observed(fields::PLAYER_BYTES_2),
                position: entry.movement.as_ref().and_then(|m| m.position),
                move_flags: entry.movement.as_ref().map(|m| m.move_flags),
                create_spline: entry
                    .movement
                    .as_ref()
                    .and_then(|m| m.create_spline.clone()),
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
                    let container = entry.type_id
                        == benilla_protocol_wrath::updates::type_id::CONTAINER;
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
        let pose = PositionSnapshot { x: 1.0, y: 2.0, z: 3.0, o: 0.5 };
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
            matches!(events.get(1), Some(SessionEvent::SpellGo { caster: 0x1522, .. })),
            "and the GO itself still follows: {events:?}"
        );
        assert_eq!(events.len(), 2);

        let remote = translate_gameplay(go(0x99), 0x1522, Instant::now());
        assert_eq!(remote.len(), 1);
        assert!(matches!(remote[0], SessionEvent::SpellGo { caster: 0x99, .. }));
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
        let at = |slot: u16| {
            benilla_protocol::field::FIELD_PLAYER_VISIBLE_ITEM_1_CREATOR + 12 * slot
        };
        assert!(pairs.contains(&(at(0) + 2, 4368)));
        assert!(pairs.contains(&(at(0) + 3, 1)));
        assert!(pairs.contains(&(at(0) + 4, 2)));
        assert!(pairs.contains(&(at(18) + 2, 5976)));
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
            !pairs.iter().any(|&(_, value)| value == 0x8888 || value == 0x9999),
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
        let window = item_window(
            &[(fields::ITEM_FIELD_DURABILITY, 0)],
            fields::ITEM_END,
        );
        let mut pairs = Vec::new();
        push_item_fields(&window, false, &mut pairs);
        assert!(pairs.contains(&(46, 0)), "a delivered zero is an observed clear");
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

    /// The typed row rides the existing container entity: same guid keeps the same entity and
    /// only a changed row is written, while a generation change replaces the entity and its row.
    #[test]
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
        assert_eq!(new_row.as_ref().and_then(|row| row.container_slot(0)), Some(0x11));
        let entity = index[&BAG];
        assert!(world.get::<crate::wrath_item::WrathItemRow>(entity).is_some());

        // The same generation updates in place; the changed row is the only write.
        remote.item_fields = Some(item_window(
            &[
                (fields::CONTAINER_FIELD_NUM_SLOTS, 2),
                (fields::CONTAINER_FIELD_SLOT_1, 0x22),
            ],
            fields::CONTAINER_END,
        ));
        let (_, _, old_row, new_row) = apply_item_world(&mut world, &mut index, BAG, &remote);
        assert_eq!(index[&BAG], entity, "the container entity is not reparented");
        assert_eq!(old_row.as_ref().and_then(|row| row.container_slot(0)), Some(0x11));
        assert_eq!(new_row.as_ref().and_then(|row| row.container_slot(0)), Some(0x22));
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(entity)
            .is_some_and(|row| row.container_slot(0) == Some(0x22)));

        // A generation change is a new object; the old entity and its row are gone.
        remote.generation = 2;
        let (_, _, old_row, _) = apply_item_world(&mut world, &mut index, BAG, &remote);
        assert!(old_row.is_none(), "the replaced entity's row does not carry over");
        let replacement = index[&BAG];
        assert_ne!(replacement, entity);
        assert!(world.get::<crate::wrath_item::WrathItemRow>(entity).is_none());
        assert!(world
            .get::<crate::wrath_item::WrathItemRow>(replacement)
            .is_some());
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
            let bank = benilla_protocol_wrath::fields::PLAYER_FIELD_BANK_SLOT_1
                + 2 * 24;
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

        let entity = world
            .spawn((WrathStreamed, Guid(0xAA), SelfPlayer))
            .id();
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
                player_bytes: None,
                player_bytes_2: None,
                position: None,
                move_flags: None,
                create_spline: None,
                generation: 1,
                player_fields: None,
                item_fields: None,
                auras: Vec::new(),
                auras_observed: false,
                aura_receipts: BTreeMap::new(),
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
        assert_eq!(sent.len(), 6, "Move is not duplicated by the tap");
        assert_eq!(sent[0].opcode, benilla_protocol_wrath::opcode::CMSG_SET_SELECTION);
        assert_eq!(sent[1].opcode, benilla_protocol_wrath::opcode::CMSG_ATTACKSWING);
        assert_eq!(sent[2].opcode, benilla_protocol_wrath::opcode::CMSG_CAST_SPELL);
        assert_eq!(sent[2].payload[0], 1);
        assert_eq!(u32::from_le_bytes(sent[2].payload[1..5].try_into().unwrap()), 133);
        assert_eq!(sent[3].opcode, benilla_protocol_wrath::opcode::CMSG_GOSSIP_SELECT_OPTION);
        assert_eq!(u32::from_le_bytes(sent[3].payload[8..12].try_into().unwrap()), 55);
        assert_eq!(sent[4].opcode, benilla_protocol_wrath::opcode::CMSG_QUESTGIVER_ACCEPT_QUEST);
        assert_eq!(sent[4].payload.len(), 16);
        assert_eq!(sent[5].opcode, benilla_protocol_wrath::opcode::CMSG_LOOT);
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
        assert!(matches!(WrathSessionStart::Auto.for_login_mode(true),
            WrathSessionStart::Parked { roster_asked: false }));
        assert!(matches!(WrathSessionStart::Auto.for_login_mode(false), WrathSessionStart::Auto));
        assert!(matches!(WrathSessionStart::Parked { roster_asked: true }.for_login_mode(true),
            WrathSessionStart::Parked { roster_asked: true }));
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
        scoped(1, 1, SessionEvent::AttackStart { attacker, victim: 2 })
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
        assert!(matches!(drained[1].event, SessionEvent::Disconnected { .. }));
        assert_eq!(replies.stats().dropped, 1, "the repeated LoggedOut");
        assert!(replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET).is_empty());
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
        assert!(matches!(drained[0].event, SessionEvent::AttackStart { attacker: 1, .. }));
        assert_eq!(unix_time(&drained[1]), Some(20), "newest value, original position");
        assert!(matches!(drained[2].event, SessionEvent::AttackStart { attacker: 3, .. }));
        assert_eq!(replies.stats().coalesced, 1);
    }

    #[test]
    fn gossip_and_vendor_observations_coalesce_per_npc_and_per_vendor() {
        let mut replies = SessionReplies::default();
        replies.push(scoped(1, 1, SessionEvent::GossipMenu {
            npc: 7,
            text_id: 1,
            options: Vec::new(),
            quests: Vec::new(),
        }));
        replies.push(scoped(1, 1, SessionEvent::VendorInventory {
            vendor: 9,
            items: Vec::new(),
        }));
        replies.push(scoped(1, 1, SessionEvent::GossipMenu {
            npc: 7,
            text_id: 2,
            options: Vec::new(),
            quests: Vec::new(),
        }));
        replies.push(scoped(1, 1, SessionEvent::VendorInventory {
            vendor: 9,
            items: Vec::new(),
        }));
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
        assert!(replies.overloaded(), "the worker turns this into a terminal");
        assert_eq!(replies.len(), WRATH_SESSION_REPLY_CAP, "memory stays bounded");
        // The reserved terminal slots still fit at the cap and are delivered first.
        replies.push(disconnected());
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 1);
        assert!(matches!(drained[0].event, SessionEvent::Disconnected { .. }));
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
        assert_eq!(replies.stats().closed_pushes, WRATH_SESSION_REPLY_CAP as u64 + 11);
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
        assert!(!replies.producer_should_pause(), "low water releases the latch");
    }

    #[test]
    fn a_transfer_prunes_only_strictly_older_replies_and_keeps_forced_edges() {
        let mut replies = SessionReplies::default();
        replies.push(scoped(1, 0, SessionEvent::AttackStart { attacker: 1, victim: 2 }));
        replies.push(scoped(2, 0, SessionEvent::AttackStart { attacker: 2, victim: 2 }));
        replies.push(scoped(2, 1, SessionEvent::AttackStart { attacker: 3, victim: 2 }));
        replies.push(forced(SessionEvent::LoginStage {
            stage: benilla_protocol::LoginStage::Connecting,
        }));
        replies.adopt_scope(2, 1);
        assert_eq!(replies.len(), 2, "the two older replies are gone");
        let drained = replies.take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert!(matches!(drained[0].event, SessionEvent::AttackStart { attacker: 3, .. }));
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
        assert_eq!(session_class(&SessionEvent::LoggedOut), SessionClass::Terminal);
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
        assert!(actions.pending.is_empty(), "the intent is consumed, not replayed");
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
        let drained = replies.lock().unwrap().take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
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
        assert_eq!(results.try_recv().map(|a| a.request_id), Ok("cast-1".to_owned()));

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
        let drained = replies.lock().unwrap().take(WRATH_DRAIN_LIMIT, WRATH_DRAIN_BUDGET);
        assert_eq!(drained.len(), 1, "only the adopted scope's reply is delivered");
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
}

