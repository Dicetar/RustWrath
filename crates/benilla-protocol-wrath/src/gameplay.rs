//! The first ordinary-gameplay wire slice: the 3.3.5 client messages the 1.12 action/UI owners
//! already produce, and the server replies those owners already understand.
//!
//! Layouts are from the local AzerothCore build 12340 sources named beside each entry. Nothing
//! here owns game state or time; the app maps these typed messages onto its existing
//! `ClientCommand`/`SessionEvent` seam.

#![allow(dead_code)]

use std::fmt;

use anyhow::Result;

use crate::cast_failure;

mod movement;
pub use movement::{MonsterFacing, MonsterMove};

/// The largest undecoded-byte count reported with a typed decode failure.
///
/// The count is a length only: no packet body, server text, path, credential or GUID is
/// carried by [`DecodeError`] or by the diagnostic value the app records.
const MAX_REPORTED_REMAINING: usize = 4096;

/// Closed decoder families the Wrath gameplay parser can name for one rejected opcode.
///
/// The app records this value instead of a decoder function name or an error message, so a
/// rejected packet can be attributed without exporting source or server text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DecoderFamily {
    QuestList,
    QuestGreeting,
    QuestDetails,
    QuestProgress,
    QuestRequestItems,
    QuestOffer,
    QuestOfferReward,
    QuestInvalid,
    QuestGiverInvalid,
    QuestFailed,
    QuestGiverFailed,
    QuestLogFull,
    QuestTemplate,
    Gossip,
    GossipMenu,
    Loot,
    LootResponse,
    Vendor,
    VendorList,
    Inventory,
    ItemPush,
    InventoryPush,
    Spell,
    Other,
}

/// Closed coarse reason for a rejected gameplay decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DecodeFailure {
    /// The packet ended before a required field.
    Truncated,
    /// A record parsed but bytes remained after its last field.
    TrailingBytes,
    /// A NUL-terminated string had no terminating NUL before the packet ended.
    UnterminatedString,
    /// A declared count is outside the layout's fixed bound.
    InvalidCount,
    /// A field value violates the packet's own contract.
    InvalidValue,
    /// The decoder failed but this source cannot classify why.
    Other,
}

/// A typed gameplay decode failure. The variant is closed; `detail` stays a static local
/// description and never carries packet bytes or server text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodeError {
    reason: DecodeFailure,
    /// Bounded count of bytes not consumed by the decoder, when it is meaningful.
    remaining: Option<u32>,
    detail: &'static str,
    /// An optional numeric field value that made the packet invalid (never text).
    value: Option<u32>,
}

impl DecodeError {
    pub(crate) fn new(reason: DecodeFailure, remaining: Option<usize>, detail: &'static str) -> Self {
        Self {
            reason,
            remaining: remaining
                .map(|count| count.min(MAX_REPORTED_REMAINING) as u32),
            detail,
            value: None,
        }
    }

    pub(crate) fn with_value(mut self, value: u32) -> Self {
        self.value = Some(value);
        self
    }

    pub(crate) fn truncated(remaining: usize) -> Self {
        Self::new(DecodeFailure::Truncated, Some(remaining), "payload cut short")
    }

    pub(crate) fn unterminated(remaining: usize) -> Self {
        Self::new(DecodeFailure::UnterminatedString, Some(remaining), "unterminated string")
    }

    pub(crate) fn trailing(remaining: usize) -> Self {
        Self::new(DecodeFailure::TrailingBytes, Some(remaining), "trailing bytes after record")
    }

    pub(crate) fn invalid_count(detail: &'static str) -> Self {
        Self::new(DecodeFailure::InvalidCount, None, detail)
    }

    pub(crate) fn invalid_value(detail: &'static str) -> Self {
        Self::new(DecodeFailure::InvalidValue, None, detail)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            Some(value) => write!(f, "{}: {value}", self.detail),
            None => f.write_str(self.detail),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Classify one gameplay decode failure without exposing its free-form message.
///
/// A failure the reader did not type returns [`DecodeFailure::Other`] rather than a guess.
pub fn classify_decode_failure(error: &anyhow::Error) -> DecodeFailure {
    error
        .downcast_ref::<DecodeError>()
        .map(|failure| failure.reason)
        .unwrap_or(DecodeFailure::Other)
}

/// Classify a reducer failure by walking its error chain for a typed [`DecodeError`].
///
/// This never inspects free-form error text: it only downcasts typed failures. A reducer
/// error that is untyped or has no typed cause returns [`DecodeFailure::Other`].
pub fn classify_reduce_failure(error: &anyhow::Error) -> DecodeFailure {
    for cause in error.chain() {
        if let Some(failure) = cause.downcast_ref::<DecodeError>() {
            return failure.reason;
        }
    }
    classify_decode_failure(error)
}

/// The bounded undecoded-byte count at the failure point, when the decoder reported one.
///
/// This is only a length. It never identifies or carries the bytes themselves.
pub fn decode_remaining_bytes(error: &anyhow::Error) -> Option<u32> {
    error
        .downcast_ref::<DecodeError>()
        .and_then(|failure| failure.remaining)
}

/// The closed decoder family that owns one opcode the gameplay parser translates.
///
/// An opcode outside the families below stays [`DecoderFamily::Other`]; this is a classification
/// boundary, not a claim that the opcode has no decoder.
pub fn decoder_family(opcode: u16) -> DecoderFamily {
    use crate::opcode::*;
    match opcode {
        SMSG_QUESTGIVER_QUEST_LIST => DecoderFamily::QuestGreeting,
        SMSG_QUESTGIVER_QUEST_DETAILS => DecoderFamily::QuestDetails,
        SMSG_QUESTGIVER_REQUEST_ITEMS => DecoderFamily::QuestProgress,
        SMSG_QUESTGIVER_OFFER_REWARD => DecoderFamily::QuestOffer,
        SMSG_QUESTGIVER_QUEST_INVALID => DecoderFamily::QuestGiverInvalid,
        SMSG_QUESTGIVER_QUEST_FAILED => DecoderFamily::QuestGiverFailed,
        SMSG_QUESTLOG_FULL => DecoderFamily::QuestLogFull,
        SMSG_QUEST_QUERY_RESPONSE => DecoderFamily::QuestTemplate,
        SMSG_GOSSIP_MESSAGE | SMSG_GOSSIP_COMPLETE => DecoderFamily::Gossip,
        SMSG_LOOT_RESPONSE
        | SMSG_LOOT_RELEASE_RESPONSE
        | SMSG_LOOT_REMOVED
        | SMSG_LOOT_MONEY_NOTIFY
        | SMSG_LOOT_CLEAR_MONEY
        | SMSG_LOOT_START_ROLL
        | SMSG_LOOT_ROLL
        | SMSG_LOOT_ROLL_WON
        | SMSG_LOOT_ALL_PASSED => DecoderFamily::Loot,
        SMSG_LIST_INVENTORY | SMSG_BUY_ITEM | SMSG_BUY_FAILED | SMSG_SELL_ITEM => {
            DecoderFamily::Vendor
        }
        SMSG_INVENTORY_CHANGE_FAILURE => DecoderFamily::Inventory,
        SMSG_ITEM_PUSH_RESULT => DecoderFamily::ItemPush,
        SMSG_CAST_FAILED
        | SMSG_SPELL_START
        | SMSG_SPELL_GO
        | SMSG_SPELL_FAILED_OTHER
        | SMSG_SPELL_FAILURE
        | SMSG_SPELL_DELAYED
        | SMSG_SPELL_COOLDOWN
        | MSG_CHANNEL_START
        | MSG_CHANNEL_UPDATE => DecoderFamily::Spell,
        _ => DecoderFamily::Other,
    }
}

/// AzerothCore MiscHandler.cpp HandleRealmSplitOpcode; 12340 CGlueMgr::OnRealmSplitMsg
/// (0x004D8200) forwards these three fields as SERVER_SPLIT_NOTICE.
pub fn parse_realm_split(payload: &[u8]) -> Result<(u32, u32, String)> {
    let mut r = Reader::new(payload);
    let reply = (r.u32()?, r.u32()?, r.cstring()?);
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(reply)
}

#[test]
fn realm_split_reply_preserves_server_state_and_rejects_truncation() {
    let mut payload = u32::MAX.to_le_bytes().to_vec();
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(b"01/01/01\0");
    assert_eq!(parse_realm_split(&payload).unwrap(), (u32::MAX, 0, "01/01/01".into()));
    for end in 0..payload.len() {
        assert!(parse_realm_split(&payload[..end]).is_err());
    }
    payload.push(0);
    assert!(parse_realm_split(&payload).is_err());
}

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
/// `SpellCastTargets::Read` (`Spell.cpp:127`) as a 32-bit mask, packed object/item GUIDs,
/// source/destination transport plus position, or a string.
pub fn cast_spell(cast_count: u8, spell_id: u32, target: CastTarget) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, target)
}

fn cast_spell_with_target(cast_count: u8, spell_id: u32, target: CastTarget) -> Vec<u8> {
    let mut p = Vec::with_capacity(24);
    p.push(cast_count);
    p.extend_from_slice(&spell_id.to_le_bytes());
    p.push(0); // cast flags: no ammo/missile payload from a stock item/spell send
    write_cast_target(&mut p, target);
    p
}

/// `CMSG_CAST_SPELL` at a game object (`CastSpellGameObject`): `TARGET_FLAG_GAMEOBJECT`.
pub fn cast_spell_gameobject(cast_count: u8, spell_id: u32, go_guid: u64) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, CastTarget::GameObject(go_guid))
}

/// `CMSG_CAST_SPELL` at an item (`CastSpellItem`): `TARGET_FLAG_ITEM`.
pub fn cast_spell_item(cast_count: u8, spell_id: u32, item_guid: u64) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, CastTarget::Item(item_guid))
}

/// `CMSG_CAST_SPELL` at the partner's trade slot (`CastSpellTradeItem`): 12340 treats the
/// trade-item guid field as the slot value, with only `TRADE_SLOT_NONTRADED` (6) accepted by
/// `SpellCastTargets::Update` (`Spell.cpp:487-493`).
pub fn cast_spell_trade_item(cast_count: u8, spell_id: u32, slot: u8) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, CastTarget::TradeItem(slot))
}

/// `CMSG_CAST_SPELL` at a corpse (`CastSpellCorpse`): the ally or enemy corpse mask.
pub fn cast_spell_corpse(
    cast_count: u8,
    spell_id: u32,
    ally: bool,
    corpse_guid: u64,
) -> Vec<u8> {
    let target = if ally {
        CastTarget::CorpseAlly(corpse_guid)
    } else {
        CastTarget::CorpseEnemy(corpse_guid)
    };
    cast_spell_with_target(cast_count, spell_id, target)
}

/// `CMSG_CAST_SPELL` at a destination point (`CastSpellAtDest`): `TARGET_FLAG_DEST_LOCATION`.
pub fn cast_spell_at_dest(cast_count: u8, spell_id: u32, dest: [f32; 3]) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, CastTarget::Dest(dest))
}

/// `CMSG_CAST_SPELL` at a source point (`CastSpellAtSource`): `TARGET_FLAG_SOURCE_LOCATION`.
pub fn cast_spell_at_source(cast_count: u8, spell_id: u32, src: [f32; 3]) -> Vec<u8> {
    cast_spell_with_target(cast_count, spell_id, CastTarget::Source(src))
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

/// `CMSG_LOOT_ROLL` (`GroupHandler.cpp:494-499`): the roll's raw `u64` item guid, the item slot
/// word and the vote byte (`0` pass, `1` need, `2` greed, `3` disenchant).
pub fn loot_roll(guid: u64, item_slot: u32, roll_type: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(13);
    p.extend_from_slice(&guid.to_le_bytes());
    p.extend_from_slice(&item_slot.to_le_bytes());
    p.push(roll_type);
    p
}

/// `CMSG_LIST_INVENTORY` (`ItemHandler.cpp:844`): the vendor guid.
pub fn list_inventory(vendor: u64) -> Vec<u8> {
    vendor.to_le_bytes().to_vec()
}

/// `CMSG_BUY_ITEM` (`ItemPackets.cpp:94`): guid, item entry, the 1-based vendor row, u32 count,
/// and the trailing byte the stock client sends as 1. 12340 addresses the row, not the entry.
pub fn buy_item(vendor: u64, entry: u32, slot: u32, count: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(21);
    p.extend_from_slice(&vendor.to_le_bytes());
    p.extend_from_slice(&entry.to_le_bytes());
    p.extend_from_slice(&slot.to_le_bytes());
    p.extend_from_slice(&count.to_le_bytes());
    p.push(1);
    p
}

/// `CMSG_BUY_ITEM_IN_SLOT` (`ItemPackets.cpp:84`): guid, entry, 1-based row, bag guid, bag slot,
/// u32 count.
pub fn buy_item_in_slot(
    vendor: u64,
    entry: u32,
    slot: u32,
    bag_guid: u64,
    bag_slot: u8,
    count: u32,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(29);
    p.extend_from_slice(&vendor.to_le_bytes());
    p.extend_from_slice(&entry.to_le_bytes());
    p.extend_from_slice(&slot.to_le_bytes());
    p.extend_from_slice(&bag_guid.to_le_bytes());
    p.push(bag_slot);
    p.extend_from_slice(&count.to_le_bytes());
    p
}

/// `CMSG_SELL_ITEM` (`ItemPackets.cpp:71`): guid, item guid, u32 count (0 sells the stack).
pub fn sell_item(vendor: u64, item_guid: u64, count: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(20);
    p.extend_from_slice(&vendor.to_le_bytes());
    p.extend_from_slice(&item_guid.to_le_bytes());
    p.extend_from_slice(&count.to_le_bytes());
    p
}

/// `CMSG_BUYBACK_ITEM` (`ItemPackets.cpp:78`): guid and the absolute buyback slot.
pub fn buyback_item(vendor: u64, slot: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&vendor.to_le_bytes());
    p.extend_from_slice(&slot.to_le_bytes());
    p
}

/// `CMSG_REPAIR_ITEM` (`NPCHandler.cpp:752`): guid, item guid (0 = all), u8 guild-bank money.
pub fn repair_item(vendor: u64, item_guid: u64, guild_bank: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(17);
    p.extend_from_slice(&vendor.to_le_bytes());
    p.extend_from_slice(&item_guid.to_le_bytes());
    p.push(u8::from(guild_bank));
    p
}

