//! The ranged stance, Elden Ring style: the ranged key draws the ranged-slot weapon (a bow, gun,
//! crossbow, thrown weapon or wand) and puts it away again; while it is drawn, light shoots it.
//!
//! - **Drawing** is the stock sheath order (`CMSG_SETSHEATHED` 2 ranged, 1 back to melee), so the
//!   weapon shows in hand for everyone and the stock draw clip plays.
//! - **A shot** is the stock ranged spell the server knows for that weapon, cast at the target:
//!   Wand Shoot (5019) for a wand, Throw (2764) for a thrown weapon, Auto Shot (75) for a hunter's
//!   bow, gun or crossbow, Shoot (3018) for anyone else's. The server owns range, ammunition,
//!   line of sight, the cast and the damage, and refuses what does not hold.
//! - **The target** is the lock-on target, else the WoW target, else the nearest attackable unit
//!   in front of the camera ([`AIM_DEGREES`] either side, [`RANGE_YARDS`] out). Without a lock the
//!   body faces the camera while the stance is up ([`super::drive`]), so what the camera aims at
//!   is in front.
//! - **Stamina**: each shot costs [`SHOT_POINTS`] kit points, as an Elden Ring bow draw does; with
//!   too little the shot is refused and the bar flashes.
//!
//! Mounted, or with the ranged weapon gone, the stance drops. The melee presses are off while it is
//! up (the dodge still rolls); [`super::input`] reads [`SoulsState::ranged`].

use bevy::prelude::*;

use super::input::KeyRead;
use super::targets::{attackable, off_facing, Units};
use super::{current_kit, souls_on, ErControls, SoulsConfig, SoulsState};
use crate::combat_kit::{PlayerKit, STAMINA_SCALE};
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::creature_anim::Wielded;
use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfPlayer};
use crate::player::camera::FlyCam;
use crate::player::Player;
use crate::target::{RelationWorld, Selection};
use crate::ui_script::UiKeyboardCapture;

/// `ITEM_CLASS_WEAPON`.
const ITEM_CLASS_WEAPON: u8 = 2;
/// Weapon subclasses (`ItemSubClassWeapon`): bow, gun, thrown, crossbow, wand.
const BOW: u8 = 2;
const GUN: u8 = 3;
const THROWN: u8 = 16;
const CROSSBOW: u8 = 18;
const WAND: u8 = 19;
/// `CLASS_HUNTER`.
const CLASS_HUNTER: u8 = 3;
/// The stock ranged spells (3.3.5a `Spell.dbc`).
const SPELL_AUTO_SHOT: u32 = 75;
const SPELL_SHOOT: u32 = 3018;
const SPELL_THROW: u32 = 2764;
const SPELL_WAND_SHOOT: u32 = 5019;
/// `CMSG_SETSHEATHED` states.
const SHEATH_MELEE: u32 = 1;
const SHEATH_RANGED: u32 = 2;
/// The aim picks a unit this far out at most (the stock ranged spells reach 30 to 35 yards).
const RANGE_YARDS: f32 = 35.0;
/// ...and this many degrees either side of the camera's line.
const AIM_DEGREES: f32 = 25.0;
/// A shot's stamina, kit points (authored; ER's short bow draws cost about this).
const SHOT_POINTS: u32 = 10;
/// The bar flashes red this long after a shot refused for stamina.
const REFUSAL_FLASH_SECONDS: f32 = 0.4;
/// One shot per this long at most: a held or bounced key does not spam the server.
const SHOT_INTERVAL_SECONDS: f32 = 0.35;

/// The spell that shoots `ranged` (its `(class, subclass)`) for a character of `class`, or `None`
/// for anything that is not a ranged weapon.
pub(super) fn shot_spell(ranged: Option<(u8, u8)>, class: Option<u8>) -> Option<u32> {
    let (item_class, subclass) = ranged?;
    if item_class != ITEM_CLASS_WEAPON {
        return None;
    }
    match subclass {
        WAND => Some(SPELL_WAND_SHOOT),
        THROWN => Some(SPELL_THROW),
        BOW | GUN | CROSSBOW if class == Some(CLASS_HUNTER) => Some(SPELL_AUTO_SHOT),
        BOW | GUN | CROSSBOW => Some(SPELL_SHOOT),
        _ => None,
    }
}

