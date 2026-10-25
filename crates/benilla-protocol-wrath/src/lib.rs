//! The 3.3.5a (build 12340) session layer for the fork's retarget.
//!
//! Everything here mirrors the local AzerothCore source the fork will talk to, with the exact
//! file each fact came from named where it lives:
//!
//! - world header crypto: `AuthCrypt.cpp:22-36` — two RC4 streams keyed by
//!   `HMAC-SHA1(fixed label, session_key)`, both dropping the first 1024 output bytes, applied
//!   to header bytes only; the client's send stream uses the server's client-decrypt label and
//!   vice versa.
//! - client→server header: `WorldSocket.h:58-64`, `WorldSocket.cpp:305-329` — six encrypted
//!   bytes: big-endian `u16` size (payload + 4), little-endian `u32` opcode; size range 4..10240.
//! - server→client header: `ServerPktHeader.h:25-45`, `WorldSocket.cpp:177` — two encrypted
//!   big-endian size bytes (three with the `0x80` flag when size > 0x7FFF), then two encrypted
//!   little-endian opcode bytes; size covers payload + opcode.
//! - the auth-session digest: `WorldSocket.cpp:612-623` — SHA-1 over account name, four zero
//!   bytes, the client's local challenge, the server's auth seed, and the 40-byte session key.
//!
//! This crate is deliberately time-free: no clock reads, no sleeps, no timeouts. Sequencing and
//! deadlines belong to the caller, so the same code serves a live client, a script, and the
//! deterministic reducer replay (H2) unchanged.

mod crypt;
mod digest;
mod header;
pub mod auth;
pub mod cast_failure;
pub mod logon;
pub mod fields;
pub mod gameplay;
pub mod opcode;
pub mod reducer;
pub mod session;
pub mod updates;
pub mod world;

pub use crypt::AuthCrypt;
pub use digest::auth_session_digest;
pub use header::{ClientHeader, ServerHeader};
