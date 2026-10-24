//! The pure wire→state reducer: `SMSG_UPDATE_OBJECT` (0x0A9) and its compressed form
//! (0x1F6, `UpdateData` compression) into GUID-keyed typed state. Same ordered inputs, same
//! state — no clocks, no I/O, no scheduling dependence; replay (H2) reduces through this
//! exact code. GUID identity is the map key and survives despawn/recreate as a generation
//! count, never an entity-id swap.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};

use crate::opcode;
use crate::updates::{movement_block, values_block, Reader, MovementSnapshot};

/// What one packet did to the world state, in arrival order.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeEvent {
    Created { guid: u64 },
    Updated { guid: u64 },
    Moved { guid: u64 },
    Removed { guid: u64 },
}

/// One tracked object: raw u32 update fields plus the last movement snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectEntry {
    pub guid: u64,
    pub type_id: u8,
    pub fields: Vec<u32>,
    pub movement: Option<MovementSnapshot>,
    /// Increments when this GUID is destroyed and created again — identity check for the
    /// despawn/recreate dance visibility does.
    pub generation: u32,
}

impl ObjectEntry {
    /// A u32 field by index, if the mask ever delivered it.
    pub fn field(&self, index: u16) -> Option<u32> {
        self.fields.get(index as usize).copied()
    }
    /// Two adjacent fields as the 64-bit GUID the object writes for itself
    /// (`fields[OBJECT_FIELD_GUID] | fields[+1] << 32`).
    pub fn self_guid(&self, index: u16) -> Option<u64> {
        Some(self.field(index)? as u64 | (self.field(index + 1)? as u64) << 32)
    }
}

/// The replicated world: GUID-keyed, iteration-ordered for deterministic snapshots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldState {
    objects: BTreeMap<u64, ObjectEntry>,
    /// Highest generation reached per GUID that is not currently present.
    tombstones: BTreeMap<u64, u32>,
}

