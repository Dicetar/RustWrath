//! `wrath-world <account> <password> [seconds]` — the renderless lab session:
//! logon, world auth (AUTH_OK 0x0C), startup-burst dispatch, char enum/create,
//! player login, bounded dwell with ping/pong + time-sync, logout request and
//! completion. Phase-aware per Astra F02; exits nonzero on any missing milestone.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::session::{SelfTarget, SessionState};
use benilla_protocol_wrath::world::{is_idle, WorldConnection};
use benilla_wm_native::{
    pump_once, Error as WmError, ProducerSession, ProducerSnapshot, PublisherConfig,
    PublisherRetirement, PumpError, PumpResult,
};

/// The driver's adapter: implements the bridge's `ProducerSession` over the pure
/// `SessionState`, converting export errors into the pump's error type. No bridge dependency
/// reaches the producer or the packet decoder.
struct ProducerAdapter<'a> {
    session: &'a mut SessionState,
}

/// Owned snapshot handed to the pump, implementing the bridge's `ProducerSnapshot` trait.
struct SnapshotOwned(benilla_protocol_wrath::session::SelfObservation);

impl std::fmt::Debug for SnapshotOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "seq {} rev {} map {} pos ({:.1}, {:.1}, {:.1}, {:.1}) target {:?} visible {}/{} complete={} age_ms {}",
            self.0.sequence,
            self.0.state_revision,
            self.0.map_id,
            self.0.position.x,
            self.0.position.y,
            self.0.position.z,
            self.0.position.o,
            match &self.0.target {
                SelfTarget::Unknown => "unknown".to_string(),
                SelfTarget::None => "none".to_string(),
                SelfTarget::Target(g) => format!("{g}"),
            },
            self.0.visibility.guids.len(),
            self.0.visibility.total_guid_count,
            self.0.visibility.complete,
            format!("position_observed_at={}", self.0.position_observed_at_unix_ms),
        )
    }
}

impl ProducerSnapshot for SnapshotOwned {
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
            SelfTarget::Unknown => benilla_wm_native::TargetObservation::Unknown,
            SelfTarget::None => benilla_wm_native::TargetObservation::None,
            SelfTarget::Target(g) => benilla_wm_native::TargetObservation::target(*g)?,
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

impl ProducerSession for ProducerAdapter<'_> {
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
                let owned = SnapshotOwned(snapshot);
                println!("[wm] export {owned:?}");
                Ok(Some(owned))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(benilla_wm_native::ProducerExportError::new(e.to_string())),
        }
    }

    fn mark_published(&mut self) {
        self.session.mark_published();
    }
}

/// Driver-owned clocks (the producer stays time-free): wall-clock for observations and
/// monotonic session uptime.
fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const CMSG_CHAR_CREATE: u32 = 0x036;
const CMSG_TIME_SYNC_RESP: u32 = 0x391;

fn name_of(op: u16) -> String {
    match op {
        0x1EE => "SMSG_AUTH_RESPONSE".into(),
        0x2EF => "SMSG_ADDON_INFO".into(),
        0x03B => "SMSG_CHAR_ENUM".into(),
        0x03A => "SMSG_CHAR_CREATE".into(),
        0x236 => "SMSG_LOGIN_VERIFY_WORLD".into(),
        0x042 => "SMSG_LOGIN_SETTIMESPEED".into(),
        0x155 => "SMSG_BINDPOINTUPDATE".into(),
        0x0A9 => "SMSG_UPDATE_OBJECT".into(),
        0x1DD => "SMSG_PONG".into(),
        0x04D => "SMSG_LOGOUT_COMPLETE".into(),
        0x390 => "SMSG_TIME_SYNC_REQ".into(),
        0x51E => "SMSG_MULTIPLE_MOVES".into(),
        other => format!("{other:#06x}"),
    }
}

/// Bounded cursor over a packet payload: every read is checked (Astra F06).
struct Cur<'a> {
    p: &'a [u8],
    at: usize,
}

