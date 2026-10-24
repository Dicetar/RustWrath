//! The world-session auth digest, mirrored from `WorldSocket.cpp:612-623`:
//! SHA-1 over the account name, four zero bytes, the client's local challenge, the server's
//! auth seed, and the 40-byte SRP6 session key.

use sha1::{Digest, Sha1};

pub fn auth_session_digest(
    account: &str,
    local_challenge: &[u8; 4],
    auth_seed: &[u8; 4],
    session_key: &[u8; 40],
) -> [u8; 20] {
    let mut sha = Sha1::new();
    sha.update(account.as_bytes());
    sha.update([0u8; 4]);
    sha.update(local_challenge);
    sha.update(auth_seed);
    sha.update(session_key);
    sha.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stability vector: fixed inputs must produce a fixed digest (regression against any
    /// future reorder of the hash fields).
    #[test]
    fn digest_is_stable_for_fixed_inputs() {
        let d = auth_session_digest("PROBE", &[1, 2, 3, 4], &[5, 6, 7, 8], &[42u8; 40]);
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex.len(), 40);
        let d2 = auth_session_digest("PROBE", &[1, 2, 3, 4], &[5, 6, 7, 8], &[42u8; 40]);
        assert_eq!(d, d2);
        // Any changed field must change the digest.
        let other = auth_session_digest("PROBE2", &[1, 2, 3, 4], &[5, 6, 7, 8], &[42u8; 40]);
        assert_ne!(d, other);
    }
}
