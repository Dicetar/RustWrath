//! 12340 server splines: AC MovementPacketBuilder.cpp / MoveSplineInit.cpp. Offsets in a
//! linear path are relative to the start/end midpoint, unlike the donor's endpoint offsets.
use super::Reader;
use anyhow::{ensure, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum MonsterFacing {
    None,
    Spot([f32; 3]),
    Target(u64),
    Angle(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonsterMove {
    pub guid: u64,
    pub transport: Option<(u64, i8)>,
    pub movement_byte: u8,
    pub start: [f32; 3],
    pub spline_id: u32,
    pub facing: MonsterFacing,
    pub stop: bool,
    pub flags: u32,
    pub duration_ms: u32,
    pub animation: Option<(u8, u32)>,
    pub parabolic: Option<(f32, u32)>,
    /// Travel order including start. Curved/cyclic flags remain explicit for the consumer.
    pub path: Vec<[f32; 3]>,
}

fn scalar(r: &mut Reader<'_>) -> Result<f32> {
    let value = r.f32()?;
    ensure!(value.is_finite(), "non-finite spline coordinate");
    Ok(value)
}

fn point(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([scalar(r)?, scalar(r)?, scalar(r)?])
}

pub(super) fn read(body: &[u8], on_transport: bool) -> Result<MonsterMove> {
    let mut r = Reader::new(body);
    let guid = r.packed_guid()?;
    let transport = if on_transport { Some((r.packed_guid()?, r.u8()? as i8)) } else { None };
    let movement_byte = r.u8()?;
    let start = point(&mut r)?;
    let spline_id = r.u32()?;
    let kind = r.u8()?;
    let facing = match kind {
        0 | 1 => MonsterFacing::None,
        2 => MonsterFacing::Spot(point(&mut r)?),
        3 => MonsterFacing::Target(r.u64()?),
        4 => MonsterFacing::Angle(scalar(&mut r)?),
        _ => return Err(crate::gameplay::DecodeError::invalid_value("unknown monster move type").into()),
    };
    let mut result = MonsterMove {
        guid, transport, movement_byte, start, spline_id, facing, stop: kind == 1,
        flags: 0, duration_ms: 0, animation: None, parabolic: None, path: Vec::new(),
    };
    if !result.stop {
        result.flags = r.u32()?;
        if result.flags & 0x0020_0000 != 0 {
            result.animation = Some((r.u8()?, r.u32()?));
        }
        result.duration_ms = r.u32()?;
        if result.flags & 0x800 != 0 {
            result.parabolic = Some((scalar(&mut r)?, r.u32()?));
        }
        let count = r.u32()? as usize;
        let smooth = result.flags & (0x2000 | 0x40000) != 0;
        let required = if smooth { count.checked_mul(12) } else if count == 0 {
            Some(0)
        } else { (count - 1).checked_mul(4).and_then(|n| n.checked_add(12)) };
        ensure!(required.is_some_and(|n| n <= r.remaining()), "spline points exceed packet");
        if count > 0 {
            result.path.reserve(count + 1);
            result.path.push(start);
            if smooth {
                for _ in 0..count { result.path.push(point(&mut r)?); }
            } else {
                let end = point(&mut r)?;
                let middle = std::array::from_fn::<_, 3, _>(|i| (start[i] + end[i]) * 0.5);
                for _ in 1..count {
                    let packed = r.u32()?;
                    let offset = [((packed << 21) as i32 >> 21) as f32 * 0.25,
                        ((packed << 10) as i32 >> 21) as f32 * 0.25,
                        ((packed as i32) >> 22) as f32 * 0.25];
                    result.path.push(std::array::from_fn(|i| middle[i] - offset[i]));
                }
                result.path.push(end);
            }
        }
    }
    ensure!(result.path.iter().flatten().all(|v| v.is_finite()), "non-finite spline path");
    ensure!(r.remaining() == 0, "unexpected spline tail");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(kind: u8) -> Vec<u8> {
        let mut b = vec![1, 7, 0];
        for value in [0.0f32; 3] { b.extend_from_slice(&value.to_le_bytes()); }
        b.extend_from_slice(&123u32.to_le_bytes());
        b.push(kind);
        b
    }

    #[test]
    fn monster_move_linear_path_uses_midpoint_offsets_and_stop_has_no_spline_tail() {
        let stop = read(&head(1), false).unwrap();
        assert!(stop.stop && stop.path.is_empty());
        let mut b = head(0);
        for value in [0u32, 1000, 2] { b.extend_from_slice(&value.to_le_bytes()); }
        for value in [8.0f32, 4.0, 0.0] { b.extend_from_slice(&value.to_le_bytes()); }
        b.extend_from_slice(&(8u32 | (4 << 11)).to_le_bytes());
        let decoded = read(&b, false).unwrap();
        assert_eq!(decoded.path, vec![[0.0; 3], [2.0, 1.0, 0.0], [8.0, 4.0, 0.0]]);
        for len in 0..b.len() { assert!(read(&b[..len], false).is_err()); }
        let count_at = 28;
        b[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&b, false).is_err());
    }
}