/// A 12340 `SpellCastTargets` block for an item use. Coordinates are world-space (transport 0),
/// matching the stock target block; trade-item targets need the real item guid and are not
/// constructed here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastTarget {
    None,
    Unit(u64),
    GameObject(u64),
    Item(u64),
    TradeItem(u8),
    Source([f32; 3]),
    Dest([f32; 3]),
    CorpseAlly(u64),
    CorpseEnemy(u64),
}

fn write_point(p: &mut Vec<u8>, point: [f32; 3]) {
    // A zero transport guid, then the world position (`SpellCastTargets::Read`, Spell.cpp:140).
    p.push(0);
    for value in point {
        p.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_cast_target(p: &mut Vec<u8>, target: CastTarget) {
    let (mask, guid) = match target {
        CastTarget::None => {
            p.extend_from_slice(&0u32.to_le_bytes());
            return;
        }
        CastTarget::Unit(guid) => (TARGET_FLAG_UNIT, guid),
        CastTarget::GameObject(guid) => (TARGET_FLAG_GAMEOBJECT, guid),
        CastTarget::Item(guid) => (TARGET_FLAG_ITEM, guid),
        CastTarget::TradeItem(slot) => (TARGET_FLAG_TRADE_ITEM, u64::from(slot)),
        CastTarget::CorpseAlly(guid) => (TARGET_FLAG_CORPSE_ALLY, guid),
        CastTarget::CorpseEnemy(guid) => (TARGET_FLAG_CORPSE_ENEMY, guid),
        CastTarget::Source(point) => {
            p.extend_from_slice(&TARGET_FLAG_SOURCE_LOCATION.to_le_bytes());
            write_point(p, point);
            return;
        }
        CastTarget::Dest(point) => {
            p.extend_from_slice(&TARGET_FLAG_DEST_LOCATION.to_le_bytes());
            write_point(p, point);
            return;
        }
    };
    p.extend_from_slice(&mask.to_le_bytes());
    p.extend_from_slice(&pack_guid(guid));
}

/// `CMSG_USE_ITEM` (`SpellHandler.cpp:58-73`, `Spell.cpp:127-178`): bag, slot, cast count, spell
/// id, the used item's guid, glyph index (0; 1.12 has none), cast flags (0; no ammo payload),
/// then the 12340 targets block. The server verifies the guid against the bag/slot item.
pub fn use_item(
    bag: u8,
    slot: u8,
    cast_count: u8,
    spell_id: u32,
    item_guid: u64,
    target: CastTarget,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(32);
    p.push(bag);
    p.push(slot);
    p.push(cast_count);
    p.extend_from_slice(&spell_id.to_le_bytes());
    p.extend_from_slice(&item_guid.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes()); // glyph index
    p.push(0); // cast flags
    write_cast_target(&mut p, target);
    p
}

/// `CMSG_OPEN_ITEM` (`ItemHandler.cpp`): bag and slot.
pub fn open_item(bag: u8, slot: u8) -> Vec<u8> {
    vec![bag, slot]
}

/// `CMSG_AUTOEQUIP_ITEM` (`ItemPackets.cpp:49`): source bag and slot.
pub fn auto_equip_item(bag: u8, slot: u8) -> Vec<u8> {
    vec![bag, slot]
}

/// `CMSG_SWAP_INV_ITEM` (`ItemPackets.cpp:29`): destination then source player-array slot.
pub fn swap_inv_item(dst_slot: u8, src_slot: u8) -> Vec<u8> {
    vec![dst_slot, src_slot]
}

/// `CMSG_SWAP_ITEM` (`ItemPackets.cpp:41`): destination bag/slot first, then source.
pub fn swap_item(dst_bag: u8, dst_slot: u8, src_bag: u8, src_slot: u8) -> Vec<u8> {
    vec![dst_bag, dst_slot, src_bag, src_slot]
}

/// `CMSG_AUTOSTORE_BAG_ITEM` (`ItemPackets.cpp:108`): source bag/slot, destination bag.
pub fn auto_store_bag_item(src_bag: u8, src_slot: u8, dst_bag: u8) -> Vec<u8> {
    vec![src_bag, src_slot, dst_bag]
}

/// `CMSG_SPLIT_ITEM` (`ItemPackets.cpp:20`): source bag/slot, destination bag/slot, u32 count.
pub fn split_item(src_bag: u8, src_slot: u8, dst_bag: u8, dst_slot: u8, count: u32) -> Vec<u8> {
    let mut p = vec![src_bag, src_slot, dst_bag, dst_slot];
    p.extend_from_slice(&count.to_le_bytes());
    p
}

/// `CMSG_DESTROYITEM` (`ItemPackets.cpp:55`): bag, slot, count and three unused bytes.
pub fn destroy_item(bag: u8, slot: u8, count: u8) -> Vec<u8> {
    vec![bag, slot, count, 0, 0, 0]
}

/// `CMSG_SET_AMMO` (`ItemHandler.cpp:1016`): the ammo template entry.
pub fn set_ammo(entry: u32) -> Vec<u8> {
    entry.to_le_bytes().to_vec()
}

/// `CMSG_MESSAGECHAT` (`ChatHandler.cpp:59`): u32 type, u32 language, then the whisper/channel
/// target as a cstring when present, then the message as a cstring. The client's own language
/// is native (Common 7 or Orcish 1); AFK/DND accept universal 0.
pub fn message_chat(chat_type: u32, language: u32, target: Option<&str>, text: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(12 + target.map_or(0, str::len) + text.len());
    p.extend_from_slice(&chat_type.to_le_bytes());
    p.extend_from_slice(&language.to_le_bytes());
    if let Some(target) = target {
        p.extend_from_slice(target.as_bytes());
        p.push(0);
    }
    p.extend_from_slice(text.as_bytes());
    p.push(0);
    p
}

/// `CMSG_BANKER_ACTIVATE` (`BankHandler.cpp:44`): the banker guid.
pub fn banker_activate(banker: u64) -> Vec<u8> {
    banker.to_le_bytes().to_vec()
}

/// `CMSG_BUY_BANK_SLOT` (`BankPackets.cpp:36`): the banker guid; the server buys the next slot.
pub fn buy_bank_slot(banker: u64) -> Vec<u8> {
    banker.to_le_bytes().to_vec()
}

/// `CMSG_AUTOBANK_ITEM` (`BankPackets.cpp:20`): source bag and slot in the bank block.
pub fn autobank_item(bag: u8, slot: u8) -> Vec<u8> {
    vec![bag, slot]
}

/// `CMSG_AUTOSTORE_BANK_ITEM` (`BankPackets.cpp:26`): source bag and slot; the server decides
/// whether the position is in the bank or the bags, so the same verb moves both ways.
pub fn autostore_bank_item(bag: u8, slot: u8) -> Vec<u8> {
    vec![bag, slot]
}

/// `CMSG_AREATRIGGER` (`MiscHandler.cpp:700`): the AreaTrigger.dbc id.
pub fn area_trigger(trigger_id: u32) -> Vec<u8> {
    trigger_id.to_le_bytes().to_vec()
}

/// `CMSG_QUESTLOG_REMOVE_QUEST` (`QuestHandler.cpp:400`): the log slot byte.
pub fn questlog_remove(slot: u8) -> Vec<u8> {
    vec![slot]
}

/// `CMSG_QUEST_CONFIRM_ACCEPT` (`QuestHandler.cpp:453`): the escort quest id.
pub fn quest_confirm_accept(quest: u32) -> Vec<u8> {
    quest.to_le_bytes().to_vec()
}

/// `CMSG_PUSHQUESTTOPARTY` (`QuestHandler.cpp:540`): the quest id.
pub fn push_quest_to_party(quest: u32) -> Vec<u8> {
    quest.to_le_bytes().to_vec()
}

/// `MSG_QUEST_PUSH_RESULT` (`QuestHandler.cpp:620-635`): the sharer guid, the quest id and the
/// one-byte verdict. 12340 requires the quest id the 1.12 command does not carry.
pub fn quest_push_result(sharer: u64, quest: u32, msg: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(13);
    p.extend_from_slice(&sharer.to_le_bytes());
    p.extend_from_slice(&quest.to_le_bytes());
    p.push(msg);
    p
}

/// `CMSG_BINDER_ACTIVATE` (`NPCHandler.cpp:286`): the binder guid.
pub fn binder_activate(binder: u64) -> Vec<u8> {
    binder.to_le_bytes().to_vec()
}

/// `CMSG_SUMMON_RESPONSE` (`MovementHandler.cpp:851`): summoner guid and the accept byte.
pub fn summon_response(summoner: u64, accept: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.extend_from_slice(&summoner.to_le_bytes());
    p.push(u8::from(accept));
    p
}

/// `MSG_TALENT_WIPE_CONFIRM` (`SkillHandler.cpp:58`): the trainer guid (accept only).
pub fn talent_wipe_confirm(trainer: u64) -> Vec<u8> {
    trainer.to_le_bytes().to_vec()
}

/// `CMSG_PLAYED_TIME` (`CharacterPackets.cpp:37`): one `TriggerScriptEvent` byte.
pub fn played_time() -> Vec<u8> {
    vec![0]
}

/// `CMSG_QUERY_TIME` (`QueryHandler.cpp:72`): empty.
pub fn query_time() -> Vec<u8> {
    Vec::new()
}

/// `MSG_RANDOM_ROLL` (`MiscPackets.cpp:85`): minimum then maximum.
pub fn random_roll(min: u32, max: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(8);
    p.extend_from_slice(&min.to_le_bytes());
    p.extend_from_slice(&max.to_le_bytes());
    p
}

/// `CMSG_ITEM_TEXT_QUERY` (`ItemHandler.cpp:1463`): the item guid. This is the 3.3.5 inscribed
/// item text, not the 1.12 mail-text `(text_id, mail_id)` form.
pub fn item_text_query(item_guid: u64) -> Vec<u8> {
    item_guid.to_le_bytes().to_vec()
}

/// `CMSG_PAGE_TEXT_QUERY` (`QueryHandler.cpp:361`): page id then the queried guid.
pub fn page_text_query(page_id: u32, guid: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&page_id.to_le_bytes());
    p.extend_from_slice(&guid.to_le_bytes());
    p
}

/// `CMSG_STANDSTATECHANGE` (`MiscHandler.cpp:569`): u32 stand state.
pub fn stand_state_change(state: u32) -> Vec<u8> {
    state.to_le_bytes().to_vec()
}

/// `CMSG_SET_SHEATHED` (`CombatHandler.cpp:73`): u32 sheath state.
pub fn set_sheathed(state: u32) -> Vec<u8> {
    state.to_le_bytes().to_vec()
}

/// `CMSG_MOUNTSPECIAL_ANIM` (`MovementHandler.cpp:803`): empty.
pub fn mount_special() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_SET_ACTIONBAR_TOGGLES` (`MiscHandler.cpp:961`): one byte of bar toggles.
pub fn set_action_bar_toggles(toggles: u8) -> Vec<u8> {
    vec![toggles]
}

/// `CMSG_CANCEL_CAST` (`SpellHandler.cpp:552`): an ignored counter byte, then the spell id.
pub fn cancel_cast(spell_id: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(5);
    p.push(0);
    p.extend_from_slice(&spell_id.to_le_bytes());
    p
}

/// `CMSG_CANCEL_CHANNELLING` (`SpellHandler.cpp:651`): the spell id (the server reads it).
pub fn cancel_channelling(spell_id: u32) -> Vec<u8> {
    spell_id.to_le_bytes().to_vec()
}

/// `CMSG_CANCEL_AURA` (`SpellHandler.cpp:566`): the aura's spell id.
pub fn cancel_aura(spell_id: u32) -> Vec<u8> {
    spell_id.to_le_bytes().to_vec()
}

/// `CMSG_CANCEL_AUTO_REPEAT_SPELL` (`SpellHandler.cpp:644`): empty.
pub fn cancel_auto_repeat() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_TRAINER_LIST` (`NPCHandler.cpp:81`): the `Hello` packet's unit guid.
pub fn trainer_list(trainer: u64) -> Vec<u8> {
    trainer.to_le_bytes().to_vec()
}

/// `CMSG_TRAINER_BUY_SPELL` (`NPCPackets.cpp:48`): trainer guid then the service spell.
pub fn trainer_buy_spell(trainer: u64, spell_id: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&trainer.to_le_bytes());
    p.extend_from_slice(&spell_id.to_le_bytes());
    p
}

/// `CMSG_REPOP_REQUEST` (`MiscHandler.cpp:58-86`): the 12340 handler reads one byte.
pub fn repop_request() -> Vec<u8> {
    vec![0]
}

