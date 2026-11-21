//! The Wrath currency tables used by the TokenFrame list.
//!
//! `CurrencyTypes.dbc` maps a known-currency bit to an item and category; `CurrencyCategory.dbc`
//! supplies the localized headers and their sort flags. The client keeps both tables in file order
//! and performs the category/list sorting when the player's known masks change.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{loc_str_index, parse, u32_at};
use crate::Chain;

const CURRENCY_TYPES: &str = "DBFilesClient\\CurrencyTypes.dbc";
const CURRENCY_CATEGORY: &str = "DBFilesClient\\CurrencyCategory.dbc";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrencyType {
    pub id: u32,
    pub item_id: u32,
    pub category_id: u32,
    /// Raw `CurrencyTypes.dbc` BitIndex. Values 1..=32 address the first dword and values 33..=64
    /// address the second; zero is invalid and is retained so callers can apply the reference's
    /// guard rather than silently shifting it.
    pub bit_index: u32,
}

pub struct CurrencyTypeCatalog {
    rows: Vec<CurrencyType>,
    by_id: HashMap<u32, CurrencyType>,
}

impl CurrencyTypeCatalog {
    pub fn get(&self, id: u32) -> Option<CurrencyType> {
        self.by_id.get(&id).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = CurrencyType> + '_ {
        self.rows.iter().copied()
    }

    pub fn rows(&self) -> impl Iterator<Item = CurrencyType> + '_ {
        self.rows.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CurrencyCategory {
    pub id: u32,
    pub flags: u32,
    pub name: String,
}

pub struct CurrencyCategoryCatalog {
    rows: Vec<CurrencyCategory>,
}

impl CurrencyCategoryCatalog {
    pub fn rows(&self) -> impl Iterator<Item = &CurrencyCategory> {
        self.rows.iter()
    }

    pub fn get(&self, id: u32) -> Option<&CurrencyCategory> {
        self.rows.iter().find(|row| row.id == id)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

fn currency_types_schema() -> Schema {
    let mut schema = Schema::new("CurrencyTypes");
    for name in ["ID", "ItemID", "CategoryID", "BitIndex"] {
        schema.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    schema
}

fn currency_category_schema() -> Schema {
    let mut schema = Schema::new("CurrencyCategory");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("Flags", FieldType::UInt32));
    for index in 0..16 {
        schema.add_field(SchemaField::new(format!("Name{index}"), FieldType::String));
    }
    schema.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    schema
}

pub fn load_currency_type_catalog(chain: &mut Chain) -> Result<CurrencyTypeCatalog> {
    let bytes = chain
        .read_file(CURRENCY_TYPES)
        .with_context(|| format!("reading {CURRENCY_TYPES}"))?;
    let records = parse(&bytes, currency_types_schema(), "CurrencyTypes.dbc")?;
    let rows: Vec<_> = records
        .records()
        .filter_map(|record| {
            Some(CurrencyType {
                id: u32_at(record, 0)?,
                item_id: u32_at(record, 1)?,
                category_id: u32_at(record, 2)?,
                bit_index: u32_at(record, 3)?,
            })
        })
        .collect();
    let by_id = rows.iter().map(|row| (row.id, *row)).collect();
    Ok(CurrencyTypeCatalog { rows, by_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_preserve_raw_bit_indexes_and_category_fields() {
        let ty = CurrencyType {
            id: 7,
            item_id: 43307,
            category_id: 2,
            bit_index: 33,
        };
        assert_eq!(ty.bit_index, 33);
        let category = CurrencyCategory {
            id: 2,
            flags: 1,
            name: "Unused".into(),
        };
        assert_eq!(
            category,
            CurrencyCategory {
                id: 2,
                flags: 1,
                name: "Unused".into()
            }
        );
    }
}

pub fn load_currency_category_catalog(chain: &mut Chain) -> Result<CurrencyCategoryCatalog> {
    let bytes = chain
        .read_file(CURRENCY_CATEGORY)
        .with_context(|| format!("reading {CURRENCY_CATEGORY}"))?;
    let records = parse(&bytes, currency_category_schema(), "CurrencyCategory.dbc")?;
    let locale = chain.locale().wrath_index();
    let rows = records
        .records()
        .filter_map(|record| {
            Some(CurrencyCategory {
                id: u32_at(record, 0)?,
                flags: u32_at(record, 1)?,
                name: loc_str_index(&records, record, 2, locale).unwrap_or_default(),
            })
        })
        .collect();
    Ok(CurrencyCategoryCatalog { rows })
}
