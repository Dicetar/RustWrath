//! Lock-on and the Souls hit test.
//!
//! - **Lock-on.** The lock key toggles a lock: the selected unit when it is attackable, alive and
//!   within [`LOCK_ACQUIRE_YARDS`], else the nearest such unit in front of the camera-facing body.
//!   The lock breaks past [`LOCK_BREAK_YARDS`], on death, or when the unit leaves the world.
//!   With `lock_targets` the lock is the WoW target ([`lock_follows_target`]): a new lock
//!   selects its unit, and a Tab or click onto another attackable unit moves the lock to it.
//! - **Hits.** Inside each kit hit window (frames the frame clock skipped are caught up), the
//!   kit's blade gives the reach (the farthest blade end from the body, plus the capsule radius)
//!   and the arc (the widest blade bearing, clamped). An attackable, living unit within the reach
//!   (plus its own combat reach), inside the arc and roughly level is hit, once per hit per unit.
//!   A hit is logged; with `hit_stop` the playback clock and the player's clips freeze for the
//!   kit's hit-stop and the camera jolts. With the server script answering
//!   ([`super::server`]), each window's struck units go to it, nearest first, for Souls damage;
//!   without it, `engage_on_hit` starts the stock melee swing on the target instead.

use std::time::Duration;

use benilla_combat_core::{KitHit, Milli};
use bevy::prelude::*;

use super::drive::yaw_toward;
use super::input::KeyRead;
use super::server::SoulsLink;
use super::{
    current_kit, running_action, souls_on, ErControls, Instance, SoulsConfig, SoulsState,
    YARDS_PER_METRE,
};
use crate::aura_visual::AnimRateFreeze;
use crate::camera_shake::CameraShakes;
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::net::{Guid, NetCommands, ObjectStore, SelfPlayer};
use crate::player::camera::FlyCam;
use crate::player::Player;
use crate::target::{RelationWorld, SelectCommit, Selection};
use crate::ui_script::UiKeyboardCapture;

/// A lock takes a unit this close.
const LOCK_ACQUIRE_YARDS: f32 = 30.0;
/// With no attackable selection, the nearest unit in front this close is taken.
const LOCK_NEAREST_YARDS: f32 = 20.0;
/// "In front": within this many degrees of the facing.
const LOCK_FRONT_DEGREES: f32 = 70.0;
/// A lock breaks past this.
const LOCK_BREAK_YARDS: f32 = 40.0;
/// A unit more than this far above or below is out of the swing.
const HIT_HEIGHT_YARDS: f32 = 3.0;
/// The arc's clamp, degrees either side of the facing.
const ARC_MIN_DEGREES: f32 = 30.0;
const ARC_MAX_DEGREES: f32 = 100.0;
/// The reach of a hit with no blade samples, metres.
const BARE_REACH_METRES: f32 = 1.5;

/// The hit test's tracking of one running attack.
#[derive(Clone, PartialEq, Debug, Default)]
pub(super) struct SwingTrack {
    instance: Option<Instance>,
    /// The frame the last test ran at; windows between it and now are caught up.
    last_frame: Option<Milli>,
    /// `(hit index, unit)` pairs already struck by this run.
    struck: Vec<(usize, Entity)>,
}

/// The units the lock and the hit test consider.
pub(super) type Units<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Guid,
        &'static ObjectStore,
        &'static GlobalTransform,
    ),
    Without<SelfPlayer>,
>;

/// Whether `store` is a living unit `relations` lets us attack.
pub(super) fn attackable(relations: &RelationWorld, guid: u64, store: &ObjectStore) -> bool {
    if !store.is_unit() || store.0.unit_health().is_some_and(|health| health == 0) {
        return false;
    }
    let Some(me) = relations.me() else {
        return false;
    };
    let them = relations.unit_of(guid, store);
    relations.get().can_attack(&me, &them)
}

