//! 12340 equipment manager packets: the saved gear-set list the server sends at login, the id the
//! server assigns to a saved set, the result of a set's use, and the three client sends that save,
//! delete and use a set.
//!
//! Every reader validates one body against the pinned 3.3.5a client decomp and the pinned
//! AzerothCore build-12340 writer named beside it; every encoder writes the body the stock client
//! writes. A slot guid of `1` is the client's "ignored slot" marker on both directions
//! (`CGEquipmentManager::SaveSet`, `0x5ADE50`; `Player::SendEquipmentSetList`,
//! `Player.cpp:14795-14802`). This module owns no game state.
//!
//! Source anchors (pinned `D:/Projects/wow/reference/wowdecomp-reference` at
//! `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` and
//! `D:/Projects/wow/server/WM_BridgeLab/src/azerothcore`):
//!
//! * list: `Packet_SMSG_EQUIPMENT_SET_LIST` `0x5AF490` (`WoW/Source/Ui/EquipmentManager.cpp`);
//!   `Player::SendEquipmentSetList` (`Player.cpp:14780-14808`)
//! * saved: `Packet_SMSG_EQUIPMENT_SET_SAVED` `0x5AF6C0`
//! * use result: `Packet_SMSG_EQUIPMENT_SET_USE_RESULT` `0x5AF710`
//! * save: `CGEquipmentManager::SaveSet` `0x5ADE50`; `WorldSession::HandleEquipmentSetSave`
//!   (`CharacterHandler.cpp:1746-1815`)
//! * delete: `CGEquipmentManager::DeleteEquipmentSet` `0x5AE260`;
//!   `WorldSession::HandleEquipmentSetDelete` (`CharacterHandler.cpp:1817-1825`)
//! * use: `CGEquipmentManager::UseEquipmentSet` `0x5ADA20`;
//!   `WorldSession::HandleEquipmentSetUse` (`CharacterHandler.cpp:1827-`)

use super::{pack_guid, DecodeError, Reader};
use anyhow::Result;

/// The equipment slots a set names: `EQUIPMENT_SET::m_items[19]` (`EquipmentManager.h`).
pub const EQUIPMENT_SET_SLOTS: usize = 19;

/// The most sets the client holds: the list reader stops at ten (`0x5AF490`,
/// `i < 10`) and `CanCreateSet` (`0x5AF460`) refuses an eleventh.
pub const MAX_EQUIPMENT_SETS: usize = 10;

/// The raw slot guid that marks an ignored slot (`0x5ADE50`, `Player.cpp:14797-14799`).
pub const IGNORED_SLOT_GUID: u64 = 1;

/// `EQUIPMENT_SET::m_name[128]`: 127 bytes plus the terminator.
pub const MAX_EQUIPMENT_SET_NAME: usize = 127;

/// `EQUIPMENT_SET::m_icon[256]`: 255 bytes plus the terminator.
pub const MAX_EQUIPMENT_SET_ICON: usize = 255;

/// One saved set as `SMSG_LOAD_EQUIPMENT_SET` carries it and `CMSG_SAVE_EQUIPMENT_SET` writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSet {
    /// The server's set guid; 0 for a set the server has not answered yet.
    pub guid: u64,
    /// The set's index, 0..=9 (`EQUIPMENT_SET::m_id`).
    pub id: u32,
    pub name: String,
    /// The icon file name, without the `Interface\Icons\` directory.
    pub icon: String,
    /// One raw item guid per equipment slot: 0 empty, [`IGNORED_SLOT_GUID`] ignored.
    pub items: [u64; EQUIPMENT_SET_SLOTS],
}

/// `SMSG_EQUIPMENT_SET_ID`: the guid the server gave set `id`; 0 refuses the save
/// (`OnEquipmentSetSaved`, `0x5AE760`, deletes the set then).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentSetSaved {
    pub id: u32,
    pub guid: u64,
}

