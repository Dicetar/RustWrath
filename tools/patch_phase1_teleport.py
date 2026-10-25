"""Phase 1: controlled-player near-teleport (0x0C7), driver lifecycle fixes."""

opath = "crates/benilla-protocol-wrath/src/opcode.rs"
o = open(opath, encoding="utf-8").read()
o = o.replace(
    "pub const MSG_MOVE_TELEPORT: u16 = 0x0C5;",
    """pub const MSG_MOVE_TELEPORT: u16 = 0x0C5;
/// Near teleport, server to the CONTROLLED player (`Player.cpp:1375`,
/// `SendTeleportAckPacket`): packed GUID + u32 order counter + standalone movement payload.
/// The client replies with the same opcode carrying packed GUID + u32 flags + u32 time
/// (`MovementHandler.cpp:277-282` — the ack handler reads exactly those three fields).
pub const MSG_MOVE_TELEPORT_ACK: u16 = 0x0C7;
/// Worldport acknowledgment, client to server (`MovementHandler.cpp:44`): empty payload,
/// processed only while a far teleport is outstanding.
pub const CMSG_WORLD_MOVE_WORLDPORT_ACK: u32 = 0x0DC;""",
)
open(opath, "w", encoding="utf-8", newline="\n").write(o)
print("opcodes added")

spath = "crates/benilla-protocol-wrath/src/session.rs"
s = open(spath, encoding="utf-8").read()

# --- pending ack state ---
old = """    /// The retained snapshot: produced, awaiting successful handoff, replaced by newer state.
    unsent: Option<SelfObservation>,
    pending_lifecycle: Vec<LifeEvent>,
}"""
new = """    /// The retained snapshot: produced, awaiting successful handoff, replaced by newer state.
    unsent: Option<SelfObservation>,
    pending_lifecycle: Vec<LifeEvent>,
    /// Order counters from `MSG_MOVE_TELEPORT_ACK` (server→client) awaiting the client's
    /// acknowledgment packet (`Player.cpp:1375`: the counter increments per teleport).
    pending_teleport_acks: Vec<u32>,
}"""
assert old in s, "pending state anchor missing"
s = s.replace(old, new, 1)

old = """            handoff_revision: 0,
            unsent: None,
            pending_lifecycle: Vec::new(),
        }
    }"""
new = """            handoff_revision: 0,
            unsent: None,
            pending_lifecycle: Vec::new(),
            pending_teleport_acks: Vec::new(),
        }
    }"""
assert old in s, "new() anchor missing"
s = s.replace(old, new, 1)

# --- start_session clears acks ---
old = """        self.handoff_revision = 0;
        self.unsent = None;
    }

    /// The current lifecycle phase."""
new = """        self.handoff_revision = 0;
        self.unsent = None;
        self.pending_teleport_acks.clear();
    }

    /// The current lifecycle phase."""
assert old in s, "start_session ack clear anchor missing"
s = s.replace(old, new, 1)

# --- controlled-player teleport arm (0x0C7) + observer doc fix ---
old = """            opcode::MSG_MOVE_TELEPORT => {
                // Near teleport (`Unit.cpp:16198`): packed GUID + standalone movement payload
                // (no update-flags prefix). Same world scope — a position update, not a
                // transfer.
                let mut r = Reader::new(payload);
                let guid = r.packed_guid().context("teleport guid")?;
                let snapshot = crate::updates::standalone_movement_block(&mut r)
                    .context("teleport movement block")?;
                let relevant = match self.world.get(guid).and_then(|e| e.movement.as_ref()) {
                    Some(prev) => !prev.observation_equivalent(&snapshot),
                    None => true,
                };
                changed |= relevant;
                self.entry(guid, 0).movement = Some(snapshot);
                events.push(ChangeEvent::Moved { guid });
            }"""
new = """            opcode::MSG_MOVE_TELEPORT_ACK => {
                // The controlled player's near-teleport correction (`Player.cpp:1375`):
                // packed GUID + u32 order counter + standalone movement payload. This is the
                // authoritative position for the teleport; the driver owes the server an
                // acknowledgment packet (packed GUID + u32 flags + u32 time, both read and
                // unused by `MovementHandler.cpp:277-282`).
                let mut r = Reader::new(payload);
                let guid = r.packed_guid().context("teleport-ack guid")?;
                let counter = r.u32().context("teleport-ack counter")?;
                let snapshot = crate::updates::standalone_movement_block(&mut r)
                    .context("teleport-ack movement block")?;
                changed |= self.world.apply_movement(guid, snapshot);
                events.push(ChangeEvent::Moved { guid });
                if Some(guid) == self.player_guid {
                    self.pending_teleport_acks.push(counter);
                }
            }
            opcode::MSG_MOVE_TELEPORT => {
                // Observer near-teleport broadcast (`Unit.cpp:16198`,
                // `SendMessageToSet(&data2, false)` — never sent to the mover): packed GUID +
                // standalone movement payload (no update-flags prefix). Same world scope — a
                // position update, not a transfer. The controlled player instead receives
                // MSG_MOVE_TELEPORT_ACK above.
                let mut r = Reader::new(payload);
                let guid = r.packed_guid().context("teleport guid")?;
                let snapshot = crate::updates::standalone_movement_block(&mut r)
                    .context("teleport movement block")?;
                changed |= self.world.apply_movement(guid, snapshot);
                events.push(ChangeEvent::Moved { guid });
            }"""
assert old in s, "teleport arm anchor missing"
s = s.replace(old, new, 1)

# --- take_pending_teleport_acks ---
old = """    /// Drain lifecycle events the driver must act on (publisher retirement).
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent> {
        std::mem::take(&mut self.pending_lifecycle)
    }"""
new = """    /// Drain lifecycle events the driver must act on (publisher retirement).
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent> {
        std::mem::take(&mut self.pending_lifecycle)
    }

    /// Drain order counters owed a `MSG_MOVE_TELEPORT_ACK` acknowledgment packet (client to
    /// server: packed GUID of the player, u32 flags, u32 time — both read and unused by the
    /// server, sent after the client lands).
    pub fn take_pending_teleport_acks(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_teleport_acks)
    }"""
assert old in s, "take_lifecycle anchor missing"
s = s.replace(old, new, 1)

open(spath, "w", encoding="utf-8", newline="\n").write(s)
print("session.rs: controlled-player teleport path added")