/// The angle between the facing `yaw` and the bearing to `to`, radians in `[0, PI]`.
pub(super) fn off_facing(yaw: f32, to: Vec3) -> f32 {
    let delta = yaw_toward(to) - yaw;
    (delta + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Check the opening from the creature's facing, not the player's camera.
fn opening_geometry(to_player: Vec3, facing: f32, reach: f32, now: f32, until: f32) -> bool {
    now < until
        && to_player.length_squared() <= reach * reach
        && off_facing(facing, to_player).abs() <= std::f32::consts::FRAC_PI_4
}

pub(super) fn broken_opening(
    units: &Units,
    relations: &RelationWorld,
    player_pos: Vec3,
    now: f32,
    broken: (u64, f32),
) -> Option<u64> {
    let (target, until) = broken;
    units.iter().find_map(|(_, guid, store, at)| {
        let yaw = at.compute_transform().rotation.to_euler(EulerRot::YXZ).0;
        let reach = (store.0.unit_combat_reach() + 1.5 + 4.0 / 3.0).max(5.0);
        (guid.0 == target
            && attackable(relations, guid.0, store)
            && opening_geometry(player_pos - at.translation(), yaw, reach, now, until))
        .then_some(target)
    })
}

fn backstab_geometry(rear_angle: f32, target: Option<u64>, player: u64) -> bool {
    target != Some(player) && rear_angle.abs() >= 3.0 * std::f32::consts::FRAC_PI_4
}

pub(super) fn backstab_opening(
    units: &Units,
    relations: &RelationWorld,
    player_pos: Vec3,
) -> Option<u64> {
    let player = relations.me_guid()?;
    units
        .iter()
        .filter_map(|(_, guid, store, at)| {
            if guid.0 >> 48 != 0xf130 || !attackable(relations, guid.0, store) {
                return None;
            }
            let to = player_pos - at.translation();
            let yaw = at.compute_transform().rotation.to_euler(EulerRot::YXZ).0;
            let reach = (store.0.unit_combat_reach() + 1.5 + 4.0 / 3.0).max(5.0);
            let distance = to.length_squared();
            (distance <= reach * reach
                && backstab_geometry(off_facing(yaw, to), store.0.unit_target(), player))
            .then_some((distance, guid.0))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, guid)| guid)
}

/// Toggle and keep the lock.
#[allow(clippy::too_many_arguments)]
pub(super) fn toggle_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    capture: Res<UiKeyboardCapture>,
    er: Res<ErControls>,
    cams: Query<&FlyCam>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    selection: Res<Selection>,
    player: Res<Player>,
    relations: RelationWorld,
    units: Units,
    mut souls: ResMut<SoulsState>,
) {
    if !souls_on(&view) || !config.features.lock_on {
        if souls.lock.is_some() {
            souls.lock = None;
        }
        return;
    }
    let horizontal = |at: Vec3| {
        let to = at - player.pos;
        Vec2::new(to.x, to.z).length()
    };
    // Keep: the locked unit is still in the world, alive, attackable and in range.
    if let Some(locked) = souls.lock {
        let keep = units.get(locked).is_ok_and(|(_, guid, store, at)| {
            attackable(&relations, guid.0, store)
                && horizontal(at.translation()) <= LOCK_BREAK_YARDS
        });
        if !keep {
            souls.lock = None;
            info!("souls: lock-on broken");
        }
    }
    let input = KeyRead::new(&keys, &mouse, &capture, &er);
    if !input.just_pressed(config.binds.lock_on) && !input.just_pressed(config.binds.lock_on_alt) {
        return;
    }
    // Under the free camera the lock takes what the camera looks at, as ER's does.
    let facing = match cams.single() {
        Ok(cam) if er.free_camera => cam.yaw(),
        _ => player.facing(),
    };
    if souls.lock.take().is_some() {
        info!("souls: lock-on released");
        return;
    }
    let selected = selection
        .target
        .and_then(|entity| units.get(entity).ok())
        .filter(|(_, guid, store, at)| {
            attackable(&relations, guid.0, store)
                && horizontal(at.translation()) <= LOCK_ACQUIRE_YARDS
        });
    let pick = selected.map(|(entity, ..)| entity).or_else(|| {
        let front = LOCK_FRONT_DEGREES.to_radians();
        units
            .iter()
            .filter(|(_, guid, store, at)| {
                let to = at.translation() - player.pos;
                horizontal(at.translation()) <= LOCK_NEAREST_YARDS
                    && off_facing(facing, to).abs() <= front
                    && attackable(&relations, guid.0, store)
            })
            .min_by(|a, b| horizontal(a.3.translation()).total_cmp(&horizontal(b.3.translation())))
            .map(|(entity, ..)| entity)
    });
    souls.lock = pick;
    match pick {
        Some(entity) => info!("souls: locked on {entity:?}"),
        None => info!("souls: nothing to lock on"),
    }
}

/// What [`lock_follows_target`] does this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockSync {
    /// Nothing changed, or the lock and the target already agree.
    Keep,
    /// The lock changed to a unit that is not the target: select it.
    Select,
    /// The target changed (Tab, a click, a macro) while a lock is held: the lock follows it.
    Follow,
}

