//! Wire parsing for `SMSG_UPDATE_OBJECT` blocks, mirrored from the local sources: block types and
//! the out-of-range form from `UpdateData.h:28-33` / `UpdateData.cpp:54-70`, packed GUIDs from
//! `ByteBuffer.h:503-516`, create/movement/values block layout from `Object.cpp:178-256`,
//! `Object.cpp:332-482` and `Unit.cpp (BuildMovementPacket)`, and the values mask from
//! `Object.cpp:508-512` (u8 block count, u32 mask words, ascending set-bit values).

use anyhow::{bail, Context, Result};

/// Update-flag bits (`UpdateData.h:33-46`), in the builder's write order.
pub mod update_flags {
    pub const SELF: u16 = 0x0001;
    pub const TRANSPORT: u16 = 0x0002;
    pub const HAS_TARGET: u16 = 0x0004;
    pub const UNKNOWN: u16 = 0x0008;
    pub const LOWGUID: u16 = 0x0010;
    pub const LIVING: u16 = 0x0020;
    pub const STATIONARY_POSITION: u16 = 0x0040;
    pub const VEHICLE: u16 = 0x0080;
    pub const POSITION: u16 = 0x0100;
    pub const ROTATION: u16 = 0x0200;
}

/// Movement-flag bits used by the living block (`Unit::BuildMovementPacket` and its guards).
pub mod move_flags {
    pub const ON_TRANSPORT: u32 = 0x0000_0200;
    pub const FALLING: u32 = 0x0000_1000;
    pub const SWIMMING_OR_FLYING: u32 = 0x0220_0000;
    pub const SPLINE_ELEVATION: u32 = 0x0400_0000;
    pub const SPLINE_ENABLED: u32 = 0x0800_0000;
}

/// The extra movement flags carrying `MOVEMENTFLAG2_INTERPOLATED_MOVEMENT`.
pub const MOVE2_INTERPOLATED: u16 = 0x1;

/// Object type ids as the create block writes them (`Object.cpp:230`).
pub mod type_id {
    pub const OBJECT: u8 = 0;
    pub const ITEM: u8 = 1;
    pub const CONTAINER: u8 = 2;
    pub const UNIT: u8 = 3;
    pub const PLAYER: u8 = 4;
    pub const GAMEOBJECT: u8 = 5;
    pub const DYNAMICOBJECT: u8 = 6;
    pub const CORPSE: u8 = 7;
}

/// A parsed cursor over one packet payload: every read checked.
pub struct Reader<'a> {
    pub p: &'a [u8],
    pub at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(p: &'a [u8]) -> Self {
        Reader { p, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.p.get(self.at..self.at + n).ok_or_else(|| anyhow::anyhow!("payload cut short"))?;
        self.at += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn done(&self) -> bool {
        self.at >= self.p.len()
    }

    /// A packed GUID (`ByteBuffer.h:503-516`): a mask byte, then the nonzero bytes it names.
    pub fn packed_guid(&mut self) -> Result<u64> {
        let mask = self.u8()?;
        let mut guid = 0u64;
        for i in 0..8 {
            if mask & (1 << i) != 0 {
                guid |= (self.u8()? as u64) << (i * 8);
            }
        }
        Ok(guid)
    }
}

/// World coordinates with orientation, exactly as one position-bearing block delivered them.
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
}

impl MovementSnapshot {
    /// Whether two snapshots differ in anything the observation schema can see. The wire's
    /// per-packet timestamp (`time`) is a clock, not state, and is deliberately excluded: a
    /// periodic heartbeat with identical position, flags and speeds is not a source state
    /// change and must not advance `state_revision`.
    pub fn observation_equivalent(&self, other: &Self) -> bool {
        self.move_flags == other.move_flags
            && self.extra_flags == other.extra_flags
            && self.position == other.position
            && self.speeds == other.speeds
    }
}

