//! The `ObjectIcons.blp` blips of the 12340 classifier (`CGMinimapFrame`'s object enumerator)
//! on the 3.3.5 8×2 grid: quest cells 9 (yellow "!"), 10 (yellow "?") and 11 (blue "!"), the
//! tracked cell 8 and the hostile-monster cell 5. The party dot keeps the 1.12 cell; 12340 draws
//! party members from `PartyRaidBlips` instead, not built.

use std::collections::HashMap;

use bevy::math::Rect;
use bevy::prelude::*;

use benilla_assets::coords::bevy_to_wow;
use benilla_formats::{LockCatalog, ShapeshiftForm, LOCK_KEY_SKILL};
use benilla_protocol::EntityKind;
use benilla_ui::script::minimap_tracking::{tracked_info, TrackingKind, TrackingType};

use crate::creature_type::CreatureTypeSources;
use crate::go_templates::GameObjectTemplates;
use crate::names::NameCache;
use crate::net::{GuidIndex, NetEntity, ObjectStore};
use crate::ui_pass::{UiQuad, UiQuads, UvRect};

use super::{group_positions, BlipCtx, MinimapBlipHover, TrackedCandidates, BLIP_BASIS_PX};

/// The quest dot's quad, 8 px (`0xbc82a8`, frozen by the ctor); the per-cell scale table
/// `{1,1,1,1,1.3}` scales only the party cell 4.
const QUEST_DOT_PX: f32 = 8.0;
/// The party dot's quad: the same 8-px base at the cell table's 1.3× party scale.
const PARTY_DOT_PX: f32 = 8.0 * 1.3;
/// The 1.12 4×4 grid's blue party cell 4. On the 3.3.5 texture this is not a party dot: 12340
/// draws party and raid members from `PartyRaidBlips.blp` in class colours, not built.
const PARTY_DOT_CELL: [f32; 4] = [0.0, 0.25, 0.25, 0.5];
/// `MINIMAP_OBJECT_MONSTER_HOSTILE`, the red dot. Creature tracking draws every tracked unit
/// with it: 12340 picks 5/6/7 (and 2/3/4 for players) by reaction, which the blip pass lacks.
const CELL_MONSTER_HOSTILE: u32 = 5;
/// The gold dot of a tracked object: a resource node, a tracked town service or mailbox.
const CELL_TRACKED: u32 = 8;
/// A quest available, the yellow "!".
const CELL_QUEST_AVAILABLE: u32 = 9;
/// A quest ready to turn in, the yellow "?".
const CELL_QUEST_COMPLETE: u32 = 10;
/// A repeatable quest available, the blue "!".
const CELL_QUEST_REPEATABLE: u32 = 11;

/// The texture rectangle of `ObjectIcons.blp` cell `n`: the 3.3.5 file is 256×64, eight 32-px
/// cells a row, two rows.
fn object_icon_cell(n: u32) -> [f32; 4] {
    let u = (n & 7) as f32 * 0.125;
    let v = (n >> 3) as f32 * 0.5;
    [u, u + 0.125, v, v + 0.5]
}

