//! The Souls combat state the core reads; `benilla-souls` owns the systems that write it (the
//! presses, root motion, lock-on, hits, HUD and the server link).
//!
//! The core reads it in four places: the bindings, the camera and the UI input feed stand aside
//! for [`ErControls`]; the F10 toggle reloads [`SoulsConfig`]; the animation driver poses
//! [`SoulsState::crouched`] and mutes [`SwingMute`]'s swings; the chat handler hands the server
//! link's whispers to [`tap_addon`]. [`SoulsStatePlugin`] inserts them, so the core runs the
//! same with or without the systems on top.

pub mod config;
pub mod controls;
pub mod server;

use benilla_combat_core::{ActionId, Milli};
use bevy::prelude::*;

use std::sync::atomic::{AtomicBool, Ordering};

pub use config::SoulsConfig;
pub use controls::ErControls;
pub use server::{tap_addon, SwingMute};

/// The server let our mover fly (`SMSG_MOVE_SET_CAN_FLY`, unset by `_UNSET_CAN_FLY`). The mount
/// moves (sprint, double jump) are for a mount that cannot fly. Recorded where the Wrath session
/// decodes the order, which the client does not otherwise apply.
static CAN_FLY: AtomicBool = AtomicBool::new(false);

/// Record the server's fly order for our mover.
pub fn set_can_fly(apply: bool) {
    CAN_FLY.store(apply, Ordering::Relaxed);
}

/// Whether the server lets our mover fly.
pub fn can_fly() -> bool {
    CAN_FLY.load(Ordering::Relaxed)
}

/// Inserts the Souls state, where `benilla-souls`'s `SoulsPlugin` did before it moved up; the
/// client adds that plugin right after this one.
pub struct SoulsStatePlugin;

impl Plugin for SoulsStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoulsConfig>()
            .init_resource::<SoulsState>()
            .init_resource::<SwingMute>()
            .init_resource::<ErControls>();
    }
}

/// The Souls runtime state. [`Default`] is "nothing held, nothing locked", which every system
/// restores the frame the mode is off.
#[derive(Resource, Default, Debug)]
pub struct SoulsState {
    /// When the dodge key went down (seconds of app time), while it is held.
    pub dodge_down_at: Option<f32>,
    /// The current dodge hold turned into a sprint, so its release does not roll.
    pub dodge_sprinted: bool,
    /// Holding dodge while moving: the bar drains and light/heavy are the running attacks.
    pub sprinting: bool,
    /// Fractional stamina owed by the sprint drain.
    pub sprint_owed: f32,
    /// When a movement key was last held (app seconds), for the sprint's neutral grace.
    pub last_moving_at: f32,
    /// When the last sprint ended by letting go (app seconds): a press this soon after is still
    /// the running attack.
    pub sprint_ended_at: Option<f32>,
    /// The charged heavy being held, if any.
    pub charge: Option<ChargeHold>,
    /// The guard is up.
    pub guarding: bool,
    /// A guard broken at zero stamina stays down until then (app seconds).
    pub guard_broken_until: f32,
    /// A guarded hit opens the guard counter until then (app seconds).
    pub counter_until: f32,
    /// The locked-on unit.
    pub lock: Option<Entity>,
    /// Root motion and turn-lock tracking of the running action.
    pub drive: DriveTrack,
    /// The hit test's tracking of the running attack.
    pub swing: SwingTrack,
    /// A critical action's hit is sent once via CRIT instead of its normal hit windows.
    pub critical: Option<Instance>,
    /// The guid the last landed hit started stock melee on.
    pub engaged: Option<u64>,
    /// The bar flashes red until then (app seconds): a press refused for stamina.
    pub refused_until: f32,
    /// Current poise, authored points; `None` is full.
    pub poise: Option<f32>,
    /// Poise refills to full this long after the last hit that took it (app seconds).
    pub poise_reset_at: f32,
    /// Crouched: a light press is the crouch attack.
    pub crouched: bool,
    /// The body was in the air last frame.
    pub was_airborne: bool,
    /// The jump's accepted air row, or the landing row still owed for its one press.
    pub air_attack: Option<&'static str>,
    /// Mounted on a mount that cannot fly, with the mount moves on: sprint and the double jump.
    pub mount_moves: bool,
    /// The ranged stance: the ranged weapon is drawn and light shoots it.
    pub ranged: bool,
}

impl SoulsState {
    /// The body is crouched: the animation driver poses it as the stealth crouch (StealthStand
    /// and StealthWalk). False whenever the mode or the `crouch` switch is off.
    pub fn crouched(&self) -> bool {
        self.crouched
    }

    /// Whether this state is the reset one; the off path writes only when it is not.
    pub fn is_reset(&self) -> bool {
        self.dodge_down_at.is_none()
            && !self.sprinting
            && self.charge.is_none()
            && !self.guarding
            && self.lock.is_none()
            && self.drive == DriveTrack::default()
            && self.swing == SwingTrack::default()
            && self.critical.is_none()
            && self.engaged.is_none()
            && self.poise.is_none()
            && !self.crouched
            && !self.was_airborne
            && self.air_attack.is_none()
            && !self.mount_moves
            && !self.ranged
    }
}

/// A charged heavy being held: the action pressed, its run once the engine starts it (it may be
/// held in the buffer first), and whether the key was released.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChargeHold {
    pub id: ActionId,
    pub instance: Option<Instance>,
    pub released: bool,
}

/// One run of an action: its id and the playback tick it started on. Two runs of the same action
/// back to back are different instances.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Instance {
    pub id: ActionId,
    pub started: u64,
}

/// Root-motion and turn-lock tracking of one running action.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct DriveTrack {
    pub instance: Option<Instance>,
    /// The action's cumulative `[left, up, forward]` root motion at the last frame, metres.
    pub last_motion: [Milli; 3],
    /// Root motion still owed, yards: `x` left, `y` forward.
    pub debt: Vec2,
    pub last_pos: Vec3,
    /// The facing held through the current no-turn window.
    pub held_yaw: Option<f32>,
}

/// The hit test's tracking of one running attack.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SwingTrack {
    pub instance: Option<Instance>,
    /// The frame the last test ran at; windows between it and now are caught up.
    pub last_frame: Option<Milli>,
    /// `(hit index, unit)` pairs already struck by this run.
    pub struck: Vec<(usize, Entity)>,
}
