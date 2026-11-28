//! The achievement getters over a seated catalog: the category tree, the table reads, the
//! earned-date shaping, the criteria tuple and the link.

use super::common::wrath_script;
use crate::script::*;

/// A small catalog shaped like the real one: Statistics under category 1, two real categories, a
/// hidden row, a feat-of-strength row and one criteria row.
fn push_catalog(s: &mut UiScript) {
    let categories = vec![
        (
            1,
            AchievementCategoryView {
                id: 1,
                parent: -1,
                name: "Statistics".into(),
                ui_order: 0,
            },
        ),
        (
            92,
            AchievementCategoryView {
                id: 92,
                parent: -1,
                name: "General".into(),
                ui_order: 0,
            },
        ),
        (
            128,
            AchievementCategoryView {
                id: 128,
                parent: 92,
                name: "World".into(),
                ui_order: 1,
            },
        ),
    ];
    let achievements = vec![
        (
            6,
            AchievementView {
                id: 6,
                category: 92,
                points: 10,
                title: "Level 10".into(),
                description: "Reach level 10.".into(),
                icon: "Interface\\Icons\\Achievement_Level_10".into(),
                reward: "A reward.".into(),
                ..Default::default()
            },
        ),
        (
            7,
            AchievementView {
                id: 7,
                category: 92,
                flags: 0x2,
                title: "Hidden".into(),
                ..Default::default()
            },
        ),
        (
            8,
            AchievementView {
                id: 8,
                category: 92,
                flags: 0x800,
                title: "Feat".into(),
                ..Default::default()
            },
        ),
        (
            9,
            AchievementView {
                id: 9,
                category: 128,
                points: 5,
                title: "Explorer".into(),
                ..Default::default()
            },
        ),
    ];
    let criteria = vec![AchievementCriteriaView {
        id: 100,
        achievement_id: 6,
        kind: 1,
        asset_id: 0,
        quantity: 10,
        description: "Reach level 10.".into(),
        flags: 0,
    }];
    s.set_achievement_catalog(std::sync::Arc::new(AchievementCatalog::from_parts(
        categories,
        achievements,
        criteria,
    )));
}

#[test]
fn the_category_tree_answers_the_stock_queries() {
    let mut s = wrath_script();
    assert!(!s.eval::<bool>("return CanShowAchievementUI()").unwrap());
    push_catalog(&mut s);
    assert!(s.eval::<bool>("return CanShowAchievementUI()").unwrap());

    // Statistics (1) and its subtree are not offered; the parent of a top-level category is -1.
    assert_eq!(
        s.eval::<(i64, i64, i64, bool)>(
            "local t = GetCategoryList() return t[1], t[2], t[3] or 0, t[1] == 1"
        )
        .unwrap(),
        (92, 128, 0, false),
        "the top-level list is General then World, never Statistics"
    );
    assert_eq!(
        s.eval::<(String, i64, i64)>("return GetCategoryInfo(92)")
            .unwrap(),
        ("General".into(), -1, 0)
    );
    assert_eq!(
        s.eval::<(String, i64, i64)>("return GetCategoryInfo(128)")
            .unwrap(),
        ("World".into(), 92, 0)
    );

    // Hidden and unearned feat rows are not counted; the normal one is.
    assert_eq!(
        s.eval::<(i64, i64)>("return GetCategoryNumAchievements(92)")
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        s.eval::<(i64, i64)>("return GetCategoryNumAchievements(-1)")
            .unwrap(),
        (2, 0),
        "the aggregate is every unhidden row outside category 81 and 0x800"
    );
    assert_eq!(s.eval::<i64>("return GetNumAchievements()").unwrap(), 4);
    assert_eq!(
        s.eval::<(i64, i64)>("return GetNumCompletedAchievements()")
            .unwrap(),
        (2, 0),
        "the hidden and feat rows are not real achievements yet"
    );
    assert_eq!(
        s.eval::<i64>("return GetTotalAchievementPoints()").unwrap(),
        0
    );
}

