//! The world session over TCP: auth challenge, the digest proof, header crypto from the first
//! post-auth packet, then typed send/recv. Every shape mirrored from `WorldSocket.cpp`.

use std::collections::VecDeque;
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
    /// Optional wire recording of every post-crypto packet (fixtures for the reducer replay).
    /// A compound move packet is recorded as it arrived, not as its sub-messages.
    recording: Option<Vec<(u16, Vec<u8>)>>,
    /// Sub-messages of a compound move packet not yet handed out by [`Self::recv`].
    unpacked: VecDeque<Packet>,
}

/// One decoded world packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub opcode: u16,
    pub payload: Vec<u8>,
}

/// Split `SMSG_MULTIPLE_MOVES`/`SMSG_COMPRESSED_MOVES` into the packets they carry, in order, or
/// `None` for any other opcode. 12340's `UnpackAndProcessOpcodes` (`Unit_C.cpp`, called by
/// `0x00716AF0` and `0x00716AD0`) reads a `u32` size, inflates the rest to that size when
/// compressed (and otherwise takes the rest as it is), then loops over `u8 length` records whose
/// bytes are a `u16` opcode and its body, handing each to the connection's normal dispatch.
pub fn unpack_moves(opcode: u16, payload: &[u8]) -> Result<Option<Vec<Packet>>> {
    let compressed = match opcode {
        opcode::SMSG_MULTIPLE_MOVES => false,
        opcode::SMSG_COMPRESSED_MOVES => true,
        _ => return Ok(None),
    };
    let size_bytes: [u8; 4] = payload
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .context("compound move packet shorter than its size field")?;
    let size = u32::from_le_bytes(size_bytes) as usize;
    let rest = &payload[4..];
    let inflated;
    let stream: &[u8] = if compressed {
        // The size is the server's own claim; bound it before allocating on it.
        anyhow::ensure!(
            size <= 0x10_0000,
            "compressed move packet claims {size} bytes"
        );
        let mut out = Vec::with_capacity(size);
        flate2::read::ZlibDecoder::new(rest)
            .take(size as u64)
            .read_to_end(&mut out)
            .context("inflating SMSG_COMPRESSED_MOVES")?;
        anyhow::ensure!(
            out.len() == size,
            "SMSG_COMPRESSED_MOVES inflated to {} bytes, not the {size} it claims",
            out.len()
        );
        inflated = out;
        &inflated
    } else {
        rest
    };
    let mut packets = Vec::new();
    let mut at = 0;
    while at < stream.len() {
        let length = usize::from(stream[at]);
        at += 1;
        let record = stream
            .get(at..at + length)
            .context("compound move record runs past the packet")?;
        anyhow::ensure!(
            length >= 2,
            "compound move record of {length} bytes has no opcode"
        );
        packets.push(Packet {
            opcode: u16::from_le_bytes([record[0], record[1]]),
            payload: record[2..].to_vec(),
        });
        at += length;
    }
    Ok(Some(packets))
}

fn head_bytes(v: &[u8]) -> Vec<u8> {
    v.to_vec()
}

