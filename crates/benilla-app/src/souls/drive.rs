//! What the Souls mode asks of the player controller each frame ([`SoulsDrive`]):
//!
//! - **Root motion.** The running kit action's root motion (`motion_at`, cumulative left/forward
//!   metres) is followed as a debt: each frame adds the action's own displacement since the last
//!   frame and subtracts what the body actually moved along the facing. The controller is then
//!   driven by the sign of the debt on each axis (at the stock run/strafe speed), which keeps the
//!   predicted body where the animation puts it without a teleport, so the server sees ordinary
//!   movement. Until the action's movement cancel, with no debt, the axes are held at zero: the
//!   body does not walk out of an attack. Held movement past the cancel releases the action.
//! - **Turn lock.** Inside the kit's no-turn windows the facing is held where it was.
//! - **Turn rate.** Inside the kit's turn-rate windows the controller caps the frame's turn at the
//!   window's degrees a second, so an attack tracks a target only so fast.
//! - **Lock-on.** With a lock, the facing follows the target's bearing and the turn keys strafe;
//!   under the free camera the camera swings round behind the body toward the target too.
//! - **Free camera** ([`super::ErControls`]). The camera orbits on its own and movement is
//!   camera-relative: the held keys name a bearing off the camera, the body turns toward it (fast,
//!   not instantly) and runs forward. An action's first frames snap the facing to that bearing, so
//!   a roll or a swing goes where the stick points; after them the kit's turn-rate windows (else
//!   [`FREE_TRACK_DEGREES`]) let it track.
//!
//! Off (or with no kit table), the drive is the default: the controller runs its stock path.

use bevy::prelude::*;
use benilla_combat_core::{KitAction, Milli};

use super::{
    current_kit, in_window, running_action, souls_on, ErControls, Instance, SoulsConfig,
    SoulsState, YARDS_PER_METRE,
};
use crate::bindings::{BindingsState, Input};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::player::camera::FlyCam;
use crate::player::{Player, SoulsDrive};

/// Debt below this many yards on an axis is not chased (the controller would overshoot it).
const DEADBAND_YARDS: f32 = 0.25;
/// A frame's displacement above this is a teleport or a correction, not movement to count.
const TELEPORT_YARDS: f32 = 5.0;
/// Under the free camera the body turns toward the run's bearing this fast, degrees a second.
const FREE_TURN_DEGREES: f32 = 720.0;
/// An action's facing snaps to the held bearing through its first frames (kit milli-frames).
const FREE_SNAP_FRAMES: Milli = Milli(3000);
/// An action with no turn-rate window of its own tracks the held bearing this fast.
const FREE_TRACK_DEGREES: f32 = 240.0;

/// Root-motion and turn-lock tracking of one running action.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) struct DriveTrack {
    instance: Option<Instance>,
    /// The action's cumulative `[left, up, forward]` root motion at the last frame, metres.
    last_motion: [Milli; 3],
    /// Root motion still owed, yards: `x` left, `y` forward.
    debt: Vec2,
    last_pos: Vec3,
    /// The facing held through the current no-turn window.
    held_yaw: Option<f32>,
}

/// The facing's forward and left in Bevy axes for yaw `yaw`.
fn basis(yaw: f32) -> (Vec3, Vec3) {
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let left = Vec3::new(-yaw.cos(), 0.0, yaw.sin());
    (forward, left)
}

/// The Bevy yaw that faces along `direction`.
pub(super) fn yaw_toward(direction: Vec3) -> f32 {
    f32::atan2(-direction.x, -direction.z)
}

/// The axis value that chases `debt` yards: one step either way past the deadband.
fn chase(debt: f32) -> i32 {
    if debt > DEADBAND_YARDS {
        1
    } else if debt < -DEADBAND_YARDS {
        -1
    } else {
        0
    }
}