/// `CMSG_RESURRECT_RESPONSE` (`MiscHandler.cpp:677-698`): raw caster GUID and the accept byte.
pub fn resurrect_response(caster: u64, accept: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.extend_from_slice(&caster.to_le_bytes());
    p.push(u8::from(accept));
    p
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
    pub flags: u32,
    pub repeatable: bool,
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QuestPanelExtra {
    pub flags: u32,
    pub suggested_players: u32,
    pub xp: u32,
    pub honor_tenths: u32,
    pub honor_multiplier: f32,
    pub cast_spell: i32,
    pub title_id: u32,
    pub talents: u32,
    pub arena_points: u32,
    pub reputation: [(u32, i32, u32); 5],
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestDetails {
    pub extra: QuestPanelExtra,
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

#[derive(Clone, Debug, PartialEq)]
pub struct QuestOfferReward {
    pub extra: QuestPanelExtra,
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
    pub stats: Vec<(u32, i32)>,
    pub damages: Vec<ItemDamage>,
    pub dmg_min: f32,
    pub dmg_max: f32,
    pub dmg_type: u32,
    pub armor: u32,
    pub resistances: [i32; 6],
    pub delay_ms: u32,
    pub ammo_type: u32,
    pub ranged_mod_range: f32,
    pub spells: Vec<ItemSpell>,
    pub spell_charges_0: i32,
    pub use_spell: Option<ItemUseSpell>,
    pub bonding: u32,
    pub description: String,
    pub page_text: u32,
    pub language_id: u32,
    pub page_material: u32,
    pub start_quest: u32,
    pub lock_id: u32,
    pub material: u32,
    pub sheath: u32,
    pub random_property: u32,
    pub block: u32,
    pub item_set: u32,
    pub max_durability: u32,
    pub area: u32,
    pub map: u32,
    pub bag_family: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemDamage {
    pub min: f32,
    pub max: f32,
    pub school: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSpell {
    pub index: u8,
    pub spell_id: u32,
    pub trigger: u32,
    pub charges: i32,
    pub cooldown_ms: i32,
    pub category: u32,
    pub category_cooldown_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemUseSpell {
    pub spell_id: u32,
    pub cooldown_ms: i32,
    pub category: u32,
    pub category_cooldown_ms: i32,
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
    pub suggested_players: u32,
    pub xp_difficulty: u32,
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
    /// 12340's `RewardHonor`/`RewardHonorMultiplier`, the honor line the log and reward panels
    /// read; 0 for a quest without one.
    pub honor_reward: u32,
    pub honor_multiplier: f32,
    /// 12340's title, talent and arena-point rewards; 0 when the quest grants none.
    pub title_reward: u32,
    pub bonus_talents: u32,
    pub bonus_arena_points: u32,
    /// Three 12340 arrays of five: faction id, amount and the override; a zero faction is a
    /// row the client does not show.
    pub rep_rewards: [(u32, i32, u32); 5],
    pub rewards: [(u32, u32); 4],
    pub choices: [(u32, u32); 6],
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
    /// `randomSuffix`, the rolled suffix factor. 12340's loot response carries the real value;
    /// item links must preserve it separately from [`Self::random_property_id`].
    pub suffix_factor: u32,
    pub slot_type: u8,
}

/// One 12340 `SMSG_MESSAGECHAT`/`SMSG_GM_MESSAGECHAT` line (`Chat.cpp:374-443`). `chat_type`
/// stays in the 12340 `ChatMsg` numbering; the app maps it to the 1.12 owner's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub chat_type: u8,
    pub language: u32,
    pub sender_guid: u64,
    pub target_guid: u64,
    pub sender_name: Option<String>,
    pub channel: Option<String>,
    pub text: String,
    pub chat_tag: u8,
    pub achievement_id: Option<u32>,
}

/// One 12340 `SMSG_LIST_INVENTORY` row (`ItemHandler.cpp:941`): eight words, the last the
/// extended cost the 1.12 merchant owner cannot present. `slot` is the 1-based row the buy
/// packets address; `current_count` `0xFFFF_FFFF` is unlimited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorRow {
    pub slot: u32,
    pub entry: u32,
    pub display_id: u32,
    pub current_count: u32,
    pub price: u32,
    pub max_durability: u32,
    pub buy_count: u32,
    pub extended_cost: u32,
}

/// One 12340 `SMSG_TRAINER_LIST` row (`NPCPackets.cpp:31-41`): `point_cost[0]` is the talent
/// cost (always zero for trainer services) and `point_cost[1]` is the primary-profession
/// first-rank flag the 1.12 owner splits into its two booleans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainerRow {
    pub spell_id: u32,
    pub state: u8,
    pub cost: u32,
    pub point_cost: [u32; 2],
    pub req_level: u8,
    pub req_skill_line: u32,
    pub req_skill_rank: u32,
    pub req_ability: [u32; 3],
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

/// One `SpellCastTargets` as 12340 writes it (`SpellCastTargets::Write`, `Spell.cpp:180-215`).
/// The object guid is one of UNIT/MINIPET, GAMEOBJECT or a corpse by the mask's priority; the
/// item guid is the spell's target item (not the cast item in the packet's first slot). A
/// source/dest point is transport-relative when its transport guid is non-zero and then has no
/// world position: the point is not fabricated, the transport is kept for diagnosis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpellTargets {
    pub mask: u32,
    pub unit: Option<u64>,
    pub gameobject: Option<u64>,
    pub item: Option<u64>,
    pub source: Option<[f32; 3]>,
    pub source_transport: Option<u64>,
    pub dest: Option<[f32; 3]>,
    pub dest_transport: Option<u64>,
    pub string: Option<String>,
}

/// One active cooldown in `SMSG_INITIAL_SPELLS`, exactly as the pinned writer sends it
/// (`Player::SendInitialSpells`, `Player.cpp:2730`): a `u32` spell id, `u16` cast item
/// entry, `u16` spell category and the two remaining times. The category word's bit 31 is the
/// held/infinity mark and its low bits are the remaining time, matching the 1.12
/// `ReadInitialSpells` convention (`0x5e6510`); a category spell carries `0` in `spell_cd_ms`,
/// a plain spell carries `0` in `category_cd_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialSpellCooldown {
    pub spell_id: u32,
    pub item_id: u16,
    pub category: u16,
    pub spell_cd_ms: u32,
    pub category_cd_ms: u32,
}

/// One typed reply this slice understands.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    MonsterMove(MonsterMove),
    AttackStart { attacker: u64, victim: u64 },
    AttackStop { attacker: u64, victim: u64 },
    AttackSwingError(AttackSwingError),
    CastFailed {
        spell_id: u32,
        reason: cast_failure::Reason,
        arg: Option<u32>,
        /// 12340's leading cast counter; the native lifecycle matches terminal edges on it.
        cast_count: u8,
        /// The raw `SpellCastResult` code, before the 1.12 reason mapping.
        raw_reason: u8,
    },
    SpellStart {
        caster: u64,
        spell_id: u32,
        cast_flags: u32,
        /// 12340's `m_cast_count`: the client's cast identity for this attempt.
        cast_count: u8,
        /// The cast item guid when the packet's first guid is not the caster.
        cast_item: Option<u64>,
        /// 12340's `m_timer`: the time left when the packet was written, not the full cast time.
        cast_time_ms: i32,
        targets: SpellTargets,
        ammo_display_id: Option<u32>,
    },
    SpellGo {
        caster: u64,
        spell_id: u32,
        cast_flags: u32,
        /// 12340's `m_cast_count`, matching the START that opened this attempt.
        cast_count: u8,
        /// 12340's server game-time timestamp on GO, in its own clock domain.
        server_time_ms: u32,
        hits: Vec<u64>,
        misses: Vec<(u64, u8, u8)>,
        targets: SpellTargets,
        ammo_display_id: Option<u32>,
        item_caster: Option<u64>,
    },
    /// `SMSG_INITIAL_SPELLS`: the learned spell ids and the active cooldown tail as the pinned
    /// writer sends it. `declared_cooldowns` preserves the writer's declared count, which can
    /// exceed the entries actually written because it counts map entries it filters as not
    /// client-visible; `cooldowns` is exactly the complete entries present in the packet.
    InitialSpells {
        spells: Vec<u32>,
        cooldowns: Vec<InitialSpellCooldown>,
        declared_cooldowns: u16,
    },
    /// `SMSG_SPELL_COOLDOWN`: `(spell_id, cooldown_ms)` pairs after the caster and 12340's flag
    /// byte. 0 ms means the spell's own DBC recovery.
    SpellCooldowns {
        caster: u64,
        cooldowns: Vec<(u32, u32)>,
    },
    ItemCooldown { item_guid: u64, spell_id: u32 },
    CooldownEvent { spell_id: u32, caster: u64 },
    ClearCooldown { spell_id: u32, caster: u64 },
    /// `MSG_CHANNEL_START`: 12340 broadcasts the caster's guid, which the self-only 1.12 event
    /// does not carry.
    ChannelStart {
        caster: u64,
        spell_id: u32,
        duration_ms: u32,
    },
    /// `MSG_CHANNEL_UPDATE`: as above; 0 ms is over.
    ChannelUpdate { caster: u64, remaining_ms: u32 },
    /// `SMSG_SPELL_DELAYED`: pushback on a cast in flight.
    SpellDelayed { caster: u64, delay_ms: u32 },
    SpellFailure {
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        raw_reason: u8,
    },
    /// `SMSG_ATTACKERSTATEUPDATE` mapped onto the 1.12 swing shape.
    AttackerState {
        attacker: u64,
        victim: u64,
        hit_info: u32,
        damage: u32,
        victim_state: u32,
        absorb: u32,
        resist: i32,
        blocked: u32,
        melee_spell_id: u32,
        school: u32,
    },
    CancelCombat,
    AiReaction { unit: u64, reaction: u32 },
    CancelAutoRepeat,
    PlaySpellVisual { unit: u64, kit_id: u32 },
    /// `SMSG_SPELLNONMELEEDAMAGELOG` without the 12340-only overkill word.
    SpellDamage {
        target: u64,
        attacker: u64,
        spell_id: u32,
        damage: u32,
        school: u8,
        absorb: u32,
        resist: i32,
        blocked: u32,
        hit_info: u32,
    },
    /// `SMSG_PERIODICAURALOG`'s counted tick list (12340 writes one; the shape is a list as in
    /// 1.12).
    PeriodicAura {
        target: u64,
        caster: u64,
        spell_id: u32,
        ticks: Vec<PeriodicAuraTick>,
    },
    /// `MSG_CORPSE_QUERY`'s answer: the corpse's adjusted display map/position and real map.
    CorpseQuery {
        found: bool,
        display_map: i32,
        position: [f32; 3],
        corpse_map: u32,
    },
    CorpseReclaimDelay { delay_ms: u32 },
    ResurrectRequest {
        caster: u64,
        name: String,
        sickness: bool,
        /// False when 12340's instant-res override word was present.
        has_timer: bool,
    },
    SpiritHealerConfirm { npc: u64 },
    LogoutResponse { reason: u32, instant: bool },
    LoggedOut,
    LogoutCancelled,
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
        racial_leader: bool,
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
    /// `SMSG_LOOT_START_ROLL` (`Group.cpp::SendLootStartRoll`, `:860-881`): the roll's allocated
    /// item guid, 3.3.3's map id, slot, item, suffix factor, property, stack count, countdown and
    /// the per-player vote mask.
    LootStartRoll {
        guid: u64,
        map_id: u32,
        item_slot: u32,
        item_id: u32,
        suffix_factor: u32,
        random_property_id: u32,
        item_count: u32,
        countdown_ms: u32,
        vote_mask: u8,
    },
    /// `SMSG_LOOT_ROLL` (`Group.cpp::SendLootRoll`, `:906-928`): a vote or dice result, plus
    /// 12340's automatic-pass byte.
    LootRoll {
        guid: u64,
        item_slot: u32,
        roller: u64,
        item_id: u32,
        suffix_factor: u32,
        random_property_id: u32,
        roll_number: u8,
        roll_type: u8,
        auto_pass: bool,
    },
    /// `SMSG_LOOT_ROLL_WON` (`Group.cpp::SendLootRollWon`, `:930-951`): the winner and roll,
    /// with the item fields ahead of the winner guid.
    LootRollWon {
        guid: u64,
        item_slot: u32,
        item_id: u32,
        suffix_factor: u32,
        random_property_id: u32,
        winner: u64,
        roll_number: u8,
        roll_type: u8,
    },
    /// `SMSG_LOOT_ALL_PASSED` (`Group.cpp::SendLootAllPassed`, `:953-959`): property before
    /// suffix, the reverse of the other three packets.
    LootAllPassed {
        guid: u64,
        item_slot: u32,
        item_id: u32,
        random_property_id: u32,
        suffix_factor: u32,
    },
    ItemPush(ItemPush),
    VendorInventory {
        vendor: u64,
        items: Vec<VendorRow>,
    },
    VendorBuyResult {
        vendor: u64,
        slot: u32,
        new_count: u32,
        purchase_count: u32,
    },
    VendorBuyFailed {
        vendor: u64,
        item_entry: u32,
        reason: u8,
        /// 12340 writes this word only when non-zero (`PlayerStorage.cpp:4156`).
        param: Option<u32>,
    },
    VendorSellFailed {
        vendor: u64,
        item_guid: u64,
        reason: u8,
        /// 12340 writes this word only when non-zero (`PlayerStorage.cpp:4168`).
        param: Option<u32>,
    },
    TrainerList {
        trainer: u64,
        trainer_type: u32,
        services: Vec<TrainerRow>,
        greeting: String,
    },
    TrainerBuySucceeded {
        trainer: u64,
        spell_id: u32,
    },
    TrainerBuyFailed {
        trainer: u64,
        spell_id: u32,
        error: u32,
    },
    Chat(ChatLine),
    ChatPlayerNotFound {
        name: String,
    },
    Notification {
        text: String,
    },
    AreaTriggerMessage {
        text: String,
    },
    QuestConfirmAccept {
        quest_id: u32,
        title: String,
        sender: u64,
    },
    QuestPushResult {
        member: u64,
        msg: u8,
    },
    BinderConfirm {
        binder: u64,
    },
    SummonRequest {
        summoner: u64,
        zone: u32,
        delay_ms: u32,
    },
    TalentWipeConfirm {
        trainer: u64,
        cost: u32,
    },
    PlayedTime {
        total: u32,
        level: u32,
        /// 12340's third byte; the 1.12 owner has no script-event trigger.
        trigger: u8,
    },
    ServerUnixTime {
        unix_time: u32,
        /// 12340's `TimeResponse`: seconds until the next daily reset, a distinct native fact.
        reset_in_secs: u32,
    },
    RandomRoll {
        min: u32,
        max: u32,
        roll: u32,
        guid: u64,
    },
    PageText {
        page_id: u32,
        text: String,
        next_page_id: u32,
    },
    /// `SMSG_INVENTORY_CHANGE_FAILURE`: the raw error, the two item guids and the error-specific
    /// tail. The shared 1.12 owner consumes only `reason`; the native contract keeps the rest.
    InventoryFailure {
        reason: u8,
        item: Option<u64>,
        item2: Option<u64>,
        bag_type: u8,
        required_level: Option<u32>,
        item_limit_category: Option<u32>,
    },
    /// `SMSG_ITEM_TEXT_QUERY_RESPONSE` (`ItemHandler.cpp:1463-1483`): `has_text == false` means
    /// the one-byte absent form and no text.
    ItemText {
        has_text: bool,
        item_guid: u64,
        text: String,
    },
    ShowBank {
        banker: u64,
    },
    /// `ERR_BANKSLOT_*` (`ItemDefines`/`bank_slot_result`); 12340 sends OK too, unlike 1.12.
    BuyBankSlotResult {
        result: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackSwingError {
    NotInRange,
    BadFacing,
    DeadOrUnattackable,
}

/// One periodic aura tick, shaped by its aura type (`Unit::SendPeriodicAuraLog`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PeriodicAuraTick {
    Damage {
        amount: u32,
        school: u32,
        absorb: u32,
        resist: i32,
    },
    Heal {
        amount: u32,
    },
    Energize {
        power: u32,
        amount: u32,
    },
    ManaLeech {
        power: u32,
        amount: u32,
        multiplier: f32,
    },
}

/// Decode one server opcode into the app-facing shape: `Ok(None)` for an opcode this slice does
/// not consume, the codec's error for a malformed body. The session's direct movement arm reads
/// this wrapper; [`parse`] remains the raw `Option<Result>` form its other callers pattern on.
pub fn decode_server(opcode: u16, body: &[u8]) -> Result<Option<ServerEvent>> {
    match parse(opcode, body) {
        Some(Ok(event)) => Ok(Some(event)),
        Some(Err(error)) => Err(error),
        None => Ok(None),
    }
}

/// Parse one server opcode; `None` means this slice does not translate that opcode.
pub fn parse(opcode: u16, body: &[u8]) -> Option<Result<ServerEvent>> {
    use crate::opcode::*;
    Some(match opcode {
        SMSG_MONSTER_MOVE => movement::read(body, false).map(ServerEvent::MonsterMove),
        SMSG_MONSTER_MOVE_TRANSPORT => movement::read(body, true).map(ServerEvent::MonsterMove),
        SMSG_ATTACKSTART => read_attack_start(body),
        SMSG_ATTACKSTOP => read_attack_stop(body),
        SMSG_ATTACKSWING_NOTINRANGE => Ok(ServerEvent::AttackSwingError(AttackSwingError::NotInRange)),
        SMSG_ATTACKSWING_BADFACING => Ok(ServerEvent::AttackSwingError(AttackSwingError::BadFacing)),
        SMSG_ATTACKSWING_DEADTARGET | SMSG_ATTACKSWING_CANT_ATTACK => {
            Ok(ServerEvent::AttackSwingError(AttackSwingError::DeadOrUnattackable))
        }
        SMSG_AI_REACTION => read_ai_reaction(body),
        SMSG_ATTACKERSTATEUPDATE => read_attacker_state(body),
        SMSG_CANCEL_COMBAT => Ok(ServerEvent::CancelCombat),
        SMSG_CANCEL_AUTO_REPEAT => read_cancel_auto_repeat(body),
        SMSG_PLAY_SPELL_VISUAL => read_play_spell_visual(body),
        SMSG_PERIODICAURALOG => read_periodic_aura(body),
        SMSG_SPELLNONMELEEDAMAGELOG => read_spell_damage(body),
        MSG_CORPSE_QUERY => read_corpse_query(body),
        SMSG_CORPSE_RECLAIM_DELAY => read_corpse_reclaim_delay(body),
        SMSG_RESURRECT_REQUEST => read_resurrect_request(body),
        SMSG_SPIRIT_HEALER_CONFIRM => read_spirit_healer_confirm(body),
        SMSG_CAST_FAILED => read_cast_failed(body),
        SMSG_SPELL_START => read_spell_start(body),
        SMSG_SPELL_GO => read_spell_go(body),
        // 12340 `Spell::SendInterrupted` broadcasts both 0x133 and 0x2A6 with the same body; the
        // seam has one observed-interruption edge, so only the 1.12-named one is translated.
        SMSG_SPELL_FAILED_OTHER => read_spell_failure(body),
        SMSG_SPELL_DELAYED => read_spell_delayed(body),
        SMSG_SPELL_COOLDOWN => read_spell_cooldowns(body),
        SMSG_COOLDOWN_EVENT => read_cooldown_event(body).map(|(spell_id, caster)| {
            ServerEvent::CooldownEvent { spell_id, caster }
        }),
        SMSG_CLEAR_COOLDOWN => read_cooldown_event(body).map(|(spell_id, caster)| {
            ServerEvent::ClearCooldown { spell_id, caster }
        }),
        SMSG_ITEM_COOLDOWN => read_item_cooldown(body),
        MSG_CHANNEL_START => read_channel_start(body),
        MSG_CHANNEL_UPDATE => read_channel_update(body),
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
        SMSG_LOOT_REMOVED => body
            .first()
            .copied()
            .ok_or_else(|| anyhow::Error::from(DecodeError::truncated(0)))
            .map(|slot| ServerEvent::LootRemoved { slot }),
        SMSG_LOOT_MONEY_NOTIFY => u32_body(body).map(|amount| ServerEvent::LootMoney { amount }),
        SMSG_LOOT_CLEAR_MONEY => Ok(ServerEvent::LootClearMoney),
        SMSG_LOOT_START_ROLL => read_loot_start_roll(body),
        SMSG_LOOT_ROLL => read_loot_roll(body),
        SMSG_LOOT_ROLL_WON => read_loot_roll_won(body),
        SMSG_LOOT_ALL_PASSED => read_loot_all_passed(body),
        SMSG_ITEM_PUSH_RESULT => read_item_push(body),
        SMSG_LIST_INVENTORY => read_list_inventory(body),
        SMSG_BUY_ITEM => read_buy_item(body),
        SMSG_BUY_FAILED => read_buy_failed(body),
        SMSG_SELL_ITEM => read_sell_failed(body),
        SMSG_TRAINER_LIST => read_trainer_list(body),
        SMSG_TRAINER_BUY_SUCCEEDED => read_trainer_buy_succeeded(body),
        SMSG_TRAINER_BUY_FAILED => read_trainer_buy_failed(body),
        SMSG_MESSAGECHAT => read_message_chat(body, false),
        SMSG_GM_MESSAGECHAT => read_message_chat(body, true),
        SMSG_CHAT_PLAYER_NOT_FOUND => Reader::new(body).cstring().map(|name| ServerEvent::ChatPlayerNotFound { name }),
        SMSG_NOTIFICATION => Reader::new(body).cstring().map(|text| ServerEvent::Notification { text }),
        SMSG_AREA_TRIGGER_MESSAGE => read_area_trigger_message(body),
        SMSG_QUEST_CONFIRM_ACCEPT => read_quest_confirm_accept(body),
        MSG_QUEST_PUSH_RESULT => read_quest_push_result(body),
        SMSG_BINDER_CONFIRM => read_binder_confirm(body),
        SMSG_SUMMON_REQUEST => read_summon_request(body),
        MSG_TALENT_WIPE_CONFIRM => read_talent_wipe_confirm(body),
        SMSG_PLAYED_TIME => read_played_time(body),
        SMSG_QUERY_TIME_RESPONSE => read_server_time(body),
        MSG_RANDOM_ROLL => read_random_roll(body),
        SMSG_PAGE_TEXT_QUERY_RESPONSE => read_page_text(body),
        SMSG_INVENTORY_CHANGE_FAILURE => read_inventory_failure(body),
        SMSG_ITEM_TEXT_QUERY_RESPONSE => read_item_text(body),
        SMSG_SHOW_BANK => read_show_bank(body),
        SMSG_BUY_BANK_SLOT_RESULT => u32_body(body).map(|result| ServerEvent::BuyBankSlotResult {
            result,
        }),
        _ => return None,
    })
}

fn read_list_inventory(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let vendor = r.u64()?;
    let count = r.u8()?;
    let mut items = Vec::with_capacity(count as usize);
    if count == 0 {
        // `ItemHandler.cpp:958-961`: an empty list appends one more zero byte.
        let _no_inventory = r.u8()?;
        return Ok(ServerEvent::VendorInventory { vendor, items });
    }
    for _ in 0..count {
        items.push(VendorRow {
            slot: r.u32()?,
            entry: r.u32()?,
            display_id: r.u32()?,
            current_count: r.u32()?,
            price: r.u32()?,
            max_durability: r.u32()?,
            buy_count: r.u32()?,
            extended_cost: r.u32()?,
        });
    }
    Ok(ServerEvent::VendorInventory { vendor, items })
}

fn read_buy_item(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::VendorBuyResult {
        vendor: r.u64()?,
        slot: r.u32()?,
        new_count: r.u32()?,
        purchase_count: r.u32()?,
    })
}

fn read_buy_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let vendor = r.u64()?;
    let item_entry = r.u32()?;
    let param = if r.remaining() >= 5 { Some(r.u32()?) } else { None };
    Ok(ServerEvent::VendorBuyFailed {
        vendor,
        item_entry,
        reason: r.u8()?,
        param,
    })
}

