//! The Wrath achievement tables: `Achievement_Category.dbc`, `Achievement.dbc` and
//! `Achievement_Criteria.dbc`, the client-side catalog behind the achievement UI. The server
//! only sends which rows are earned and their criteria progress; the titles, descriptions,
//! categories, points and icons are this table's.
//!
//! Layouts are the 3.3.5a `AchievementRec`/`Achievement_CriteriaRec`/`Achievement_CategoryRec`
//! shapes: 62, 31 and 20 columns, with a sixteen-locale string block plus its flags dword for
//! each title/description/reward/name.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{field_count, i32_at, load_spell_icon_map, loc_str, parse, u32_at};
use crate::Chain;

const ACHIEVEMENT: &str = "DBFilesClient\\Achievement.dbc";
const ACHIEVEMENT_CRITERIA: &str = "DBFilesClient\\Achievement_Criteria.dbc";
const ACHIEVEMENT_CATEGORY: &str = "DBFilesClient\\Achievement_Category.dbc";

/// One `Achievement_Category.dbc` row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AchievementCategory {
    pub id: u32,
    /// The parent category; `1` is the root, as `GetCategoryInfo` answers it as -1.
    pub parent: i32,
    pub name: String,
    pub ui_order: i32,
}

/// One `Achievement.dbc` row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Achievement {
    pub id: u32,
    /// `-1` for both factions.
    pub faction: i32,
    pub instance_id: i32,
    /// The achievement this one supersedes, 0 for none.
    pub supercedes: i32,
    pub title: String,
    pub description: String,
    pub category: i32,
    pub points: i32,
    pub ui_order: i32,
    pub flags: i32,
    /// The `SpellIcon.dbc` texture path, empty while unresolved.
    pub icon: String,
    pub reward: String,
    pub minimum_criteria: i32,
    pub shares_criteria: i32,
}

/// One `Achievement_Criteria.dbc` row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AchievementCriteria {
    pub id: u32,
    pub achievement_id: u32,
    /// `CriteriaType` (`AchievementCriteria.dbc`'s `Type` meaning).
    pub kind: i32,
    pub asset_id: i32,
    pub quantity: i32,
    pub start_event: i32,
    pub start_asset: i32,
    pub fail_event: i32,
    pub fail_asset: i32,
    pub description: String,
    pub flags: i32,
    pub timer_start_event: i32,
    pub timer_asset_id: i32,
    pub timer_time: i32,
    pub ui_order: i32,
}

/// The three achievement tables, indexed the way the Lua getters read them.
#[derive(Clone, Debug, Default)]
pub struct AchievementCatalog {
    categories: Vec<(u32, AchievementCategory)>,
    achievements: Vec<(u32, Achievement)>,
    achievements_by_id: HashMap<u32, usize>,
    criteria: Vec<AchievementCriteria>,
    criteria_by_id: HashMap<u32, usize>,
    criteria_by_achievement: HashMap<u32, Vec<usize>>,
}

impl AchievementCatalog {
    /// A catalog over explicit rows, for tests and fixtures.
    pub fn from_parts(
        categories: Vec<(u32, AchievementCategory)>,
        achievements: Vec<(u32, Achievement)>,
        criteria: Vec<AchievementCriteria>,
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

    /// Every category in file order.
    pub fn categories(&self) -> impl Iterator<Item = (u32, &AchievementCategory)> + '_ {
        self.categories.iter().map(|(id, row)| (*id, row))
    }

