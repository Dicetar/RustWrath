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
use benilla_protocol_wrath::gameplay::{self, ServerEvent as WrathGameplayEvent};
use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
use crate::entities::Equipment;
use crate::items::{Countdowns, ItemObject};
use crate::net::{
    CharPick, CharRequest, ClientCommand, Guid, GuidIndex, NetCommandTap, NetEntity, ObjectStore,
    ParkRequest, ParkSender, SelfGuid, SelfPlayer, UnitSpeeds, WorldEnterCascadeMessage,
    EnteredWorldMessage, FieldChanged,
};
use benilla_protocol::messages::{
    ActionButton, AttackSwingError, GossipOption as ProtocolGossipOption,
    ItemDamage as ProtocolItemDamage, ItemInfo, ItemPushResult,
    ItemSpellEntry as ProtocolItemSpell, ItemUseSpell as ProtocolItemUseSpell,
    LootItem as ProtocolLootItem, NpcTextBlock as ProtocolNpcTextBlock, QuestComplete,
    QuestDetails, QuestGiverList, QuestListEntry, QuestObjective, QuestOfferReward,
    QuestRequestItems, QuestRewardItem, QuestTemplate,
};

use benilla_protocol_wrath::session::{LifeEvent, Phase, SelfObservation, SessionState};

/// Bounded lifecycle queue depth: lifetimes of interest are short (one world transfer is one
/// event); 16 gives many frames of headroom at one-event-per-transfer rates.
pub const WRATH_EVENT_CAPACITY: usize = 16;

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
}

impl WrathFeed {
    fn new() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(WRATH_EVENT_CAPACITY))),
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

/// The select screen's asks and Enters, drained by [`wrath_pick_admission`]. Bound in
/// [`WrathNetPlugin::build`] over the stock `CharPick`, whose reader is the stock IO thread
/// the Wrath session replaces.
#[derive(Resource)]
pub struct WrathPickQueue(pub(crate) Receiver<ParkRequest>);

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
    /// The running parked session's screen-ask channel. The select screen's next ask lands
    /// here; a dead one (the parked session ended) makes the ask a fresh-admission attempt.
    /// `None` for the boot flow and for any session started without the screen, which is also
    /// how the end-of-session retry policy tells a connection that served a screen from a boot
    /// dial: a parked link that dies does not retry.
    park_commands: Option<Sender<WrathParkedCommand>>,
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
        let Some(config) = self.config.clone() else { return };
        if self.shutdown || self.worker.is_some() || self.retiring.is_some() {
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
        let (parked_in, parked_asked) = match kind {
            WrathSessionStart::Auto => {
                self.park_commands = None;
                (None, false)
            }
            WrathSessionStart::Parked { roster_asked } => {
                let (park_tx, park_rx) = crossbeam_channel::unbounded::<WrathParkedCommand>();
                self.park_commands = Some(park_tx);
                (Some(park_rx), roster_asked)
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
                    parked_in,
                    parked_asked,
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
            next_cast_count: 1,
            empty: (),
        }
    }
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
            park_commands: None,
        })
        .init_resource::<WrathWorldView>();
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
    if control.worker.as_ref().is_some_and(|w| w.is_finished()) {
        if let Some(handle) = control.worker.take() {
            if handle.join().is_err() && control.last_error.is_none() {
                control.last_error = Some("wrath session worker panicked".to_owned());
            }
        }
        if !control.ended {
            control.ended = true;
            // The terminal feed and its reply queue are drained in Net; only then may a
            // replacement be admitted. If Net already drained `SessionEnded`, leave its ready
            // flag alone.
            control.terminal_ready = false;
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

/// Map a typed 3.3.5 reply onto the established 1.12 event seam. Fields with no 3.3.5 equivalent
/// are genuinely empty; nothing is invented from local state.
fn translate_gameplay(event: WrathGameplayEvent) -> Vec<SessionEvent> {
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
        WrathGameplayEvent::CastFailed {
            spell_id,
            reason,
            arg,
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
        } => vec![SessionEvent::SpellStart {
            caster,
            spell_id,
            cast_flags: cast_flags as u16,
            cast_time_ms: cast_time_ms.max(0) as u32,
            target: None,
            ammo_display_id: None,
        }],
        WrathGameplayEvent::LogoutResponse { reason, instant } => {
            vec![SessionEvent::LogoutResponse { reason, instant }]
        }
        WrathGameplayEvent::LoggedOut => vec![SessionEvent::LoggedOut],
        WrathGameplayEvent::LogoutCancelled => vec![SessionEvent::LogoutCancelled],
        WrathGameplayEvent::SpellFailure { caster, spell_id } => {
            vec![SessionEvent::SpellFailedOther { caster, spell_id }]
        }
        WrathGameplayEvent::SpellGo {
            caster,
            spell_id,
            cast_flags,
            hits,
            misses,
            target_mask,
            item_caster,
        } => vec![SessionEvent::SpellGo {
            caster,
            spell_id,
            cast_flags: cast_flags as u16,
            target: hits.first().copied(),
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
            go_target: None,
            target_mask,
            source: None,
            dest: None,
            ammo_display_id: None,
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
                    gameplay::cast_spell(count, spell_id, target),
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
            ClientCommand::SetActionButton { button, packed } => Some((
                0x128, // CMSG_SET_ACTION_BUTTON
                gameplay::set_action_button(button, packed),
            )),
            _ => None,
        };
        if let Some((opcode, payload)) = packet {
            if let Ok(mut queue) = input.0.lock() {
                if queue.len() < 256 {
                    queue.push_back(WrathOutbound { opcode, payload });
                }
            }
        }
    }
}

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
        crate::net::handlers::dispatch(world, deliver);
    }
}