fn read_sell_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let vendor = r.u64()?;
    let item_guid = r.u64()?;
    let param = if r.remaining() >= 5 { Some(r.u32()?) } else { None };
    Ok(ServerEvent::VendorSellFailed {
        vendor,
        item_guid,
        reason: r.u8()?,
        param,
    })
}

fn read_trainer_list(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let trainer = r.u64()?;
    let trainer_type = r.u32()?;
    let count = r.u32()?;
    let mut services = Vec::with_capacity((count as usize).min(1024));
    for _ in 0..count {
        services.push(TrainerRow {
            spell_id: r.u32()?,
            state: r.u8()?,
            cost: r.u32()?,
            point_cost: [r.u32()?, r.u32()?],
            req_level: r.u8()?,
            req_skill_line: r.u32()?,
            req_skill_rank: r.u32()?,
            req_ability: [r.u32()?, r.u32()?, r.u32()?],
        });
    }
    Ok(ServerEvent::TrainerList {
        trainer,
        trainer_type,
        services,
        greeting: r.cstring()?,
    })
}

fn read_trainer_buy_succeeded(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::TrainerBuySucceeded {
        trainer: r.u64()?,
        spell_id: r.u32()?,
    })
}

/// 12340 `ObjectGuid::GetHigh()` is bits 48-63; players have high `0x0000`, pets `0xF140`.
fn guid_high(guid: u64) -> u16 {
    ((guid >> 48) & 0xffff) as u16
}

fn read_message_chat(b: &[u8], gm: bool) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let chat_type = r.u8()?;
    let language = r.u32()?;
    let sender_guid = r.u64()?;
    let _unknown = r.u32()?; // 2.1.0
    let mut sender_name = None;
    let mut target_guid = 0u64;
    let mut channel = None;
    match chat_type {
        0x08 | 0x0C..=0x10 | 0x29 | 0x2A | 0x2F => {
            // `senderName` is length-prefixed (NUL included), then the unit target.
            let _name_len = r.u32()?;
            sender_name = Some(r.cstring()?);
            target_guid = r.u64()?;
            let target_high = guid_high(target_guid);
            if chat_type != 0x08
                && target_guid != 0
                && target_high != 0x0000
                && target_high != 0xF140
            {
                let _target_len = r.u32()?;
                let _target_name = r.cstring()?;
            }
        }
        0x24..=0x26 => {
            target_guid = r.u64()?;
            if target_guid != 0 && guid_high(target_guid) != 0x0000 {
                let _target_len = r.u32()?;
                let _target_name = r.cstring()?;
            }
        }
        0x30 | 0x31 => {
            target_guid = r.u64()?;
        }
        _ => {
            if gm {
                let _name_len = r.u32()?;
                sender_name = Some(r.cstring()?);
            }
            if chat_type == 0x11 {
                channel = Some(r.cstring()?);
            }
            target_guid = r.u64()?;
        }
    }
    let _text_len = r.u32()?;
    let text = r.cstring()?;
    let chat_tag = r.u8()?;
    let achievement_id = matches!(chat_type, 0x30 | 0x31).then(|| r.u32()).transpose()?;
    Ok(ServerEvent::Chat(ChatLine {
        chat_type,
        language,
        sender_guid,
        target_guid,
        sender_name,
        channel,
        text,
        chat_tag,
        achievement_id,
    }))
}

fn read_quest_confirm_accept(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::QuestConfirmAccept {
        quest_id: r.u32()?,
        title: r.cstring()?,
        sender: r.u64()?,
    })
}

fn read_quest_push_result(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::QuestPushResult {
        member: r.u64()?,
        msg: r.u8()?,
    })
}

/// `SMSG_AREA_TRIGGER_MESSAGE` (`WorldSession.cpp:268`): `u32 length` including the terminating
/// NUL, then the line. The length is authoritative; a mismatch, embedded NUL or trailing byte is
/// a malformed packet, not an empty message.
fn read_area_trigger_message(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let length = r.u32()? as usize;
    if length == 0 {
        return Err(DecodeError::invalid_value("area trigger message length is zero").into());
    }
    if length > r.remaining() {
        return Err(DecodeError::truncated(r.remaining()).into());
    }
    let bytes = r.take(length)?;
    if bytes.last() != Some(&0) {
        return Err(DecodeError::unterminated(length).into());
    }
    if bytes[..length - 1].contains(&0) {
        return Err(DecodeError::invalid_value(
            "area trigger message contains an embedded NUL",
        )
        .into());
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(ServerEvent::AreaTriggerMessage {
        text: String::from_utf8_lossy(&bytes[..length - 1]).into_owned(),
    })
}

fn read_binder_confirm(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::BinderConfirm {
        binder: r.u64()?,
    })
}

fn read_summon_request(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::SummonRequest {
        summoner: r.u64()?,
        zone: r.u32()?,
        delay_ms: r.u32()?,
    })
}

fn read_talent_wipe_confirm(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::TalentWipeConfirm {
        trainer: r.u64()?,
        cost: r.u32()?,
    })
}

fn read_played_time(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::PlayedTime {
        total: r.u32()?,
        level: r.u32()?,
        trigger: r.u8()?,
    })
}

fn read_server_time(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::ServerUnixTime {
        unix_time: r.u32()?,
        reset_in_secs: r.u32()?,
    })
}

fn read_random_roll(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::RandomRoll {
        min: r.u32()?,
        max: r.u32()?,
        roll: r.u32()?,
        guid: r.u64()?,
    })
}

fn read_page_text(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::PageText {
        page_id: r.u32()?,
        text: r.cstring()?,
        next_page_id: r.u32()?,
    })
}

/// `SMSG_INVENTORY_CHANGE_FAILURE` (`PlayerStorage.cpp:4113-4154`): `u8 reason`; when non-zero,
/// two raw item guids, a bag-type byte, then the reason-specific tail (level, autoequip triple or
/// item-limit category). `EQUIP_ERR_OK` writes only the reason byte.
fn read_inventory_failure(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let reason = r.u8()?;
    if reason == 0 {
        return Ok(ServerEvent::InventoryFailure {
            reason,
            item: None,
            item2: None,
            bag_type: 0,
            required_level: None,
            item_limit_category: None,
        });
    }
    let item = r.u64()?;
    let item2 = r.u64()?;
    let bag_type = r.u8()?;
    let mut required_level = None;
    let mut item_limit_category = None;
    match reason {
        // EQUIP_ERR_CANT_EQUIP_LEVEL_I / EQUIP_ERR_PURCHASE_LEVEL_TOO_LOW.
        1 | 87 => required_level = Some(r.u32()?),
        // EQUIP_ERR_EVENT_AUTOEQUIP_BIND_CONFIRM: item guid, slot, container guid.
        81 => {
            let _item = r.u64()?;
            let _slot = r.u32()?;
            let _container = r.u64()?;
        }
        // EQUIP_ERR_ITEM_MAX_LIMIT_CATEGORY_{COUNT,SOCKETED,EQUIPPED}_EXCEEDED.
        84 | 85 | 89 => item_limit_category = Some(r.u32()?),
        _ => {}
    }
    Ok(ServerEvent::InventoryFailure {
        reason,
        item: Some(item).filter(|value| *value != 0),
        item2: Some(item2).filter(|value| *value != 0),
        bag_type,
        required_level,
        item_limit_category,
    })
}

