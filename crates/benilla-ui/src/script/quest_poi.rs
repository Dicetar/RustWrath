//! The 3.3.5a quest points of interest: the per-quest area blobs `SMSG_QUEST_POI_QUERY_RESPONSE`
//! carries, the world map's visible-quest list over them, and the bindings the stock map and
//! watch frame read (`QuestMapUpdateAllQuests`, `QuestPOIUpdateIcons`,
//! `QuestPOIGetQuestIDByVisibleIndex`, `QuestPOIGetIconInfo`, `QuestPOIGetQuestIDByIndex`).
//!
//! The reference asks `CMSG_QUEST_POI_QUERY` for each log quest it holds no POI data for, keeps
//! the reply keyed by quest id with each blob's center the mean of its points, and fires
//! `QUEST_POI_UPDATE`. A blob is live for a quest when the quest is complete and the blob is its
//! turn-in (objective -1), or the quest is not and the blob's objective bit is set in
//! [`super::Model::poi_objective_mask`]. A live blob is on the displayed map when it is on its map
//! and projects inside it ([`super::model::WorldLocFn`]), and a blob flagged `4` only on the
//! `WorldMapArea` it names.
//!
//! Unsupported: dungeon floors (no `DungeonMap.dbc` owner, so a blob's floor is not tested), and
//! the blob fill/border pixels (`WorldMapBlobFrame:DrawQuestBlob` records the drawn quests and
//! the hover test answers from them, but no renderer paints the areas yet).

use std::collections::{HashMap, HashSet};

use mlua::{Lua, MultiValue, Value};

use super::binding_abi;
use super::Model;

/// One area of a quest's POI, as `SMSG_QUEST_POI_QUERY_RESPONSE` carries it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuestPoiBlobView {
    /// The objective it marks, -1 for the turn-in.
    pub objective_index: i32,
    pub map_id: u32,
    pub world_map_area_id: u32,
    pub floor: u32,
    /// The icon goes to the live blob with the least priority.
    pub priority: u32,
    pub flags: u32,
    /// The outline in world `(x, y)`.
    pub points: Vec<(i32, i32)>,
}

impl QuestPoiBlobView {
    /// The mean of the outline's points, the reference's blob center.
    pub fn center(&self) -> Option<(f32, f32)> {
        if self.points.is_empty() {
            return None;
        }
        let n = self.points.len() as f64;
        let (sx, sy) = self
            .points
            .iter()
            .fold((0.0f64, 0.0f64), |(sx, sy), &(x, y)| {
                (sx + f64::from(x), sy + f64::from(y))
            });
        Some(((sx / n) as f32, (sy / n) as f32))
    }
}

/// An icon `QuestPOIUpdateIcons` placed: the quest's best live blob on the displayed map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PoiIcon {
    pub complete: bool,
    pub uv: (f32, f32),
    pub objective: i32,
}

/// The POI data in hand, what is asked, and the map's visible list.
#[derive(Clone, Debug, Default)]
pub struct QuestPoiStore {
    /// The blobs by quest id, in reply order; a quest the server knows no POI for holds none.
    pub(crate) pois: HashMap<u32, Vec<QuestPoiBlobView>>,
    /// The quest ids in the order their data landed, `QuestPOIGetQuestIDByIndex`'s order.
    pub(crate) order: Vec<u32>,
    /// Asked once each, so an answerless quest is not asked again every frame.
    asked: HashSet<u32>,
    /// `QuestMapUpdateAllQuests`'s list: `(questId, questLogIndex)` in display order.
    pub(crate) visible: Vec<(u32, usize)>,
    /// `QuestPOIUpdateIcons`'s icons by quest id.
    pub(crate) icons: HashMap<u32, PoiIcon>,
}

/// Whether a blob is live under a quest's objective mask (`UpdateAllQuestPOI`).
pub(crate) fn blob_live(mask: u32, blob: &QuestPoiBlobView) -> bool {
    if mask == u32::MAX {
        blob.objective_index == -1
    } else {
        (0..32).contains(&blob.objective_index) && mask & (1 << blob.objective_index) != 0
    }
}

/// The displayed map's `WorldMapArea` id and `Map.dbc` id, `None` at world or continent level,
/// where the quest map lists nothing.
fn displayed_area(model: &Model) -> Option<(u32, Option<u32>)> {
    let wm = &model.worldmap;
    if let Some(id) = wm.direct_area {
        return Some((id, None));
    }
    let (c, z) = wm.selection;
    if c == 0 || z == 0 {
        return None;
    }
    let cont = wm.continents.get(c as usize - 1)?;
    let zone = cont.zones.get(z as usize - 1)?;
    Some((zone.wma_id, Some(cont.map_id)))
}