    /// Every achievement in file order, the order a category list indexes.
    pub fn achievements(&self) -> impl Iterator<Item = (u32, &Achievement)> + '_ {
        self.achievements.iter().map(|(id, row)| (*id, row))
    }

    pub fn category(&self, id: u32) -> Option<&AchievementCategory> {
        self.categories
            .iter()
            .find_map(|(category_id, row)| (*category_id == id).then_some(row))
    }

    pub fn achievement(&self, id: u32) -> Option<&Achievement> {
        self.achievements_by_id
            .get(&id)
            .map(|&index| &self.achievements[index].1)
    }

    /// The criteria rows of one achievement, in file order.
    pub fn criteria_for(
        &self,
        achievement_id: u32,
    ) -> impl Iterator<Item = &AchievementCriteria> + '_ {
        self.criteria_by_achievement
            .get(&achievement_id)
            .into_iter()
            .flatten()
            .map(|&index| &self.criteria[index])
    }

    /// Every criteria row in file order.
    pub fn criteria_rows(&self) -> impl Iterator<Item = &AchievementCriteria> + '_ {
        self.criteria.iter()
    }

    pub fn criteria(&self, id: u32) -> Option<&AchievementCriteria> {
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

fn loc(s: &mut Schema, name: &str) {
    s.add_field(SchemaField::new(name, FieldType::String));
    s.add_field(SchemaField::new_array(
        format!("{name}OtherLocales"),
        FieldType::String,
        15,
    ));
    s.add_field(SchemaField::new(format!("{name}Flags"), FieldType::UInt32));
}

fn achievement_schema() -> Schema {
    let mut s = Schema::new("Achievement");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Faction", FieldType::Int32));
    s.add_field(SchemaField::new("InstanceID", FieldType::Int32));
    s.add_field(SchemaField::new("Supercedes", FieldType::Int32));
    loc(&mut s, "Title");
    loc(&mut s, "Description");
    for name in ["Category", "Points", "UiOrder", "Flags", "IconID"] {
        s.add_field(SchemaField::new(name, FieldType::Int32));
    }
    loc(&mut s, "Reward");
    s.add_field(SchemaField::new("MinimumCriteria", FieldType::Int32));
    s.add_field(SchemaField::new("SharesCriteria", FieldType::Int32));
    s
}

fn criteria_schema() -> Schema {
    let mut s = Schema::new("Achievement_Criteria");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("AchievementID", FieldType::UInt32));
    for name in [
        "Type",
        "AssetID",
        "Quantity",
        "StartEvent",
        "StartAsset",
        "FailEvent",
        "FailAsset",
    ] {
        s.add_field(SchemaField::new(name, FieldType::Int32));
    }
    loc(&mut s, "Description");
    for name in [
        "Flags",
        "TimerStartEvent",
        "TimerAssetID",
        "TimerTime",
        "UiOrder",
    ] {
        s.add_field(SchemaField::new(name, FieldType::Int32));
    }
    s
}

fn category_schema() -> Schema {
    let mut s = Schema::new("Achievement_Category");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Parent", FieldType::Int32));
    loc(&mut s, "Name");
    s.add_field(SchemaField::new("UiOrder", FieldType::Int32));
    s
}

