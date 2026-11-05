//! 12340 pet, talent and stable-master server replies.
//!
//! Every reader here validates one packet body against the pinned AzerothCore build-12340
//! writer named beside it and the pinned 3.3.5a client decomp. These are 12340 layouts: the
//! pet bar carries a `u16` creature family after the guid and a `u32` spell id in each
//! cooldown row, and the talent reply carries a leading player/pet discriminator. No game
//! state is owned here; the readers return typed rows for the app's existing pet/talent
//! owners, and reject truncation, counts the client cannot store, and trailing bytes instead
//! of skipping or clamping them.

use super::{DecodeError, Reader};
use anyhow::Result;

/// `MAX_UNIT_ACTION_BAR_INDEX` (`Player::PetSpellInitialize`, `Player.cpp:9675/9680`): the ten
/// bar words the server always writes.
pub const PET_ACTION_SLOTS: usize = 10;

/// Conservative ceiling for `SMSG_PET_GUIDS`. The client grows `s_pets` to the declared count
/// (`PetInfo.cpp:184/1071`); this bound keeps one malformed `u32` from sizing an unbounded
/// allocation while well above the handful of controlled pets real traffic carries.
pub const MAX_PET_GUIDS: u32 = 64;

/// `MAX_TALENT_SPECS` (`SharedDefines.h`): the client stores two player talent groups
/// (`TalentInfo.cpp:1161`).
pub const MAX_TALENT_SPECS: u8 = 2;

/// `MAX_GLYPH_SLOT_INDEX` (`SharedDefines.h:664`): six glyphs per spec
/// (`TalentInfo.cpp:950-960`).
pub const MAX_GLYPHS_PER_SPEC: u8 = 6;

/// `CGStableInfo::m_pets[6]` (`StableInfo.h:22`): the client writes row `i` at `m_pets[i + 1]`,
/// so the wire's current pet plus stabled rows may not exceed five.
pub const MAX_STABLED_PETS: u8 = 5;

/// `SMSG_TALENTS_INFO`'s leading discriminator (`Player::SendTalentsInfoData`,
/// `Player.cpp:14726-14734`): 0 is the player, 1 is the pet.
const TALENT_TYPE_PLAYER: u8 = 0;
const TALENT_TYPE_PET: u8 = 1;

/// `SMSG_PET_SPELLS`: the pet action bar's entire 12340 body.
///
/// `Player::PetSpellInitialize` (`Player.cpp:9664-9734`) writes the guid, the 12340-only
/// creature family, the duration and the packed state dword, ten bar words, the spellbook and
/// the cooldown rows; `Packet_SMSG_PET_SPELLS` (`PetInfo.cpp:860-927`) reads exactly that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetSpells {
    pub pet_guid: u64,
    /// `CreatureTemplate::family`, read by the client into `CGPetInfo::SetPet` for the pet
    /// talent tab. A zero guid teardown carries `0` here.
    pub creature_family: u16,
    pub duration_ms: u32,
    /// One dword as the client stores it: react byte, command byte, flags `u16`.
    pub state: u32,
    pub bar: [u32; PET_ACTION_SLOTS],
    pub spells: Vec<u32>,
    pub cooldowns: Vec<PetSpellCooldown>,
}

/// One `SMSG_PET_SPELLS` cooldown row (`Player.cpp:9713-9730`): a 32-bit spell id (12340),
/// the spell category, then the remaining spell and category cooldowns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetSpellCooldown {
    pub spell_id: u32,
    pub category: u16,
    pub spell_cd_ms: u32,
    pub category_cd_ms: u32,
}

