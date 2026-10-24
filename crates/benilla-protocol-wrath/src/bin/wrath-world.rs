//! `wrath-world <account> <password> [seconds]` — the renderless lab session:
//! logon, world auth (AUTH_OK 0x0C), startup-burst dispatch, char enum/create,
//! player login, bounded dwell with ping/pong + time-sync, logout request and
//! completion. Phase-aware per Astra F02; exits nonzero on any missing milestone.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::world::{is_idle, WorldConnection};

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
    world.send(opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;
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
    }
    if !logged_in {
        bail!("dwell finished without SMSG_LOGIN_VERIFY_WORLD");
    }
    println!("[world] dwell ok: LOGIN_VERIFY_WORLD seen, object updates: {saw_update}");

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