impl<'a> Cur<'a> {
    fn u8(&mut self) -> Result<u8> {
        let v = *self.p.get(self.at).ok_or_else(|| anyhow!("payload cut short"))?;
        self.at += 1;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32> {
        let s = self.p.get(self.at..self.at + 4).ok_or_else(|| anyhow!("payload cut short"))?;
        self.at += 4;
        Ok(u32::from_le_bytes(s.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        let s = self.p.get(self.at..self.at + 8).ok_or_else(|| anyhow!("payload cut short"))?;
        self.at += 8;
        Ok(u64::from_le_bytes(s.try_into().unwrap()))
    }
    fn cstr(&mut self) -> Result<String> {
        let start = self.at;
        loop {
            match self.p.get(self.at) {
                None => bail!("unterminated string in payload"),
                Some(0) => break,
                Some(_) => self.at += 1,
            }
        }
        let s = String::from_utf8_lossy(&self.p[start..self.at]).into_owned();
        self.at += 1;
        Ok(s)
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let account = args.next().context("account")?;
    let password = args.next().context("password")?;
    let dwell: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(30);
    let record_path = args.next();

    let logon = logon("127.0.0.1:3724", &account, &password).context("logon")?;
    let realm = logon
        .realms
        .first()
        .ok_or_else(|| anyhow!("realm list is empty"))?;
    println!("[auth] ok; realm `{}` id {}", realm.name, realm.realm_id);

    let (mut world, seed) = WorldConnection::connect("127.0.0.1:8095")?;
    if record_path.is_some() {
        world.enable_recording();
    }
    println!("[world] challenge seed {seed:02x?}");
    world.auth_session(&logon.account, &logon.session_key, &seed, realm.realm_id as u32)?;
    world.stream_mut().set_read_timeout(Some(Duration::from_millis(500)))?;
    println!("[world] AUTH_SESSION sent; crypto armed");

    // Dispatch: wait for AUTH_RESPONSE (F01: success is AUTH_OK 0x0C), tolerating the startup
    // burst around it (F02). Any idle gap is retried under a deadline; a partial read is fatal.
    let mut authed = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !authed {
        if Instant::now() > deadline {
            bail!("timeout waiting for SMSG_AUTH_RESPONSE");
        }
        match world.recv() {
            Ok(p) if p.opcode == opcode::SMSG_AUTH_RESPONSE => {
                let code = *p.payload.first().context("empty AUTH_RESPONSE")?;
                if code != opcode::AUTH_OK {
                    bail!("world auth rejected: code {code:#04x}");
                }
                println!("[world] AUTH_RESPONSE: AUTH_OK");
                authed = true;
            }
            Ok(p) => println!("[startup] {} ({} bytes)", name_of(p.opcode), p.payload.len()),
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }

    // Character enumeration, waiting for the enum answer amid any remaining burst (F02).
    world.send(opcode::CMSG_CHAR_ENUM, &[])?;
    let mut guid = None;
    let mut got_enum = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !got_enum {
        if Instant::now() > deadline {
            bail!("timeout waiting for SMSG_CHAR_ENUM");
        }
        match world.recv() {
            Ok(p) if p.opcode == opcode::SMSG_CHAR_ENUM => {
                let mut cur = Cur { p: &p.payload, at: 0 };
                let count = cur.u8()?;
                println!("[chars] {count} on the account");
                if count > 0 {
                    let g = cur.u64()?;
                    let name = cur.cstr()?;
                    let race = cur.u8()?;
                    let class = cur.u8()?;
                    let _gender = cur.u8()?;
                    for _ in 0..5 {
                        cur.u8()?; // skin, face, hair style, hair color, facial hair
                    }
                    let level = cur.u8()?;
                    let zone = cur.u32()?;
                    let map = cur.u32()?;
                    println!("[chars] `{name}` guid {g} race {race} class {class} level {level} zone {zone} map {map}");
                    guid = Some(g);
                }
                got_enum = true;
            }
            Ok(p) => println!("[startup] {} ({} bytes)", name_of(p.opcode), p.payload.len()),
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }

    // Create exactly one disposable character when the account is empty (F06: explicit identity).
    if guid.is_none() {
        let mut p = b"Projectbee\0".to_vec();
        p.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0, 0, 0]); // human warrior male, defaults
        world.send(CMSG_CHAR_CREATE, &p)?;
        println!("[chars] create sent; waiting for the burst to settle");
        // The authoritative create evidence is the next enum, not a specific response opcode.
        let settle = Instant::now() + Duration::from_secs(5);
        while Instant::now() < settle {
            match world.recv() {
                Ok(_) => {}
                Err(e) if is_idle(&e) => {}
                Err(e) => return Err(e),
            }
        }
        world.send(opcode::CMSG_CHAR_ENUM, &[])?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut created_guid = None;
        loop {
            if Instant::now() > deadline {
                bail!("timeout waiting for post-create SMSG_CHAR_ENUM");
            }
            match world.recv() {
                Ok(p) if p.opcode == opcode::SMSG_CHAR_ENUM => {
                    let mut cur = Cur { p: &p.payload, at: 0 };
                    let count = cur.u8()?;
                    if count == 0 {
                        bail!("create produced no character (enum still empty)");
                    }
                    let g = cur.u64()?;
                    let name = cur.cstr()?;
                    println!("[chars] created `{name}` guid {g}");
                    created_guid = Some(g);
                    break;
                }
                Ok(p) => println!("[startup] {} ({} bytes)", name_of(p.opcode), p.payload.len()),
                Err(e) if is_idle(&e) => {}
                Err(e) => return Err(e),
            }
        }
        guid = created_guid;
    }
    let guid = guid.context("no character to log in")?;

    // --- WM observation producer (convergence task): session-owned replicated state ---
    // The producer reduces every post-login packet; export happens on state change in the
    // dwell loop below. Clocks are driver-owned; the producer is time-free.
    let session_start = Instant::now();
    let mut session = SessionState::new();
    session.start_session(guid);
    let mut exports: u64 = 0;

    // --- WM bridge: enabled only when the owner provides the receiver token; disabled means
    // no publisher, no socket, no traffic — baseline login is untouched.
    let wm_config = match std::env::var("WM_NATIVE_OBSERVATION_TOKEN") {
        Ok(token) => {
            let loopback = benilla_wm_native::LoopbackConfig::new(
                std::net::SocketAddr::from(([127, 0, 0, 1], benilla_wm_native::DEFAULT_PORT)),
                benilla_wm_native::AuthToken::new(token)?,
            )?;
            Some(PublisherConfig::new(loopback))
        }
        Err(_) => {
            println!("[wm] bridge disabled: WM_NATIVE_OBSERVATION_TOKEN not set");
            None
        }
    };
    let mut wm_publisher = match &wm_config {
        Some(config) => Some(benilla_wm_native::ObservationPublisher::spawn(config.clone())?),
        None => None,
    };
    let mut wm_retiring: Vec<PublisherRetirement> = Vec::new();

    /// Retires the current publisher and orders its replacement after the old worker fully
    /// stops (`spawn_after`) — replacement ordering without blocking the packet path.
    fn wm_replace(wm_config: Option<&PublisherConfig>, current: &mut Option<benilla_wm_native::ObservationPublisher>, retiring: &mut Vec<PublisherRetirement>) -> Result<()> {
        if let Some(config) = wm_config {
            if let Some(publisher) = current.take() {
                retiring.push(publisher.retire());
            }
            let replacement = match retiring.last() {
                Some(retirement) => benilla_wm_native::ObservationPublisher::spawn_after(config.clone(), retirement)?,
                None => benilla_wm_native::ObservationPublisher::spawn(config.clone())?,
            };
            *current = Some(replacement);
            // Finished retirements are droppable (Drop detaches an already-stopped worker).
            retiring.retain(|r| !r.is_finished());
        }
        Ok(())
    }

    world.send(opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;
    session.note_player_login();
    println!("[world] PLAYER_LOGIN guid {guid}");

    let deadline = Instant::now() + Duration::from_secs(dwell);
    let mut ping_seq = 1u32;
    let mut last_ping = Instant::now() - Duration::from_secs(9);
    let mut logged_in = false;
    let mut saw_update = false;
    while Instant::now() < deadline {
        if last_ping.elapsed() >= Duration::from_secs(10) {
            let mut ping = ping_seq.to_le_bytes().to_vec();
            ping.extend_from_slice(&0u32.to_le_bytes());
            world.send(opcode::CMSG_PING, &ping)?;
            ping_seq += 1;
            last_ping = Instant::now();
        }
        match world.recv() {
            Ok(p) => {
                let arrival = unix_now_ms();
                session.reduce(p.opcode, &p.payload, arrival)?;
                match p.opcode {
                    0x236 => logged_in = true,
                    0x0A9 => saw_update = true,
                    0x390 => {
                        let mut cur = Cur { p: &p.payload, at: 0 };
                        let counter = cur.u32()?;
                        let mut resp = counter.to_le_bytes().to_vec();
                        resp.extend_from_slice(&0u32.to_le_bytes());
                        world.send(CMSG_TIME_SYNC_RESP, &resp)?;
                    }
                    _ => {}
                }
                if std::env::var("WRATH_DEBUG").is_ok() {
                    println!("[dwell] {} ({} bytes)", name_of(p.opcode), p.payload.len());
                }
            }
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
        // Lifecycle: a map transition retires the WM publisher and orders its replacement
        // after the old worker fully stops (retire/spawn_after — no blocking on this path).
        // Near teleport: acknowledge the server's order counter (packed GUID, u32 flags,
        // u32 time — both read and unused server-side, sent after landing).
        for counter in session.take_pending_teleport_acks() {
            let mut ack = Vec::new();
            let mut mask = 0u8;
            let mut guid_bytes = Vec::new();
            for i in 0..8 {
                let b = (guid >> (i * 8)) as u8;
                if b != 0 { mask |= 1 << i; guid_bytes.push(b); }
            }
            ack.push(mask);
            ack.extend_from_slice(&guid_bytes);
            ack.extend_from_slice(&0u32.to_le_bytes()); // flags (unused by the server)
            ack.extend_from_slice(&0u32.to_le_bytes()); // time (unused by the server)
            world.send(opcode::MSG_MOVE_TELEPORT_ACK as u32, &ack)?;
            println!("[world] teleport ack sent (counter {counter})");
        }
        // Far transfer: the NEW_WORLD was processed (old scope discarded, destination
        // recorded); perform the driver-side teardown/loading boundary and acknowledge. The
        // server processes this ack before sending any new-world traffic, so from here the
        // reducing stream is the new world.
        if session.phase() == benilla_protocol_wrath::session::Phase::Loading {
            world.send(opcode::CMSG_WORLD_MOVE_WORLDPORT_ACK, &[])?;
            session.worldport_acknowledged();
            println!("[world] worldport acknowledged; new world scope live");
        }
        for event in session.take_lifecycle() {
            println!("[wm] lifecycle {event:?}: retiring publisher, ordering replacement");
            wm_replace(wm_config.as_ref(), &mut wm_publisher, &mut wm_retiring)?;
        }
        wm_retiring.retain(|r| !r.is_finished());

        // One bounded retry/export/handoff step per tick — runs on idle ticks too, so a
        // retained snapshot retries without waiting for new source traffic.
        if let Some(publisher) = wm_publisher.as_ref() {
            let mut adapter = ProducerAdapter { session: &mut session };
            match pump_once(publisher, &mut adapter, unix_now_ms(), session_start.elapsed().as_millis() as u64) {
                Ok(PumpResult::Idle) => {}
                Ok(PumpResult::HandedOff(receipt)) => {
                    exports += 1;
                    println!("[wm] handed off seq {} rev {}", receipt.sequence, receipt.state_revision);
                }
                Ok(PumpResult::Retained) => {
                    // Producer retains the snapshot; the next tick re-exports it.
                }
                Err(PumpError::Producer(e)) => {
                    println!("[wm] export unavailable: {}", e.message());
                }
                Err(PumpError::Superseded { sequence, latest_sequence }) => {
                    println!("[wm] superseded seq {sequence} by {latest_sequence} (ordering violation under a single pump)");
                }
                Err(PumpError::Conversion(e)) => {
                    println!("[wm] conversion error: {e}");
                }
                Err(PumpError::Publisher(WmError::Disconnected)) => {
                    println!("[wm] publisher disconnected: respawning with retire/spawn_after ordering");
                    wm_replace(wm_config.as_ref(), &mut wm_publisher, &mut wm_retiring)?;
                }
                Err(PumpError::Publisher(e)) => {
                    println!("[wm] publisher error: {e}");
                }
            }
        }
    }
    if !logged_in {
        bail!("dwell finished without SMSG_LOGIN_VERIFY_WORLD");
    }
    println!("[world] dwell ok: LOGIN_VERIFY_WORLD seen, object updates: {saw_update}, wm exports: {exports}");

    world.send(opcode::CMSG_LOGOUT_REQUEST, &[])?;
    println!("[world] LOGOUT_REQUEST sent");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() > deadline {
            bail!("SMSG_LOGOUT_COMPLETE never arrived");
        }
        match world.recv() {
            Ok(p) if p.opcode == 0x04D => {
                println!("[world] LOGOUT_COMPLETE — clean session end to end");
                session.finish_session();
                if let Some(publisher) = wm_publisher.take() {
                    let retirement = publisher.retire();
                    // Off the packet path: wait briefly so the receiver sees a clean close.
                    let _ = retirement.wait_timeout(std::time::Duration::from_secs(2));
                }
                for r in wm_retiring.drain(..) {
                    let _ = r.wait_timeout(std::time::Duration::from_millis(200));
                }
                if let Some(path) = record_path.as_deref() {
                    let rec = world.take_recording();
                    let mut out = (rec.len() as u32).to_le_bytes().to_vec();
                    for (op, payload) in &rec {
                        out.extend_from_slice(&op.to_le_bytes());
                        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                        out.extend_from_slice(payload);
                    }
                    std::fs::write(path, &out)?;
                    println!("[world] recorded {} packets to {}", rec.len(), path);
                }
                return Ok(());
            }
            Ok(_) => {}
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }
}
