//! The 12340 vehicle kit's seat indices and the passenger seat's capability flags.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

pub struct VehicleSeats {
    kits: HashMap<u32, [u32; 8]>,
    flags: HashMap<u32, u32>,
}

impl VehicleSeats {
    pub fn load(chain: &mut Chain) -> Result<Self> {
        let read = |chain: &mut Chain, name: &str, columns| {
            let bytes = chain
                .read_file(&format!("DBFilesClient\\{name}.dbc"))
                .with_context(|| format!("reading {name}.dbc"))?;
            let mut schema = Schema::new(name);
            for column in 0..columns {
                schema.add_field(SchemaField::new(
                    format!("Column{column}"),
                    FieldType::UInt32,
                ));
            }
            parse(&bytes, schema, name)
        };
        let kits = read(chain, "Vehicle", 40)?
            .records()
            .iter()
            .filter_map(|row| {
                let kit = u32_at(row, 0)?;
                // VehicleRec::m_seatID begins at 0x18.
                Some((
                    kit,
                    std::array::from_fn(|i| u32_at(row, 6 + i).unwrap_or(0)),
                ))
            })
            .collect();
        let flags = read(chain, "VehicleSeat", 58)?
            .records()
            .iter()
            .filter_map(|row| Some((u32_at(row, 0)?, u32_at(row, 1)?)))
            .collect();
        Ok(Self { kits, flags })
    }

    pub fn flags(&self, vehicle_id: u32, seat_index: u8) -> Option<u32> {
        let id = *self.kits.get(&vehicle_id)?.get(usize::from(seat_index))?;
        if id == 0 {
            return None;
        }
        self.flags.get(&id).copied()
    }
}
