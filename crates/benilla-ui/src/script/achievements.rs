//! The Wrath achievement getters over the client-side tables and the server's earned/criteria
//! state. The catalog is projected from the app's achievement views; the earned set and criteria
//! counters are pushed by the session feed. The packet layer is a later slice: this module answers
//! the stock FrameXML's read paths.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mlua::{Lua, MultiValue, Table, Value};

use super::Model;

/// One Achievement_Category.dbc row, as the Lua getters read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AchievementCategoryView {
    pub id: u32,
    pub parent: i32,
    pub name: String,
    pub ui_order: i32,
}

/// One Achievement.dbc row, as the Lua getters read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AchievementView {
    pub id: u32,
    pub faction: i32,
    pub supercedes: i32,
    pub category: i32,
    pub points: i32,
    pub flags: i32,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub reward: String,
    pub minimum_criteria: i32,
    pub shares_criteria: i32,
}

/// One Achievement_Criteria.dbc row, as the Lua getters read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AchievementCriteriaView {
    pub id: u32,
    pub achievement_id: u32,
    pub kind: i32,
    pub asset_id: i32,
    pub quantity: i32,
    pub description: String,
    pub flags: i32,
}

/// The achievement tables indexed the way the getters read them.
#[derive(Clone, Debug, Default)]
pub struct AchievementCatalog {
    categories: Vec<(u32, AchievementCategoryView)>,
    achievements: Vec<(u32, AchievementView)>,
    achievements_by_id: HashMap<u32, usize>,
    criteria: Vec<AchievementCriteriaView>,
    criteria_by_id: HashMap<u32, usize>,
    criteria_by_achievement: HashMap<u32, Vec<usize>>,
}

impl AchievementCatalog {
    pub fn from_parts(
        categories: Vec<(u32, AchievementCategoryView)>,
        achievements: Vec<(u32, AchievementView)>,
        criteria: Vec<AchievementCriteriaView>,
    ) -> Self {
        let achievements_by_id = achievements
            .iter()
            .enumerate()
            .map(|(index, (id, _))| (*id, index))
            .collect();
        let mut criteria_by_id = HashMap::new();
        let mut criteria_by_achievement: HashMap<u32, Vec<usize>> = HashMap::new();
        for (index, row) in criteria.iter().enumerate() {
            criteria_by_id.insert(row.id, index);
            criteria_by_achievement
                .entry(row.achievement_id)
                .or_default()
                .push(index);
        }
        Self {
            categories,
            achievements,
            achievements_by_id,
            criteria,
            criteria_by_id,
            criteria_by_achievement,
        }
    }

    pub fn categories(&self) -> impl Iterator<Item = (u32, &AchievementCategoryView)> + '_ {
        self.categories.iter().map(|(id, row)| (*id, row))
    }

    pub fn achievements(&self) -> impl Iterator<Item = (u32, &AchievementView)> + '_ {
        self.achievements.iter().map(|(id, row)| (*id, row))
    }

    pub fn category(&self, id: u32) -> Option<&AchievementCategoryView> {
        self.categories
            .iter()
            .find_map(|(category_id, row)| (*category_id == id).then_some(row))
    }

    pub fn achievement(&self, id: u32) -> Option<&AchievementView> {
        self.achievements_by_id
            .get(&id)
            .map(|&index| &self.achievements[index].1)
    }

    pub fn criteria_for(
        &self,
        achievement_id: u32,
    ) -> impl Iterator<Item = &AchievementCriteriaView> + '_ {
        self.criteria_by_achievement
            .get(&achievement_id)
            .into_iter()
            .flatten()
            .map(|&index| &self.criteria[index])
    }

    pub fn criteria(&self, id: u32) -> Option<&AchievementCriteriaView> {
        self.criteria_by_id
            .get(&id)
            .map(|&index| &self.criteria[index])
    }

    pub fn len(&self) -> usize {
        self.achievements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.achievements.is_empty()
    }
}

/// The catalog and the session's earned/criteria/tracker state.
#[derive(Clone, Default)]
pub(crate) struct AchievementState {
    pub(crate) catalog: Option<Arc<AchievementCatalog>>,
    /// Earned achievement ids, with the server date (month, day, year), zero-based as the
    /// server sends it; the getters add the one Lua expects.
    pub(crate) completed: HashMap<u32, (i32, i32, i32)>,
    /// The order the server sent the earned rows in: `GetAchievementInfo(category, index)`
    /// walks the earned list before the tables.
    pub(crate) completed_order: Vec<u32>,
    /// Criteria counters by criteria id.
    pub(crate) criteria: HashMap<u32, u32>,
    /// Criteria ids the server reported recently, behind `GetLatestUpdatedStats`.
    pub(crate) recent_criteria: Vec<u32>,
    /// The comparison target's guid, behind the `GetComparison*` getters.
    pub(crate) inspect_unit: Option<u64>,
    /// The inspected character's earned rows, in the order the server sent them.
    pub(crate) inspect_completed: HashMap<u32, (i32, i32, i32)>,
    pub(crate) inspect_order: Vec<u32>,
    /// The inspected character's criteria counters.
    pub(crate) inspect_criteria: HashMap<u32, u32>,
    /// The inspected character's recent criteria.
    pub(crate) inspect_recent: Vec<u32>,
    /// The tracker's ids in insertion order.
    pub(crate) tracked: Vec<u32>,
    /// Completed quests, behind `GetQuestsCompleted`.
    pub(crate) completed_quests: HashSet<u32>,
}