/// Read `SMSG_PET_SPELLS` (`Player::PetSpellInitialize`, `Player.cpp:9664`; client
/// `PetInfo.cpp:860`).
///
/// A zero guid is the 8-byte teardown the client handles by clearing the bar. Any non-zero
/// guid must be followed by the fixed family/duration/state/bar block, the spellbook and the
/// cooldown rows. An impossible count would exhaust the reader and surface as a truncation;
/// leftover bytes are rejected.
pub fn read_pet_spells(body: &[u8]) -> Result<PetSpells> {
    let mut r = Reader::new(body);
    let pet_guid = r.u64()?;

    if pet_guid == 0 {
        if r.remaining() != 0 {
            return Err(DecodeError::trailing(r.remaining()).into());
        }
        return Ok(PetSpells {
            pet_guid: 0,
            creature_family: 0,
            duration_ms: 0,
            state: 0,
            bar: [0; PET_ACTION_SLOTS],
            spells: Vec::new(),
            cooldowns: Vec::new(),
        });
    }

    let creature_family = r.u16()?;
    let duration_ms = r.u32()?;
    let state = r.u32()?;

    let mut bar = [0u32; PET_ACTION_SLOTS];
    for slot in &mut bar {
        *slot = r.u32()?;
    }

    let spell_count = r.u8()?;
    // A `u8` count caps this at 255 rows; the reader still bounds each read.
    let mut spells = Vec::with_capacity(usize::from(spell_count));
    for _ in 0..spell_count {
        spells.push(r.u32()?);
    }

    let cooldown_count = r.u8()?;
    let mut cooldowns = Vec::with_capacity(usize::from(cooldown_count));
    for _ in 0..cooldown_count {
        cooldowns.push(PetSpellCooldown {
            spell_id: r.u32()?,
            category: r.u16()?,
            spell_cd_ms: r.u32()?,
            category_cd_ms: r.u32()?,
        });
    }

    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }

    Ok(PetSpells {
        pet_guid,
        creature_family,
        duration_ms,
        state,
        bar,
        spells,
        cooldowns,
    })
}

/// `SMSG_PET_MODE`: the packed react/command/flags dword for one pet (`Packet_SMSG_PET_MODE`,
/// `PetInfo.cpp:1178-1190`: `u64 pet_guid, u32 mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetMode {
    pub pet_guid: u64,
    pub state: u32,
}

pub fn read_pet_mode(body: &[u8]) -> Result<PetMode> {
    let mut r = Reader::new(body);
    let pet_guid = r.u64()?;
    let state = r.u32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(PetMode { pet_guid, state })
}

/// `SMSG_PET_ACTION_FEEDBACK`: `Unit::SendPetActionFeedback` (`Unit.cpp:13183-13192`) writes
/// exactly one `u8` feedback code.
///
/// Note: the client's handler additionally reads a `u32` spell id for codes 2 and 3
/// (`PetInfo.cpp:417-438`), but the pinned server writer never appends it. This reader follows
/// the writer and rejects any tail.
pub fn read_pet_action_feedback(body: &[u8]) -> Result<u8> {
    let mut r = Reader::new(body);
    let reason = r.u8()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(reason)
}

/// `SMSG_PET_ACTION_SOUND`: `u64 unit_guid, i32 action` (`PetPackets.cpp:54-59`, declared
/// `8 + 4`) as `Packet_SMSG_PET_ACTION_SOUND` reads it (`Unit_C.cpp:17949-17952`).
pub fn read_pet_action_sound(body: &[u8]) -> Result<(u64, i32)> {
    let mut r = Reader::new(body);
    let unit_guid = r.u64()?;
    let action = r.i32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok((unit_guid, action))
}

/// `SMSG_PET_DISMISS_SOUND`: `i32 model_id, f32 x, f32 y, f32 z` (`PetPackets.cpp:61-67`,
/// declared `4 + 12`) as `Packet_SMSG_PET_DISMISS_SOUND` reads it (`Unit_C.cpp:7490-7497`).
pub fn read_pet_dismiss_sound(body: &[u8]) -> Result<(i32, [f32; 3])> {
    let mut r = Reader::new(body);
    let model_id = r.i32()?;
    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok((model_id, [x, y, z]))
}

/// `SMSG_PET_LEARNED_SPELL`: one `u32` spell id (`PetLearnedSpell::Write`,
/// `PetPackets.cpp:42-46`; client `PetInfo.cpp:363-364`).
pub fn read_pet_learned_spell(body: &[u8]) -> Result<u32> {
    read_single_u32(body)
}

/// `SMSG_PET_REMOVED_SPELL`: one `u32` spell id (`PetUnlearnedSpell::Write`,
/// `PetPackets.cpp:48-52`; client `PetInfo.cpp:380-381`).
pub fn read_pet_removed_spell(body: &[u8]) -> Result<u32> {
    read_single_u32(body)
}

