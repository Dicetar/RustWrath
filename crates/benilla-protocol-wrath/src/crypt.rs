//! The client-side mirror of AzerothCore's `AuthCrypt` (`AuthCrypt.cpp`): RC4-drop1024 over
//! header bytes, keyed per direction by `HMAC-SHA1(label, session_key)`.

use hmac::{Hmac, Mac};

type HmacSha1 = Hmac<sha1::Sha1>;

/// Plain RC4 as WoW uses it (`Acore::Crypto::ARC4`).
pub struct Rc4 {
    s: [u8; 256],
    i: usize,
    j: usize,
}

impl Rc4 {
    pub(crate) fn new(key: &[u8]) -> Self {
        let mut s: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut j = 0usize;
        for i in 0..256 {
            j = (j + s[i] as usize + key[i % key.len()] as usize) & 0xFF;
            s.swap(i, j);
        }
        // The PRGA restarts from i = j = 0 after the key schedule: standard RC4 resets both
        // (OpenSSL's EVP set-key does the same). The KSA's final j is NOT carried over —
        // carrying it desynchronized every S->C header (Astra, 2026-10-24).
        Self { s, i: 0, j: 0 }
    }

    pub(crate) fn xor_in_place(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            self.i = (self.i + 1) & 0xFF;
            self.j = (self.j + self.s[self.i] as usize) & 0xFF;
            self.s.swap(self.i, self.j);
            *byte ^= self.s[(self.s[self.i] as usize + self.s[self.j] as usize) & 0xFF];
        }
    }
}

/// The label AzerothCore seeds the server's encrypt (client's receive) stream with.
const SERVER_ENCRYPTION_KEY: [u8; 16] = [
    0xCC, 0x98, 0xAE, 0x04, 0xE8, 0x97, 0xEA, 0xCA, 0x12, 0xDD, 0xC0, 0x93, 0x42, 0x91, 0x53, 0x57,
];
/// The label AzerothCore seeds the server's client-decrypt (client's send) stream with.
const SERVER_DECRYPTION_KEY: [u8; 16] = [
    0xC2, 0xB3, 0x72, 0x3C, 0xC6, 0xAE, 0xD9, 0xB5, 0x34, 0x3C, 0x53, 0xEE, 0x2F, 0x43, 0x67, 0xCE,
];

pub(crate) fn stream(label: &[u8; 16], session_key: &[u8; 40]) -> Rc4 {
    let mut mac = HmacSha1::new_from_slice(label).expect("HMAC accepts any key length");
    mac.update(session_key);
    let digest = mac.finalize().into_bytes();
    let mut rc4 = Rc4::new(&digest);
    // ARC4-drop1024: consume the first 1024 output bytes synchronously (`AuthCrypt.cpp:30-33`).
    let mut sink = [0u8; 1024];
    rc4.xor_in_place(&mut sink);
    rc4
}

/// The client's view of the session crypto: one stream per direction, headers only.
pub struct AuthCrypt {
    send: Rc4,
    recv: Rc4,
}

impl AuthCrypt {
    /// Initialize both streams from the 40-byte SRP6 session key.
    pub fn new(session_key: &[u8; 40]) -> Self {
        Self {
            send: stream(&SERVER_DECRYPTION_KEY, session_key),
            recv: stream(&SERVER_ENCRYPTION_KEY, session_key),
        }
    }

    /// Encrypt a client→server header in place (six bytes).
    pub fn encrypt_send(&mut self, header: &mut [u8]) {
        self.send.xor_in_place(header);
    }

    /// Decrypt a server→client header in place (the two or three size bytes plus two opcode
    /// bytes; call sites decrypt exactly what they received).
    pub fn decrypt_recv(&mut self, header: &mut [u8]) {
        self.recv.xor_in_place(header);
    }
}

#[cfg(test)]
mod kat {
    use super::*;

