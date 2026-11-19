//! The local player's Souls kit: the Elden Ring moveset's timings, read from a local file and
//! compiled into the prediction engine's table in place of the stock-clip profiles (work plan
//! step E, loaded; `docs/architecture/souls-combat-client-authority.md`).
//!
//! **The file.** `benilla-config/combat/player.kit` ([`crate::local_state::player_kit_path`]), in
//! the `rustwrath-kit 1` format ([`Kit`]). The owner makes it with `import_kit` from data they
//! generated from their own copy of the game; it is never committed. It is read at startup and
//! again on an equipped main-hand subclass change or Experimental request (`F10`), so a new kit takes effect without a
//! restart. No file, or one that does not read or compile, leaves the stock-clip profiles in
//! place, and the reason is logged.
//!
//! **Presses** ([`resolve_press`]). A light press continues the running action's chain (its
//! `next_light`: the next light, or a roll's or backstep's attack) and otherwise opens with
//! `Light1`. A dodge press rolls in the held direction, forward and back before a strafe, and
//! backsteps with no direction held, as the donor does without lock-on.
//!
//! **Stamina** ([`engine_stamina`]). The engine counts whole points per tick. The kit's costs and
//! bar are scaled by [`STAMINA_SCALE`] so the reference's regeneration, 45 points a second, is a
//! whole number per tick at the compile rate. Nothing displays the bar, so the scale is not
//! visible.
//!
//! **Not driven yet.** The clips are the WoW rig's own Dodge and swing until the retarget (step
//! F) plays each action's `source`. Root motion, turn locks, hits after the first, per-hit
//! stamina timing, the charge window, heavy presses and guard are carried in the kit and not
//! used.

use std::collections::HashMap;

use benilla_combat_core::{
    ActionId, ActionKind, ActionRevision, ActionTable, Kit, KitError, Milli, StaminaConfig,
    TickRate,
};
use bevy::prelude::*;

use crate::creature_anim::move_flags;

/// Kit stamina points become this many engine points: 45 points a second at 100 Hz is 9 a tick.
pub(crate) const STAMINA_SCALE: i32 = 20;
/// Regeneration in kit points a second. The Elden Ring reference's own estimate (its
/// `STAMINA_REGEN`, marked ESTIMATE there), taken as project policy, not extracted data.
const REGEN_PER_SECOND: u64 = 45;
/// The action a light press opens a chain with.
const LIGHT_OPENER: &str = "Light1";

/// The local player's kit as last read; `None` when there is no usable file.
#[derive(Resource, Debug, Default)]
pub(crate) struct PlayerKit {
    pub(crate) kit: Option<Kit>,
    /// Bumped whenever a read changes [`Self::kit`], so the profile build compiles the new one.
    pub(crate) generation: u32,
    selected_main: Option<(u8, u8)>,
    selected_weapon: u64,
}