impl super::UiScript {
    /// Seat the catalog the app projected from the three achievement DBCs.
    pub fn set_achievement_catalog(&mut self, catalog: Arc<AchievementCatalog>) {
        self.model_mut().achievements.catalog = Some(catalog);
    }

    /// Replace one achievement's earned date; None clears it.
    pub fn set_achievement_completed(&mut self, id: u32, date: Option<(i32, i32, i32)>) {
        let state = &mut self.model_mut().achievements;
        match date {
            Some(date) => {
                if state.completed.insert(id, date).is_none() {
                    state.completed_order.push(id);
                }
            }
            None => {
                if state.completed.remove(&id).is_some() {
                    state.completed_order.retain(|done| *done != id);
                }
            }
        }
    }

    /// Replace one criteria row's counter.
    pub fn set_achievement_criteria(&mut self, id: u32, counter: u32) {
        self.model_mut().achievements.criteria.insert(id, counter);
    }

    /// Replace the recent-criteria list behind `GetLatestUpdatedStats`.
    pub fn set_achievement_recent_criteria(&mut self, ids: Vec<u32>) {
        self.model_mut().achievements.recent_criteria = ids;
    }

    /// Point the comparison surface at a guid, or clear it; the comparison feed follows.
    pub fn set_achievement_inspection_unit(&mut self, guid: Option<u64>) {
        self.model_mut().achievements.inspect_unit = guid;
    }

    /// Replace one inspected achievement's earned date; `None` clears it.
    pub fn set_achievement_inspection_completed(&mut self, id: u32, date: Option<(i32, i32, i32)>) {
        let state = &mut self.model_mut().achievements;
        match date {
            Some(date) => {
                if state.inspect_completed.insert(id, date).is_none() {
                    state.inspect_order.push(id);
                }
            }
            None => {
                if state.inspect_completed.remove(&id).is_some() {
                    state.inspect_order.retain(|done| *done != id);
                }
            }
        }
    }

    /// Replace one inspected criteria row's counter.
    pub fn set_achievement_inspection_criteria(&mut self, id: u32, counter: u32) {
        self.model_mut()
            .achievements
            .inspect_criteria
            .insert(id, counter);
    }

    /// Replace the inspected character's recent-criteria list.
    pub fn set_achievement_inspection_recent(&mut self, ids: Vec<u32>) {
        self.model_mut().achievements.inspect_recent = ids;
    }

    /// Replace the tracker list.
    pub fn set_tracked_achievements(&mut self, ids: Vec<u32>) {
        self.model_mut().achievements.tracked = ids;
    }

    /// Replace the completed-quest set behind `GetQuestsCompleted`.
    pub fn set_completed_quests(&mut self, ids: Vec<u32>) {
        self.model_mut().achievements.completed_quests = ids.into_iter().collect();
    }
}

fn catalog(lua: &Lua) -> Option<Arc<AchievementCatalog>> {
    lua.app_data_ref::<Model>()
        .and_then(|model| model.achievements.catalog.clone())
        .filter(|catalog| !catalog.is_empty())
}

fn completed_date(lua: &Lua, id: u32) -> Option<(i32, i32, i32)> {
    lua.app_data_ref::<Model>()
        .and_then(|model| model.achievements.completed.get(&id).copied())
}

/// Whether a later row that supersedes `id` is earned
/// (`CGAchievementInfo::IsSupersededByCompleted`): an upgraded achievement hides the old one.
fn superseded_by_completed(
    catalog: &AchievementCatalog,
    completed: &HashMap<u32, (i32, i32, i32)>,
    id: u32,
) -> bool {
    catalog
        .achievements()
        .any(|(other, row)| row.supercedes as u32 == id && completed.contains_key(&other))
}

