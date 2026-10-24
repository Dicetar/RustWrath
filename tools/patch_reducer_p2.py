"""Patch pass 2: destroy-object opcode + optional position (Astra finding 3)."""
upath = "crates/benilla-protocol-wrath/src/updates.rs"
u = open(upath, encoding="utf-8").read()

# MovementSnapshot: position becomes Option — a block with no position-bearing flag carries no
# coordinates, and the reducer must never expose an invented (0,0,0,0).
old = """/// The movement snapshot a reducer keeps: identity-bearing fields only.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementSnapshot {
    pub update_flags: u16,
    pub move_flags: u32,
    pub extra_flags: u16,
    pub time: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub o: f32,
    /// The nine living-block speeds in wire order; `None` for non-living objects.
    pub speeds: Option<[f32; 9]>,
}"""
new = """/// World coordinates with orientation, exactly as one position-bearing block delivered them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSnapshot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub o: f32,
}

/// The movement snapshot a reducer keeps: identity-bearing fields only. `position` is `None`
/// when the block carried no position-bearing update flag — a missing position is never
/// represented as an invented zero (Astra review finding 3).
#[derive(Clone, Debug, PartialEq)]
pub struct MovementSnapshot {
    pub update_flags: u16,
    pub move_flags: u32,
    pub extra_flags: u16,
    pub time: u32,
    pub position: Option<PositionSnapshot>,
    /// The nine living-block speeds in wire order; `None` for non-living objects.
    pub speeds: Option<[f32; 9]>,
}"""
assert old in u, "snapshot struct anchor missing"
u = u.replace(old, new, 1)

old = """    let mut snap = MovementSnapshot {
        update_flags: flags,
        move_flags: 0,
        extra_flags: 0,
        time: 0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        o: 0.0,
        speeds: None,
    };"""
new = """    let mut snap = MovementSnapshot {
        update_flags: flags,
        move_flags: 0,
        extra_flags: 0,
        time: 0,
        position: None,
        speeds: None,
    };"""
assert old in u, "snap init anchor missing"
u = u.replace(old, new, 1)

old = """    if flags & update_flags::LIVING != 0 {
        // `Unit::BuildMovementPacket`
        snap.move_flags = r.u32()?;
        snap.extra_flags = r.u16()?;
        snap.time = r.u32()?;
        snap.x = r.f32()?;
        snap.y = r.f32()?;
        snap.z = r.f32()?;
        snap.o = r.f32()?;"""
new = """    if flags & update_flags::LIVING != 0 {
        // `Unit::BuildMovementPacket`
        snap.move_flags = r.u32()?;
        snap.extra_flags = r.u16()?;
        snap.time = r.u32()?;
        let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        snap.position = Some(PositionSnapshot { x, y, z, o });"""
assert old in u, "living position anchor missing"
u = u.replace(old, new, 1)

old = """    } else if flags & update_flags::POSITION != 0 {
        let _transport = r.packed_guid()?;
        snap.x = r.f32()?;
        snap.y = r.f32()?;
        snap.z = r.f32()?;
        let _repeat = (r.f32()?, r.f32()?, r.f32()?); // transport offsets or position again
        snap.o = r.f32()?;
        let _corpse_orientation = r.f32()?;
    } else if flags & update_flags::STATIONARY_POSITION != 0 {
        snap.x = r.f32()?;
        snap.y = r.f32()?;
        snap.z = r.f32()?;
        snap.o = r.f32()?;
    }"""
new = """    } else if flags & update_flags::POSITION != 0 {
        let _transport = r.packed_guid()?;
        let (x, y, z) = (r.f32()?, r.f32()?, r.f32()?);
        let _repeat = (r.f32()?, r.f32()?, r.f32()?); // transport offsets or position again
        let o = r.f32()?;
        let _corpse_orientation = r.f32()?;
        snap.position = Some(PositionSnapshot { x, y, z, o });
    } else if flags & update_flags::STATIONARY_POSITION != 0 {
        let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        snap.position = Some(PositionSnapshot { x, y, z, o });
    }"""
assert old in u, "position branches anchor missing"
u = u.replace(old, new, 1)

open(upath, "w", encoding="utf-8", newline="\n").write(u)
print("updates.rs patched")

# --- reducer.rs: SMSG_DESTROY_OBJECT + MovementSnapshot position accessors ---
rpath = "crates/benilla-protocol-wrath/src/reducer.rs"
r = open(rpath, encoding="utf-8").read()

old = """                1 => {
                    // MOVEMENT: packed guid, one movement block.
                    let guid = r.packed_guid()?;
                    let snapshot = movement_block(&mut r)?;
                    self.entry(guid, 0).movement = Some(snapshot);
                    events.push(ChangeEvent::Moved { guid });
                }"""
new = """                1 => {
                    // MOVEMENT: packed guid, one movement block.
                    let guid = r.packed_guid()?;
                    let snapshot = movement_block(&mut r)?;
                    self.entry(guid, 0).movement = Some(snapshot);
                    events.push(ChangeEvent::Moved { guid });
                }
                0xAA => {
                    // SMSG_DESTROY_OBJECT (`Object.cpp:278-286`): full u64 GUID (not packed)
                    // plus an on-death flag the reducer does not need.
                    let guid = r.u64().context("destroy guid")?;
                    let _on_death = r.u8().context("destroy on-death flag")?;
                    self.remove(guid, &mut events);
                }"""
assert old in r, "movement/mid anchor missing"
r = r.replace(old, new, 1)

open(rpath, "w", encoding="utf-8", newline="\n").write(r)
print("reducer.rs patched")
