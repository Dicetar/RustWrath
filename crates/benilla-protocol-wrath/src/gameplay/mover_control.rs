//! The server's orders to the unit we control, and its broadcasts of other units' modes and
//! speeds: 12340 registers them on `Packet_MoveMode` (`0x00732450`), `Packet_SplineMoveEffect`
//! (`0x00741C30`), `Packet_SplineSpeedChange` (`0x00741BC0`) and
//! `Packet_SMSG_CLIENT_CONTROL_UPDATE` (`0x0072D0B0`). The bodies follow AzerothCore's writers,
//! cited per reader.

use anyhow::Result;

use super::{DecodeError, Reader, ServerEvent};
use crate::opcode::*;

/// The nine `UnitMoveType` speeds, in `Unit.h`'s order (`SetSpeed2Opc_table`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedKind {
    Walk,
    Run,
    RunBack,
    Swim,
    SwimBack,
    TurnRate,
    Flight,
    FlightBack,
    PitchRate,
}

/// A mode the server orders on the unit we control; each order is `packed guid, u32 counter`
/// and owes its own ack carrying that counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    /// `SMSG_FORCE_MOVE_ROOT`/`_UNROOT` (`Unit::SetRooted`).
    Root,
    /// `SMSG_MOVE_WATER_WALK`/`_LAND_WALK` (`Unit::SetWaterWalking`).
    WaterWalk,
    /// `SMSG_MOVE_FEATHER_FALL`/`_NORMAL_FALL` (`Unit::SetFeatherFall`).
    FeatherFall,
    /// `SMSG_MOVE_SET_HOVER`/`_UNSET_HOVER` (`Unit::SetHover`).
    Hover,
    /// `SMSG_MOVE_SET_CAN_FLY`/`_UNSET_CAN_FLY` (`Unit::SetCanFly`, `Unit.cpp:16547`).
    CanFly,
    /// `SMSG_MOVE_GRAVITY_DISABLE`/`_ENABLE` (`Unit::SetDisableGravity`); `apply` disables.
    GravityDisabled,
}

/// A mode broadcast for any unit, a bare packed guid with nothing to ack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplineMode {
    Root,
    WaterWalk,
    FeatherFall,
    Hover,
    /// `apply` is the walk flag's direction: `SET_WALK_MODE` applies, `SET_RUN_MODE` clears.
    WalkMode,
    Swimming,
    Flying,
    /// `apply` disables gravity.
    GravityDisabled,
}

/// Decode one of the opcodes this module owns, `None` for any other.
pub(super) fn parse(opcode: u16, body: &[u8]) -> Option<Result<ServerEvent>> {
    if let Some(kind) = forced_speed_kind(opcode) {
        return Some(read_force_speed(kind, body));
    }
    if let Some(kind) = spline_speed_kind(opcode) {
        return Some(read_spline_speed(kind, body));
    }
    if let Some((mode, apply)) = move_mode(opcode) {
        return Some(read_move_mode(mode, apply, body));
    }
    if let Some((mode, apply)) = spline_mode(opcode) {
        return Some(read_spline_mode(mode, apply, body));
    }
    match opcode {
        SMSG_MOVE_KNOCK_BACK => Some(read_knock_back(body)),
        SMSG_CLIENT_CONTROL_UPDATE => Some(read_client_control(body)),
        _ => None,
    }
}

fn forced_speed_kind(opcode: u16) -> Option<SpeedKind> {
    Some(match opcode {
        SMSG_FORCE_WALK_SPEED_CHANGE => SpeedKind::Walk,
        SMSG_FORCE_RUN_SPEED_CHANGE => SpeedKind::Run,
        SMSG_FORCE_RUN_BACK_SPEED_CHANGE => SpeedKind::RunBack,
        SMSG_FORCE_SWIM_SPEED_CHANGE => SpeedKind::Swim,
        SMSG_FORCE_SWIM_BACK_SPEED_CHANGE => SpeedKind::SwimBack,
        SMSG_FORCE_TURN_RATE_CHANGE => SpeedKind::TurnRate,
        SMSG_FORCE_FLIGHT_SPEED_CHANGE => SpeedKind::Flight,
        SMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE => SpeedKind::FlightBack,
        SMSG_FORCE_PITCH_RATE_CHANGE => SpeedKind::PitchRate,
        _ => return None,
    })
}

fn spline_speed_kind(opcode: u16) -> Option<SpeedKind> {
    Some(match opcode {
        SMSG_SPLINE_SET_WALK_SPEED => SpeedKind::Walk,
        SMSG_SPLINE_SET_RUN_SPEED => SpeedKind::Run,
        SMSG_SPLINE_SET_RUN_BACK_SPEED => SpeedKind::RunBack,
        SMSG_SPLINE_SET_SWIM_SPEED => SpeedKind::Swim,
        SMSG_SPLINE_SET_SWIM_BACK_SPEED => SpeedKind::SwimBack,
        SMSG_SPLINE_SET_TURN_RATE => SpeedKind::TurnRate,
        SMSG_SPLINE_SET_FLIGHT_SPEED => SpeedKind::Flight,
        SMSG_SPLINE_SET_FLIGHT_BACK_SPEED => SpeedKind::FlightBack,
        SMSG_SPLINE_SET_PITCH_RATE => SpeedKind::PitchRate,
        _ => return None,
    })
}