/// Whether a row sits outside the statistics tree (`CGAchievementInfo::IsRealAchievement`):
/// the statistics rows live under category 1 and are not counted as achievements.
fn is_real_achievement(catalog: &AchievementCatalog, row: &AchievementView) -> bool {
    if row.category <= 0 {
        return false;
    }
    let Some(category) = catalog.category(row.category as u32) else {
        return false;
    };
    if category.id == 1 {
        return false;
    }
    let mut parent = category.parent;
    while parent > 0 {
        let Some(row) = catalog.category(parent as u32) else {
            break;
        };
        if row.id == 1 {
            return false;
        }
        parent = row.parent;
    }
    true
}

/// The statistics text (`CGAchievementInfo::BuildOutputString`) for the counters we hold. The
/// date, money and per-day forms need the criteria packet's timer and asset fields; until that
/// feed exists those rows read as their decimal value.
fn statistic_text(
    catalog: &AchievementCatalog,
    row: &AchievementView,
    counters: &HashMap<u32, u32>,
    completed: bool,
) -> String {
    let source = if row.shares_criteria > 0 {
        catalog.achievement(row.shares_criteria as u32)
    } else {
        Some(row)
    };
    let rows: Vec<&AchievementCriteriaView> = source
        .map(|source| catalog.criteria_for(source.id).collect())
        .unwrap_or_default();
    let counter = |row: &AchievementCriteriaView| counters.get(&row.id).copied().unwrap_or(0);
    let value = rows.first().map(|row| counter(row)).unwrap_or(0);
    let sum: u32 = rows.iter().map(|row| counter(row)).sum();
    let highest = rows.iter().map(|row| counter(row)).max().unwrap_or(0);
    let count = rows
        .iter()
        .filter(|row| counter(row) >= row.quantity.max(0) as u32)
        .count() as u32;
    let result = if row.flags & 0x8 != 0 {
        sum
    } else if row.flags & 0x10 != 0 {
        highest
    } else if row.flags & 0x20 != 0 {
        count
    } else if value != 0 {
        // A fresh count is clamped to the criteria quantity unless the row shows progress
        // against a target (`flags & 0x1`).
        let quantity = rows.first().map_or(0, |row| row.quantity.max(0) as u32);
        if row.flags & 0x1 == 0 {
            value.min(quantity)
        } else {
            value
        }
    } else if completed && row.minimum_criteria == 0 {
        rows.first().map_or(0, |row| row.quantity.max(0) as u32)
    } else {
        0
    };
    if result == 0 && row.flags & 0x1 != 0 {
        "--".to_string()
    } else {
        result.to_string()
    }
}

fn number_arg(lua: &Lua, value: Value, usage: &'static str) -> mlua::Result<f64> {
    lua.coerce_number(value)?
        .filter(|n| n.is_finite())
        .ok_or_else(|| mlua::Error::runtime(usage))
}

fn info_values(
    lua: &Lua,
    row: &AchievementView,
    completed: Option<(i32, i32, i32)>,
) -> mlua::Result<Vec<Value>> {
    let (is_done, month, day, year) = match completed {
        Some((month, day, year)) => (
            true,
            Value::Integer(i64::from(month + 1)),
            Value::Integer(i64::from(day + 1)),
            Value::Integer(i64::from(year)),
        ),
        None => (false, Value::Nil, Value::Nil, Value::Nil),
    };
    Ok(vec![
        Value::Integer(i64::from(row.id)),
        Value::String(lua.create_string(&row.title)?),
        Value::Integer(i64::from(row.points)),
        Value::Boolean(is_done),
        month,
        day,
        year,
        Value::String(lua.create_string(&row.description)?),
        Value::Integer(i64::from(row.flags)),
        Value::String(lua.create_string(&row.icon)?),
        Value::String(lua.create_string(&row.reward)?),
    ])
}

fn criteria_values(
    lua: &Lua,
    row: &AchievementCriteriaView,
    achievement: Option<&AchievementView>,
    counter: u32,
    completed: bool,
) -> mlua::Result<Vec<Value>> {
    let quantity = row.quantity.max(0) as u32;
    let done =
        (completed && achievement.is_none_or(|a| a.minimum_criteria == 0)) || counter >= quantity;
    let shown = if done && achievement.is_some_and(|a| a.minimum_criteria == 0) {
        quantity
    } else {
        counter
    };
    let mut flags = row.flags;
    if achievement.is_some_and(|a| a.flags & 0x80 != 0) {
        flags |= 0x1;
    }
    Ok(vec![
        Value::String(lua.create_string(&row.description)?),
        Value::Integer(i64::from(row.kind)),
        Value::Boolean(done),
        Value::Integer(i64::from(shown)),
        Value::Integer(i64::from(quantity)),
        Value::Nil,
        Value::Integer(i64::from(flags)),
        Value::Integer(i64::from(row.asset_id)),
        Value::String(lua.create_string(&row.description)?),
        Value::Integer(i64::from(row.id)),
    ])
}

