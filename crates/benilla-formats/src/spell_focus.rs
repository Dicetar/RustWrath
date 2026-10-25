//! `SpellFocusObject.dbc`: the object a spell must be cast near (Anvil, Forge, …), named by the
//! crafting book's "Requires:" line from `Spell.dbc` `RequiresSpellFocus`. The server does the
//! proximity check (`Spell::CheckCast`); the client only names it.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{field_count, loc_str, parse, u32_at};
use crate::Chain;

const SPELL_FOCUS_OBJECT: &str = "DBFilesClient\\SpellFocusObject.dbc";
const SPELL_FOCUS_FIELDS: usize = 10;
const WRATH_SPELL_FOCUS_FIELDS: usize = 18;
const COL_NAME: usize = 1;

/// `SpellFocusObject` id → its name in the locale's column.
pub struct SpellFocusCatalog {
    names: HashMap<u32, String>,
}

impl SpellFocusCatalog {
    /// The name for a `RequiresSpellFocus` id; `None` for 0, which draws no requirement line.
    pub fn name(&self, focus_id: u32) -> Option<&str> {
        self.names.get(&focus_id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

fn schema(columns: usize) -> Schema {
    let mut s = Schema::new("SpellFocusObject");
    for i in 0..columns {
        let ty = if i == COL_NAME {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

/// Load SpellFocusObject.dbc from the patch chain into a [`SpellFocusCatalog`].
pub fn load_spell_focus_catalog(chain: &mut Chain) -> Result<SpellFocusCatalog> {
    let bytes = chain
        .read_file(SPELL_FOCUS_OBJECT)
        .with_context(|| format!("reading {SPELL_FOCUS_OBJECT}"))?;
    // DBCStructure.h::SpellFocusObjectEntry: ID followed by sixteen name locales and flags.
    let columns = match field_count(&bytes) {
        Some(SPELL_FOCUS_FIELDS) => SPELL_FOCUS_FIELDS,
        Some(WRATH_SPELL_FOCUS_FIELDS) => WRATH_SPELL_FOCUS_FIELDS,
        Some(columns) => bail!(
            "unsupported SpellFocusObject.dbc layout: {columns} fields (expected 10 or 18)"
        ),
        None => bail!("parsing SpellFocusObject.dbc header: truncated field count"),
    };
    let rs = parse(&bytes, schema(columns), "SpellFocusObject.dbc")?;
    let locale = chain.locale();
    let mut names = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        if let Some(name) = loc_str(&rs, r, COL_NAME, locale) {
            names.insert(id, name);
        }
    }
    Ok(SpellFocusCatalog { names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_spell_focus_names_the_profession_objects() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_spell_focus_catalog(&mut chain).expect("load SpellFocusObject.dbc");

        assert_eq!(cat.name(1), Some("Anvil"));
        assert_eq!(cat.name(3), Some("Forge"));
        assert_eq!(cat.name(4), Some("Cooking Fire"));
        assert_eq!(cat.name(0), None, "0 = no requirement");
        assert_eq!(cat.len(), 138, "the 5875 file's full row count");
    }
}
