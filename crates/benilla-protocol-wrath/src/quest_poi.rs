//! Wrath quest point-of-interest packets. Consumers own the cache and map selection.

use anyhow::{bail, Context, Result};

pub const CMSG_QUEST_POI_QUERY: u32 = 0x1e3;
pub const SMSG_QUEST_POI_QUERY_RESPONSE: u16 = 0x1e4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPoi {
    pub quest_id: u32,
    pub blobs: Vec<QuestPoiBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPoiBlob {
    pub id: i32,
    pub objective_index: i32,
    pub map_id: i32,
    pub world_map_area_id: i32,
    pub floor: i32,
    pub priority: i32,
    pub flags: i32,
    pub points: Vec<(i32, i32)>,
}

/// Encode the unknown quest IDs gathered from the 25-slot quest log.
pub fn query(quest_ids: &[u32]) -> Result<Vec<u8>> {
    if quest_ids.len() > 25 {
        bail!("quest POI query exceeds the 25 quest-log slots");
    }
    let mut out = Vec::with_capacity(4 + 4 * quest_ids.len());
    out.extend_from_slice(&(quest_ids.len() as u32).to_le_bytes());
    for id in quest_ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    Ok(out)
}

/// Decode a complete response, checking counts against the remaining bytes before allocating.
pub fn decode_response(mut bytes: &[u8]) -> Result<Vec<QuestPoi>> {
    fn word(bytes: &mut &[u8]) -> Result<u32> {
        let (head, rest) = bytes.split_at_checked(4).context("truncated quest POI response")?;
        *bytes = rest;
        Ok(u32::from_le_bytes(head.try_into().expect("four-byte slice")))
    }
    fn count(bytes: &mut &[u8], min_size: usize) -> Result<usize> {
        let n = word(bytes)? as usize;
        if n > bytes.len() / min_size {
            bail!("quest POI count exceeds remaining packet");
        }
        Ok(n)
    }
    let quests = count(&mut bytes, 8)?;
    let mut result = Vec::with_capacity(quests);
    for _ in 0..quests {
        let quest_id = word(&mut bytes)?;
        let blobs = count(&mut bytes, 32)?;
        let mut records = Vec::with_capacity(blobs);
        for _ in 0..blobs {
            let id = word(&mut bytes)? as i32;
            let objective_index = word(&mut bytes)? as i32;
            let map_id = word(&mut bytes)? as i32;
            let world_map_area_id = word(&mut bytes)? as i32;
            let floor = word(&mut bytes)? as i32;
            let priority = word(&mut bytes)? as i32;
            let flags = word(&mut bytes)? as i32;
            let points = count(&mut bytes, 8)?;
            let mut coordinates = Vec::with_capacity(points);
            for _ in 0..points {
                coordinates.push((word(&mut bytes)? as i32, word(&mut bytes)? as i32));
            }
            records.push(QuestPoiBlob { id, objective_index, map_id, world_map_area_id, floor, priority, flags, points: coordinates });
        }
        result.push(QuestPoi { quest_id, blobs: records });
    }
    if !bytes.is_empty() { bail!("trailing bytes in quest POI response"); }
    Ok(result)
}
