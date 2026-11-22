//! Bounded, session-thread-only flight recorder. No formatting or IO on the packet path.
use benilla_protocol_wrath::world::{Packet, WorldConnection};
use std::{collections::VecDeque, io::Write, path::Path, time::SystemTime};

struct Entry {
    opcode: u32,
    len: usize,
    at: SystemTime,
    bytes: [u8; 512],
}

pub(super) struct Recorder {
    inbound: VecDeque<Entry>,
    outbound: VecDeque<Entry>,
    active: bool,
    path: &'static str,
    opcode: Option<u32>,
}

impl Recorder {
    pub(super) fn new() -> Self {
        Self {
            inbound: VecDeque::with_capacity(256),
            outbound: VecDeque::with_capacity(64),
            active: false,
            path: "session setup",
            opcode: None,
        }
    }

    pub(super) fn unauthenticated(&mut self) {
        self.active = false;
        self.inbound.clear();
        self.outbound.clear();
        self.step("world authentication", None);
    }

    pub(super) fn authenticated(&mut self) {
        self.inbound.clear();
        self.outbound.clear();
        self.active = true;
    }

    pub(super) fn step(&mut self, path: &'static str, opcode: Option<u32>) {
        self.path = path;
        self.opcode = opcode;
    }

    fn packet(&mut self, inbound: bool, opcode: u32, payload: &[u8]) {
        // Defense in depth: never retain world authentication, even if sent after AUTH_OK.
        if !self.active || matches!(opcode, 0x1ec..=0x1ee) {
            return;
        }
        let (ring, cap) = if inbound {
            (&mut self.inbound, 256)
        } else {
            (&mut self.outbound, 64)
        };
        if ring.len() == cap {
            ring.pop_front();
        }
        let mut bytes = [0; 512];
        let n = payload.len().min(512);
        bytes[..n].copy_from_slice(&payload[..n]);
        ring.push_back(Entry {
            opcode,
            len: payload.len(),
            at: SystemTime::now(),
            bytes,
        });
    }

    pub(super) fn send(
        &mut self,
        world: &mut WorldConnection,
        opcode: u32,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        self.step("socket send", Some(opcode));
        self.packet(false, opcode, payload);
        world.send(opcode, payload)
    }

    pub(super) fn recv(&mut self, world: &mut WorldConnection) -> anyhow::Result<Packet> {
        self.step("socket read (next opcode unavailable)", None);
        let packet = world.recv()?;
        self.packet(true, u32::from(packet.opcode), &packet.payload);
        self.step("packet handling", Some(u32::from(packet.opcode)));
        Ok(packet)
    }

    pub(super) fn persist(&self, error: &anyhow::Error) {
        if let Some(home) = crate::local_state::home() {
            if let Err(e) = self.write(&home.join("Logs/SessionFailures"), error) {
                bevy::log::error!("session failure recorder write failed: {e}");
            }
        }
    }

    fn write(&self, dir: &Path, error: &anyhow::Error) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = dir.join(format!("{stamp:020}-{}.txt", std::process::id()));
        let mut file = std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?,
        );
        writeln!(
            file,
            "error: {error:#}\npath: {}\nopcode: {:?}",
            self.path, self.opcode
        )?;
        for (direction, ring) in [("in", &self.inbound), ("out", &self.outbound)] {
            for entry in ring {
                writeln!(
                    file,
                    "{direction} {:?} opcode=0x{:04x} len={}",
                    entry.at, entry.opcode, entry.len
                )?;
                for byte in &entry.bytes[..entry.len.min(512)] {
                    write!(file, "{byte:02x}")?;
                }
                writeln!(file)?;
            }
        }
        file.flush()?;
        let mut files = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "txt"))
            .collect::<Vec<_>>();
        files.sort();
        let remove = files.len().saturating_sub(10);
        for path in files.into_iter().take(remove) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_payload_limit() {
        let mut r = Recorder::new();
        r.authenticated();
        for i in 0..300 {
            r.packet(true, i, &[7; 600]);
            r.packet(false, i, &[8; 600]);
        }
        assert_eq!(r.inbound.len(), 256);
        assert_eq!(r.outbound.len(), 64);
        assert_eq!(r.inbound.front().unwrap().opcode, 44);
        assert_eq!(r.outbound.front().unwrap().opcode, 236);
        assert_eq!(r.inbound.back().unwrap().bytes, [7; 512]);
    }
    #[test]
    fn authentication_is_never_captured() {
        let mut r = Recorder::new();
        r.packet(true, 1, b"secret");
        r.packet(false, 1, b"secret");
        r.authenticated();
        for op in 0x1ec..=0x1ee {
            r.packet(true, op, b"secret");
            r.packet(false, op, b"secret");
        }
        assert!(r.inbound.is_empty() && r.outbound.is_empty());
    }
    #[test]
    fn reducer_failure_file_and_retention() {
        let dir = std::env::temp_dir().join(format!(
            "session-recorder-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut r = Recorder::new();
        r.authenticated();
        r.packet(true, 0x123, &[0xab]);
        r.step("reducer", Some(0x123));
        let error = benilla_protocol_wrath::session::SessionState::new()
            .reduce(
                benilla_protocol_wrath::opcode::SMSG_LOGIN_VERIFY_WORLD,
                &[],
                0,
            )
            .expect_err("empty LOGIN_VERIFY_WORLD must fail reduction")
            .context("forced reducer error");
        for _ in 0..12 {
            r.write(&dir, &error).unwrap();
        }
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(files.len(), 10);
        let content = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
        assert!(content.contains(&format!("error: {error:#}")));
        assert!(content.contains("path: reducer"));
        assert!(content.contains("opcode=0x0123"));
        assert!(content.contains("ab"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
