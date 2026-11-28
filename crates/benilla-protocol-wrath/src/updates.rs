//! Wire parsing for `SMSG_UPDATE_OBJECT` blocks, mirrored from the local sources: block types and
//! the out-of-range form from `UpdateData.h:28-33` / `UpdateData.cpp:54-70`, packed GUIDs from
//! `ByteBuffer.h:503-516`, create/movement/values block layout from `Object.cpp:178-256`,
//! `Object.cpp:332-482` and `Unit.cpp (BuildMovementPacket)`, and the values mask from
//! `Object.cpp:508-512` (u8 block count, u32 mask words, ascending set-bit values).

use anyhow::{Context, Result};

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

/// `MOVEMENTFLAG2_INTERPOLATED_MOVEMENT` (`UnitDefines.h:440`): only this extra flag puts the
/// transport's `time2` on the wire (`Unit::BuildMovementPacket`, `Unit.cpp:15894-15895`). `0x1`
/// is `MOVEMENTFLAG2_NO_STRAFE`, which vehicles carry; reading `time2` on it shifted every later
/// field by four bytes and dropped the session ("unknown update block type").
pub const MOVE2_INTERPOLATED: u16 = 0x400;

/// `MOVEMENTFLAG2_ALWAYS_ALLOW_PITCHING` (`UnitDefines.h:435`), set on every vehicle whose
/// Vehicle.dbc row allows pitching (`Vehicle.cpp:604-605`).
pub const MOVE2_ALWAYS_ALLOW_PITCHING: u16 = 0x20;

/// Whether a living movement block carries the pitch float: SWIMMING or FLYING, or the
/// always-allow-pitching extra flag (`Unit::BuildMovementPacket`, `Unit.cpp:16114-16117`).
/// Reading it on the move flags alone misaligns every later field of such a vehicle's block.
fn has_pitch(move_flags: u32, extra_flags: u16) -> bool {
    move_flags & move_flags::SWIMMING_OR_FLYING != 0
        || extra_flags & MOVE2_ALWAYS_ALLOW_PITCHING != 0
}

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
        let s = self.p.get(self.at..self.at + n).ok_or_else(|| {
            crate::gameplay::DecodeError::truncated(self.p.len().saturating_sub(self.at))
        })?;
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

/// Spline payload carried inside a `LIVING` create/movement update block when
/// `move_flags::SPLINE_ENABLED` (`0x0800_0000`) is set (`MovementPacketBuilder::WriteCreate`).
#[derive(Clone, Debug, PartialEq)]
pub struct CreateSplineSnapshot {
    pub flags: u32,
    pub facing_angle: Option<f32>,
    pub facing_target: Option<u64>,
    pub facing_spot: Option<[f32; 3]>,
    pub time_passed_ms: u32,
    pub duration_ms: u32,
    pub id: u32,
    pub path: Vec<[f32; 3]>,
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
    pub transport: Option<(u64, i8)>,
    /// Vehicle.dbc row of this object, when the update carries the VEHICLE flag.
    pub vehicle_id: Option<u32>,
    /// The nine living-block speeds in wire order; `None` for non-living objects.
    pub speeds: Option<[f32; 9]>,
    /// Active spline from a create/movement block when `SPLINE_ENABLED` is set.
    pub create_spline: Option<CreateSplineSnapshot>,
    /// The unit's melee victim from the `HAS_TARGET` packed guid (AC sets the flag on a create
    /// while the unit has a victim); `None` when the flag is clear or the guid is zero. The
    /// 12340 client seeds its attack target from it at creation (`CCombat::SetClientInitData`).
    pub attack_target: Option<u64>,
    /// A transport GameObject's path clock (`UPDATEFLAG_TRANSPORT`, `Object.cpp:459-465`): the
    /// ms its path has run (`Transport::GetPathProgress`, kept modulo the period by the
    /// client), the anchor from which a boat, zeppelin or lift is animated locally; `None`
    /// when the block carried no transport flag. Only a create sends it.
    pub transport_progress: Option<u32>,
}