impl WorldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, guid: u64) -> Option<&ObjectEntry> {
        self.objects.get(&guid)
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u64, &ObjectEntry)> {
        self.objects.iter()
    }

    /// Reduce one world packet into the state, returning what changed.
    pub fn reduce(&mut self, opcode: u16, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        match opcode {
            opcode::SMSG_UPDATE_OBJECT => self.reduce_update_object(payload),
            opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
                // `WorldSocket.cpp:97-119`: u32 uncompressed size, then zlib of the plain
                // update-object payload — no inner opcode, no re-dispatch.
                let mut r = Reader::new(payload);
                let size = r.u32().context("compressed size")? as usize;
                let raw = flate2::read::ZlibDecoder::new(&payload[r.at..])
                    .bytes_read()
                    .context("inflating compressed update")?;
                if raw.len() != size {
                    bail!("compressed update size mismatch: header {size}, inflated {}", raw.len());
                }
                self.reduce_update_object(&raw)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn reduce_update_object(&mut self, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        let mut r = Reader::new(payload);
        let mut events = Vec::new();
        let blocks = r.u32().context("block count")?;
        for _ in 0..blocks {
            let block_type = r.u8().context("block type")?;
            match block_type {
                0 => {
                    // VALUES: packed guid, values.
                    let guid = r.packed_guid()?;
                    self.apply_values(guid, &mut r)?;
                    events.push(ChangeEvent::Updated { guid });
                }
                1 => {
                    // MOVEMENT: packed guid, one movement block.
                    let guid = r.packed_guid()?;
                    let snapshot = movement_block(&mut r)?;
                    self.entry(guid, 0).movement = Some(snapshot);
                    events.push(ChangeEvent::Moved { guid });
                }
                2 | 3 => {
                    // CREATE_OBJECT(2) / CREATE_OBJECT2(3): packed guid, type id, movement,
                    // values.
                    let guid = r.packed_guid()?;
                    let type_id = r.u8().context("object type id")?;
                    let snapshot = movement_block(&mut r)?;
                    let generation = if self.objects.contains_key(&guid) {
                        self.objects[&guid].generation
                    } else {
                        self.tombstones.get(&guid).copied().unwrap_or(0)
                    };
                    let entry = self.entry(guid, type_id);
                    entry.generation = generation;
                    entry.movement = Some(snapshot);
                    self.apply_values(guid, &mut r)?;
                    events.push(ChangeEvent::Created { guid });
                }
                4 => {
                    // OUT_OF_RANGE_OBJECTS: u32 count, packed guids (`UpdateData.cpp:58-66`).
                    let count = r.u32().context("out-of-range count")?;
                    for _ in 0..count {
                        let guid = r.packed_guid()?;
                        self.remove(guid, &mut events);
                    }
                }
                other => bail!("unknown update block type {other}"),
            }
        }
        if !r.done() {
            bail!("{} trailing bytes after the last update block", r.p.len() - r.at);
        }
        Ok(events)
    }

    fn entry(&mut self, guid: u64, type_id: u8) -> &mut ObjectEntry {
        self.objects.entry(guid).or_insert_with(|| ObjectEntry {
            guid,
            type_id,
            fields: Vec::new(),
            movement: None,
            generation: 0,
        })
    }

    fn apply_values(&mut self, guid: u64, r: &mut Reader) -> Result<()> {
        let updates = values_block(r)?;
        let entry = self.entry(guid, 0);
        for (index, value) in updates {
            let index = index as usize;
            if entry.fields.len() <= index {
                entry.fields.resize(index + 1, 0);
            }
            entry.fields[index] = value;
        }
        Ok(())
    }

    fn remove(&mut self, guid: u64, events: &mut Vec<ChangeEvent>) {
        if let Some(entry) = self.objects.remove(&guid) {
            // The GUID stays meaningful across a recreate: remember how many lives it has had.
            self.tombstones.insert(guid, entry.generation + 1);
            events.push(ChangeEvent::Removed { guid });
        }
    }

    /// GUID generations seen so far (despawn/recreate identity).
    pub fn generation_of(&self, guid: u64) -> u32 {
        self.objects
            .get(&guid)
            .map(|e| e.generation)
            .or_else(|| self.tombstones.get(&guid).copied())
            .unwrap_or(0)
    }
}

trait ReadBytes: Sized {
    fn bytes_read(self) -> Result<Vec<u8>>;
}

impl<R: std::io::Read> ReadBytes for R {
    fn bytes_read(mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut self, &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields;

    /// Test-side encoder mirroring the server's writers — an independent construction, so a
    /// shared parse/encode defect cannot pass silently (the F12 rule).
    fn pack_guid(guid: u64) -> Vec<u8> {
        let mut mask = 0u8;
        let mut bytes = Vec::new();
        for i in 0..8 {
            let b = (guid >> (i * 8)) as u8;
            if b != 0 {
                mask |= 1 << i;
                bytes.push(b);
            }
        }
        let mut out = vec![mask];
        out.extend_from_slice(&bytes);
        out
    }

    fn u16b(v: u16) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }
    fn u32b(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    fn values_wire(values: &[(u16, u32)]) -> Vec<u8> {
        let words = values.iter().map(|(i, _)| *i as usize / 32 + 1).max().unwrap_or(1);
        let mut mask = vec![0u32; words];
        for (i, _) in values {
            mask[*i as usize / 32] |= 1u32 << (*i % 32);
        }
        let mut p = vec![words as u8];
        for w in &mask {
            p.extend(u32b(*w));
        }
        for (_, v) in values {
            p.extend(u32b(*v));
        }
        p
    }

    fn stationary_create(guid: u64, type_id: u8, x: f32, values: &[(u16, u32)]) -> Vec<u8> {
        let mut p = u32b(1); // one block
        p.push(3); // CREATE_OBJECT2
        p.extend(pack_guid(guid));
        p.push(type_id);
        p.extend(u16b(crate::updates::update_flags::STATIONARY_POSITION));
        p.extend(x.to_le_bytes());
        p.extend(2.0f32.to_le_bytes());
        p.extend(3.0f32.to_le_bytes());
        p.extend(0.5f32.to_le_bytes());
        p.extend(values_wire(values));
        p
    }

    fn values_update(guid: u64, values: &[(u16, u32)]) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(0); // VALUES
        p.extend(pack_guid(guid));
        p.extend(values_wire(values));
        p
    }

    fn out_of_range(guids: &[u64]) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(4);
        p.extend(u32b(guids.len() as u32));
        for g in guids {
            p.extend(pack_guid(*g));
        }
        p
    }

    #[test]
    fn create_carries_values_and_movement() {
        let guid = 0x0000_0001_0000_0042u64;
        let payload = stationary_create(
            guid,
            crate::updates::type_id::PLAYER,
            -8913.0,
            &[
                (fields::OBJECT_FIELD_GUID, 0x42),
                (fields::OBJECT_FIELD_GUID + 1, 1),
                (fields::UNIT_FIELD_HEALTH, 58),
                (fields::UNIT_FIELD_LEVEL, 1),
            ],
        );
        let mut state = WorldState::new();
        let events = state.reduce(opcode::SMSG_UPDATE_OBJECT, &payload).unwrap();
        assert_eq!(events, vec![ChangeEvent::Created { guid }]);
        let entry = state.get(guid).unwrap();
        assert_eq!(entry.type_id, crate::updates::type_id::PLAYER);
        assert_eq!(entry.field(fields::UNIT_FIELD_HEALTH), Some(58));
        assert_eq!(entry.self_guid(fields::OBJECT_FIELD_GUID), Some(guid));
        let snap = entry.movement.as_ref().unwrap();
        assert_eq!(snap.x, -8913.0);
        assert_eq!(snap.o, 0.5);
    }

    #[test]
    fn values_update_changes_only_masked_fields() {
        let guid = 7;
        let mut state = WorldState::new();
        state
            .reduce(opcode::SMSG_UPDATE_OBJECT, &stationary_create(guid, 3, 1.0, &[(fields::UNIT_FIELD_HEALTH, 100)]))
            .unwrap();
        state
            .reduce(opcode::SMSG_UPDATE_OBJECT, &values_update(guid, &[(fields::UNIT_FIELD_HEALTH, 42)]))
            .unwrap();
        assert_eq!(state.get(guid).unwrap().field(fields::UNIT_FIELD_HEALTH), Some(42));
    }

    #[test]
    fn guid_survives_destroy_and_recreate_with_a_generation() {
        let guid = 0xAA;
        let mut state = WorldState::new();
        state.reduce(opcode::SMSG_UPDATE_OBJECT, &stationary_create(guid, 3, 1.0, &[])).unwrap();
        assert_eq!(state.generation_of(guid), 0);
        let events = state.reduce(opcode::SMSG_UPDATE_OBJECT, &out_of_range(&[guid])).unwrap();
        assert_eq!(events, vec![ChangeEvent::Removed { guid }]);
        assert!(state.get(guid).is_none());
        assert_eq!(state.generation_of(guid), 1);
        state.reduce(opcode::SMSG_UPDATE_OBJECT, &stationary_create(guid, 3, 2.0, &[])).unwrap();
        let entry = state.get(guid).unwrap();
        assert_eq!(entry.generation, 1, "recreate inherits the tombstone generation");
        assert_eq!(entry.movement.as_ref().unwrap().x, 2.0);
    }

    #[test]
    fn reduction_is_deterministic_for_the_same_ordered_inputs() {
        let guid = 0x1234;
        let inputs = vec![
            stationary_create(guid, 3, 5.0, &[(fields::UNIT_FIELD_HEALTH, 90)]),
            values_update(guid, &[(fields::UNIT_FIELD_HEALTH, 75)]),
            out_of_range(&[guid]),
            stationary_create(guid, 3, 6.0, &[(fields::UNIT_FIELD_HEALTH, 80)]),
        ];
        let mut runs = Vec::new();
        for _ in 0..3 {
            let mut state = WorldState::new();
            let mut events = Vec::new();
            for payload in &inputs {
                events.extend(state.reduce(opcode::SMSG_UPDATE_OBJECT, payload).unwrap());
            }
            runs.push((state, events));
        }
        assert!(runs[0] == runs[1] && runs[1] == runs[2]);
    }

    #[test]
    fn compressed_update_inflates_and_reduces() {
        let guid = 9;
        let payload = stationary_create(guid, 3, 1.0, &[(fields::UNIT_FIELD_HEALTH, 33)]);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut z, &payload).unwrap();
        let compressed = z.finish().unwrap();
        let mut packet = (payload.len() as u32).to_le_bytes().to_vec();
        packet.extend_from_slice(&compressed);
        let mut state = WorldState::new();
        state.reduce(opcode::SMSG_COMPRESSED_UPDATE_OBJECT, &packet).unwrap();
        assert_eq!(state.get(guid).unwrap().field(fields::UNIT_FIELD_HEALTH), Some(33));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut payload = stationary_create(3, 3, 1.0, &[]);
        payload.push(0xEE);
        let mut state = WorldState::new();
        assert!(state.reduce(opcode::SMSG_UPDATE_OBJECT, &payload).is_err());
    }
}