impl PlayerKit {
    /// Read the file again; a changed result bumps the generation.
    pub(crate) fn reload(&mut self) {
        let kit = read_kit(self.selected_main);
        if kit != self.kit {
            self.kit = kit;
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

/// The startup read.
pub(crate) fn load_player_kit(mut kit: ResMut<PlayerKit>) {
    kit.reload();
}

/// One mapping for the client and import-kits.ps1. Names are from the reference WEAPONS table;
/// staff uses Twinblade (there is no staff entry).
const WEAPON_KITS: &str = include_str!("../../../scripts/combat-kits.csv");

fn weapon_kit(main: Option<(u8, u8)>) -> Option<&'static str> {
    let (2, subclass) = main? else { return None };
    WEAPON_KITS.lines().skip(1).find_map(|row| {
        let mut fields = row.split(',');
        let id = fields.next()?.parse::<u8>().ok()?;
        let name = fields.next()?;
        (id == subclass).then_some(name)
    })
}

/// Equipment changes use the same reload as F10; disarming does not change the equipped kit.
pub(crate) fn select_player_kit(
    me: Query<
        (&crate::creature_anim::Wielded, &crate::net::ObjectStore),
        With<crate::net::SelfPlayer>,
    >,
    inventory: Option<Res<crate::wrath_inventory::WrathInventory>>,
    mut kit: ResMut<PlayerKit>,
) {
    let body = me.single().ok();
    let main = body.and_then(|(wielded, _)| wielded.main);
    let weapon = inventory
        .as_deref()
        .map(|inventory| inventory.equipment(crate::items::EQUIPMENT_SLOT_MAINHAND))
        .or_else(|| {
            body.and_then(|(_, store)| {
                store
                    .0
                    .player_inv_slot(crate::items::EQUIPMENT_SLOT_MAINHAND)
            })
        })
        .unwrap_or(0);
    if main != kit.selected_main || weapon != kit.selected_weapon {
        kit.selected_main = main;
        kit.selected_weapon = weapon;
        kit.reload();
    }
}

/// A missing subclass file falls back to player.kit, then to stock profiles.
fn read_kit(main: Option<(u8, u8)>) -> Option<Kit> {
    let fallback = crate::local_state::player_kit_path()?;
    read_kit_at(main, &fallback)
}

fn read_kit_at(main: Option<(u8, u8)>, fallback: &std::path::Path) -> Option<Kit> {
    let path = weapon_kit(main)
        .and_then(|name| {
            fallback
                .parent()
                .map(|p| p.join("kits").join(format!("{name}.kit")))
        })
        .unwrap_or_else(|| fallback.to_path_buf());
    read_kit_file(&path).or_else(|| {
        (path != fallback)
            .then(|| read_kit_file(fallback))
            .flatten()
    })
}

fn read_kit_file(path: &std::path::Path) -> Option<Kit> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            info!(
                "souls kit: none at {}; the stock-clip profiles stay",
                path.display()
            );
            return None;
        }
        Err(error) => {
            warn!("souls kit: {} could not be read: {error}", path.display());
            return None;
        }
    };
    match Kit::parse(&text) {
        Ok(kit) => {
            info!(
                "souls kit: {} — {} ({} actions)",
                path.display(),
                kit.label,
                kit.actions.len()
            );
            Some(kit)
        }
        Err(error) => {
            warn!("souls kit: {} is not a usable kit: {error}", path.display());
            None
        }
    }
}

/// A Souls press, before the kit names the action it starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Press {
    Light,
    Dodge,
}

/// The kit action `press` asks for, given the running action and the mover's `MOVEMENTFLAGS`.
///
/// - Light: the running action's `next_light`, else [`LIGHT_OPENER`], else the kit's first light
///   attack.
/// - Dodge: the roll in the held direction (forward or back before a strafe), or the backstep
///   with no direction held; a kit without that one gives its first dodge.
pub(crate) fn resolve_press(
    kit: &Kit,
    running: Option<ActionId>,
    press: Press,
    flags: u32,
) -> Option<ActionId> {
    let first_of = |kind: ActionKind| {
        kit.actions
            .iter()
            .find(|action| action.kind == kind)
            .map(|action| action.id)
    };
    match press {
        Press::Light => running
            .and_then(|id| kit.action(id))
            .and_then(|action| action.next_light)
            .or_else(|| kit.action_named(LIGHT_OPENER).map(|action| action.id))
            .or_else(|| first_of(ActionKind::MeleeLight)),
        Press::Dodge => kit
            .action_named(dodge_name(flags))
            .map(|action| action.id)
            .or_else(|| first_of(ActionKind::Dodge)),
    }
}

/// The kit dodge for the held direction.
fn dodge_name(flags: u32) -> &'static str {
    if flags & move_flags::FORWARD != 0 {
        "RollFront"
    } else if flags & move_flags::BACKWARD != 0 {
        "RollBack"
    } else if flags & move_flags::STRAFE_LEFT != 0 {
        "RollLeft"
    } else if flags & move_flags::STRAFE_RIGHT != 0 {
        "RollRight"
    } else {
        "Backstep"
    }
}

/// Compile `kit` into an engine table at `rate`, its stamina costs scaled by [`STAMINA_SCALE`].
pub(crate) fn compile_kit(
    kit: &Kit,
    revision: ActionRevision,
    rate: TickRate,
) -> Result<ActionTable, KitError> {
    let scale = |points: Milli| Milli(points.0.saturating_mul(STAMINA_SCALE));
    let mut scaled = kit.clone();
    for action in &mut scaled.actions {
        action.stamina = scale(action.stamina);
        for hit in &mut action.hits {
            hit.stamina = scale(hit.stamina);
        }
    }
    scaled.to_table(revision, rate)
}

