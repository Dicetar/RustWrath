//! `Weather.dbc`: a weather state's ambience loop and effect type. 3.3.5's `SMSG_WEATHER`
//! carries a row id from this table, not a sound id: the client's handler reads
//! `u32 weatherId, f32 intensity, u8 abrupt` and then `m_effectType`, `m_transitionSkyBox` and
//! `m_ambienceID` from the row (`SMSG_WEATHER` in `CGGameUI::MessageHandler`).
//!
//! Row layout (`WeatherRec`, 8 columns): `ID`, `AmbienceID`, `EffectType`, `TransitionSkyBox`,
//! `EffectColor[3]`, `EffectTexture`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const WEATHER: &str = "DBFilesClient\\Weather.dbc";

/// One `Weather.dbc` row.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherEntry {
    pub id: u32,
    /// The `SoundEntries` loop kit the weather ambience plays (`m_ambienceID`), 0 for none.
    pub ambience: u32,
    /// The precipitation kind the renderer selects (`m_effectType`): 1 rain, 2 snow, 3 sandstorm.
    pub effect_type: u32,
    /// Seconds the sky takes to reach this state on a smooth change (`m_transitionSkyBox`).
    pub transition_sky: f32,
    /// The effect tint (`m_effectColor`), unread by the client's weather renderer here.
    pub color: [f32; 3],
    /// The particle texture name (`m_effectTexture`).
    pub texture: String,
}

/// The weather states by id.
pub struct WeatherCatalog {
    rows: HashMap<u32, WeatherEntry>,
}

impl WeatherCatalog {
    /// A catalog of `(id, ambience, effect_type)` rows for tests and tools that run without an
    /// install; the transition, tint and texture fields stay empty.
    pub fn from_rows(rows: impl IntoIterator<Item = (u32, u32, u32)>) -> Self {
        let rows = rows
            .into_iter()
            .map(|(id, ambience, effect_type)| {
                (
                    id,
                    WeatherEntry {
                        id,
                        ambience,
                        effect_type,
                        transition_sky: 0.0,
                        color: [0.0; 3],
                        texture: String::new(),
                    },
                )
            })
            .collect();
        Self { rows }
    }

    pub fn get(&self, id: u32) -> Option<&WeatherEntry> {
        self.rows.get(&id)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("Weather");
    for name in ["ID", "AmbienceID", "EffectType"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s.add_field(SchemaField::new("TransitionSkyBox", FieldType::Float32));
    s.add_field(SchemaField::new_array("EffectColor", FieldType::Float32, 3));
    s.add_field(SchemaField::new("EffectTexture", FieldType::String));
    s
}

/// Read `Weather.dbc` off the patch chain.
pub fn load_weather_catalog(chain: &mut Chain) -> Result<WeatherCatalog> {
    let bytes = chain
        .read_file(WEATHER)
        .with_context(|| format!("reading {WEATHER}"))?;
    catalog_from(&bytes)
}

fn catalog_from(bytes: &[u8]) -> Result<WeatherCatalog> {
    let rs = parse(bytes, schema(), "Weather")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            WeatherEntry {
                id,
                ambience: u32_at(r, 1).unwrap_or(0),
                effect_type: u32_at(r, 2).unwrap_or(0),
                transition_sky: f32_at(r, 3).unwrap_or(0.0),
                color: [
                    f32_at(r, 4).unwrap_or(0.0),
                    f32_at(r, 5).unwrap_or(0.0),
                    f32_at(r, 6).unwrap_or(0.0),
                ],
                texture: str_at(&rs, r, 7).unwrap_or_default(),
            },
        );
    }
    Ok(WeatherCatalog { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic 8-column table: ids 1 and 2, the second naming no ambience.
    #[test]
    fn rows_resolve_ambience_effect_type_and_transition() {
        let rows: &[[u32; 8]] = &[
            [1, 8533, 1, 6.0f32.to_bits(), 0, 0, 0, 0],
            [2, 0, 2, 4.5f32.to_bits(), 0, 0, 0, 0],
        ];
        let mut dbc = Vec::new();
        dbc.extend_from_slice(b"WDBC");
        dbc.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        dbc.extend_from_slice(&8u32.to_le_bytes()); // fields
        dbc.extend_from_slice(&32u32.to_le_bytes()); // record size
        dbc.extend_from_slice(&1u32.to_le_bytes()); // string block
        for row in rows {
            for v in row {
                dbc.extend_from_slice(&v.to_le_bytes());
            }
        }
        dbc.push(0); // the string block, an empty texture name at offset 0

        let cat = catalog_from(&dbc).unwrap();
        assert_eq!(cat.len(), 2);
        let rain = cat.get(1).expect("rain row");
        assert_eq!(rain.ambience, 8533);
        assert_eq!(rain.effect_type, 1);
        assert!((rain.transition_sky - 6.0).abs() < 1e-6);
        let snow = cat.get(2).expect("snow row");
        assert_eq!(snow.ambience, 0, "a state with no ambience loop");
        assert_eq!(snow.effect_type, 2);
        assert_eq!(cat.get(9), None);
    }
}
