//! The Wrath (3.3.5a) session feed into the graphical app: the bridge between the
//! `benilla-protocol-wrath` session/producer and this app's established stage pipeline.
//!
//! Scheduling contract (Astra R5, attached to issue 22 — this module implements the feed half):
//!
//! - The session loop (logon → world → reduce → producer → pump) runs on its own IO thread,
//!   exactly like the 1.12 `net::io` thread — never in a Bevy `Update` system. The renderless
//!   driver's blocking `recv` loop stays on that thread.
//! - The thread publishes **latest-only** self snapshots through a single-slot handoff
//!   (`WrathLatest`) and **bounded** lifecycle/phase events through a queue
//!   (`WrathEvents`, capacity [`WRATH_EVENT_CAPACITY`], oldest dropped). A full slot is never a
//!   blocked packet loop: observations are latest-only by contract, and a dropped lifecycle
//!   event is recoverable because the newest full state always follows.
//! - The app drains both in the existing `WorldStage::Net` stage — before `Input` reads
//!   position — with a per-frame drain bound, so bursts cannot starve input and no-packet frames
//!   do no work. World-generation changes invalidate the mapped entity set and the pending
//!   publication state **in the same Net tick** they are drained, before input/control.
//! - Server authority stays authoritative: this feed carries replicated state and lifecycle
//!   only. Prediction (issue 23) reconciles against it and never manufactures outcomes.
//!
//! Entity translation into the existing `GuidIndex`/`ObjectFields` path is issue 22's remaining
//! work and is deliberately **not** stubbed here: the feed exposes typed snapshots, the
//! translation consumes them at the named stage.

use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use benilla_world::schedule::WorldStage;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use benilla_protocol_wrath::session::{LifeEvent, Phase, SelfObservation, SessionState};

/// Bounded lifecycle queue depth: lifetimes of interest are short (one world transfer is one
/// event); 16 gives many frames of headroom at one-event-per-transfer rates.
pub const WRATH_EVENT_CAPACITY: usize = 16;

/// Per-frame drain bound for the Net-stage system: a burst longer than this leaves the rest
/// for the next frame instead of starving input (R5 budget; measured, not tuned, per frame).
pub const WRATH_DRAIN_LIMIT: usize = 64;

/// Latest-only self snapshot slot: the IO thread overwrites, the Net stage reads and clears.
pub type WrathLatest = Arc<Mutex<Option<WorldDigest>>>;

/// Bounded lifecycle/phase event queue, shared IO-thread → Net stage.
pub type WrathEvents = Arc<Mutex<VecDeque<WrathFeedEvent>>>;

/// What the IO thread publishes.
#[derive(Clone, Debug, PartialEq)]
pub enum WrathFeedEvent {
    /// The replicated session phase changed (login, transfer boundary, logout).
    Phase(Phase),
    /// A world scope changed: the mapped entity set and pending publications for the old
    /// scope are invalid from this event on (issue 22 clears its entity map here).
    WorldTransfer { world_generation: u64, map_id: u32 },
    /// A producer export was refused with a reason — diagnostics for the debug panel.
    ExportSkipped(String),
    /// The IO-thread session ended (Ok = clean logout; Err = failure reason).
    SessionEnded(Result<(), String>),
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
    /// Monotonic client uptime origin, stamped by the IO thread at session start.
    pub started: std::time::Instant,
}

