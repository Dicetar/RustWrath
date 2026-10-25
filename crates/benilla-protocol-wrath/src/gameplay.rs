//! The first ordinary-gameplay wire slice: the 3.3.5 client messages the 1.12 action/UI owners
//! already produce, and the server replies those owners already understand.
//!
//! Layouts are from the local AzerothCore build 12340 sources named beside each entry. Nothing
//! here owns game state or time; the app maps these typed messages onto its existing
//! `ClientCommand`/`SessionEvent` seam.

#![allow(dead_code)]

use anyhow::Result;

use crate::cast_failure;

/// `TARGET_FLAG_UNIT` (`SharedDefines.h`): a unit GUID follows the target mask, packed.
pub const TARGET_FLAG_UNIT: u32 = 0x0002;

/// `CMSG_SET_SELECTION` (`MiscHandler.cpp:543`): full `u64` GUID.
pub fn logout_request() -> Vec<u8> {
    Vec::new()
}

pub fn logout_cancel() -> Vec<u8> {
    Vec::new()
}

pub fn set_selection(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_ATTACKSWING` (`CombatHandler.cpp:28`): full `u64` GUID.
pub fn attack_swing(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_ATTACKSTOP` (`CombatHandler.cpp:56`): empty.
pub fn attack_stop() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_CAST_SPELL` (`SpellHandler.cpp:376`): `u8 count, u32 spell, u8 castFlags`, then
/// `SpellCastTargets::Read` (`Spell.cpp:127`) as `u32 mask` and a packed unit GUID.
pub fn cast_spell(cast_count: u8, spell_id: u32, target: Option<u64>) -> Vec<u8> {
    let mut p = Vec::with_capacity(4 + 8);
    p.push(cast_count);
    p.extend_from_slice(&spell_id.to_le_bytes());
    p.push(0);
    match target {
        Some(guid) => {
            p.extend_from_slice(&TARGET_FLAG_UNIT.to_le_bytes());
            p.extend_from_slice(&pack_guid(guid));
        }
        None => p.extend_from_slice(&0u32.to_le_bytes()),
    }
    p
}

/// `CMSG_GOSSIP_HELLO` (`NPCHandler.cpp:137`): full `u64` GUID.
pub fn gossip_hello(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// `CMSG_GOSSIP_SELECT_OPTION` (`MiscHandler.cpp:89`): full GUID, menu id, option id, then the
/// code string only for a coded option.
pub fn gossip_select(npc: u64, menu_id: u32, option: u32, code: Option<&str>) -> Vec<u8> {
    let mut p = Vec::with_capacity(24);
    p.extend_from_slice(&npc.to_le_bytes());
    p.extend_from_slice(&menu_id.to_le_bytes());
    p.extend_from_slice(&option.to_le_bytes());
    if let Some(code) = code {
        p.extend_from_slice(code.as_bytes());
        p.push(0);
    }
    p
}

/// `CMSG_NPC_TEXT_QUERY` (`QueryHandler.cpp:280`): `u32 textId` then full `u64` GUID.
pub fn npc_text_query(text_id: u32, npc: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&text_id.to_le_bytes());
    p.extend_from_slice(&npc.to_le_bytes());
    p
}

/// `CMSG_QUESTGIVER_STATUS_QUERY` (`QuestHandler.cpp:35`): full `u64` GUID.
pub fn questgiver_status_query(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// `CMSG_QUESTGIVER_HELLO` (`QuestHandler.cpp:76`): full `u64` GUID.
pub fn questgiver_hello(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// `CMSG_QUESTGIVER_QUERY_QUEST` (`QuestHandler.cpp:204`): GUID, quest, `u8` (0).
pub fn questgiver_query(npc: u64, quest: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(13);
    p.extend_from_slice(&npc.to_le_bytes());
    p.extend_from_slice(&quest.to_le_bytes());
    p.push(0);
    p
}

/// `CMSG_QUESTGIVER_ACCEPT_QUEST` (`QuestHandler.cpp:118`): GUID, quest, `u32` divider (0).
pub fn questgiver_accept(npc: u64, quest: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&npc.to_le_bytes());
    p.extend_from_slice(&quest.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p
}

/// `CMSG_QUESTGIVER_COMPLETE_QUEST` (`QuestHandler.cpp:489`): GUID, quest.
pub fn questgiver_complete(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// `CMSG_QUESTGIVER_REQUEST_REWARD` (`QuestHandler.cpp:364`): GUID, quest.
pub fn questgiver_request_reward(npc: u64, quest: u32) -> Vec<u8> {
    guid_quest(npc, quest)
}

/// `CMSG_QUESTGIVER_CHOOSE_REWARD` (`QuestHandler.cpp:252`): GUID, quest, zero-based choice.
pub fn questgiver_choose_reward(npc: u64, quest: u32, choice: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&npc.to_le_bytes());
    p.extend_from_slice(&quest.to_le_bytes());
    p.extend_from_slice(&choice.to_le_bytes());
    p
}

/// `CMSG_QUEST_QUERY` (`QuestHandler.cpp:234`): quest id.
pub fn gameobject_query(entry: u32, guid: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&entry.to_le_bytes());
    p.extend_from_slice(&guid.to_le_bytes());
    p
}

pub fn item_query(entry: u32) -> Vec<u8> {
    entry.to_le_bytes().to_vec()
}

pub fn quest_query(quest: u32) -> Vec<u8> {
    quest.to_le_bytes().to_vec()
}

/// `CMSG_LOOT` (`LootHandler.cpp:238`): full `u64` GUID.
pub fn loot(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_AUTOSTORE_LOOT_ITEM` (`LootHandler.cpp:33`): the wire slot byte.
pub fn autostore_loot_item(slot: u8) -> Vec<u8> {
    vec![slot]
}

/// `CMSG_LOOT_MONEY` (`LootHandler.cpp:114`): empty.
pub fn loot_money() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_LOOT_RELEASE` (`LootHandler.cpp:256`): full `u64` GUID.
pub fn loot_release(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_SET_ACTION_BUTTON` is shared between the fork's 1.12 and 3.3.5 sessions at the app
/// seam; 3.3.5 writes the slot byte and the packed word.
pub fn set_action_button(button: u8, packed: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(5);
    p.push(button);
    p.extend_from_slice(&packed.to_le_bytes());
    p
}

fn guid_quest(npc: u64, quest: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&npc.to_le_bytes());
    p.extend_from_slice(&quest.to_le_bytes());
    p
}

