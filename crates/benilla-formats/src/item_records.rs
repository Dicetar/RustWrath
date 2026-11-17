//! `Item.dbc`: the client-side half of an item, its class, subclass, material, display, inventory
//! type and sheath. 12340 resolves a creature's `UNIT_VIRTUAL_ITEM_SLOT_ID` words through this table
//! on create and on every change (`CGUnit_C::OnVirtualItemChanged`), so a server that sends only
//! item ids still arms its creatures. A missing row reads as all zeros there, which is an empty hand.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

/// One `Item.dbc` row (3.3.5 layout, eight int columns).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemRecord {
    pub class: u32,
    pub subclass: u32,
    pub sound_override_subclass: i32,
    pub material: u32,
    pub display: u32,
    pub inventory_type: u32,
    pub sheath: u32,
}

/// `Item.dbc` keyed by item id.
#[derive(Default)]
pub struct ItemRecordCatalog {
    rows: HashMap<u32, ItemRecord>,
}

impl ItemRecordCatalog {
    pub fn get(&self, id: u32) -> Option<ItemRecord> {
        self.rows.get(&id).copied()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("Item");
    for name in [
        "ID",
        "ClassID",
        "SubclassID",
        "SoundOverrideSubclassID",
        "Material",
        "DisplayInfoID",
        "InventoryType",
        "SheatheType",
    ] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s
}

/// Read `Item.dbc` off the patch chain.
pub fn load_item_record_catalog(chain: &mut Chain) -> Result<ItemRecordCatalog> {
    let bytes = chain
        .read_file("DBFilesClient\\Item.dbc")
        .context("reading Item.dbc")?;
    let rs = parse(&bytes, schema(), "Item")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let at = |i| u32_at(r, i).unwrap_or(0);
        rows.insert(
            id,
            ItemRecord {
                class: at(1),
                subclass: at(2),
                sound_override_subclass: at(3) as i32,
                material: at(4),
                display: at(5),
                inventory_type: at(6),
                sheath: at(7),
            },
        );
    }
    Ok(ItemRecordCatalog { rows })
}