impl MovementSnapshot {
    /// Compare pose and spline state. The reducer compares persistent transport and vehicle
    /// state separately, since some movement packets do not deliver those fields. A timestamp
    /// alone does not advance `state_revision`.
    pub fn observation_equivalent(&self, other: &Self) -> bool {
        self.move_flags == other.move_flags
            && self.extra_flags == other.extra_flags
            && self.position == other.position
            && self.speeds == other.speeds
            && self.create_spline == other.create_spline
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
        transport: None,
        vehicle_id: None,
        speeds: None,
        create_spline: None,
        attack_target: None,
        transport_progress: None,
    };
    if flags & update_flags::LIVING != 0 {
        // `Unit::BuildMovementPacket`
        snap.move_flags = r.u32()?;
        snap.extra_flags = r.u16()?;
        snap.time = r.u32()?;
        let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        snap.position = Some(PositionSnapshot { x, y, z, o });
        if snap.move_flags & move_flags::ON_TRANSPORT != 0 {
            snap.transport = Some(read_transport(r, snap.extra_flags)?);
        }
        if has_pitch(snap.move_flags, snap.extra_flags) {
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
            let mut facing_angle = None;
            let mut facing_target = None;
            let mut facing_spot = None;
            if spline_flags & 0x0002_0000 != 0 {
                facing_angle = Some(r.f32()?); // final angle
            } else if spline_flags & 0x0001_0000 != 0 {
                facing_target = Some(r.u64()?); // final target guid
            } else if spline_flags & 0x0000_8000 != 0 {
                facing_spot = Some([r.f32()?, r.f32()?, r.f32()?]); // final point xyz
            }
            let time_passed_ms = r.u32()?; // time passed
            let duration_ms = r.u32()?; // duration
            let id = r.u32()?; // spline id
            let _duration_mod = r.f32()?; // duration mod
            let _duration_mod_next = r.f32()?; // duration mod next
            let _vertical_accel = r.f32()?; // vertical acceleration
            let _effect_start_time = r.u32()?; // effect start time
            let nodes = r.u32()? as usize;
            let mut path = Vec::with_capacity(nodes.min(1024) + 2);
            path.push([x, y, z]);
            for _ in 0..nodes {
                let p = [r.f32()?, r.f32()?, r.f32()?];
                if path.last().copied() != Some(p) {
                    path.push(p);
                }
            }
            let _spline_mode = r.u8()?; // spline mode
            let final_dest = [r.f32()?, r.f32()?, r.f32()?]; // final destination
            if path.last().copied() != Some(final_dest) {
                path.push(final_dest);
            }
            snap.create_spline = Some(CreateSplineSnapshot {
                flags: spline_flags,
                facing_angle,
                facing_target,
                facing_spot,
                time_passed_ms,
                duration_ms,
                id,
                path,
            });
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
        snap.attack_target = Some(r.packed_guid()?).filter(|&guid| guid != 0);
    }
    if flags & update_flags::TRANSPORT != 0 {
        snap.transport_progress = Some(r.u32()?);
    }
    if flags & update_flags::VEHICLE != 0 {
        snap.vehicle_id = Some(r.u32()?);
        r.f32()?; // vehicle orientation
    }
    if flags & update_flags::ROTATION != 0 {
        r.i64()?; // packed world rotation
    }
    Ok(snap)
}

fn read_transport(r: &mut Reader, extra_flags: u16) -> Result<(u64, i8)> {
    let guid = r.packed_guid()?;
    for _ in 0..4 {
        r.f32()?; // offsets x y z o
    }
    r.u32()?; // transport time
    let seat = r.u8()? as i8;
    if extra_flags & MOVE2_INTERPOLATED != 0 {
        r.u32()?; // time2
    }
    Ok((guid, seat))
}

/// Parse a standalone movement payload — the `Unit::BuildMovementPacket` shape used by
/// `MSG_MOVE_TELEPORT` (S→C near teleport, `Unit.cpp:16198`): packed GUID, then movement data
/// with **no** u16 update-flags prefix and none of the update-block-only tail fields
/// (`LOWGUID`/`HAS_TARGET`/`VEHICLE`/`ROTATION`).
pub fn standalone_movement_block(r: &mut Reader) -> Result<MovementSnapshot> {
    let mut snap = MovementSnapshot {
        update_flags: update_flags::LIVING,
        move_flags: r.u32()?,
        extra_flags: r.u16()?,
        time: r.u32()?,
        position: None,
        transport: None,
        vehicle_id: None,
        speeds: None,
        create_spline: None,
        attack_target: None,
        transport_progress: None,
    };
    let (x, y, z, o) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
    snap.position = Some(PositionSnapshot { x, y, z, o });
    if snap.move_flags & move_flags::ON_TRANSPORT != 0 {
        snap.transport = Some(read_transport(r, snap.extra_flags)?);
    }
    if has_pitch(snap.move_flags, snap.extra_flags) {
        r.f32()?;
    }
    r.u32()?;
    if snap.move_flags & move_flags::FALLING != 0 {
        for _ in 0..4 {
            r.f32()?;
        }
    }
    if snap.move_flags & move_flags::SPLINE_ELEVATION != 0 {
        r.f32()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The head of a living block (`Unit::BuildMovementPacket`) for an object standing still,
    /// up to and including the fall time; `pitch` is written only when given.
    fn living_head(move_flags: u32, extra_flags: u16, pitch: Option<f32>) -> Vec<u8> {
        let mut p = move_flags.to_le_bytes().to_vec();
        p.extend(extra_flags.to_le_bytes());
        p.extend(7u32.to_le_bytes()); // time
        for v in [1.0f32, 2.0, 3.0, 0.5] {
            p.extend(v.to_le_bytes());
        }
        if let Some(pitch) = pitch {
            p.extend(pitch.to_le_bytes());
        }
        p.extend(0u32.to_le_bytes()); // fall time
        p
    }

    /// A vehicle whose row allows pitching gets `MOVEMENTFLAG2_ALWAYS_ALLOW_PITCHING`, and the
    /// server writes its pitch with no SWIMMING or FLYING flag. The create block must still read
    /// the speeds and the vehicle id from their own bytes and end where the builder ended.
    #[test]
    fn pitching_vehicle_reads_its_pitch_without_swim_or_fly() {
        let mut p = (update_flags::LIVING | update_flags::VEHICLE)
            .to_le_bytes()
            .to_vec();
        p.extend(living_head(0, MOVE2_ALWAYS_ALLOW_PITCHING, Some(0.25)));
        let speeds = [2.5f32, 7.0, 4.5, 2.5, 4.72, 7.0, 4.5, 3.5, 3.5];
        for s in speeds {
            p.extend(s.to_le_bytes());
        }
        p.extend(321u32.to_le_bytes()); // vehicle id
        p.extend(0.5f32.to_le_bytes()); // vehicle orientation
        let mut r = Reader::new(&p);
        let snap = movement_block(&mut r).unwrap();
        assert!(r.done());
        assert_eq!(snap.speeds, Some(speeds));
        assert_eq!(snap.vehicle_id, Some(321));

        // The standalone movement shape gates the pitch the same way.
        let p = living_head(0, MOVE2_ALWAYS_ALLOW_PITCHING, Some(0.25));
        let mut r = Reader::new(&p);
        standalone_movement_block(&mut r).unwrap();
        assert!(r.done());
    }

    /// A vehicle riding a transport with `NO_STRAFE` and friends set (`0x1b`, seen live on a
    /// `0xF150…` vehicle): no `time2` follows the seat, so the speeds start right after the fall
    /// time. Reading `time2` on `0x1` shifted them and the next block read as type 42.
    #[test]
    fn transport_time2_follows_only_interpolated_movement() {
        for (extra, time2) in [(0x1bu16, false), (MOVE2_INTERPOLATED | 0x1b, true)] {
            let mut p = update_flags::LIVING.to_le_bytes().to_vec();
            p.extend(move_flags::ON_TRANSPORT.to_le_bytes());
            p.extend(extra.to_le_bytes());
            p.extend(7u32.to_le_bytes()); // time
            for v in [1.0f32, 2.0, 3.0, 0.5] {
                p.extend(v.to_le_bytes());
            }
            p.extend([0x01, 0x42]); // packed transport guid 0x42
            for v in [0.1f32, 0.2, 0.3, 0.0] {
                p.extend(v.to_le_bytes());
            }
            p.extend(99u32.to_le_bytes()); // transport time
            p.push(2); // seat
            if time2 {
                p.extend(55u32.to_le_bytes());
            }
            p.extend(0u32.to_le_bytes()); // fall time
            let speeds = [2.5f32, 7.0, 4.5, 4.72, 2.5, 7.0, 4.5, 3.5, 3.5];
            for s in speeds {
                p.extend(s.to_le_bytes());
            }
            let mut r = Reader::new(&p);
            let snap = movement_block(&mut r).unwrap();
            assert!(r.done(), "extra {extra:#x}");
            assert_eq!(snap.speeds, Some(speeds), "extra {extra:#x}");
            assert_eq!(snap.transport, Some((0x42, 2)));
        }
    }

    /// A transport GameObject's create, as `Transport::Create` flags it (`Transport.cpp:36`,
    /// `:685`): the path clock is kept for the local animation, and every later field still
    /// lines up.
    #[test]
    fn a_transport_create_keeps_its_path_clock() {
        let flags = update_flags::TRANSPORT
            | update_flags::LOWGUID
            | update_flags::STATIONARY_POSITION
            | update_flags::ROTATION;
        let mut p = flags.to_le_bytes().to_vec();
        for v in [1544.2f32, 240.8, -40.5, 1.57] {
            p.extend(v.to_le_bytes());
        }
        p.extend(0x77u32.to_le_bytes()); // low guid
        p.extend(123_456u32.to_le_bytes()); // path progress
        p.extend(0x0123_4567_89ab_cdefi64.to_le_bytes()); // packed rotation
        let mut r = Reader::new(&p);
        let snap = movement_block(&mut r).unwrap();
        assert!(r.done());
        assert_eq!(snap.transport_progress, Some(123_456));
        assert_eq!(snap.position.map(|p| p.x), Some(1544.2));

        // Any other object's block carries no clock.
        let mut p = update_flags::STATIONARY_POSITION.to_le_bytes().to_vec();
        for v in [1.0f32, 2.0, 3.0, 0.0] {
            p.extend(v.to_le_bytes());
        }
        let snap = movement_block(&mut Reader::new(&p)).unwrap();
        assert_eq!(snap.transport_progress, None);
    }
}