/// `SMSG_ITEM_TEXT_QUERY_RESPONSE` (`ItemHandler.cpp:1470-1481`): `u8 has_text`; 0 means the
/// item guid and text follow, 1 means the item has no text and nothing follows.
fn read_item_text(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let flag = r.u8()?;
    if flag != 0 {
        return Ok(ServerEvent::ItemText {
            has_text: false,
            item_guid: 0,
            text: String::new(),
        });
    }
    Ok(ServerEvent::ItemText {
        has_text: true,
        item_guid: r.u64()?,
        text: r.cstring()?,
    })
}

fn read_show_bank(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::ShowBank {
        banker: r.u64()?,
    })
}

fn read_trainer_buy_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::TrainerBuyFailed {
        trainer: r.u64()?,
        spell_id: r.u32()?,
        error: r.u32()?,
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

// `HitInfo` bits whose conditional tails `SendAttackStateUpdate` writes (Unit.h:97-122).
const HITINFO_FULL_ABSORB: u32 = 0x0000_0020;
const HITINFO_PARTIAL_ABSORB: u32 = 0x0000_0040;
const HITINFO_FULL_RESIST: u32 = 0x0000_0080;
const HITINFO_PARTIAL_RESIST: u32 = 0x0000_0100;
const HITINFO_BLOCK: u32 = 0x0000_2000;
const HITINFO_RAGE_GAIN: u32 = 0x0080_0000;

/// `SMSG_ATTACKERSTATEUPDATE` (`Unit.cpp:6870-6960`): 12340 writes `Overkill` after the total,
/// keeps absorb/resist in blocks gated by `HitInfo` rather than inside every sub-block, and sends
/// `TargetState` as a byte. The `HITINFO_UNK1` debug tail is left unread; nothing re-parses the
/// payload, and guessing its width would be fabrication.
fn read_attacker_state(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let hit_info = r.u32()?;
    let attacker = r.packed_guid()?;
    let victim = r.packed_guid()?;
    let damage = r.u32()?;
    let _overkill = r.u32()?;
    let subs = r.u8()?;
    let mut absorb = 0u32;
    let mut resist = 0i32;
    let mut school = 0u32;
    for i in 0..subs {
        let block_school = r.u32()?;
        if i == 0 {
            school = block_school;
        }
        let _damage_f = r.f32()?;
        let _damage = r.u32()?;
    }
    if hit_info & (HITINFO_FULL_ABSORB | HITINFO_PARTIAL_ABSORB) != 0 {
        for _ in 0..subs {
            absorb += r.u32()?;
        }
    }
    if hit_info & (HITINFO_FULL_RESIST | HITINFO_PARTIAL_RESIST) != 0 {
        for _ in 0..subs {
            resist += r.u32()? as i32;
        }
    }
    let victim_state = u32::from(r.u8()?);
    let _attacker_state = r.u32()?;
    let melee_spell_id = r.u32()?;
    let blocked = if hit_info & HITINFO_BLOCK != 0 {
        r.u32()?
    } else {
        0
    };
    if hit_info & HITINFO_RAGE_GAIN != 0 {
        let _rage = r.u32()?;
    }
    Ok(ServerEvent::AttackerState {
        attacker,
        victim,
        hit_info,
        damage,
        victim_state,
        absorb,
        resist,
        blocked,
        melee_spell_id,
        school,
    })
}

/// `SMSG_AI_REACTION` (`Unit.cpp:12995-12998`): raw `u64` guid and `u32` reaction.
fn read_ai_reaction(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let unit = r.u64()?;
    let reaction = r.u32()?;
    Ok(ServerEvent::AiReaction { unit, reaction })
}

/// `SMSG_CANCEL_AUTO_REPEAT` (`PlayerMisc.cpp:153-158`): a packed target guid the 1.12 event does
/// not carry.
fn read_cancel_auto_repeat(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let _target = r.packed_guid()?;
    Ok(ServerEvent::CancelAutoRepeat)
}

/// `SMSG_PLAY_SPELL_VISUAL` (`Unit.cpp:15149-15162`): raw `u64` guid and `SpellVisualKit` id.
fn read_play_spell_visual(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let unit = r.u64()?;
    let kit_id = r.u32()?;
    Ok(ServerEvent::PlaySpellVisual { unit, kit_id })
}

/// `SMSG_SPELLNONMELEEDAMAGELOG` (`Unit.cpp:6679-6724`): the 12340 body carries an `Overkill`
/// word the 1.12 seam does not, and its `physicalLog` byte is not the seam's periodic flag.
fn read_spell_damage(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let target = r.packed_guid()?;
    let attacker = r.packed_guid()?;
    let spell_id = r.u32()?;
    let damage = r.u32()?;
    let _overkill = r.u32()?;
    let school = r.u8()?;
    let absorb = r.u32()?;
    let resist = r.u32()? as i32;
    let _physical_log = r.u8()?;
    let _unused = r.u8()?;
    let blocked = r.u32()?;
    let hit_info = r.u32()?;
    let _debug = r.u8()?;
    Ok(ServerEvent::SpellDamage {
        target,
        attacker,
        spell_id,
        damage,
        school,
        absorb,
        resist,
        blocked,
        hit_info,
    })
}

/// `SMSG_PERIODICAURALOG` (`Unit.cpp:6780-6842`): packed target and caster, spell, count, then one
/// type-shaped block per tick. An unknown aura type is an error: its width is unknowable, exactly
/// as the 1.12 reader treats it.
fn read_periodic_aura(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let target = r.packed_guid()?;
    let caster = r.packed_guid()?;
    let spell_id = r.u32()?;
    let count = r.u32()?;
    let mut ticks = Vec::with_capacity(count.min(64) as usize);
    for _ in 0..count {
        let aura_type = r.u32()?;
        let tick = match aura_type {
            3 | 89 => {
                let amount = r.u32()?;
                let _overkill = r.u32()?;
                let school = r.u32()?;
                let absorb = r.u32()?;
                let resist = r.u32()? as i32;
                let _critical = r.u8()?;
                PeriodicAuraTick::Damage {
                    amount,
                    school,
                    absorb,
                    resist,
                }
            }
            8 | 20 => {
                let amount = r.u32()?;
                let _overheal = r.u32()?;
                let _absorb = r.u32()?;
                let _critical = r.u8()?;
                PeriodicAuraTick::Heal { amount }
            }
            21 | 24 => PeriodicAuraTick::Energize {
                power: r.u32()?,
                amount: r.u32()?,
            },
            64 => PeriodicAuraTick::ManaLeech {
                power: r.u32()?,
                amount: r.u32()?,
                multiplier: r.f32()?,
            },
            other => {
                return Err(DecodeError::invalid_value(
                    "SMSG_PERIODICAURALOG: unknown aura type",
                )
                .with_value(other)
                .into())
            }
        };
        ticks.push(tick);
    }
    Ok(ServerEvent::PeriodicAura {
        target,
        caster,
        spell_id,
        ticks,
    })
}

fn read_cast_failed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let wrath_reason = r.u8()?;
    // `WriteCastResultInfo` (`Spell.cpp:4521-4632`): one argument word for these reasons.
    // NEED_MORE_ITEMS and MIN_SKILL write two and only the first is the seam's `arg`; the
    // equipped-item reasons write a class word then a subclass mask, and only the class is the
    // seam's. The totem reasons write zero, one or two words by the spell's DBC totals, which
    // the wire cannot tell, so no word is read for them. Any unread word stays in the packet.
    let arg = match wrath_reason {
        29..=31 | 54 | 55 | 100 | 101 | 102 | 129 | 147 | 150 | 172 => {
            (r.remaining() >= 4).then(|| r.u32()).transpose()?
        }
        _ => None,
    };
    Ok(ServerEvent::CastFailed {
        spell_id,
        reason: cast_failure::reason(wrath_reason),
        arg,
        cast_count,
        raw_reason: wrath_reason,
    })
}

// --- Spell targets (`SpellCastTargets::Write`, `Spell.cpp:180-215`) ----------------------------

const TARGET_FLAG_UNIT: u32 = 0x0000_0002;
const TARGET_FLAG_ITEM: u32 = 0x0000_0010;
const TARGET_FLAG_SOURCE_LOCATION: u32 = 0x0000_0020;
const TARGET_FLAG_DEST_LOCATION: u32 = 0x0000_0040;
const TARGET_FLAG_CORPSE_ENEMY: u32 = 0x0000_0200;
const TARGET_FLAG_GAMEOBJECT: u32 = 0x0000_0800;
const TARGET_FLAG_TRADE_ITEM: u32 = 0x0000_1000;
const TARGET_FLAG_STRING: u32 = 0x0000_2000;
const TARGET_FLAG_CORPSE_ALLY: u32 = 0x0000_8000;
const TARGET_FLAG_UNIT_MINIPET: u32 = 0x0001_0000;

/// The flags whose writer test writes the one packed object guid (`SpellCastTargets::Write`).
const TARGET_FLAG_OBJECT_GUID: u32 = TARGET_FLAG_UNIT
    | TARGET_FLAG_UNIT_MINIPET
    | TARGET_FLAG_GAMEOBJECT
    | TARGET_FLAG_CORPSE_ENEMY
    | TARGET_FLAG_CORPSE_ALLY;

fn read_spell_targets(r: &mut Reader<'_>) -> Result<SpellTargets> {
    let mask = r.u32()?;
    let mut unit = None;
    let mut gameobject = None;
    if mask & TARGET_FLAG_OBJECT_GUID != 0 {
        let guid = r.packed_guid()?;
        if mask & (TARGET_FLAG_UNIT | TARGET_FLAG_UNIT_MINIPET) != 0 {
            unit = Some(guid);
        } else if mask & TARGET_FLAG_GAMEOBJECT != 0 {
            gameobject = Some(guid);
        }
    }
    let item = if mask & (TARGET_FLAG_ITEM | TARGET_FLAG_TRADE_ITEM) != 0 {
        Some(r.packed_guid()?)
    } else {
        None
    };
    let (source, source_transport) =
        read_spell_target_point(r, mask & TARGET_FLAG_SOURCE_LOCATION != 0)?;
    let (dest, dest_transport) = read_spell_target_point(r, mask & TARGET_FLAG_DEST_LOCATION != 0)?;
    let string = if mask & TARGET_FLAG_STRING != 0 {
        Some(r.cstring()?)
    } else {
        None
    };
    Ok(SpellTargets {
        mask,
        unit,
        gameobject,
        item,
        source,
        source_transport,
        dest,
        dest_transport,
        string,
    })
}

/// A source/dest block: the packed transport guid then three floats, read as a world point only
/// when the transport is empty (`SpellCastTargets::Write`).
fn read_spell_target_point(
    r: &mut Reader<'_>,
    present: bool,
) -> Result<(Option<[f32; 3]>, Option<u64>)> {
    if !present {
        return Ok((None, None));
    }
    let transport = r.packed_guid()?;
    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    if transport == 0 {
        Ok((Some([x, y, z]), None))
    } else {
        Ok((None, Some(transport)))
    }
}

// --- Cast-flag payloads (`SpellCastFlags`, Spell.h:55-84) --------------------------------------

const CAST_FLAG_PROJECTILE: u32 = 0x0000_0020;
const CAST_FLAG_POWER_LEFT_SELF: u32 = 0x0000_0800;
const CAST_FLAG_ADJUST_MISSILE: u32 = 0x0002_0000;
const CAST_FLAG_VISUAL_CHAIN: u32 = 0x0008_0000;
const CAST_FLAG_RUNE_LIST: u32 = 0x0020_0000;
const CAST_FLAG_UNKNOWN_23: u32 = 0x0040_0000;

/// `WriteAmmoToPacket` (`Spell.cpp:4880-4960`): display id then inventory type, the latter dropped.
fn read_ammo(r: &mut Reader<'_>) -> Result<u32> {
    let display_id = r.u32()?;
    let _inventory_type = r.u32()?;
    Ok(display_id)
}

/// START's payloads in `SendSpellStart`'s order (`Spell.cpp:4731-4741`): power, ammo, the
/// UNKNOWN_23 pair. The power is consumed, not carried: the 1.12 start event has no power.
fn read_spell_start_payloads(r: &mut Reader<'_>, cast_flags: u32) -> Result<Option<u32>> {
    if cast_flags & CAST_FLAG_POWER_LEFT_SELF != 0 {
        let _power = r.u32()?;
    }
    let ammo = if cast_flags & CAST_FLAG_PROJECTILE != 0 {
        Some(read_ammo(r)?)
    } else {
        None
    };
    if cast_flags & CAST_FLAG_UNKNOWN_23 != 0 {
        let _a = r.u32()?;
        let _b = r.u32()?;
    }
    Ok(ammo)
}

/// GO's payloads in `SendSpellGo`'s order (`Spell.cpp:4832-4875`): power, the rune list, the
/// missile adjustment, ammo, the visual-chain pair and the destination marker. The rune writer
/// emits one byte per rune usable before the cast and cooling after, which the two masks count.
fn read_spell_go_payloads(
    r: &mut Reader<'_>,
    cast_flags: u32,
    target_mask: u32,
) -> Result<Option<u32>> {
    if cast_flags & CAST_FLAG_POWER_LEFT_SELF != 0 {
        let _power = r.u32()?;
    }
    if cast_flags & CAST_FLAG_RUNE_LIST != 0 {
        let initial = r.u8()?;
        let after = r.u8()?;
        for _ in 0..(initial & !after).count_ones() {
            let _rune_cooldown = r.u8()?;
        }
    }
    if cast_flags & CAST_FLAG_ADJUST_MISSILE != 0 {
        let _elevation = r.f32()?;
        let _delay = r.u32()?;
    }
    let ammo = if cast_flags & CAST_FLAG_PROJECTILE != 0 {
        Some(read_ammo(r)?)
    } else {
        None
    };
    if cast_flags & CAST_FLAG_VISUAL_CHAIN != 0 {
        let _a = r.u32()?;
        let _b = r.u32()?;
    }
    if target_mask & TARGET_FLAG_DEST_LOCATION != 0 {
        let _dest_marker = r.u8()?;
    }
    Ok(ammo)
}