fn criteria_by_index(
    catalog: &AchievementCatalog,
    achievement_id: u32,
    index: i64,
) -> Option<&AchievementCriteriaView> {
    let achievement = catalog.achievement(achievement_id)?;
    let source = if achievement.shares_criteria > 0 {
        catalog.achievement(achievement.shares_criteria as u32)?
    } else {
        achievement
    };
    // `0xB0` achievements show their hidden criteria too (the reference sets `showAll`).
    let show_all = achievement.flags & 0xB0 != 0;
    let mut remaining = index;
    for row in catalog.criteria_for(source.id) {
        if row.flags & 0x2 != 0 && !show_all {
            continue;
        }
        remaining -= 1;
        if remaining == 0 {
            return Some(row);
        }
    }
    None
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "CanShowAchievementUI",
        lua.create_function(|lua, ()| Ok(catalog(lua).is_some()))?,
    )?;

    g.set(
        "GetCategoryList",
        lua.create_function(|lua, table: Option<Table>| {
            let out = table.unwrap_or(lua.create_table()?);
            let Some(catalog) = catalog(lua) else {
                return Ok(out);
            };
            let mut index = 1;
            for (id, row) in catalog.categories() {
                if id == 1 {
                    continue;
                }
                let mut cursor = row.parent;
                let mut under_statistics = false;
                while cursor != 0 {
                    if cursor == 1 {
                        under_statistics = true;
                        break;
                    }
                    match catalog.category(cursor as u32) {
                        Some(parent) => cursor = parent.parent,
                        None => break,
                    }
                }
                if under_statistics {
                    continue;
                }
                out.raw_set(index, id)?;
                index += 1;
            }
            Ok(out)
        })?,
    )?;

    g.set(
        "GetCategoryInfo",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetCategoryInfo(categoryID)")?;
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.category(id as u32) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(&row.name)?),
                Value::Integer(if row.parent == 1 {
                    -1
                } else {
                    i64::from(row.parent)
                }),
                Value::Integer(0),
            ]))
        })?,
    )?;

    g.set(
        "GetCategoryNumAchievements",
        lua.create_function(|lua, (id, include_all): (Value, Option<bool>)| {
            let id = number_arg(lua, id, "Usage: GetCategoryNumAchievements(categoryID)")? as i32;
            let Some(catalog) = catalog(lua) else {
                return Ok((0i64, 0i64));
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let done = &model.achievements.completed;
            // The aggregate categories -1 and -2 imply `includeAll`, as the reference sets it.
            let mut include_all = include_all.unwrap_or(false);
            let mut total = 0i64;
            let mut completed = 0i64;
            let mut previous = 0i32;
            for (achievement_id, row) in catalog.achievements() {
                if row.flags & 0x2 != 0 {
                    continue;
                }
                let any = if id == -1 {
                    row.flags & 0x1 == 0
                } else if id == -2 {
                    row.flags & 0x1 != 0
                } else {
                    row.category == id
                };
                if !any {
                    continue;
                }
                if id == -1 || id == -2 {
                    include_all = true;
                }
                let is_done = done.contains_key(&achievement_id);
                // Category 81 (feats of strength) and 0x800 rows only appear once earned.
                if (row.category == 81 || row.flags & 0x800 != 0) && !is_done {
                    continue;
                }
                // A row whose predecessor it supersedes replaces it in the count.
                if previous != 0 && previous == row.supercedes {
                    previous = achievement_id as i32;
                    if include_all {
                        total += 1;
                    }
                    continue;
                }
                if is_done {
                    if include_all || !superseded_by_completed(&catalog, done, achievement_id) {
                        completed += 1;
                        total += 1;
                    }
                    continue;
                }
                previous = achievement_id as i32;
                total += 1;
            }
            Ok((total, completed))
        })?,
    )?;

    g.set(
        "GetNumAchievements",
        lua.create_function(|lua, ()| Ok(catalog(lua).map_or(0i64, |c| c.len() as i64)))?,
    )?;

    g.set(
        "GetTotalAchievementPoints",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok(0i64);
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let points = model
                .achievements
                .completed
                .keys()
                .filter_map(|id| catalog.achievement(*id))
                .map(|row| i64::from(row.points))
                .sum();
            Ok(points)
        })?,
    )?;

    g.set(
        "GetNumCompletedAchievements",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok((0i64, 0i64));
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let done = &model.achievements.completed;
            let mut total = 0i64;
            let mut completed = 0i64;
            for (id, row) in catalog.achievements() {
                if row.flags & 0x2 != 0 || row.category == 81 {
                    continue;
                }
                if !is_real_achievement(&catalog, row) {
                    continue;
                }
                if done.contains_key(&id) {
                    completed += 1;
                    total += 1;
                } else if row.flags & 0x800 == 0 {
                    total += 1;
                }
            }
            Ok((total, completed))
        })?,
    )?;

    g.set(
        "HasCompletedAnyAchievement",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(!model.achievements.completed.is_empty())
        })?,
    )?;

    g.set(
        "GetAchievementCategory",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetAchievementCategory(achievementID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.achievement(id) else {
                return Ok(MultiValue::new());
            };
            if row.category == 0 || row.flags & 0x2 != 0 {
                return Ok(MultiValue::new());
            }
            Ok(MultiValue::from_vec(vec![Value::Integer(i64::from(
                row.category,
            ))]))
        })?,
    )?;

    g.set(
        "GetAchievementNumRewards",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetAchievementNumRewards(achievementID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(Value::Nil);
            };
            Ok(match catalog.achievement(id) {
                Some(row) if row.flags & 0x2 == 0 => Value::Integer(1),
                _ => Value::Nil,
            })
        })?,
    )?;

    g.set(
        "GetAchievementReward",
        lua.create_function(|lua, (id, _index): (Value, Value)| {
            let id = number_arg(
                lua,
                id,
                "Usage: GetAchievementNumRewards(achievementID, rewardIndex)",
            )? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(Value::Nil);
            };
            Ok(match catalog.achievement(id) {
                Some(row) if row.flags & 0x2 == 0 => Value::Integer(i64::from(row.points)),
                _ => Value::Nil,
            })
        })?,
    )?;

    g.set(
        "GetAchievementNumCriteria",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetAchievementNumCriteria(achievementID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(0i64);
            };
            let Some(achievement) = catalog.achievement(id) else {
                return Ok(0i64);
            };
            if achievement.flags & 0x2 != 0 {
                return Ok(0i64);
            }
            let source = if achievement.shares_criteria > 0 {
                achievement.shares_criteria as u32
            } else {
                id
            };
            // A `0xB0` achievement's own description counts as its first criteria.
            let own = i64::from(achievement.flags & 0xB0 != 0);
            let rows = catalog
                .criteria_for(source)
                .filter(|row| row.flags & 0x2 == 0)
                .count() as i64;
            Ok(own + rows)
        })?,
    )?;

    g.set(
        "GetAchievementCriteriaInfo",
        lua.create_function(|lua, args: MultiValue| {
            let Some(first) = args.front().cloned().filter(|v| !matches!(v, Value::Nil)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GetAchievementCrieriaInfo(achievementID, criteriaIndex)",
                ));
            };
            let first = number_arg(
                lua,
                first,
                "Usage: GetAchievementCrieriaInfo(achievementID, criteriaIndex)",
            )?;
            let second = args
                .get(1)
                .cloned()
                .and_then(|v| lua.coerce_number(v).ok().flatten());
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let row = match second {
                Some(index) => criteria_by_index(&catalog, first as u32, index as i64),
                None => catalog.criteria(first as u32),
            };
            let Some(row) = row else {
                return Ok(MultiValue::new());
            };
            let achievement = catalog.achievement(row.achievement_id);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let counter = model
                .achievements
                .criteria
                .get(&row.id)
                .copied()
                .unwrap_or(0);
            Ok(MultiValue::from_vec(criteria_values(
                lua,
                row,
                achievement,
                counter,
                model
                    .achievements
                    .completed
                    .contains_key(&row.achievement_id),
            )?))
        })?,
    )?;

    g.set(
        "GetAchievementInfo",
        lua.create_function(|lua, args: MultiValue| {
            let Some(first) = args.front().cloned().filter(|v| !matches!(v, Value::Nil)) else {
                return Err(mlua::Error::runtime(
                    "Usage: GetAchievementInfo(achievementID)",
                ));
            };
            let first = number_arg(lua, first, "Usage: GetAchievementInfo(achievementID)")?;
            let second = args
                .get(1)
                .cloned()
                .and_then(|v| lua.coerce_number(v).ok().flatten());
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let row = match second {
                Some(index) => {
                    let category = first as i32;
                    let mut remaining = index as i64;
                    if remaining < 1 {
                        return Ok(MultiValue::new());
                    }
                    let model = lua.app_data_ref::<Model>().expect("model app_data");
                    let state = &model.achievements;
                    let mut found: Option<u32> = None;
                    // The earned rows first, in the order the server sent them; the aggregate
                    // categories count only the unhidden ones.
                    for id in &state.completed_order {
                        let Some(candidate) = catalog.achievement(*id) else {
                            continue;
                        };
                        let eligible = if category == -1 {
                            candidate.flags & 0x1 == 0
                        } else if category == -2 {
                            candidate.flags & 0x1 != 0
                        } else {
                            candidate.category == category
                                && !superseded_by_completed(&catalog, &state.completed, *id)
                        };
                        if !eligible {
                            continue;
                        }
                        if category > 0 || candidate.flags & 0x2 == 0 {
                            remaining -= 1;
                        }
                        if remaining == 0 {
                            found = Some(*id);
                            break;
                        }
                    }
                    // Then the tables for what was never earned. The inspected list - the
                    // comparison feature - waits for its packet.
                    if found.is_none() && category != 81 {
                        let mut previous = 0i32;
                        for (id, candidate) in catalog.achievements() {
                            if category == -1 {
                                if candidate.flags & 0x1 != 0 || candidate.category == 81 {
                                    continue;
                                }
                                if state.completed.contains_key(&id) {
                                    continue;
                                }
                                if candidate.flags & 0x800 != 0 {
                                    continue;
                                }
                                if candidate.flags & 0x2 == 0 {
                                    remaining -= 1;
                                }
                                if remaining == 0 {
                                    found = Some(id);
                                    break;
                                }
                                continue;
                            }
                            if category == -2 {
                                if candidate.flags & 0x1 == 0 || candidate.category == 81 {
                                    continue;
                                }
                                if state.completed.contains_key(&id) {
                                    continue;
                                }
                                if candidate.flags & 0x800 != 0 {
                                    continue;
                                }
                                if candidate.flags & 0x2 == 0 {
                                    remaining -= 1;
                                }
                                if remaining == 0 {
                                    found = Some(id);
                                    break;
                                }
                                continue;
                            }
                            if candidate.category != category {
                                continue;
                            }
                            if previous != 0 && previous == candidate.supercedes {
                                previous = id as i32;
                                continue;
                            }
                            if state.completed.contains_key(&id) || candidate.flags & 0x800 != 0 {
                                continue;
                            }
                            previous = id as i32;
                            remaining -= 1;
                            if remaining == 0 {
                                found = Some(id);
                                break;
                            }
                        }
                    }
                    found.and_then(|id| catalog.achievement(id))
                }
                None => catalog.achievement(first as u32),
            };
            let Some(row) = row else {
                return Ok(MultiValue::new());
            };
            let done = completed_date(lua, row.id);
            // A hidden row is never readable, and a 0x800 row only once earned.
            if row.flags & 0x2 != 0 || (row.flags & 0x800 != 0 && done.is_none()) {
                return Ok(MultiValue::new());
            }
            Ok(MultiValue::from_vec(info_values(lua, row, done)?))
        })?,
    )?;

    g.set(
        "GetAchievementInfoFromCriteria",
        lua.create_function(|lua, id: Value| {
            let id =
                number_arg(lua, id, "Usage: GetAchievementInfoFromCriteria(criteriaID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(criteria) = catalog.criteria(id) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.achievement(criteria.achievement_id) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(criteria.achievement_id)),
                Value::String(lua.create_string(&row.title)?),
                Value::Integer(i64::from(row.points)),
                Value::String(lua.create_string(&row.description)?),
                Value::Integer(i64::from(row.flags)),
                Value::String(lua.create_string(&row.icon)?),
                Value::String(lua.create_string(&row.reward)?),
            ]))
        })?,
    )?;

    // `GetStatisticsCategoryList([table])` (`0x5B14B0`): the statistics tree's categories,
    // keyed 1..n in file order.
    g.set(
        "GetStatisticsCategoryList",
        lua.create_function(|lua, table: Option<Table>| {
            let out = table.unwrap_or(lua.create_table()?);
            let Some(catalog) = catalog(lua) else {
                return Ok(out);
            };
            let mut index = 1;
            for (id, row) in catalog.categories() {
                let mut cursor = row.parent;
                let mut under_statistics = false;
                while cursor > 0 {
                    if cursor == 1 {
                        under_statistics = true;
                        break;
                    }
                    match catalog.category(cursor as u32) {
                        Some(parent) => cursor = parent.parent,
                        None => break,
                    }
                }
                if !under_statistics {
                    continue;
                }
                out.raw_set(index, id)?;
                index += 1;
            }
            Ok(out)
        })?,
    )?;

    // `GetStatistic(achievementID)` (`0x5B5620`): the row's readable counter text.
    g.set(
        "GetStatistic",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetStatistic(achievementID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.achievement(id) else {
                return Ok(MultiValue::new());
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let text = statistic_text(
                &catalog,
                row,
                &model.achievements.criteria,
                model.achievements.completed.contains_key(&id),
            );
            Ok(MultiValue::from_vec(vec![Value::String(
                lua.create_string(&text)?,
            )]))
        })?,
    )?;

    // `GetLatestCompletedAchievements()` (`0x5B1C20`): up to five earned real achievements, in
    // the order the server sent them.
    g.set(
        "GetLatestCompletedAchievements",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let mut out = Vec::new();
            for id in &model.achievements.completed_order {
                if out.len() == 5 {
                    break;
                }
                let Some(row) = catalog.achievement(*id) else {
                    continue;
                };
                if is_real_achievement(&catalog, row) {
                    out.push(Value::Integer(i64::from(*id)));
                }
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    // `GetLatestUpdatedStats()` (`0x5B1C90`): up to five criteria the server reported recently.
    g.set(
        "GetLatestUpdatedStats",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(MultiValue::from_vec(
                model
                    .achievements
                    .recent_criteria
                    .iter()
                    .take(5)
                    .map(|id| Value::Integer(i64::from(*id)))
                    .collect(),
            ))
        })?,
    )?;

    // `QueryQuestsCompleted()` (`AchievementInfo_Scripts.cpp:33`): sends
    // `CMSG_QUERY_QUESTS_COMPLETED`; the app's session feed answers through
    // `GetQuestsCompleted` when the server replies.
    g.set("QueryQuestsCompleted", lua.create_function(|_, ()| Ok(()))?)?;

    g.set(
        "GetAchievementLink",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetAchievementLink(achievementID)")? as u32;
            let Some(catalog) = catalog(lua) else {
                return Ok(Value::Nil);
            };
            let Some(row) = catalog.achievement(id) else {
                return Ok(Value::Nil);
            };
            let guid = lua
                .app_data_ref::<Model>()
                .and_then(|model| model.unit("player").map(|unit| unit.guid))
                .unwrap_or(0);
            let (done, month, day, year) = match completed_date(lua, id) {
                Some((month, day, year)) => (1, month + 1, day + 1, year),
                None => (0, 1, 1, 0),
            };
            let link = format!(
                "|cffffff00|Hachievement:{id}:{guid:016X}:{done}:{month}:{day}:{year}:0:0:0:0|h[{}]|h|r",
                row.title
            );
            Ok(Value::String(lua.create_string(&link)?))
        })?,
    )?;

    // The comparison surface (`SetAchievementComparisonUnit`, `0x5B1AB0`): the client resolves
    // the token, remembers it and sends `CMSG_QUERY_INSPECT_ACHIEVEMENTS`. This host remembers
    // the target and answers from the inspected sets the feed fills; until it does, the
    // comparison is empty.
    g.set(
        "SetAchievementComparisonUnit",
        lua.create_function(|lua, token: Value| {
            let Some(token) = lua.coerce_string(token)? else {
                return Ok(false);
            };
            let token = token.to_str()?;
            let token: &str = &token;
            let guid = lua.app_data_ref::<Model>().and_then(|model| {
                model
                    .unit(token)
                    .or_else(|| model.unit_named(token))
                    .map(|unit| unit.guid)
            });
            let Some(guid) = guid else {
                return Ok(false);
            };
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .achievements
                .inspect_unit = Some(guid);
            Ok(true)
        })?,
    )?;
    g.set(
        "ClearAchievementComparisonUnit",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let state = &mut model.achievements;
            state.inspect_unit = None;
            state.inspect_completed.clear();
            state.inspect_order.clear();
            state.inspect_criteria.clear();
            state.inspect_recent.clear();
            Ok(())
        })?,
    )?;

    g.set(
        "GetAchievementComparisonInfo",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(
                lua,
                id,
                "Usage: GetAchievementComparisonInfo(achievementID)",
            )? as u32;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if model.achievements.inspect_unit.is_none() {
                return Ok(MultiValue::new());
            }
            let Some(date) = model.achievements.inspect_completed.get(&id).copied() else {
                return Ok(MultiValue::new());
            };
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.achievement(id) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(true),
                Value::Integer(i64::from(date.0 + 1)),
                Value::Integer(i64::from(date.1 + 1)),
                Value::Integer(i64::from(date.2)),
                Value::Integer(i64::from(row.flags)),
            ]))
        })?,
    )?;

    // `GetComparisonCategoryNumAchievements(categoryID)` (`0x5B4xx`): the inspected rows of a
    // category, hidden rows excluded.
    g.set(
        "GetComparisonCategoryNumAchievements",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(
                lua,
                id,
                "Usage: GetComparisonCategoryNumAchievements(categoryID)",
            )? as i32;
            let Some(catalog) = catalog(lua) else {
                return Ok(0i64);
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let state = &model.achievements;
            let mut count = 0i64;
            for (achievement_id, row) in catalog.achievements() {
                if row.flags & 0x2 != 0 {
                    continue;
                }
                let in_category = if id == -1 {
                    row.flags & 0x1 == 0
                } else if id == -2 {
                    row.flags & 0x1 != 0
                } else {
                    row.category == id
                };
                if !in_category || !state.inspect_completed.contains_key(&achievement_id) {
                    continue;
                }
                let include_all = id == -1 || id == -2;
                if include_all
                    || !superseded_by_completed(&catalog, &state.completed, achievement_id)
                {
                    count += 1;
                }
            }
            Ok(count)
        })?,
    )?;

    // `GetNumComparisonCompletedAchievements()` (`0x5B3DC0`): `(total, completed)` over the
    // inspected character.
    g.set(
        "GetNumComparisonCompletedAchievements",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok((0i64, 0i64));
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let state = &model.achievements;
            let mut total = 0i64;
            let mut completed = 0i64;
            for (id, row) in catalog.achievements() {
                if row.flags & 0x2 != 0 || row.category == 81 {
                    continue;
                }
                if !is_real_achievement(&catalog, row) {
                    continue;
                }
                if state.inspect_completed.contains_key(&id) {
                    completed += 1;
                    total += 1;
                } else if row.flags & 0x800 == 0 {
                    total += 1;
                }
            }
            Ok((total, completed))
        })?,
    )?;

    // `GetComparisonAchievementPoints()` (`0x5B1E40`).
    g.set(
        "GetComparisonAchievementPoints",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok(0i64);
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let points = model
                .achievements
                .inspect_completed
                .keys()
                .filter_map(|id| catalog.achievement(*id))
                .map(|row| i64::from(row.points))
                .sum();
            Ok(points)
        })?,
    )?;

    g.set(
        "GetComparisonStatistic",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: GetComparisonStatistic(achievementID)")? as u32;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if model.achievements.inspect_unit.is_none() {
                return Ok(MultiValue::new());
            }
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let Some(row) = catalog.achievement(id) else {
                return Ok(MultiValue::new());
            };
            let text = statistic_text(
                &catalog,
                row,
                &model.achievements.inspect_criteria,
                model.achievements.inspect_completed.contains_key(&id),
            );
            Ok(MultiValue::from_vec(vec![Value::String(
                lua.create_string(&text)?,
            )]))
        })?,
    )?;

    g.set(
        "GetLatestCompletedComparisonAchievements",
        lua.create_function(|lua, ()| {
            let Some(catalog) = catalog(lua) else {
                return Ok(MultiValue::new());
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let mut out = Vec::new();
            for id in &model.achievements.inspect_order {
                if out.len() == 5 {
                    break;
                }
                let Some(row) = catalog.achievement(*id) else {
                    continue;
                };
                if is_real_achievement(&catalog, row) {
                    out.push(Value::Integer(i64::from(*id)));
                }
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    g.set(
        "GetLatestUpdatedComparisonStats",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(MultiValue::from_vec(
                model
                    .achievements
                    .inspect_recent
                    .iter()
                    .take(5)
                    .map(|id| Value::Integer(i64::from(*id)))
                    .collect(),
            ))
        })?,
    )?;

    // Tracker (`0x5B2040`, `0x5B2090`, `0x5B4870`, `0x5B48D0`).
    g.set(
        "GetTrackedAchievements",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(MultiValue::from_vec(
                model
                    .achievements
                    .tracked
                    .iter()
                    .map(|id| Value::Integer(i64::from(*id)))
                    .collect(),
            ))
        })?,
    )?;
    g.set(
        "GetNumTrackedAchievements",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.achievements.tracked.len() as i64)
        })?,
    )?;
    g.set(
        "IsTrackedAchievement",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: IsTrackedAchievement(achievementID)")? as u32;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.achievements.tracked.contains(&id))
        })?,
    )?;
    g.set(
        "AddTrackedAchievement",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: AddTrackedAchievement(achievementID)")? as u32;
            let known = catalog(lua).is_some_and(|catalog| catalog.achievement(id).is_some());
            if id == 0 || !known {
                return Err(mlua::Error::runtime(
                    "Usage: AddTrackedAchievement(achievementID)",
                ));
            }
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if !model.achievements.tracked.contains(&id) {
                model.achievements.tracked.push(id);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "RemoveTrackedAchievement",
        lua.create_function(|lua, id: Value| {
            let id = number_arg(lua, id, "Usage: RemoveTrackedAchievement(achievementID)")? as u32;
            if id == 0 {
                return Err(mlua::Error::runtime(
                    "Usage: RemoveTrackedAchievement(achievementID)",
                ));
            }
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.achievements.tracked.retain(|tracked| *tracked != id);
            Ok(())
        })?,
    )?;

    // `GetQuestsCompleted([table])` (`0x5B5290`): fill the table id -> true.
    g.set(
        "GetQuestsCompleted",
        lua.create_function(|lua, table: Option<Table>| {
            let out = table.unwrap_or(lua.create_table()?);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            for id in &model.achievements.completed_quests {
                out.raw_set(*id, true)?;
            }
            Ok(out)
        })?,
    )?;

    Ok(())
}
