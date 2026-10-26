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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use benilla_protocol::{
    messages::ObjectType, wire::Vector3d, Character, CharEnumItem, EntityKind, MoveSpeeds,
    ObjectFields, RosterCause, SessionEvent,
};
use crate::client_state::ClientState;
use benilla_protocol_wrath::cast_failure;
use benilla_wm_native::{ActionBridge, ActionIntent, ActionRequest, ActionResult, ActionStatus};
use benilla_protocol_wrath::gameplay::{self, ServerEvent as WrathGameplayEvent};
use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
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
    ItemUseSpell as ProtocolItemUseSpell, LootItem as ProtocolLootItem,
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
}

/// A translated gameplay reply tagged with the source session and world scope. The app rejects
/// replies older than the scope it has already adopted, so a queue race across a transfer cannot
/// apply a retired world's tooltip, quest or inventory reply to the replacement world.
pub struct WrathSessionEvent {
    session_epoch: u64,
    world_generation: u64,
    event: SessionEvent,
    /// Terminal lifecycle events must be delivered even after the scope was invalidated.
    force: bool,
}

impl WrathSessionEvent {
    fn scoped(session: &SessionState, event: SessionEvent) -> Self {
        Self {
            session_epoch: session.epoch(),
            world_generation: session.world_generation(),
            event,
            force: false,
        }
    }

    fn forced(event: SessionEvent) -> Self {
        Self {
            session_epoch: 0,
            world_generation: 0,
            event,
            force: true,
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
    /// The replicated session phase changed (login, transfer boundary, logout).
    Phase(Phase),
    /// A world scope changed: the mapped entity set and pending publications for the old
    /// scope are invalid from this event on (issue 22 clears its entity map here).
    WorldTransfer { world_generation: u64, map_id: u32 },
    /// A producer export was refused with a reason â€” diagnostics for the debug panel.
    ExportSkipped(String),
    /// The IO-thread session ended (Ok = clean logout; Err = failure reason).
    SessionEnded(Result<(), String>),
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
}

impl WrathFeedEvent {
    fn from_session(event: LifeEvent) -> Self {
        match event {
            LifeEvent::WorldTransfer { world_generation, map_id } => {
                Self::WorldTransfer { world_generation, map_id }
            }
        }
    }
}

/// Shared handles between the IO thread and the app.
#[derive(Clone)]
pub struct WrathFeed {
    pub latest: WrathLatest,
    pub events: WrathEvents,
    /// Native cast/channel/item-text/failure traffic on its own bounded queue so it cannot
    /// displace the lifecycle edges.
    native: Arc<Mutex<VecDeque<crate::wrath_native::WrathNativeFeedEvent>>>,
}

impl WrathFeed {
    fn new() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(WRATH_EVENT_CAPACITY))),
            native: Arc::new(Mutex::new(VecDeque::with_capacity(
                WRATH_NATIVE_EVENT_CAPACITY,
            ))),
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
    pub account: String,
    pub password: String,
    pub auth_addr: String,
    pub world_addr: String,
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

impl WrathSessionConfig {
    /// Build the selected graphical path from the same protected environment the rest of the
    /// client uses. Credentials never come from committed source or command-line arguments.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        use anyhow::Context;
        if !selected_from_env() {
            return Ok(None);
        }
        let account = std::env::var("WOW_USER").context("WOW_WRATH needs WOW_USER")?;
        let password = std::env::var("WOW_PASS").context("WOW_WRATH needs WOW_PASS")?;
        let auth_addr =
            std::env::var("WOW_WRATH_AUTH").unwrap_or_else(|_| "127.0.0.1:3724".to_owned());
        let world_addr =
            std::env::var("WOW_WRATH_WORLD").unwrap_or_else(|_| "127.0.0.1:8095".to_owned());
        let dwell = std::env::var("WOW_WRATH_DWELL")
            .ok()
            .map(|raw| raw.parse::<u64>().map(Duration::from_secs))
            .transpose()
            .context("WOW_WRATH_DWELL must be whole seconds")?;
        let wm_token = std::env::var("WM_NATIVE_OBSERVATION_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        Ok(Some(Self {
            account,
            password,
            auth_addr,
            world_addr,
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
    pub npc_flags: Option<u32>,
    pub dynamic_flags: Option<u32>,
    /// `UNIT_FIELD_TARGET`; absent when the reducer has not observed both GUID words.
    pub target: Option<u64>,
    /// Wrath `PLAYER_BYTES`: skin, face, hair style, hair colour.
    pub player_bytes: Option<u32>,
    /// Wrath `PLAYER_BYTES_2`: facial hair, bank slots, rest state, arena faction.
    pub player_bytes_2: Option<u32>,
    pub position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    /// The last reduced movement flags for this object.
    pub move_flags: Option<u32>,
    pub generation: u32,
    /// The player's raw Wrath field window, needed where 1.12 and 3.3.5 pack different field
    /// indices (quest log, inventory, coinage). Only a player carries it.
    pub player_fields: Option<Vec<u32>>,
    /// An item or container's raw Wrath field window. The item block is 0..64 and a container
    /// extends it with its slot array; both are translated explicitly rather than copied by
    /// index because the 1.12 item/container layouts differ.
    pub item_fields: Option<Vec<u32>>,
    /// The unit's visible aura slots from `SMSG_AURA_UPDATE[_ALL]`. The 1.12 bar is descriptor
    /// data, so the translation synthesizes `UNIT_FIELD_AURA`/`AURAFLAGS` from these.
    pub auras: Vec<benilla_protocol_wrath::reducer::AuraSlot>,
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

/// One ask a parked session serves from the select screen, in the order the app made it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WrathParkedCommand {
    /// `GetCharacterListUpdate`: `CMSG_CHAR_ENUM` again on the parked link.
    Enumerate,
    /// Enter World the screen asked for: `CMSG_PLAYER_LOGIN` on the parked link.
    Enter(u64),
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
    pub(crate) received: u64,
    pub(crate) refused: u64,
    pub(crate) completed: u64,
}

struct PendingWmAction {
    intent: ActionIntent,
    /// The spell this action commits to, resolved at admission: a cast's own id or the cast
    /// being cancelled. Reply matching uses it so a cancel is attributed to its spell.
    cast_spell: Option<u32>,
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
        self.completed += 1;
        let _ = self.results.send(ActionResult {
            request_id,
            status,
            session_epoch,
            world_generation,
            observed_state_revision,
            detail,
        });
    }