/// Write the frame's [`SoulsDrive`] for the controller.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_controller(
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    playback: Res<CombatPlaybackState>,
    bindings: Res<BindingsState>,
    er: Res<ErControls>,
    cams: Query<&FlyCam>,
    targets: Query<&GlobalTransform>,
    mut souls: ResMut<SoulsState>,
    mut player: ResMut<Player>,
) {
    if !souls_on(&view) {
        if player.souls != SoulsDrive::default() {
            player.souls = SoulsDrive::default();
        }
        return;
    }
    let features = config.features;
    let mut drive = SoulsDrive {
        free_camera: er.active() && er.free_camera,
        orbit: er.hides_cursor(),
        ..SoulsDrive::default()
    };

    // Lock-on: face the target, the turn keys strafe.
    let mut locked_on = false;
    if features.lock_on {
        if let Some(target) = souls.lock.and_then(|entity| targets.get(entity).ok()) {
            let to = target.translation() - player.pos;
            if to.x * to.x + to.z * to.z > 0.01 {
                let bearing = yaw_toward(to);
                drive.face = Some(bearing);
                if drive.free_camera {
                    drive.cam_yaw = Some(bearing);
                }
            }
            drive.turn_keys_strafe = true;
            locked_on = true;
        }
    }

    // The free camera's run: the bearing the held keys name off the camera, without a lock.
    let free_bearing = cams
        .single()
        .ok()
        .filter(|_| drive.free_camera && !locked_on)
        .and_then(|cam| run_bearing(&bindings, cam.yaw()));
    let free_turn = FREE_TURN_DEGREES.to_radians();

    let running = current_kit(&kit, &profiles).and_then(|kit| running_action(kit, &playback));
    let Some((instance, action, frame)) = running else {
        souls.drive = DriveTrack::default();
        if let Some(bearing) = free_bearing {
            free_run(&mut drive, bearing, free_turn);
        }
        write_drive(&mut player, drive);
        return;
    };
    let mut track = souls.drive;
    if track.instance != Some(instance) {
        track = DriveTrack {
            instance: Some(instance),
            last_motion: action.motion_at(Milli::ZERO),
            debt: Vec2::ZERO,
            last_pos: player.pos,
            held_yaw: None,
        };
    }
    // Under the free camera the turn keys move as well.
    let movement_held = [
        Input::MoveForward,
        Input::MoveBackward,
        Input::StrafeLeft,
        Input::StrafeRight,
    ]
    .into_iter()
    .any(|input| bindings.pressed(input))
        || free_bearing.is_some();
    let released = movement_held && action.cancel.movement.is_some_and(|from| frame >= from);

    // Root motion: owed displacement minus what the body moved along the facing.
    let (forward, left) = basis(player.facing());
    let moved = player.pos - track.last_pos;
    if moved.length() < TELEPORT_YARDS {
        track.debt -= Vec2::new(moved.dot(left), moved.dot(forward));
    }
    let motion = action.motion_at(frame);
    let step = |axis: usize| {
        (motion[axis].0 - track.last_motion[axis].0) as f32 / 1000.0 * YARDS_PER_METRE
    };
    track.debt += Vec2::new(step(0), step(2));
    track.last_motion = motion;
    track.last_pos = player.pos;
    if features.root_motion && !released {
        // Right is positive on the controller's strafe axis; the debt's x is left.
        drive.axes = Some((chase(track.debt.y), -chase(track.debt.x)));
    } else {
        track.debt = Vec2::ZERO;
    }

    // Turn lock: the facing holds through a no-turn window (over the lock-on bearing too).
    let locked = !released && action.no_turn.iter().any(|&window| in_window(frame, window));
    if features.turn_lock && locked {
        let held = *track.held_yaw.get_or_insert(player.facing());
        drive.face = Some(held);
    } else {
        track.held_yaw = None;
    }
    // Turn rate: inside a turn-rate window the whole turn (lock-on tracking included) is capped.
    if features.turn_rate && !released {
        drive.turn_rate = turn_rate_at(action, frame);
    }
    // The free camera: released, the body runs off the camera again; inside the action its
    // first frames snap to the held bearing and the rest track it (never through a no-turn
    // window, which the turn lock above already holds).
    if let Some(bearing) = free_bearing {
        if released {
            free_run(&mut drive, bearing, free_turn);
        } else if !(features.turn_lock && locked) {
            drive.face = Some(bearing);
            drive.turn_rate = if frame < FREE_SNAP_FRAMES {
                None
            } else {
                drive.turn_rate.or(Some(FREE_TRACK_DEGREES.to_radians()))
            };
        }
    }
    souls.drive = track;
    write_drive(&mut player, drive);
}

