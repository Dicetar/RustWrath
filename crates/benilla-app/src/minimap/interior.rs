//! The WMO-interior minimap's group selection (the portal flood-fill) and tile-name stem.

use bevy::math::{Affine3A, Vec3};

use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
use benilla_assets::WmoModel;

/// MOGP flags: `0x8` exterior, `0x40` the outdoor-lit interior class a city street carries; the
/// flood keeps to groups whose `0x48` bits match the seed's (`QueryMapObjMinimapGroup`).
const GROUP_EXTERIOR: u32 = 0x8;
const GROUP_EXTERIOR_LIT: u32 = 0x40;
/// A portal into a group flagged `0x80` is never crossed.
const GROUP_NO_MINIMAP_PORTAL: u32 = 0x80;
/// The two MOGP group ids (`WMOAreaTable.WMOGroupID`) the flood neither leaves nor enters.
const FLOOD_STOP_GROUP_IDS: [u32; 2] = [23015, 23016];
/// `minimapPortalMax`'s default: the recursion depth cap.
const MINIMAP_PORTAL_MAX: u32 = 99;

/// The `WMOAreaTable` flag bits the minimap reads (`MinimapUpdate`): the whole-WMO row's `0x1`
/// (outdoor area) and `0x18` (indoor area) open the building's map, a group row's `0x20` forces the
/// outdoor map.
const AREA_OUTDOOR: u32 = 0x1;
const AREA_INDOOR: u32 = 0x18;
const AREA_FORCE_OUTDOOR: u32 = 0x20;

/// How the building's map draws: the whole-WMO tile set `<wmo>_<groupCount>_<x>_<y>` gridded over
/// the MOHD box, or the portal flood's per-group tiles from a seed group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InteriorMap {
    Whole,
    Groups { seed: usize, inside: bool },
}

/// `MinimapUpdate`'s building verdict for a player linked to `group_flags`' group (MOGP flags) under
/// the group's and the whole WMO's `WMOAreaTable` flags; `None` draws the terrain.
///
/// The group row's `0x20` forces the outdoor map. Otherwise the building's map opens when the root
/// row says outdoor or indoor area, or the group is not exterior. The root's indoor-area bits then
/// pick the whole-WMO set unless the group carries neither `0x8` nor `0x40`
/// (`CMapEntity::QueryMapObjMinimap`): Stormwind's root row is `0x10`, so its `0x40` streets and
/// exterior groups draw the whole city while a shop interior floods its own groups.
pub(super) fn interior_map_mode(
    group_flags: u32,
    group_row: Option<u32>,
    root_row: Option<u32>,
    seed: usize,
) -> Option<InteriorMap> {
    if group_row.is_some_and(|f| f & AREA_FORCE_OUTDOOR != 0) {
        return None;
    }
    let root = root_row.unwrap_or(0);
    let inside = group_flags & GROUP_EXTERIOR == 0;
    let indoor_area = root & AREA_INDOOR != 0;
    if !(root & AREA_OUTDOOR != 0 || indoor_area || inside) {
        return None;
    }
    if indoor_area && group_flags & (GROUP_EXTERIOR | GROUP_EXTERIOR_LIT) != 0 {
        Some(InteriorMap::Whole)
    } else {
        Some(InteriorMap::Groups { seed, inside })
    }
}

/// The `md5translate.trs` key stem of a WMO's interior tiles: the model path without `World\` and
/// `.wmo` (name builder `0x6da330`), backslashed and lowercased, the trs lookup being
/// case-insensitive. `None` if not a `World\…\*.wmo`.
pub(super) fn wmo_minimap_stem(wmo_path: &str) -> Option<String> {
    let p = wmo_path.replace('/', "\\").to_ascii_lowercase();
    let stem = p.split_once("world\\")?.1.strip_suffix(".wmo")?;
    Some(stem.to_string())
}

