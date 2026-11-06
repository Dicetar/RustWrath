//! Terrain conform (`0x7106c0`): a model whose M2 has `GlobalModelFlags & 3` of 1 pitches to the
//! ground (mounts, quadrupeds) and of 3 pitches and rolls (low wide bodies: kodo, basilisk, crab),
//! wild or mounted alike. The tilt is one [`ConformNode`] the root bones hang under, so a rider
//! tilts with its mount while the unit's yaw and collider stay upright. `WOW_NO_MOUNT_TILT=1`
//! disables it.

use avian3d::prelude::SpatialQuery;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::prelude::*;

/// Ray envelope around the unit's feet: start this far up, reach this far down past the feet.
const TILT_PROBE_UP: f32 = 2.0;
const TILT_PROBE_DOWN: f32 = 4.0;
/// The walkable cutoff (`[0x80e028]`, about cos 89°): a steeper face targets world-up.
const TILT_WALKABLE_Y: f32 = 0.0175;
/// The freeze band (`[0x80c6a8]`, about cos 69.07°): steeper ground holds the last stance.
const TILT_FREEZE_Y: f32 = 0.3572;
/// The up-vector smoothing (`[0x80c6a0]`): the residual toward the target decays by `0.0018^dt`.
const TILT_DECAY: f32 = 0.0018;

/// The tilt carrier a flagged model's root bones hang under, a child of the unit spawned when the
/// display's `terrain_tilt != 0`; [`conform_units`] writes its rotation, the `0x7106c0` stage.
#[derive(Component)]
pub(super) struct ConformNode {
    /// The unit whose feet sample the ground and whose yaw frames it: for a mount child, the host.
    pub(super) unit: Entity,
    /// The model's `GlobalModelFlags & 3` dispatch mode (1 = pitch, 3 = pitch + roll).
    pub(super) mode: u8,
}

/// The smoothed up-vector's target for one sampled face normal (`0x637140`, `0x614cd0`): world-up
/// on a miss or an unwalkable face, `None` (hold the stance) in the freeze band, else the normal.
fn tilt_target(sampled: Option<Vec3>) -> Option<Vec3> {
    match sampled {
        Some(n) if n.y < TILT_WALKABLE_Y => Some(Vec3::Y),
        Some(n) if n.y < TILT_FREEZE_Y => None,
        Some(n) => Some(n),
        None => Some(Vec3::Y),
    }
}

/// The `0x7106c0` flag dispatch in the unit's local frame: flag 1 (`0x710769`) keeps left
/// horizontal, a pure pitch of `atan2(n.z, n.y)` without roll; flag 3 (`0x710a80`) takes the normal
/// as up; any other, flag 2 included, is level. No clamp, no easing: all smoothing is upstream.
fn conform_rotation(mode: u8, n_local: Vec3) -> Quat {
    match mode {
        1 => Quat::from_rotation_x(n_local.z.atan2(n_local.y)),
        3 => {
            let up = n_local.normalize_or_zero();
            // left = up × forward, in the Bevy local frame (forward −Z, left −X, up +Y).
            let left = up.cross(Vec3::NEG_Z);
            if up == Vec3::ZERO || left.length_squared() < 1e-6 {
                return Quat::IDENTITY; // degenerate normal (≈ along the facing axis)
            }
            let left = left.normalize();
            let fwd = left.cross(up).normalize();
            Quat::from_mat3(&Mat3::from_cols(-left, up, -fwd))
        }
        _ => Quat::IDENTITY,
    }
}

/// Advance one unit's smoothed up-vector: a first sight seeds at the sampled target, a parked rig
/// keeps the state it had (no ray), and an awake rig eases toward a fresh target. Returns whether
/// the ray ran. Re-seeding is therefore first-sight only; a wake continues from the held vector,
/// as the reference, which samples every unit every frame, would have been easing all along.
fn update_up(
    up: &mut Option<Vec3>,
    parked: bool,
    decay: f32,
    sample: impl FnOnce() -> Option<Vec3>,
) -> bool {
    if parked && up.is_some() {
        return false;
    }
    let target = sample();
    let s = up.get_or_insert_with(|| target.unwrap_or(Vec3::Y));
    if let Some(t) = target {
        *s = t + (*s - t) * decay;
    }
    true
}