    /// Terminal scope loss: every remaining intent becomes `Unknown`, because a command may
    /// already have reached the server. Nothing is replayed.
    fn fail_all(&mut self, status: ActionStatus, detail: &str) {
        let ids: Vec<String> = self.pending.keys().cloned().collect();
        for id in ids {
            if let Some(pending) = self.pending.remove(&id) {
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
    }
}

/// Application selection/configuration and worker ownership. Inserted by
/// [`WrathNetPlugin`]; a `None` config keeps the plugin inert (baseline login untouched).
#[derive(Resource)]
pub struct WrathControl {
    pub config: Option<WrathSessionConfig>,
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
    /// The existing glue returned to character selection after the clean logout. A replacement
    /// is admitted only after that visible stop, not merely because a terminal frame arrived.
    left_world: bool,
    /// AppExit or an explicit stop; no further starts.
    shutdown: bool,
    /// Terminal failure from the last worker run. A clean end leaves this `None`.
    pub last_error: Option<String>,
    /// Replies for the established `SessionEvent` handlers, tagged by source scope and drained
    /// in the Net stage rather than through the stock 1.12 inbound channel.
    session_events: Option<Receiver<WrathSessionEvent>>,
    /// The app's own end of that tagged channel. A panicked worker never sends its terminal
    /// event, so the ownership tick synthesizes the forced `Disconnected` through this clone.
    session_events_tx: Option<Sender<WrathSessionEvent>>,
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

impl WrathControl {
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
            if park.send(command).is_ok() {
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
        let feed = WrathFeed::new();
        self.feed = Some(feed.clone());
        self.last_error = None;
        self.ended = false;
        self.terminal_ready = false;
        self.retry_at = None;
        self.entered_world = false;
        self.left_world = false;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let commands = self.commands.clone();
        if let Some(commands) = commands.as_ref() {
            if let Ok(mut queue) = commands.lock() {
                queue.clear();
            }
        }
        let (events_out, events_in) = crossbeam_channel::unbounded();
        self.session_events = Some(events_in);
        self.session_events_tx = Some(events_out.clone());
        let (publisher_retire, publisher_retire_rx) = crossbeam_channel::unbounded();
        self.publisher_retire_rx = Some(publisher_retire_rx);
        let events_out = Some(events_out);
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
                if let Ok(mut q) = feed.events.lock() {
                    if q.len() >= WRATH_EVENT_CAPACITY {
                        q.pop_front();
                    }
                    q.push_back(WrathFeedEvent::SessionEnded(
                        result.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")),
                    ));
                }
                if let Some(sink) = events_out {
                    let end = if result.is_ok() {
                        benilla_protocol::SessionEnd::LoggedOut
                    } else {
                        benilla_protocol::SessionEnd::Lost
                    };
                    let reason = result
                        .err()
                        .map(|e| format!("{e:#}"))
                        .unwrap_or_else(|| "wrath session ended".to_owned());
                    let _ = sink.send(WrathSessionEvent::forced(SessionEvent::Disconnected {
                        reason,
                        end,
                    }));
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
        self.shutdown = true;
        self.restart_allowed = false;
        self.retry_at = None;
        self.cancel.store(true, Ordering::SeqCst);
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
        let Some(receiver) = self.session_events.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        while out.len() < WRATH_DRAIN_LIMIT {
            match receiver.try_recv() {
                Ok(event) => out.push(event),
                Err(_) => break,
            }
        }
        out
    }

    fn session_events_empty(&self) -> bool {
        match self.session_events.as_ref() {
            Some(receiver) => receiver.is_empty(),
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
    /// Active native casts keyed by caster, retaining 12340 identity/timing beyond the vanilla
    /// cast-bar edges. Removed by the authoritative terminal edge.
    pub casts: BTreeMap<u64, crate::wrath_native::WrathActiveCast>,
    /// Active native channels keyed by caster.
    pub channels: BTreeMap<u64, crate::wrath_native::WrathActiveChannel>,
    /// The last terminal cast edge, retained after its active entry is removed.
    pub last_cast_end: Option<crate::wrath_native::WrathCastEnded>,
    /// The last native inventory failure with identities/tail; transient display state.
    pub last_inventory_failure: Option<crate::wrath_native::WrathInventoryFailure>,
    /// Stable per-attempt native handle source. Independent of wire `cast_count`.
    pub next_cast_serial: u64,
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
            next_cast_serial: 1,
            next_cast_count: 1,
            empty: (),
        }
    }
}

impl WrathWorldView {
    /// The active native cast for `unit`, if the START edge is still open. `None` after the
    /// authoritative launch/failure/interruption edge, even though `last_cast_end` keeps its
    /// identity for diagnostics.
    pub fn casting(&self, unit: u64) -> Option<&crate::wrath_native::WrathActiveCast> {
        self.casts.get(&unit)
    }

    /// The active native channel for `unit`, if any.
    pub fn channeling(&self, unit: u64) -> Option<&crate::wrath_native::WrathActiveChannel> {
        self.channels.get(&unit)
    }

    /// Record that a local cancel command was issued for `spell_id`. This is not a terminal
    /// state: the authoritative failure/interruption/launch edge still decides the outcome.
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
    pub fn auras(&self, guid: u64) -> &[benilla_protocol_wrath::reducer::AuraSlot] {
        self.entities
            .get(&guid)
            .map_or(&[], |entity| entity.auras.as_slice())
    }

    /// The last terminal cast edge only when it belongs to the currently adopted scope. A
    /// transfer/replacement leaves the old record in place for diagnostics but this accessor
    /// never presents it as current.
    pub fn last_cast_end_in_scope(&self) -> Option<&crate::wrath_native::WrathCastEnded> {
        self.last_cast_end.as_ref().filter(|ended| {
            ended.scope.session_epoch == self.session_epoch
                && ended.scope.world_generation == self.world_generation
        })
    }
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
            left_world: false,
            shutdown: false,
            last_error: None,
            session_events: None,
            session_events_tx: None,
            park_commands: None,
            publisher_retiring: Vec::new(),
            publisher_retire_rx: None,
            wm_actions: None,
        })
        .init_resource::<WrathWorldView>()
        .add_message::<crate::wrath_native::WrathCastEvent>()
        .add_message::<crate::wrath_native::WrathItemText>();
        if self.config.is_some() {
            app.insert_resource(WrathInput(commands));
            // The select screen's asks and Enters travel the stock `CharPick` channel, whose
            // reader is the stock IO thread. That thread never spawns for the Wrath session,
            // so its receiver is dropped and every send dies silently. Rebind the resource to
            // a wrath-owned channel: the same ordered send path, drained by the pick
            // admission here (`roster::send_pick` and the screen's re-asks stay untouched).
            let (pick_tx, pick_rx) = crossbeam_channel::unbounded();
            app.insert_resource(CharPick(ParkSender::park(pick_tx)))
                .insert_resource(WrathPickQueue(pick_rx));
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
    let handed_off: Vec<_> = match control.publisher_retire_rx.as_ref() {
        Some(receiver) => receiver.try_iter().collect(),
        None => Vec::new(),
    };
    control.publisher_retiring.extend(handed_off);
    control
        .publisher_retiring
        .retain(|retirement| !retirement.is_finished());
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
            if let Some(sink) = control.session_events_tx.as_ref() {
                let _ = sink.send(WrathSessionEvent::forced(SessionEvent::Disconnected {
                    reason,
                    end: benilla_protocol::SessionEnd::Lost,
                }));
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
    if control.shutdown || (*state.get() != ClientState::InWorld && !may_retry) {
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
        control.start(WrathSessionStart::Auto);
    }
}

/// The select screen's asks, drained onto the Wrath session (the stock run's channel reader
/// lives in the IO thread this plugin replaces). At character selection an enumeration or an
/// Enter World ask lands on the running parked session, or starts one and lands there; the
/// admission gates are `start`'s. Elsewhere the asks are stale and dropped with their order
/// intact. Create, delete and rename are the stock run's park loop and have no Wrath
/// counterpart yet; a select-screen ask for one is refused with a note, nothing sent.
fn wrath_pick_admission(
    mut control: ResMut<WrathControl>,
    state: Res<State<ClientState>>,
    pick: Option<Res<WrathPickQueue>>,
) {
    let Some(pick) = pick.as_ref() else { return };
    let at_select = *state.get() == ClientState::CharSelect;
    for request in pick.0.try_iter() {
        let ParkRequest::Char(request) = request else {
            // Realm asks stay the stock run's path: the wrath session keeps the one realm it
            // dialed, and a realm dialog over the select screen is the stock run's flow.
            continue;
        };
        match request {
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
            CharRequest::Create(_) | CharRequest::Delete(_) | CharRequest::Rename { .. } => {
                warn!("wrath: the select screen's create, delete and rename asks are not wired to the wrath session; the graphical flow keeps the records the account holds");
            }
            CharRequest::Abandon => {
                // The parked session's stop is app-owned (the shutdown edge owns the cancel);
                // a select-screen Back is the stock run's park, not a Wrath flow.
            }
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

        let mut commands: Vec<ClientCommand> = Vec::new();
        let mut refused: Option<&str> = None;
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
                commands.push(ClientCommand::CastSpell {
                    spell_id: *spell_id,
                    target: *target,
                });
            }
            ActionIntent::CastCancel => match actions.last_cast_spell {
                Some(spell_id) => commands.push(ClientCommand::CancelCast { spell_id }),
                None => refused = Some("no client-known cast to cancel"),
            },
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
            ActionIntent::CastCancel => actions.last_cast_spell,
            _ => None,
        };
        actions.pending.insert(
            request.request_id.clone(),
            PendingWmAction {
                intent: request.intent.clone(),
                cast_spell,
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

/// The stock wire position translated to 12340's player array. Bank slots 39-62 and the
/// inventory/backpack agree; the six stock bank bags 63-68 move to 67-72, and the 1.12 keyring
/// slots 81-96 (plus the buyback range) move five positions later.
fn wrath_item_pos(bag: u8, slot: u8) -> (u8, u8) {
    if (63..=68).contains(&bag) {
        (bag + 4, slot)
    } else if bag == 255 && (69..=112).contains(&slot) {
        (bag, slot + 5)
    } else {
        (bag, slot)
    }
}

/// 12340 bank bags start at 67 where the stock owner uses 63; slots inside the bank block
/// (39-66) and the inventory/backpack indices agree, so only the six stock bank bags move.
fn wrath_bank_bag(bag: u8) -> u8 {
    wrath_item_pos(bag, 0).0
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
fn wrath_shutdown_on_exit(mut control: ResMut<WrathControl>, mut exits: MessageReader<AppExit>) {
    if exits.read().next().is_some() {
        control.stop();
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

fn quest_entries(quests: Vec<gameplay::QuestListEntry>) -> Vec<QuestListEntry> {
    quests
        .into_iter()
        .map(|q| QuestListEntry {
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
fn translate_gameplay(event: WrathGameplayEvent, self_guid: u64) -> Vec<SessionEvent> {
    match event {
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
            ..
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
        } => vec![SessionEvent::SpellGo {
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
        }],
        WrathGameplayEvent::InitialSpells { spells } => vec![SessionEvent::SpellBook {
            spell_ids: spells,
            cooldowns: Vec::new(),
        }],
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
        WrathGameplayEvent::ActionButtons { buttons } => vec![SessionEvent::ActionButtons {
            buttons: buttons
                .into_iter()
                .map(|b| ActionButton {
                    slot: b.slot,
                    action: b.action,
                    kind: b.kind,
                })
                .collect(),
        }],
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
                rewards: t.rewards,
                choices: [
                    t.choices[0],
                    t.choices[1],
                    t.choices[2],
                    t.choices[3],
                    (0, 0),
                    (0, 0),
                ],
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
    }
}

/// The action/UI commands the stock 1.12 net thread would have consumed, translated to the
/// 3.3.5 wire. The stock UI remains the only producer and its existing handlers remain the
/// only consumers of replies.
fn wrath_tap_commands(
    tap: Option<ResMut<NetCommandTap>>,
    mut view: ResMut<WrathWorldView>,
    input: ResMut<WrathInput>,
) {
    let Some(mut tap) = tap else { return };
    let Some(receiver) = tap.0.as_mut() else { return };
    while let Ok(command) = receiver.try_recv() {
        let count = view.next_cast_count;
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
            ClientCommand::CastSpell { spell_id, target } => {
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
            ClientCommand::CancelCast { spell_id } => {
                let self_guid = view.player_guid;
                view.note_cancel_requested(self_guid, spell_id);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CANCEL_CAST,
                    gameplay::cancel_cast(spell_id),
                ))
            }
            ClientCommand::CancelChannelling { spell_id } => {
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
                gameplay::autostore_bank_item(wrath_bank_bag(bag), slot),
            )),
            ClientCommand::CastSpellGameObject { spell_id, go_guid } => {
                let count = view.next_cast_count;
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_gameobject(count, spell_id, go_guid),
                ))
            }
            ClientCommand::CastSpellItem { spell_id, item_guid } => {
                let count = view.next_cast_count;
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_item(count, spell_id, item_guid),
                ))
            }
            ClientCommand::CastSpellTradeItem { spell_id, slot } => {
                let count = view.next_cast_count;
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
            } => {
                use benilla_protocol::messages::CorpseTarget;
                let count = view.next_cast_count;
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
            ClientCommand::CastSpellAtDest { spell_id, dest } => {
                let count = view.next_cast_count;
                view.next_cast_count = count.wrapping_add(1).max(1);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_CAST_SPELL,
                    gameplay::cast_spell_at_dest(count, spell_id, dest),
                ))
            }
            ClientCommand::CastSpellAtSource { spell_id, src } => {
                let count = view.next_cast_count;
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
                    let (bag, slot) = wrath_item_pos(bag_index, slot);
                    let count = view.next_cast_count;
                    view.next_cast_count = count.wrapping_add(1).max(1);
                    Some((
                        benilla_protocol_wrath::opcode::CMSG_USE_ITEM,
                        gameplay::use_item(bag, slot, count, spell_id, item_guid, target),
                    ))
                }
                _ => {
                    debug!(
                        "wrath: item use dropped (no item guid or an unsupported target block)"
                    );
                    None
                }
            },
            ClientCommand::OpenItem { bag_index, slot } => {
                let (bag, slot) = wrath_item_pos(bag_index, slot);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_OPEN_ITEM,
                    gameplay::open_item(bag, slot),
                ))
            }
            ClientCommand::AutoEquipItem { bag_index, slot } => {
                let (bag, slot) = wrath_item_pos(bag_index, slot);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_AUTOEQUIP_ITEM,
                    gameplay::auto_equip_item(bag, slot),
                ))
            }
            ClientCommand::SwapInvItem { src_slot, dst_slot } => Some((
                benilla_protocol_wrath::opcode::CMSG_SWAP_INV_ITEM,
                gameplay::swap_inv_item(dst_slot, src_slot),
            )),
            ClientCommand::SwapItem {
                dst_bag,
                dst_slot,
                src_bag,
                src_slot,
            } => {
                let (dst_bag, dst_slot) = wrath_item_pos(dst_bag, dst_slot);
                let (src_bag, src_slot) = wrath_item_pos(src_bag, src_slot);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_SWAP_ITEM,
                    gameplay::swap_item(dst_bag, dst_slot, src_bag, src_slot),
                ))
            }
            ClientCommand::AutoStoreBagItem {
                src_bag,
                src_slot,
                dst_bag,
            } => {
                let (src_bag, src_slot) = wrath_item_pos(src_bag, src_slot);
                let (dst_bag, _) = wrath_item_pos(dst_bag, 0);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_AUTOSTORE_BAG_ITEM,
                    gameplay::auto_store_bag_item(src_bag, src_slot, dst_bag),
                ))
            }
            ClientCommand::SplitItem {
                src_bag,
                src_slot,
                dst_bag,
                dst_slot,
                count,
            } => {
                let (src_bag, src_slot) = wrath_item_pos(src_bag, src_slot);
                let (dst_bag, dst_slot) = wrath_item_pos(dst_bag, dst_slot);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_SPLIT_ITEM,
                    gameplay::split_item(src_bag, src_slot, dst_bag, dst_slot, u32::from(count)),
                ))
            }
            ClientCommand::DestroyItem {
                bag_index,
                slot,
                count,
            } => {
                let (bag, slot) = wrath_item_pos(bag_index, slot);
                Some((
                    benilla_protocol_wrath::opcode::CMSG_DESTROYITEM,
                    gameplay::destroy_item(bag, slot, count),
                ))
            }
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
            _ => None,
        };
        if let Some((opcode, payload)) = packet {
            if let Ok(mut queue) = input.0.lock() {
                if queue.len() < WRATH_OUTBOX_CAP {
                    queue.push_back(WrathOutbound {
                        opcode,
                        payload,
                        movement: false,
                    });
                } else {
                    // The only way to fill this queue is a burst of uncapped actions or a
                    // session whose writer stopped draining; refuse visibly instead of hiding
                    // the loss. Movement never fills it (its own cap evicts motion first).
                    warn!(
                        "wrath: outbox full ({}), refusing client opcode {opcode:#x}",
                        WRATH_OUTBOX_CAP
                    );
                }
            }
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
fn scope_is_older(
    current_epoch: u64,
    current_world: u64,
    event_epoch: u64,
    event_world: u64,
) -> bool {
    event_epoch < current_epoch
        || (event_epoch == current_epoch && event_world < current_world)
}

/// Deliver Wrath replies after the Net stage has adopted the matching world scope. This is the
/// only path for Wrath gameplay replies; the stock 1.12 inbound channel is not used, so a queued
/// old-world reply cannot overtake the transfer teardown.
fn wrath_dispatch_events(world: &mut World) {
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
        if event.force
            || (!ended
                && !scope_is_older(
                    current_epoch,
                    current_world,
                    event.session_epoch,
                    event.world_generation,
                ))
        {
            deliver.push(event.event);
        }
    }
    if !deliver.is_empty() {
        route_wm_action_outcomes(world, &deliver);
        crate::net::handlers::dispatch(world, deliver);
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
            let outcome = match (event, &pending.intent) {
                (
                    SessionEvent::CastResult {
                        spell_id,
                        success,
                        reason,
                        ..
                    },
                    ActionIntent::Cast {
                        spell_id: wanted, ..
                    },
                ) if spell_id == wanted => Some((
                    if *success {
                        ActionStatus::Succeeded
                    } else {
                        ActionStatus::Failed
                    },
                    (*reason).map(|reason| format!("cast result {reason}")),
                )),
                (
                    SessionEvent::SpellGo {
                        caster, spell_id, ..
                    },
                    ActionIntent::Cast {
                        spell_id: wanted, ..
                    },
                ) if Some(*caster) == self_guid && spell_id == wanted => {
                    Some((ActionStatus::Succeeded, None))
                }
                (
                    SessionEvent::SpellFailedOther { caster, spell_id },
                    ActionIntent::Cast { .. } | ActionIntent::CastCancel,
                ) if Some(*caster) == self_guid && pending.cast_spell == Some(*spell_id) => {
                    let status = match pending.intent {
                        ActionIntent::CastCancel => ActionStatus::Succeeded,
                        _ => ActionStatus::Failed,
                    };
                    Some((status, Some("cast interrupted".to_owned())))
                }
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
) {
    let ui_now_seconds = clock.as_ref().map(|clock| clock.ui_now);
    let native = control
        .feed
        .as_ref()
        .map(|feed| feed.take_native())
        .unwrap_or_default();
    let mut drained = 0usize;
    for event in control.take_events() {
        if drained >= WRATH_DRAIN_LIMIT {
            break;
        }
        drained += 1;
        match event {
            WrathFeedEvent::Phase(phase) => view.phase = Some(phase),
            WrathFeedEvent::WorldTransfer { world_generation, map_id } => {
                // Same-frame teardown: old-scope entities are gone before Input runs.
                for event in crate::wrath_native::invalidate_scope(
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
            WrathFeedEvent::ExportSkipped(reason) => {
                debug!("wrath export skipped: {reason}");
            }
            WrathFeedEvent::SessionEnded(result) => {
                control.ended = true;
                control.terminal_ready = true;
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
                    && control.park_commands.is_none()
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
                if let Some(actions) = actions.as_deref_mut() {
                    actions.fail_all(
                        ActionStatus::Unknown,
                        "session ended before an authoritative result",
                    );
                }
                for event in crate::wrath_native::invalidate_scope(
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
            match event {
                crate::wrath_native::WrathNativeFeedEvent::Cast(update) => {
                    if let Some(mut event) =
                        crate::wrath_native::apply_update(&mut *view, update)
                    {
                        let started = matches!(
                            &event.kind,
                            crate::wrath_native::WrathCastEventKind::Started { .. }
                                | crate::wrath_native::WrathCastEventKind::ChannelStarted { .. }
                        );
                        let cast_edge = matches!(
                            &event.kind,
                            crate::wrath_native::WrathCastEventKind::Started { .. }
                                | crate::wrath_native::WrathCastEventKind::Delayed { .. }
                                | crate::wrath_native::WrathCastEventKind::Launched { .. }
                                | crate::wrath_native::WrathCastEventKind::Failed { .. }
                                | crate::wrath_native::WrathCastEventKind::Interrupted { .. }
                        );
                        let channel_edge = matches!(
                            &event.kind,
                            crate::wrath_native::WrathCastEventKind::ChannelStarted { .. }
                                | crate::wrath_native::WrathCastEventKind::ChannelUpdated { .. }
                                | crate::wrath_native::WrathCastEventKind::ChannelEnded
                        );
                        if started {
                            event.start_ui_now_seconds = ui_now_seconds;
                        }
                        event.ui_now_seconds = ui_now_seconds;
                        if let Some(identity) = event.identity {
                            if cast_edge {
                                if let Some(cast) = view.casts.get_mut(&identity.caster) {
                                    if matches!(
                                        &event.kind,
                                        crate::wrath_native::WrathCastEventKind::Started { .. }
                                    ) {
                                        cast.start_ui_now_seconds = ui_now_seconds;
                                    }
                                    cast.last_update_ui_now_seconds = ui_now_seconds;
                                }
                            }
                            if channel_edge {
                                if let Some(channel) = view.channels.get_mut(&identity.caster) {
                                    if matches!(
                                        &event.kind,
                                        crate::wrath_native::WrathCastEventKind::ChannelStarted { .. }
                                    ) {
                                        channel.start_ui_now_seconds = ui_now_seconds;
                                    }
                                    channel.last_update_ui_now_seconds = ui_now_seconds;
                                }
                            }
                        }
                        cast_events.write(event);
                    }
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

    // A worker can exit after its final event was already drained (or panicked without one). The
    // exposed view is invalid on that terminal edge even when no digest follows.
    if control.ended {
        if let Some(actions) = actions.as_deref_mut() {
            actions.fail_all(
                ActionStatus::Unknown,
                "session ended before an authoritative result",
            );
        }
        for event in crate::wrath_native::invalidate_scope(
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
        if let Some(actions) = actions.as_deref_mut() {
            actions.fail_all(
                ActionStatus::Unknown,
                "world scope changed before an authoritative result",
            );
        }
        // A replacement session restarts its epoch, world generation and revision counters.
        // Adopt them as a new scope rather than comparing them with the retired session's
        // monotonic values.
        for event in crate::wrath_native::invalidate_scope(
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
        view.last_changes.world_transferred = true;
        view.state_revision = 0;
    } else {
        if digest.world_generation < view.world_generation {
            return; // stale world scope
        }
        if digest.world_generation != view.world_generation {
            if let Some(actions) = actions.as_deref_mut() {
                actions.fail_all(
                    ActionStatus::Unknown,
                    "world scope changed before an authoritative result",
                );
            }
            for event in crate::wrath_native::invalidate_scope(
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

/// Copy the player inventory bands the 1.12 item owners walk. The 3.3.5 arrays are wider in the
/// bank (28 -> 24 slots), bank bags (7 -> 6) and otherwise use a different base; every array here
/// is copied explicitly, with the extra 3.3.5 tail dropped rather than folded into a neighbour.
fn push_player_inventory_fields(raw: &[u32], pairs: &mut Vec<(u16, u32)>) {
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
            if let Some(value) = raw.get(src + i) {
                pairs.push((dst + i as u16, *value));
            }
        }
    }
}

/// Translate one item/container's Wrath fields into the 1.12 item layout. The first fields
/// coincide; the enchant array is shorter in 1.12, and the durability/random columns moved after
/// the two extra 3.3.5 enchant slots. Container fields start at a different base.
fn push_item_fields(raw: &[u32], container: bool, pairs: &mut Vec<(u16, u32)>) {
    for src in 6..=21u16 {
        if let Some(value) = raw.get(src as usize) {
            pairs.push((src, *value));
        }
    }
    for i in 0..21u16 {
        if let Some(value) = raw.get((22 + i) as usize) {
            pairs.push((22 + i, *value));
        }
    }
    for (src, dst) in [(58u16, 43u16), (59, 44), (60, 46), (61, 47)] {
        if let Some(value) = raw.get(src as usize) {
            pairs.push((dst, *value));
        }
    }
    if container {
        if let Some(value) = raw.get(64) {
            pairs.push((48, *value));
        }
        for i in 0..36u16 {
            if let Some(value) = raw.get((66 + 2 * i) as usize) {
                pairs.push((50 + 2 * i, *value));
            }
            if let Some(value) = raw.get((67 + 2 * i) as usize) {
                pairs.push((51 + 2 * i, *value));
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

/// Spawn or update one item/container entity in the established `ItemObject`/`ObjectStore`
/// owner. Items carry no `NetEntity` or transform; their GUID identity stays in `GuidIndex`.
fn apply_item_world(
    world: &mut World,
    index: &mut HashMap<u64, Entity>,
    guid: u64,
    remote: &WrathEntity,
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
    if let Some(raw) = remote.item_fields.as_deref() {
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
    match existing {
        Some(entity) => {
            let mut e = world.entity_mut(entity);
            e.remove::<NetEntity>();
            e.insert((
                ObjectStore(fields),
                ItemObject { container },
                WrathGeneration(remote.generation),
            ));
        }
        None => {
            let entity = world
                .spawn((
                    WrathStreamed,
                    WrathGeneration(remote.generation),
                    Guid(guid),
                    ObjectStore(fields),
                    ItemObject { container },
                    Countdowns::default(),
                ))
                .id();
            index.insert(guid, entity);
        }
    }
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
        if let Some(input) = world.get_resource::<WrathInput>() {
            if let Ok(mut queue) = input.0.lock() {
                queue.clear();
            }
        }
        if let Some(mut bags) = world.get_resource_mut::<crate::item_watch::BagCache>() {
            *bags = crate::item_watch::BagCache::default();
        }
        if let Some(mut lane) = world.get_resource_mut::<crate::item_lane::ItemLane>() {
            lane.clear();
        }
    }
    *world.resource_mut::<GuidIndex>() = index;

    if player_guid == 0
        || !matches!(
            phase,
            Some(Phase::InWorld | Phase::TransferPending | Phase::Loading)
        )
    {
        return;
    }
    let mut index = std::mem::take(&mut *world.resource_mut::<GuidIndex>());

    for guid in &changes.removed {
        if let Some(entity) = index.0.remove(guid) {
            world.despawn(entity);
        }
    }

    for guid in changes.created.iter().chain(changes.updated.iter()) {
        let Some(remote) = entities.get(guid) else {
            continue;
        };
        if matches!(
            remote.type_id,
            benilla_protocol_wrath::updates::type_id::ITEM
                | benilla_protocol_wrath::updates::type_id::CONTAINER
        ) {
            apply_item_world(world, &mut index.0, *guid, remote);
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
        if let Some(bytes) = remote.bytes_1 {
            pairs.push((138, bytes));
        }
        if let Some(flags) = remote.npc_flags {
            pairs.push((147, flags));
        }
        if let Some(flags) = remote.dynamic_flags {
            pairs.push((143, flags));
        }
        if let Some(display) = remote.display_id {
            pairs.push((131, display));
            pairs.push((132, display));
        }
        if let Some(bytes) = remote.bytes_0 {
            pairs.push((36, bytes));
        }
        if let Some(target) = remote.target {
            pairs.push((16, target as u32));
            pairs.push((17, (target >> 32) as u32));
        }
        if kind == EntityKind::Player {
            if let Some(bytes) = remote.player_bytes {
                pairs.push((193, bytes));
            }
            if let Some(bytes) = remote.player_bytes_2 {
                pairs.push((194, bytes));
            }
            if let Some(raw) = remote.player_fields.as_deref() {
                // 3.3.5 uses 148 as UNIT_END; 1.12 uses a different player layout. Map the
                // durable fields the stock quest/loot/player frames read.
                for slot in 0..20usize {
                    let src = 158 + slot * 5;
                    let id = raw.get(src).copied().unwrap_or(0);
                    if id == 0 {
                        continue;
                    }
                    let state = raw.get(src + 1).copied().unwrap_or(0);
                    let counts = u64::from(raw.get(src + 2).copied().unwrap_or(0))
                        | (u64::from(raw.get(src + 3).copied().unwrap_or(0)) << 32);
                    let timer = raw.get(src + 4).copied().unwrap_or(0);
                    let mut packed = (state & 0xff) << 24;
                    for i in 0..4 {
                        let count = ((counts >> (i * 16)) & 0xffff).min(63) as u32;
                        packed |= count << (i * 6);
                    }
                    let dst = 198 + slot * 3;
                    pairs.push((dst as u16, id));
                    pairs.push(((dst + 1) as u16, packed));
                    pairs.push(((dst + 2) as u16, timer));
                }
                for (src, dst) in [(25u16, 23u16), (26, 24), (27, 25), (33, 29), (34, 30), (35, 31)] {
                    if let Some(value) = raw.get(src as usize) {
                        pairs.push((dst, *value));
                    }
                }
                // `PLAYER_FLAGS` (3.3.5 150 -> 1.12 190): the ghost bit 0x10 is what the death
                // owner reads off the self descriptor.
                if let Some(value) = raw.get(150) {
                    pairs.push((benilla_protocol::field::FIELD_PLAYER_FLAGS, *value));
                }
                push_player_inventory_fields(raw, &mut pairs);
                if let Some(value) = raw.get(634) {
                    pairs.push((716, *value));
                }
                if let Some(value) = raw.get(635) {
                    pairs.push((717, *value));
                }
                if let Some(value) = raw.get(1170) {
                    pairs.push((1176, *value));
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
                let old_health = old.as_ref().and_then(benilla_protocol::ObjectFields::unit_health);
                let old_max = old
                    .as_ref()
                    .and_then(benilla_protocol::ObjectFields::unit_max_health);
                let new_span = matches!(kind, EntityKind::Unit | EntityKind::Player)
                    .then(|| aura_span(&fields));
                world.entity_mut(entity).insert((
                    NetEntity {
                        kind,
                        display_id: remote.display_id,
                        scale: remote.scale.unwrap_or(1.0),
                    },
                    ObjectStore(fields),
                ));
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
                    if let Some(transform) = transform {
                        world.entity_mut(entity).insert(transform);
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
                    if server_flags & MOVING == 0 && local_flags & MOVING == 0 {
                        let bevy = wow_to_bevy([pose.x, pose.y, pose.z]);
                        let current = world
                            .get_resource::<crate::player::Player>()
                            .map_or(Vec3::ZERO, |p| p.pos);
                        if current.distance_squared(bevy) > 0.01 {
                            if let Some(mut player) = world
                                .get_resource_mut::<crate::player::Player>()
                            {
                                player.apply_server_pose(bevy, pose.o);
                            }
                        }
                    }
                }
            }
            None => {
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
                    transform.unwrap_or_default(),
                ));
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
    let pose_changed = local.pos.replace(pos) != Some(pos) || local.facing.replace(facing) != Some(facing);
    let periodic = local
        .last_send
        .is_none_or(|at| now.duration_since(at) >= Duration::from_millis(500));
    if !changed && !pose_changed && !periodic {
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
                return;
            };
            let _ = queue.remove(oldest_motion);
        }
        queue.push_back(WrathOutbound {
            opcode,
            payload,
            movement: true,
        });
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
    events_out: Option<Sender<WrathSessionEvent>>,
    parked: Option<ParkedInput>,
    publisher_retire: Option<Sender<benilla_wm_native::PublisherRetirement>>,
) -> anyhow::Result<()> {
    use benilla_wm_native::{pump_once, replace_publisher, PublisherConfig};

    let (parked, roster_asked) = match parked {
        Some(input) => (Some(input.commands), input.roster_asked),
        None => (None, false),
    };

    let logon = benilla_protocol_wrath::logon::logon(&config.auth_addr, &config.account, &config.password)?;
    let realm = logon
        .realms
        .first()
        .ok_or_else(|| anyhow::anyhow!("realm list is empty"))?;

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
    feed.push_event(WrathFeedEvent::Phase(session.phase()));
    let started = std::time::Instant::now();

    let (mut world, seed) =
        benilla_protocol_wrath::world::WorldConnection::connect(&config.world_addr)?;
    world.stream_mut().set_read_timeout(Some(Duration::from_millis(200)))?;
    world.auth_session(&logon.account, &logon.session_key, &seed, realm.realm_id as u32)?;
    session.note_player_login();

    struct Adapter<'a> {
        session: &'a mut SessionState,
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
                    Ok(Some(SnapshotOwned(snapshot)))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(benilla_wm_native::ProducerExportError::new(e.to_string())),
            }
        }
        fn mark_published(&mut self) {
            self.session.mark_published();
        }
    }
    struct SnapshotOwned(SelfObservation);
    impl benilla_wm_native::ProducerSnapshot for SnapshotOwned {
        fn session_epoch(&self) -> u64 {
            self.0.session_epoch
        }
        fn player_guid(&self) -> u64 {
            self.0.player_guid
        }
        fn sequence(&self) -> u64 {
            self.0.sequence
        }
        fn state_revision(&self) -> u64 {
            self.0.state_revision
        }
        fn world_generation(&self) -> u64 {
            self.0.world_generation
        }
        fn position_observed_at_unix_ms(&self) -> i64 {
            self.0.position_observed_at_unix_ms
        }
        fn observed_at_unix_ms(&self) -> i64 {
            self.0.observed_at_unix_ms
        }
        fn client_uptime_ms(&self) -> u64 {
            self.0.client_uptime_ms
        }
        fn map_id(&self) -> u32 {
            self.0.map_id
        }
        fn position(&self) -> benilla_wm_native::ProducerPosition {
            benilla_wm_native::ProducerPosition {
                x: self.0.position.x,
                y: self.0.position.y,
                z: self.0.position.z,
                orientation: self.0.position.o,
            }
        }
        fn target(&self) -> benilla_wm_native::Result<benilla_wm_native::TargetObservation> {
            Ok(match &self.0.target {
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
            match self.0.combat {
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
        fn resources(&self) -> benilla_wm_native::ResourceObservation {
            benilla_wm_native::ResourceObservation {
                health: self.0.resources.health,
                max_health: self.0.resources.max_health,
                power_type: self.0.resources.power_type,
                power: self.0.resources.power,
            }
        }
        fn death(&self) -> benilla_wm_native::DeathObservation {
            match self.0.death {
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
                self.0.visibility.complete,
                self.0.visibility.total_guid_count,
                self.0.visibility.guids.clone(),
            )
        }
        fn zone_id(&self) -> Option<u32> {
            self.0.zone_id
        }
        fn area_id(&self) -> Option<u32> {
            self.0.area_id
        }
    }

    // Character selection: reuse the renderless driver's create-first flow is intentionally
    // out of scope here â€” the graphical path logs into an existing character (issue 23's
    // driver owns creation). Enumerate and pick the first disposable character.
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
        if let Some(sink) = events_out.as_ref() {
            let _ = sink.send(WrathSessionEvent::scoped(
                &session,
                SessionEvent::CharacterList {
                    characters: roster_from(records),
                    realm: None,
                    cause,
                    epoch: session.epoch(),
                    generation: 0,
                },
            ));
        }
    };
    // The parked session publishes the fresh roster as the reply the screen's ask ordered; the
    // boot flow publishes nothing, its own first record decides the entry. A later re-ask
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
        while entered.is_none() {
            if cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            for request in parked.try_iter() {
                match request {
                    WrathParkedCommand::Enumerate => {
                        world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
                    }
                    WrathParkedCommand::Enter(picked) => {
                        if records.iter().any(|record| record.guid == picked) {
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
                }
            }
            if entered.is_some() {
                break;
            }
            match world.recv() {
                Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_ENUM => {
                    records = benilla_protocol_wrath::gameplay::parse_char_enum(&p.payload)?;
                    publish_roster(&records, RosterCause::Asked);
                }
                Ok(_) => {}
                Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
                Err(e) => return Err(e),
            }
        }
        entered.expect("picked guid")
    } else {
        records
            .first()
            .map(|record| record.guid)
            .expect("disposable record guid")
    };
    session.start_session(guid); // re-begin with the real full GUID; phase resets to selection
    session.note_player_login();
    feed.push_event(WrathFeedEvent::Phase(session.phase()));
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
        if cancel.load(Ordering::SeqCst) {
            publisher.retire_all();
            return Ok(());
        }
        if let Some(commands) = commands.as_ref() {
            let pending: Vec<_> = commands
                .lock()
                .map(|mut queue| queue.drain(..).collect())
                .unwrap_or_default();
            for command in pending {
                world.send(command.opcode, &command.payload)?;
            }
        }
        if last_ping.elapsed() >= Duration::from_secs(10) {
            let mut ping = ping_seq.to_le_bytes().to_vec();
            ping.extend_from_slice(&0u32.to_le_bytes());
            world.send(benilla_protocol_wrath::opcode::CMSG_PING, &ping)?;
            ping_seq += 1;
            last_ping = std::time::Instant::now();
        }
        match world.recv() {
            Ok(p) => {
                last_server_response = std::time::Instant::now();
                session.reduce(p.opcode, &p.payload, now_ms())?;
                if let Some(parsed) = gameplay::parse(p.opcode, &p.payload) {
                    match parsed {
                        Ok(event @ WrathGameplayEvent::Gossip { npc, menu_id, .. }) => {
                            feed.push_event(WrathFeedEvent::GossipMenu {
                                npc,
                                menu_id,
                                session_epoch: session.epoch(),
                                world_generation: session.world_generation(),
                            });
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event, session.player_guid().unwrap_or(0)) {
                                        let _ = events_out
                                            .send(WrathSessionEvent::scoped(&session, event));
                                    }
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
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event, session.player_guid().unwrap_or(0)) {
                                        let _ = events_out
                                            .send(WrathSessionEvent::scoped(&session, event));
                                    }
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
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event, session.player_guid().unwrap_or(0)) {
                                        let _ = events_out
                                            .send(WrathSessionEvent::scoped(&session, event));
                                    }
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
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event, session.player_guid().unwrap_or(0)) {
                                        let _ = events_out
                                            .send(WrathSessionEvent::scoped(&session, event));
                                    }
                                }
                            }
                        }
                        Ok(WrathGameplayEvent::LoggedOut) => {
                            logged_out = true;
                            if let Some(events_out) = events_out.as_ref() {
                                let _ = events_out.send(WrathSessionEvent::forced(
                                    SessionEvent::LoggedOut,
                                ));
                            }
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
                            if let Some(update) = crate::wrath_native::update_from_gameplay(
                                &event,
                                crate::wrath_native::WrathScope {
                                    session_epoch: session.epoch(),
                                    world_generation: session.world_generation(),
                                    state_revision: session.state_revision(),
                                },
                                now_ms(),
                                session.player_guid().unwrap_or(0),
                            ) {
                                feed.push_native(crate::wrath_native::WrathNativeFeedEvent::Cast(
                                    update,
                                ));
                            }
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event, session.player_guid().unwrap_or(0)) {
                                        let _ = events_out
                                            .send(WrathSessionEvent::scoped(&session, event));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            debug!("wrath: ignored malformed 0x{:X} gameplay reply: {e}", p.opcode);
                        }
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
            Err(e) => return Err(e),
        }
        if logged_out {
            session.finish_session();
            feed.push_event(WrathFeedEvent::Phase(session.phase()));
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
            anyhow::bail!(
                "world server made no response for {} s",
                SERVER_RESPONSE_TIMEOUT.as_secs()
            );
        }
        for event in session.take_lifecycle() {
            // The same lifecycle policy as the renderless driver: retire the old scope and order
            // the replacement after that worker stops, without blocking the packet loop.
            replace_publisher(
                publisher_config.as_ref(),
                &mut publisher.publisher,
                &mut publisher.retiring,
            )?;
            feed.push_event(WrathFeedEvent::from_session(event));
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
            feed.push_event(WrathFeedEvent::Phase(session.phase()));
        }
    }

    world.send(benilla_protocol_wrath::opcode::CMSG_LOGOUT_REQUEST, &[])?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        match world.recv() {
            Ok(p) if p.opcode == 0x04D => {
                session.finish_session();
                feed.push_event(WrathFeedEvent::Phase(session.phase()));
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
                entry: entry.field(fields::OBJECT_FIELD_ENTRY),
                display_id: entry.field(fields::UNIT_FIELD_DISPLAYID),
                scale: entry.field(fields::OBJECT_FIELD_SCALE_X).map(f32::from_bits),
                health: observed(fields::UNIT_FIELD_HEALTH),
                max_health: observed(fields::UNIT_FIELD_MAXHEALTH),
                powers,
                max_powers,
                level: entry.field(fields::UNIT_FIELD_LEVEL),
                bytes_0: entry.field(fields::UNIT_FIELD_BYTES_0),
                faction: entry.field(fields::UNIT_FIELD_FACTIONTEMPLATE),
                unit_flags: entry.field(fields::UNIT_FIELD_FLAGS),
                mount_display_id: entry.field(fields::UNIT_FIELD_MOUNTDISPLAYID),
                bytes_1: entry.field(fields::UNIT_FIELD_BYTES_1),
                npc_flags: entry.field(fields::UNIT_NPC_FLAGS),
                dynamic_flags: entry.field(fields::UNIT_DYNAMIC_FLAGS),
                target: entry.self_guid(fields::UNIT_FIELD_TARGET).filter(|&guid| guid != 0),
                player_bytes: entry.field(fields::PLAYER_BYTES),
                player_bytes_2: entry.field(fields::PLAYER_BYTES_2),
                position: entry.movement.as_ref().and_then(|m| m.position),
                move_flags: entry.movement.as_ref().map(|m| m.move_flags),
                generation: entry.generation,
                player_fields: (entry.type_id == benilla_protocol_wrath::updates::type_id::PLAYER)
                    .then(|| (0..1171).map(|i| entry.field(i).unwrap_or(0)).collect()),
                item_fields: matches!(
                    entry.type_id,
                    benilla_protocol_wrath::updates::type_id::ITEM
                        | benilla_protocol_wrath::updates::type_id::CONTAINER
                )
                .then(|| (0..138).map(|i| entry.field(i).unwrap_or(0)).collect()),
                auras: entry.auras.clone(),
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

    #[test]
    fn terminal_scope_loss_clears_ecs_and_input_without_a_digest() {
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        world.init_resource::<SelfGuid>();
        world.init_resource::<WrathWorldView>();
        world.init_resource::<crate::player::Player>();
        world.init_resource::<crate::player::OwnControlLost>();
        world.init_resource::<crate::player::StandPending>();
        let outbox: WrathOutbox = Arc::new(Mutex::new(VecDeque::from([WrathOutbound {
            opcode: 1,
            payload: vec![1],
            movement: false,
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
        assert!(outbox.lock().unwrap().is_empty(), "pending actions cleared");
    }
}













#[cfg(test)]
mod command_tap_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn view(npc: u64) -> WrathWorldView {
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
                npc_flags: None,
                dynamic_flags: None,
                target: None,
                player_bytes: None,
                player_bytes_2: None,
                position: None,
                move_flags: None,
                generation: 1,
                player_fields: None,
                item_fields: None,
                auras: Vec::new(),
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
                left_world: false,
                shutdown: false,
                last_error: None,
                session_events: None,
                session_events_tx: None,
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