/// The interior query window (`0x6d95f8`-`0x6d96ab`): the `c`-wide cell holding the player,
/// snapped as `round(p·r − 0.5)·c` with `r = 1/c` (half to even), and `[z − c/2, z]` high, in WoW
/// world axes. x takes `r` as a stored float (`0x6d9629`), y the unrounded register (`0x6d9617`);
/// each product is stored as a float before the 0.5 comes off.
fn interior_window(p: [f32; 3], c: f32) -> ([f32; 3], [f32; 3]) {
    let r = 1.0 / f64::from(c);
    #[allow(clippy::cast_possible_truncation)] // the reference's float stores
    let snap = |v: f32, r: f64| {
        let cell = (f64::from((f64::from(v) * r) as f32) - 0.5).round_ties_even();
        (cell * f64::from(c)) as f32
    };
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (snap(p[0], f64::from(r as f32)), snap(p[1], r));
    let z = p[2] - c * 0.5;
    ([x, y, z], [x + c, y + c, c * 0.5 + z])
}

/// Which groups' tiles draw, per absolute group index: the reference's portal flood-fill from the
/// player's group (`QueryMapObjMinimapGroup`) inside the query box.
///
/// The flood keeps to the seed's class: inside, the groups whose `0x48` bits equal the seed's `0x40`
/// (a street floods streets, a room rooms); outside, the exterior groups. Every group it keeps
/// emits; it never crosses into a `0x80` group or past group ids 23015/23016, and stops at
/// `minimapPortalMax` levels.
///
/// The box is [`interior_window`] grown by `c` on every side (`0x6d96b6`-`0x6d96f3`), moved into
/// the model frame as the AABB of its image (`0x6dc540`), its top z then replaced by the seed
/// group's ceiling (`0x6a524b`, the group's `+0x28`). A group whose bbox misses the box in XY
/// neither emits nor floods onward (`0x6a51f4`); there is no per-group Z test. A portal is crossed
/// unless its polygon lies wholly outside one of the box's six planes, the only Z gate.
pub(super) fn interior_group_selection(
    model: &WmoModel,
    world_from_local: &Affine3A,
    player_pos: Vec3,
    radius: f32,
    seed: usize,
    inside: bool,
) -> Vec<bool> {
    let n = model.group_nav.len();
    let mut drawable = vec![false; n];
    if seed >= n {
        return drawable;
    }
    let class_bits = if inside {
        GROUP_EXTERIOR | GROUP_EXTERIOR_LIT
    } else {
        GROUP_EXTERIOR
    };
    let class = if inside {
        model.group_nav[seed].flags & GROUP_EXTERIOR_LIT
    } else {
        GROUP_EXTERIOR
    };
    let c = radius;
    let (wmin, wmax) = interior_window(bevy_to_wow(player_pos), c);
    let (wmin, wmax) = (wmin.map(|v| v - c), wmax.map(|v| v + c));

    // A world point (WoW axes) to the model frame, where the reference floods.
    let local_from_world = world_from_local.inverse();
    let to_model = |w: [f32; 3]| bevy_to_wow(local_from_world.transform_point3(wow_to_bevy(w)));
    let (mut box_min, mut box_max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for x in [wmin[0], wmax[0]] {
        for y in [wmin[1], wmax[1]] {
            for z in [wmin[2], wmax[2]] {
                let m = to_model([x, y, z]);
                for k in 0..3 {
                    box_min[k] = box_min[k].min(m[k]);
                    box_max[k] = box_max[k].max(m[k]);
                }
            }
        }
    }
    box_max[2] = model.group_nav[seed].bbox_max[2];

    let xy_overlap = |lo: [f32; 3], hi: [f32; 3]| {
        lo[0] <= box_max[0] && hi[0] >= box_min[0] && lo[1] <= box_max[1] && hi[1] >= box_min[1]
    };
    // 6-plane outcode of a model point against the box, one bit per face it is outside.
    let outcode = |w: [f32; 3]| {
        let mut o = 0u8;
        for k in 0..3 {
            if w[k] < box_min[k] {
                o |= 1 << (2 * k);
            }
            if w[k] > box_max[k] {
                o |= 1 << (2 * k + 1);
            }
        }
        o
    };

    let mut visited = vec![false; n];
    let mut stack = vec![(seed, 0u32)];
    while let Some((g, depth)) = stack.pop() {
        if g >= n || visited[g] {
            continue;
        }
        let gn = &model.group_nav[g];
        if gn.flags & class_bits != class {
            continue; // another class: a street does not flood a room, nor a room the street
        }
        visited[g] = true;
        // The XY gate blocks emit and recursion both; a stacked storey shares the XY footprint and
        // passes it.
        if !xy_overlap(gn.bbox_min, gn.bbox_max) {
            continue;
        }
        drawable[g] = true;
        if FLOOD_STOP_GROUP_IDS.contains(&gn.area_table_id) || depth >= MINIMAP_PORTAL_MAX {
            continue;
        }
        // Cross a portal unless its polygon is wholly outside one box plane, Z included.
        let start = gn.ref_start as usize;
        let end = (start + gn.ref_count as usize).min(model.portal_refs.len());
        for r in &model.portal_refs[start..end] {
            let nb = r.group as usize;
            if r.group == u16::MAX || nb >= n || visited[nb] {
                continue;
            }
            let target = &model.group_nav[nb];
            if target.flags & GROUP_NO_MINIMAP_PORTAL != 0
                || FLOOD_STOP_GROUP_IDS.contains(&target.area_table_id)
            {
                continue;
            }
            let Some(info) = model.portal_infos.get(r.portal as usize) else {
                continue;
            };
            let vs = info.start_vertex as usize;
            let Some(poly) = model.portal_vertices.get(vs..vs + info.count as usize) else {
                continue;
            };
            let mut and = 0xFFu8;
            for v in poly {
                and &= outcode(*v);
                if and == 0 {
                    break; // outside no single plane: the portal reaches into the box
                }
            }
            if and == 0 {
                stack.push((nb, depth + 1));
            }
        }
    }
    drawable
}

#[cfg(test)]
mod tests {
    /// A 4-group building: g0 (the player's) and g1 (a cellar) join by a floor hole and both draw;
    /// g2 sits 1000 yd off in XY, reached but not drawn; g3 overlaps the box but is reachable only
    /// through g2, by a portal inside the box, so only g2's XY gate keeps it out (`0x6a51f4`).
    #[test]
    fn interior_flood_fill_gates_on_xy_and_blocks_recursion_through_a_missed_group() {
        use super::interior_group_selection;
        use benilla_assets::{WmoGroupNav, WmoModel, WmoPortalInfo, WmoPortalRef};
        use bevy::math::{Affine3A, Vec3};

        let nav = |zmin: f32, zmax: f32, x: f32, ref_start: u16, ref_count: u16| WmoGroupNav {
            flags: 0,
            bbox_min: [x - 5.0, -5.0, zmin],
            bbox_max: [x + 5.0, 5.0, zmax],
            ref_start,
            ref_count,
            area_table_id: 0,
            fog_indices: [0; 4],
            group_liquid: benilla_formats::NO_GROUP_LIQUID,
        };
        // A 1-yd square portal polygon centred at (x, 0, z), lying in the plane x = const.
        let poly = |x: f32, z: f32| {
            [
                [x, -0.5, z - 0.5],
                [x, 0.5, z - 0.5],
                [x, 0.5, z + 0.5],
                [x, -0.5, z + 0.5],
            ]
        };
        let mut portal_vertices = Vec::new();
        portal_vertices.extend(poly(0.0, 0.0)); // portal 0: g0<->g1, a floor hole at the player
        portal_vertices.extend(poly(4.0, 1.0)); // portal 1: g0<->g2, well inside the query box
        portal_vertices.extend(poly(3.0, 1.0)); // portal 2: g2<->g3, also inside the box
        let info = |start: u16| WmoPortalInfo {
            start_vertex: start,
            count: 4,
            plane: [1.0, 0.0, 0.0, 0.0],
        };
        let pref = |portal: u16, group: u16, side: i16| WmoPortalRef {
            portal,
            group,
            side,
        };
        let model = WmoModel {
            wmo_id: 1,
            submeshes: Vec::new(),
            submesh_group: Vec::new(),
            portal_vertices,
            portal_infos: vec![info(0), info(4), info(8)],
            // g0 refs portals 0,1 · g1 refs portal 0 · g2 refs portals 1,2 · g3 refs portal 2
            portal_refs: vec![
                pref(0, 1, 1),
                pref(1, 2, 1),
                pref(0, 0, -1),
                pref(1, 0, -1),
                pref(2, 3, 1),
                pref(2, 2, -1),
            ],
            group_nav: vec![
                nav(0.0, 3.0, 0.0, 0, 2),    // g0 the player's floor
                nav(-10.0, -7.0, 0.0, 2, 1), // g1 cellar, stacked under g0
                nav(0.0, 3.0, 1000.0, 3, 2), // g2 far away in XY
                nav(0.0, 3.0, 0.0, 5, 1),    // g3 overlaps XY, reachable only through g2
            ],
            fogs: Vec::new(),
            skybox: None,
            group_collision_tris: Vec::new(),
            group_camera_only_tris: Vec::new(),
            group_collision_bounds: Vec::new(),
            group_collision_grids: Vec::new(),
            collision_bounds: None,
            collision: None,
            collision_camera: None,
            doodads: Vec::new(),
            doodad_sets: Vec::new(),
            lights: Vec::new(),
            group_bounds: Vec::new(),
            group_footprints: Vec::new(),
            material_ground_type: Vec::new(),
            material_diff_color: Vec::new(),
            group_footprint_bounds: Vec::new(),
            group_footprint_grids: Vec::new(),
            group_light_refs: Vec::new(),
            group_liquids: Vec::new(),
            doodad_base: Default::default(),
            doodad_owner: Default::default(),
            doodad_groups: Default::default(),
            bbox: None,
            convex_volume: Vec::new(),
        };

        let drawable =
            interior_group_selection(&model, &Affine3A::IDENTITY, Vec3::ZERO, 25.0, 0, true);
        assert!(drawable[0], "the player's own group draws");
        assert!(
            drawable[1],
            "the cellar is stacked in XY and its floor-hole portal sits inside the box's Z extent"
        );
        assert!(
            !drawable[2],
            "a reached group whose bbox misses the query window in XY must not emit tiles"
        );
        assert!(
            !drawable[3],
            "the XY gate that rejected g2 must also block flooding THROUGH it (`0x6a51f4`)"
        );
    }

    /// The box's Z extent keeps a distant storey out: a portal far below the box floor is not
    /// crossed.
    #[test]
    fn interior_flood_fill_rejects_a_portal_outside_the_query_box_z_extent() {
        use super::interior_group_selection;
        use benilla_assets::{WmoGroupNav, WmoModel, WmoPortalInfo, WmoPortalRef};
        use bevy::math::{Affine3A, Vec3};

        let nav = |zmin: f32, zmax: f32, ref_start: u16, ref_count: u16| WmoGroupNav {
            flags: 0,
            bbox_min: [-5.0, -5.0, zmin],
            bbox_max: [5.0, 5.0, zmax],
            ref_start,
            ref_count,
            area_table_id: 0,
            fog_indices: [0; 4],
            group_liquid: benilla_formats::NO_GROUP_LIQUID,
        };
        // The connecting portal sits at z ≈ -300, far below the box floor (player.z - 1.5·25).
        let model = WmoModel {
            wmo_id: 1,
            submeshes: Vec::new(),
            submesh_group: Vec::new(),
            portal_vertices: vec![
                [0.0, -0.5, -300.5],
                [0.0, 0.5, -300.5],
                [0.0, 0.5, -299.5],
                [0.0, -0.5, -299.5],
            ],
            portal_infos: vec![WmoPortalInfo {
                start_vertex: 0,
                count: 4,
                plane: [1.0, 0.0, 0.0, 0.0],
            }],
            portal_refs: vec![
                WmoPortalRef {
                    portal: 0,
                    group: 1,
                    side: 1,
                },
                WmoPortalRef {
                    portal: 0,
                    group: 0,
                    side: -1,
                },
            ],
            group_nav: vec![nav(0.0, 3.0, 0, 1), nav(-303.0, -300.0, 1, 1)],
            fogs: Vec::new(),
            skybox: None,
            group_collision_tris: Vec::new(),
            group_camera_only_tris: Vec::new(),
            group_collision_bounds: Vec::new(),
            group_collision_grids: Vec::new(),
            collision_bounds: None,
            collision: None,
            collision_camera: None,
            doodads: Vec::new(),
            doodad_sets: Vec::new(),
            lights: Vec::new(),
            group_bounds: Vec::new(),
            group_footprints: Vec::new(),
            material_ground_type: Vec::new(),
            material_diff_color: Vec::new(),
            group_footprint_bounds: Vec::new(),
            group_footprint_grids: Vec::new(),
            group_light_refs: Vec::new(),
            group_liquids: Vec::new(),
            doodad_base: Default::default(),
            doodad_owner: Default::default(),
            doodad_groups: Default::default(),
            bbox: None,
            convex_volume: Vec::new(),
        };

        let drawable =
            interior_group_selection(&model, &Affine3A::IDENTITY, Vec3::ZERO, 25.0, 0, true);
        assert!(drawable[0]);
        assert!(
            !drawable[1],
            "a portal below the query box's Z floor is not crossed, so the deep storey never draws"
        );
    }

    /// A seed group `g0` at the origin, 10 yd square, `[0, seed_top]` high, and a neighbour `g1`
    /// spanning `x` through a portal: a 1-yd square at (`portal_x`, 0, `portal_z`).
    fn two_groups(
        seed_top: f32,
        g1_x: [f32; 2],
        portal_x: f32,
        portal_z: f32,
    ) -> benilla_assets::WmoModel {
        use benilla_assets::{WmoGroupNav, WmoModel, WmoPortalInfo, WmoPortalRef};
        let nav = |bmin: [f32; 3], bmax: [f32; 3], ref_start: u16| WmoGroupNav {
            flags: 0,
            bbox_min: bmin,
            bbox_max: bmax,
            ref_start,
            ref_count: 1,
            area_table_id: 0,
            fog_indices: [0; 4],
            group_liquid: benilla_formats::NO_GROUP_LIQUID,
        };
        let (x, z) = (portal_x, portal_z);
        WmoModel {
            portal_vertices: vec![
                [x, -0.5, z - 0.5],
                [x, 0.5, z - 0.5],
                [x, 0.5, z + 0.5],
                [x, -0.5, z + 0.5],
            ],
            portal_infos: vec![WmoPortalInfo {
                start_vertex: 0,
                count: 4,
                plane: [1.0, 0.0, 0.0, 0.0],
            }],
            portal_refs: vec![
                WmoPortalRef {
                    portal: 0,
                    group: 1,
                    side: 1,
                },
                WmoPortalRef {
                    portal: 0,
                    group: 0,
                    side: -1,
                },
            ],
            group_nav: vec![
                nav([-5.0, -5.0, 0.0], [5.0, 5.0, seed_top], 0),
                nav([g1_x[0], -5.0, 0.0], [g1_x[1], 5.0, 3.0], 1),
            ],
            ..Default::default()
        }
    }

    /// The query window is the snapped cell `round(p·r − 0.5)·c` (`0x6d95f8`-`0x6d96ab`) grown
    /// by `c` on every side (`0x6d96b6`-`0x6d96f3`): at the origin with `c` 25, x spans
    /// `[−25, 50]`, so a group 30 yd west misses it and one 40 yd east is inside.
    #[test]
    fn the_query_window_is_the_snapped_cell_grown_by_its_width() {
        use super::interior_group_selection;
        use bevy::math::{Affine3A, Vec3};
        let at = |g1_x: [f32; 2], portal_x: f32| {
            let model = two_groups(3.0, g1_x, portal_x, 1.0);
            interior_group_selection(&model, &Affine3A::IDENTITY, Vec3::ZERO, 25.0, 0, true)[1]
        };
        assert!(
            !at([-45.0, -30.0], -4.0),
            "west of the window's x minimum −25"
        );
        assert!(at([-45.0, -24.0], -4.0), "reaching into it");
        assert!(
            at([45.0, 52.0], 4.0),
            "east, inside the window's x maximum 50"
        );
        assert!(!at([51.0, 60.0], 4.0), "past it");
        // At x = 30 the cell is [25, 50], so the window is [0, 75].
        let model = two_groups(3.0, [-12.0, -1.0], -4.0, 1.0);
        let west = interior_group_selection(
            &model,
            &Affine3A::IDENTITY,
            benilla_assets::coords::wow_to_bevy([30.0, 0.0, 0.0]),
            25.0,
            0,
            true,
        );
        assert!(
            !west[1],
            "the snap moved the window off it, though it lies within 2c"
        );
    }

    /// The flood's box top is the seed group's ceiling (`0x6a524b`, its `+0x28`), not the
    /// player's height plus `c`: in a hall taller than `c` a portal above `z + c` is crossed, and
    /// under a low ceiling a portal above it is not.
    #[test]
    fn the_flood_box_tops_out_at_the_seed_groups_ceiling() {
        use super::interior_group_selection;
        use bevy::math::{Affine3A, Vec3};
        let crossed = |seed_top: f32, portal_z: f32| {
            let model = two_groups(seed_top, [-5.0, 5.0], 0.0, portal_z);
            interior_group_selection(&model, &Affine3A::IDENTITY, Vec3::ZERO, 25.0, 0, true)[1]
        };
        assert!(
            crossed(40.0, 35.0),
            "a 40-yd hall: the portal at 35 is under its ceiling"
        );
        assert!(
            !crossed(3.0, 11.0),
            "a 3-yd room: the portal at 11 is above its ceiling"
        );
        assert!(crossed(3.0, 2.0), "and one under it is crossed");
    }

    #[test]
    fn wmo_stem_strips_world_prefix_and_extension() {
        use super::wmo_minimap_stem;
        // The stem plus `_001_00_00.blp` is the trs key
        // `WMO\KhazModan\Cities\Ironforge\ironforge_001_00_00.blp`.
        assert_eq!(
            wmo_minimap_stem("World/wmo/KhazModan/Cities/Ironforge/Ironforge.wmo").as_deref(),
            Some("wmo\\khazmodan\\cities\\ironforge\\ironforge"),
        );
        assert_eq!(
            wmo_minimap_stem("World\\wmo\\Azeroth\\Buildings\\Stormwind\\Stormwind.wmo").as_deref(),
            Some("wmo\\azeroth\\buildings\\stormwind\\stormwind"),
        );
        assert_eq!(wmo_minimap_stem("not/a/model.txt"), None);
    }

    /// Stormwind's whole-WMO row is `0x10`: its `0x40` street groups and `0x8` exterior groups
    /// draw the whole-city set, a shop room (neither bit) floods its own groups, and a group row's
    /// `0x20` (the Valley of Heroes) forces the terrain map. A building with no indoor root row
    /// opens only for an interior group.
    #[test]
    fn the_root_area_row_picks_the_whole_wmo_set_for_streets() {
        use super::{interior_map_mode, InteriorMap};
        let (street, exterior, room) = (0x2841, 0x8 | 0x1, 0x2205);
        assert_eq!(
            interior_map_mode(street, Some(0x4), Some(0x10), 7),
            Some(InteriorMap::Whole)
        );
        assert_eq!(
            interior_map_mode(exterior, None, Some(0x10), 46),
            Some(InteriorMap::Whole)
        );
        assert_eq!(
            interior_map_mode(room, Some(0), Some(0x10), 21),
            Some(InteriorMap::Groups {
                seed: 21,
                inside: true
            })
        );
        assert_eq!(interior_map_mode(street, Some(0x20), Some(0x10), 7), None);
        assert_eq!(interior_map_mode(exterior, None, None, 3), None);
        assert_eq!(
            interior_map_mode(room, None, None, 3),
            Some(InteriorMap::Groups {
                seed: 3,
                inside: true
            })
        );
        assert_eq!(
            interior_map_mode(exterior, None, Some(0x1), 3),
            Some(InteriorMap::Groups {
                seed: 3,
                inside: false
            })
        );
    }

    /// The flood keeps to the seed's `0x48` class: a street seed (`0x40`) does not flood into a
    /// room, a room seed does not flood into the street, and no flood enters a `0x80` group.
    #[test]
    fn the_flood_keeps_to_the_seed_groups_class() {
        use super::interior_group_selection;
        use bevy::math::{Affine3A, Vec3};
        let with_flags = |f0: u32, f1: u32| {
            let mut model = two_groups(3.0, [3.0, 13.0], 4.0, 1.0);
            model.group_nav[0].flags = f0;
            model.group_nav[1].flags = f1;
            interior_group_selection(&model, &Affine3A::IDENTITY, Vec3::ZERO, 25.0, 0, true)
        };
        assert_eq!(with_flags(0x40, 0x40), [true, true], "street to street");
        assert_eq!(with_flags(0x40, 0), [true, false], "street to room");
        assert_eq!(with_flags(0, 0x40), [true, false], "room to street");
        assert_eq!(with_flags(0, 0x80), [true, false], "into a 0x80 group");
        assert_eq!(with_flags(0, 0), [true, true], "room to room");
    }
}
