//! Possession limits from the 12340 ItemLimitCategory.dbc table.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

pub fn load_item_limit_quantities(chain: &mut Chain) -> Result<HashMap<u32, u32>> {
    let bytes = chain
        .read_file("DBFilesClient\\ItemLimitCategory.dbc")
        .context("reading ItemLimitCategory.dbc")?;
    let mut schema = Schema::new("ItemLimitCategory");
    for column in 0..20 {
        schema.add_field(SchemaField::new(
            format!("Column{column}"),
            FieldType::UInt32,
        ));
    }
    let rows = parse(&bytes, schema, "ItemLimitCategory.dbc")?;
    // ID, 16 locale columns, locale flags, quantity, limit flags.
    Ok(rows
        .records()
        .iter()
        .filter_map(|row| Some((u32_at(row, 0)?, u32_at(row, 18)?)))
        .collect())
}
