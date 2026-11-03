//! Quest-log tag names: the quest template's `Type` → the suffix after a quest-log title.
//! `QuestInfo.dbc` (`ID`, the `Name` loc-string) holds the sparse ids 1 Elite (not the later
//! "Group"), 21 Life, 41 PvP, 62 Raid, 81 Dungeon, 82 World Event and 83 Legendary; `Type` 0, most
//! quests, has no tag. `GetQuestLogTitle` (`0x4df930`) reads it through `0x4df2a0`: the cached
//! template's `+0x10` (the 5th `u32` of `SMSG_QUEST_QUERY_RESPONSE`), bounds-checked against the
//! max id (`0xc0d9d0`), indexes `0xc0d9cc` directly, and a missing row reaches Lua as `nil`. The
//! full table ships in `patch-2.MPQ`; `dbc.MPQ` has only PvP, Life, Elite and Raid. `Type`'s one
//! other reader, the `PLAYER_QUEST_LOG` watcher (`0x5dde6b`, `0x5ddf02`), fires tutorial 40 when an
//! Elite quest is accepted.

use std::collections::HashMap;

use anyhow::Result;
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::load_id_name_table;

/// 12340 QuestXP: a quest level and ten reward difficulty columns.
pub fn load_quest_experience(chain: &mut Chain) -> Result<HashMap<u32, [u32; 10]>> {
    let bytes = chain.read_file("DBFilesClient\\QuestXP.dbc")?;
    let mut schema = Schema::new("QuestXP");
    for column in 0..11 {
        schema.add_field(SchemaField::new(format!("Column{column}"), FieldType::UInt32));
    }
    let records = crate::dbc::parse(&bytes, schema, "QuestXP")?;
    let mut result = HashMap::new();
    for record in records.records() {
        let level = crate::dbc::u32_at(record, 0).unwrap_or(0);
        let values = std::array::from_fn(|i| crate::dbc::u32_at(record, i + 1).unwrap_or(0));
        result.insert(level, values);
    }
    Ok(result)
}

/// `Type` → tag name.
#[derive(Debug, Default)]
pub struct QuestTagNames(HashMap<u32, String>);

impl QuestTagNames {
    /// The tag for a quest `Type`. `None` must reach Lua as `nil`, never `""`: the reference's
    /// quest-log row tests the tag's presence.
    pub fn resolve(&self, quest_type: u32) -> Option<&str> {
        self.0.get(&quest_type).map(String::as_str)
    }

    /// How many tags the table names (7 in 5875).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Load the tag names through the patch chain.
pub fn load_quest_tag_names(chain: &mut Chain) -> Result<QuestTagNames> {
    Ok(QuestTagNames(load_id_name_table(
        chain,
        "DBFilesClient\\QuestInfo.dbc",
        1,
        10,
        "QuestInfo",
    )?))
}

/// Localized 12340 CharTitles names, selected by gender with the reference's other-name fallback.
#[derive(Debug, Default)]
pub struct QuestTitleNames {
    male: HashMap<u32, String>,
    female: HashMap<u32, String>,
}

impl QuestTitleNames {
    /// The largest CharTitles record id, the value `GetNumTitles` answers plus one
    /// (`g_charTitlesDB.m_maxID + 1`, `0x005165E0`). Zero when the table is empty.
    pub fn max_id(&self) -> u32 {
        self.male
            .keys()
            .chain(self.female.keys())
            .copied()
            .max()
            .unwrap_or(0)
    }

    pub fn render(&self, id: u32, female: bool, player: &str) -> Option<String> {
        let (first, second) = if female { (&self.female, &self.male) } else { (&self.male, &self.female) };
        let format = first.get(&id).filter(|name| !name.is_empty()).or_else(|| second.get(&id))?;
        let mut output = String::new();
        let mut chars = format.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' { output.push(ch); continue; }
            match chars.next() {
                Some('s') => output.push_str(player),
                Some('%') => output.push('%'),
                Some(other) => { output.push('%'); output.push(other); }
                None => output.push('%'),
            }
        }
        Some(output)
    }
}

/// CharTitlesRec (37 columns): two sixteen-locale blocks at 2 and 19, each followed by its mask.
pub fn load_quest_title_names(chain: &mut Chain) -> Result<QuestTitleNames> {
    let file = "DBFilesClient\\CharTitles.dbc";
    Ok(QuestTitleNames {
        male: load_id_name_table(chain, file, 2, 37, "CharTitles")?,
        female: load_id_name_table(chain, file, 19, 37, "CharTitles")?,
    })
}

#[cfg(test)]
mod title_tests {
    use super::*;
    #[test]
    fn titles_choose_gender_fallback_and_format_the_player_name() {
        let names = QuestTitleNames {
            male: [(1, "Lord %s".into()), (2, "Master %s".into())].into(),
            female: [(1, "Lady %s".into()), (2, String::new())].into(),
        };
        assert_eq!(names.render(1, true, "Firna").as_deref(), Some("Lady Firna"));
        assert_eq!(names.render(1, false, "Test").as_deref(), Some("Lord Test"));
        assert_eq!(names.render(2, true, "Firna").as_deref(), Some("Master Firna"));
        assert_eq!(names.render(0, true, "Firna"), None);
    }
}