/// The stance key, the stance's upkeep and the shots.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn ranged_stance(
    raw: (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<UiKeyboardCapture>,
        Res<ErControls>,
    ),
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    player: Res<Player>,
    cams: Query<&FlyCam>,
    me: Query<(&Wielded, &ObjectStore), With<SelfPlayer>>,
    aim: (Units, RelationWorld, Res<Selection>),
    net: Option<Res<NetCommands>>,
    mut playback: ResMut<CombatPlaybackState>,
    mut souls: ResMut<SoulsState>,
    mut last_shot: Local<f32>,
) {
    let (keys, mouse, capture, er) = raw;
    let send = |command: ClientCommand| {
        if let Some(net) = net.as_deref() {
            if net.0.send(command).is_err() {
                warn!("souls ranged: the network thread is gone");
            }
        }
    };
    let Ok((wielded, store)) = me.single() else {
        return;
    };
    let spell = shot_spell(wielded.ranged, store.0.unit_class());
    let mounted = store.0.unit_mount_display_id() != 0;
    let usable = souls_on(&view) && config.features.ranged && spell.is_some() && !mounted;
    if !usable {
        if souls.ranged {
            souls.ranged = false;
            info!("souls ranged: stance dropped");
            if souls_on(&view) {
                send(ClientCommand::SetSheathed {
                    state: SHEATH_MELEE,
                });
            }
        }
        return;
    }
    let input = KeyRead::new(&keys, &mouse, &capture, &er);
    if input.just_pressed(config.binds.ranged) {
        souls.ranged = !souls.ranged;
        let state = if souls.ranged {
            SHEATH_RANGED
        } else {
            SHEATH_MELEE
        };
        info!(
            "souls ranged: {}",
            if souls.ranged { "drawn" } else { "put away" }
        );
        send(ClientCommand::SetSheathed { state });
        return;
    }
    let Some(spell) = spell.filter(|_| souls.ranged) else {
        return;
    };
    let t = time.elapsed_secs();
    if !input.just_pressed(config.binds.light) || t - *last_shot < SHOT_INTERVAL_SECONDS {
        return;
    }
    let (units, relations, selection) = aim;
    let in_range = |at: Vec3| (at - player.pos).xz().length() <= RANGE_YARDS;
    let locked = souls.lock.and_then(|entity| units.get(entity).ok());
    let selected = selection.target.and_then(|entity| units.get(entity).ok());
    let target = locked
        .or(selected)
        .filter(|(_, guid, store, at)| {
            attackable(&relations, guid.0, store) && in_range(at.translation())
        })
        .map(|(_, guid, ..)| guid.0)
        .or_else(|| {
            let line = match cams.single() {
                Ok(cam) if er.free_camera => cam.yaw(),
                _ => player.facing(),
            };
            let cone = AIM_DEGREES.to_radians();
            units
                .iter()
                .filter_map(|(_, guid, store, at)| {
                    let to = at.translation() - player.pos;
                    (attackable(&relations, guid.0, store)
                        && in_range(at.translation())
                        && off_facing(line, to).abs() <= cone)
                        .then_some((to.xz().length(), guid.0))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                .map(|(_, guid)| guid)
        });
    let Some(target) = target else {
        info!("souls ranged: nothing to shoot in front");
        return;
    };
    // The draw's stamina, scaled like the kit's own costs.
    let scale = if current_kit(&kit, &profiles).is_some() {
        STAMINA_SCALE.unsigned_abs()
    } else {
        1
    };
    let cost = SHOT_POINTS.saturating_mul(scale);
    let now = playback.ticks_at();
    if let Some(engine) = playback.engine.as_mut() {
        let stamina = engine.stamina();
        if stamina < cost {
            souls.refused_until = t + REFUSAL_FLASH_SECONDS;
            return;
        }
        engine.set_stamina(stamina - cost, now);
    }
    *last_shot = t;
    info!("souls ranged: shot {spell} at {target:#x}");
    send(ClientCommand::CastSpell {
        spell_id: spell,
        target: Some(target),
        trace: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each ranged weapon shoots with its own stock spell; a hunter's bow, gun or crossbow is Auto
    /// Shot, and a melee weapon or an empty slot shoots nothing.
    #[test]
    fn each_ranged_weapon_picks_its_stock_shot() {
        let warrior = Some(1);
        let hunter = Some(CLASS_HUNTER);
        assert_eq!(shot_spell(Some((2, WAND)), Some(8)), Some(SPELL_WAND_SHOOT));
        assert_eq!(shot_spell(Some((2, THROWN)), warrior), Some(SPELL_THROW));
        for bow in [BOW, GUN, CROSSBOW] {
            assert_eq!(shot_spell(Some((2, bow)), warrior), Some(SPELL_SHOOT));
            assert_eq!(shot_spell(Some((2, bow)), hunter), Some(SPELL_AUTO_SHOT));
        }
        assert_eq!(shot_spell(Some((2, 7)), warrior), None, "a sword");
        assert_eq!(shot_spell(Some((4, 1)), warrior), None, "armour");
        assert_eq!(shot_spell(None, hunter), None);
    }
}