impl WrathFeed {
    fn new() -> Self {
        Self {
            latest: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(WRATH_EVENT_CAPACITY))),
            started: std::time::Instant::now(),
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
/// environment/launcher, never from committed files (system review §8).
#[derive(Clone)]
pub struct WrathSessionConfig {
    pub account: String,
    pub password: String,
    pub auth_addr: String,
    pub world_addr: String,
    /// WM bridge token; absent = the bridge stays disabled for this session.
    pub wm_token: Option<String>,
    pub dwell: Duration,
}

/// One replicated entity, as the display path will consume it. Full GUID, world scope and
/// reducer object generation are preserved; ECS entity ids are assigned by the translation
/// (issue 22) and never enter this type.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathEntity {
    pub guid: u64,
    pub type_id: u8,
    /// `UNIT_FIELD_DISPLAYID` — the model the renderer needs (asset mapping is issue 22).
    pub display_id: Option<u32>,
    pub scale: Option<u32>,
    pub health: Option<u32>,
    pub level: Option<u32>,
    pub position: Option<benilla_protocol_wrath::updates::PositionSnapshot>,
    pub generation: u32,
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

/// Application selection/configuration and worker ownership. Inserted by
/// [`WrathNetPlugin`]; a `None` config keeps the plugin inert (baseline login untouched).
#[derive(Resource)]
pub struct WrathControl {
    pub config: Option<WrathSessionConfig>,
    worker: Option<std::thread::JoinHandle<()>>,
    feed: Option<WrathFeed>,
    /// Terminal failure from the last worker run (disconnect, auth failure, logout).
    pub last_error: Option<String>,
}

impl WrathControl {
    /// Whether a worker is currently running.
    pub fn running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    fn start(&mut self) {
        let Some(config) = self.config.clone() else { return };
        if self.running() {
            return;
        }
        let feed = WrathFeed::new();
        self.feed = Some(feed.clone());
        self.last_error = None;
        let handle = std::thread::Builder::new()
            .name("wrath-session".to_owned())
            .spawn(move || {
                let result = run_session(&feed, &config);
                if let Ok(mut q) = feed.events.lock() {
                    if q.len() >= WRATH_EVENT_CAPACITY {
                        q.pop_front();
                    }
                    q.push_back(WrathFeedEvent::SessionEnded(
                        result.map_err(|e| format!("{e:#}")),
                    ));
                }
            })
            .expect("spawn wrath session thread");
        self.worker = Some(handle);
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
        app.insert_resource(WrathControl {
            config: self.config.clone(),
            worker: None,
            feed: None,
            last_error: None,
        })
        .init_resource::<WrathWorldView>();
        if self.config.is_some() {
            // Session ownership: the worker starts on the first ownership tick; terminal
            // failure surfaces through `WrathControl::last_error` with the exposed state
            // invalidated in the same drain. Deliberate stop is a control decision (issue 22).
            app.add_systems(PreUpdate, wrath_session_ownership);
            app.add_systems(Update, wrath_net_drain.in_set(WorldStage::Net));
        }
    }
}

/// Worker ownership: starts the session once; surfaces terminal failure and invalidates.
fn wrath_session_ownership(mut control: ResMut<WrathControl>) {
    if control.worker.is_none() && control.last_error.is_none() && control.config.is_some() {
        control.start();
    }
    if let Some(worker) = control.worker.as_ref() {
        if worker.is_finished() {
            if let Some(handle) = control.worker.take() {
                let _ = handle.join();
            }
            control.last_error = Some("wrath session worker exited".to_owned());
            if let Some(feed) = control.feed.take() {
                if let Ok(mut slot) = feed.latest.lock() {
                    *slot = None;
                }
            }
        }
    }
}

/// The Net-stage drain: bounded, ordered, lifecycle-invalidating (R5).
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
                view.last_changes.world_transferred = true;
            }
            WrathFeedEvent::ExportSkipped(reason) => {
                debug!("wrath export skipped: {reason}");
            }
            WrathFeedEvent::SessionEnded(result) => match result {
                Ok(()) => info!("wrath session ended cleanly"),
                Err(e) => {
                    warn!("wrath session failed: {e}");
                    control.last_error = Some(e);
                    view.entities.clear();
                    view.self_observation = None;
                    view.last_changes.world_transferred = true;
                }
            },
        }
    }

    let Some(digest) = control.take_digest() else { return };
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

