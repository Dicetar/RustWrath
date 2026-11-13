//! `gtCombatRatings.dbc`: 3.3.5a combat-rating scalers (`GtCombatRatingsEntry`, 32 rating classes ×
//! 100 levels). `GetCombatRatingBonus(ratingIndex)` divides `PLAYER_FIELD_COMBAT_RATING_1[index-1]`
//! by `scalers[(index - 1) * 100 + (level.clamp(1, 100) - 1)]`.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, field_count, parse};
use crate::Chain;

const GT_COMBAT_RATINGS: &str = "DBFilesClient\\gtCombatRatings.dbc";

fn gt_combat_ratings_schema(fields: Option<usize>) -> Schema {
    let mut s = Schema::new("gtCombatRatings");
    if fields == Some(2) {
        s.add_field(SchemaField::new("ID", FieldType::UInt32));
        s.add_field(SchemaField::new("Data", FieldType::Float32));
    } else {
        s.add_field(SchemaField::new("Data", FieldType::Float32));
    }
    s
}

/// Parse `gtCombatRatings.dbc` bytes into flat `Float32` scalers in row order (`rating_idx * 100 + (level - 1)`).
pub fn parse_gt_combat_ratings_bytes(bytes: &[u8]) -> Result<Vec<f32>> {
    let fields = field_count(bytes);
    let data_col = usize::from(fields == Some(2));
    let rs = parse(bytes, gt_combat_ratings_schema(fields), "gtCombatRatings")?;
    let mut scalers = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(v) = f32_at(r, data_col) else {
            continue;
        };
        scalers.push(v);
    }
    Ok(scalers)
}

/// Load `gtCombatRatings.dbc` from the patch chain.
pub fn load_gt_combat_ratings(chain: &mut Chain) -> Result<Vec<f32>> {
    let bytes = chain
        .read_file(GT_COMBAT_RATINGS)
        .with_context(|| format!("reading {GT_COMBAT_RATINGS}"))?;
    parse_gt_combat_ratings_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::parse_gt_combat_ratings_bytes;

    fn build_dbc(fields: u32, rows: &[f32]) -> Vec<u8> {
        let strings = b"\0";
        let mut dbc = Vec::new();
        dbc.extend_from_slice(b"WDBC");
        dbc.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        dbc.extend_from_slice(&fields.to_le_bytes());
        dbc.extend_from_slice(&(fields * 4).to_le_bytes());
        dbc.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        for (i, &val) in rows.iter().enumerate() {
            if fields == 2 {
                dbc.extend_from_slice(&(i as u32).to_le_bytes());
            }
            dbc.extend_from_slice(&val.to_bits().to_le_bytes());
        }
        dbc.extend_from_slice(strings);
        dbc
    }

    #[test]
    fn parses_one_field_gametable_and_two_field_variant() {
        let rows = [12.5_f32, 32.79_f32, 45.91_f32];
        assert_eq!(
            parse_gt_combat_ratings_bytes(&build_dbc(1, &rows)).unwrap(),
            rows
        );
        assert_eq!(
            parse_gt_combat_ratings_bytes(&build_dbc(2, &rows)).unwrap(),
            rows
        );
    }
}