fn wrath_net_drain(mut control: ResMut<WrathControl>, mut view: ResMut<WrathWorldView>) {
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
                view.entities.clear();
                view.world_generation = world_generation;
                view.map_id = map_id;
                view.self_observation = None;
                view.player_guid = 0;
                view.entered_world = false;
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
                view.entities.clear();
                view.self_observation = None;
                view.player_guid = 0;
                view.entered_world = false;
                view.gossip_menu = None;
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

    // A worker can exit after its final event was already drained (or panicked without one). The
    // exposed view is invalid on that terminal edge even when no digest follows.
    if control.ended {
        view.entities.clear();
        view.self_observation = None;
        view.player_guid = 0;
        view.entered_world = false;
        view.gossip_menu = None;
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
        // A replacement session restarts its epoch, world generation and revision counters.
        // Adopt them as a new scope rather than comparing them with the retired session's
        // monotonic values.
        view.entities.clear();
        view.last_changes.world_transferred = true;
        view.state_revision = 0;
    } else {
        if digest.world_generation < view.world_generation {
            return; // stale world scope
        }
        if digest.world_generation != view.world_generation {
            view.entities.clear();
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
                world.entity_mut(entity).insert((
                    NetEntity {
                        kind,
                        display_id: remote.display_id,
                        scale: remote.scale.unwrap_or(1.0),
                    },
                    ObjectStore(fields),
                ));
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
                    let local_flags = world
                        .get_resource::<crate::player::Player>()
                        .map_or(0, crate::player::Player::move_flags);
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
const W_FLAG_FALLING: u32 = 0x2000;
const W_FLAG_SWIMMING: u32 = 0x200_0000;

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
    let mut flags = player.move_flags();
    // 1.12 places swimming at 0x200000 and transport at 0x2000000; 3.3.5 swaps those meanings.
    if flags & 0x20_0000 != 0 {
        flags = (flags & !0x20_0000) | W_FLAG_SWIMMING;
    }
    if flags & 0x0200_0000 != 0 {
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
        if queue.len() >= 64 {
            queue.pop_front();
        }
        queue.push_back(WrathOutbound { opcode, payload });
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
/// error, signals retirement and waits only for the bounded publisher stop window; a replacement
/// worker can then start without overlapping publication.
struct PublisherOwner {
    publisher: Option<benilla_wm_native::ObservationPublisher>,
    retiring: Vec<benilla_wm_native::PublisherRetirement>,
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
            let _ = retirement.wait_timeout(Duration::from_millis(200));
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
/// established roster seam expects, and enters on the ask the screen made. `roster_asked`
/// marks the enumeration that started the session; the boot flow publishes no roster.
pub fn run_session_with_cancel(
    feed: &WrathFeed,
    config: &WrathSessionConfig,
    cancel: &AtomicBool,
    commands: Option<WrathOutbox>,
    events_out: Option<Sender<WrathSessionEvent>>,
    parked: Option<Receiver<WrathParkedCommand>>,
    roster_asked: bool,
) -> anyhow::Result<()> {
    use benilla_wm_native::{pump_once, replace_publisher, PublisherConfig};

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
            Some(PublisherConfig::new(loopback))
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
                                    for event in translate_gameplay(event) {
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
                            if let Some(events_out) = events_out.as_ref() {
                                if session.player_guid().is_some() {
                                    for event in translate_gameplay(event) {
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
        entities.insert(
            *guid,
            WrathEntity {
                guid: *guid,
                type_id: entry.type_id,
                entry: entry.field(fields::OBJECT_FIELD_ENTRY),
                display_id: entry.field(fields::UNIT_FIELD_DISPLAYID),
                scale: entry.field(fields::OBJECT_FIELD_SCALE_X).map(f32::from_bits),
                health: entry.field(fields::UNIT_FIELD_HEALTH),
                max_health: entry.field(fields::UNIT_FIELD_MAXHEALTH),
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
                park_commands: Some(tx),
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