/// One slot of `CMSG_EQUIPMENT_SET_USE`: the item guid (or [`IGNORED_SLOT_GUID`], or 0 for
/// "empty this slot") and where the client found it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EquipmentSetUseSlot {
    pub item: u64,
    pub bag: u8,
    pub slot: u8,
}

fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(DecodeError::trailing(r.remaining()).into())
    }
}

fn bounded_string(r: &mut Reader<'_>, max: usize, detail: &'static str) -> Result<String> {
    let s = r.cstring()?;
    if s.len() > max {
        return Err(DecodeError::invalid_value(detail)
            .with_value(s.len() as u32)
            .into());
    }
    Ok(s)
}

fn read_set(r: &mut Reader<'_>) -> Result<EquipmentSet> {
    let guid = r.packed_guid()?;
    let id = r.u32()?;
    let name = bounded_string(r, MAX_EQUIPMENT_SET_NAME, "equipment set name too long")?;
    let icon = bounded_string(r, MAX_EQUIPMENT_SET_ICON, "equipment set icon too long")?;
    let mut items = [0u64; EQUIPMENT_SET_SLOTS];
    for item in &mut items {
        *item = r.packed_guid()?;
    }
    Ok(EquipmentSet {
        guid,
        id,
        name,
        icon,
        items,
    })
}

/// `SMSG_LOAD_EQUIPMENT_SET` (`0x5AF490`): a `u32` count, then per set a packed set guid, the
/// `u32` id, the name and icon strings and 19 packed slot guids. The client stores at most ten;
/// a larger count is rejected rather than half-read.
pub fn read_load_equipment_set(body: &[u8]) -> Result<Vec<EquipmentSet>> {
    let mut r = Reader::new(body);
    let count = r.u32()? as usize;
    if count > MAX_EQUIPMENT_SETS {
        return Err(DecodeError::invalid_count("equipment set count")
            .with_value(count as u32)
            .into());
    }
    let sets = (0..count)
        .map(|_| read_set(&mut r))
        .collect::<Result<Vec<_>>>()?;
    finish(&r)?;
    Ok(sets)
}

/// `SMSG_EQUIPMENT_SET_ID` (`0x5AF6C0`): the `u32` id, then the packed set guid.
pub fn read_equipment_set_id(body: &[u8]) -> Result<EquipmentSetSaved> {
    let mut r = Reader::new(body);
    let saved = EquipmentSetSaved {
        id: r.u32()?,
        guid: r.packed_guid()?,
    };
    finish(&r)?;
    Ok(saved)
}

/// `SMSG_USE_EQUIPMENT_SET_RESULT` (`0x5AF710`): one result byte, 0 for success; 4 is the
/// bags-full failure the client names (`EQUIPMENT_MANAGER_BAGS_FULL`).
pub fn read_use_equipment_set_result(body: &[u8]) -> Result<u8> {
    let mut r = Reader::new(body);
    let result = r.u8()?;
    finish(&r)?;
    Ok(result)
}

/// `CMSG_SAVE_EQUIPMENT_SET` (`0x5ADE50`): the packed set guid (0 for a new set), the `u32` id,
/// the name and icon strings, then 19 packed slot guids.
pub fn encode_save_equipment_set(set: &EquipmentSet) -> Vec<u8> {
    let mut out = pack_guid(set.guid);
    out.extend_from_slice(&set.id.to_le_bytes());
    out.extend_from_slice(set.name.as_bytes());
    out.push(0);
    out.extend_from_slice(set.icon.as_bytes());
    out.push(0);
    for &item in &set.items {
        out.extend(pack_guid(item));
    }
    out
}

/// `CMSG_EQUIPMENT_SET_DELETE` (`0x5AE260`): the packed set guid.
pub fn encode_equipment_set_delete(set_guid: u64) -> Vec<u8> {
    pack_guid(set_guid)
}

