//! The 3.3.5a auth protocol (port 3724) against the local authserver, every shape mirrored from
//! `src/server/apps/authserver/Server/AuthSession.cpp`:
//!
//! - challenge reply (`LogonChallengeCallback`): `B[32]`, `g_len=1`, `g[1]`, `n_len=32`,
//!   `N[32]`, `salt[32]`, version-challenge `[16]`, `security_flags u8` — byte-identical to the
//!   1.12 shape benilla already speaks;
//! - proof reply (`AUTH_LOGON_PROOF_S`, AuthSession.cpp:84-92): `M2[20]`, `AccountFlags u32`,
//!   `SurveyId u32`, `LoginFlags u16` — the 2.x/3.x addition over 1.12;
//! - realm list (`RealmListCallback`): per entry `type u8`, `lock u8`, `flags u8`, name and
//!   address as null-terminated strings, `population f32`, `chars u8`, `timezone u8`,
//!   `realm_id u8`, optional version fields when the realm flags carry SPECIFYBUILD, trailer
//!   `0x0010`.
//!
//! The version-integrity `crc_hash` is twenty zero bytes: the lab runs `StrictVersionCheck = 0`
//! (`authserver.conf:164`), where `VerifyVersion` short-circuits true. A strict server refuses
//! this; that is a future oracle question, not this client's concern.

use std::io::{Read, Write};

use anyhow::{bail, Context, Result};

const GAME_NAME_WOW: u32 = 0x0057_6f57; // "WoW\0"
const PLATFORM_X86: u32 = 0x0078_3836; // "x86\0"
const OS_WINDOWS: u32 = 0x0057_696e; // "Win\0"
pub const BUILD_12340: u16 = 12340;
const LOCALE_ENUS: u32 = 0x0053_556e; // "enUS" tag as the client packs it ("nUS" + S? see below)

/// The challenge inputs the client needs for SRP6.
pub struct ChallengeReply {
    pub server_public_key: [u8; 32],
    pub generator: u8,
    pub large_safe_prime: [u8; 32],
    pub salt: [u8; 32],
}

/// One realm from the realm list.
pub struct RealmInfo {
    pub realm_type: u8,
    pub flags: u8,
    pub name: String,
    pub address: String,
    pub population: f32,
    pub characters: u8,
    pub realm_id: u8,
}

fn read_u8(r: &mut impl Read) -> std::io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u16_le(r: &mut impl Read) -> std::io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32_le(r: &mut impl Read) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_cstring(r: &mut impl Read) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let b = read_u8(r)?;
        if b == 0 {
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
        bytes.push(b);
    }
}

/// Send `CMD_AUTH_LOGON_CHALLENGE` for build 12340, Windows, enUS. `account` must be the
/// uppercased name SRP6 hashes.
pub fn write_logon_challenge(w: &mut impl Write, account: &str) -> std::io::Result<()> {
    let mut body = Vec::with_capacity(34 + account.len());
    body.extend_from_slice(&GAME_NAME_WOW.to_le_bytes());
    body.extend_from_slice(&[3, 3, 5]); // version 3.3.5
    body.extend_from_slice(&BUILD_12340.to_le_bytes());
    body.extend_from_slice(&PLATFORM_X86.to_le_bytes());
    body.extend_from_slice(&OS_WINDOWS.to_le_bytes());
    body.extend_from_slice(&LOCALE_ENUS.to_le_bytes());
    body.extend_from_slice(&120u32.to_le_bytes()); // utc_timezone_offset
    body.extend_from_slice(&[127, 0, 0, 1]); // client_ip_address
    body.push(account.len() as u8);
    body.extend_from_slice(account.as_bytes());
    let mut packet = Vec::with_capacity(4 + body.len());
    packet.push(0x00); // CMD_AUTH_LOGON_CHALLENGE
    packet.push(0x03); // protocol version
    packet.extend_from_slice(&(body.len() as u16).to_le_bytes());
    packet.extend_from_slice(&body);
    w.write_all(&packet)
}

