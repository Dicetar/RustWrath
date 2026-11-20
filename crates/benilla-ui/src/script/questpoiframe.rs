//! The 3.3.5a `QuestPOIFrame` widget's Lua surface (`CGQuestPOIFrame`): the blob style setters,
//! `DrawQuestBlob`, and the hover test `UpdateMouseOverTooltip` with its two readers. The stock
//! instance is `WorldMapFrame.xml`'s `WorldMapBlobFrame`, which `WorldMapFrame.lua` scales on
//! every map-size toggle (`:1328`, `:1375`) and draws the selected quest's areas on.
//!
//! The setters clamp as the reference's do: the alphas to 0..=255, then truncated to a byte; the
//! border scalar to 0..=10; the merge threshold to 0.1..=0.5; the spline points to 8..=30.
//! `DrawQuestBlob(questId, true)` turns a quest on only when its POI data is held (`AddQuest`
//! behind the POI lookup); `false` turns it off. The hover test reads the turned-on quests' live
//! blobs on the displayed map ([`super::quest_poi`]).
//!
//! Unsupported: the blob pixels. The state records what is drawn and the style it is drawn in; no
//! renderer paints the fill and border yet.

use mlua::{Lua, Table, Value};

use super::binding_abi::{self, flag_or_default};
use super::object::frame_handle_of;
use super::quest_poi::{blob_live, project_on_map};
use super::{Model, UiTarget};

use crate::widget::{KindState, QuestPoiState};

/// Registry key of the QuestPOIFrame method table (`kind_method_registries`).
pub(super) const REG_QUESTPOI_METHODS: &str = "__benilla_questpoiframe_methods";

/// Run `f` over a frame's QuestPOI state; any other receiver raises.
fn with_poi<T>(
    lua: &Lua,
    this: &Table,
    f: impl FnOnce(&mut QuestPoiState) -> T,
) -> mlua::Result<T> {
    let h = frame_handle_of(lua, this)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let frame = model
        .arena
        .frame_mut(h)
        .ok_or_else(|| mlua::Error::runtime("stale frame handle"))?;
    match &mut frame.kind_state {
        KindState::QuestPoi(st) => Ok(f(st)),
        _ => Err(mlua::Error::runtime(
            "Wrong object type for member function",
        )),
    }
}

/// A number argument, `lua_tonumber`'s 0 for anything else.
fn num(lua: &Lua, v: Value) -> f64 {
    binding_abi::coerced_number(lua, Some(v))
}

fn truthy(v: Option<Value>) -> bool {
    flag_or_default(v.as_ref(), 0) != 0
}

/// Even-odd point-in-polygon over map UV.
fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut hit = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// `UpdateMouseOverPOI`: the first drawn quest with a live blob under map UV `(x, y)`, and the
/// objectives of its blobs that hold it, at most four. Nothing off `0..=1` or with no area shown.
fn mouse_over(model: &Model, drawn: &[u32], x: f32, y: f32) -> (u32, Vec<i32>) {
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return (0, Vec::new());
    }
    for &quest in drawn {
        let Some(blobs) = model.quest_poi.pois.get(&quest) else {
            continue;
        };
        let mask = model.poi_objective_mask(quest);
        let mut objectives = Vec::new();
        for blob in blobs.iter().filter(|b| blob_live(mask, b)) {
            if blob.points.len() < 3 {
                continue;
            }
            let poly: Option<Vec<(f32, f32)>> = blob
                .points
                .iter()
                .map(|&(px, py)| {
                    let project = model.world_loc_projector.as_ref()?;
                    let wm = &model.worldmap;
                    project(
                        (wm.selection.0, wm.selection.1, wm.direct_area),
                        blob.map_id,
                        px as f32,
                        py as f32,
                    )
                })
                .collect();
            // A blob is tested only where its center is on the displayed map (`IsPOIOnCurrentMap`).
            let on_map = blob
                .center()
                .and_then(|(cx, cy)| project_on_map(model, blob.map_id, cx, cy))
                .is_some();
            if let Some(poly) = poly.filter(|_| on_map) {
                if inside(&poly, x, y) && objectives.len() < 4 {
                    objectives.push(blob.objective_index);
                }
            }
        }
        if !objectives.is_empty() {
            return (quest, objectives);
        }
    }
    (0, Vec::new())
}