fn read_spell_start(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let item_or_caster = r.packed_guid()?;
    let caster = r.packed_guid()?;
    let cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let cast_flags = r.u32()?;
    let cast_time_ms = r.i32()?;
    let targets = read_spell_targets(&mut r)?;
    let ammo_display_id = read_spell_start_payloads(&mut r, cast_flags)?;
    Ok(ServerEvent::SpellStart {
        caster,
        spell_id,
        cast_flags,
        cast_count,
        cast_item: (item_or_caster != caster).then_some(item_or_caster),
        cast_time_ms,
        targets,
        ammo_display_id,
    })
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
            racial_leader: false,
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
    let _display_id_2 = r.u32()?;
    let _display_id_3 = r.u32()?;
    let _display_id_4 = r.u32()?;
    let _mod_health = r.f32()?;
    let _mod_mana = r.f32()?;
    let racial_leader = r.u8()? != 0;
    Ok(ServerEvent::CreatureName {
        entry,
        name: Some(name),
        subname: Some(subname).filter(|s| !s.is_empty()),
        creature_type,
        pet_family,
        rank,
        type_flags,
        display_id,
        racial_leader,
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
    // 3.3.5 writes a variable stats block, then the fixed damage/resistance/spell blocks. The
    // 1.12 template keeps the same semantics with shorter fixed arrays; read each by its own
    // count rather than copying the whole 3.3.5 record.
    let stats_count = r.u32()?;
    // Build 12340 ItemStats_C has ten stat slots (AzerothCore MAX_ITEM_PROTO_STATS).
    // Clamping the count would reinterpret excess stat bytes as the following fields.
    if stats_count > 10 {
        return Err(DecodeError::invalid_count("item stat count exceeds 12340 limit 10").into());
    }
    let mut stats = Vec::with_capacity(stats_count as usize);
    for _ in 0..stats_count {
        let stat_type = r.u32()?;
        let stat_value = r.i32()?;
        if stat_type != 0 || stat_value != 0 {
            stats.push((stat_type, stat_value));
        }
    }
    let _scaling_distribution = r.u32()?;
    let _scaling_value = r.u32()?;

    // 12340's `MAX_ITEM_PROTO_DAMAGES` is 2 since 3.1.0 (`ItemTemplate.h:613`); the 1.12
    // template carries five, so reading five here would desync every following field.
    let mut damages = Vec::with_capacity(2);
    let mut dmg_min = 0.0;
    let mut dmg_max = 0.0;
    let mut dmg_type = 0;
    for i in 0..2 {
        let min = r.f32()?;
        let max = r.f32()?;
        let school = r.u32()?;
        if i == 0 {
            dmg_min = min;
            dmg_max = max;
            dmg_type = school;
        }
        if max > 0.0 {
            damages.push(ItemDamage { min, max, school });
        }
    }

    let armor = r.u32()?;
    let resistances = [
        r.i32()?,
        r.i32()?,
        r.i32()?,
        r.i32()?,
        r.i32()?,
        r.i32()?,
    ];
    let delay_ms = r.u32()?;
    let ammo_type = r.u32()?;
    let ranged_mod_range = r.f32()?;

    let mut spells = Vec::with_capacity(5);
    let mut spell_charges_0 = 0;
    let mut use_spell = None;
    for index in 0..5u8 {
        let spell_id = r.u32()?;
        let trigger = r.u32()?;
        let charges = r.i32()?;
        let cooldown_ms = r.i32()?;
        let category = r.u32()?;
        let category_cooldown_ms = r.i32()?;
        if index == 0 {
            spell_charges_0 = charges;
        }
        if spell_id != 0 {
            let spell = ItemSpell {
                index,
                spell_id,
                trigger,
                charges,
                cooldown_ms,
                category,
                category_cooldown_ms,
            };
            if trigger == 0 && use_spell.is_none() {
                use_spell = Some(ItemUseSpell {
                    spell_id,
                    cooldown_ms,
                    category,
                    category_cooldown_ms,
                });
            }
            spells.push(spell);
        }
    }

    let bonding = r.u32()?;
    let description = r.cstring()?;
    let page_text = r.u32()?;
    let language_id = r.u32()?;
    let page_material = r.u32()?;
    let start_quest = r.u32()?;
    let lock_id = r.u32()?;
    let material = r.i32()?.max(0) as u32;
    let sheath = r.u32()?;
    let random_property = r.u32()?;
    let _random_suffix = r.u32()?;
    let block = r.u32()?;
    let item_set = r.u32()?;
    let max_durability = r.u32()?;
    let area = r.u32()?;
    let map = r.u32()?;
    let bag_family = r.u32()?;

    Ok(ServerEvent::ItemInfo {
        entry,
        info: Some(ItemInfo {
            class, subclass, name, display_info_id, quality, flags, buy_price, sell_price,
            inventory_type, allowable_class, allowable_race, item_level, required_level,
            required_skill, required_skill_rank, required_spell, required_honor_rank,
            required_city_rank, required_rep_faction, required_rep_rank, max_count, stackable,
            container_slots, stats, damages, dmg_min, dmg_max, dmg_type, armor, resistances,
            delay_ms, ammo_type, ranged_mod_range, spells, spell_charges_0, use_spell, bonding,
            description, page_text, language_id, page_material, start_quest, lock_id, material,
            sheath, random_property, block, item_set, max_durability, area, map, bag_family,
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
    let cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let raw_reason = r.u8()?;
    // 12340's `SendInterrupted` broadcasts this to every observer, self included (`Spell.cpp:
    // 5126-5141`); 1.12's matching event is `SMSG_SPELL_FAILED_OTHER`, caster-keyed and never a
    // player `SMSG_CAST_RESULT`.
    Ok(ServerEvent::SpellFailure {
        caster,
        spell_id,
        cast_count,
        raw_reason,
    })
}

fn read_spell_delayed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.packed_guid()?;
    let delay_ms = r.u32()?;
    Ok(ServerEvent::SpellDelayed { caster, delay_ms })
}

/// `SMSG_SPELL_COOLDOWN` (`Unit.cpp:17017-17036`): raw caster, a `SPELL_COOLDOWN_FLAG_*` byte
/// with no 1.12 counterpart, then `(spell, ms)` pairs to the end.
fn read_spell_cooldowns(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.u64()?;
    let _flags = r.u8()?;
    let mut cooldowns = Vec::new();
    while r.remaining() >= 8 {
        let spell_id = r.u32()?;
        let cooldown_ms = r.u32()?;
        cooldowns.push((spell_id, cooldown_ms));
    }
    Ok(ServerEvent::SpellCooldowns { caster, cooldowns })
}

/// `SMSG_ITEM_COOLDOWN` (`Player.cpp:11787-11790`): raw item guid and spell id.
fn read_item_cooldown(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let item_guid = r.u64()?;
    let spell_id = r.u32()?;
    Ok(ServerEvent::ItemCooldown { item_guid, spell_id })
}

/// `MSG_CORPSE_QUERY`'s answer (`QueryHandler.cpp:226-266`): a found byte, then map, position
/// and the corpse's real map; a lone 0 when there is no corpse.
fn read_corpse_query(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let found = r.u8()? != 0;
    if !found {
        return Ok(ServerEvent::CorpseQuery {
            found: false,
            display_map: 0,
            position: [0.0; 3],
            corpse_map: 0,
        });
    }
    let display_map = r.i32()?;
    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    let corpse_map = r.u32()?;
    Ok(ServerEvent::CorpseQuery {
        found: true,
        display_map,
        position: [x, y, z],
        corpse_map,
    })
}

/// `SMSG_CORPSE_RECLAIM_DELAY` (`Player.cpp:13007-13012`): the ms until reclaim.
fn read_corpse_reclaim_delay(b: &[u8]) -> Result<ServerEvent> {
    Ok(ServerEvent::CorpseReclaimDelay {
        delay_ms: u32_body(b)?,
    })
}

/// `SMSG_RESURRECT_REQUEST` (`Spell.cpp:5191-5200`): raw caster, a name length, the name and its
/// NUL, the sickness byte and an optional instant-res delay override (only for
/// `SPELL_ATTR3_NO_RES_TIMER`). The 1.12 seam's `has_timer` is the absence of that override.
fn read_resurrect_request(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.u64()?;
    let _name_len = r.u32()?;
    let name = r.cstring()?;
    let sickness = r.u8()? != 0;
    let has_timer = r.remaining() < 4;
    Ok(ServerEvent::ResurrectRequest {
        caster,
        name,
        sickness,
        has_timer,
    })
}

/// `SMSG_SPIRIT_HEALER_CONFIRM` (`spell_generic.cpp:2735`): the spirit healer's raw guid.
fn read_spirit_healer_confirm(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::SpiritHealerConfirm { npc: r.u64()? })
}

/// `SMSG_COOLDOWN_EVENT` / `SMSG_CLEAR_COOLDOWN` (`Player.cpp:11052-11055`,
/// `Player.cpp:14778-14781`): the spell id first, then the raw caster guid.
fn read_cooldown_event(b: &[u8]) -> Result<(u32, u64)> {
    let mut r = Reader::new(b);
    let spell_id = r.u32()?;
    let caster = r.u64()?;
    Ok((spell_id, caster))
}

/// `MSG_CHANNEL_START` (`Spell.cpp:5158-5177`): packed caster, spell id, duration.
fn read_channel_start(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.packed_guid()?;
    let spell_id = r.u32()?;
    let duration_ms = r.u32()?;
    Ok(ServerEvent::ChannelStart {
        caster,
        spell_id,
        duration_ms,
    })
}

/// `MSG_CHANNEL_UPDATE` (`Spell.cpp:5143-5156`): packed caster, time left.
fn read_channel_update(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let caster = r.packed_guid()?;
    let remaining_ms = r.u32()?;
    Ok(ServerEvent::ChannelUpdate {
        caster,
        remaining_ms,
    })
}

fn read_spell_go(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    let item_or_caster = r.packed_guid()?;
    let caster = r.packed_guid()?;
    let cast_count = r.u8()?;
    let spell_id = r.u32()?;
    let cast_flags = r.u32()?;
    let server_time_ms = r.u32()?;
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
    let targets = read_spell_targets(&mut r)?;
    let ammo_display_id = read_spell_go_payloads(&mut r, cast_flags, targets.mask)?;
    Ok(ServerEvent::SpellGo {
        caster,
        spell_id,
        cast_flags,
        cast_count,
        server_time_ms,
        hits,
        misses,
        targets,
        ammo_display_id,
        item_caster: (item_or_caster != caster).then_some(item_or_caster),
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
    let declared_cooldowns = r.u16()?;
    // Each cooldown entry is 16 bytes (spell u32, item u16, category u16, two remainders u32).
    // The pinned writer declares `m_spellCooldowns.size()` but skips entries with
    // needSendToClient == false or no SpellInfo, so read every complete entry present, never a
    // partial one; anything left over after the full entries is a protocol error.
    let present = (r.remaining() / 16).min(usize::from(declared_cooldowns));
    let mut cooldowns = Vec::with_capacity(present);
    for _ in 0..present {
        cooldowns.push(InitialSpellCooldown {
            spell_id: r.u32()?,
            item_id: r.u16()?,
            category: r.u16()?,
            spell_cd_ms: r.u32()?,
            category_cd_ms: r.u32()?,
        });
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(ServerEvent::InitialSpells {
        spells,
        cooldowns,
        declared_cooldowns,
    })
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
            icon: r.u32()?,
            level: r.i32()? as u32,
            flags: r.u32()?,
            repeatable: r.u8()? != 0,
            title: r.cstring()?,
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
            icon: r.u32()?,
            level: r.i32()? as u32,
            flags: r.i32()? as u32,
            repeatable: r.u8()? != 0,
            title: r.cstring()?,
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
    // 12340 (`SMSG_QUESTGIVER_QUEST_DETAILS`): the second GUID, the extra dword after
    // `is_finished`, and the reputation/emote tail are Wrath-only; the 1.12 reader's
    // money-first tail cuts the packet short.
    let mut r = Reader::new(b);
    let npc = r.u64()?;
    let _guid2 = r.u64()?;
    let quest_id = r.u32()?;
    let title = r.cstring()?;
    let details = r.cstring()?;
    let objectives = r.cstring()?;
    let auto_finish = r.u8()?;
    let mut extra = QuestPanelExtra {
        flags: r.u32()?, suggested_players: r.u32()?, ..Default::default()
    };
    let _finished = r.u8()?;
    let _unknown = r.u32()?;
    let choices = read_reward_block(&mut r)?;
    let rewards = read_reward_block(&mut r)?;
    let money = r.u32()?;
    extra.xp = r.u32()?;
    extra.honor_tenths = r.u32()?;
    extra.honor_multiplier = r.f32()?;
    let reward_spell = r.u32()?;
    extra.cast_spell = r.i32()?;
    extra.title_id = r.u32()?;
    extra.talents = r.u32()?;
    extra.arena_points = r.u32()?;
    extra.reputation = read_quest_reputation(&mut r)?;
    let emotes = r.u32()?;
    for _ in 0..emotes {
        let _emote = r.u32()?;
        let _delay = r.u32()?;
    }
    if r.at != b.len() {
        return Err(DecodeError::trailing(b.len() - r.at).into());
    }
    Ok(ServerEvent::QuestDetails(QuestDetails {
        extra, npc, quest_id, title, details, objectives,
        auto_finish: auto_finish as u32, choices, rewards, money: money as i32, reward_spell,
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
    let mut extra = QuestPanelExtra {
        flags: quest_flags, suggested_players: r.u32()?, ..Default::default()
    };
    let emote_count = r.u32()?;
    for _ in 0..emote_count {
        let _delay = r.u32()?;
        let _emote = r.u32()?;
    }
    let choices = read_reward_block(&mut r)?;
    let rewards = read_reward_block(&mut r)?;
    let money = r.i32()?;
    extra.xp = r.u32()?;
    extra.honor_tenths = r.u32()?;
    extra.honor_multiplier = r.f32()?;
    let _unk = r.u32()?;
    let reward_spell = r.u32()?;
    extra.cast_spell = r.i32()?;
    extra.title_id = r.u32()?;
    extra.talents = r.u32()?;
    extra.arena_points = r.u32()?;
    let _unknown = r.u32()?;
    extra.reputation = read_quest_reputation(&mut r)?;
    if r.at != b.len() {
        return Err(DecodeError::trailing(b.len() - r.at).into());
    }
    Ok(ServerEvent::QuestOffer(QuestOfferReward {
        extra, npc, quest_id, title, offer_text, auto_finish: auto_finish as u32,
        choices, rewards, money, quest_flags, reward_spell,
    }))
}

fn read_quest_reputation(r: &mut Reader<'_>) -> Result<[(u32, i32, u32); 5]> {
    let mut rows = [(0, 0, 0); 5];
    for row in &mut rows { row.0 = r.u32()?; }
    for row in &mut rows { row.1 = r.i32()?; }
    for row in &mut rows { row.2 = r.u32()?; }
    Ok(rows)
}

fn read_quest_template(b: &[u8]) -> Result<QuestTemplate> {
    let mut r = Reader::new(b);
    let quest_id = r.u32()?;
    let method = r.u32()?;
    let level = r.u32()?;
    let _min_level = r.u32()?;
    let zone_or_sort = r.i32()?;
    let quest_type = r.u32()?;
    let suggested_players = r.u32()?;
    let rep_objective_faction = r.u32()?;
    let rep_objective_value = r.u32()?;
    let opposite_rep_faction = r.u32()?;
    let opposite_rep_value = r.u32()?;
    let next_quest_in_chain = r.u32()?;
    let xp_difficulty = r.u32()?;
    let money = r.i32()?;
    let money_max_level = r.u32()?;
    let reward_spell = r.u32()?;
    let _reward_spell_cast = r.i32()?;
    let honor_reward = r.u32()?;
    let honor_multiplier = r.f32()?;
    let src_item_id = r.u32()?;
    let flags = r.u32()?;
    let title_reward = r.u32()?;
    let _players_slain = r.u32()?;
    let bonus_talents = r.u32()?;
    let bonus_arena_points = r.u32()?;
    let _unk = r.u32()?;
    let mut rewards = [(0u32, 0u32); 4];
    for slot in &mut rewards {
        *slot = (r.u32()?, r.u32()?);
    }
    // 12340 has six choice slots, not the four the 2.4 layout carried; the two extra pairs
    // shift every field after them.
    let mut choices = [(0u32, 0u32); 6];
    for slot in &mut choices {
        *slot = (r.u32()?, r.u32()?);
    }
    // Three arrays of five: faction ids, amounts and overrides.
    let mut rep_factions = [0u32; 5];
    for faction in &mut rep_factions {
        *faction = r.u32()?;
    }
    let mut rep_amounts = [0u32; 5];
    for amount in &mut rep_amounts {
        *amount = r.u32()?;
    }
    let mut rep_overrides = [0u32; 5];
    for over in &mut rep_overrides {
        *over = r.u32()?;
    }
    let rep_rewards = std::array::from_fn(|i| {
        (rep_factions[i], rep_amounts[i] as i32, rep_overrides[i])
    });
    let point_map_id = r.u32()?;
    let point_x = r.f32()?;
    let point_y = r.f32()?;
    let point_opt = r.u32()?;
    let title = r.cstring()?;
    let objectives_text = r.cstring()?;
    let details = r.cstring()?;
    let end_text = r.cstring()?;
    // 12340 adds a fifth string, the completed-log text.
    let _completed_text = r.cstring()?;
    let mut raw = [(0u32, 0u32, 0u32, 0u32); 4];
    for slot in &mut raw {
        *slot = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
    }
    // The item requirement block moved after the objectives and is redundant with each
    // objective's own item fields, which the model already carries.
    for _ in 0..6 {
        let _item = r.u32()?;
        let _count = r.u32()?;
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
    if r.at != b.len() {
        return Err(DecodeError::trailing(b.len() - r.at).into());
    }
    Ok(QuestTemplate {
        quest_id, method, level, zone_or_sort, quest_type, suggested_players, xp_difficulty, rep_objective_faction,
        rep_objective_value, opposite_rep_faction, opposite_rep_value, next_quest_in_chain,
        money, money_max_level, reward_spell, src_item_id, flags, honor_reward,
        honor_multiplier, title_reward, bonus_talents, bonus_arena_points, rep_rewards,
        rewards, choices, point_map_id, point_x, point_y, point_opt, title, objectives_text,
        details, end_text, objectives,
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
            suffix_factor: r.u32()?,
            random_property_id: r.u32()?,
            slot_type: r.u8()?,
        });
    }
    Ok(ServerEvent::LootResponse { guid, loot_type, gold, items })
}

fn read_loot_release(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootReleaseResponse { guid: r.u64()?, result: r.u8()? })
}

fn read_loot_start_roll(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootStartRoll {
        guid: r.u64()?,
        map_id: r.u32()?,
        item_slot: r.u32()?,
        item_id: r.u32()?,
        suffix_factor: r.u32()?,
        random_property_id: r.u32()?,
        item_count: r.u32()?,
        countdown_ms: r.u32()?,
        vote_mask: r.u8()?,
    })
}

fn read_loot_roll(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootRoll {
        guid: r.u64()?,
        item_slot: r.u32()?,
        roller: r.u64()?,
        item_id: r.u32()?,
        suffix_factor: r.u32()?,
        random_property_id: r.u32()?,
        roll_number: r.u8()?,
        roll_type: r.u8()?,
        auto_pass: r.u8()? != 0,
    })
}

fn read_loot_roll_won(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootRollWon {
        guid: r.u64()?,
        item_slot: r.u32()?,
        item_id: r.u32()?,
        suffix_factor: r.u32()?,
        random_property_id: r.u32()?,
        winner: r.u64()?,
        roll_number: r.u8()?,
        roll_type: r.u8()?,
    })
}

fn read_loot_all_passed(b: &[u8]) -> Result<ServerEvent> {
    let mut r = Reader::new(b);
    Ok(ServerEvent::LootAllPassed {
        guid: r.u64()?,
        item_slot: r.u32()?,
        item_id: r.u32()?,
        random_property_id: r.u32()?,
        suffix_factor: r.u32()?,
    })
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
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| DecodeError::truncated(self.remaining()))?;
        let s = self
            .p
            .get(self.at..end)
            .ok_or_else(|| DecodeError::truncated(self.remaining()))?;
        self.at = end;
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
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| DecodeError::unterminated(rest.len()))?;
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

/// One visual slot of a roster record (`Player::BuildEnumData`'s equipment loop,
/// `Player.cpp:1231`-1297): the slot item's `CreatureDisplayInfo.dbc` id (0 for an empty slot),
/// the item's `InventoryType`, and the one permanent enchant's aura id the server already
/// resolved for that item (`0` for none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharEnumVisual {
    pub display_id: u32,
    pub inventory_type: u8,
    pub enchant_id: u32,
}

/// One `SMSG_CHAR_ENUM` roster record, as the 3.3.5a `Player::BuildEnumData`
/// (`Player.cpp:1106`-1299) writes it: the raw guid and the cstr name, the identity and
/// appearance bytes, the zone/map/position triple, the guild id, the character and customize
/// flags, the first-login byte, the pet triple, and 23 visual slots (equipment 0..18 then bag
/// slots 19..22, the bag ids always written even when the first-login zone hides the zone).
#[derive(Clone, Debug, PartialEq)]
pub struct CharEnumRecord {
    pub guid: u64,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_style: u8,
    /// The wire level byte; 0 marks a first-login character (the 1.12 `Character`'s
    /// `new_character` edge).
    pub level: u8,
    pub zone: u32,
    pub map: u32,
    pub position: [f32; 3],
    pub guild_id: u32,
    pub character_flags: u32,
    pub customize_flags: u32,
    pub first_login: bool,
    pub pet_display_id: u32,
    pub pet_level: u32,
    pub pet_family: u32,
    pub visuals: [CharEnumVisual; 23],
}

fn read_char_enum_visuals(r: &mut Reader<'_>) -> Result<[CharEnumVisual; 23]> {
    let mut visuals = [CharEnumVisual {
        display_id: 0,
        inventory_type: 0,
        enchant_id: 0,
    }; 23];
    for slot in &mut visuals {
        slot.display_id = r.u32()?;
        slot.inventory_type = r.u8()?;
        slot.enchant_id = r.u32()?;
    }
    Ok(visuals)
}

/// `SMSG_CHAR_ENUM` (`Player.cpp:1233`-1238): a byte count followed by that many packed
/// `BuildEnumData` records. Trailing bytes are not the records' business; the count drives the
/// walk so a server that appends never poisons the parse.
pub fn parse_char_enum(payload: &[u8]) -> Result<Vec<CharEnumRecord>> {
    let mut r = Reader::new(payload);
    let count = r.u8()?;
    let mut records = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let guid = r.u64()?;
        let name = r.cstring()?;
        let race = r.u8()?;
        let class = r.u8()?;
        let gender = r.u8()?;
        let skin = r.u8()?;
        let face = r.u8()?;
        let hair_style = r.u8()?;
        let hair_color = r.u8()?;
        let facial_style = r.u8()?;
        let level = r.u8()?;
        let zone = r.u32()?;
        let map = r.u32()?;
        let x = r.f32()?;
        let y = r.f32()?;
        let z = r.f32()?;
        let guild_id = r.u32()?;
        let character_flags = r.u32()?;
        let customize_flags = r.u32()?;
        let first_login = r.u8()? != 0;
        let pet_display_id = r.u32()?;
        let pet_level = r.u32()?;
        let pet_family = r.u32()?;
        let visuals = read_char_enum_visuals(&mut r)?;
        records.push(CharEnumRecord {
            guid,
            name,
            race,
            class,
            gender,
            skin,
            face,
            hair_style,
            hair_color,
            facial_style,
            level,
            zone,
            map,
            position: [x, y, z],
            guild_id,
            character_flags,
            customize_flags,
            first_login,
            pet_display_id,
            pet_level,
            pet_family,
            visuals,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrath_monster_move_reaches_the_gameplay_decoder() {
        // AC MovementPacketBuilder: mover, movement byte, start, spline id, stop type.
        let mut body = vec![1, 7, 0];
        for value in [1.0f32, 2.0, 3.0] {
            body.extend_from_slice(&value.to_le_bytes());
        }
        body.extend_from_slice(&123u32.to_le_bytes());
        body.push(1);
        let event = parse(0x0dd, &body).expect("roaming/stop packets must not be discarded")
            .expect("the complete stop body is valid");
        let ServerEvent::MonsterMove(m) = event else { panic!("wrong movement event") };
        assert!(m.stop);
        assert_eq!((m.guid, m.spline_id, m.start), (7, 123, [1.0, 2.0, 3.0]));
    }

    fn le<const N: usize>(v: &[u8]) -> [u8; N] {
        v.try_into().unwrap()
    }

    /// One whole record in the exact `Player::BuildEnumData` order
    /// (`Player.cpp:1138`-1297), with the name and two visual slots filled.
    fn enum_record_bytes() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0x0000_0100_0000_0042u64.to_le_bytes()); // guid
        b.extend_from_slice(b"Padfoot\0"); // name cstr
        b.push(1); // race
        b.push(4); // class
        b.push(0); // gender
        b.push(3); // skin
        b.push(2); // face
        b.push(4); // hair style
        b.push(5); // hair color
        b.push(1); // facial style
        b.push(10); // level
        b.extend_from_slice(&12u32.to_le_bytes()); // zone
        b.extend_from_slice(&0u32.to_le_bytes()); // map
        b.extend_from_slice((-8943.5f32).to_le_bytes().as_slice()); // x
        b.extend_from_slice(&132.25f32.to_le_bytes()); // y
        b.extend_from_slice(&61.5f32.to_le_bytes()); // z
        b.extend_from_slice(&0u32.to_le_bytes()); // guild id
        b.extend_from_slice(&0x0040_0000u32.to_le_bytes()); // character flags
        b.extend_from_slice(&0u32.to_le_bytes()); // customize flags
        b.push(0); // first login
        b.extend_from_slice(&0u32.to_le_bytes()); // pet display
        b.extend_from_slice(&0u32.to_le_bytes()); // pet level
        b.extend_from_slice(&0u32.to_le_bytes()); // pet family
        for index in 0..23u8 {
            let slot = index as u32;
            b.extend_from_slice(&slot.to_le_bytes()); // display id
            b.push((index % 2 == 0) as u8); // inventory type
            b.extend_from_slice(&slot.to_le_bytes()); // enchant aura id
        }
        b
    }

    #[test]
    fn char_enum_count_then_build_enum_data_records() {
        let mut payload = vec![1u8]; // one record
        payload.extend_from_slice(&enum_record_bytes());
        let records = parse_char_enum(&payload).expect("parse");
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.guid, 0x0000_0100_0000_0042);
        assert_eq!(record.name, "Padfoot");
        assert_eq!(record.race, 1);
        assert_eq!(record.class, 4);
        assert_eq!(record.level, 10);
        assert_eq!(record.zone, 12);
        assert_eq!(record.map, 0);
        assert_eq!(record.position[0], -8943.5);
        assert_eq!(record.character_flags, 0x0040_0000);
        assert!(!record.first_login);
        assert_eq!(record.visuals.len(), 23);
        assert_eq!(record.visuals[0].display_id, 0);
        assert_eq!(record.visuals[0].inventory_type, 1);
        assert_eq!(record.visuals[1].display_id, 1);
        assert_eq!(record.visuals[1].inventory_type, 0);
        assert_eq!(record.visuals[22].enchant_id, 22);
    }

    #[test]
    fn char_enum_parses_two_records_and_takes_the_count_along() {
        let mut payload = vec![2u8];
        payload.extend_from_slice(&enum_record_bytes());
        payload.extend_from_slice(&enum_record_bytes());
        let records = parse_char_enum(&payload).expect("parse");
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn char_enum_refuses_a_record_cut_short() {
        let mut payload = vec![1u8];
        let whole = enum_record_bytes();
        payload.extend_from_slice(&whole[..whole.len() - 3]);
        assert!(parse_char_enum(&payload).is_err());
    }

    #[test]
    fn cast_target_is_a_mask_then_a_packed_guid() {
        let target = 0x0000_0000_0000_002a;
        let p = cast_spell(7, 133, CastTarget::Unit(target));
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
                    suffix_factor: 0,
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
    #[test]
    fn area_trigger_message_uses_the_length_prefix() {
        let mut body = 6u32.to_le_bytes().to_vec();
        body.extend_from_slice(b"hello\0");
        assert_eq!(
            read_area_trigger_message(&body).unwrap(),
            ServerEvent::AreaTriggerMessage { text: "hello".into() }
        );
        let mut overrun = 10u32.to_le_bytes().to_vec();
        overrun.extend_from_slice(b"hi\0");
        assert!(read_area_trigger_message(&overrun).is_err());
        let mut unterminated = 5u32.to_le_bytes().to_vec();
        unterminated.extend_from_slice(b"hello");
        assert!(read_area_trigger_message(&unterminated).is_err());
    }

    #[test]
    fn query_time_keeps_the_reset_word() {
        let mut body = 1_700_000_000u32.to_le_bytes().to_vec();
        body.extend_from_slice(&42_000u32.to_le_bytes());
        assert_eq!(
            read_server_time(&body).unwrap(),
            ServerEvent::ServerUnixTime {
                unix_time: 1_700_000_000,
                reset_in_secs: 42_000,
            }
        );
    }

    #[test]
    fn inventory_failure_keeps_both_item_guids() {
        let mut body = vec![2u8];
        body.extend_from_slice(&0x11u64.to_le_bytes());
        body.extend_from_slice(&0x22u64.to_le_bytes());
        body.push(7);
        assert_eq!(
            read_inventory_failure(&body).unwrap(),
            ServerEvent::InventoryFailure {
                reason: 2,
                item: Some(0x11),
                item2: Some(0x22),
                bag_type: 7,
                required_level: None,
                item_limit_category: None,
            }
        );
    }

    #[test]
    fn item_text_has_a_present_and_absent_form() {
        let mut present = vec![0u8];
        present.extend_from_slice(&0x33u64.to_le_bytes());
        present.extend_from_slice(b"page\0");
        assert_eq!(
            read_item_text(&present).unwrap(),
            ServerEvent::ItemText {
                has_text: true,
                item_guid: 0x33,
                text: "page".into(),
            }
        );
        assert_eq!(
            read_item_text(&[1]).unwrap(),
            ServerEvent::ItemText {
                has_text: false,
                item_guid: 0,
                text: String::new(),
            }
        );
    }

    /// The 12340 details tail: second GUID, the dword after `is_finished`, reputation
    /// arrays and the emote list, which the 1.12 reader did not know.
    fn quest_details_bytes() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0x0000_0002_0000_0011u64.to_le_bytes()); // npc
        b.extend_from_slice(&0u64.to_le_bytes()); // guid2
        b.extend_from_slice(&338u32.to_le_bytes()); // quest
        b.extend_from_slice(b"The Green Hills\0");
        b.extend_from_slice(b"Details.\0");
        b.extend_from_slice(b"Objectives.\0");
        b.push(1); // auto finish
        b.extend_from_slice(&8u32.to_le_bytes()); // flags
        b.extend_from_slice(&0u32.to_le_bytes()); // suggested
        b.push(0); // is_finished
        b.extend_from_slice(&0u32.to_le_bytes()); // 12340 dword
        b.extend_from_slice(&1u32.to_le_bytes()); // one choice
        b.extend_from_slice(&44u32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&555u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // no fixed rewards
        b.extend_from_slice(&4700u32.to_le_bytes()); // money
        b.extend_from_slice(&0u32.to_le_bytes()); // xp
        b.extend_from_slice(&0u32.to_le_bytes()); // honor
        b.extend_from_slice(&0f32.to_le_bytes()); // honor multiplier
        b.extend_from_slice(&0u32.to_le_bytes()); // reward spell
        b.extend_from_slice(&0u32.to_le_bytes()); // casted spell
        b.extend_from_slice(&0u32.to_le_bytes()); // title
        b.extend_from_slice(&0u32.to_le_bytes()); // talent
        b.extend_from_slice(&0u32.to_le_bytes()); // arena
        for _ in 0..15 {
            b.extend_from_slice(&0u32.to_le_bytes()); // reputations
        }
        b.extend_from_slice(&1u32.to_le_bytes()); // one emote
        b.extend_from_slice(&5u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    #[test]
    fn quest_details_reads_the_12340_tail() {
        let event = parse(crate::opcode::SMSG_QUESTGIVER_QUEST_DETAILS, &quest_details_bytes()).unwrap();
        match event.unwrap() {
            ServerEvent::QuestDetails(d) => {
                assert_eq!(d.quest_id, 338);
                assert_eq!(d.title, "The Green Hills");
                assert_eq!(d.money, 4700);
                assert_eq!(d.choices.len(), 1);
                assert_eq!(d.choices[0].item_id, 44);
                assert_eq!(d.choices[0].count, 2);
                assert_eq!(d.rewards.len(), 0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn quest_offer_retains_reward_metadata_and_requires_the_full_tail() {
        let mut b = 0x42u64.to_le_bytes().to_vec();
        b.extend_from_slice(&7u32.to_le_bytes());
        b.extend_from_slice(b"Title\0Reward text\0");
        b.push(1);
        for word in [0x2008u32, 3, 0, 0, 0, 400, 123, 70] {
            b.extend_from_slice(&word.to_le_bytes());
        }
        b.extend_from_slice(&1.5f32.to_le_bytes());
        for word in [8u32, 55, 66, 77, 2, 33, 0] {
            b.extend_from_slice(&word.to_le_bytes());
        }
        for word in [72u32, 0, 0, 0, 0, u32::MAX, 0, 0, 0, 0, 100, 0, 0, 0, 0] {
            b.extend_from_slice(&word.to_le_bytes());
        }
        let ServerEvent::QuestOffer(offer) = read_quest_offer(&b).unwrap() else { panic!("offer"); };
        assert_eq!(offer.reward_spell, 55);
        assert_eq!(offer.extra, QuestPanelExtra {
            flags: 0x2008, suggested_players: 3, xp: 123, honor_tenths: 70,
            honor_multiplier: 1.5, cast_spell: 66, title_id: 77, talents: 2, arena_points: 33,
            reputation: [(72, -1, 100), (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0)],
        });
        assert!(read_quest_offer(&b[..b.len()-1]).is_err());
        b.push(0);
        assert!(read_quest_offer(&b).is_err());
    }

    /// The 12340 template: six choice slots, the completed text and the relocated item
    /// requirement block; the 2.4 layout shifted every string after the choices.
    #[test]
    fn quest_template_reads_the_12340_layout() {
        let mut b = Vec::new();
        for v in [338u32, 0, 40, 30, 5280, 0, 3, 0, 0, 0, 0, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&4700i32.to_le_bytes()); // money
        b.extend_from_slice(&0u32.to_le_bytes()); // max-level money
        b.extend_from_slice(&0u32.to_le_bytes()); // reward spell
        b.extend_from_slice(&0u32.to_le_bytes()); // casted spell
        b.extend_from_slice(&0u32.to_le_bytes()); // honor
        b.extend_from_slice(&0f32.to_le_bytes()); // honor multiplier
        b.extend_from_slice(&0u32.to_le_bytes()); // source item
        b.extend_from_slice(&8u32.to_le_bytes()); // flags
        for _ in 0..5 {
            b.extend_from_slice(&0u32.to_le_bytes()); // title, slain, talents, arena, unknown
        }
        for _ in 0..4 {
            b.extend_from_slice(&0u32.to_le_bytes()); // reward id
            b.extend_from_slice(&0u32.to_le_bytes()); // reward count
        }
        for slot in [11u32, 22, 33, 44, 55, 66] {
            b.extend_from_slice(&slot.to_le_bytes()); // choice id
            b.extend_from_slice(&(slot / 11).to_le_bytes()); // choice count
        }
        for _ in 0..15 {
            b.extend_from_slice(&0u32.to_le_bytes()); // reputation arrays
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // point map
        b.extend_from_slice(&0f32.to_le_bytes()); // x
        b.extend_from_slice(&0f32.to_le_bytes()); // y
        b.extend_from_slice(&0u32.to_le_bytes()); // point opt
        b.extend_from_slice(b"Title\0Objective\0Details\0End\0Completed\0");
        for objective in [(7u32, 3u32, 0u32, 0u32), (0, 0, 0, 0), (0, 0, 0, 0), (0, 0, 0, 0)] {
            for v in [objective.0, objective.1, objective.2, objective.3] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        for _ in 0..6 {
            b.extend_from_slice(&0u32.to_le_bytes()); // item requirement id
            b.extend_from_slice(&0u32.to_le_bytes()); // item requirement count
        }
        b.extend_from_slice(b"One\0Two\0Three\0Four\0");
        let template = read_quest_template(&b).expect("parse");
        assert_eq!(template.quest_id, 338);
        assert_eq!(template.suggested_players, 3);
        assert_eq!(template.title, "Title");
        assert_eq!(template.objectives_text, "Objective");
        assert_eq!(template.details, "Details");
        assert_eq!(template.end_text, "End");
        assert_eq!(template.money, 4700);
        assert_eq!(template.choices[5], (66, 6));
        assert_eq!(template.objectives[0].creature_or_go, 7);
        assert_eq!(template.objectives[0].required_count, 3);
        assert_eq!(template.objectives[0].text, "One");
        assert_eq!(template.objectives[3].text, "Four");
    }

    #[test]
    fn cast_failure_keeps_the_attempt_identity() {
        let mut body = vec![7u8];
        body.extend_from_slice(&133u32.to_le_bytes());
        body.push(2);
        match read_cast_failed(&body).unwrap() {
            ServerEvent::CastFailed {
                spell_id,
                cast_count,
                raw_reason,
                ..
            } => {
                assert_eq!(spell_id, 133);
                assert_eq!(cast_count, 7);
                assert_eq!(raw_reason, 2);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    fn build_item_info_payload(
        entry: u32,
        stats_count_header: u32,
        stats: &[(u32, i32)],
        bag_family: u32,
    ) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&entry.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes()); // class
        b.extend_from_slice(&7u32.to_le_bytes()); // subclass
        b.extend_from_slice(&(-1i32).to_le_bytes()); // sound_override_subclass
        b.extend_from_slice(b"Worn Shortsword\0\0\0\0"); // 4 cstrings
        b.extend_from_slice(&1542u32.to_le_bytes()); // display_info_id
        b.extend_from_slice(&1u32.to_le_bytes()); // quality
        b.extend_from_slice(&0u32.to_le_bytes()); // flags
        b.extend_from_slice(&0u32.to_le_bytes()); // flags2
        b.extend_from_slice(&35u32.to_le_bytes()); // buy_price
        b.extend_from_slice(&7u32.to_le_bytes()); // sell_price
        b.extend_from_slice(&21u32.to_le_bytes()); // inventory_type
        b.extend_from_slice(&(-1i32).to_le_bytes()); // allowable_class
        b.extend_from_slice(&(-1i32).to_le_bytes()); // allowable_race
        b.extend_from_slice(&2u32.to_le_bytes()); // item_level
        b.extend_from_slice(&1u32.to_le_bytes()); // required_level
        for _ in 0..7 {
            b.extend_from_slice(&0u32.to_le_bytes()); // required_skill..required_rep_rank
        }
        b.extend_from_slice(&0i32.to_le_bytes()); // max_count
        b.extend_from_slice(&1i32.to_le_bytes()); // stackable
        b.extend_from_slice(&0u32.to_le_bytes()); // container_slots
        b.extend_from_slice(&stats_count_header.to_le_bytes());
        for &(stat_type, stat_val) in stats {
            b.extend_from_slice(&stat_type.to_le_bytes());
            b.extend_from_slice(&stat_val.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // scaling_distribution
        b.extend_from_slice(&0u32.to_le_bytes()); // scaling_value
        for i in 0..2 {
            let (min, max) = if i == 0 { (1.0f32, 3.0f32) } else { (0.0, 0.0) };
            b.extend_from_slice(&min.to_le_bytes());
            b.extend_from_slice(&max.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // armor
        for _ in 0..6 {
            b.extend_from_slice(&0i32.to_le_bytes()); // resistances
        }
        b.extend_from_slice(&1900u32.to_le_bytes()); // delay_ms
        b.extend_from_slice(&0u32.to_le_bytes()); // ammo_type
        b.extend_from_slice(&0.0f32.to_le_bytes()); // ranged_mod_range
        for _ in 0..5 {
            for _ in 0..6 {
                b.extend_from_slice(&0u32.to_le_bytes()); // 5 spell slots * 6 dwords
            }
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // bonding
        b.extend_from_slice(b"Starter blade\0"); // description
        for _ in 0..14 {
            b.extend_from_slice(&0u32.to_le_bytes()); // page_text..map (14 dwords)
        }
        b.extend_from_slice(&bag_family.to_le_bytes()); // trailing sentinel
        b
    }

    #[test]
    fn item_info_rejects_oversized_stat_count_without_cursor_desync() {
        // 0 stats: trailing bag_family sentinel must align and parse accurately.
        let zero_stats = build_item_info_payload(25, 0, &[], 0xCAFE_1234);
        match read_item_info(&zero_stats).expect("0 stats should parse") {
            ServerEvent::ItemInfo {
                entry,
                info: Some(info),
            } => {
                assert_eq!(entry, 25);
                assert!(info.stats.is_empty());
                assert_eq!(info.bag_family, 0xCAFE_1234);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        // 10 stats (12340 MAX_ITEM_PROTO_STATS limit): all 10 stats and trailing bag_family align.
        let ten_pairs: Vec<(u32, i32)> = (1..=10).map(|i| (i, i as i32 * 2)).collect();
        let ten_stats = build_item_info_payload(49623, 10, &ten_pairs, 0xBEEF_5678);
        match read_item_info(&ten_stats).expect("10 stats should parse") {
            ServerEvent::ItemInfo {
                entry,
                info: Some(info),
            } => {
                assert_eq!(entry, 49623);
                assert_eq!(info.stats, ten_pairs);
                assert_eq!(info.bag_family, 0xBEEF_5678);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        // 11 and 64 stats: must reject immediately rather than clamping and desyncing the cursor.
        for bad_count in [11u32, 64u32] {
            let pairs: Vec<(u32, i32)> = (0..bad_count).map(|i| (3, i as i32 + 1)).collect();
            let oversized = build_item_info_payload(99999, bad_count, &pairs, 0x1111_2222);
            let err = read_item_info(&oversized).expect_err("oversized stats_count must fail");
            assert!(
                err.to_string().contains("exceeds 12340 limit 10"),
                "unexpected error for stats_count={bad_count}: {err:#}"
            );
        }

        // Truncated stat block (header claims 4 stats, payload truncated mid-stats): must fail.
        let mut truncated = Vec::new();
        truncated.extend_from_slice(&25u32.to_le_bytes());
        truncated.extend_from_slice(&2u32.to_le_bytes());
        truncated.extend_from_slice(&7u32.to_le_bytes());
        truncated.extend_from_slice(&(-1i32).to_le_bytes());
        truncated.extend_from_slice(b"Truncated\0\0\0\0");
        for _ in 0..17 {
            truncated.extend_from_slice(&0u32.to_le_bytes());
        }
        truncated.extend_from_slice(&4u32.to_le_bytes()); // claims 4 stats, 0 follow
        assert!(read_item_info(&truncated).is_err());
    }
}