/// The engine's stamina for `kit` at `rate`: the kit's bar (or `fallback`'s, in kit points) and
/// [`REGEN_PER_SECOND`], both scaled by [`STAMINA_SCALE`]. The regeneration delay is
/// `fallback`'s; the donor has none and instead stops regenerating while an action runs, which
/// the engine does not model.
pub(crate) fn engine_stamina(kit: &Kit, rate: TickRate, fallback: StaminaConfig) -> StaminaConfig {
    let scale = u64::from(STAMINA_SCALE.unsigned_abs());
    let max = match kit.stamina_max {
        Some(points) => (u64::from(points.0.max(0).unsigned_abs()) * scale + 500) / 1000,
        None => u64::from(fallback.max) * scale,
    };
    let hz = u64::from(rate.hz());
    let regen = (REGEN_PER_SECOND * scale + hz / 2) / hz;
    StaminaConfig {
        max: u32::try_from(max.max(1)).unwrap_or(u32::MAX),
        regen_per_tick: u32::try_from(regen.max(1)).unwrap_or(u32::MAX),
        regen_delay_ticks: fallback.regen_delay_ticks,
    }
}

/// `Parry1H`, the stand-in for the kit's authored `Parry` (every humanoid rig has it).
const ANIM_PARRY_1H: u16 = 21;