/// Read exactly `n` bytes, having already consumed at least one byte of this packet. A
/// timeout here is FATAL: bytes are lost and the stream boundary is unknowable, so the error
/// deliberately carries no io::Error for `is_idle` to match (Astra F03 + follow-up).
fn read_exact(r: &mut TcpStream, n: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    match r.read_exact(&mut buf) {
        Ok(()) => Ok(buf),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
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
        use std::net::ToSocketAddrs;
        let timeout = std::time::Duration::from_secs(10);
        let mut stream = addr
            .to_socket_addrs()?
            .map(|a| TcpStream::connect_timeout(&a, timeout))
            .find_map(Result::ok)
            .with_context(|| format!("connecting to {addr}"))?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
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
        Ok((
            Self {
                stream,
                crypt: None,
                recording: None,
                unpacked: VecDeque::new(),
            },
            seed,
        ))
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
        self.stream
            .write_all(&header)
            .context("sending AUTH_SESSION header")?;
        self.stream
            .write_all(&p)
            .context("sending AUTH_SESSION payload")?;
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

    /// Record every received packet (post-crypto) for fixture capture.
    pub fn enable_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    /// Take the recorded packet stream (opcode, payload), clearing the recorder.
    pub fn take_recording(&mut self) -> Vec<(u16, Vec<u8>)> {
        self.recording.take().unwrap_or_default()
    }

    /// Direct stream access for read-timeout configuration (driver-only).
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Receive one packet. A compound move packet never comes out: its sub-messages do, one per
    /// call and in their order, before the next wire read, as 12340 dispatches each through the
    /// connection ([`unpack_moves`]). A compound nested inside one is split the same way.
    pub fn recv(&mut self) -> Result<Packet> {
        loop {
            let packet = match self.unpacked.pop_front() {
                Some(packet) => packet,
                None => self.recv_wire()?,
            };
            match unpack_moves(packet.opcode, &packet.payload)? {
                Some(parts) => {
                    for part in parts.into_iter().rev() {
                        self.unpacked.push_front(part);
                    }
                }
                None => return Ok(packet),
            }
        }
    }

    /// Read one packet off the wire, decrypting the server header when crypto is armed. The
    /// two-or-three byte size prefix decides the header length, so the first two bytes are
    /// decrypted before the large-packet flag is even visible — the RC4 stream processes them in
    /// arrival order.
    fn recv_wire(&mut self) -> Result<Packet> {
        // Idle is legal only before the first byte of a packet arrives; read() gives that exact
        // boundary (Astra F03: a partial read must never be retried as a fresh packet).
        let mut first = [0u8; 1];
        match self.stream.read(&mut first) {
            Ok(1) => {}
            Ok(_) => anyhow::bail!("world stream closed"),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
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
            println!(
                "[recv] opcode={:#06x} len={}",
                header.opcode, header.payload_len
            );
        }
        let payload = if header.payload_len > 0 {
            read_exact(&mut self.stream, header.payload_len)?
        } else {
            Vec::new()
        };
        if let Some(rec) = self.recording.as_mut() {
            rec.push((header.opcode, payload.clone()));
        }
        Ok(Packet {
            opcode: header.opcode,
            payload,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// AzerothCore's login compound (`Player.cpp:11602`): a root and a hover order for packed
    /// guid `0x05` (mask 0x01), counters 7 and 8, each record `u8 (2 + 2 + 4)`.
    fn login_compound() -> Vec<u8> {
        let mut records = Vec::new();
        let orders = [
            (opcode::SMSG_FORCE_MOVE_ROOT, 7u32),
            (opcode::SMSG_MOVE_SET_HOVER, 8),
        ];
        for (op, counter) in orders {
            records.push(8u8);
            records.extend_from_slice(&op.to_le_bytes());
            records.extend_from_slice(&[0x01, 0x05]);
            records.extend_from_slice(&counter.to_le_bytes());
        }
        records
    }

    #[test]
    fn compound_moves_split_into_their_orders_plain_or_inflated() {
        let records = login_compound();
        let expected = vec![
            Packet {
                opcode: opcode::SMSG_FORCE_MOVE_ROOT,
                payload: vec![0x01, 0x05, 7, 0, 0, 0],
            },
            Packet {
                opcode: opcode::SMSG_MOVE_SET_HOVER,
                payload: vec![0x01, 0x05, 8, 0, 0, 0],
            },
        ];

        let mut plain = (records.len() as u32).to_le_bytes().to_vec();
        plain.extend_from_slice(&records);
        assert_eq!(
            unpack_moves(opcode::SMSG_MULTIPLE_MOVES, &plain).unwrap(),
            Some(expected.clone())
        );

        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&records).unwrap();
        let mut compressed = (records.len() as u32).to_le_bytes().to_vec();
        compressed.extend_from_slice(&z.finish().unwrap());
        assert_eq!(
            unpack_moves(opcode::SMSG_COMPRESSED_MOVES, &compressed).unwrap(),
            Some(expected)
        );

        // Any other opcode passes through; a record running past the end is refused.
        assert_eq!(
            unpack_moves(opcode::SMSG_FORCE_MOVE_ROOT, &plain).unwrap(),
            None
        );
        plain.truncate(plain.len() - 1);
        assert!(unpack_moves(opcode::SMSG_MULTIPLE_MOVES, &plain).is_err());
    }
}