/// A packed GUID (`ByteBuffer.h:503-516`): mask byte then the named bytes.
pub fn pack_guid(guid: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    let mut mask = 0u8;
    let mut bytes = [0u8; 8];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (guid >> (i * 8)) as u8;
        if *b != 0 {
            mask |= 1 << i;
        }
    }
    out.push(mask);
    for b in bytes {
        if b != 0 {
            out.push(b);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionButton {
    pub slot: u8,
    pub action: u32,
    pub kind: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestListEntry {
    pub quest_id: u32,
    pub icon: u32,
    pub level: u32,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GossipOption {
    pub index: u32,
    pub icon: u8,
    pub coded: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NpcTextBlock {
    pub probability: f32,
    pub male: String,
    pub female: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardItem {
    pub item_id: u32,
    pub count: u32,
    pub display_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDetails {
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub details: String,
    pub objectives: String,
    pub auto_finish: u32,
    pub choices: Vec<RewardItem>,
    pub rewards: Vec<RewardItem>,
    pub money: i32,
    pub reward_spell: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestRequestItems {
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub request_text: String,
    pub emote: u32,
    pub close_on_cancel: u32,
    pub required_money: u32,
    pub required_items: Vec<RewardItem>,
    pub is_complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestOfferReward {
    pub npc: u64,
    pub quest_id: u32,
    pub title: String,
    pub offer_text: String,
    pub auto_finish: u32,
    pub choices: Vec<RewardItem>,
    pub rewards: Vec<RewardItem>,
    pub money: i32,
    pub quest_flags: u32,
    pub reward_spell: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestComplete {
    pub quest_id: u32,
    pub xp: u32,
    pub money: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemInfo {
    pub class: u32,
    pub subclass: u32,
    pub name: String,
    pub display_info_id: u32,
    pub quality: u32,
    pub flags: u32,
    pub buy_price: u32,
    pub sell_price: u32,
    pub inventory_type: u32,
    pub allowable_class: i32,
    pub allowable_race: i32,
    pub item_level: u32,
    pub required_level: u32,
    pub required_skill: u32,
    pub required_skill_rank: u32,
    pub required_spell: u32,
    pub required_honor_rank: u32,
    pub required_city_rank: u32,
    pub required_rep_faction: u32,
    pub required_rep_rank: u32,
    pub max_count: u32,
    pub stackable: u32,
    pub container_slots: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestObjective {
    pub creature_or_go: u32,
    pub required_count: u32,
    pub item_id: u32,
    pub item_count: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestTemplate {
    pub quest_id: u32,
    pub method: u32,
    pub level: u32,
    pub zone_or_sort: i32,
    pub quest_type: u32,
    pub rep_objective_faction: u32,
    pub rep_objective_value: u32,
    pub opposite_rep_faction: u32,
    pub opposite_rep_value: u32,
    pub next_quest_in_chain: u32,
    pub money: i32,
    pub money_max_level: u32,
    pub reward_spell: u32,
    pub src_item_id: u32,
    pub flags: u32,
    pub rewards: [(u32, u32); 4],
    pub choices: [(u32, u32); 4],
    pub point_map_id: u32,
    pub point_x: f32,
    pub point_y: f32,
    pub point_opt: u32,
    pub title: String,
    pub objectives_text: String,
    pub details: String,
    pub end_text: String,
    pub objectives: [QuestObjective; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LootItem {
    pub slot: u8,
    pub item_id: u32,
    pub count: u32,
    pub display_info_id: u32,
    pub random_property_id: u32,
    pub slot_type: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPush {
    pub player: u64,
    pub from_npc: bool,
    pub created: bool,
    pub show_in_chat: bool,
    pub bag_slot: u8,
    pub item_slot: u32,
    pub item_entry: u32,
    pub suffix_factor: u32,
    pub random_property_id: i32,
    pub count: u32,
}

/// One typed reply this slice understands.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    AttackStart { attacker: u64, victim: u64 },
    AttackStop { attacker: u64, victim: u64 },
    AttackSwingError(AttackSwingError),
    CastFailed {
        spell_id: u32,
        reason: cast_failure::Reason,
        arg: Option<u32>,
    },
    SpellStart { caster: u64, spell_id: u32, cast_flags: u32, cast_time_ms: i32 },
    SpellGo {
        caster: u64,
        spell_id: u32,
        cast_flags: u32,
        hits: Vec<u64>,
        misses: Vec<(u64, u8, u8)>,
        target_mask: u16,
        item_caster: Option<u64>,
    },
    SpellFailure { caster: u64, spell_id: u32 },
    LogoutResponse { reason: u32, instant: bool },
    LoggedOut,
    LogoutCancelled,
    InitialSpells { spells: Vec<u32> },
    ItemInfo { entry: u32, info: Option<ItemInfo> },
    PlayerName { guid: u64, name: String, race: u32, gender: u32, class: u32 },
    GameObjectInfo {
        entry: u32,
        type_id: u32,
        display_id: u32,
        name: String,
        data: [i32; 24],
    },
    CreatureName {
        entry: u32,
        name: Option<String>,
        subname: Option<String>,
        creature_type: u32,
        pet_family: u32,
        rank: u32,
        type_flags: u32,
        display_id: u32,
    },
    ActionButtons { buttons: Vec<ActionButton> },
    SpellLearned { spell_id: u32 },
    Gossip {
        npc: u64,
        menu_id: u32,
        text_id: u32,
        options: Vec<GossipOption>,
        quests: Vec<QuestListEntry>,
    },
    GossipComplete,
    QuestGiverStatus { npc: u64, status: u32 },
    QuestGreeting {
        npc: u64,
        greeting: String,
        emote_delay: u32,
        emote: u32,
        quests: Vec<QuestListEntry>,
    },
    QuestDetails(QuestDetails),
    QuestProgress(QuestRequestItems),
    QuestOffer(QuestOfferReward),
    QuestComplete(QuestComplete),
    QuestTemplate(Box<QuestTemplate>),
    QuestObjectivesComplete { quest_id: u32 },
    QuestObjectiveKill { quest_id: u32, entry: u32, count: u32, required: u32 },
    QuestObjectiveItem { item_id: u32, count: u32 },
    QuestLogFull,
    QuestGiverInvalid { reason: u32 },
    QuestGiverFailed { quest_id: u32, reason: u32 },
    QuestFailed { quest_id: u32, timed: bool },
    NpcText { text_id: u32, blocks: Vec<NpcTextBlock> },
    LootResponse {
        guid: u64,
        loot_type: u8,
        gold: u32,
        items: Vec<LootItem>,
    },
    LootError { guid: u64, error: u8 },
    LootReleaseResponse { guid: u64, result: u8 },
    LootRemoved { slot: u8 },
    LootMoney { amount: u32 },
    LootClearMoney,
    ItemPush(ItemPush),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackSwingError {
    NotInRange,
    BadFacing,
    DeadOrUnattackable,
}

/// Parse one server opcode; `None` means this slice does not translate that opcode.
pub fn parse(opcode: u16, body: &[u8]) -> Option<Result<ServerEvent>> {
    use crate::opcode::*;
    Some(match opcode {
        SMSG_ATTACKSTART => read_attack_start(body),
        SMSG_ATTACKSTOP => read_attack_stop(body),
        SMSG_ATTACKSWING_NOTINRANGE => Ok(ServerEvent::AttackSwingError(AttackSwingError::NotInRange)),
        SMSG_ATTACKSWING_BADFACING => Ok(ServerEvent::AttackSwingError(AttackSwingError::BadFacing)),
        SMSG_ATTACKSWING_DEADTARGET | SMSG_ATTACKSWING_CANT_ATTACK => {
            Ok(ServerEvent::AttackSwingError(AttackSwingError::DeadOrUnattackable))
        }
        SMSG_CAST_FAILED => read_cast_failed(body),
        SMSG_SPELL_START => read_spell_start(body),
        SMSG_SPELL_GO => read_spell_go(body),
        SMSG_SPELL_FAILURE => read_spell_failure(body),
        SMSG_LOGOUT_RESPONSE => read_logout_response(body),
        SMSG_LOGOUT_COMPLETE => Ok(ServerEvent::LoggedOut),
        SMSG_LOGOUT_CANCEL_ACK => Ok(ServerEvent::LogoutCancelled),
        SMSG_INITIAL_SPELLS => read_initial_spells(body),
        SMSG_ITEM_QUERY_SINGLE_RESPONSE => read_item_info(body),
        SMSG_NAME_QUERY_RESPONSE => read_name(body),
        SMSG_CREATURE_QUERY_RESPONSE => read_creature(body),
        SMSG_GAMEOBJECT_QUERY_RESPONSE => read_gameobject(body),
        SMSG_ACTION_BUTTONS => read_action_buttons(body),
        SMSG_LEARNED_SPELL => u32_body(body).map(|spell_id| ServerEvent::SpellLearned { spell_id }),
        SMSG_GOSSIP_MESSAGE => read_gossip(body),
        SMSG_GOSSIP_COMPLETE => Ok(ServerEvent::GossipComplete),
        SMSG_QUESTGIVER_STATUS => read_questgiver_status(body),
        SMSG_QUESTGIVER_QUEST_LIST => read_quest_greeting(body),
        SMSG_QUESTGIVER_QUEST_DETAILS => read_quest_details(body),
        SMSG_QUESTGIVER_REQUEST_ITEMS => read_quest_request(body),
        SMSG_QUESTGIVER_OFFER_REWARD => read_quest_offer(body),
        SMSG_QUESTGIVER_QUEST_COMPLETE => read_quest_complete(body),
        SMSG_QUEST_QUERY_RESPONSE => read_quest_template(body).map(|q| ServerEvent::QuestTemplate(Box::new(q))),
        SMSG_QUESTUPDATE_COMPLETE => u32_body(body).map(|quest_id| ServerEvent::QuestObjectivesComplete { quest_id }),
        SMSG_QUESTUPDATE_ADD_KILL => read_quest_kill(body),
        SMSG_QUESTUPDATE_ADD_ITEM if body.is_empty() => return None,
        SMSG_QUESTUPDATE_ADD_ITEM => read_quest_item(body),
        SMSG_QUESTLOG_FULL => Ok(ServerEvent::QuestLogFull),
        SMSG_QUESTGIVER_QUEST_INVALID => u32_body(body).map(|reason| ServerEvent::QuestGiverInvalid { reason }),
        SMSG_QUESTGIVER_QUEST_FAILED => read_quest_failed(body),
        SMSG_QUESTUPDATE_FAILED => u32_body(body).map(|quest_id| ServerEvent::QuestFailed { quest_id, timed: false }),
        SMSG_QUESTUPDATE_FAILEDTIMER => u32_body(body).map(|quest_id| ServerEvent::QuestFailed { quest_id, timed: true }),
        SMSG_NPC_TEXT_UPDATE => read_npc_text(body),
        SMSG_LOOT_RESPONSE => read_loot_response(body),
        SMSG_LOOT_RELEASE_RESPONSE => read_loot_release(body),
        SMSG_LOOT_REMOVED => body.first().copied().ok_or_else(|| anyhow::anyhow!("payload cut short")).map(|slot| ServerEvent::LootRemoved { slot }),
        SMSG_LOOT_MONEY_NOTIFY => u32_body(body).map(|amount| ServerEvent::LootMoney { amount }),
        SMSG_LOOT_CLEAR_MONEY => Ok(ServerEvent::LootClearMoney),
        SMSG_ITEM_PUSH_RESULT => read_item_push(body),
        _ => return None,
    })
}

fn u32_body(b: &[u8]) -> Result<u32> {
    Reader::new(b).u32()
}

fn read_attack_start(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::AttackStart { attacker: r.u64()?, victim: r.u64()? })
}

fn read_attack_stop(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let attacker = r.packed_guid()?;
    let victim = if r.remaining() == 0 { 0 } else { r.packed_guid()? };
    Ok(ServerEvent::AttackStop { attacker, victim })
}

fn read_cast_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let _cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let wrath_reason = r.u8()?;
    // `Spell::WriteCastResultInfo` writes argument words for these reasons only. A 1.12 reader
    // uses the first word for the arms it has; the second word (equipped-item mask) is not part
    // of the 1.12 event and is deliberately left in the packet.
    let arg = match wrath_reason {
        29..=31 | 54 | 100 | 101 | 102 | 131 | 147 => {
            (r.remaining() >= 4).then(|| r.u32()).transpose()?
        }
        _ => None,
    };
    Ok(ServerEvent::CastFailed {
        spell_id,
        reason: cast_failure::reason(wrath_reason),
        arg,
    })
}

fn read_spell_start(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let _item = r.packed_guid()?;
    let caster = r.packed_guid()?;
    let _cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let cast_flags = r.u32()?;
    let cast_time_ms = r.i32()?;
    Ok(ServerEvent::SpellStart { caster, spell_id, cast_flags, cast_time_ms })
}

fn read_name(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.packed_guid()?;
    if r.u8()? != 0 {
        return Ok(ServerEvent::PlayerName { guid, name: String::new(), race: 0, gender: 0, class: 0 });
    }
    let name = r.cstring()?;
    let _realm = r.cstring()?;
    let race = r.u8()? as u32;
    let gender = r.u8()? as u32;
    let class = r.u8()? as u32;
    Ok(ServerEvent::PlayerName { guid, name, race, gender, class })
}

fn read_gameobject(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let entry = r.u32()?;
    if entry & 0x8000_0000 != 0 {
        return Ok(ServerEvent::GameObjectInfo {
            entry: entry & 0x7fff_ffff,
            type_id: 0,
            display_id: 0,
            name: String::new(),
            data: [0; 24],
        });
    }
    let type_id = r.u32()?;
    let display_id = r.u32()?;
    let name = r.cstring()?;
    for _ in 0..3 {
        let _ = r.cstring()?;
    }
    let _icon = r.cstring()?;
    let _cast_bar = r.cstring()?;
    let _unk = r.cstring()?;
    // 3.3.5 has a wider, differently indexed data block than the 24-dword 1.12 record. The
    // columns cannot be copied by position: the 1.12 consumer resolves lock/page/meeting-stone
    // fields by type, and wrong numbers would misroute interaction. Keep the name and display,
    // but report the columns as absent so the 1.12 owners do not read a foreign layout.
    for _ in 0..24 {
        let _ = r.i32()?;
    }
    Ok(ServerEvent::GameObjectInfo {
        entry,
        type_id,
        display_id,
        name,
        data: [0; 24],
    })
}

fn read_creature(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let entry = r.u32()?;
    if entry & 0x8000_0000 != 0 {
        return Ok(ServerEvent::CreatureName {
            entry: entry & 0x7fff_ffff,
            name: None,
            subname: None,
            creature_type: 0,
            pet_family: 0,
            rank: 0,
            type_flags: 0,
            display_id: 0,
        });
    }
    let name = r.cstring()?;
    for _ in 0..3 {
        let _ = r.cstring()?;
    }
    let subname = r.cstring()?;
    let _icon = r.cstring()?;
    let type_flags = r.u32()?;
    let creature_type = r.u32()?;
    let pet_family = r.u32()?;
    let rank = r.u32()?;
    let _kill_credit_0 = r.u32()?;
    let _kill_credit_1 = r.u32()?;
    let display_id = r.u32()?;
    Ok(ServerEvent::CreatureName {
        entry,
        name: Some(name),
        subname: Some(subname).filter(|s| !s.is_empty()),
        creature_type,
        pet_family,
        rank,
        type_flags,
        display_id,
    })
}

fn read_item_info(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let entry = r.u32()?;
    if entry & 0x8000_0000 != 0 {
        return Ok(ServerEvent::ItemInfo { entry: entry & 0x7fff_ffff, info: None });
    }
    let class = r.u32()?;
    let subclass = r.u32()?;
    let _sound_override_subclass = r.i32()?;
    let name = r.cstring()?;
    for _ in 0..3 {
        let _ = r.cstring()?;
    }
    let display_info_id = r.u32()?;
    let quality = r.u32()?;
    let flags = r.u32()?;
    let _flags2 = r.u32()?;
    let buy_price = r.u32()?;
    let sell_price = r.u32()?;
    let inventory_type = r.u32()?;
    let allowable_class = r.i32()?;
    let allowable_race = r.i32()?;
    let item_level = r.u32()?;
    let required_level = r.u32()?;
    let required_skill = r.u32()?;
    let required_skill_rank = r.u32()?;
    let required_spell = r.u32()?;
    let required_honor_rank = r.u32()?;
    let required_city_rank = r.u32()?;
    let required_rep_faction = r.u32()?;
    let required_rep_rank = r.u32()?;
    let max_count = r.i32()?.max(0) as u32;
    let stackable = r.i32()?.max(0) as u32;
    let container_slots = r.u32()?;
    Ok(ServerEvent::ItemInfo {
        entry,
        info: Some(ItemInfo {
            class, subclass, name, display_info_id, quality, flags, buy_price, sell_price,
            inventory_type, allowable_class, allowable_race, item_level, required_level,
            required_skill, required_skill_rank, required_spell, required_honor_rank,
            required_city_rank, required_rep_faction, required_rep_rank, max_count, stackable,
            container_slots,
        }),
    })
}

fn read_logout_response(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LogoutResponse { reason: r.u32()?, instant: r.u8()? != 0 })
}

fn read_spell_failure(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.packed_guid()?;
    let _cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let _reason = r.u8()?;
    // 3.3.5 broadcasts this to every observer, self included; 1.12's matching event is
    // `SMSG_SPELL_FAILED_OTHER`, which is caster-keyed and never a player `SMSG_CAST_RESULT`.
    Ok(ServerEvent::SpellFailure { caster, spell_id })
}

fn read_spell_go(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let item_or_caster = r.packed_guid()?;
    let caster = r.packed_guid()?;
    let _cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let cast_flags = r.u32()?;
    let _timestamp = r.u32()?;
    let hit_count = r.u8()?;
    let mut hits = Vec::with_capacity(hit_count as usize);
    for _ in 0..hit_count {
        hits.push(r.u64()?);
    }
    let miss_count = r.u8()?;
    let mut misses = Vec::with_capacity(miss_count as usize);
    for _ in 0..miss_count {
        let target = r.u64()?;
        let info = r.u8()?;
        let reflect = if info == 11 { r.u8()? } else { 0 };
        misses.push((target, info, reflect));
    }
    let target_mask = if r.remaining() >= 4 { r.u32()? as u16 } else { 0 };
    Ok(ServerEvent::SpellGo {
        caster,
        spell_id,
        cast_flags,
        hits,
        misses,
        target_mask,
        item_caster: (item_or_caster != 0 && item_or_caster != caster).then_some(item_or_caster),
    })
}

fn read_initial_spells(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let _unk = r.u8()?;
    let count = r.u16()?;
    let mut spells = Vec::with_capacity(count as usize);
    for _ in 0..count {
        spells.push(r.u32()?);
        let _slot = r.u16()?;
    }
    Ok(ServerEvent::InitialSpells { spells })
}

fn read_action_buttons(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let state = r.u8()?;
    let mut buttons = Vec::new();
    if state != 2 {
        let mut slot = 0u8;
        while r.remaining() >= 4 {
            let packed = r.u32()?;
            if packed != 0 {
                buttons.push(ActionButton {
                    slot,
                    action: packed & 0x00ff_ffff,
                    kind: (packed >> 24) as u8,
                });
            }
            slot = slot.saturating_add(1);
        }
    }
    Ok(ServerEvent::ActionButtons { buttons })
}

fn read_gossip(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let menu_id = r.u32()?;
    let text_id = r.u32()?;
    let options_count = r.u32()?;
    let mut options = Vec::with_capacity(options_count.min(64) as usize);
    for _ in 0..options_count {
        let index = r.u32()?;
        let icon = r.u8()?;
        let coded = r.u8()? != 0;
        let _box_money = r.u32()?;
        options.push(GossipOption { index, icon, coded, message: r.cstring()? });
        let _box_message = r.cstring()?;
    }
    let quests = read_quest_rows(&mut r)?;
    Ok(ServerEvent::Gossip { npc, menu_id, text_id, options, quests })
}

/// 3.3.5 `QuestGiverStatus` (`QuestDef.h:110-126`) to 1.12 `dialog_status`
/// (`quest/giver.rs:13-22`). The numeric values shifted: 3.3.5 puts INCOMPLETE at 5 and
/// AVAILABLE at 8, while 1.12 puts them at 3 and 5. Unknown/scripted values have no marker.
pub fn vanilla_quest_giver_status(wrath: u32) -> Option<u32> {
    Some(match wrath {
        0 => 0,             // NONE
        1 => 1,             // UNAVAILABLE
        2 | 4 | 7 | 8 => 5, // LOW_LEVEL_AVAILABLE, ..._AVAILABLE_REP, AVAILABLE_REP, AVAILABLE
        3 | 6 => 4,         // LOW_LEVEL_REWARD_REP, REWARD_REP
        5 => 3,             // INCOMPLETE
        9 => 7,             // REWARD2 (no minimap dot)
        10 => 6,            // REWARD (yellow minimap dot)
        _ => return None,
    })
}

fn read_questgiver_status(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let status = vanilla_quest_giver_status(r.u8()? as u32).unwrap_or(0);
    Ok(ServerEvent::QuestGiverStatus { npc, status })
}

fn read_quest_greeting(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let greeting = r.cstring()?;
    let emote_delay = r.u32()?;
    let emote = r.u32()?;
    let count = r.u8()?;
    let mut quests = Vec::with_capacity(count as usize);
    for _ in 0..count {
        quests.push(QuestListEntry {
            quest_id: r.u32()?,
            icon: vanilla_quest_giver_status(r.u32()?).unwrap_or(0),
            level: r.i32()?.max(0) as u32,
            title: {
                let _flags = r.u32()?;
                let _repeatable = r.u8()?;
                r.cstring()?
            },
        });
    }
    Ok(ServerEvent::QuestGreeting { npc, greeting, emote_delay, emote, quests })
}

fn read_quest_rows(r: &mut Reader<'_>) -> Result<Vec<QuestListEntry>> {
    let count = r.u32()?;
    let mut quests = Vec::with_capacity(count.min(64) as usize);
    for _ in 0..count {
        quests.push(QuestListEntry {
            quest_id: r.u32()?,
            icon: vanilla_quest_giver_status(r.u32()?).unwrap_or(0),
            level: r.i32()?.max(0) as u32,
            title: {
                let _flags = r.u32()?;
                let _repeatable = r.u8()?;
                r.cstring()?
            },
        });
    }
    Ok(quests)
}

fn read_reward_block(r: &mut Reader<'_>) -> Result<Vec<RewardItem>> {
    let count = r.u32()?;
    let mut items = Vec::with_capacity(count.min(16) as usize);
    for _ in 0..count {
        items.push(RewardItem { item_id: r.u32()?, count: r.u32()?, display_id: r.u32()? });
    }
    Ok(items)
}

fn read_quest_details(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let _divider = r.u32()?;
    let quest_id = r.u32()?;
    let title = r.cstring()?;
    let details = r.cstring()?;
    let objectives = r.cstring()?;
    let auto_finish = r.u8()?;
    let _flags = r.u32()?;
    let _suggested = r.u32()?;
    let _finished = r.u8()?;
    let choices = read_reward_block(&mut r)?;
    let rewards = read_reward_block(&mut r)?;
    let money = r.i32()?;
    let _xp = r.u32()?;
    let _honor = r.u32()?;
    let _honor_multiplier = r.f32()?;
    let reward_spell = r.u32()?;
    Ok(ServerEvent::QuestDetails(QuestDetails {
        npc, quest_id, title, details, objectives,
        auto_finish: auto_finish as u32, choices, rewards, money, reward_spell,
    }))
}

fn read_quest_request(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let quest_id = r.u32()?;
    let title = r.cstring()?;
    let request_text = r.cstring()?;
    let _unk = r.u32()?;
    let emote = r.u32()?;
    let close_on_cancel = r.u32()?;
    let _flags = r.u32()?;
    let _suggested = r.u32()?;
    let required_money = r.u32()?;
    let required_items = read_reward_block(&mut r)?;
    let _flag0 = r.u32()?;
    let flag_complete = r.u32()?;
    Ok(ServerEvent::QuestProgress(QuestRequestItems {
        npc, quest_id, title, request_text, emote, close_on_cancel,
        required_money, required_items, is_complete: flag_complete != 0,
    }))
}

fn read_quest_offer(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let quest_id = r.u32()?;
    let title = r.cstring()?;
    let offer_text = r.cstring()?;
    let auto_finish = r.u8()?;
    let quest_flags = r.u32()?;
    let _suggested = r.u32()?;
    let emote_count = r.u32()?;
    for _ in 0..emote_count {
        let _delay = r.u32()?;
        let _emote = r.u32()?;
    }
    let choices = read_reward_block(&mut r)?;
    let rewards = read_reward_block(&mut r)?;
    let money = r.i32()?;
    let _xp = r.u32()?;
    let _honor = r.u32()?;
    let _honor_multiplier = r.f32()?;
    let _unk = r.u32()?;
    let reward_spell = r.u32()?;
    Ok(ServerEvent::QuestOffer(QuestOfferReward {
        npc, quest_id, title, offer_text, auto_finish: auto_finish as u32,
        choices, rewards, money, quest_flags, reward_spell,
    }))
}

fn read_quest_template(b: &[u8]) -> Result<QuestTemplate> {
    let mut r = Reader::new(b);
    let quest_id = r.u32()?;
    let method = r.u32()?;
    let level = r.u32()?;
    let _min_level = r.u32()?;
    let zone_or_sort = r.i32()?;
    let quest_type = r.u32()?;
    let _suggested = r.u32()?;
    let rep_objective_faction = r.u32()?;
    let rep_objective_value = r.u32()?;
    let opposite_rep_faction = r.u32()?;
    let opposite_rep_value = r.u32()?;
    let next_quest_in_chain = r.u32()?;
    let _xp_id = r.u32()?;
    let money = r.i32()?;
    let money_max_level = r.u32()?;
    let reward_spell = r.u32()?;
    let _reward_spell_cast = r.i32()?;
    let _honor_addition = r.u32()?;
    let _honor_multiplier = r.f32()?;
    let src_item_id = r.u32()?;
    let flags = r.u32()?;
    let _char_title = r.u32()?;
    let _players_slain = r.u32()?;
    let _bonus_talents = r.u32()?;
    let _arena_points = r.u32()?;
    let _unk = r.u32()?;
    let mut rewards = [(0u32, 0u32); 4];
    for slot in &mut rewards {
        *slot = (r.u32()?, r.u32()?);
    }
    let mut choices = [(0u32, 0u32); 4];
    for slot in &mut choices {
        *slot = (r.u32()?, r.u32()?);
    }
    for _ in 0..5 {
        let _faction = r.u32()?;
        let _value = r.i32()?;
        let _override = r.i32()?;
    }
    let point_map_id = r.u32()?;
    let point_x = r.f32()?;
    let point_y = r.f32()?;
    let point_opt = r.u32()?;
    let title = r.cstring()?;
    let objectives_text = r.cstring()?;
    let details = r.cstring()?;
    let _area_description = r.cstring()?;
    let end_text = r.cstring()?;
    let mut raw = [(0u32, 0u32, 0u32, 0u32); 4];
    for slot in &mut raw {
        *slot = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
    }
    let mut item_objectives = Vec::with_capacity(6);
    for _ in 0..6 {
        let item = r.u32()?;
        let count = r.u32()?;
        if item != 0 {
            item_objectives.push((item, count));
        }
    }
    let mut item_objectives = item_objectives.into_iter();
    for slot in &mut raw {
        if slot.0 == 0 && slot.2 == 0 {
            if let Some((item, count)) = item_objectives.next() {
                slot.2 = item;
                slot.3 = count;
            }
        }
    }
    let mut texts = Vec::with_capacity(4);
    for _ in 0..4 {
        texts.push(r.cstring()?);
    }
    let objectives = std::array::from_fn(|i| QuestObjective {
        creature_or_go: raw[i].0,
        required_count: raw[i].1,
        item_id: raw[i].2,
        item_count: raw[i].3,
        text: std::mem::take(&mut texts[i]),
    });
    Ok(QuestTemplate {
        quest_id, method, level, zone_or_sort, quest_type, rep_objective_faction,
        rep_objective_value, opposite_rep_faction, opposite_rep_value, next_quest_in_chain,
        money, money_max_level, reward_spell, src_item_id, flags, rewards, choices,
        point_map_id, point_x, point_y, point_opt, title, objectives_text, details, end_text,
        objectives,
    })
}

fn read_quest_complete(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let quest_id = r.u32()?;
    let xp = r.u32()?;
    let money = r.u32()?;
    Ok(ServerEvent::QuestComplete(QuestComplete { quest_id, xp, money }))
}

fn read_quest_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::QuestGiverFailed { quest_id: r.u32()?, reason: r.u32()? })
}

fn read_quest_kill(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let quest_id = r.u32()?;
    let entry = r.u32()?;
    let count = r.u32()?;
    let required = r.u32()?;
    Ok(ServerEvent::QuestObjectiveKill { quest_id, entry, count, required })
}

fn read_quest_item(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::QuestObjectiveItem { item_id: r.u32()?, count: r.u32()? })
}

fn read_npc_text(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let text_id = r.u32()?;
    let mut blocks = Vec::with_capacity(8);
    for _ in 0..8 {
        let probability = r.f32()?;
        let male = r.cstring()?;
        let female = r.cstring()?;
        let _language = r.u32()?;
        for _ in 0..3 {
            let _delay = r.u32()?;
            let _emote = r.u32()?;
        }
        blocks.push(NpcTextBlock { probability, male, female });
    }
    Ok(ServerEvent::NpcText { text_id, blocks })
}

fn read_loot_response(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let guid = r.u64()?;
    let loot_type = r.u8()?;
    if loot_type == 0 {
        return Ok(ServerEvent::LootError { guid, error: r.u8()? });
    }
    let gold = r.u32()?;
    let count = r.u8()?;
    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        items.push(LootItem {
            slot: r.u8()?,
            item_id: r.u32()?,
            count: r.u32()?,
            display_info_id: r.u32()?,
            random_property_id: {
                let _random_suffix = r.u32()?;
                r.u32()?
            },
            slot_type: r.u8()?,
        });
    }
    Ok(ServerEvent::LootResponse { guid, loot_type, gold, items })
}

fn read_loot_release(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootReleaseResponse { guid: r.u64()?, result: r.u8()? })
}

fn read_item_push(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::ItemPush(ItemPush {
        player: r.u64()?,
        from_npc: r.u32()? != 0,
        created: r.u32()? != 0,
        show_in_chat: r.u32()? != 0,
        bag_slot: r.u8()?,
        item_slot: r.u32()?,
        item_entry: r.u32()?,
        suffix_factor: r.u32()?,
        random_property_id: r.i32()?,
        count: r.u32()?,
    }))
}

struct Reader<'a> {
    p: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(p: &'a [u8]) -> Self {
        Self { p, at: 0 }
    }
    fn remaining(&self) -> usize {
        self.p.len().saturating_sub(self.at)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.p.get(self.at..self.at + n).ok_or_else(|| anyhow::anyhow!("payload cut short"))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn cstring(&mut self) -> Result<String> {
        let rest = &self.p[self.at..];
        let end = rest.iter().position(|&b| b == 0).ok_or_else(|| anyhow::anyhow!("unterminated string"))?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.at += end + 1;
        Ok(s)
    }
    fn packed_guid(&mut self) -> Result<u64> {
        let mask = self.u8()?;
        let mut guid = 0u64;
        for i in 0..8 {
            if mask & (1 << i) != 0 {
                guid |= (self.u8()? as u64) << (i * 8);
            }
        }
        Ok(guid)
    }
}











#[cfg(test)]
mod tests {
    use super::*;

    fn le<const N: usize>(v: &[u8]) -> [u8; N] {
        v.try_into().unwrap()
    }

    #[test]
    fn cast_target_is_a_mask_then_a_packed_guid() {
        let target = 0x0000_0000_0000_002a;
        let p = cast_spell(7, 133, Some(target));
        assert_eq!(&p[..4], &[7, 0x85, 0, 0]);
        assert_eq!(u32::from_le_bytes(le::<4>(&p[6..10])), TARGET_FLAG_UNIT);
        assert_eq!(&p[10..], &pack_guid(target));
    }

    #[test]
    fn accept_has_the_divider_word_the_server_reads() {
        let p = questgiver_accept(0x1000, 7);
        assert_eq!(p.len(), 16);
        assert_eq!(u32::from_le_bytes(le::<4>(&p[12..16])), 0);
    }

    #[test]
    fn npc_text_query_is_text_then_guid() {
        let p = npc_text_query(42, 0x1000);
        assert_eq!(u32::from_le_bytes(le::<4>(&p[..4])), 42);
        assert_eq!(u64::from_le_bytes(le::<8>(&p[4..])), 0x1000);
    }

    #[test]
    fn attack_start_carries_two_full_guids() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x11u64.to_le_bytes());
        b.extend_from_slice(&0x22u64.to_le_bytes());
        assert_eq!(
            parse(crate::opcode::SMSG_ATTACKSTART, &b).unwrap().unwrap(),
            ServerEvent::AttackStart { attacker: 0x11, victim: 0x22 }
        );
    }

    #[test]
    fn attack_stop_uses_packed_guids() {
        let mut b = pack_guid(0x11);
        b.extend_from_slice(&pack_guid(0x22));
        b.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            parse(crate::opcode::SMSG_ATTACKSTOP, &b).unwrap().unwrap(),
            ServerEvent::AttackStop { attacker: 0x11, victim: 0x22 }
        );
    }

    #[test]
    fn loot_response_reads_slot_type_after_the_suffix_words() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x11u64.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&37u32.to_le_bytes());
        b.push(1);
        b.push(0);
        b.extend_from_slice(&[0x1d, 0x0a, 0, 0]);
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&1117u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.push(0);
        let ev = parse(crate::opcode::SMSG_LOOT_RESPONSE, &b).unwrap().unwrap();
        assert_eq!(
            ev,
            ServerEvent::LootResponse {
                guid: 0x11,
                loot_type: 1,
                gold: 37,
                items: vec![LootItem {
                    slot: 0,
                    item_id: 2589,
                    count: 2,
                    display_info_id: 1117,
                    random_property_id: 0,
                    slot_type: 0,
                }],
            }
        );
    }

    #[test]
    fn name_query_response_starts_with_a_packed_guid() {
        let mut b = pack_guid(0x44);
        b.push(0);
        b.extend_from_slice(b"Tester\0");
        b.extend_from_slice(b"Realm\0");
        b.extend_from_slice(&[1, 0, 4]);
        b.push(0);
        assert_eq!(
            parse(crate::opcode::SMSG_NAME_QUERY_RESPONSE, &b).unwrap().unwrap(),
            ServerEvent::PlayerName {
                guid: 0x44,
                name: "Tester".into(),
                race: 1,
                gender: 0,
                class: 4,
            }
        );
    }
}