/// Parse one full movement update (`Object::BuildMovementUpdate`, `Object.cpp:332-482`),
/// consuming exactly the bytes the builder wrote.
pub fn movement_block(r: &mut Reader) -> Result<MovementSnapshot> {
    let flags = r.u16().context("update flags")?;
    let mut snap = MovementSnapshot {
        update_flags: flags,
        move_flags: 0,
        extra_flags: 0,
        time: 0,
        position: None,
        speeds: None,
    };
    if flags & update_flags::LIVING != 0 {
        // `Unit::BuildMovementPacket`
        snap.move_flags = r.u32()?;
        snap.extra_flags = r.u16()?;
        snap.time = r.u32()?;
        let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        snap.position = Some(PositionSnapshot { x, y, z, o });
        if snap.move_flags & move_flags::ON_TRANSPORT != 0 {
            r.packed_guid()?; // transport/vehicle base
            for _ in 0..4 {
                r.f32()?; // offsets x y z o
            }
            r.u32()?; // transport time
            r.u8()?; // seat
            if snap.extra_flags & MOVE2_INTERPOLATED != 0 {
                r.u32()?; // time2
            }
        }
        if snap.move_flags & move_flags::SWIMMING_OR_FLYING != 0 {
            r.f32()?; // pitch
        }
        r.u32()?; // fall time
        if snap.move_flags & move_flags::FALLING != 0 {
            for _ in 0..4 {
                r.f32()?; // jump zspeed, sin, cos, xyspeed
            }
        }
        if snap.move_flags & move_flags::SPLINE_ELEVATION != 0 {
            r.f32()?;
        }
        let mut speeds = [0f32; 9];
        for slot in &mut speeds {
            *slot = r.f32()?;
        }
        snap.speeds = Some(speeds);
        if snap.move_flags & move_flags::SPLINE_ENABLED != 0 {
            // `MovementPacketBuilder.cpp (WriteCreate)` — spline flag facing bits from
            // `MoveSplineFlag.h:46-48`: Final_Point 0x8000, Final_Target 0x10000 (a u64),
            // Final_Angle 0x20000 (an f32).
            let spline_flags = r.u32()?;
            if spline_flags & 0x0002_0000 != 0 {
                r.f32()?; // final angle
            } else if spline_flags & 0x0001_0000 != 0 {
                r.u64()?; // final target guid
            } else if spline_flags & 0x0000_8000 != 0 {
                for _ in 0..3 {
                    r.f32()?; // final point xyz
                }
            }
            r.u32()?; // time passed
            r.u32()?; // duration
            r.u32()?; // spline id
            r.f32()?; // duration mod
            r.f32()?; // duration mod next
            r.f32()?; // vertical acceleration
            r.u32()?; // effect start time
            let nodes = r.u32()? as usize;
            for _ in 0..nodes {
                for _ in 0..3 {
                    r.f32()?; // one path node
                }
            }
            r.u8()?; // spline mode
            for _ in 0..3 {
                r.f32()?; // final destination
            }
        }
    } else if flags & update_flags::POSITION != 0 {
        let _transport = r.packed_guid()?;
        let (x, y, z) = (r.f32()?, r.f32()?, r.f32()?);
        let _repeat = (r.f32()?, r.f32()?, r.f32()?); // transport offsets or position again
        let o = r.f32()?;
        let _corpse_orientation = r.f32()?;
        snap.position = Some(PositionSnapshot { x, y, z, o });
    } else if flags & update_flags::STATIONARY_POSITION != 0 {
        let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        snap.position = Some(PositionSnapshot { x, y, z, o });
    }
    if flags & update_flags::UNKNOWN != 0 {
        r.u32()?;
    }
    if flags & update_flags::LOWGUID != 0 {
        r.u32()?;
    }
    if flags & update_flags::HAS_TARGET != 0 {
        r.packed_guid()?;
    }
    if flags & update_flags::TRANSPORT != 0 {
        r.u32()?; // path progress
    }
    if flags & update_flags::VEHICLE != 0 {
        r.u32()?; // vehicle id
        r.f32()?; // vehicle orientation
    }
    if flags & update_flags::ROTATION != 0 {
        r.i64()?; // packed world rotation
    }
    Ok(snap)
}

/// One values block: u8 mask-word count, u32 mask words, ascending set-bit values.
pub fn values_block(r: &mut Reader) -> Result<Vec<(u16, u32)>> {
    let blocks = r.u8()? as usize;
    let mut mask = vec![0u32; blocks];
    for word in &mut mask {
        *word = r.u32()?;
    }
    let mut out = Vec::new();
    for (word_index, word) in mask.iter().enumerate() {
        for bit in 0..32 {
            if word & (1 << bit) != 0 {
                let index = (word_index * 32 + bit) as u16;
                out.push((index, r.u32()?));
            }
        }
    }
    Ok(out)
}