fn move_mode(opcode: u16) -> Option<(MoveMode, bool)> {
    Some(match opcode {
        SMSG_FORCE_MOVE_ROOT => (MoveMode::Root, true),
        SMSG_FORCE_MOVE_UNROOT => (MoveMode::Root, false),
        SMSG_MOVE_WATER_WALK => (MoveMode::WaterWalk, true),
        SMSG_MOVE_LAND_WALK => (MoveMode::WaterWalk, false),
        SMSG_MOVE_FEATHER_FALL => (MoveMode::FeatherFall, true),
        SMSG_MOVE_NORMAL_FALL => (MoveMode::FeatherFall, false),
        SMSG_MOVE_SET_HOVER => (MoveMode::Hover, true),
        SMSG_MOVE_UNSET_HOVER => (MoveMode::Hover, false),
        SMSG_MOVE_SET_CAN_FLY => (MoveMode::CanFly, true),
        SMSG_MOVE_UNSET_CAN_FLY => (MoveMode::CanFly, false),
        SMSG_MOVE_GRAVITY_DISABLE => (MoveMode::GravityDisabled, true),
        SMSG_MOVE_GRAVITY_ENABLE => (MoveMode::GravityDisabled, false),
        _ => return None,
    })
}

fn spline_mode(opcode: u16) -> Option<(SplineMode, bool)> {
    Some(match opcode {
        SMSG_SPLINE_MOVE_ROOT => (SplineMode::Root, true),
        SMSG_SPLINE_MOVE_UNROOT => (SplineMode::Root, false),
        SMSG_SPLINE_MOVE_WATER_WALK => (SplineMode::WaterWalk, true),
        SMSG_SPLINE_MOVE_LAND_WALK => (SplineMode::WaterWalk, false),
        SMSG_SPLINE_MOVE_FEATHER_FALL => (SplineMode::FeatherFall, true),
        SMSG_SPLINE_MOVE_NORMAL_FALL => (SplineMode::FeatherFall, false),
        SMSG_SPLINE_MOVE_SET_HOVER => (SplineMode::Hover, true),
        SMSG_SPLINE_MOVE_UNSET_HOVER => (SplineMode::Hover, false),
        SMSG_SPLINE_MOVE_SET_WALK_MODE => (SplineMode::WalkMode, true),
        SMSG_SPLINE_MOVE_SET_RUN_MODE => (SplineMode::WalkMode, false),
        SMSG_SPLINE_MOVE_START_SWIM => (SplineMode::Swimming, true),
        SMSG_SPLINE_MOVE_STOP_SWIM => (SplineMode::Swimming, false),
        SMSG_SPLINE_MOVE_SET_FLYING => (SplineMode::Flying, true),
        SMSG_SPLINE_MOVE_UNSET_FLYING => (SplineMode::Flying, false),
        SMSG_SPLINE_MOVE_GRAVITY_DISABLE => (SplineMode::GravityDisabled, true),
        SMSG_SPLINE_MOVE_GRAVITY_ENABLE => (SplineMode::GravityDisabled, false),
        _ => return None,
    })
}

fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(())
}

/// `Unit::SetSpeed` (`Unit.cpp:11276`): `packed guid, u32 counter`, a `u8 0` for the run speed
/// alone, then `f32 speed`.
fn read_force_speed(kind: SpeedKind, b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    let counter = r.u32()?;
    if kind == SpeedKind::Run {
        let _unused = r.u8()?;
    }
    let speed = r.f32()?;
    finish(&r)?;
    Ok(ServerEvent::ForceSpeedChange {
        kind,
        guid,
        counter,
        speed,
    })
}

/// `Unit::SetSpeed`'s broadcast form: `packed guid, f32 speed`.
fn read_spline_speed(kind: SpeedKind, b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    let speed = r.f32()?;
    finish(&r)?;
    Ok(ServerEvent::SpeedChanged { kind, guid, speed })
}

fn read_move_mode(mode: MoveMode, apply: bool, b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    let counter = r.u32()?;
    finish(&r)?;
    Ok(ServerEvent::MoveMode {
        mode,
        apply,
        guid,
        counter,
    })
}

fn read_spline_mode(mode: SplineMode, apply: bool, b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    finish(&r)?;
    Ok(ServerEvent::SplineMoveMode { mode, apply, guid })
}

/// `Unit::KnockbackFrom`: `packed guid, u32 counter, f32 vcos, f32 vsin, f32 speedXY,
/// f32 -speedZ`. The last float is already down-positive.
fn read_knock_back(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    let counter = r.u32()?;
    let cos_angle = r.f32()?;
    let sin_angle = r.f32()?;
    let xy_speed = r.f32()?;
    let z_speed_down = r.f32()?;
    finish(&r)?;
    Ok(ServerEvent::KnockBack {
        guid,
        counter,
        cos_angle,
        sin_angle,
        xy_speed,
        z_speed_down,
    })
}

/// `Player::SetClientControl`: `packed guid, u8 allowMove`.
fn read_client_control(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    let allow_move = r.u8()? != 0;
    finish(&r)?;
    Ok(ServerEvent::ClientControl { guid, allow_move })
}