/// A blob's point projected onto the displayed map, `None` off it (`IsPOIOnCurrentMap`).
pub(crate) fn project_on_map(model: &Model, map: u32, x: f32, y: f32) -> Option<(f32, f32)> {
    let wm = &model.worldmap;
    let project = model.world_loc_projector.as_ref()?;
    let uv = project((wm.selection.0, wm.selection.1, wm.direct_area), map, x, y)?;
    // `|x·y| >= 0.001`, the reference's not-at-the-origin guard.
    ((0.0..=1.0).contains(&uv.0) && (0.0..=1.0).contains(&uv.1) && (uv.0 * uv.1).abs() >= 0.001)
        .then_some(uv)
}

/// A blob shown on the displayed map: on its map and inside it, and a `flags & 4` blob only on the
/// area it names.
pub(crate) fn blob_on_map(model: &Model, wma: u32, blob: &QuestPoiBlobView) -> Option<(f32, f32)> {
    if blob.flags & 4 != 0 && blob.world_map_area_id != wma {
        return None;
    }
    let (x, y) = blob.center()?;
    project_on_map(model, blob.map_id, x, y)
}

/// `QuestMapUpdateAllQuests`: the log quests with a live blob on the displayed map, in the
/// reference's three passes: complete or objective-less quests, then watched ones in watch order,
/// then the rest in log order. Stored for `QuestPOIGetQuestIDByVisibleIndex`.
fn update_all_quests(model: &mut Model) -> usize {
    let Some((wma, _)) = displayed_area(model) else {
        model.quest_poi.visible.clear();
        return 0;
    };
    let mut shown: Vec<(u32, usize, u32)> = Vec::new();
    for (n, row) in model.quest_log.entries.iter().enumerate() {
        if row.is_header {
            continue;
        }
        let Some(blobs) = model.quest_poi.pois.get(&row.quest_id) else {
            continue;
        };
        let mask = model.poi_objective_mask(row.quest_id);
        let on_map = blobs
            .iter()
            .any(|b| blob_live(mask, b) && blob_on_map(model, wma, b).is_some());
        if on_map {
            shown.push((row.quest_id, n + 1, mask));
        }
    }
    let watched = &model.quest_log_watched;
    let first = shown.iter().filter(|(_, _, m)| *m == u32::MAX || *m == 0);
    let incomplete = |&&(_, _, m): &&(u32, usize, u32)| m != u32::MAX && m != 0;
    let in_watch_order = watched
        .iter()
        .filter_map(|id| shown.iter().filter(incomplete).find(|(q, _, _)| q == id));
    let rest = shown
        .iter()
        .filter(incomplete)
        .filter(|(q, _, _)| !watched.contains(q));
    let visible: Vec<(u32, usize)> = first
        .chain(in_watch_order)
        .chain(rest)
        .map(|&(q, i, _)| (q, i))
        .collect();
    let count = visible.len();
    model.quest_poi.visible = visible;
    count
}

/// `UpdatePOIIcons`: each log quest's icon at its best live blob on the displayed map, the least
/// priority first, then the one nearest the player.
fn update_icons(model: &mut Model) {
    let mut icons = HashMap::new();
    if let Some((wma, _)) = displayed_area(model) {
        let player = model.worldmap.player_uv;
        for row in model.quest_log.entries.iter().filter(|r| !r.is_header) {
            let Some(blobs) = model.quest_poi.pois.get(&row.quest_id) else {
                continue;
            };
            let mask = model.poi_objective_mask(row.quest_id);
            let best = blobs
                .iter()
                .filter(|b| blob_live(mask, b))
                .filter_map(|b| blob_on_map(model, wma, b).map(|uv| (b, uv)))
                .min_by(|(a, ua), (b, ub)| {
                    let dist = |uv: &(f32, f32)| {
                        player.map_or(0.0, |p| (uv.0 - p.0).powi(2) + (uv.1 - p.1).powi(2))
                    };
                    a.priority
                        .cmp(&b.priority)
                        .then(dist(ua).total_cmp(&dist(ub)))
                });
            if let Some((b, uv)) = best {
                icons.insert(
                    row.quest_id,
                    PoiIcon {
                        complete: mask == u32::MAX,
                        uv,
                        objective: b.objective_index,
                    },
                );
            }
        }
    }
    model.quest_poi.icons = icons;
}