/// `CMSG_EQUIPMENT_SET_USE` (`0x5ADA20`): per slot 0..=18 the packed item guid, the source bag
/// byte and the source slot byte.
pub fn encode_equipment_set_use(slots: &[EquipmentSetUseSlot; EQUIPMENT_SET_SLOTS]) -> Vec<u8> {
    let mut out = Vec::with_capacity(EQUIPMENT_SET_SLOTS * 11);
    for slot in slots {
        out.extend(pack_guid(slot.item));
        out.push(slot.bag);
        out.push(slot.slot);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(guid: u64, id: u32, name: &str) -> EquipmentSet {
        let mut items = [0u64; EQUIPMENT_SET_SLOTS];
        items[0] = 0x4000_0000_0000_1234;
        items[4] = IGNORED_SLOT_GUID;
        items[15] = 0x4000_0000_0000_00FF;
        EquipmentSet {
            guid,
            id,
            name: name.into(),
            icon: "INV_Sword_04".into(),
            items,
        }
    }

    /// The list is the save body per set behind a count, so a save's bytes read back as a list
    /// row; ignored and empty slots survive.
    #[test]
    fn equipment_set_list_round_trips_through_the_save_layout() {
        let rows = [set(0x77, 0, "Tank"), set(0x1_0000_0078, 3, "Heal")];
        let mut body = (rows.len() as u32).to_le_bytes().to_vec();
        for row in &rows {
            body.extend(encode_save_equipment_set(row));
        }
        assert_eq!(read_load_equipment_set(&body).unwrap(), rows);
        assert!(read_load_equipment_set(&0u32.to_le_bytes())
            .unwrap()
            .is_empty());
        // Eleven sets cannot be stored; a trailing byte or a cut body is refused.
        assert!(read_load_equipment_set(&11u32.to_le_bytes()).is_err());
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(read_load_equipment_set(&trailing).is_err());
        assert!(read_load_equipment_set(&body[..body.len() - 1]).is_err());
    }

    /// `SMSG_EQUIPMENT_SET_ID` and `SMSG_USE_EQUIPMENT_SET_RESULT`, the two short replies.
    #[test]
    fn equipment_set_replies_decode() {
        let mut body = 7u32.to_le_bytes().to_vec();
        body.extend(pack_guid(0x1234));
        assert_eq!(
            read_equipment_set_id(&body).unwrap(),
            EquipmentSetSaved {
                id: 7,
                guid: 0x1234
            }
        );
        let mut refused = 2u32.to_le_bytes().to_vec();
        refused.extend(pack_guid(0));
        assert_eq!(read_equipment_set_id(&refused).unwrap().guid, 0);
        assert_eq!(read_use_equipment_set_result(&[4]).unwrap(), 4);
        assert!(read_use_equipment_set_result(&[0, 0]).is_err());
        assert!(read_use_equipment_set_result(&[]).is_err());
    }

    /// The delete and use sends: a packed guid, and 19 (guid, bag, slot) triples.
    #[test]
    fn equipment_set_delete_and_use_encode_the_client_layout() {
        assert_eq!(encode_equipment_set_delete(0x0102), vec![0b11, 0x02, 0x01]);
        let mut slots = [EquipmentSetUseSlot::default(); EQUIPMENT_SET_SLOTS];
        slots[0] = EquipmentSetUseSlot {
            item: 0x55,
            bag: 0xFF,
            slot: 23,
        };
        slots[1].item = IGNORED_SLOT_GUID;
        let body = encode_equipment_set_use(&slots);
        // Slot 0: mask 0b1, 0x55, bag, slot; slot 1: mask 0b1, 0x01, 0, 0; the rest mask 0, 0, 0.
        assert_eq!(&body[..8], &[0b1, 0x55, 0xFF, 23, 0b1, 0x01, 0, 0]);
        assert_eq!(body.len(), 8 + 17 * 3);
    }
}