/// Read the challenge reply; `Err` carries the server's reject code on failure.
pub fn read_challenge_reply(r: &mut impl Read) -> Result<ChallengeReply> {
    let opcode = read_u8(r)?;
    if opcode != 0x00 {
        bail!("expected CMD_AUTH_LOGON_CHALLENGE (0x00), got {opcode:#x}");
    }
    let _protocol = read_u8(r)?;
    let result = read_u8(r)?;
    if result != 0 {
        bail!("logon challenge rejected: result code {result:#04x}");
    }
    let mut server_public_key = [0u8; 32];
    r.read_exact(&mut server_public_key)?;
    let generator_len = read_u8(r)? as usize;
    let mut generator_bytes = vec![0u8; generator_len];
    r.read_exact(&mut generator_bytes)?;
    let generator = *generator_bytes.first().context("empty generator")?;
    let prime_len = read_u8(r)? as usize;
    let mut prime = vec![0u8; prime_len];
    r.read_exact(&mut prime)?;
    let large_safe_prime: [u8; 32] =
        prime.try_into().map_err(|_| anyhow::anyhow!("prime was {prime_len} bytes, expected 32"))?;
    let mut salt = [0u8; 32];
    r.read_exact(&mut salt)?;
    let mut version_challenge = [0u8; 16];
    r.read_exact(&mut version_challenge)?;
    let _security_flags = read_u8(r)?;
    Ok(ChallengeReply { server_public_key, generator, large_safe_prime, salt })
}

/// Send `CMD_AUTH_LOGON_PROOF`: `A[32]`, `M1[20]`, zero `crc_hash[20]` (non-strict lab), no
/// telemetry, no security flags.
pub fn write_logon_proof(
    w: &mut impl Write,
    client_public_key: &[u8; 32],
    client_proof: &[u8; 20],
) -> std::io::Result<()> {
    let mut packet = Vec::with_capacity(75);
    packet.push(0x01);
    packet.extend_from_slice(client_public_key);
    packet.extend_from_slice(client_proof);
    packet.extend_from_slice(&[0u8; 20]);
    packet.push(0); // number_of_telemetry_keys
    packet.push(0); // security_flags
    w.write_all(&packet)
}

/// Read the proof reply; returns the server's `M2` for verification.
pub fn read_proof_reply(r: &mut impl Read) -> Result<[u8; 20]> {
    let opcode = read_u8(r)?;
    if opcode != 0x01 {
        bail!("expected CMD_AUTH_LOGON_PROOF (0x01), got {opcode:#x}");
    }
    let error = read_u8(r)?;
    if error != 0 {
        bail!("logon proof rejected: error code {error:#04x} (0x04 = wrong password)");
    }
    let mut server_proof = [0u8; 20];
    r.read_exact(&mut server_proof)?;
    let _account_flags = read_u32_le(r)?;
    let _survey_id = read_u32_le(r)?;
    let _login_flags = read_u16_le(r)?;
    Ok(server_proof)
}

/// Send `CMD_REALM_LIST` (opcode plus a zero `u32`).
pub fn write_realm_list_request(w: &mut impl Write) -> std::io::Result<()> {
    w.write_all(&[0x10, 0, 0, 0, 0])
}

/// Read the realm list (`RealmListCallback` field order).
pub fn read_realm_list(r: &mut impl Read) -> Result<Vec<RealmInfo>> {
    let opcode = read_u8(r)?;
    if opcode != 0x10 {
        bail!("expected CMD_REALM_LIST (0x10), got {opcode:#x}");
    }
    let _size = read_u16_le(r)?;
    let _unk = read_u32_le(r)?;
    let count = read_u16_le(r)?;
    let mut realms = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let realm_type = read_u8(r)?;
        let _lock = read_u8(r)?; // 2.x/3.x clients read the lock byte
        let flags = read_u8(r)?;
        let name = read_cstring(r)?;
        let address = read_cstring(r)?;
        let mut pop = [0u8; 4];
        r.read_exact(&mut pop)?;
        let population = f32::from_le_bytes(pop);
        let characters = read_u8(r)?;
        let _timezone = read_u8(r)?;
        let realm_id = read_u8(r)?;
        if flags & 0x04 != 0 {
            // REALM_FLAG_SPECIFYBUILD: major, minor, bugfix, build u16.
            let _ = read_u8(r)?;
            let _ = read_u8(r)?;
            let _ = read_u8(r)?;
            let _ = read_u16_le(r)?;
        }
        realms.push(RealmInfo { realm_type, flags, name, address, population, characters, realm_id });
    }
    Ok(realms)
}
