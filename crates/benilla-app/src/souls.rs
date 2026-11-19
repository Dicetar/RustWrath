//! Souls (Elden Ring–style) combat on top of the action playback ([`crate::combat_playback`]):
//! the presses, root motion, turn locks, lock-on, guard, sprint, charged heavies, hits with
//! hit-stop, and the stamina HUD.
//!
//! **Hot-togglable.** Everything here acts only while the client's *requested* mode is
//! Experimental (`F10`). Every system resets its own state the frame the mode is off, so stock
//! movement, facing, targeting and combat return untouched. Each feature also has its own switch
//! in `benilla-config/combat/souls.toml` ([`config`]), re-read every time `F10` turns the mode on.
//!
//! **The server half** is an ALE Lua script, `integrations/ale/rustwrath_souls.lua`, which the owner
//! copies into the server's `lua_scripts`. While it answers ([`server`]), landed hits are reported
//! to it for Souls damage (cleave and riposte included), sprint raises the run speed, and enemy
//! melee becomes telegraphed strikes the client judges (dodge, guard, parry or hit). Without it a
//! landed hit falls back to `engage_on_hit`: the stock melee swing starts on the target
//! (`CMSG_ATTACKSWING`), so the server's own auto-attack deals damage.
//!
//! The pieces:
//!
//! - [`input`]: dodge on release (hold to sprint), light, heavy (charge on hold, release early for
//!   the uncharged swing), running attacks, guard and the guard counter, poise and hyperarmor,
//!   crouch (`X`) and the crouch attack, jump attacks, the left-hand chain (`Mouse5`) and the parry
//!   (`Shift+MouseRight`). The last three need rows the kit names (`JumpLightLand`/
//!   `JumpHeavyLand`, played on landing; `LeftLight1…` or `PairedLight1…`; `Parry`); without them the press does nothing.
//! - [`drive`]: the kit's root motion, no-turn windows and turn-rate caps, and the lock-on
//!   facing, written to
//!   [`crate::player::SoulsDrive`] for the controller.
//! - [`controls`]: the Elden Ring layout: the mouse orbits with the cursor hidden and the buttons
//!   attack, the cursor key (`LeftCtrl`) brings the cursor up for the hotbars and panels, the
//!   layout's keys override their WoW bindings, and `E` interacts with what is in front.
//! - [`targets`]: lock-on, and the hit test of the kit's blade against attackable units.
//! - [`server`]: the link to the server script: mode, hit reports, sprint, enemy strikes.
//! - [`hud`]: the stamina bar, the mode chip, the lock-on marker and the strike bars.
//! - [`retarget`]: the Elden Ring clips on the player's own rig: each kit action plays the clip it
//!   was read from, and (on its own switch) locomotion plays the ER idle, walk, run, sprint,
//!   crouch and airborne loops.

mod config;
mod controls;
mod drive;
mod hud;
mod input;
mod retarget;
mod server;
mod targets;

use benilla_combat_core::{ActionId, Kit, KitAction, Milli, TickRate};
use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use crate::combat_kit::PlayerKit;
use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_playback::{
    kit_table_current, CombatPlaybackState, PlaybackSet, PROFILE_TICK_RATE_HZ,
};
use crate::combat_profiles::CombatProfiles;

pub(crate) use config::SoulsConfig;
pub(crate) use controls::ErControls;
pub(crate) use server::{tap_addon, SwingMute};

/// Metres (the kit's unit) to yards (the world's).
const YARDS_PER_METRE: f32 = 1.093_61;

pub(crate) struct SoulsPlugin;

impl Plugin for SoulsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoulsConfig>()
            .init_resource::<SoulsState>()
            .init_resource::<server::SoulsLink>()
            .init_resource::<SwingMute>()
            .init_resource::<ErControls>()
            .add_systems(Startup, (config::load_souls_config, hud::spawn_hud))
            .add_systems(
                Update,
                controls::claim_inputs.before(crate::ui_script::UiInput),
            )
            .add_systems(
                Update,
                controls::interact_key
                    .in_set(WorldStage::Input)
                    .in_set(crate::client_state::InWorldGated),
            )
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
                    input::grip_toggle,
                    input::souls_input,
                    targets::toggle_lock,
                    targets::lock_follows_target,
                    targets::resolve_hits,
                    server::drive_link,
                    targets::hold_clip,
                    hud::drive_hud,
                )
                    .chain()
                    .after(PlaybackSet),
            );
        retarget::plugin(app);
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
    /// When a movement key was last held (app seconds), for the sprint's neutral grace.
    last_moving_at: f32,
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
    /// A critical action's hit is sent once via CRIT instead of its normal hit windows.
    critical: Option<Instance>,
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
    /// The body was in the air last frame.
    was_airborne: bool,
    /// A press made in the air: the kit row of the landing swing it plays on touching down.
    air_attack: Option<&'static str>,
}

impl SoulsState {
    /// The body is crouched: the animation driver poses it as the stealth crouch (StealthStand
    /// and StealthWalk). False whenever the mode or the `crouch` switch is off.
    pub(crate) fn crouched(&self) -> bool {
        self.crouched
    }

    /// Whether this state is the reset one; the off path writes only when it is not.
    fn is_reset(&self) -> bool {
        self.dodge_down_at.is_none()
            && !self.sprinting
            && self.charge.is_none()
            && !self.guarding
            && self.lock.is_none()
            && self.drive == drive::DriveTrack::default()
            && self.swing == targets::SwingTrack::default()
            && self.critical.is_none()
            && self.engaged.is_none()
            && self.poise.is_none()
            && !self.crouched
            && !self.was_airborne
            && self.air_attack.is_none()
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
    kit.kit
        .as_ref()
        .filter(|_| kit_table_current(kit, profiles))
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