#[test]
fn achievement_info_reads_the_table_and_the_link_carries_its_date() {
    let mut s = wrath_script();
    push_catalog(&mut s);

    assert_eq!(
        s.eval::<(i64, String, i64, bool, Option<i64>, String, i64, String)>(
            "local id, name, points, done, month = GetAchievementInfo(6) \
             local desc = select(8, GetAchievementInfo(6)) \
             return id, name, points, done, month, desc, select(9, GetAchievementInfo(6)), select(10, GetAchievementInfo(6))"
        )
        .unwrap(),
        (
            6,
            "Level 10".into(),
            10,
            false,
            None,
            "Reach level 10.".into(),
            0,
            "Interface\\Icons\\Achievement_Level_10".into()
        )
    );
    // The category index path finds the same row.
    assert_eq!(
        s.eval::<i64>("return (GetAchievementInfo(92, 1))").unwrap(),
        6
    );
    assert_eq!(
        s.eval::<i64>("return GetAchievementCategory(6)").unwrap(),
        92
    );
    assert!(
        s.eval::<bool>("return GetAchievementInfo(7) == nil")
            .unwrap(),
        "a hidden row is not readable directly"
    );

    let link = s
        .eval::<String>("return GetAchievementLink(6)")
        .expect("a link");
    assert!(
        link.starts_with("|cffffff00|Hachievement:6:") && link.ends_with("|h[Level 10]|h|r"),
        "unexpected link {link}"
    );
    assert!(
        link.contains(":0:1:1:0:0:0:0:0|h["),
        "an earned date is zero until the packets land: {link}"
    );

    s.set_achievement_completed(6, Some((10, 4, 2010)));
    assert_eq!(
        s.eval::<(bool, i64, i64, i64)>(
            "local _, _, _, done, month, day, year = GetAchievementInfo(6) return done, month, day, year"
        )
        .unwrap(),
        (true, 11, 5, 2010),
        "the tables store the server's zero-based month and day"
    );
    assert_eq!(
        s.eval::<(i64, i64)>("return GetNumCompletedAchievements()")
            .unwrap(),
        (2, 1)
    );
    assert_eq!(
        s.eval::<i64>("return GetTotalAchievementPoints()").unwrap(),
        10
    );
    assert!(s
        .eval::<String>("return GetAchievementLink(6)")
        .unwrap()
        .contains(":1:11:5:2010:0:0:0:0|h["));
    assert!(s
        .eval::<bool>("return HasCompletedAnyAchievement()")
        .unwrap());
}

#[test]
fn criteria_answer_the_tuple_and_the_sharing_achievement() {
    let mut s = wrath_script();
    push_catalog(&mut s);

    assert_eq!(
        s.eval::<i64>("return GetAchievementNumCriteria(6)")
            .unwrap(),
        1
    );
    // The tuple shape is (text, kind, done, shown, required, name, flags, asset, output, id).
    let tuple = s
        .eval::<(
            String,
            i64,
            bool,
            i64,
            i64,
            Option<String>,
            i64,
            i64,
            String,
            i64,
        )>("return GetAchievementCriteriaInfo(6, 1)")
        .expect("the criteria tuple");
    assert_eq!(tuple.0, "Reach level 10.");
    assert_eq!(tuple.1, 1);
    assert!(!tuple.2);
    assert_eq!((tuple.3, tuple.4), (0, 10));
    assert_eq!(tuple.5, None);
    assert_eq!(tuple.9, 100);
    // A direct criteria id resolves the same row.
    assert_eq!(
        s.eval::<String>("return (GetAchievementCriteriaInfo(100))")
            .unwrap(),
        "Reach level 10."
    );

    s.set_achievement_criteria(100, 4);
    let shown = s
        .eval::<(bool, i64)>(
            "local _, _, done, shown = GetAchievementCriteriaInfo(6, 1) return done, shown",
        )
        .unwrap();
    assert_eq!(shown, (false, 4));

    assert_eq!(
        s.eval::<(i64, String, i64, String)>(
            "local id, name, points, desc = GetAchievementInfoFromCriteria(100) return id, name, points, desc"
        )
        .unwrap(),
        (6, "Level 10".into(), 10, "Reach level 10.".into())
    );
}