/// What was seen last frame, so a change can be told from a standing state.
#[derive(Default)]
pub(super) struct LockTargetMemo {
    lock: Option<Entity>,
    selected: Option<u64>,
}

/// Only an explicit switch input may move a standing lock. Selection writes alone are not intent.
fn lock_sync(
    memo: &LockTargetMemo,
    lock: Option<Entity>,
    lock_guid: Option<u64>,
    selected: Option<u64>,
    switch_pressed: bool,
) -> LockSync {
    if lock != memo.lock {
        return if lock.is_some() && lock_guid.is_some() && lock_guid != selected {
            LockSync::Select
        } else {
            LockSync::Keep
        };
    }
    if switch_pressed && lock.is_some() && selected != memo.selected && selected != lock_guid {
        return LockSync::Follow;
    }
    LockSync::Keep
}

/// The lock is the WoW target (`lock_targets`): a new lock selects its unit through the one
/// `SetSelection` path, so the target frame, the server's selection and `/target`-reading macros
/// follow it. A Tab or world click can move the lock to another attackable unit within
/// [`LOCK_ACQUIRE_YARDS`]. Automatic target writes and invalid picks leave it alone.
/// Releasing or breaking the lock leaves the WoW target alone.
#[allow(clippy::too_many_arguments)]
pub(super) fn lock_follows_target(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    capture: Res<UiKeyboardCapture>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    player: Res<Player>,
    relations: RelationWorld,
    units: Query<(&Guid, &ObjectStore, &GlobalTransform), Without<SelfPlayer>>,
    mut select: SelectCommit,
    mut souls: ResMut<SoulsState>,
    mut memo: Local<LockTargetMemo>,
) {
    let features = &config.features;
    if !souls_on(&view) || !features.lock_on || !features.lock_targets {
        *memo = LockTargetMemo::default();
        return;
    }
    let lock_guid = souls
        .lock
        .and_then(|entity| units.get(entity).ok())
        .map(|(guid, ..)| guid.0);
    let selected = select.selection().guid;
    let switch_pressed = (!capture.typing
        && !capture.consumed.contains(&KeyCode::Tab)
        && keys.just_pressed(KeyCode::Tab))
        || [MouseButton::Left, MouseButton::Right]
            .into_iter()
            .any(|button| mouse.just_pressed(button) || mouse.just_released(button));
    match lock_sync(&memo, souls.lock, lock_guid, selected, switch_pressed) {
        LockSync::Keep => {}
        LockSync::Select => {
            if let Some(guid) = lock_guid {
                select.select_unit(guid);
            }
        }
        LockSync::Follow => {
            let follow = select.selection().target.filter(|&entity| {
                units.get(entity).is_ok_and(|(guid, store, at)| {
                    let to = at.translation() - player.pos;
                    attackable(&relations, guid.0, store)
                        && Vec2::new(to.x, to.z).length() <= LOCK_ACQUIRE_YARDS
                })
            });
            if let Some(entity) = follow {
                souls.lock = Some(entity);
                info!("souls: lock-on follows the target to {entity:?}");
            }
        }
    }
    memo.lock = souls.lock;
    memo.selected = select.selection().guid;
}

