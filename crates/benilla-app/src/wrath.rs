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

use std::collections::VecDeque;
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
pub type WrathLatest = Arc<Mutex<Option<SelfObservation>>>;

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

    fn publish_snapshot(&self, snapshot: SelfObservation) {
        if let Ok(mut slot) = self.latest.lock() {
            *slot = Some(snapshot);
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
}

/// A lab session configuration for the IO thread. Credentials come from the owner's
/// environment/launcher, never from committed files (system review §8).
pub struct WrathSessionConfig {
    pub account: String,
    pub password: String,
    pub auth_addr: String,
    pub world_addr: String,
    /// WM bridge token; absent = the bridge stays disabled for this session.
    pub wm_token: Option<String>,
    pub dwell: Duration,
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
                    self.feed.publish_snapshot(snapshot.clone());
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

// Small helper kept local so the module stays self-contained.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}