/// Each kit action bound to the rig clip that stands in for it until the retarget: the parry to
/// `Parry1H`, dodges to the rig's Dodge clip, attacks to its main-hand swing. An action whose
/// stand-in the rig lacks plays no clip; the engine still runs it.
pub(crate) fn clip_bindings(
    kit: &Kit,
    dodge_clip: Option<u16>,
    swing_clip: Option<u16>,
) -> HashMap<ActionId, u16> {
    kit.actions
        .iter()
        .filter_map(|action| {
            let clip = match action.kind {
                ActionKind::Dodge if action.name == "Parry" => Some(ANIM_PARRY_1H),
                ActionKind::Dodge => dodge_clip,
                _ => swing_clip,
            };
            clip.map(|clip| (action.id, clip))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_weapon_kit_falls_back_to_player_then_stock() {
        let temp = std::env::temp_dir().join(format!(
            "rustwrath-kits-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(temp.join("kits")).unwrap();
        let fallback = temp.join("player.kit");
        assert!(read_kit_at(Some((2, 15)), &fallback).is_none());
        std::fs::write(&fallback, KIT).unwrap();
        assert_eq!(
            read_kit_at(Some((2, 15)), &fallback).unwrap().label,
            "test kit"
        );
        std::fs::write(
            temp.join("kits/dagger.kit"),
            KIT.replace("label test kit", "label dagger"),
        )
        .unwrap();
        assert_eq!(
            read_kit_at(Some((2, 15)), &fallback).unwrap().label,
            "dagger"
        );
        assert_eq!(
            read_kit_at(Some((2, 7)), &fallback).unwrap().label,
            "test kit"
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn weapon_subclasses_pick_the_import_table() {
        for (subclass, name) in [
            (0, "axe"),
            (1, "axe-two"),
            (4, "mace"),
            (5, "mace-two"),
            (6, "polearm"),
            (7, "sword"),
            (8, "sword-two"),
            (10, "staff"),
            (13, "fist"),
            (15, "dagger"),
            (17, "spear"),
        ] {
            assert_eq!(weapon_kit(Some((2, subclass))), Some(name));
        }
        assert_eq!(weapon_kit(Some((2, 2))), None);
        assert_eq!(weapon_kit(Some((4, 7))), None);
        assert_eq!(weapon_kit(None), None);
        for row in WEAPON_KITS.lines().skip(1) {
            let fields: Vec<_> = row.split(',').collect();
            assert_eq!(fields.len(), 4);
            let subclass: u8 = fields[0].parse().unwrap();
            assert_eq!(
                fields[3] == "two",
                matches!(subclass, 1 | 5 | 6 | 8 | 10 | 17)
            );
        }
    }

    /// An authored kit: a backstep, two rolls and a two-step light chain, at 30 frames a second.
    /// No game value.
    const KIT: &str = "rustwrath-kit 1
label test kit
frame_rate 30
stamina_max 50
action 100 dodge
  name Backstep
  source a000
  total 30
  input 15 15
  cancel 20 20 20 never 20 20 never
  stamina 4
  iframes 0 6
  next_light 104
end
action 101 dodge
  name RollFront
  source a001
  total 30
  input 15 15
  cancel 20 20 20 never 20 20 never
  stamina 6
  iframes 2 10
end
action 102 dodge
  name RollLeft
  source a002
  total 30
  input 15 15
  cancel 20 20 20 never 20 20 never
  stamina 6
  iframes 2 10
end
action 103 light
  name Light1
  source a003
  total 30
  input 6 6
  cancel 15 never 12 never 15 15 never
  stamina 0
  next_light 104
  hit 8 11 1 1 5 0 0.2
end
action 104 light
  name Light2
  source a004
  total 30
  input 6 6
  cancel 15 never 12 never 15 15 never
  stamina 0
  next_light 103
  hit 8 11 1 1 5 0 0.2
end
";

    fn kit() -> Kit {
        Kit::parse(KIT).expect("the authored kit reads")
    }

    /// A light press continues the running action's chain and otherwise opens it; a dodge press
    /// follows the held direction, backsteps with none, and falls back to the first dodge when
    /// the kit lacks the asked-for roll.
    #[test]
    fn presses_resolve_through_the_chain_and_the_held_direction() {
        let kit = kit();
        let light = |running| resolve_press(&kit, running, Press::Light, 0);
        assert_eq!(light(None), Some(ActionId(103)), "the opener");
        assert_eq!(light(Some(ActionId(103))), Some(ActionId(104)), "the chain");
        assert_eq!(
            light(Some(ActionId(104))),
            Some(ActionId(103)),
            "the chain wraps"
        );
        assert_eq!(
            light(Some(ActionId(100))),
            Some(ActionId(104)),
            "the backstep's attack"
        );
        assert_eq!(
            light(Some(ActionId(101))),
            Some(ActionId(103)),
            "no follow-up: the opener"
        );

        let dodge = |flags| resolve_press(&kit, None, Press::Dodge, flags);
        assert_eq!(dodge(0), Some(ActionId(100)), "no direction: backstep");
        assert_eq!(
            dodge(move_flags::FORWARD | move_flags::STRAFE_LEFT),
            Some(ActionId(101))
        );
        assert_eq!(dodge(move_flags::STRAFE_LEFT), Some(ActionId(102)));
        assert_eq!(
            dodge(move_flags::BACKWARD),
            Some(ActionId(100)),
            "no RollBack in this kit: its first dodge"
        );
    }

    /// The compiled table carries the scaled costs, and the stamina config scales the bar and
    /// makes 45 points a second a whole number per tick.
    #[test]
    fn the_kit_compiles_with_scaled_stamina() {
        let kit = kit();
        let rate = TickRate::new(100).expect("100 Hz");
        let table = compile_kit(&kit, ActionRevision(1), rate).expect("compiles");
        let roll = table.get(ActionId(101)).expect("the roll");
        assert_eq!(roll.stamina_cost, 6 * 20);
        let swing = table.get(ActionId(103)).expect("the swing");
        assert_eq!(swing.stamina_cost, 5 * 20, "an attack costs its hits");
        assert_eq!(roll.total_ticks, 100, "30 frames at 30 fps is one second");

        let fallback = StaminaConfig {
            max: 100,
            regen_per_tick: 1,
            regen_delay_ticks: 100,
        };
        let stamina = engine_stamina(&kit, rate, fallback);
        assert_eq!(stamina.max, 50 * 20);
        assert_eq!(
            stamina.regen_per_tick, 9,
            "45 points a second at 100 Hz, scaled by 20"
        );
        assert_eq!(stamina.regen_delay_ticks, 100);

        let bindings = clip_bindings(&kit, Some(30), Some(17));
        assert_eq!(bindings.get(&ActionId(100)), Some(&30));
        assert_eq!(bindings.get(&ActionId(104)), Some(&17));
        assert!(clip_bindings(&kit, None, Some(17))
            .get(&ActionId(101))
            .is_none());
    }
}
