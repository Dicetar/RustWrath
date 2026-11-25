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
//!   (`Shift+MouseRight`). Jump attacks start their air row and play its landing animation
//!   on touchdown. Older kits with only a landing row start the action on touchdown.
//!   The left-hand chain and parry use the kit's `LeftLight1…`, `PairedLight1…` and `Parry` rows.
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

mod controls;
mod drive;
mod hud;
mod input;
mod ranged;
mod retarget;
mod server;
mod targets;

use benilla_combat_core::{ActionKind, Kit, KitAction, Milli, TickRate};
use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use crate::combat_kit::PlayerKit;
use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_playback::{
    kit_table_current, CombatPlaybackState, PlaybackSet, PROFILE_TICK_RATE_HZ,
};
use crate::combat_profiles::CombatProfiles;

// The core modules the systems name, imported at the root so their `crate::…` paths read as they
// did inside `benilla-app`: the boot's public modules, and the rest through the core's narrow
// `souls_seam` re-exports.
use benilla_app::souls_seam::{
    aura_visual, bindings, camera_shake, combat_kit, combat_mode, combat_playback, combat_profiles,
    creature_anim, entities, names, net, player, query_cache, target, ui_script, vplates,
};
use benilla_app::{client_state, local_state};

/// The Wrath session's selection, under the path the server link names it by.
mod wrath {
    pub(crate) use benilla_app::wrath_selected_from_env as selected_from_env;
}

use benilla_app::souls::config;
pub(crate) use benilla_app::souls::{ChargeHold, ErControls, Instance, SoulsConfig, SoulsState};

/// Metres (the kit's unit) to yards (the world's).
const YARDS_PER_METRE: f32 = 1.093_61;

pub struct SoulsPlugin;

impl Plugin for SoulsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<server::SoulsLink>()
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
                    ranged::ranged_stance,
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
    let frame = presented_frame(
        kit.frame_at_tick(rate, elapsed),
        playback.attack_haste,
        action.kind,
    );
    Some((instance, action, frame))
}

/// The frame a running action is presented at. Rage's ER bonus scales melee frames, so the swing
/// animation and its hit contact come sooner together; rolls, parries and every other action keep
/// the authored speed (their windows are engine ticks, and hasting those would desync them).
fn presented_frame(frame: Milli, haste: Option<u32>, kind: ActionKind) -> Milli {
    match haste {
        Some(per_mille) if matches!(kind, ActionKind::MeleeLight | ActionKind::MeleeHeavy) => {
            Milli((i64::from(frame.0) * i64::from(per_mille) / 1000) as i32)
        }
        _ => frame,
    }
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

    /// Rage's ER bonus scales melee frames only: 1000 per-mille is authored speed, 1150 is +15 %,
    /// and a dodge or parry keeps the authored frame so its engine-tick windows do not move.
    #[test]
    fn melee_frames_are_hasted_and_other_actions_are_not() {
        assert_eq!(
            presented_frame(Milli(1_000), None, ActionKind::MeleeLight),
            Milli(1_000)
        );
        assert_eq!(
            presented_frame(Milli(1_000), Some(1_000), ActionKind::MeleeLight),
            Milli(1_000)
        );
        assert_eq!(
            presented_frame(Milli(1_000), Some(1_150), ActionKind::MeleeHeavy),
            Milli(1_150)
        );
        assert_eq!(
            presented_frame(Milli(1_000), Some(1_150), ActionKind::Dodge),
            Milli(1_000)
        );
    }
}