/// Load all three achievement tables off the patch chain. The catalog is empty when the chain
/// has no achievement files; a malformed table is an error, not a partial catalog.
pub fn load_achievement_catalog(chain: &mut Chain) -> Result<AchievementCatalog> {
    let icons = load_spell_icon_map(chain).unwrap_or_default();
    let locale = chain.locale();

    let category_bytes = chain
        .read_file(ACHIEVEMENT_CATEGORY)
        .with_context(|| format!("reading {ACHIEVEMENT_CATEGORY}"))?;
    if field_count(&category_bytes) != Some(20) {
        bail!("{ACHIEVEMENT_CATEGORY}: expected 20 fields");
    }
    let category_set = parse(&category_bytes, category_schema(), "Achievement_Category")?;
    let mut categories = Vec::with_capacity(category_set.records().len());
    for r in category_set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        categories.push((
            id,
            AchievementCategory {
                id,
                parent: i32_at(r, 1).unwrap_or(0),
                name: loc_str(&category_set, r, 2, locale).unwrap_or_default(),
                ui_order: i32_at(r, 19).unwrap_or(0),
            },
        ));
    }

    let achievement_bytes = chain
        .read_file(ACHIEVEMENT)
        .with_context(|| format!("reading {ACHIEVEMENT}"))?;
    if field_count(&achievement_bytes) != Some(62) {
        bail!("{ACHIEVEMENT}: expected 62 fields");
    }
    let achievement_set = parse(&achievement_bytes, achievement_schema(), "Achievement")?;
    let mut achievements = Vec::with_capacity(achievement_set.records().len());
    for r in achievement_set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let icon_id = i32_at(r, 42).unwrap_or(0);
        achievements.push((
            id,
            Achievement {
                id,
                faction: i32_at(r, 1).unwrap_or(0),
                instance_id: i32_at(r, 2).unwrap_or(0),
                supercedes: i32_at(r, 3).unwrap_or(0),
                title: loc_str(&achievement_set, r, 4, locale).unwrap_or_default(),
                description: loc_str(&achievement_set, r, 21, locale).unwrap_or_default(),
                category: i32_at(r, 38).unwrap_or(0),
                points: i32_at(r, 39).unwrap_or(0),
                ui_order: i32_at(r, 40).unwrap_or(0),
                flags: i32_at(r, 41).unwrap_or(0),
                icon: icons.get(&(icon_id as u32)).cloned().unwrap_or_default(),
                reward: loc_str(&achievement_set, r, 43, locale).unwrap_or_default(),
                minimum_criteria: i32_at(r, 60).unwrap_or(0),
                shares_criteria: i32_at(r, 61).unwrap_or(0),
            },
        ));
    }

    let criteria_bytes = chain
        .read_file(ACHIEVEMENT_CRITERIA)
        .with_context(|| format!("reading {ACHIEVEMENT_CRITERIA}"))?;
    if field_count(&criteria_bytes) != Some(31) {
        bail!("{ACHIEVEMENT_CRITERIA}: expected 31 fields");
    }
    let criteria_set = parse(&criteria_bytes, criteria_schema(), "Achievement_Criteria")?;
    let mut criteria = Vec::with_capacity(criteria_set.records().len());
    for r in criteria_set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let Some(achievement_id) = u32_at(r, 1) else {
            continue;
        };
        criteria.push(AchievementCriteria {
            id,
            achievement_id,
            kind: i32_at(r, 2).unwrap_or(0),
            asset_id: i32_at(r, 3).unwrap_or(0),
            quantity: i32_at(r, 4).unwrap_or(0),
            start_event: i32_at(r, 5).unwrap_or(0),
            start_asset: i32_at(r, 6).unwrap_or(0),
            fail_event: i32_at(r, 7).unwrap_or(0),
            fail_asset: i32_at(r, 8).unwrap_or(0),
            description: loc_str(&criteria_set, r, 9, locale).unwrap_or_default(),
            flags: i32_at(r, 26).unwrap_or(0),
            timer_start_event: i32_at(r, 27).unwrap_or(0),
            timer_asset_id: i32_at(r, 28).unwrap_or(0),
            timer_time: i32_at(r, 29).unwrap_or(0),
            ui_order: i32_at(r, 30).unwrap_or(0),
        });
    }

    Ok(AchievementCatalog::from_parts(
        categories,
        achievements,
        criteria,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u32, category: i32, supercedes: i32) -> (u32, Achievement) {
        (
            id,
            Achievement {
                id,
                category,
                supercedes,
                title: format!("Achievement {id}"),
                ..Default::default()
            },
        )
    }

    #[test]
    fn the_indexes_keep_file_order_and_group_criteria_by_achievement() {
        let categories = vec![(
            1,
            AchievementCategory {
                id: 1,
                parent: 1,
                name: "General".into(),
                ui_order: 0,
            },
        )];
        let achievements = vec![row(10, 1, 0), row(11, 1, 0)];
        let criteria = vec![
            AchievementCriteria {
                id: 100,
                achievement_id: 10,
                ..Default::default()
            },
            AchievementCriteria {
                id: 101,
                achievement_id: 10,
                ..Default::default()
            },
        ];
        let catalog = AchievementCatalog::from_parts(categories, achievements, criteria);
        assert_eq!(catalog.len(), 2);
        assert_eq!(
            catalog.achievement(11).map(|a| a.title.as_str()),
            Some("Achievement 11")
        );
        assert_eq!(
            catalog.category(1).map(|c| c.name.as_str()),
            Some("General")
        );
        assert_eq!(catalog.criteria_for(10).count(), 2);
        assert_eq!(catalog.criteria(101).map(|c| c.achievement_id), Some(10));
        assert!(catalog.achievement(12).is_none());
    }

    #[test]
    fn the_real_tables_load_and_keep_their_rows() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let catalog = load_achievement_catalog(&mut chain).expect("load achievements");
        assert!(catalog.len() > 500, "only {} achievements", catalog.len());
        assert!(catalog.categories().count() > 20);
        let first = catalog.achievements().next().expect("one achievement");
        assert!(
            !first.1.title.is_empty(),
            "achievement {} has no title",
            first.0
        );
        assert!(catalog.criteria_for(first.0).count() > 0);
    }
}
