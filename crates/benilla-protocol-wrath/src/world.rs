//! The world session over TCP: auth challenge, the digest proof, header crypto from the first
//! post-auth packet, then typed send/recv. Every shape mirrored from `WorldSocket.cpp`.

use std::io::{Read, Write};
use std::net::TcpStream;

use anyhow::{bail, Context, Result};

use crate::auth_session_digest;
use crate::crypt::AuthCrypt;
use crate::opcode;
use crate::ClientHeader;

pub struct WorldConnection {
    stream: TcpStream,
    crypt: Option<AuthCrypt>,
}

/// One decoded world packet.
pub struct Packet {
    pub opcode: u16,
    pub payload: Vec<u8>,
}

fn head_bytes(v: &[u8]) -> Vec<u8> { v.to_vec() }

/// Read exactly `n` bytes, having already consumed at least one byte of this packet. A
/// timeout here is FATAL: bytes are lost and the stream boundary is unknowable, so the error
/// deliberately carries no io::Error for `is_idle` to match (Astra F03 + follow-up).
fn read_exact(r: &mut TcpStream, n: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    match r.read_exact(&mut buf) {
        Ok(()) => Ok(buf),
        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
            anyhow::bail!("timed out mid-packet: framing lost, connection unusable (fatal)")
        }
        Err(_) => anyhow::bail!("world stream ended mid-packet (fatal)"),
    }
}

/// Typed marker: no packet had started arriving; retrying is safe. Mid-packet waits never
/// produce this (they are fatal by policy), so `is_idle` needs no string matching (Astra F03).
#[derive(Debug, thiserror::Error)]
#[error("idle: no packet started arriving")]
pub struct Idle;

/// True for the typed idle marker only.
pub fn is_idle(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Idle>().is_some()
}

impl WorldConnection {
    /// Connect and consume the opening `SMSG_AUTH_CHALLENGE` (plaintext), returning the 4-byte
    /// auth seed.
    pub fn connect(addr: &str) -> Result<(Self, [u8; 4])> {
        let mut stream = TcpStream::connect(addr).with_context(|| format!("connecting to {addr}"))?;
        let head = read_exact(&mut stream, 4)?;
        let size = ((head[0] as usize) << 8 | head[1] as usize) - 2;
        let op = u16::from_le_bytes([head[2], head[3]]);
        if op != opcode::SMSG_AUTH_CHALLENGE {
            bail!("expected SMSG_AUTH_CHALLENGE, got {op:#06x}");
        }
        let payload = read_exact(&mut stream, size)?;
        // Layout (`WorldSocket.cpp:222-230`): u32(1), seed[4], 32 random bytes.
        let mut seed = [0u8; 4];
        seed.copy_from_slice(&payload[4..8]);
        Ok((Self { stream, crypt: None }, seed))
    }

    /// Send `CMSG_AUTH_SESSION` (plaintext header) with the digest proof, then arm the header
    /// crypto from the session key. `WorldSocket.cpp:529-545` field order; the addon blob is the
    /// four zero bytes that make `ReadAddonsInfo` return at once.
    pub fn auth_session(
        &mut self,
        account: &str,
        session_key: &[u8; 40],
        auth_seed: &[u8; 4],
        realm_id: u32,
    ) -> Result<()> {
        let local_challenge: [u8; 4] = rand_local_challenge();
        let digest = auth_session_digest(account, &local_challenge, auth_seed, session_key);
        let mut p = Vec::with_capacity(64 + account.len());
        p.extend_from_slice(&12340u32.to_le_bytes()); // Build
        p.extend_from_slice(&0u32.to_le_bytes()); // LoginServerID
        p.extend_from_slice(account.as_bytes());
        p.push(0); // account null terminator
        p.extend_from_slice(&0u32.to_le_bytes()); // LoginServerType
        p.extend_from_slice(&local_challenge);
        p.extend_from_slice(&0u32.to_le_bytes()); // RegionID
        p.extend_from_slice(&0u32.to_le_bytes()); // BattlegroupID
        p.extend_from_slice(&realm_id.to_le_bytes()); // RealmID
        p.extend_from_slice(&0u64.to_le_bytes()); // DosResponse
        p.extend_from_slice(&digest);
        p.extend_from_slice(&[0u8; 4]); // AddonInfo: u32 0 = no addons
        let header = ClientHeader::new(opcode::CMSG_AUTH_SESSION, p.len())
            .context("AUTH_SESSION payload out of range")?
            .encode();
        self.stream.write_all(&header).context("sending AUTH_SESSION header")?;
        self.stream.write_all(&p).context("sending AUTH_SESSION payload")?;
        self.crypt = Some(AuthCrypt::new(session_key));
        Ok(())
    }

