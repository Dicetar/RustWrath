//! `TaxiNodes.dbc`, every flight-master node: its map and position, name, and the taxi mount each
//! team rides from it. The layout is vmangos's `TaxiNodesEntry` (`DBCStructure.h:789-799`): `ID`,
//! `MapID`, `X`, `Y`, `Z`, the `Name` loc-string (5-13), then `MountCreatureID[2]`, Horde first.
//! The mounts are `creature_template` entries, not display ids, resolved like any NPC's.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, field_count, loc_str, parse, u32_at};
use crate::Chain;

const TAXI_NODES: &str = "DBFilesClient\\TaxiNodes.dbc";

/// One `TaxiNodes.dbc` row, a flight-master node.
#[derive(Clone, Debug)]
pub struct TaxiNode {
    /// The id `SMSG_SHOWTAXINODES`'s known-mask and `CMSG_ACTIVATETAXI`'s nodes name.
    pub id: u32,
    /// The map this node's `pos` is expressed in (`Map.dbc` id).
    pub map_id: u32,
    /// World position `(x, y, z)` on `map_id`.
    pub pos: [f32; 3],
    /// The name in the locale's column, the taxi map's node label.
    pub name: String,
    /// The `creature_template` entry of a Horde rider's taxi mount here, `0` for no Horde service.
    pub mount_horde: u32,
    /// The `creature_template` entry of an Alliance rider's taxi mount here, `0` for none.
    pub mount_alliance: u32,
}

/// `TaxiNodes.dbc` rows keyed by `ID`.
pub struct TaxiNodes {
    rows: HashMap<u32, TaxiNode>,
}

impl TaxiNodes {
    /// A catalog of these rows, keyed by id, a later row replacing an earlier one.
    pub fn from_rows(rows: impl IntoIterator<Item = TaxiNode>) -> Self {
        Self {
            rows: rows.into_iter().map(|n| (n.id, n)).collect(),
        }
    }

    pub fn get(&self, id: u32) -> Option<&TaxiNode> {
        self.rows.get(&id)
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &TaxiNode> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn schema(fields: Option<u32>) -> Schema {
    // 1.12 (5875) has 16 fields (8 locale columns); 3.3.5a (12340 `TaxiNodesRec::Read`
    // `0x008B9E40`) has 24 fields (16 locale columns).
    let other_locales = if fields == Some(24) { 15 } else { 7 };
    let mut s = Schema::new("TaxiNodes");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("MapID", FieldType::UInt32));
    for name in ["X", "Y", "Z"] {
        s.add_field(SchemaField::new(name, FieldType::Float32));
    }
    s.add_field(SchemaField::new("Name", FieldType::String)); // locale 0
    s.add_field(SchemaField::new_array(
        "NameOtherLocales",
        FieldType::String,
        other_locales,
    ));
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s.add_field(SchemaField::new("MountIdHorde", FieldType::UInt32));
    s.add_field(SchemaField::new("MountIdAlliance", FieldType::UInt32));
    s
}

fn parse_taxi_nodes_bytes(bytes: &[u8], locale: usize) -> Result<TaxiNodes> {
    let fields = field_count(bytes);
    let (horde_col, alliance_col) = if fields == Some(24) { (22, 23) } else { (14, 15) };
    let rs = parse(bytes, schema(fields), "TaxiNodes")?;
    let rows = rs.records().iter().filter_map(|r| {
        let id = u32_at(r, 0)?;
        let (x, y, z) = (f32_at(r, 2)?, f32_at(r, 3)?, f32_at(r, 4)?);
        Some(TaxiNode {
            id,
            map_id: u32_at(r, 1).unwrap_or(0),
            pos: [x, y, z],
            name: loc_str(&rs, r, 5, locale).unwrap_or_default(),
            mount_horde: u32_at(r, horde_col).unwrap_or(0),
            mount_alliance: u32_at(r, alliance_col).unwrap_or(0),
        })
    });
    Ok(TaxiNodes::from_rows(rows))
}

/// Read `TaxiNodes.dbc` off the patch chain into a [`TaxiNodes`] catalog.
pub fn load_taxi_nodes(chain: &mut Chain) -> Result<TaxiNodes> {
    let bytes = chain
        .read_file(TAXI_NODES)
        .context("reading TaxiNodes.dbc")?;
    parse_taxi_nodes_bytes(&bytes, chain.locale())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 5875 layout on Stormwind's node, which serves the Alliance only.
    #[test]
    fn real_taxi_nodes_layout_sanity() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_taxi_nodes(&mut chain).expect("load TaxiNodes");
        assert!(cat.len() >= 85, "at least 85 taxi nodes");

        let stormwind = cat.get(2).expect("node 2 exists");
        assert_eq!(stormwind.name, "Stormwind, Elwynn");
        assert_eq!(stormwind.map_id, 0, "Eastern Kingdoms");
        assert_eq!(stormwind.mount_horde, 0, "no Horde service from Stormwind");
        assert_eq!(
            stormwind.mount_alliance, 541,
            "Stormwind's Alliance gryphon mount template"
        );

        // The other end of the hop the `TaxiPath` test pins.
        let sentinel_hill = cat.get(4).expect("node 4 exists");
        assert_eq!(sentinel_hill.name, "Sentinel Hill, Westfall");
    }

    #[test]
    fn wrath_24_field_taxi_nodes_schema_parses_mounts_after_16_locales() {
        let strings = b"\0Stormwind, Elwynn\0";
        let mut dbc = Vec::new();
        dbc.extend_from_slice(b"WDBC");
        dbc.extend_from_slice(&1u32.to_le_bytes()); // 1 record
        dbc.extend_from_slice(&24u32.to_le_bytes()); // 24 fields (12340)
        dbc.extend_from_slice(&(24u32 * 4).to_le_bytes()); // 96-byte record
        dbc.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        let mut rec = [0u32; 24];
        rec[0] = 2; // ID
        rec[1] = 0; // MapID
        rec[2] = (-8842.0f32).to_bits();
        rec[3] = 488.5f32.to_bits();
        rec[4] = 109.6f32.to_bits();
        rec[5] = 1; // Name enUS offset
        rec[22] = 0; // MountIdHorde
        rec[23] = 541; // MountIdAlliance
        for field in rec {
            dbc.extend_from_slice(&field.to_le_bytes());
        }
        dbc.extend_from_slice(strings);

        let cat = parse_taxi_nodes_bytes(&dbc, 0).expect("24-field TaxiNodes parses");
        let node = cat.get(2).expect("node 2");
        assert_eq!(node.name, "Stormwind, Elwynn");
        assert_eq!(node.mount_horde, 0);
        assert_eq!(node.mount_alliance, 541);
    }
}
