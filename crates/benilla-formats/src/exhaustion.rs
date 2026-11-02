//! `Exhaustion.dbc`: the rest states. `GetRestState` (`0x48d350`) indexes it by the
//! `PLAYER_BYTES_2` rest-state byte (`[0xc0dd78]`) and returns the id, the name from this file's
//! string block (not GlobalStrings) and the factor. `GetXPExhaustion` (`0x48d3f0`) scales the
//! rested pool by row 1's factor, 2.0 in the data. Rows 3-5 are beta tiers never sent.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, field_count, loc_str, parse, u32_at};
use crate::Chain;

const EXHAUSTION: &str = "DBFilesClient\\Exhaustion.dbc";

/// One rest state, as `GetRestState` returns it.
pub struct ExhaustionRow {
    /// Also the wire's rest-state byte, which the client indexes by.
    pub id: u32,
    /// The state name, from the locale's column.
    pub name: String,
    /// The XP multiplier.
    pub factor: f32,
}

fn exhaustion_schema(fields: Option<u32>) -> Schema {
    // 1.12 (5875) has 15 fields (8 locale columns); 3.3.5a (12340 `ExhaustionRec::Read`)
    // has 23 fields (16 locale columns).
    let locales = if fields == Some(23) { 16 } else { 8 };
    let mut s = Schema::new("Exhaustion");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Xp", FieldType::UInt32));
    s.add_field(SchemaField::new("Factor", FieldType::Float32));
    s.add_field(SchemaField::new("OutdoorHours", FieldType::Float32));
    s.add_field(SchemaField::new("InnHours", FieldType::Float32));
    for i in 0..locales {
        s.add_field(SchemaField::new(format!("Name{i}"), FieldType::String));
    }
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s.add_field(SchemaField::new("Threshold", FieldType::UInt32));
    s
}

fn parse_exhaustion_bytes(bytes: &[u8], locale: usize) -> Result<Vec<ExhaustionRow>> {
    let rs = parse(bytes, exhaustion_schema(field_count(bytes)), "Exhaustion")?;
    let mut rows = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        let (Some(id), Some(factor)) = (u32_at(r, 0), f32_at(r, 2)) else {
            continue;
        };
        let name = loc_str(&rs, r, 5, locale).unwrap_or_default();
        rows.push(ExhaustionRow { id, name, factor });
    }
    Ok(rows)
}

/// Load Exhaustion.dbc from the patch chain.
pub fn load_exhaustion(chain: &mut Chain) -> Result<Vec<ExhaustionRow>> {
    let bytes = chain
        .read_file(EXHAUSTION)
        .with_context(|| format!("reading {EXHAUSTION}"))?;
    parse_exhaustion_bytes(&bytes, chain.locale())
}

#[cfg(test)]
mod tests {
    use super::{load_exhaustion, parse_exhaustion_bytes};

    #[test]
    fn the_shipped_rest_states_carry_the_rested_double() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let rows = load_exhaustion(&mut chain).expect("Exhaustion.dbc");
        let by_id: std::collections::HashMap<u32, (&str, f32)> = rows
            .iter()
            .map(|r| (r.id, (r.name.as_str(), r.factor)))
            .collect();
        assert_eq!(by_id[&1], ("Rested", 2.0), "the ×2 is this row's data");
        assert_eq!(by_id[&2], ("Normal", 1.0));
        assert_eq!(by_id[&3], ("XXXTired", 1.0), "beta tier, never sent");
        assert_eq!(by_id[&4], ("XXXTired", 0.5));
        assert_eq!(by_id[&5], ("XXXExhausted", 0.25));
        assert_eq!(rows.len(), 5, "the whole shipped table");
    }

    #[test]
    fn wrath_23_field_exhaustion_schema_parses_rested_row() {
        let strings = b"\0Rested\0";
        let mut dbc = Vec::new();
        dbc.extend_from_slice(b"WDBC");
        dbc.extend_from_slice(&1u32.to_le_bytes()); // 1 record
        dbc.extend_from_slice(&23u32.to_le_bytes()); // 23 fields (12340)
        dbc.extend_from_slice(&(23u32 * 4).to_le_bytes()); // 92-byte record
        dbc.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        let mut rec = [0u32; 23];
        rec[0] = 1; // ID
        rec[1] = 10000; // Xp
        rec[2] = 2.0f32.to_bits(); // Factor
        rec[5] = 1; // Name0 offset
        for field in rec {
            dbc.extend_from_slice(&field.to_le_bytes());
        }
        dbc.extend_from_slice(strings);

        let rows = parse_exhaustion_bytes(&dbc, 0).expect("23-field Exhaustion parses");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 1);
        assert_eq!(rows[0].name, "Rested");
        assert_eq!(rows[0].factor, 2.0);
    }
}