/// A hit's reach (yards, from the body) and half-arc (radians) from its blade samples.
fn hit_shape(hit: &KitHit) -> (f32, f32) {
    let mut reach: f32 = 0.0;
    let mut arc: f32 = 0.0;
    for sample in &hit.blade {
        for end in [&sample[0..3], &sample[3..6]] {
            let (x, z) = (end[0].0 as f32 / 1000.0, end[2].0 as f32 / 1000.0);
            reach = reach.max((x * x + z * z).sqrt());
            if x != 0.0 || z != 0.0 {
                arc = arc.max(x.atan2(z).abs());
            }
        }
    }
    if hit.blade.is_empty() {
        reach = BARE_REACH_METRES;
    }
    let radius = hit.radius.0.max(0) as f32 / 1000.0;
    let arc = arc.clamp(ARC_MIN_DEGREES.to_radians(), ARC_MAX_DEGREES.to_radians());
    ((reach + radius) * YARDS_PER_METRE, arc)
}

/// Whether the hit window `[from, to)` was open at any frame in `(last, now]`, or at `now` on the
/// first test of a run.
fn window_reached(from: Milli, to: Milli, last: Option<Milli>, now: Milli) -> bool {
    match last {
        Some(last) => from <= now && to > last && now > last,
        None => from <= now && now < to,
    }
}

/// Test the running attack's hit windows against the units around the body.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_hits(
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    player: Res<Player>,
    relations: RelationWorld,
    units: Units,
    net: Option<Res<NetCommands>>,
    mut shakes: ResMut<CameraShakes>,
    mut playback: ResMut<CombatPlaybackState>,
    mut souls: ResMut<SoulsState>,
    mut link: ResMut<SoulsLink>,
) {
    let features = config.features;
    if !souls_on(&view) || !features.hits {
        if souls.swing != SwingTrack::default() {
            souls.swing = SwingTrack::default();
        }
        if !souls_on(&view) {
            souls.engaged = None;
        }
        return;
    }
    let running = current_kit(&kit, &profiles).and_then(|kit| running_action(kit, &playback));
    let Some((instance, action, frame)) = running.filter(|(_, action, _)| !action.hits.is_empty())
    else {
        souls.swing = SwingTrack::default();
        return;
    };
    if souls.critical == Some(instance) {
        return;
    }
    souls.critical = None;
    let mut track = std::mem::take(&mut souls.swing);
    if track.instance != Some(instance) {
        track = SwingTrack {
            instance: Some(instance),
            ..default()
        };
    }
    let yaw = player.facing();
    let mut stop = Duration::ZERO;
    for (index, hit) in action.hits.iter().enumerate() {
        if !window_reached(hit.from, hit.to, track.last_frame, frame) {
            continue;
        }
        let (reach, arc) = hit_shape(hit);
        // This window's struck units, for the server: (distance, guid).
        let mut struck: Vec<(f32, u64)> = Vec::new();
        for (entity, guid, store, at) in &units {
            if track.struck.contains(&(index, entity)) {
                continue;
            }
            let to = at.translation() - player.pos;
            let distance =
                Vec2::new(to.x, to.z).length() - store.0.unit_combat_reach().clamp(0.0, 3.0) * 0.5;
            if distance > reach
                || to.y.abs() > HIT_HEIGHT_YARDS
                || off_facing(yaw, to).abs() > arc
                || !attackable(&relations, guid.0, store)
            {
                continue;
            }
            track.struck.push((index, entity));
            struck.push((distance, guid.0));
            info!(
                "souls: {} hit {} lands on {:#x} (motion value {}, {:.1} yd)",
                action.name,
                index + 1,
                guid.0,
                hit.motion_value,
                distance.max(0.0)
            );
            if features.hit_stop {
                let millis = u64::from(hit.hit_stop.0.max(0).unsigned_abs());
                stop = stop.max(Duration::from_millis(millis));
            }
            if !link.reporting() && features.engage_on_hit && souls.engaged != Some(guid.0) {
                if let Some(net) = net.as_deref() {
                    let _ = net
                        .0
                        .send(crate::net::ClientCommand::AttackSwing { guid: guid.0 });
                    souls.engaged = Some(guid.0);
                }
            }
        }
        if link.reporting() && !struck.is_empty() {
            struck.sort_by(|a, b| a.0.total_cmp(&b.0));
            let guids: Vec<u64> = struck.iter().map(|(_, guid)| *guid).collect();
            link.report_hit(hit.motion_value.0, &guids, kit.two_handing());
        }
    }
    if stop > Duration::ZERO {
        playback.hit_stop = playback.hit_stop.max(stop);
        shakes.add_impact(player.pos, time.elapsed_secs(), stop.as_secs_f32());
    }
    track.last_frame = Some(frame);
    souls.swing = track;
}

