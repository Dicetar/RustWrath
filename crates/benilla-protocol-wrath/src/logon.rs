//! The SRP6 logon driver: challenge → proof → verified session key → realm list, over one TCP
//! connection to the authserver. The SRP6 math is benilla-srp's, already pinned to the published
//! WoW vectors; this module only supplies the 3.3.5 wire shapes.

use std::net::TcpStream;

use anyhow::{Context, Result};
use benilla_srp::{NormalizedString, PublicKey, SrpClientChallenge};

use crate::auth::{self, RealmInfo};

/// A completed logon: the 40-byte session key and the realm list.
pub struct Logon {
    pub session_key: [u8; 40],
    pub account: String,
    pub realms: Vec<RealmInfo>,
}

/// Authenticate `account`/`password` against the authserver at `addr` (`host:port`).
pub fn logon(addr: &str, account: &str, password: &str) -> Result<Logon> {
    let mut stream = TcpStream::connect(addr).with_context(|| format!("connecting to {addr}"))?;
    let username_n = NormalizedString::new(account).context("normalizing account name")?;
    let account_upper = username_n.as_ref().to_string();
    let password_n = NormalizedString::new(password).context("normalizing password")?;

    auth::write_logon_challenge(&mut stream, username_n.as_ref())
        .context("sending logon challenge")?;
    let reply = auth::read_challenge_reply(&mut stream).context("reading challenge reply")?;

    let challenge = SrpClientChallenge::new(
        username_n,
        password_n,
        reply.generator,
        reply.large_safe_prime,
        PublicKey::from_le_bytes(reply.server_public_key)
            .context("server public key rejected by SRP6 validation")?,
        reply.salt,
    );
    auth::write_logon_proof(
        &mut stream,
        challenge.client_public_key(),
        challenge.client_proof(),
    )
    .context("sending logon proof")?;
    let server_proof = auth::read_proof_reply(&mut stream).context("reading proof reply")?;
    let session_key = *challenge
        .verify_server_proof(server_proof)
        .map_err(|e| anyhow::anyhow!("server proof mismatch (wrong password?): {e}"))?
        .session_key();

    auth::write_realm_list_request(&mut stream).context("requesting realm list")?;
    let realms = auth::read_realm_list(&mut stream).context("reading realm list")?;
    Ok(Logon { session_key, account: account_upper, realms })
}