/// `SMSG_PET_GUIDS`: `u32 count` then `count * u64` controlled-pet guids
/// (`Packet_SMSG_PET_GUIDS`, `PetInfo.cpp:1071-1086`). Counts above [`MAX_PET_GUIDS`] are
/// rejected instead of driving an allocation.
pub fn read_pet_guids(body: &[u8]) -> Result<Vec<u64>> {
    let mut r = Reader::new(body);
    let count = r.u32()?;
    if count > MAX_PET_GUIDS {
        return Err(DecodeError::invalid_count("pet guid count exceeds client bound")
            .with_value(count)
            .into());
    }
    let mut guids = Vec::with_capacity(count as usize);
    for _ in 0..count {
        guids.push(r.u64()?);
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(guids)
}

/// `SMSG_TALENTS_INFO`: either the player's dual-spec groups or the single pet group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TalentsInfo {
    /// The leading discriminator: `true` for [`Self::pet_talents`], `false` for
    /// [`Self::player`].
    pub pet: bool,
    pub player: Option<PlayerTalents>,
    pub pet_talents: Option<PetTalents>,
}

/// `Player::BuildPlayerTalentsInfoData` (`Player.cpp:14608-14652`): unspent points, the spec
/// count, the active spec index, then each spec's talents and glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerTalents {
    pub unspent_points: u32,
    pub specs: Vec<TalentSpec>,
    pub active_spec: u8,
}

/// One player talent group: `(Talent.dbc id, rank)` pairs plus the six glyph property ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TalentSpec {
    pub talents: Vec<(u32, u8)>,
    pub glyphs: Vec<u16>,
}

/// `Player::BuildPetTalentsInfoData` (`Player.cpp:14654-14724`) as `ReadPetTalents` consumes it
/// (`TalentInfo.cpp:966-984`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetTalents {
    pub unspent_points: u32,
    pub talents: Vec<(u32, u8)>,
}

/// Read `SMSG_TALENTS_INFO` (`Player::SendTalentsInfoData`, `Player.cpp:14726`; client
/// `Packet_SMSG_TALENTS_INFO`, `Player_C.cpp:3534`, then `TalentInfo.cpp:1153/1178`).
///
/// The discriminator selects one of two bodies. Player spec count is capped at
/// [`MAX_TALENT_SPECS`], glyph count at [`MAX_GLYPHS_PER_SPEC`], and an active spec outside a
/// non-empty spec list is rejected. Every other discriminator value is invalid.
pub fn read_talents_info(body: &[u8]) -> Result<TalentsInfo> {
    let mut r = Reader::new(body);
    let talent_type = r.u8()?;

    let info = match talent_type {
        TALENT_TYPE_PLAYER => {
            let unspent_points = r.u32()?;
            let spec_count = r.u8()?;
            let active_spec = r.u8()?;
            if spec_count > MAX_TALENT_SPECS {
                return Err(DecodeError::invalid_count("player spec count exceeds client bound")
                    .with_value(u32::from(spec_count))
                    .into());
            }
            if spec_count > 0 && active_spec >= spec_count {
                return Err(DecodeError::invalid_value("active talent spec is out of range")
                    .with_value(u32::from(active_spec))
                    .into());
            }

            let mut specs = Vec::with_capacity(usize::from(spec_count));
            for _ in 0..spec_count {
                let talent_count = r.u8()?;
                // A `u8` count caps the group at 255 rows; the reader still bounds each read.
                let mut talents = Vec::with_capacity(usize::from(talent_count));
                for _ in 0..talent_count {
                    talents.push((r.u32()?, r.u8()?));
                }

                let glyph_count = r.u8()?;
                if glyph_count > MAX_GLYPHS_PER_SPEC {
                    return Err(
                        DecodeError::invalid_count("glyph count exceeds client bound")
                            .with_value(u32::from(glyph_count))
                            .into(),
                    );
                }
                let mut glyphs = Vec::with_capacity(usize::from(glyph_count));
                for _ in 0..glyph_count {
                    glyphs.push(r.u16()?);
                }

                specs.push(TalentSpec { talents, glyphs });
            }

            TalentsInfo {
                pet: false,
                player: Some(PlayerTalents {
                    unspent_points,
                    specs,
                    active_spec,
                }),
                pet_talents: None,
            }
        }
        TALENT_TYPE_PET => {
            let unspent_points = r.u32()?;
            let talent_count = r.u8()?;
            let mut talents = Vec::with_capacity(usize::from(talent_count));
            for _ in 0..talent_count {
                talents.push((r.u32()?, r.u8()?));
            }
            TalentsInfo {
                pet: true,
                player: None,
                pet_talents: Some(PetTalents {
                    unspent_points,
                    talents,
                }),
            }
        }
        other => {
            return Err(DecodeError::invalid_value("unknown talent info discriminator")
                .with_value(u32::from(other))
                .into());
        }
    };

    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(info)
}