/// The bearing the held movement keys name off the camera's yaw `cam_yaw`, or `None` with none
/// held (or a cancelled pair). The turn keys count as strafes.
fn run_bearing(bindings: &BindingsState, cam_yaw: f32) -> Option<f32> {
    let held = |input: Input| i32::from(bindings.pressed(input));
    let forward = held(Input::MoveForward) - held(Input::MoveBackward);
    let right = (held(Input::StrafeRight) | held(Input::TurnRight))
        - (held(Input::StrafeLeft) | held(Input::TurnLeft));
    stick_bearing(forward, right, cam_yaw)
}

/// The yaw a `forward`/`right` stick names off `cam_yaw` (left-positive, as the facing).
fn stick_bearing(forward: i32, right: i32, cam_yaw: f32) -> Option<f32> {
    if forward == 0 && right == 0 {
        return None;
    }
    Some(cam_yaw + f32::atan2(-(right as f32), forward as f32))
}

/// Run forward toward `bearing`, turning at `rate` radians a second; the turn keys move, not turn.
fn free_run(drive: &mut SoulsDrive, bearing: f32, rate: f32) {
    drive.face = Some(bearing);
    drive.axes = Some((1, 0));
    drive.turn_rate = Some(rate);
    drive.turn_keys_strafe = true;
}

/// The turn-rate cap at `frame`, radians a second: the first `turn` window holding it.
fn turn_rate_at(action: &KitAction, frame: Milli) -> Option<f32> {
    action
        .turn
        .iter()
        .find(|&&(from, to, _)| in_window(frame, (from, to)))
        .map(|&(_, _, degrees)| (degrees.0 as f32 / 1000.0).to_radians())
}

fn write_drive(player: &mut Player, drive: SoulsDrive) {
    if player.souls != drive {
        player.souls = drive;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The basis and the bearing agree with the controller's yaw convention: forward is
    /// `(-sin y, 0, -cos y)`, and the yaw toward a direction recovers it.
    #[test]
    fn basis_and_bearing_agree() {
        for yaw in [0.0_f32, 0.7, -2.0, 3.0] {
            let (forward, left) = basis(yaw);
            assert!((yaw_toward(forward) - yaw).abs() < 1e-4);
            assert!(forward.dot(left).abs() < 1e-6);
        }
        // Facing -Z, left is -X; facing -X (a quarter turn), left is +Z.
        let (forward, left) = basis(0.0);
        assert!((forward - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
        assert!((left - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-6);
        let (_, left) = basis(std::f32::consts::FRAC_PI_2);
        assert!((left - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
        assert_eq!(chase(0.3), 1);
        assert_eq!(chase(-0.3), -1);
        assert_eq!(chase(0.1), 0);
    }

    /// The free camera's stick: forward is the camera's yaw, right turns the bearing clockwise
    /// (negative), back faces the camera.
    #[test]
    fn stick_bearing_is_camera_relative() {
        let cam = 0.5_f32;
        assert_eq!(stick_bearing(0, 0, cam), None);
        assert!((stick_bearing(1, 0, cam).unwrap() - cam).abs() < 1e-6);
        let right = stick_bearing(0, 1, cam).unwrap();
        assert!((right - (cam - std::f32::consts::FRAC_PI_2)).abs() < 1e-6);
        let back = stick_bearing(-1, 0, cam).unwrap();
        assert!(((back - cam).abs() - std::f32::consts::PI).abs() < 1e-6);
        // Moving right along the bearing is moving along the camera's right.
        let (forward, _) = basis(right);
        let (_, cam_left) = basis(cam);
        assert!((forward + cam_left).length() < 1e-5);
    }
}
