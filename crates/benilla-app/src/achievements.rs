//! The achievement catalog behind the Wrath achievement getters: `Achievement.dbc`,
//! `Achievement_Criteria.dbc` and `Achievement_Category.dbc` projected into the UI script's
//! tables once per client.
//!
//! The earned set, criteria counters and the tracker are the session feed's, not this module's:
//! the server is the only authority on what a character has done. Until the packet slice lands
//! the catalog alone answers the stock FrameXML's read paths - category lists, names, points,
//! criteria text and links - with nothing marked earned.

use std::sync::Arc;

use bevy::prelude::*;

use benilla_formats::{
    Achievement as FormatsAchievement, AchievementCatalog as FormatsCatalog,
    AchievementCategory as FormatsCategory, AchievementCriteria as FormatsCriteria,
};
use benilla_ui::script::{
    AchievementCatalog, AchievementCategoryView, AchievementCriteriaView, AchievementView,
};

use benilla_assets::{AssetSet, LockRecover, WorldAssets};

/// Project the DBC catalog into the UI script's read views. The formats rows carry more columns
/// than any getter reads; the projection keeps what `GetAchievementInfo` and its neighbours
/// answer and drops the rest.
pub(crate) fn project(catalog: &FormatsCatalog) -> AchievementCatalog {
    let categories = catalog
        .categories()
        .map(|(id, row)| (id, category_view(row)))
        .collect();
    let achievements = catalog
        .achievements()
        .map(|(id, row)| (id, achievement_view(row)))
        .collect();
    let criteria = catalog.criteria_rows().map(criteria_view).collect();
    AchievementCatalog::from_parts(categories, achievements, criteria)
}

fn category_view(row: &FormatsCategory) -> AchievementCategoryView {
    AchievementCategoryView {
        id: row.id,
        parent: row.parent,
        name: row.name.clone(),
        ui_order: row.ui_order,
    }
}

fn achievement_view(row: &FormatsAchievement) -> AchievementView {
    AchievementView {
        id: row.id,
        faction: row.faction,
        supercedes: row.supercedes,
        category: row.category,
        points: row.points,
        flags: row.flags,
        title: row.title.clone(),
        description: row.description.clone(),
        icon: row.icon.clone(),
        reward: row.reward.clone(),
        minimum_criteria: row.minimum_criteria,
        shares_criteria: row.shares_criteria,
    }
}

fn criteria_view(row: &FormatsCriteria) -> AchievementCriteriaView {
    AchievementCriteriaView {
        id: row.id,
        achievement_id: row.achievement_id,
        kind: row.kind,
        asset_id: row.asset_id,
        quantity: row.quantity,
        description: row.description.clone(),
        flags: row.flags,
    }
}

/// The shared projected catalog; absent, the achievement getters answer empty.
#[derive(Resource)]
pub(crate) struct AchievementRes(pub(crate) Arc<AchievementCatalog>);

fn load_achievements(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = {
        let mut chain = assets.chain.lock_recover();
        benilla_formats::load_achievement_catalog(&mut chain)
    };
    match loaded {
        Ok(catalog) => {
            info!("achievements: {} rows in Achievement.dbc", catalog.len());
            commands.insert_resource(AchievementRes(Arc::new(project(&catalog))));
        }
        Err(e) => warn!("achievements: the DBCs failed - no achievement catalog: {e:#}"),
    }
}

/// Seat the catalog into a new VM once. `CanShowAchievementUI` and every getter read it from
/// there; a VM that predates the load keeps its empty answer until the next session.
fn feed_achievements(
    mut vm: crate::script_calls::VmWith<Option<Res<'static, AchievementRes>>>,
    mut seated: Local<crate::ui_script::VmMemo<bool>>,
) {
    let catalog = vm.own().as_ref().map(|res| res.0.clone());
    let Some(catalog) = catalog else { return };
    vm.run(|script| {
        let Some(script) = script else { return };
        if !std::mem::replace(seated.get(script), true) {
            script.set_achievement_catalog(catalog);
        }
    });
}

pub(crate) struct AchievementsPlugin;

impl Plugin for AchievementsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_achievements.after(AssetSet::Open))
            .add_systems(Update, feed_achievements.after(crate::ui_script::UiInput));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_formats::{AchievementCatalog as FormatsCatalog, AchievementCategory};

    fn catalog() -> FormatsCatalog {
        let categories = vec![
            (
                1,
                AchievementCategory {
                    id: 1,
                    parent: -1,
                    name: "Statistics".into(),
                    ui_order: 0,
                },
            ),
            (
                92,
                AchievementCategory {
                    id: 92,
                    parent: 1,
                    name: "General".into(),
                    ui_order: 0,
                },
            ),
            (
                128,
                AchievementCategory {
                    id: 128,
                    parent: 92,
                    name: "World".into(),
                    ui_order: 1,
                },
            ),
        ];
        let achievements = vec![(
            6,
            FormatsAchievement {
                id: 6,
                category: 92,
                points: 10,
                title: "Level 10".into(),
                description: "Reach level 10.".into(),
                icon: "Interface\\Icons\\Achievement_Level_10".into(),
                ..Default::default()
            },
        )];
        FormatsCatalog::from_parts(categories, achievements, Vec::new())
    }

    #[test]
    fn the_projection_keeps_the_fields_the_getters_read() {
        let projected = project(&catalog());
        assert_eq!(projected.len(), 1);
        let row = projected.achievement(6).expect("row 6");
        assert_eq!(row.title, "Level 10");
        assert_eq!(row.points, 10);
        assert_eq!(row.category, 92);
        assert_eq!(
            projected.category(92).map(|c| c.name.as_str()),
            Some("General")
        );
    }
}
