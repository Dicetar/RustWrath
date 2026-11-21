//! Trade-skill link encoding, parsing and bitmap decoding for 3.3.5a build 12340.
//!
//! Reference behavior (`wowdecomp` pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`):
//! - Alphabet (`0x00A1B640`): standard 64-character base64 (`A-Z`, `a-z`, `0-9`, `+`, `/`), no
//!   `=` padding.
//! - Decode table (`0x00A1B688`): 128-entry ASCII lookup returning 0..=63 (0 for unmapped ASCII).
//! - Record order (`SpellTableBuildSkillLineLinkInfo` `0x00812030`,
//!   `SpellTableGetSkillLineLinkInfo` `0x008104A0`): first contiguous raw-DBC run of
//!   `SkillLineAbility.dbc` rows for a `SkillLine.dbc` entry with `m_canLink != 0`.
//! - Bit packing (`CGTradeSkillInfo::GetTradeSkillListLink` `0x005DB060`,
//!   `CGTradeSkillInfo::RefreshList` `0x005DD5A0`): LSB-first within each 6-bit character, bit `i`
//!   at byte `i / 6`, bit position `i % 6`. Trailing partial group is emitted without `=`.
//! - Link format (`0x005DB060`):
//!   `|cffffd000|Htrade:<spellId>:<curRank>:<maxRank>:<guidHex>:<bitmap>|h[<spellName>]|h|r`
//!   with uppercase unpadded hexadecimal GUID (`%I64X`).
//! - Hyperlink parsing (`Script_SetHyperlink` `0x0062DAE0`, `CGTradeSkillInfo::SetLink`
//!   `0x005DE300`, `CGTradeSkillInfo::SetTradeSkill` `0x005DE120`).

use std::collections::HashSet;

