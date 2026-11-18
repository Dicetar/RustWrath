//! Souls (Elden Ring–style) combat on top of the action playback ([`crate::combat_playback`]):
//! the presses, root motion, turn locks, lock-on, guard, sprint, charged heavies, hits with
//! hit-stop, and the stamina HUD.
//!
//! **Hot-togglable.** Everything here acts only while the client's *requested* mode is
//! Experimental (`F10`). Every system resets its own state the frame the mode is off, so stock
//! movement, facing, targeting and combat return untouched. Each feature also has its own switch
//! in `benilla-config/combat/souls.toml` ([`config`]), re-read every time `F10` turns the mode on.
//!
//! **Presentation-only until the server slice.** The prediction runs the same engine the server
//! will; nothing here claims damage. A landed hit is logged and, with `engage_on_hit`, starts the
//! stock melee swing on the target (`CMSG_ATTACKSWING`), so the server's own auto-attack deals
//! damage in the interim. Sprint drains the predicted bar but cannot raise the server's run speed.
//!
//! The pieces:
//!
//! - [`input`]: dodge on release (hold to sprint), light, heavy (charge on hold, release early for
//!   the uncharged swing), running attacks, guard and the guard counter, poise and hyperarmor,
//!   crouch (`F6`) and the crouch attack, jump attacks, the left-hand chain (`F11`) and the parry
//!   (`F12`). The last three need rows the kit names (`JumpLight`/`JumpHeavy`, `LeftLight1…` or
//!   `PairedLight1…`, `Parry`); without them the press does nothing.
//! - [`drive`]: the kit's root motion, no-turn windows and turn-rate caps, and the lock-on
//!   facing, written to
//!   [`crate::player::SoulsDrive`] for the controller.
//! - [`targets`]: lock-on, and the hit test of the kit's blade against attackable units.
//! - [`hud`]: the stamina bar, the mode chip and the lock-on marker.

mod config;
mod drive;
mod hud;
mod input;
mod targets;

use bevy::prelude::*;
use benilla_combat_core::{ActionId, Kit, KitAction, Milli, TickRate};
use benilla_world::schedule::WorldStage;

use crate::combat_kit::PlayerKit;
use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_playback::{
    kit_table_current, CombatPlaybackState, PlaybackSet, PROFILE_TICK_RATE_HZ,
};
use crate::combat_profiles::CombatProfiles;

pub(crate) use config::SoulsConfig;

/// Metres (the kit's unit) to yards (the world's).
const YARDS_PER_METRE: f32 = 1.093_61;

pub(crate) struct SoulsPlugin;

impl Plugin for SoulsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoulsConfig>()
            .init_resource::<SoulsState>()
            .add_systems(Startup, (config::load_souls_config, hud::spawn_hud))
            .add_systems(
                Update,
                drive::drive_controller
                    .in_set(WorldStage::Input)
                    .before(crate::player::PlayerControlSet)
                    .in_set(crate::client_state::InWorldGated),
            )
            .add_systems(
                Update,
                (
                    input::souls_input,
                    targets::toggle_lock,
                    targets::resolve_hits,
                    targets::hold_clip,
                    hud::drive_hud,
                )
                    .chain()
                    .after(PlaybackSet),
            );
    }
}

/// The Souls runtime state. [`Default`] is "nothing held, nothing locked", which every system
/// restores the frame the mode is off.
#[derive(Resource, Default, Debug)]
pub(crate) struct SoulsState {
    /// When the dodge key went down (seconds of app time), while it is held.
    dodge_down_at: Option<f32>,
    /// The current dodge hold turned into a sprint, so its release does not roll.
    dodge_sprinted: bool,
    /// Holding dodge while moving: the bar drains and light/heavy are the running attacks.
    sprinting: bool,
    /// Fractional stamina owed by the sprint drain.
    sprint_owed: f32,
    /// The charged heavy being held, if any.
    charge: Option<ChargeHold>,
    /// The guard is up.
    guarding: bool,
    /// A guard broken at zero stamina stays down until then (app seconds).
    guard_broken_until: f32,
    /// A guarded hit opens the guard counter until then (app seconds).
    counter_until: f32,
    /// The locked-on unit.
    lock: Option<Entity>,
    /// Root motion and turn-lock tracking of the running action.
    drive: drive::DriveTrack,
    /// The hit test's tracking of the running attack.
    swing: targets::SwingTrack,
    /// The guid the last landed hit started stock melee on.
    engaged: Option<u64>,
    /// The bar flashes red until then (app seconds): a press refused for stamina.
    refused_until: f32,
    /// Current poise, authored points; `None` is full.
    poise: Option<f32>,
    /// Poise refills to full this long after the last hit that took it (app seconds).
    poise_reset_at: f32,
    /// Crouched: a light press is the crouch attack.
    crouched: bool,
}

impl SoulsState {
    /// Whether this state is the reset one; the off path writes only when it is not.
    fn is_reset(&self) -> bool {
        self.dodge_down_at.is_none()
            && !self.sprinting
            && self.charge.is_none()
            && !self.guarding
            && self.lock.is_none()
            && self.drive == drive::DriveTrack::default()
            && self.swing == targets::SwingTrack::default()
            && self.engaged.is_none()
            && self.poise.is_none()
            && !self.crouched
    }
}

/// A charged heavy being held: the action pressed, its run once the engine starts it (it may be
/// held in the buffer first), and whether the key was released.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ChargeHold {
    id: ActionId,
    instance: Option<Instance>,
    released: bool,
}

/// One run of an action: its id and the playback tick it started on. Two runs of the same action
/// back to back are different instances.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Instance {
    id: ActionId,
    started: u64,
}

/// Whether Souls combat is on: the requested mode is Experimental.
fn souls_on(view: &CombatModeView) -> bool {
    view.requested == Some(CombatMode::Experimental)
}

/// The kit the profile table was compiled from, once it is current.
fn current_kit<'a>(kit: &'a PlayerKit, profiles: &CombatProfiles) -> Option<&'a Kit> {
    kit.kit.as_ref().filter(|_| kit_table_current(kit, profiles))
}

/// The running kit action: its instance, its kit row and the frame it is on.
fn running_action<'a>(
    kit: &'a Kit,
    playback: &CombatPlaybackState,
) -> Option<(Instance, &'a KitAction, Milli)> {
    let engine = playback.engine.as_ref()?;
    let id = engine.active()?;
    let now = playback.ticks_at();
    let elapsed = engine.active_elapsed(now)?;
    let action = kit.action(id)?;
    let rate = TickRate::new(PROFILE_TICK_RATE_HZ)?;
    let instance = Instance {
        id,
        started: now.0.saturating_sub(elapsed),
    };
    Some((instance, action, kit.frame_at_tick(rate, elapsed)))
}

/// Whether `frame` is inside the half-open window `[from, to)`.
fn in_window(frame: Milli, (from, to): (Milli, Milli)) -> bool {
    from <= frame && frame < to
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_are_half_open() {
        assert!(in_window(Milli(5000), (Milli(5000), Milli(24000))));
        assert!(in_window(Milli(23999), (Milli(5000), Milli(24000))));
        assert!(!in_window(Milli(24000), (Milli(5000), Milli(24000))));
        assert!(!in_window(Milli(4999), (Milli(5000), Milli(24000))));
    }
}