    /// Send one packet with an encrypted header.
    pub fn send(&mut self, opcode: u32, payload: &[u8]) -> Result<()> {
        let mut header = ClientHeader::new(opcode, payload.len())
            .context("payload out of client size window")?
            .encode();
        if let Some(crypt) = self.crypt.as_mut() {
            crypt.encrypt_send(&mut header);
        }
        self.stream.write_all(&header)?;
        self.stream.write_all(payload)?;
        Ok(())
    }

    /// Direct stream access for read-timeout configuration (driver-only).
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Receive one packet, decrypting the server header when crypto is armed. The two-or-three
    /// byte size prefix decides the header length, so the first two bytes are decrypted before
    /// the large-packet flag is even visible — the RC4 stream processes them in arrival order.
    pub fn recv(&mut self) -> Result<Packet> {
        // Idle is legal only before the first byte of a packet arrives; read() gives that exact
        // boundary (Astra F03: a partial read must never be retried as a fresh packet).
        let mut first = [0u8; 1];
        match self.stream.read(&mut first) {
            Ok(1) => {}
            Ok(_) => return Err(anyhow::Error::new(Idle)),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                return Err(anyhow::Error::new(Idle))
            }
            Err(e) => return Err(anyhow::Error::new(e).context("world stream read error")),
        }
        if let Some(crypt) = self.crypt.as_mut() {
            crypt.decrypt_recv(&mut first);
        }
        let mut second = read_exact(&mut self.stream, 1)?;
        if let Some(crypt) = self.crypt.as_mut() {
            crypt.decrypt_recv(&mut second);
        }
        let large = (first[0] & 0x80) != 0;
        let mut head = Vec::with_capacity(if large { 5 } else { 4 });
        head.extend_from_slice(&first);
        head.extend_from_slice(&second);
        if large {
            let mut third = read_exact(&mut self.stream, 1)?;
            if let Some(crypt) = self.crypt.as_mut() {
                crypt.decrypt_recv(&mut third);
            }
            head.extend_from_slice(&third);
        }
        let mut opcode_bytes = read_exact(&mut self.stream, 2)?;
        if let Some(crypt) = self.crypt.as_mut() {
            crypt.decrypt_recv(&mut opcode_bytes);
        }
        head.extend_from_slice(&opcode_bytes);
        if std::env::var("WRATH_DEBUG").is_ok() {
            println!("[recv] head={:02x?}", head);
        }
        let header = crate::ServerHeader::decode(&head).context("decoding server header")?;
        if std::env::var("WRATH_DEBUG").is_ok() {
            println!("[recv] opcode={:#06x} len={}", header.opcode, header.payload_len);
        }
        let payload = if header.payload_len > 0 {
            read_exact(&mut self.stream, header.payload_len)?
        } else {
            Vec::new()
        };
        Ok(Packet { opcode: header.opcode, payload })
    }
}

/// A local challenge without pulling a dependency: time, address and pid stirred by hand. It
/// only needs to be the exact bytes hashed into the digest.
fn rand_local_challenge() -> [u8; 4] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos ^ std::process::id().rotate_left(16)).to_le_bytes()
}