/// `MSG_LIST_STABLED_PETS`: the stable master's reply (`WorldSession::SendStablePet`,
/// `NPCHandler.cpp:348-409`; client `Packet_MSG_LIST_STABLED_PETS`, `StableInfo.cpp:31-71`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StableList {
    pub npc_guid: u64,
    pub stable_slots: u8,
    pub pets: Vec<StabledPet>,
}

/// One stable row. `state` is 1 for the current/active pet and 2 for a stabled/inactive pet
/// (`NPCHandler.cpp:377/402`); the client moves the state-without-`0x2` row to its current slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StabledPet {
    pub pet_number: u32,
    pub creature_entry: u32,
    pub level: u32,
    pub name: String,
    pub state: u8,
}

/// Read `MSG_LIST_STABLED_PETS` (`NPCHandler.cpp:348`).
///
/// The reply is the raw `u64` stable-master guid, the row count, the bought slot count, then
/// each row's numbers, NUL-terminated name and state byte. The row count is capped at
/// [`MAX_STABLED_PETS`] and trailing bytes are rejected.
pub fn read_list_stabled_pets(body: &[u8]) -> Result<StableList> {
    let mut r = Reader::new(body);
    let npc_guid = r.u64()?;
    let pet_count = r.u8()?;
    let stable_slots = r.u8()?;

    if pet_count > MAX_STABLED_PETS {
        return Err(DecodeError::invalid_count("stable pet count exceeds client bound")
            .with_value(u32::from(pet_count))
            .into());
    }

    let mut pets = Vec::with_capacity(usize::from(pet_count));
    for _ in 0..pet_count {
        pets.push(StabledPet {
            pet_number: r.u32()?,
            creature_entry: r.u32()?,
            level: r.u32()?,
            name: r.cstring()?,
            state: r.u8()?,
        });
    }

    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }

    Ok(StableList {
        npc_guid,
        stable_slots,
        pets,
    })
}

/// `SMSG_STABLE_RESULT`: one `u8` `STABLE_*` code (`WorldSession::SendStableResult`,
/// `NPCHandler.cpp:411-416`; client `StableInfo.cpp:75-112`).
pub fn read_stable_result(body: &[u8]) -> Result<u8> {
    let mut r = Reader::new(body);
    let result = r.u8()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(result)
}

/// One `SMSG_LEARNED_SPELL` row: 12340's six-byte `u32 spell_id, u16 slot` body.
///
/// `Player::SendLearnPacket` (`Player.cpp:3056-3064`) always writes the second word as `0`;
/// the field is named for what the client does with it (`AddKnownSpell`'s slot/suppress
/// argument, `Player_C.cpp:4378-4396`). The older shared `u32_body` is too short for 12340.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LearnedSpell {
    pub spell_id: u32,
    pub suppress_or_favorite: u16,
}

pub fn read_learned_spell(body: &[u8]) -> Result<LearnedSpell> {
    let mut r = Reader::new(body);
    let spell_id = r.u32()?;
    let suppress_or_favorite = r.u16()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(LearnedSpell {
        spell_id,
        suppress_or_favorite,
    })
}

/// `SMSG_SUPERCEDED_SPELL`: `u32 old_spell_id, u32 new_spell_id` (`Player.cpp:3098-3102`;
/// client `Player_C.cpp:4403-4414`).
pub fn read_superceded_spell(body: &[u8]) -> Result<(u32, u32)> {
    let mut r = Reader::new(body);
    let old_spell_id = r.u32()?;
    let new_spell_id = r.u32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok((old_spell_id, new_spell_id))
}