/// The tracking-menu row the player chose (`minimapTrackedInfo`, read by
/// [`TrackedInfo::from_cvar`]), as the blip classifier tests it.
#[derive(Clone, Copy, Default)]
pub(in crate::minimap) struct TrackedInfo(Option<&'static TrackingType>);

impl TrackedInfo {
    pub(in crate::minimap) fn from_cvar(value: Option<&str>) -> Self {
        Self(value.and_then(tracked_info))
    }

    /// "Track Low Level Quests": the low-level available statuses 2 and 4 draw too.
    fn trivial(self) -> bool {
        self.0.is_some_and(|t| t.kind == TrackingKind::TrivialQuests)
    }

    /// A town-service row whose 3.3.5 NPC flag the unit carries.
    fn unit(self, npc_flags: u32) -> bool {
        self.0
            .is_some_and(|t| t.kind == TrackingKind::NpcFlag && npc_flags & t.flags != 0)
    }

    /// The mailbox row: a GameObject of its type.
    fn gameobject(self, type_id: u32) -> bool {
        self.0
            .is_some_and(|t| t.kind == TrackingKind::GameObjectType && type_id == t.flags)
    }
}
/// `UNIT_DYNAMIC_FLAGS` bit 0x2, set per viewer on a Hunter's Mark victim for its caster; one of
/// `0x5ed210`'s two always-show clauses (`+0x224 & 0x2`).
const UNIT_DYNFLAG_TRACK_UNIT: u32 = 0x2;
/// The quest cell of the server's raw 3.3.5 `QuestGiverStatus`, as 12340's classifier maps it:
/// REWARD (10) the "?", AVAILABLE (8) the "!", AVAILABLE_REP (7) the blue "!". With trivial
/// tracking on, LOW_LEVEL_AVAILABLE (2) and LOW_LEVEL_AVAILABLE_REP (4) join the two "!".
/// INCOMPLETE (5) and the rest draw nothing.
fn quest_dot_cell(status: u32, trivial: bool) -> Option<[f32; 4]> {
    let cell = match status {
        10 => CELL_QUEST_COMPLETE,
        8 => CELL_QUEST_AVAILABLE,
        2 if trivial => CELL_QUEST_AVAILABLE,
        7 => CELL_QUEST_REPEATABLE,
        4 if trivial => CELL_QUEST_REPEATABLE,
        _ => return None,
    };
    Some(object_icon_cell(cell))
}

/// The raw 3.3.5 NPC flags of a candidate, 0 before the Wrath feed delivered them.
fn npc_flags(raw: Option<&crate::wrath::WrathNpcFlags>) -> u32 {
    raw.map_or(0, |f| f.0)
}

/// The three gates `0x4eaa90` applies to a unit or player ahead of the quest-dot compare at
/// `0x4eac31`, so they bar the quest dot and the tracking dot alike, never a GameObject or party
/// dot. A unit with no descriptor yet fails, as a zeroed health field does in the reference.
fn unit_dot_eligible(store: Option<&ObjectStore>, me: Option<u64>) -> bool {
    let Some(f) = store.map(|s| &s.0) else {
        return false;
    };
    // 1. A signed `<= 0` on `UNIT_FIELD_HEALTH` (`jle` at `0x4eac1e`): the dead get no dot.
    if f.unit_health().unwrap_or(0) as i32 <= 0 {
        return false;
    }
    // 2. The owner, `UNIT_FIELD_CHARMEDBY` when non-zero else `UNIT_FIELD_SUMMONEDBY`, equal to
    //    our guid (`0x468550`) rejects: our own pet, minion or charmed unit is never a blip.
    let owner = match f.unit_charmed_by() {
        Some(g) if g != 0 => g,
        _ => f.unit_summoned_by().unwrap_or(0),
    };
    if owner != 0 && Some(owner) == me {
        return false;
    }
    // 3. `byte [eax+0x213] & 4`, `UNIT_FIELD_BYTES_1` byte 3 bit 2 (vmangos's untrackable name).
    !f.unit_is_untrackable()
}

/// Draw a quest blip per quest giver whose status [`quest_dot_cell`] maps, culled at the view
/// radius in 3-D distance with no rim arrow, in the map's own layer, under the arrow children.
/// A unit the tracking menu tracks takes the tracked dot instead: 12340 tests tracking first.
pub(in crate::minimap) fn emit_quest_dots(
    ctx: &BlipCtx,
    statuses: &HashMap<u64, u32>,
    info: TrackedInfo,
    candidates: &TrackedCandidates,
    self_guid: Option<u64>,
    icons: &Handle<Image>,
    player_indoors: bool,
    unit_indoors: impl Fn(Vec3) -> bool,
    quads: &mut UiQuads,
    hover: &mut MinimapBlipHover,
) {
    for (guid, net, tf, store, raw_flags) in candidates.iter() {
        let npc = guid.0;
        if !matches!(net.kind, EntityKind::Unit | EntityKind::Player) {
            // 12340 gives quest-giver GameObjects these cells too; their hover would need the
            // template name, not built.
            continue;
        }
        if info.unit(npc_flags(raw_flags)) {
            continue; // [`emit_tracking_dots`] drew the tracked dot
        }
        let Some(cell) = statuses
            .get(&npc)
            .copied()
            .and_then(|s| quest_dot_cell(s, info.trivial()))
        else {
            continue;
        };
        if !unit_dot_eligible(store, self_guid) {
            continue;
        }
        let w = bevy_to_wow(tf.translation());
        let d3 =
            ((w[0] - ctx.wx).powi(2) + (w[1] - ctx.wy).powi(2) + (w[2] - ctx.wz).powi(2)).sqrt();
        let fresh = (d3 <= ctx.radius_yd).then(|| ctx.offset(w));
        let Some(off) = super::stale_offset(super::DotSource::Unit(npc), fresh, ctx.px_per_yd)
        else {
            continue;
        };
        // The grey `0xffb0b0b0` marks a dot across an interior boundary; the reference decides
        // through `0x670540`, untraced, and this uses an indoor-containment mismatch.
        let grey = unit_indoors(tf.translation()) != player_indoors;
        let tint = if grey { 0xb0 as f32 / 255.0 } else { 1.0 };
        let rect = Rect::from_center_size(
            ctx.center + off,
            Vec2::splat(ctx.side * (QUEST_DOT_PX / BLIP_BASIS_PX)),
        );
        quads.overlays.push(UiQuad {
            rect,
            z_key: ctx.z,
            texture: Some(icons.clone()),
            uv: UvRect::from_tex_coords(cell),
            color: [tint, tint, tint, ctx.alpha],
            ..default()
        });
        if let Some(c) = ctx.cursor {
            if rect.contains(c) {
                *hover = MinimapBlipHover::Npc(npc, grey);
            }
        }
    }
}

/// Resource tracking (`0x5ed2b0`): any skill-keyed `Lock.dbc` slot of the lock whose `LockType`
/// `n` has bit `1 << (n − 1)` set in `PLAYER_TRACK_RESOURCES`, the bit the server sets from the
/// aura's MiscValue (vmangos `HandleAuraTrackResources`).
fn tracked_resource(mask: u32, lock_id: u32, locks: &LockCatalog) -> bool {
    if mask == 0 || lock_id == 0 {
        return false;
    }
    locks.slots(lock_id).is_some_and(|slots| {
        slots.iter().any(|s| {
            s.key_type == LOCK_KEY_SKILL
                && (1..=32).contains(&s.index)
                && mask & (1u32 << (s.index - 1)) != 0
        })
    })
}

/// Our tracking masks and the track-stealthed bit (`PLAYER_FIELD_BYTES & 0x2`).
#[derive(Clone, Copy, Default)]
pub(in crate::minimap) struct SelfTracking {
    pub(in crate::minimap) creatures: u32,
    pub(in crate::minimap) resources: u32,
    pub(in crate::minimap) stealthed: bool,
}

/// Creature tracking (`0x5ed210`): either always-show clause, `UNIT_DYNFLAG_TRACK_UNIT` or our
/// track-stealthed bit (aura 151) with the unit's CREEP flag, else the creature type's bit
/// `type − 1` in `PLAYER_TRACK_CREATURES`. The predicate itself tests no alive/dead or faction.
fn tracked_creature(
    tracking: SelfTracking,
    creature_type: Option<u32>,
    dynamic_flags: u32,
    unit_creeping: bool,
) -> bool {
    if dynamic_flags & UNIT_DYNFLAG_TRACK_UNIT != 0 {
        return true;
    }
    if tracking.stealthed && unit_creeping {
        return true;
    }
    tracking.creatures != 0
        && creature_type
            .is_some_and(|t| (1..=32).contains(&t) && tracking.creatures & (1u32 << (t - 1)) != 0)
}

/// Draw the tracking dots: the gold cell 8 per resource node or tracked mailbox, then per unit
/// the tracking menu tracks (ahead of its quest status, as 12340 tests), else the red cell 5 per
/// unit creature tracking finds that takes no quest blip. Drawn before the quest and party dots.
pub(in crate::minimap) fn emit_tracking_dots(
    ctx: &BlipCtx,
    tracking: SelfTracking,
    info: TrackedInfo,
    candidates: &TrackedCandidates,
    statuses: &HashMap<u64, u32>,
    self_guid: Option<u64>,
    names: &NameCache,
    templates: &GameObjectTemplates,
    locks: Option<&LockCatalog>,
    forms: Option<&HashMap<u32, ShapeshiftForm>>,
    icons: &Handle<Image>,
    player_indoors: bool,
    unit_indoors: impl Fn(Vec3) -> bool,
    quads: &mut UiQuads,
    hover: &mut MinimapBlipHover,
) {
    // The unit pass runs even with an empty creature mask: the always-show clauses need none.
    let mut dot =
        |key: super::DotSource, tf: &GlobalTransform, cell: [f32; 4], name: DotName| {
            let w = bevy_to_wow(tf.translation());
            let d3 = ((w[0] - ctx.wx).powi(2)
                + (w[1] - ctx.wy).powi(2)
                + (w[2] - ctx.wz).powi(2))
            .sqrt();
            let fresh = (d3 <= ctx.radius_yd).then(|| ctx.offset(w));
            let Some(off) = super::stale_offset(key, fresh, ctx.px_per_yd) else {
                return;
            };
            let grey = unit_indoors(tf.translation()) != player_indoors;
            let tint = if grey { 0xb0 as f32 / 255.0 } else { 1.0 };
            let rect = Rect::from_center_size(
                ctx.center + off,
                Vec2::splat(ctx.side * (QUEST_DOT_PX / BLIP_BASIS_PX)),
            );
            quads.overlays.push(UiQuad {
                rect,
                z_key: ctx.z,
                texture: Some(icons.clone()),
                uv: UvRect::from_tex_coords(cell),
                color: [tint, tint, tint, ctx.alpha],
                ..default()
            });
            if let Some(c) = ctx.cursor {
                if rect.contains(c) {
                    *hover = match name {
                        DotName::Guid(guid) => MinimapBlipHover::Npc(guid, grey),
                        DotName::Known(n) => MinimapBlipHover::TrackedGo(n, grey),
                    };
                }
            }
        };
    // Cell 8, GameObjects: the tracked mailbox, or a node resource tracking finds.
    let resources = locks.filter(|_| tracking.resources != 0);
    if resources.is_some() || info.0.is_some_and(|t| t.kind == TrackingKind::GameObjectType) {
        for (guid, net, tf, store, _) in candidates.iter() {
            if net.kind != EntityKind::GameObject {
                continue;
            }
            let Some(t) = store.and_then(|s| templates.of(&s.0)) else {
                continue; // template not answered yet
            };
            if info.gameobject(t.type_id)
                || resources.is_some_and(|l| tracked_resource(tracking.resources, t.lock_id, l))
            {
                dot(
                    super::DotSource::GameObject(guid.0),
                    tf,
                    object_icon_cell(CELL_TRACKED),
                    DotName::Known(t.name.clone()),
                );
            }
        }
    }
    // Units: the tracked service (cell 8), else creature tracking (cell 5).
    let types = CreatureTypeSources {
        names: Some(names),
        forms,
        ..Default::default()
    };
    for (guid, net, tf, store, raw_flags) in candidates.iter() {
        if !matches!(net.kind, EntityKind::Unit | EntityKind::Player) {
            continue;
        }
        if !unit_dot_eligible(store, self_guid) {
            continue;
        }
        let cell = if info.unit(npc_flags(raw_flags)) {
            CELL_TRACKED
        } else {
            let quest = statuses
                .get(&guid.0)
                .copied()
                .and_then(|s| quest_dot_cell(s, info.trivial()));
            if quest.is_some() {
                continue; // [`emit_quest_dots`] draws this unit
            }
            let (dyn_flags, creeping) = store
                .map(|s| (s.0.unit_dynamic_flags(), s.0.unit_is_stealthed()))
                .unwrap_or((0, false));
            let creature_type = store.map(|s| types.of(s));
            if !tracked_creature(tracking, creature_type, dyn_flags, creeping) {
                continue;
            }
            CELL_MONSTER_HOSTILE
        };
        dot(
            super::DotSource::Unit(guid.0),
            tf,
            object_icon_cell(cell),
            DotName::Guid(guid.0),
        );
    }
}

/// How a tracking dot's hover names itself: a unit resolves by guid through the name cache;
/// a GameObject's template name is already in hand.
enum DotName {
    Guid(u64),
    Known(String),
}

/// The party and pet dots, the in-range half of the party placement `0x6dad10`: blue cell 4 at
/// 1.3× for each of the five [`group_positions`] records within 0.8 of the radius
/// (`0x4ed6b0`-`0x4ed7a7`), drawn last in the map's own layer (`0x4ed7b7`), under every arrow
/// child.
pub(in crate::minimap) fn emit_party_dots(
    ctx: &BlipCtx,
    group: &crate::ui_party::GroupState,
    guids: &GuidIndex,
    unit_pos: &Query<&GlobalTransform, With<NetEntity>>,
    pet: Option<u64>,
    icons: &Handle<Image>,
    quads: &mut UiQuads,
) {
    for (slot, pos) in group_positions(group, guids, unit_pos, pet)
        .into_iter()
        .enumerate()
    {
        let Some((x, y)) = pos else {
            continue;
        };
        let d = ((x - ctx.wx).powi(2) + (y - ctx.wy).powi(2)).sqrt();
        let fresh =
            (d / ctx.radius_yd <= super::BLIP_EDGE_RATIO).then(|| ctx.offset([x, y, 0.0]));
        let Some(off) =
            super::stale_offset(super::DotSource::Party(slot as u8), fresh, ctx.px_per_yd)
        else {
            continue; // out of range: the arrow pass drew it
        };
        quads.overlays.push(UiQuad {
            rect: Rect::from_center_size(
                ctx.center + off,
                Vec2::splat(ctx.side * (PARTY_DOT_PX / BLIP_BASIS_PX)),
            ),
            z_key: ctx.z,
            texture: Some(icons.clone()),
            uv: UvRect::from_tex_coords(PARTY_DOT_CELL),
            color: [1.0, 1.0, 1.0, ctx.alpha],
            ..default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creature type 7, Humanoid: a player's, by race (`ChrRaces.dbc` column 9).
    const CREATURE_TYPE_HUMANOID: u32 = 7;

    /// The first row is the control: an ordinary live creature still passes.
    #[test]
    fn the_dead_our_own_minions_and_the_untrackable_draw_no_dot_of_either_kind() {
        use crate::net::ObjectStore;
        use benilla_protocol::ObjectFields;

        const FIELD_HEALTH: u16 = 22; // UNIT_FIELD_HEALTH
        const FIELD_SUMMONEDBY: u16 = 12; // UNIT_FIELD_SUMMONEDBY (2 dwords)
        const FIELD_CHARMEDBY: u16 = 10; // UNIT_FIELD_CHARMEDBY (2 dwords)
        const FIELD_BYTES_1: u16 = 138;
        /// `UNIT_FIELD_BYTES_1` byte 3 bit 2, the `& 4` the classifier tests.
        const UNTRACKABLE: u32 = 0x4 << 24;
        const ME: u64 = 0x0000_0000_0000_0007;
        const SOMEONE_ELSE: u64 = 0x0000_0000_0000_0042;

        let store = |pairs: &[(u16, u32)]| ObjectStore(ObjectFields::from_pairs(pairs));
        let alive = [(FIELD_HEALTH, 100u32)];
        let with = |extra: &[(u16, u32)]| {
            let mut v = alive.to_vec();
            v.extend_from_slice(extra);
            store(&v)
        };
        let lo = |g: u64| (g & 0xffff_ffff) as u32;
        let hi = |g: u64| (g >> 32) as u32;

        assert!(
            unit_dot_eligible(Some(&with(&[])), Some(ME)),
            "the control: an ordinary live creature still takes a dot"
        );
        assert!(
            !unit_dot_eligible(Some(&store(&[(FIELD_HEALTH, 0)])), Some(ME)),
            "the dead draw nothing — a SIGNED `<= 0` at 0x4eac1e"
        );
        assert!(
            !unit_dot_eligible(None, Some(ME)),
            "…and so does a candidate whose descriptor has not landed"
        );
        assert!(
            !unit_dot_eligible(
                Some(&with(&[
                    (FIELD_SUMMONEDBY, lo(ME)),
                    (FIELD_SUMMONEDBY + 1, hi(ME)),
                ])),
                Some(ME)
            ),
            "our own minion is not a blip — equality on the owner guid is the REJECT"
        );
        assert!(
            unit_dot_eligible(
                Some(&with(&[
                    (FIELD_SUMMONEDBY, lo(SOMEONE_ELSE)),
                    (FIELD_SUMMONEDBY + 1, hi(SOMEONE_ELSE)),
                ])),
                Some(ME)
            ),
            "…but somebody ELSE's minion still is"
        );
        assert!(
            !unit_dot_eligible(
                Some(&with(&[
                    (FIELD_CHARMEDBY, lo(ME)),
                    (FIELD_CHARMEDBY + 1, hi(ME)),
                    // A non-zero CHARMEDBY wins; SUMMONEDBY is only the fallback.
                    (FIELD_SUMMONEDBY, lo(SOMEONE_ELSE)),
                    (FIELD_SUMMONEDBY + 1, hi(SOMEONE_ELSE)),
                ])),
                Some(ME)
            ),
            "a unit WE charmed is not a blip, and charm outranks summon"
        );
        assert!(
            !unit_dot_eligible(Some(&with(&[(FIELD_BYTES_1, UNTRACKABLE)])), Some(ME)),
            "and the untrackable bit blanks it outright"
        );
    }

    /// The dot loop draws all five group records' dots (`0x4ed6b0`-`0x4ed7a7`, stride `0x50` to
    /// `0x190`), the pet's record (slot 4, `0x6dad8c`) among them: our pet in range draws the
    /// party's blue dot, and beyond 0.8 of the radius none, where its rim arrow is
    /// (`0x6daf5c`).
    #[test]
    fn the_pet_draws_the_party_dot_in_range() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let unit = |w: [f32; 3]| {
            (
                NetEntity {
                    kind: EntityKind::Unit,
                    display_id: None,
                    scale: 1.0,
                },
                GlobalTransform::from_translation(benilla_assets::coords::wow_to_bevy(w)),
            )
        };
        let near = world.spawn(unit([50.0, 0.0, 0.0])).id();
        let far = world.spawn(unit([120.0, 0.0, 0.0])).id();
        let mut guids = GuidIndex::default();
        guids.0.extend([(0xF140, near), (0xF141, far)]);
        world.insert_resource(guids);
        world.init_resource::<crate::ui_party::GroupState>();
        let dots = |world: &mut World, pet: Option<u64>| {
            world
                .run_system_once(
                    move |q: Query<&GlobalTransform, With<NetEntity>>,
                          guids: Res<GuidIndex>,
                          group: Res<crate::ui_party::GroupState>| {
                        let ctx = BlipCtx {
                            center: Vec2::ZERO,
                            side: BLIP_BASIS_PX,
                            px_per_yd: BLIP_BASIS_PX * 0.5 / 133.0,
                            radius_yd: 133.0,
                            z: 0,
                            alpha: 1.0,
                            wx: 0.0,
                            wy: 0.0,
                            wz: 0.0,
                            cursor: None,
                        };
                        let mut quads = UiQuads::default();
                        super::super::reset_stale_dots();
                        emit_party_dots(
                            &ctx,
                            &group,
                            &guids,
                            &q,
                            pet,
                            &Handle::default(),
                            &mut quads,
                        );
                        quads
                            .overlays
                            .iter()
                            .map(|q| (q.rect.center(), q.uv))
                            .collect::<Vec<_>>()
                    },
                )
                .unwrap()
        };
        let got = dots(&mut world, Some(0xF140));
        assert_eq!(got.len(), 1, "the pet in range: one dot");
        assert!(
            got[0].1 == UvRect::from_tex_coords(PARTY_DOT_CELL),
            "the party's cell"
        );
        assert!(
            got[0].0.y < 0.0 && got[0].0.x.abs() < 1e-3,
            "north of us, up: {:?}",
            got[0].0
        );
        assert!(
            dots(&mut world, Some(0xF141)).is_empty(),
            "past 0.8 of 133: the arrow's"
        );
        assert!(
            dots(&mut world, Some(0xF142)).is_empty(),
            "not streamed: nothing"
        );
        assert!(dots(&mut world, None).is_empty(), "no pet");
    }

    /// REWARD (10) is the "?" cell 10, AVAILABLE (8) the "!" cell 9 and AVAILABLE_REP (7) the
    /// blue "!" cell 11 on the 8×2 grid; the low-level 2 and 4 only with trivial tracking.
    #[test]
    fn quest_dot_cells_follow_the_wrath_status_map() {
        assert_eq!(quest_dot_cell(10, false), Some([0.25, 0.375, 0.5, 1.0]));
        assert_eq!(quest_dot_cell(8, false), Some([0.125, 0.25, 0.5, 1.0]));
        assert_eq!(quest_dot_cell(7, false), Some([0.375, 0.5, 0.5, 1.0]));
        assert_eq!(quest_dot_cell(2, true), quest_dot_cell(8, false));
        assert_eq!(quest_dot_cell(4, true), quest_dot_cell(7, false));
        for s in [0, 1, 2, 3, 4, 5, 6, 9] {
            assert_eq!(quest_dot_cell(s, false), None, "status {s} must not dot");
        }
    }

    /// Bit `1 << (n − 1)`, `n` the lock's skill-slot `LockType` or the creature type.
    #[test]
    fn tracking_predicates_follow_the_mask_bit_law() {
        use benilla_formats::{LockSlot, LOCK_KEY_ITEM, MAX_LOCK_SLOTS};
        // A Mining (LockType 3) skill slot, and a key-item slot with the same index.
        let mut vein = [LockSlot::default(); MAX_LOCK_SLOTS];
        vein[0] = LockSlot {
            key_type: LOCK_KEY_SKILL,
            index: 3,
            skill: 0,
            action: 0,
        };
        let mut keyed = [LockSlot::default(); MAX_LOCK_SLOTS];
        keyed[0] = LockSlot {
            key_type: LOCK_KEY_ITEM,
            index: 3,
            skill: 0,
            action: 0,
        };
        let locks = LockCatalog::from_rows([(38, vein), (40, keyed)]);
        assert!(tracked_resource(1 << 2, 38, &locks), "mining bit lights it");
        assert!(
            !tracked_resource(1 << 1, 38, &locks),
            "herbalism bit doesn't"
        );
        assert!(!tracked_resource(0, 38, &locks), "no mask, no dot");
        assert!(
            !tracked_resource(1 << 2, 0, &locks),
            "lockId 0 never tracks"
        );
        assert!(
            !tracked_resource(1 << 2, 40, &locks),
            "a key-ITEM slot's index is an item entry, not a LockType"
        );
        assert!(!tracked_resource(1 << 2, 99, &locks), "unknown lock id");

        // Track Beasts sets bit 0 (Beast is creature type 1).
        let beasts = SelfTracking {
            creatures: 1,
            ..default()
        };
        assert!(tracked_creature(beasts, Some(1), 0, false));
        assert!(!tracked_creature(
            beasts,
            Some(CREATURE_TYPE_HUMANOID),
            0,
            false
        ));
        assert!(!tracked_creature(
            SelfTracking::default(),
            Some(1),
            0,
            false
        ));
        assert!(
            !tracked_creature(beasts, None, 0, false),
            "type not cached yet — no dot"
        );
        // The always-show pair: Hunter's Mark needs no tracking aura; track-stealthed needs
        // both our bit and the unit's CREEP flag.
        assert!(tracked_creature(
            SelfTracking::default(),
            None,
            UNIT_DYNFLAG_TRACK_UNIT,
            false
        ));
        let stealth = SelfTracking {
            stealthed: true,
            ..default()
        };
        assert!(tracked_creature(stealth, None, 0, true));
        assert!(!tracked_creature(stealth, None, 0, false), "not sneaking");
        assert!(
            !tracked_creature(SelfTracking::default(), None, 0, true),
            "we don't track stealthed"
        );
    }

    /// The tracking spell's `EffectMiscValue`, the server's mask bit and the node's `Lock.dbc`
    /// skill slot, on the install's data.
    #[test]
    fn real_find_minerals_lights_a_copper_vein_not_an_herb() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let spells = benilla_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
        let locks = benilla_formats::load_lock_catalog(&mut chain).expect("Lock.dbc");
        let forms =
            benilla_formats::load_shapeshift_forms(&mut chain).expect("SpellShapeshiftForm.dbc");

        // Bit `1 << (MiscValue − 1)` of the aura-`kind` effect (44 creatures, 45 resources).
        let mask_of = |spell_id: u32, kind: u32| -> u32 {
            let s = spells.get(spell_id).expect("spell row");
            (0..3)
                .find_map(|i| {
                    (s.effect_apply_aura[i] == kind).then(|| {
                        let m = s.effect_misc_value[i];
                        assert!((1..=32).contains(&m), "MiscValue {m} out of mask range");
                        1u32 << (m - 1)
                    })
                })
                .expect("tracking effect present")
        };

        // Find Minerals 2580 and a Copper Vein (vmangos `gameobject_template` 1731, lockId 38).
        let minerals = mask_of(2580, 45);
        assert_eq!(minerals, 1 << 2, "Find Minerals' MiscValue is Mining (3)");
        assert!(tracked_resource(minerals, 38, &locks));
        // Find Herbs 2383 and Peacebloom or Silverleaf (lockId 29, Herbalism LockType 2).
        let herbs = mask_of(2383, 45);
        assert!(tracked_resource(herbs, 29, &locks));
        assert!(!tracked_resource(minerals, 29, &locks), "cross-profession");
        assert!(!tracked_resource(herbs, 38, &locks), "cross-profession");
        // Track Beasts 1494 (MiscValue 1, Beast) lights a cat-form (1) druid.
        let beasts = SelfTracking {
            creatures: mask_of(1494, 44),
            ..Default::default()
        };
        assert!(tracked_creature(beasts, Some(1), 0, false));
        assert!(!tracked_creature(
            beasts,
            Some(CREATURE_TYPE_HUMANOID),
            0,
            false
        ));
        // A cat-form player (`UNIT_FIELD_BYTES_1` byte 2 is form 1).
        let cat_druid = ObjectStore(benilla_protocol::ObjectFields::from_pairs(&[(
            138,
            1 << 16,
        )]));
        let cat = Some(
            CreatureTypeSources {
                names: Some(&NameCache::default()),
                forms: Some(&forms),
                ..Default::default()
            }
            .of(&cat_druid),
        );
        assert_eq!(cat, Some(1), "cat form resolves Beast (DBC col 12)");
        assert!(tracked_creature(beasts, cat, 0, false));
    }
}