/// Hold the local player's clips still while hit-stop is owed, so the swing visibly bites, and
/// resume them when it is paid (or the mode or the switch goes off). A rig an aura freeze holds
/// ([`AnimRateFreeze`]) is left for the aura to thaw.
pub(super) fn hold_clip(
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    playback: Res<CombatPlaybackState>,
    mut rigs: Query<(&mut AnimationPlayer, Has<AnimRateFreeze>), With<SelfPlayer>>,
    // Its own flag, not `SoulsState`'s: the mode-off reset must not forget a paused clip.
    mut held: Local<bool>,
) {
    let hold = souls_on(&view) && config.features.hit_stop && playback.hit_stop > Duration::ZERO;
    if hold == *held {
        return;
    }
    *held = hold;
    let Ok((mut player, aura_frozen)) = rigs.single_mut() else {
        return;
    };
    for (_, anim) in player.playing_animations_mut() {
        if hold {
            anim.pause();
        } else if !aura_frozen {
            anim.resume();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(blade: Vec<[Milli; 6]>) -> KitHit {
        KitHit {
            from: Milli(10_000),
            to: Milli(13_000),
            motion_value: Milli(1_000),
            guard_damage: Milli(1_000),
            stamina: Milli::ZERO,
            hit_stop: Milli(80),
            radius: Milli(100),
            blade,
        }
    }

    #[test]
    fn backstab_requires_rear_cone_and_no_player_target() {
        assert!(backstab_geometry(std::f32::consts::PI, None, 1));
        assert!(!backstab_geometry(std::f32::consts::PI, Some(1), 1));
        assert!(!backstab_geometry(std::f32::consts::FRAC_PI_2, Some(2), 1));
    }

    #[test]
    fn broken_opening_requires_front_reach_and_time() {
        let front = Vec3::NEG_Z;
        let facing = yaw_toward(front);
        assert!(opening_geometry(front * 3.0, facing, 5.0, 1.0, 2.0));
        assert!(!opening_geometry(-front * 3.0, facing, 5.0, 1.0, 2.0));
        assert!(!opening_geometry(front * 6.0, facing, 5.0, 1.0, 2.0));
        assert!(!opening_geometry(front * 3.0, facing, 5.0, 2.0, 2.0));
    }

    #[test]
    fn lock_and_target_follow_each_other() {
        let (a, b) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        let step = |memo: (Option<Entity>, Option<u64>), lock, lock_guid, selected| {
            let memo = LockTargetMemo {
                lock: memo.0,
                selected: memo.1,
            };
            lock_sync(&memo, lock, lock_guid, selected, true)
        };
        // A new lock on a unit that is not the target selects it; on the target, nothing to do.
        assert_eq!(
            step((None, None), Some(a), Some(10), None),
            LockSync::Select
        );
        assert_eq!(
            step((None, Some(10)), Some(a), Some(10), Some(10)),
            LockSync::Keep
        );
        // A Tab while locked moves the lock; the same target standing does nothing.
        assert_eq!(
            step((Some(a), Some(10)), Some(a), Some(10), Some(20)),
            LockSync::Follow
        );
        assert_eq!(
            step((Some(a), Some(10)), Some(a), Some(10), Some(10)),
            LockSync::Keep
        );
        // A cleared target is considered, but cannot supply an entity to follow.
        assert_eq!(
            step((Some(a), Some(10)), Some(a), Some(10), None),
            LockSync::Follow
        );
        // A released lock leaves the target alone; no lock, a target change does nothing.
        assert_eq!(
            step((Some(a), Some(10)), None, None, Some(10)),
            LockSync::Keep
        );
        assert_eq!(step((None, Some(10)), None, None, Some(20)), LockSync::Keep);
        // A new lock and a new target in one frame: the lock wins.
        assert_eq!(
            step((Some(a), Some(10)), Some(b), Some(30), Some(20)),
            LockSync::Select
        );
    }

    #[test]
    fn background_selection_changes_do_not_switch_the_lock() {
        let unit = Entity::from_raw_u32(1).unwrap();
        let mut memo = LockTargetMemo {
            lock: Some(unit),
            selected: Some(10),
        };
        for selected in [Some(20), None, Some(10)].into_iter().cycle().take(90) {
            assert_eq!(
                lock_sync(&memo, Some(unit), Some(10), selected, false),
                LockSync::Keep
            );
            memo.selected = selected;
        }
        assert_eq!(
            lock_sync(&memo, Some(unit), Some(10), Some(20), true),
            LockSync::Follow
        );
        memo.selected = Some(20);
        assert_eq!(
            lock_sync(&memo, Some(unit), Some(10), Some(20), true),
            LockSync::Keep
        );
        assert_eq!(lock_sync(&memo, None, None, Some(20), true), LockSync::Keep);
    }

    /// The blade's farthest end plus the capsule radius is the reach; its widest bearing, clamped,
    /// the arc.
    #[test]
    fn blade_gives_reach_and_arc() {
        let m = Milli;
        // One end 1.2 m straight ahead, the other 0.9 m out to the side.
        let (reach, arc) = hit_shape(&hit(vec![[m(0), m(1000), m(1200), m(900), m(1000), m(0)]]));
        assert!((reach - 1.3 * YARDS_PER_METRE).abs() < 1e-4);
        assert!((arc - 90.0_f32.to_radians()).abs() < 1e-4);
        // A straight thrust still sweeps the minimum arc.
        let (_, arc) = hit_shape(&hit(vec![[m(0), m(1000), m(500), m(0), m(1000), m(1500)]]));
        assert!((arc - ARC_MIN_DEGREES.to_radians()).abs() < 1e-4);
        let (reach, _) = hit_shape(&hit(Vec::new()));
        assert!((reach - 1.6 * YARDS_PER_METRE).abs() < 1e-4);
    }

    /// A window a long frame skipped over still lands; one already passed does not land again.
    #[test]
    fn skipped_windows_are_caught_up() {
        let m = Milli;
        assert!(window_reached(
            m(10_000),
            m(13_000),
            Some(m(9_000)),
            m(14_000)
        ));
        assert!(window_reached(m(10_000), m(13_000), None, m(11_000)));
        assert!(!window_reached(m(10_000), m(13_000), None, m(13_000)));
        assert!(!window_reached(
            m(10_000),
            m(13_000),
            Some(m(13_000)),
            m(14_000)
        ));
        assert!(!window_reached(
            m(10_000),
            m(13_000),
            Some(m(8_000)),
            m(9_000)
        ));
        assert!(!window_reached(
            m(10_000),
            m(13_000),
            Some(m(11_000)),
            m(11_000)
        ));
    }

    #[test]
    fn off_facing_wraps() {
        // Facing -Z (yaw 0): a unit at -Z is dead ahead, one at +X a quarter turn off.
        assert!(off_facing(0.0, Vec3::new(0.0, 0.0, -1.0)).abs() < 1e-6);
        let quarter = off_facing(0.0, Vec3::new(1.0, 0.0, 0.0)).abs();
        assert!((quarter - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let behind = off_facing(3.0, Vec3::new(0.0, 0.0, 1.0)).abs();
        assert!((behind - (std::f32::consts::PI - 3.0)).abs() < 1e-5);
    }
}