/// Run one renderless Wrath session on the calling thread (the IO thread), publishing into the
/// feed. This is the renderless driver's loop, retargeted at the feed instead of stdout. It
/// returns when the dwell elapses or the session fails; the app restarts it as needed.
pub fn run_session(feed: &WrathFeed, config: &WrathSessionConfig) -> anyhow::Result<()> {
    use benilla_wm_native::{pump_once, ProducerSession, PublisherConfig};

    let logon = benilla_protocol_wrath::logon::logon(&config.auth_addr, &config.account, &config.password)?;
    let realm = logon
        .realms
        .first()
        .ok_or_else(|| anyhow::anyhow!("realm list is empty"))?
        .clone();

    let mut publisher: Option<benilla_wm_native::ObservationPublisher> = match &config.wm_token {
        Some(token) => {
            let loopback = benilla_wm_native::LoopbackConfig::new(
                std::net::SocketAddr::from(([127, 0, 0, 1], benilla_wm_native::DEFAULT_PORT)),
                benilla_wm_native::AuthToken::new(token.clone())?,
            )?;
            Some(benilla_wm_native::ObservationPublisher::spawn(PublisherConfig::new(loopback))?)
        }
        None => None,
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
        feed: &'a WrathFeed,
        session: &'a mut SessionState,
    }
    impl benilla_wm_native::ProducerSession for Adapter<'_> {
        type Snapshot = SnapshotOwned;
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
    // out of scope here — the graphical path logs into an existing character (issue 23's
    // driver owns creation). Enumerate and pick the first disposable character.
    world.send(benilla_protocol_wrath::opcode::CMSG_CHAR_ENUM, &[])?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut guid = None;
    while guid.is_none() {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timeout waiting for SMSG_CHAR_ENUM");
        }
        match world.recv() {
            Ok(p) if p.opcode == benilla_protocol_wrath::opcode::SMSG_CHAR_ENUM => {
                if p.payload.first().copied().unwrap_or(0) > 0 {
                    guid = Some(u64::from_le_bytes(
                        p.payload[1..9].try_into().expect("char enum guid"),
                    ));
                } else {
                    anyhow::bail!("no disposable character on the lab account; create one with the renderless driver first");
                }
            }
            Ok(_) => {}
            Err(e) if benilla_protocol_wrath::world::is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }
    let guid = guid.expect("guid");
    session.start_session(guid); // re-begin with the real full GUID; phase resets to selection
    session.note_player_login();
    feed.push_event(WrathFeedEvent::Phase(session.phase()));
    world.send(benilla_protocol_wrath::opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;

    let deadline = std::time::Instant::now() + config.dwell;
    let mut last_ping = std::time::Instant::now();
    let mut ping_seq = 1u32;
    let mut last_digest_revision = session.state_revision();
    feed.publish_digest(build_digest(&session)); // initial state (character selection)
    while std::time::Instant::now() < deadline {
        if last_ping.elapsed() >= Duration::from_secs(10) {
            let mut ping = ping_seq.to_le_bytes().to_vec();
            ping.extend_from_slice(&0u32.to_le_bytes());
            world.send(benilla_protocol_wrath::opcode::CMSG_PING, &ping)?;
            ping_seq += 1;
            last_ping = std::time::Instant::now();
        }
        match world.recv() {
            Ok(p) => {
                session.reduce(p.opcode, &p.payload, now_ms())?;
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
        for event in session.take_lifecycle() {
            feed.push_event(WrathFeedEvent::from_session(event));
        }
        // Near teleport: acknowledge the server's order counter after landing (packed GUID +
        // u32 flags + u32 time — both read and unused server-side).
        for counter in session.take_pending_teleport_acks() {
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
        // Publish a full-world digest whenever source state advanced — independent of the WM
        // bridge, so the graphical client receives state with WM disabled/busy/reconnecting.
        if session.state_revision() != last_digest_revision {
            last_digest_revision = session.state_revision();
            feed.publish_digest(build_digest(&session));
        }
        let phase_before = session.phase();
        let outcome = publisher.as_mut().map(|publisher| {
            let mut adapter = Adapter { feed, session: &mut session };
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
                // The publisher signalled shutdown: retire it; the next session spawns a
                // fresh one. A live respawn belongs to issue 22's owner with its own
                // retirement ordering.
                if let Some(p) = publisher.take() {
                    let retirement = p.retire();
                    let _ = retirement.wait_timeout(Duration::from_secs(1));
                }
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
                if let Some(publisher) = publisher.take() {
                    publisher.retire();
                }
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
                display_id: entry.field(fields::UNIT_FIELD_DISPLAYID),
                scale: entry.field(fields::OBJECT_FIELD_SCALE_X),
                health: entry.field(fields::UNIT_FIELD_HEALTH),
                level: entry.field(fields::UNIT_FIELD_LEVEL),
                position: entry.movement.as_ref().and_then(|m| m.position),
                generation: entry.generation,
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
        self_observation: session.last_self_observation().cloned(),
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


