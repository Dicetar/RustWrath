//! `ItemRandomProperties.dbc`, the random-suffix table: the "of the Monkey" roll an item carries
//! and the enchants it grants. The name formatter `0x5d8b00` joins name and suffix through
//! `ITEM_SUFFIX_TEMPLATE`, taking its plain-name exit (`0x5d8ba5`) for an id that is
//! `0`, negative, past the table or suffix-less and its suffix exit (`0x5d8b84`) otherwise. The
//! tooltip (`0x52b7bf`-`0x52b7fb`) copies the row's five enchant ids (`row+0x8..+0x18`) into its
//! enchant slots 2..6 for `0x52c991` to print, so a linked item shows its stat lines, not
//! `<Random enchantment>`. An item object carries its own enchants (`ITEM_FIELD_ENCHANTMENT`
//! 2..6) and reads the table only for the name, off `ITEM_FIELD_RANDOM_PROPERTIES_ID`. The
//! localized suffix is columns 7..14, read at `0x1c + 4*locale`.
//!
//! 12340 keeps ID/internal-name/enchant columns in the same positions, but its name block
//! has sixteen strings (7..22) and the mask is 23: AzerothCore DBCStructure.h
//! `ItemRandomPropertiesEntry` / DBCfmt.h `ItemRandomPropertiesfmt`. This is still the
//! positive-property table; negative IDs belong to the separate `ItemRandomSuffix` table.
//! The Wrath catalog retains that table too: sixteen names at 1..16, mask/internal name
//! at 17/18, five enchant IDs at 19..23 and allocation percentages at 24..28.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{field_count, loc_str, loc_str_index, parse, u32_at};

const ITEM_RANDOM_PROPERTIES: &str = "DBFilesClient\\ItemRandomProperties.dbc";
const ITEM_RANDOM_SUFFIX: &str = "DBFilesClient\\ItemRandomSuffix.dbc";

/// The enchant slots a row grants, the item's enchant slots 2..6 (0 permanent, 1 temporary).
pub const RANDOM_PROPERTY_SLOTS: usize = 5;

/// The item-enchant slot the first random-property enchant lands in: `0x52b7e0`-`0x52b7fb`
/// copies the row's five dwords to the tooltip session's `+0x3d8..+0x3e8`, slots 2..6.
pub const RANDOM_PROPERTY_FIRST_SLOT: u8 = 2;

/// One `ItemRandomProperties` row: the suffix the name takes, and the enchants the roll grants.
#[derive(Clone, Debug, Default)]
pub struct RandomProperty {
    /// The suffix in the locale's column (`"of the Bear"`), never empty: a suffix-less row is the formatter's
    /// plain-name exit and is not loaded.
    pub suffix: String,
    /// The five `SpellItemEnchantment` ids for enchant slots 2..6, `0` where the row grants none.
    pub enchants: [u32; RANDOM_PROPERTY_SLOTS],
}

/// `ItemRandomProperties.dbc` keyed by id, for the name suffix and the tooltip's enchants.
pub struct RandomPropertyCatalog {
    rows: HashMap<u32, RandomProperty>,
    suffix_rows: HashMap<u32, RandomSuffix>,
}

struct RandomSuffix {
    name: Option<String>,
    enchants: [u32; RANDOM_PROPERTY_SLOTS],
    allocation: [u32; RANDOM_PROPERTY_SLOTS],
}

impl RandomPropertyCatalog {
    /// The selected locale's suffix, keeping positive properties and negative suffix IDs
    /// in their separate tables. An absent/empty localized name remains unavailable.
    pub fn suffix_name(&self, id: i32) -> Option<&str> {
        if id < 0 {
            self.suffix_rows.get(&id.unsigned_abs())?.name.as_deref()
        } else {
            self.get(id).map(|row| row.suffix.as_str())
        }
    }

    /// Negative-suffix enchant IDs and their seed-scaled amounts. AzerothCore's
    /// PlayerStorage.cpp applies AllocationPct * GetItemSuffixFactor() / 10000 using u32.
    /// These are catalog facts, not a claim that the tooltip has rendered the amounts.
    pub fn scaled_suffix(&self, id: i32, factor: u32) -> Option<[(u32, u32); RANDOM_PROPERTY_SLOTS]> {
        if id >= 0 {
            return None;
        }
        let row = self.suffix_rows.get(&id.unsigned_abs())?;
        Some(std::array::from_fn(|i| {
            (row.enchants[i], row.allocation[i].wrapping_mul(factor) / 10000)
        }))
    }

    /// The row for a random-property id, taken signed because `0x5d8b00` gates it so: `0`,
    /// negative and unknown ids name none.
    pub fn get(&self, id: i32) -> Option<&RandomProperty> {
        (id > 0).then(|| self.rows.get(&(id as u32)))?
    }