/// Register the QuestPOIFrame methods (Wrath only).
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if lua.app_data_ref::<Model>().expect("model app_data").target != UiTarget::Wrath335 {
        return Ok(());
    }
    let m = lua.create_table()?;

    m.set(
        "SetFillTexture",
        lua.create_function(|lua, (this, path): (Table, Option<String>)| {
            with_poi(lua, &this, |st| st.fill_texture = path)
        })?,
    )?;
    m.set(
        "SetBorderTexture",
        lua.create_function(|lua, (this, path): (Table, Option<String>)| {
            with_poi(lua, &this, |st| st.border_texture = path)
        })?,
    )?;
    m.set(
        "SetFillAlpha",
        lua.create_function(|lua, (this, a): (Table, Value)| {
            let a = num(lua, a).clamp(0.0, 255.0) as u8;
            with_poi(lua, &this, |st| st.fill_alpha = a)
        })?,
    )?;
    m.set(
        "SetBorderAlpha",
        lua.create_function(|lua, (this, a): (Table, Value)| {
            let a = num(lua, a).clamp(0.0, 255.0) as u8;
            with_poi(lua, &this, |st| st.border_alpha = a)
        })?,
    )?;
    m.set(
        "SetBorderScalar",
        lua.create_function(|lua, (this, s): (Table, Value)| {
            let s = num(lua, s).clamp(0.0, 10.0) as f32;
            with_poi(lua, &this, |st| st.border_scalar = s)
        })?,
    )?;
    m.set(
        "SetMergeThreshold",
        lua.create_function(|lua, (this, t): (Table, Value)| {
            let t = num(lua, t).clamp(0.1, 0.5) as f32;
            with_poi(lua, &this, |st| st.merge_threshold = t)
        })?,
    )?;
    m.set(
        "SetNumSplinePoints",
        lua.create_function(|lua, (this, n): (Table, Value)| {
            let n = num(lua, n).clamp(8.0, 30.0) as u32;
            with_poi(lua, &this, |st| st.num_spline_points = n)
        })?,
    )?;
    m.set(
        "EnableSmoothing",
        lua.create_function(|lua, (this, on): (Table, Option<Value>)| {
            let on = truthy(on);
            with_poi(lua, &this, |st| st.smoothing = on)
        })?,
    )?;
    m.set(
        "EnableMerging",
        lua.create_function(|lua, (this, on): (Table, Option<Value>)| {
            let on = truthy(on);
            with_poi(lua, &this, |st| st.merging = on)
        })?,
    )?;
    // DrawQuestBlob(questId, draw): on only with the quest's POI held, off unconditionally.
    m.set(
        "DrawQuestBlob",
        lua.create_function(|lua, (this, quest, draw): (Table, Value, Option<Value>)| {
            let quest = num(lua, quest) as i64;
            let Ok(quest) = u32::try_from(quest) else {
                return Ok(());
            };
            let draw = truthy(draw);
            let held = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .quest_poi
                .pois
                .contains_key(&quest);
            with_poi(lua, &this, |st| {
                st.drawn.retain(|&q| q != quest);
                if draw && held {
                    st.drawn.push(quest);
                }
            })
        })?,
    )?;
    // UpdateQuestPOI(): a no-op in the reference.
    m.set(
        "UpdateQuestPOI",
        lua.create_function(|_, _this: Table| Ok(()))?,
    )?;
    // UpdateMouseOverTooltip(x, y) → questLogIndex, numObjectives, or nothing off every blob.
    m.set(
        "UpdateMouseOverTooltip",
        lua.create_function(|lua, (this, x, y): (Table, Value, Value)| {
            let (x, y) = (num(lua, x) as f32, num(lua, y) as f32);
            let drawn = with_poi(lua, &this, |st| st.drawn.clone())?;
            let (quest, objectives, index) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let (quest, objectives) = mouse_over(&model, &drawn, x, y);
                let index = model.quest_log_index_of(quest);
                (quest, objectives, index)
            };
            let count = objectives.len();
            with_poi(lua, &this, |st| {
                st.mouse_over_quest = quest;
                st.mouse_over_objectives = objectives;
            })?;
            Ok(match (quest, index) {
                (0, _) | (_, None) => mlua::MultiValue::new(),
                (_, Some(i)) => mlua::MultiValue::from_vec(vec![
                    Value::Integer(i as i64),
                    Value::Integer(count as i64),
                ]),
            })
        })?,
    )?;
    // GetNumTooltips() → the objectives under the cursor at the last hover test.
    m.set(
        "GetNumTooltips",
        lua.create_function(|lua, this: Table| {
            with_poi(lua, &this, |st| st.mouse_over_objectives.len() as i64)
        })?,
    )?;
    // GetTooltipIndex(i) → the i-th of them, 0 past the fourth.
    m.set(
        "GetTooltipIndex",
        lua.create_function(|lua, (this, i): (Table, Value)| {
            let i = num(lua, i) as i64 - 1;
            with_poi(lua, &this, |st| {
                if !(0..=3).contains(&i) {
                    0
                } else {
                    st.mouse_over_objectives
                        .get(i as usize)
                        .map_or(0, |&o| i64::from(o))
                }
            })
        })?,
    )?;

    lua.set_named_registry_value(REG_QUESTPOI_METHODS, m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{UiScript, UiTarget};

    #[test]
    fn quest_poi_frame_is_a_wrath_widget_with_the_reference_clamps() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.run(
            r#"
            blob = CreateFrame("QuestPOIFrame", "blob", UIParent)
            blob:SetScale(0.5)
            blob:SetFillAlpha(400)
            blob:SetBorderAlpha(-3)
            blob:SetNumSplinePoints(2)
            blob:DrawQuestBlob(42, true)
            "#,
        )
        .unwrap();
        assert_eq!(
            s.eval::<String>("return blob:GetObjectType()").unwrap(),
            "QuestPOIFrame"
        );
        assert!(s.eval::<bool>("return blob:IsObjectType('Frame')").unwrap());
        // No POI held for 42, so nothing is drawn and the hover finds nothing.
        assert_eq!(s.eval::<i64>("return blob:GetNumTooltips()").unwrap(), 0);
        assert_eq!(s.eval::<i64>("return blob:GetTooltipIndex(9)").unwrap(), 0);
        assert_eq!(s.arity("blob:UpdateMouseOverTooltip(0.5, 0.5)").unwrap(), 0);
    }
}