/// Conform every flagged model to the ground under its unit: one face normal per unit from a
/// down-ray against the decal receivers (the reference averages its walkable contact normals,
/// `CMovement+0x24`; the two agree on a uniform slope), smoothed on the up-vector.
///
/// The reference updates every unit every frame; a parked rig keeps its last up-vector instead of
/// dropping it (so a wake eases on from the held slope rather than re-seeding at the current one),
/// and rays only on first sight. An unflagged unit's vector is never read, so the tracking stays
/// bounded to units showing a flagged visual.
#[allow(clippy::type_complexity)] // one Bevy system's full input set
pub(super) fn conform_units(
    time: Res<Time>,
    spatial: SpatialQuery,
    decals: benilla_world::decal::WorldDecal,
    units: Query<(&Transform, Has<benilla_world::rig_anim::AnimParked>), Without<ConformNode>>,
    mut nodes: Query<(&ConformNode, &mut Transform)>,
    mut ups: Local<EntityHashMap<Vec3>>,
    mut disabled: Local<Option<bool>>,
    mut census_at: Local<f32>,
) {
    if *disabled.get_or_insert_with(|| std::env::var_os("WOW_NO_MOUNT_TILT").is_some()) {
        return;
    }
    let filter = benilla_world::collision::WorldCollision::body_filter();
    let decay = TILT_DECAY.powf(time.delta_secs());
    let mut seen = EntityHashSet::default();
    let mut max_pitch: Option<(Entity, f32)> = None;
    let mut count = 0usize;
    for (node, mut node_tf) in &mut nodes {
        let Ok((unit_tf, parked)) = units.get(node.unit) else {
            continue;
        };
        // Sample and smooth once per unit per frame: a unit can host two flagged visuals (a
        // flagged body over a mount), and stepping twice would square the decay. A parked unit
        // already holding a vector takes neither step.
        if seen.insert(node.unit) {
            let mut up = ups.remove(&node.unit);
            update_up(&mut up, parked, decay, || {
                // A unit on a flying spline eases level here: its flying pitch and bank are on the
                // unit transform, from `sample_splines` (`0x7c5490`'s flying branch), whatever the
                // model flags. The probe misses in the air, so `tilt_target` yields world-up.
                let origin = unit_tf.translation + Vec3::Y * TILT_PROBE_UP;
                let sampled = spatial
                    .cast_ray_predicate(
                        origin,
                        Dir3::NEG_Y,
                        TILT_PROBE_UP + TILT_PROBE_DOWN,
                        true,
                        &filter,
                        &|e| decals.receives(e),
                    )
                    .map(|hit| hit.normal);
                tilt_target(sampled)
            });
            if let Some(up) = up {
                ups.insert(node.unit, up);
            }
        }
        let Some(&s) = ups.get(&node.unit) else {
            continue;
        };
        let n_local = unit_tf.rotation.inverse() * s;
        node_tf.rotation = conform_rotation(node.mode, n_local);
        count += 1;
        let pitch = n_local.z.atan2(n_local.y);
        if max_pitch.is_none_or(|(_, p)| pitch.abs() > p.abs()) {
            max_pitch = Some((node.unit, pitch));
        }
    }
    ups.retain(|e, _| seen.contains(e));
    let now = time.elapsed_secs();
    if now >= *census_at {
        *census_at = now + 1.0;
        if let Some((e, p)) = max_pitch {
            debug!(
                "terrain conform: {count} conforming units, max pitch {p:+.3} rad on {e} ({} tracked)",
                ups.len(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilt_target_bands_match_the_byte_gates() {
        assert_eq!(tilt_target(None), Some(Vec3::Y));
        assert_eq!(tilt_target(Some(Vec3::new(1.0, 0.01, 0.0))), Some(Vec3::Y));
        assert_eq!(tilt_target(Some(Vec3::new(0.9, 0.2, 0.0))), None);
        let n = Vec3::new(0.0, 0.9, 0.3).normalize();
        assert_eq!(tilt_target(Some(n)), Some(n));
    }

    #[test]
    fn conform_rotation_matches_the_flag_dispatch() {
        for mode in [0u8, 1, 2, 3] {
            let q = conform_rotation(mode, Vec3::Y);
            assert!(
                q.angle_between(Quat::IDENTITY) < 1e-3,
                "level is identity (mode {mode})"
            );
        }
        let uphill = Vec3::new(0.0, 0.9, 0.3).normalize();
        let q1 = conform_rotation(1, uphill);
        let fwd = q1 * Vec3::NEG_Z;
        assert!(fwd.y > 0.05, "flag 1 uphill tips the nose up, got {fwd}");
        let side = Vec3::new(0.3, 0.9, 0.0).normalize();
        assert!(
            conform_rotation(1, side).angle_between(Quat::IDENTITY) < 1e-3,
            "flag 1 discards pure roll"
        );
        let q3 = conform_rotation(3, side);
        assert!(
            (q3 * Vec3::Y - side).length() < 1e-4,
            "flag 3 up = the normal verbatim"
        );
        assert!(
            conform_rotation(3, uphill).angle_between(q1) < 1e-3,
            "flag 3 reduces to flag 1 without roll"
        );
        assert!(conform_rotation(2, uphill).angle_between(Quat::IDENTITY) < 1e-3);
    }

    /// A parked rig keeps its slope (no ray); waking eases on from that state instead of
    /// re-seeding at the new slope, and only a first sight is seeded by a ray.
    #[test]
    fn a_parked_rig_holds_its_slope_and_wakes_without_reseeding() {
        let parked_slope = Vec3::new(0.0, 0.9, 0.3).normalize();
        let wake_slope = Vec3::new(0.3, 0.9, 0.0).normalize();
        let mut up = None;
        let rays = std::cell::Cell::new(0usize);
        let sample = |slope: Vec3| {
            rays.set(rays.get() + 1);
            Some(slope)
        };

        assert!(update_up(&mut up, false, 0.5, || sample(parked_slope)));
        assert_eq!(rays.get(), 1, "first sight rays once");
        let held = up.expect("seeded");
        assert!((held - parked_slope).length() < 1e-6);

        // Parked: no ray, state retained.
        assert!(!update_up(&mut up, true, 0.5, || sample(wake_slope)));
        assert_eq!(rays.get(), 1, "a parked rig does not ray");
        assert_eq!(up, Some(held), "and keeps its slope");

        // Wake: one ray, easing from `held`, not a re-seed at `wake_slope`.
        assert!(update_up(&mut up, false, 0.5, || sample(wake_slope)));
        assert_eq!(rays.get(), 2);
        let woke = up.expect("still tracked");
        assert!(
            (woke - wake_slope).length() > 1e-3,
            "a wake must not snap to the fresh slope: {woke:?}"
        );
        let between = wake_slope + (held - wake_slope) * 0.5;
        assert!((woke - between).length() < 1e-6, "{woke:?} vs {between:?}");
    }

    /// A rig first seen while parked still rays once to seat itself, then holds.
    #[test]
    fn a_first_sight_while_parked_seeds_then_holds() {
        let slope = Vec3::new(0.0, 0.9, 0.3).normalize();
        let mut up = None;
        let mut rays = 0;
        assert!(update_up(&mut up, true, 0.5, || {
            rays += 1;
            Some(slope)
        }));
        assert_eq!(rays, 1, "the seed is appropriate here");
        assert!((up.unwrap() - slope).length() < 1e-6);
        assert!(!update_up(&mut up, true, 0.5, || {
            rays += 1;
            Some(Vec3::X)
        }));
        assert_eq!(rays, 1, "then the parked rig holds");
    }
}
