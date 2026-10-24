//! `wrath-cryptprobe <account> <password>`: capture the first raw server bytes after
//! AUTH_SESSION and print what each candidate S->C cipher construction makes of them.
//! Console-only diagnostics; the session key is never written to files or commits.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;
use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::world::WorldConnection;
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};

type HmacSha1 = Hmac<sha1::Sha1>;
const L_ENC: [u8; 16] = [0xCC, 0x98, 0xAE, 0x04, 0xE8, 0x97, 0xEA, 0xCA, 0x12, 0xDD, 0xC0, 0x93, 0x42, 0x91, 0x53, 0x57];
const L_DEC: [u8; 16] = [0xC2, 0xB3, 0x72, 0x3C, 0xC6, 0xAE, 0xD9, 0xB5, 0x34, 0x3C, 0x53, 0xEE, 0x2F, 0x43, 0x67, 0xCE];

struct Rc4 { s: [u8; 256], i: usize, j: usize }
impl Rc4 {
    fn new(key: &[u8]) -> Self {
        let mut s: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut j = 0usize;
        for i in 0..256 {
            j = (j + s[i] as usize + key[i % key.len()] as usize) & 0xFF;
            s.swap(i, j);
        }
        Rc4 { s, i: 0, j: 0 }
    }
    fn crypt(&mut self, data: &mut [u8]) {
        for b in data.iter_mut() {
            self.i = (self.i + 1) & 0xFF;
            self.j = (self.j + self.s[self.i] as usize) & 0xFF;
            self.s.swap(self.i, self.j);
            *b ^= self.s[(self.s[self.i] as usize + self.s[self.j] as usize) & 0xFF];
        }
    }
}

fn hmac(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut mac = HmacSha1::new_from_slice(key).unwrap();
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

fn sha1s(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let account = args.next().unwrap();
    let password = args.next().unwrap();
    let logon = logon("127.0.0.1:3724", &account, &password)?;
    let k = logon.session_key;

    let (mut world, seed) = WorldConnection::connect("127.0.0.1:8095")?;
    world.auth_session(&logon.account, &k, &seed, 1)?;
    let stream = world.stream_mut();
    stream.set_read_timeout(Some(Duration::from_millis(400)))?;

    let mut raw = Vec::new();
    let start = std::time::Instant::now();
    while raw.len() < 64 && start.elapsed() < Duration::from_secs(10) {
        let mut buf = [0u8; 128];
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => {}
        }
    }
    println!("captured {} bytes (local session only)", raw.len());

    fn dropped(key: &[u8]) -> Rc4 {
        let mut rc4 = Rc4::new(key);
        let mut sink = [0u8; 1024];
        rc4.crypt(&mut sink);
        rc4
    }
    let mut variants: Vec<(&str, Rc4)> = vec![
        ("L_enc drop1024 (current code)", dropped(&hmac(&L_ENC, &k))),
        ("L_dec drop1024 (untested!)", dropped(&hmac(&L_DEC, &k))),
        ("L_enc nodrop", Rc4::new(&hmac(&L_ENC, &k))),
        ("L_dec nodrop", Rc4::new(&hmac(&L_DEC, &k))),
        ("swap: key=K msg=L_enc drop1024", dropped(&hmac(&k, &L_ENC))),
        ("swap: key=K msg=L_dec drop1024", dropped(&hmac(&k, &L_DEC))),
    ];
    for (name, rc4) in variants.iter_mut() {
        let mut head = raw[..4.min(raw.len())].to_vec();
        rc4.crypt(&mut head);
        let size = ((head[0] as usize) << 8 | head[1] as usize) - 2;
        let opcode = u16::from_le_bytes([head[2], head[3]]);
        let sane = (4..=300).contains(&size);
        println!("{name:34} -> size={size:5} opcode={opcode:#06x} {}", if sane { "<== SANE" } else { "" });
    }
    let plain = &raw[..4.min(raw.len())];
    println!("plaintext first4: {:02x?}", plain);
    Ok(())
}