/// 12340 `s_base64Encode` (`STATIC: 0x00A1B640`).
const BASE64_ENCODE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 12340 `s_base64Decode` (`STATIC: 0x00A1B688`): maps ASCII 0..128 to 6-bit value (unmapped = 0).
const BASE64_DECODE: [u8; 128] = {
    let mut table = [0u8; 128];
    let mut i = 0;
    while i < 64 {
        table[BASE64_ENCODE[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// One `SkillLineAbility.dbc` row for a linkable skill line in raw DBC file order
/// (`0x00812030`, `0x008104A0`).
///
/// Owner: `SkillLines` (`benilla_formats::SkillLineCatalog` loaded from `SkillLineAbility.dbc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeSkillLinkRow {
    pub spell_id: u32,
    pub race_mask: u32,
    pub class_mask: u32,
}

/// App-supplied inputs for encoding the player's own `GetTradeSkillListLink()`
/// (`CGTradeSkillInfo::GetTradeSkillListLink` `0x005DB060`).
///
/// Input owners:
/// - `spell_id`: `TradeSkillOpen` (`benilla-app::ui_tradeskill`, captured from the profession
///   opener cast, `SPELL_EFFECT_TRADE_SKILL`).
/// - `spell_name`: `Spells` (`benilla_formats::SpellCatalog`, `Spell.dbc` name of `spell_id`).
/// - `can_link`: `SkillLines` (`benilla_formats::SkillLineCatalog`, `SkillLine.dbc` column 55
///   `m_canLink`).
/// - `cur_rank`, `max_rank`: `ObjectStore` (`benilla-app::net`, `PLAYER_SKILL_INFO` plus permanent
///   bonus, matching `CGPlayer_C::GetSkillValue` `0x0051A250` and `GetSkillMaxRank` `0x00594710`).
/// - `player_guid`: `SelfGuid` / `SelfPlayer` (`benilla-app::net`).
/// - `player_race`, `player_class`: `ObjectStore` (`benilla-app::net`, `UNIT_FIELD_BYTES_0`).
/// - `ability_rows`: `SkillLines` (`benilla_formats::SkillLineCatalog`, first contiguous raw-DBC
///   run of `SkillLineAbility.dbc` rows for the open skill line).
/// - `known_spells`: `PlayerActions` (`benilla-app::ui_action`, populated from
///   `SMSG_INITIAL_SPELLS` / `SMSG_LEARNED_SPELL` / `SMSG_REMOVED_SPELL`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TradeSkillLinkSource {
    pub spell_id: u32,
    pub spell_name: String,
    pub can_link: bool,
    pub cur_rank: u32,
    pub max_rank: u32,
    pub player_guid: u64,
    pub player_race: u8,
    pub player_class: u8,
    pub ability_rows: Vec<TradeSkillLinkRow>,
    pub known_spells: HashSet<u32>,
}

/// Parsed fields of a `trade:<spellId>:<rank>:<maxRank>:<guidHex>:<bitmap>` hyperlink
/// (`CGTradeSkillInfo::SetLink` `0x005DE300`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTradeLink {
    pub spell_id: u32,
    pub rank: u32,
    pub max_rank: u32,
    pub owner_guid: u64,
    pub bitmap: String,
    /// Optional `[Name]` extracted when a full chat link `|c...|Htrade:...|h[Name]|h|r` was passed.
    pub display_name: Option<String>,
}

/// Number of base64 characters required for `ability_count` rows (`count / 6 + (count % 6 != 0)`,
/// `0x005DB060` and `0x005DE120`).
pub fn bitmap_char_len(ability_count: usize) -> usize {
    ability_count.div_ceil(6)
}

/// `SpellTableMatchRaceClass(race, classId, raceMask, classMask, 0, 0)` (`0x00810320`, called with
/// zero exclude masks at `0x005DB060`).
pub fn match_race_class(race: u8, class: u8, race_mask: u32, class_mask: u32) -> bool {
    let race_ok =
        race_mask == 0 || (race_mask & 1u32.wrapping_shl(u32::from(race).wrapping_sub(1))) != 0;
    let class_ok =
        class_mask == 0 || (class_mask & 1u32.wrapping_shl(u32::from(class).wrapping_sub(1))) != 0;
    race_ok && class_ok
}

/// Encode a known-recipe bitmap in raw `SkillLineAbility.dbc` order (`0x005DB060`).
///
/// Each 6-bit character packs 6 consecutive rows LSB-first (bit 0 = first row in the group).
/// Any trailing partial group is emitted without `=` padding.
pub fn encode_bitmap(
    ability_rows: &[TradeSkillLinkRow],
    known_spells: &HashSet<u32>,
    race: u8,
    class: u8,
) -> String {
    let mut out = String::with_capacity(bitmap_char_len(ability_rows.len()));
    let mut bits: u8 = 0;
    let mut bit: u8 = 0;

    for row in ability_rows {
        if known_spells.contains(&row.spell_id)
            && match_race_class(race, class, row.race_mask, row.class_mask)
        {
            bits |= 1u8 << bit;
        }
        bit += 1;
        if bit >= 6 {
            out.push(BASE64_ENCODE[usize::from(bits)] as char);
            bit = 0;
            bits = 0;
        }
    }

    if bit > 0 {
        out.push(BASE64_ENCODE[usize::from(bits)] as char);
    }

    out
}

/// Encode the full chat hyperlink for `GetTradeSkillListLink()` (`0x005DB060`):
/// `|cffffd000|Htrade:<spellId>:<curRank>:<maxRank>:<guidHex>:<bitmap>|h[<spellName>]|h|r`.
///
/// Returns `None` when the profession cannot be linked (`!can_link`), has no ability rows, or
/// lacks a valid opener spell or player GUID.
pub fn encode_trade_skill_link(source: &TradeSkillLinkSource) -> Option<String> {
    if !source.can_link
        || source.spell_id == 0
        || source.player_guid == 0
        || source.ability_rows.is_empty()
    {
        return None;
    }

    let bitmap = encode_bitmap(
        &source.ability_rows,
        &source.known_spells,
        source.player_race,
        source.player_class,
    );

    Some(format!(
        "|cffffd000|Htrade:{}:{}:{}:{:X}:{}|h[{}]|h|r",
        source.spell_id,
        source.cur_rank,
        source.max_rank,
        source.player_guid,
        bitmap,
        source.spell_name,
    ))
}

/// Format a canonical chat link string from a parsed link and display name.
pub fn format_trade_link(parsed: &ParsedTradeLink, name: &str) -> String {
    format!(
        "|cffffd000|Htrade:{}:{}:{}:{:X}:{}|h[{}]|h|r",
        parsed.spell_id, parsed.rank, parsed.max_rank, parsed.owner_guid, parsed.bitmap, name,
    )
}

/// Parse a `trade:` hyperlink payload, accepting both bare `"trade:..."` (as `SetItemRef` passes
/// from `ChatFrame_OnHyperlinkShow`) and a full `"|cffffd000|Htrade:...|h[Name]|h|r"` chat link
/// (`Script_SetHyperlink` `0x0062DAE0`, `CGTradeSkillInfo::SetLink` `0x005DE300`).
pub fn parse_trade_link(raw: &str) -> Option<ParsedTradeLink> {
    let start = raw.find("trade:")? + "trade:".len();
    let tail = &raw[start..];
    let (payload, display_name) = match tail.find('|') {
        Some(bar) => (&tail[..bar], extract_link_name(&tail[bar..])),
        None => (tail, None),
    };

    let mut parts = payload.splitn(5, ':');
    let spell_id = parts.next()?.parse::<u32>().ok().filter(|&id| id != 0)?;
    let rank = parts.next()?.parse::<u32>().ok()?;
    let max_rank = parts.next()?.parse::<u32>().ok()?;
    let guid_hex = parts.next()?;
    let owner_guid = u64::from_str_radix(guid_hex, 16).ok().filter(|&g| g != 0)?;
    let bitmap = parts.next()?;
    if bitmap.is_empty() || !bitmap.is_ascii() {
        return None;
    }

    Some(ParsedTradeLink {
        spell_id,
        rank,
        max_rank,
        owner_guid,
        bitmap: bitmap.to_string(),
        display_name,
    })
}

/// Extract `[Name]` from a trailing `|h[Name]|h...` suffix if present.
fn extract_link_name(suffix: &str) -> Option<String> {
    let rest = suffix.strip_prefix("|h[")?;
    let end = rest.find("]|h")?;
    Some(rest[..end].to_string())
}

/// Decode a trade-skill link bitmap into the indices of set rows (`0x005DE120` length check and
/// `0x005DD5A0` bit extraction).
///
/// Returns `None` if `ability_count == 0`, `bitmap` is non-ASCII, or `bitmap.len()` does not
/// match `bitmap_char_len(ability_count)`.
pub fn decode_bitmap_indices(ability_count: usize, bitmap: &str) -> Option<Vec<usize>> {
    if ability_count == 0 || !bitmap.is_ascii() || bitmap.len() != bitmap_char_len(ability_count) {
        return None;
    }
    let bytes = bitmap.as_bytes();
    let mut indices = Vec::new();
    let mut offset = 0usize;
    let mut bit = 0u8;

    for i in 0..ability_count {
        let decoded = BASE64_DECODE[usize::from(bytes[offset])];
        if (decoded & (1u8 << bit)) != 0 {
            indices.push(i);
        }
        bit += 1;
        if bit >= 6 {
            bit = 0;
            offset += 1;
        }
    }

    Some(indices)
}

/// Decode a trade-skill link bitmap against `ability_rows` into the set spell IDs in raw DBC
/// order (`0x005DE120`, `0x005DD5A0`).
pub fn decode_bitmap(ability_rows: &[TradeSkillLinkRow], bitmap: &str) -> Option<Vec<u32>> {
    let indices = decode_bitmap_indices(ability_rows.len(), bitmap)?;
    Some(
        indices
            .into_iter()
            .map(|i| ability_rows[i].spell_id)
            .collect(),
    )
}