/// `SMSG_REMOVED_SPELL`: one `u32` spell id (`Player.cpp:3067-3069`; client
/// `Player_C.cpp:4356-4358`).
pub fn read_removed_spell(body: &[u8]) -> Result<u32> {
    read_single_u32(body)
}

/// A fixed body that is exactly one `u32`.
fn read_single_u32(body: &[u8]) -> Result<u32> {
    let mut r = Reader::new(body);
    let value = r.u32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{classify_decode_failure, DecodeFailure};

    fn push_u16(p: &mut Vec<u8>, value: u16) {
        p.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(p: &mut Vec<u8>, value: u32) {
        p.extend_from_slice(&value.to_le_bytes());
    }

    fn player_talents_body() -> Vec<u8> {
        let mut body = vec![TALENT_TYPE_PLAYER];
        push_u32(&mut body, 7); // unspent points
        body.push(2); // two specs
        body.push(1); // active spec

        // Spec 0: two talents and two glyphs.
        body.push(2);
        push_u32(&mut body, 101);
        body.push(0);
        push_u32(&mut body, 102);
        body.push(3);
        body.push(2);
        push_u16(&mut body, 500);
        push_u16(&mut body, 501);

        // Spec 1: one talent and one glyph.
        body.push(1);
        push_u32(&mut body, 201);
        body.push(4);
        body.push(1);
        push_u16(&mut body, 502);

        body
    }

    fn pet_spells_body() -> Vec<u8> {
        let mut body = Vec::with_capacity(8 + 2 + 4 + 4 + 40 + 1 + 8 + 1 + 14);
        body.extend_from_slice(&0x0000_0000_0000_00ABu64.to_le_bytes());
        push_u16(&mut body, 7); // creature family
        push_u32(&mut body, 45_000); // duration
        push_u32(&mut body, 0x0002_0100); // react 0, command 1, flags 2
        for slot in 0..PET_ACTION_SLOTS {
            push_u32(&mut body, 0x0100_0000 + slot as u32);
        }
        body.push(2); // two spells
        push_u32(&mut body, 111);
        push_u32(&mut body, 222);
        body.push(1); // one cooldown
        push_u32(&mut body, 333); // 32-bit spell id
        push_u16(&mut body, 44); // category
        push_u32(&mut body, 1_500);
        push_u32(&mut body, 2_500);
        body
    }

    #[test]
    fn player_talents_info_reads_every_field() {
        let info = read_talents_info(&player_talents_body()).unwrap();
        assert!(!info.pet);
        assert!(info.pet_talents.is_none());

        let player = info.player.expect("player talent body");
        assert_eq!(player.unspent_points, 7);
        assert_eq!(player.active_spec, 1);
        assert_eq!(player.specs.len(), 2);
        assert_eq!(player.specs[0].talents, vec![(101, 0), (102, 3)]);
        assert_eq!(player.specs[0].glyphs, vec![500, 501]);
        assert_eq!(player.specs[1].talents, vec![(201, 4)]);
        assert_eq!(player.specs[1].glyphs, vec![502]);
    }

    #[test]
    fn pet_talents_info_reads_points_and_ranks() {
        let mut body = vec![TALENT_TYPE_PET];
        push_u32(&mut body, 3);
        body.push(2);
        push_u32(&mut body, 301);
        body.push(0);
        push_u32(&mut body, 302);
        body.push(2);

        let info = read_talents_info(&body).unwrap();
        assert!(info.pet);
        assert!(info.player.is_none());
        let pet = info.pet_talents.expect("pet talent body");
        assert_eq!(pet.unspent_points, 3);
        assert_eq!(pet.talents, vec![(301, 0), (302, 2)]);
    }

    #[test]
    fn pet_spells_reads_family_bar_spells_and_32_bit_cooldown() {
        let spells = read_pet_spells(&pet_spells_body()).unwrap();
        assert_eq!(spells.pet_guid, 0xAB);
        assert_eq!(spells.creature_family, 7);
        assert_eq!(spells.duration_ms, 45_000);
        assert_eq!(spells.state, 0x0002_0100);
        for slot in 0..PET_ACTION_SLOTS {
            assert_eq!(spells.bar[slot], 0x0100_0000 + slot as u32);
        }
        assert_eq!(spells.spells, vec![111, 222]);
        assert_eq!(
            spells.cooldowns,
            vec![PetSpellCooldown {
                spell_id: 333,
                category: 44,
                spell_cd_ms: 1_500,
                category_cd_ms: 2_500,
            }]
        );
    }

    #[test]
    fn pet_spells_zero_guid_is_the_eight_byte_teardown() {
        let teardown = 0u64.to_le_bytes();
        let spells = read_pet_spells(&teardown).unwrap();
        assert_eq!(spells.pet_guid, 0);
        assert_eq!(spells.creature_family, 0);
        assert_eq!(spells.duration_ms, 0);
        assert_eq!(spells.state, 0);
        assert_eq!(spells.bar, [0; PET_ACTION_SLOTS]);
        assert!(spells.spells.is_empty());
        assert!(spells.cooldowns.is_empty());

        let mut with_tail = teardown.to_vec();
        with_tail.push(0);
        assert!(read_pet_spells(&with_tail).is_err());
    }

    #[test]
    fn learned_spell_is_the_six_byte_12340_body() {
        let mut body = Vec::new();
        push_u32(&mut body, 0x000A_BCDE);
        push_u16(&mut body, 0x1234);
        let learned = read_learned_spell(&body).unwrap();
        assert_eq!(
            learned,
            LearnedSpell {
                spell_id: 0x000A_BCDE,
                suppress_or_favorite: 0x1234,
            }
        );

        let five = &body[..5];
        assert!(read_learned_spell(five).is_err());

        let mut seven = body.clone();
        seven.push(0);
        assert!(read_learned_spell(&seven).is_err());
    }

    #[test]
    fn pet_spells_rejects_every_strict_prefix_and_trailing_byte() {
        let body = pet_spells_body();
        for end in 0..body.len() {
            assert!(
                read_pet_spells(&body[..end]).is_err(),
                "pet spells prefix {end} must fail"
            );
        }
        let mut trailing = body;
        trailing.push(0);
        let err = read_pet_spells(&trailing).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::TrailingBytes);
    }

    #[test]
    fn talents_info_rejects_every_strict_prefix_and_trailing_byte() {
        let body = player_talents_body();
        for end in 0..body.len() {
            assert!(
                read_talents_info(&body[..end]).is_err(),
                "talents prefix {end} must fail"
            );
        }
        let mut trailing = body;
        trailing.push(0);
        let err = read_talents_info(&trailing).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::TrailingBytes);

        // An unknown discriminator is a value error, not a truncated player/pet body.
        let err = read_talents_info(&[2]).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidValue);
    }

    #[test]
    fn talent_and_stable_bounds_are_rejected() {
        // Three specs cannot be stored by the two-group client.
        let mut over_specs = vec![TALENT_TYPE_PLAYER];
        push_u32(&mut over_specs, 0);
        over_specs.push(3);
        over_specs.push(0);
        let err = read_talents_info(&over_specs).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidCount);

        // Active spec 2 with only two specs (indices 0 and 1) is out of range.
        let mut bad_active = vec![TALENT_TYPE_PLAYER];
        push_u32(&mut bad_active, 0);
        bad_active.push(1);
        bad_active.push(1);
        let err = read_talents_info(&bad_active).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidValue);

        // More than six glyphs exceeds the client's storage.
        let mut over_glyphs = vec![TALENT_TYPE_PLAYER];
        push_u32(&mut over_glyphs, 0);
        over_glyphs.push(1);
        over_glyphs.push(0);
        over_glyphs.push(0); // no talents
        over_glyphs.push(7); // glyph count
        let err = read_talents_info(&over_glyphs).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidCount);

        // A pet guid count above the conservative bound is rejected before allocation.
        let mut over_guids = Vec::new();
        push_u32(&mut over_guids, MAX_PET_GUIDS + 1);
        let err = read_pet_guids(&over_guids).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidCount);

        // Stable rows are capped at the client's six entry window.
        let mut over_stable = 0x42u64.to_le_bytes().to_vec();
        over_stable.push(MAX_STABLED_PETS + 1);
        over_stable.push(0);
        let err = read_list_stabled_pets(&over_stable).unwrap_err();
        assert_eq!(classify_decode_failure(&err), DecodeFailure::InvalidCount);
    }

    #[test]
    fn stable_list_reads_current_and_stabled_rows() {
        let mut body = Vec::new();
        body.extend_from_slice(&0x0000_0000_0000_0042u64.to_le_bytes());
        body.push(2); // rows
        body.push(4); // bought stable slots
        // Current pet.
        push_u32(&mut body, 1);
        push_u32(&mut body, 1_234);
        push_u32(&mut body, 20);
        body.extend_from_slice(b"Gruff\0");
        body.push(1);
        // Stabled pet.
        push_u32(&mut body, 2);
        push_u32(&mut body, 5_678);
        push_u32(&mut body, 5);
        body.extend_from_slice(b"Cub\0");
        body.push(2);

        let list = read_list_stabled_pets(&body).unwrap();
        assert_eq!(list.npc_guid, 0x42);
        assert_eq!(list.stable_slots, 4);
        assert_eq!(
            list.pets,
            vec![
                StabledPet {
                    pet_number: 1,
                    creature_entry: 1_234,
                    level: 20,
                    name: "Gruff".into(),
                    state: 1,
                },
                StabledPet {
                    pet_number: 2,
                    creature_entry: 5_678,
                    level: 5,
                    name: "Cub".into(),
                    state: 2,
                },
            ]
        );

        let mut trailing = body;
        trailing.push(0);
        assert!(read_list_stabled_pets(&trailing).is_err());

        // An unterminated name must fail instead of swallowing later fields.
        let mut unterminated = 0x42u64.to_le_bytes().to_vec();
        unterminated.push(1);
        unterminated.push(0);
        push_u32(&mut unterminated, 1);
        push_u32(&mut unterminated, 2);
        push_u32(&mut unterminated, 3);
        unterminated.extend_from_slice(b"NoNul");
        assert!(read_list_stabled_pets(&unterminated).is_err());
    }

    #[test]
    fn fixed_small_replies_consume_their_exact_layouts() {
        let mut mode = 0x99u64.to_le_bytes().to_vec();
        push_u32(&mut mode, 0x400);
        assert_eq!(
            read_pet_mode(&mode).unwrap(),
            PetMode {
                pet_guid: 0x99,
                state: 0x400,
            }
        );

        assert_eq!(read_pet_action_feedback(&[3]).unwrap(), 3);
        assert!(read_pet_action_feedback(&[3, 0]).is_err());

        let mut sound = 0x77u64.to_le_bytes().to_vec();
        sound.extend_from_slice(&(-2i32).to_le_bytes());
        assert_eq!(read_pet_action_sound(&sound).unwrap(), (0x77, -2));
        assert!(read_pet_action_sound(&[0; 8]).is_err());

        let mut dismiss = (-9i32).to_le_bytes().to_vec();
        for value in [1.5f32, -2.5, 3.25] {
            dismiss.extend_from_slice(&value.to_le_bytes());
        }
        assert_eq!(
            read_pet_dismiss_sound(&dismiss).unwrap(),
            (-9, [1.5, -2.5, 3.25])
        );

        assert_eq!(read_pet_learned_spell(&[1, 2, 3, 4]).unwrap(), 0x0403_0201);
        assert_eq!(
            read_pet_removed_spell(&[5, 6, 7, 8]).unwrap(),
            0x0807_0605
        );
        assert_eq!(read_stable_result(&[10]).unwrap(), 10);

        let mut superceded = Vec::new();
        push_u32(&mut superceded, 10);
        push_u32(&mut superceded, 20);
        assert_eq!(read_superceded_spell(&superceded).unwrap(), (10, 20));

        assert_eq!(read_removed_spell(&[9, 0, 0, 0]).unwrap(), 9);
        assert!(read_removed_spell(&[9, 0, 0]).is_err());
    }

    #[test]
    fn pet_guids_read_count_then_rows() {
        let mut body = Vec::new();
        push_u32(&mut body, 2);
        body.extend_from_slice(&11u64.to_le_bytes());
        body.extend_from_slice(&22u64.to_le_bytes());
        assert_eq!(read_pet_guids(&body).unwrap(), vec![11, 22]);

        let mut trailing = body;
        trailing.push(0);
        assert!(read_pet_guids(&trailing).is_err());
    }
}