    /// Every row as `(id, row)`, for the whole-table push to the engine.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &RandomProperty)> + '_ {
        self.rows.iter().map(|(&id, row)| (id, row))
    }

    /// A catalog from explicit rows, for tests and fixtures.
    pub fn from_rows(rows: HashMap<u32, RandomProperty>) -> Self {
        RandomPropertyCatalog {
            rows,
            suffix_rows: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

pub(crate) fn item_random_properties_schema() -> Schema {
    schema_with_locales(8)
}

fn schema_with_locales(locales: usize) -> Schema {
    let mut s = Schema::new("ItemRandomProperties");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Name", FieldType::String));
    for i in 0..RANDOM_PROPERTY_SLOTS {
        s.add_field(SchemaField::new(
            format!("Enchantment{i}"),
            FieldType::UInt32,
        ));
    }
    for i in 0..locales {
        s.add_field(SchemaField::new(format!("Suffix{i}"), FieldType::String));
    }
    s.add_field(SchemaField::new("SuffixFlags", FieldType::UInt32));
    s
}

fn load_suffix_rows(chain: &mut Chain) -> Result<HashMap<u32, RandomSuffix>> {
    let bytes = chain
        .read_file(ITEM_RANDOM_SUFFIX)
        .with_context(|| format!("reading {ITEM_RANDOM_SUFFIX}"))?;
    let mut schema = Schema::new("ItemRandomSuffix");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    for i in 0..16 {
        schema.add_field(SchemaField::new(format!("Name{i}"), FieldType::String));
    }
    schema.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    schema.add_field(SchemaField::new("InternalName", FieldType::String));
    for group in ["Enchantment", "AllocationPct"] {
        for i in 0..RANDOM_PROPERTY_SLOTS {
            schema.add_field(SchemaField::new(format!("{group}{i}"), FieldType::UInt32));
        }
    }
    let rs = parse(&bytes, schema, "ItemRandomSuffix")?;
    let column = crate::Locale::wrath_dbc_column(chain.locale().wrath_index());
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            RandomSuffix {
                name: column.and_then(|column| loc_str_index(&rs, r, 1, column))
                    .filter(|name| !name.is_empty()),
                enchants: std::array::from_fn(|i| u32_at(r, 19 + i).unwrap_or(0)),
                allocation: std::array::from_fn(|i| u32_at(r, 24 + i).unwrap_or(0)),
            },
        );
    }
    Ok(rows)
}

/// Load `ItemRandomProperties.dbc` off the patch chain, dropping a row with no suffix, the
/// formatter's plain-name exit.
pub fn load_random_property_catalog(chain: &mut Chain) -> Result<RandomPropertyCatalog> {
    let bytes = chain
        .read_file(ITEM_RANDOM_PROPERTIES)
        .with_context(|| format!("reading {ITEM_RANDOM_PROPERTIES}"))?;
    let wrath = field_count(&bytes) == Some(24);
    let schema = if wrath {
        schema_with_locales(16)
    } else {
        item_random_properties_schema()
    };
    let rs = parse(&bytes, schema, "ItemRandomProperties")?;
    let locale = chain.locale();
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        // The suffix block at field 7, `0x1c + 4·locale`, the string the name formatter reads.
        let suffix = if wrath {
            crate::Locale::wrath_dbc_column(locale.wrath_index())
                .and_then(|column| loc_str_index(&rs, r, 7, column))
                .filter(|name| !name.is_empty())
        } else {
            loc_str(&rs, r, 7, locale)
        };
        let Some(suffix) = suffix else {
            continue;
        };
        let mut enchants = [0u32; RANDOM_PROPERTY_SLOTS];
        for (slot, e) in enchants.iter_mut().enumerate() {
            *e = u32_at(r, 2 + slot).unwrap_or(0);
        }
        rows.insert(id, RandomProperty { suffix, enchants });
    }
    let suffix_rows = if wrath {
        load_suffix_rows(chain)?
    } else {
        HashMap::new()
    };
    Ok(RandomPropertyCatalog { rows, suffix_rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_loads_with_its_suffixes_and_enchants() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_random_property_catalog(&mut chain).expect("load");
        assert_eq!(
            cat.len(),
            2012,
            "every shipped row carries a locale-0 suffix"
        );
        let row = cat.get(5).expect("row 5");
        assert_eq!(row.suffix, "of Intellect");
        assert_eq!(row.enchants, [79, 0, 0, 0, 0]);
        // The formatter's plain-name exit: 0, negative, and an id past the table name no row.
        for id in [0, -1, 999_999] {
            assert!(cat.get(id).is_none(), "{id}");
        }
    }
}