    /// Published RC4 known-answer vector (Schneier, Applied Cryptography): key "Key",
    /// plaintext "Plaintext" -> BBF316E8D940AF0AD3. An independent reference, so a defect
    /// shared by both sides of the interop tests cannot pass (Astra's F12 rule).
    #[test]
    fn rc4_matches_the_published_vector() {
        let mut rc4 = Rc4::new(b"Key");
        let mut data = b"Plaintext".to_vec();
        rc4.xor_in_place(&mut data);
        assert_eq!(data, [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
    }

    /// Second published vector (key "Wiki", plaintext "pedia").
    #[test]
    fn rc4_matches_the_second_published_vector() {
        let mut rc4 = Rc4::new(b"Wiki");
        let mut data = b"pedia".to_vec();
        rc4.xor_in_place(&mut data);
        assert_eq!(data, [0x10, 0x21, 0xBF, 0x04, 0x20]);
    }
}

#[cfg(test)]
mod bisect {
    use super::*;

    #[test]
    fn authcrypt_matches_the_inline_construction_stage_by_stage() {
        let k = [7u8; 40];
        let label: [u8; 16] = [
            0xCC, 0x98, 0xAE, 0x04, 0xE8, 0x97, 0xEA, 0xCA, 0x12, 0xDD, 0xC0, 0x93, 0x42, 0x91,
            0x53, 0x57,
        ];
        // Stage 1: HMAC digest.
        let mut mac = HmacSha1::new_from_slice(&label).unwrap();
        mac.update(&k);
        let digest: [u8; 20] = mac.finalize().into_bytes().into();
        // The crate's stream over the same inputs.
        let mut via_crate = stream(&label, &k);
        let mut crate_keystream = [0u8; 8];
        via_crate.xor_in_place(&mut crate_keystream);
        // Inline: fresh Rc4 over the same digest, drop 1024, take 8.
        let mut inline = Rc4::new(&digest);
        let mut sink = [0u8; 1024];
        inline.xor_in_place(&mut sink);
        let mut inline_keystream = [0u8; 8];
        inline.xor_in_place(&mut inline_keystream);
        assert_eq!(
            crate_keystream, inline_keystream,
            "stream() and inline disagree: digest={:02x?} crate={:02x?} inline={:02x?}",
            digest, crate_keystream, inline_keystream
        );
        // Full public path: AuthCrypt::new + decrypt_recv over zeros.
        let mut crypt = AuthCrypt::new(&k);
        let mut public_keystream = [0u8; 8];
        crypt.decrypt_recv(&mut public_keystream);
        assert_eq!(
            public_keystream, inline_keystream,
            "public path disagrees: public={:02x?} inline={:02x?}",
            public_keystream, inline_keystream
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server-side labels swapped: what AzerothCore itself runs. Our client send stream
    /// must interop with the server's client-decrypt stream and nothing else.
    fn server_side(session_key: &[u8; 40]) -> (Rc4, Rc4) {
        (
            stream(&SERVER_ENCRYPTION_KEY, session_key),  // server encrypt → our recv
            stream(&SERVER_DECRYPTION_KEY, session_key),  // server decrypt → our send
        )
    }

    #[test]
    fn client_send_round_trips_through_the_server_decrypt_stream() {
        let key = [7u8; 40];
        let mut client = AuthCrypt::new(&key);
        let (_server_encrypt, mut server_decrypt) = server_side(&key);
        for round in 0u8..3 {
            let plain = [round * 11 + 1; 6];
            let mut header = plain;
            client.encrypt_send(&mut header);
            assert_ne!(header, plain, "RC4 must change the bytes");
            server_decrypt.xor_in_place(&mut header);
            assert_eq!(header, plain, "round {round} decrypts back");
        }
    }

    #[test]
    fn server_send_round_trips_through_the_client_recv_stream() {
        let key = [9u8; 40];
        let mut client = AuthCrypt::new(&key);
        let (mut server_encrypt, _server_decrypt) = server_side(&key);
        let mut header = [0xAB, 0x01, 0x02, 0x03];
        server_encrypt.xor_in_place(&mut header);
        client.decrypt_recv(&mut header);
        assert_eq!(header, [0xAB, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn streams_advance_identically_in_both_directions() {
        let key = [3u8; 40];
        let mut client = AuthCrypt::new(&key);
        let (mut server_encrypt, mut server_decrypt) = server_side(&key);
        for round in 0..5u8 {
            let mut to_server = [round; 6];
            client.encrypt_send(&mut to_server);
            server_decrypt.xor_in_place(&mut to_server);
            assert_eq!(to_server, [round; 6], "C→S round {round}");

            let mut to_client = [round; 4];
            server_encrypt.xor_in_place(&mut to_client);
            client.decrypt_recv(&mut to_client);
            assert_eq!(to_client, [round; 4], "S→C round {round}");
        }
    }
}