impl super::UiScript {
    /// The log quests with no POI data yet, each asked once: the app sends them in one
    /// `CMSG_QUEST_POI_QUERY`.
    pub fn take_quest_poi_asks(&mut self) -> Vec<u32> {
        let mut model = self.model_mut();
        let model = &mut *model;
        let mut asks = Vec::new();
        for row in model.quest_log.entries.iter().filter(|r| !r.is_header) {
            if row.quest_id != 0
                && !model.quest_poi.pois.contains_key(&row.quest_id)
                && model.quest_poi.asked.insert(row.quest_id)
            {
                asks.push(row.quest_id);
            }
        }
        asks
    }

    /// A reply landed: the blobs by quest, held, then `QUEST_POI_UPDATE` (the reference's
    /// `HandlePOIQueryResponse` signal).
    pub fn land_quest_pois(&mut self, pois: Vec<(u32, Vec<QuestPoiBlobView>)>) {
        {
            let mut model = self.model_mut();
            for (quest, blobs) in pois {
                if !model.quest_poi.order.contains(&quest) {
                    model.quest_poi.order.push(quest);
                }
                model.quest_poi.pois.insert(quest, blobs);
            }
        }
        self.fire_event("QUEST_POI_UPDATE", Vec::new());
    }
}

/// Register the QuestPOI bindings (Wrath only; the caller gates).
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    fn model_mut(lua: &Lua) -> mlua::AppDataRefMut<'_, Model> {
        lua.app_data_mut::<Model>().expect("model app_data")
    }

    // QuestMapUpdateAllQuests() → the visible count; 0 off zone and instance maps.
    g.set(
        "QuestMapUpdateAllQuests",
        lua.create_function(move |lua, ()| Ok(update_all_quests(&mut model_mut(lua)) as i64))?,
    )?;
    // QuestPOIUpdateIcons(): re-seat every quest's icon on the displayed map.
    g.set(
        "QuestPOIUpdateIcons",
        lua.create_function(move |lua, ()| {
            update_icons(&mut model_mut(lua));
            Ok(())
        })?,
    )?;
    // QuestPOIUpdateTexture(): the reference's body is `return 0`.
    g.set(
        "QuestPOIUpdateTexture",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    // QuestPOIGetQuestIDByIndex(index) → the quest id of the index-th POI held, in landing order.
    g.set(
        "QuestPOIGetQuestIDByIndex",
        lua.create_function(|lua, index: Value| -> mlua::Result<MultiValue> {
            let index =
                binding_abi::number_arg(lua, index, "Usage: QuestPOIGetQuestIDByIndex(index)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let quest = usize::try_from(index)
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| model.quest_poi.order.get(i));
            Ok(match quest {
                Some(&q) => MultiValue::from_vec(vec![Value::Integer(i64::from(q))]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    // QuestPOIGetQuestIDByVisibleIndex(index) → questId, questLogIndex from the last
    // `QuestMapUpdateAllQuests`; nothing past it.
    g.set(
        "QuestPOIGetQuestIDByVisibleIndex",
        lua.create_function(|lua, index: Value| -> mlua::Result<MultiValue> {
            let index = binding_abi::number_arg(
                lua,
                index,
                "Usage: Script_QuestPOIGetQuestIDByVisibleIndex(index)",
            )?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let hit = usize::try_from(index)
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| model.quest_poi.visible.get(i));
            Ok(match hit {
                Some(&(q, i)) => MultiValue::from_vec(vec![
                    Value::Integer(i64::from(q)),
                    Value::Integer(i as i64),
                ]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    // QuestPOIGetIconInfo(questID) → complete, x, y, objective from the last
    // `QuestPOIUpdateIcons`; nothing for a quest without an icon on the displayed map.
    g.set(
        "QuestPOIGetIconInfo",
        lua.create_function(|lua, quest: Value| -> mlua::Result<MultiValue> {
            let quest = binding_abi::number_arg(lua, quest, "Usage: QuestPOIGetIconInfo(questID)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(icon) = u32::try_from(quest)
                .ok()
                .and_then(|q| model.quest_poi.icons.get(&q))
            else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(icon.complete),
                Value::Number(f64::from(icon.uv.0)),
                Value::Number(f64::from(icon.uv.1)),
                Value::Integer(i64::from(icon.objective)),
            ]))
        })?,
    )?;
    Ok(())
}
