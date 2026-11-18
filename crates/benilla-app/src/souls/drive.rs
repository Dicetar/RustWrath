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
//! - **Lock-on.** With a lock, the facing follows the target's bearing and the turn keys strafe.
//!
//! Off (or with no kit table), the drive is the default: the controller runs its stock path.

use bevy::prelude::*;
use benilla_combat_core::Milli;

use super::{current_kit, in_window, running_action, souls_on, Instance, SoulsConfig, SoulsState, YARDS_PER_METRE};
use crate::bindings::{BindingsState, Input};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::player::{Player, SoulsDrive};

/// Debt below this many yards on an axis is not chased (the controller would overshoot it).
const DEADBAND_YARDS: f32 = 0.25;
/// A frame's displacement above this is a teleport or a correction, not movement to count.
const TELEPORT_YARDS: f32 = 5.0;

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
    let mut drive = SoulsDrive::default();

    // Lock-on: face the target, the turn keys strafe.
    if features.lock_on {
        if let Some(target) = souls.lock.and_then(|entity| targets.get(entity).ok()) {
            let to = target.translation() - player.pos;
            if to.x * to.x + to.z * to.z > 0.01 {
                drive.face = Some(yaw_toward(to));
            }
            drive.turn_keys_strafe = true;
        }
    }

    let running = current_kit(&kit, &profiles).and_then(|kit| running_action(kit, &playback));
    let Some((instance, action, frame)) = running else {
        souls.drive = DriveTrack::default();
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
    let movement_held = [
        Input::MoveForward,
        Input::MoveBackward,
        Input::StrafeLeft,
        Input::StrafeRight,
    ]
    .into_iter()
    .any(|input| bindings.pressed(input));
    let released = movement_held && action.cancel.movement.is_some_and(|from| frame >= from);

    // Root motion: owed displacement minus what the body moved along the facing.
    let (forward, left) = basis(player.facing());
    let moved = player.pos - track.last_pos;
    if moved.length() < TELEPORT_YARDS {
        track.debt -= Vec2::new(moved.dot(left), moved.dot(forward));
    }
    let motion = action.motion_at(frame);
    let step = |axis: usize| (motion[axis].0 - track.last_motion[axis].0) as f32 / 1000.0 * YARDS_PER_METRE;
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
    souls.drive = track;
    write_drive(&mut player, drive);
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
}
