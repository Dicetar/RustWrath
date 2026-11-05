//! 12340 Wrath-only systems: dungeon finder, battlegrounds/arenas, the calendar, runes and
//! vehicles.
//!
//! Every encoder writes the body a stock 3.3.5a client writes for that opcode and every reader
//! validates one packet body against the pinned AzerothCore build-12340 writer named beside it
//! and the pinned 3.3.5a client decomp. These are 12340 layouts and none of them is the 1.12
//! donor's: `CMSG_LFG_JOIN` carries the role mask as a `u32` plus a fixed three-byte role-check
//! tail, `SMSG_LFG_JOIN_RESULT` is two `u32` words followed by an optional party lock block,
//! `SMSG_LFG_QUEUE_STATUS` puts the average wait before the player's own wait and ends with the
//! queued time *after* the three needed counts, `SMSG_BATTLEFIELD_STATUS` is a tagged union whose
//! tail depends on the status word, `SMSG_CALENDAR_SEND_CALENDAR` sends an event's start as one
//! packed `u32` (not the 1.12 pair), and `SMSG_RESYNC_RUNES` sends per-rune progress bytes rather
//! than millisecond arrays.
//!
//! This module owns no game state and no opcodes: it returns typed rows and body bytes for the
//! app's owners. It rejects truncation, counts the client or server cannot hold, values a
//! record's own contract forbids, and trailing bytes instead of skipping or clamping them.
//!
//! Source anchors (pinned `D:/WOW/WM_BridgeLab/src/azerothcore` at
//! `5e741c37364f4ec5e3bc2860bcf21f13812fb66d` and `D:/Projects/wowdecomp-reference` at
//! `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`):
//!
//! * LFG client packets: `src/server/game/Server/Packets/LFGPackets.cpp:20-34`,
//!   `src/server/game/DungeonFinding/LFG.h:37-76`,
//!   `WoW/Source/Ui/CGLookingForGroup.cpp:158-204`
//! * LFG server packets: `src/server/game/Handlers/LFGHandler.cpp:298-377`, `:437-469`,
//!   `:539-585`; `WoW/Source/Ui/LFGInfo.cpp:683-695`, `:952-1010`, `:1012-1193`, `:824-879`,
//!   `:1366-1379`
//! * battleground client packets: `src/server/game/Handlers/BattleGroundHandler.cpp:74-86`,
//!   `:393-401`, `:670-679`; `WoW/Source/Ui/BattlefieldInfo.cpp:829-873`
//! * battleground server packets: `src/server/game/Battlegrounds/BattlegroundMgr.cpp:196-246`,
//!   `src/server/game/Battlegrounds/Battleground.cpp:128-140`, `:1393-1421`,
//!   `src/server/game/Battlegrounds/Arena.cpp:32-63`,
//!   `WoW/Source/Ui/BattlefieldInfo.cpp:412-542`, `:1617-1694`
//! * calendar: `src/server/game/Handlers/CalendarHandler.cpp:53-186`, `:604-664`,
//!   `src/server/game/Calendar/CalendarMgr.cpp:696-719`,
//!   `src/server/game/Calendar/CalendarMgr.h:126-133`,
//!   `src/server/shared/Packets/ByteBuffer.cpp:137-141`,
//!   `src/server/shared/DataStores/DBCStructure.h:1158-1176`,
//!   `WoW/Source/Ui/Calendar.cpp:3182-3459`, `WoW/Source/Ui/Calendar.h:58-70`
//! * runes: `src/server/game/Entities/Player/Player.cpp:13621-13648`,
//!   `src/server/game/Entities/Player/Player.h:385-401`,
//!   `WoW/Source/Ui/RuneInfo.cpp:349-432`
//! * vehicles: `src/server/game/Handlers/VehicleHandler.cpp:26-141`, `:229-244`,
//!   `src/server/game/Server/WorldSession.cpp:1031-1064`,
//!   `WoW/Source/Object/ObjectClient/UnitVehicle_C.cpp:734-819`

use super::{pack_guid, DecodeError, Reader};
use anyhow::Result;

/// `Array<uint32, 50> Slots` (`LFGPackets.h:34`): the most dungeon slots one `CMSG_LFG_JOIN`
/// can carry, and therefore the most the server ever echoes back in an LFG update.
pub const MAX_LFG_DUNGEONS: usize = 50;

/// `MAXGROUPSIZE` (`Group.h:43`): a dungeon-finder group, and so the most party lock rows
/// `SMSG_LFG_JOIN_RESULT` can carry.
pub const MAX_LFG_PARTY_MEMBERS: usize = 5;

/// `LFGProposal::m_members[5]` (`LFGInfo.h:113`) and the client's own `i >= 5` guard
/// (`LFGInfo.cpp:837`): a proposal covers at most one dungeon group.
pub const MAX_LFG_PROPOSAL_PLAYERS: usize = 5;

/// Ceiling for one player's locked-dungeon list. The list is keyed by `LfgDungeons.dbc` rows
/// (`LFGHandler.cpp:29-37`), so it cannot reach this bound; it exists to reject a corrupt count
/// before allocating.
pub const MAX_LFG_LOCKS: usize = 512;

/// `LFGInfo::m_comment[256]` (`LFGInfo.h:48`): the client's comment buffer, so 255 bytes plus
/// the terminator.
pub const MAX_LFG_COMMENT: usize = 255;

/// Safety ceiling for one player's pending-invite list. The client's node list is growable and the
/// server's only declared invite limit is 100 *invitees per event* (`CALENDAR_MAX_INVITES`,
/// `CalendarMgr.h:130`); a player's own invite list has no such cap, so this sits well above any
/// real list instead of pretending to be one.
pub const MAX_CALENDAR_INVITES: usize = 256;

/// Safety ceiling for one player's event list. The server declares 30 own events and 100 guild
/// events (`CalendarMgr.h:128-129`), and a player can also be invited to others, so the real list
/// can exceed their sum; this ceiling is above every combination the writer can assemble.
pub const MAX_CALENDAR_EVENTS: usize = 512;

/// Ceiling for permanent instance saves. A 3.3.5a player holds one save per raid map and
/// difficulty, well under this bound; the client keeps them in a growable list.
pub const MAX_CALENDAR_LOCKOUTS: usize = 128;

/// Ceiling for raid reset rows. The writer emits one row per raid map
/// (`CalendarHandler.cpp:133-151`), so the list is the raid map count, not a player count.
pub const MAX_CALENDAR_RESETS: usize = 128;

/// Ceiling for dynamic holiday rows. The writer emits the server's `ModifiedHolidays` set
/// (`CalendarHandler.cpp:157-183`), a subset of `Holidays.dbc`.
pub const MAX_CALENDAR_HOLIDAYS: usize = 256;

/// `CalendarDayEvent::m_title[128]` (`Calendar.h:70`): 127 bytes plus the terminator.
pub const MAX_CALENDAR_TITLE: usize = 127;

/// `DYNAMICHOLIDAY::m_textureFileName[64]` (`Calendar.h:59`): 63 bytes plus the terminator.
pub const MAX_CALENDAR_TEXTURE: usize = 63;

/// `MAX_RUNES` (`Player.h:385`): a death knight has six runes. The client stores eight slots
/// (`CGRuneInfo::m_runes[8]`, `RuneInfo.cpp:14`), so six is the server's own bound.
pub const MAX_RUNES: usize = 6;

/// `CGBattlefieldInfo::m_scores[80]` (`BattlefieldInfo.cpp:806-808`): 40 per side.
pub const MAX_PVP_SCORES: usize = 80;

/// The client's per-score objective storage, `BattlefieldScore::m_stats[8]`
/// (`BattlefieldInfo.cpp:1679-1682`). The stock writers send at most five (Alterac Valley,
/// `BattlegroundAV.cpp:31-39`); the client discards anything past its eighth.
pub const MAX_PVP_OBJECTIVES: usize = 8;

/// `CMSG_BATTLEFIELD_PORT`'s fixed marker word (`BattleGroundHandler.cpp:398`, read as "0x1F90
/// constant?"). The stock client always sends it.
pub const BATTLEFIELD_PORT_MARKER: u16 = 0x1F90;

// Movement flag bits the vehicle dismissal body needs (`UnitDefines.h:381-402`).
const MOVEMENT_FLAG_ON_TRANSPORT: u32 = 0x0000_0200;
const MOVEMENT_FLAG_FALLING: u32 = 0x0000_1000;
const MOVEMENT_FLAG_SWIMMING: u32 = 0x0020_0000;
const MOVEMENT_FLAG_FLYING: u32 = 0x0200_0000;
const MOVEMENT_FLAG_SPLINE_ELEVATION: u32 = 0x0400_0000;
const MOVEMENT_FLAG2_ALWAYS_ALLOW_PITCHING: u16 = 0x0020;
const MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT: u16 = 0x0400;

/// Reject bytes after a record's last field.
fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(DecodeError::trailing(r.remaining()).into())
    }
}

/// Reject a declared count outside the layout's fixed bound.
fn bounded(count: usize, max: usize, detail: &'static str) -> Result<usize> {
    if count > max {
        Err(DecodeError::invalid_count(detail).with_value(count as u32).into())
    } else {
        Ok(count)
    }
}

// ---------------------------------------------------------------------------------------------
// Dungeon finder: client packets
// ---------------------------------------------------------------------------------------------

/// One `CMSG_LFG_JOIN` body as `LFGJoin::Read` (`LFGPackets.cpp:20-34`) and the client's own
/// `CGLookingForGroup::JoinLFG` (`CGLookingForGroup.cpp:183-204`) write it.
///
/// The role mask is a 32-bit word even though the server stores it as a `uint8`
/// (`LFGHandler.cpp:73`): the client widens the class-filtered mask
/// (`FilterRolesByClass`, `CGLookingForGroup.cpp:123-126`) into the packet. `slots` carry the
/// dungeon *slot* the client builds as `(id & 0xFFFFFF) | (type << 24)`; the server masks the
/// type back off (`LFGHandler.cpp:65`). `needs` is the three-byte role-check tail the client
/// hardcodes to count 3 (`CGLookingForGroup.cpp:193-197`) and reads as three bytes
/// (`LFGPackets.cpp:29-31`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LfgJoinRequest {
    /// The selected dungeon slots, at most [`MAX_LFG_DUNGEONS`].
    pub slots: Vec<u32>,
    /// The search comment, a NUL-terminated string in the packet.
    pub comment: String,
    /// `PLAYER_ROLE_*` bits (`LFG.h:37-44`), sent as a `u32`.
    pub roles: u8,
    /// The client's "no partial clear" checkbox.
    pub no_partial_clear: bool,
    /// The client's "achievements" checkbox.
    pub achievements: bool,
    /// The three role-check needs; a solo player's are all zero
    /// (`LFGInfo::Reset`, `LFGInfo.cpp:329-331`).
    pub needs: [u8; 3],
}

/// `CMSG_LFG_JOIN` (`LFGHandler.cpp:49-75`, `LFGPackets.cpp:20-34`; client
/// `CGLookingForGroup.cpp:183-204`): `u32 roles, u8 no_partial_clear, u8 achievements,
/// u8 slot_count, u32 slot[slot_count], u8 needs_count (3), u8 needs[3], cstring comment`.
///
/// This is the solo-player form: the three role-check needs are zero and both checkboxes are
/// clear, which is what the client sends before anyone opens the role check. Use
/// [`encode_lfg_join_request`] to carry a real request.
pub fn encode_lfg_join(dungeon_ids: &[u32], comment: &str, roles: u8) -> Vec<u8> {
    encode_lfg_join_request(&LfgJoinRequest {
        slots: dungeon_ids.to_vec(),
        comment: comment.to_owned(),
        roles,
        ..LfgJoinRequest::default()
    })
}

/// The full `CMSG_LFG_JOIN` body. The slot count is a single byte, so a caller must not exceed
/// [`MAX_LFG_DUNGEONS`]; the server's reader cannot hold more either.
pub fn encode_lfg_join_request(request: &LfgJoinRequest) -> Vec<u8> {
    debug_assert!(
        request.slots.len() <= MAX_LFG_DUNGEONS,
        "CMSG_LFG_JOIN carries at most MAX_LFG_DUNGEONS slots"
    );
    let mut p = Vec::with_capacity(12 + request.slots.len() * 4 + request.comment.len());
    p.extend_from_slice(&u32::from(request.roles).to_le_bytes());
    p.push(u8::from(request.no_partial_clear));
    p.push(u8::from(request.achievements));
    p.push(request.slots.len() as u8);
    for slot in &request.slots {
        p.extend_from_slice(&slot.to_le_bytes());
    }
    p.push(3); // needs count: hardcoded to 3 in the client
    p.extend_from_slice(&request.needs);
    p.extend_from_slice(request.comment.as_bytes());
    p.push(0);
    p
}

/// `CMSG_LFG_LEAVE` (`LFGHandler.cpp:77-92`, `LFGPackets.cpp:41-47`; client
/// `CGLookingForGroup::LeaveLFG`, `CGLookingForGroup.cpp:226-241`): empty body.
pub fn encode_lfg_leave() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_LFG_PROPOSAL_RESULT` (`LFGHandler.cpp:94-103`): `u32 proposal_id, bool accept`.
pub fn encode_lfg_proposal_result(proposal_id: u32, accept: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(5);
    p.extend_from_slice(&proposal_id.to_le_bytes());
    p.push(u8::from(accept));
    p
}

/// `CMSG_LFG_SET_COMMENT` (`LFGHandler.cpp:121-130`): one NUL-terminated comment.
pub fn encode_set_lfg_comment(comment: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(comment.len() + 1);
    p.extend_from_slice(comment.as_bytes());
    p.push(0);
    p
}

/// `CMSG_LFG_SET_ROLES` (`LFGHandler.cpp:105-119`): `PLAYER_ROLE_*` bits as one byte.
pub fn encode_lfg_set_roles(roles: u8) -> Vec<u8> {
    vec![roles]
}

// ---------------------------------------------------------------------------------------------
// Dungeon finder: server packets
// ---------------------------------------------------------------------------------------------

/// One party member's locked dungeons inside `SMSG_LFG_JOIN_RESULT`
/// (`BuildPlayerLockDungeonBlock`, `LFGHandler.cpp:29-37`; client `ParseBlackList`,
/// `LFGInfo.cpp:1366-1379`): `u64 guid, u32 lock_count, (u32 dungeon, u32 lock_status)[..]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfgPartyLock {
    pub guid: u64,
    /// `LFG_LOCKSTATUS_*` rows: `(dungeon slot, lock reason)`, at most [`MAX_LFG_LOCKS`].
    pub locks: Vec<(u32, u32)>,
}

/// `LFG_JOIN_PARTY_NOT_MEET_REQS` (`LFGMgr.h:107`): the only join result that carries the party
/// lock block (`LFGMgr.cpp:794-801`).
pub const LFG_JOIN_PARTY_NOT_MEET_REQS: u32 = 6;

/// `SMSG_LFG_JOIN_RESULT` (`SendLfgJoinResult`, `LFGHandler.cpp:437-450`; client
/// `LFGInfo.cpp:952-985` and `ParsePartyInfo`, `LFGInfo.cpp:683-695`).
///
/// The two leading words are `u32` even though the enums behind them are byte-sized: the writer
/// emits `uint32(joinData.result)` and `uint32(joinData.state)`, and the client reads both as
/// `uint32`. The party lock block follows only when the result is
/// [`LFG_JOIN_PARTY_NOT_MEET_REQS`] and the lock map is non-empty (`LFGMgr.cpp:794-801`), so it
/// is an optional tail: the body is either the two words, or the two words plus a non-empty
/// block. Bytes after any other result are rejected rather than read as an empty block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfgJoinResult {
    pub result: u32,
    /// The writer's `joinData.state`, a `LfgRoleCheckState`.
    pub state: u32,
    /// Empty when the writer sent no lock block.
    pub party_lock: Vec<LfgPartyLock>,
}

pub fn read_lfg_join_result(body: &[u8]) -> Result<LfgJoinResult> {
    let mut r = Reader::new(body);
    let result = r.u32()?;
    let state = r.u32()?;
    let mut party_lock = Vec::new();
    if r.remaining() > 0 {
        if result != LFG_JOIN_PARTY_NOT_MEET_REQS {
            return Err(DecodeError::trailing(r.remaining()).into());
        }
        let members = bounded(usize::from(r.u8()?), MAX_LFG_PARTY_MEMBERS, "LFG party lock count")?;
        if members == 0 {
            // The writer only emits the block when the lock map is non-empty.
            return Err(DecodeError::invalid_value("empty LFG party lock block").into());
        }
        party_lock.reserve(members);
        for _ in 0..members {
            let guid = r.u64()?;
            let lock_count = bounded(r.u32()? as usize, MAX_LFG_LOCKS, "LFG dungeon lock count")?;
            let mut locks = Vec::with_capacity(lock_count);
            for _ in 0..lock_count {
                locks.push((r.u32()?, r.u32()?));
            }
            party_lock.push(LfgPartyLock { guid, locks });
        }
    }
    finish(&r)?;
    Ok(LfgJoinResult {
        result,
        state,
        party_lock,
    })
}

/// `SMSG_LFG_QUEUE_STATUS` (`SendLfgQueueStatus`, `LFGHandler.cpp:452-469`; client
/// `LFGInfo.cpp:987-1010`): `u32 dungeon, i32 wait_avg, i32 wait, i32 wait_tank,
/// i32 wait_healer, i32 wait_dps, u8 tanks_needed, u8 healers_needed, u8 dps_needed,
/// u32 queued_time`.
///
/// The writer puts the *average* wait before the player's own wait, and the queued time after
/// the three needed counts; the waits are signed (`-1` is "unknown", the `LfgQueueStatusData`
/// defaults, `LFGMgr.h:306`). There is no single "players needed" field: the three role counts
/// are separate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LfgQueueStatus {
    pub dungeon_id: u32,
    pub wait_time_avg: i32,
    pub wait_time: i32,
    pub wait_time_tank: i32,
    pub wait_time_healer: i32,
    pub wait_time_dps: i32,
    pub tanks_needed: u8,
    pub healers_needed: u8,
    pub dps_needed: u8,
    pub queued_time: u32,
}

pub fn read_lfg_queue_status(body: &[u8]) -> Result<LfgQueueStatus> {
    let mut r = Reader::new(body);
    let status = LfgQueueStatus {
        dungeon_id: r.u32()?,
        wait_time_avg: r.i32()?,
        wait_time: r.i32()?,
        wait_time_tank: r.i32()?,
        wait_time_healer: r.i32()?,
        wait_time_dps: r.i32()?,
        tanks_needed: r.u8()?,
        healers_needed: r.u8()?,
        dps_needed: r.u8()?,
        queued_time: r.u32()?,
    };
    finish(&r)?;
    Ok(status)
}

/// `SMSG_LFG_UPDATE_PLAYER` (`SendLfgUpdatePlayer`, `LFGHandler.cpp:298-333`; client
/// `LFGInfo.cpp:1012-1093`): `u8 update_type, u8 has_data` and, only when `has_data` is set,
/// `u8 queued, u8 no_partial_clear, u8 achievements, u8 dungeon_count, u32 dungeon[..],
/// cstring comment`.
///
/// The writer's second byte is literally `uint8(size > 0)` and gates the whole tail, so a
/// no-dungeon update is exactly two bytes. The client stores that byte as "joined" and the two
/// following zero bytes as its `m_noPartial`/`m_achievements` flags, which is what the field
/// names here follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfgUpdatePlayer {
    pub update_type: u8,
    /// The writer's `uint8(size > 0)`; the client's `joined`.
    pub joined: bool,
    /// Present only when [`LfgUpdatePlayer::joined`] is set.
    pub queued: Option<bool>,
    pub no_partial_clear: Option<bool>,
    pub achievements: Option<bool>,
    pub dungeon_ids: Vec<u32>,
    /// Present only when [`LfgUpdatePlayer::joined`] is set; may be empty.
    pub comment: Option<String>,
}

pub fn read_lfg_update_player(body: &[u8]) -> Result<LfgUpdatePlayer> {
    let mut r = Reader::new(body);
    let update_type = r.u8()?;
    let joined = r.u8()? != 0;
    let mut update = LfgUpdatePlayer {
        update_type,
        joined,
        queued: None,
        no_partial_clear: None,
        achievements: None,
        dungeon_ids: Vec::new(),
        comment: None,
    };
    if joined {
        let queued = r.u8()? != 0;
        let no_partial_clear = r.u8()? != 0;
        let achievements = r.u8()? != 0;
        let count = bounded(usize::from(r.u8()?), MAX_LFG_DUNGEONS, "LFG update dungeon count")?;
        let mut dungeons = Vec::with_capacity(count);
        for _ in 0..count {
            dungeons.push(r.u32()?);
        }
        let comment = r.cstring()?;
        if comment.len() > MAX_LFG_COMMENT {
            return Err(
                DecodeError::invalid_value("LFG comment exceeds the client's 256-byte buffer")
                    .with_value(comment.len() as u32)
                    .into(),
            );
        }
        update.queued = Some(queued);
        update.no_partial_clear = Some(no_partial_clear);
        update.achievements = Some(achievements);
        update.dungeon_ids = dungeons;
        update.comment = Some(comment);
    }
    finish(&r)?;
    Ok(update)
}

/// `SMSG_LFG_UPDATE_PARTY` (`SendLfgUpdateParty`, `LFGHandler.cpp:335-377`; client
/// `LFGInfo.cpp:1096-1193`): `u8 update_type, u8 has_data` and, only when `has_data` is set,
/// `u8 lfg_joined, u8 queued, u8 no_partial_clear, u8 achievements, u8 roles[3],
/// u8 dungeon_count, u32 dungeon[..], cstring comment`.
///
/// The party form has one more leading byte (`lfg_joined`) and the three role-check bytes the
/// player form lacks; the writer sends those roles as zero always (`LFGHandler.cpp:368-369`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfgUpdateParty {
    pub update_type: u8,
    /// The writer's `uint8(size > 0)`; the client's `joined`.
    pub joined: bool,
    /// Present only when [`LfgUpdateParty::joined`] is set.
    pub lfg_joined: Option<bool>,
    pub queued: Option<bool>,
    pub no_partial_clear: Option<bool>,
    pub achievements: Option<bool>,
    /// The three role-check slots; the stock writer sends zero in all three.
    pub roles: Option<[u8; 3]>,
    pub dungeon_ids: Vec<u32>,
    /// Present only when [`LfgUpdateParty::joined`] is set; may be empty.
    pub comment: Option<String>,
}

pub fn read_lfg_update_party(body: &[u8]) -> Result<LfgUpdateParty> {
    let mut r = Reader::new(body);
    let update_type = r.u8()?;
    let joined = r.u8()? != 0;
    let mut update = LfgUpdateParty {
        update_type,
        joined,
        lfg_joined: None,
        queued: None,
        no_partial_clear: None,
        achievements: None,
        roles: None,
        dungeon_ids: Vec::new(),
        comment: None,
    };
    if joined {
        let lfg_joined = r.u8()? != 0;
        let queued = r.u8()? != 0;
        let no_partial_clear = r.u8()? != 0;
        let achievements = r.u8()? != 0;
        let roles = [r.u8()?, r.u8()?, r.u8()?];
        let count = bounded(usize::from(r.u8()?), MAX_LFG_DUNGEONS, "LFG update dungeon count")?;
        let mut dungeons = Vec::with_capacity(count);
        for _ in 0..count {
            dungeons.push(r.u32()?);
        }
        let comment = r.cstring()?;
        if comment.len() > MAX_LFG_COMMENT {
            return Err(
                DecodeError::invalid_value("LFG comment exceeds the client's 256-byte buffer")
                    .with_value(comment.len() as u32)
                    .into(),
            );
        }
        update.lfg_joined = Some(lfg_joined);
        update.queued = Some(queued);
        update.no_partial_clear = Some(no_partial_clear);
        update.achievements = Some(achievements);
        update.roles = Some(roles);
        update.dungeon_ids = dungeons;
        update.comment = Some(comment);
    }
    finish(&r)?;
    Ok(update)
}

/// One proposal member row (`LFGHandler.cpp:566-583`; client `LFGInfo.cpp:836-856`):
/// `u32 role, u8 is_local, u8 in_dungeon, u8 in_same_group, u8 responded, u8 accepted`.
///
/// There is no GUID in this row, so a member cannot be named from the packet alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LfgProposalPlayer {
    /// The `PLAYER_ROLE_*` mask the writer sends as a `u32`.
    pub role: u32,
    /// The receiving player's own row.
    pub is_local: bool,
    /// The member is already inside the dungeon (the writer's "silent" flag).
    pub in_dungeon: bool,
    /// The member shares the receiving player's original group.
    pub in_same_group: bool,
    /// `player.accept != LFG_ANSWER_PENDING`.
    pub responded: bool,
    /// `player.accept == LFG_ANSWER_AGREE`.
    pub accepted: bool,
}

/// `SMSG_LFG_PROPOSAL_UPDATE` (`SendLfgUpdateProposal`, `LFGHandler.cpp:539-585`; client
/// `LFGInfo.cpp:824-879`): `u32 dungeon, u8 state, u32 proposal_id, u32 completed_mask,
/// u8 silent, u8 player_count`, then one [`LfgProposalPlayer`] per member.
///
/// The writer calls the fourth word `encounters`; the client stores it as `m_completedMask` and
/// logs it as a mask, and it is a full 32-bit word in both. `player_count` is bounded by the
/// client's five-member array and its own `i >= 5` guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfgProposalUpdate {
    pub proposal_id: u32,
    pub dungeon_id: u32,
    pub state: u8,
    pub completed_mask: u32,
    /// The writer's `silent`: the client hides the proposal window when set.
    pub silent: bool,
    pub players: Vec<LfgProposalPlayer>,
}

pub fn read_lfg_proposal_update(body: &[u8]) -> Result<LfgProposalUpdate> {
    let mut r = Reader::new(body);
    let dungeon_id = r.u32()?;
    let state = r.u8()?;
    let proposal_id = r.u32()?;
    let completed_mask = r.u32()?;
    let silent = r.u8()? != 0;
    let count = bounded(
        usize::from(r.u8()?),
        MAX_LFG_PROPOSAL_PLAYERS,
        "LFG proposal player count",
    )?;
    let mut players = Vec::with_capacity(count);
    for _ in 0..count {
        players.push(LfgProposalPlayer {
            role: r.u32()?,
            is_local: r.u8()? != 0,
            in_dungeon: r.u8()? != 0,
            in_same_group: r.u8()? != 0,
            responded: r.u8()? != 0,
            accepted: r.u8()? != 0,
        });
    }
    finish(&r)?;
    Ok(LfgProposalUpdate {
        proposal_id,
        dungeon_id,
        state,
        completed_mask,
        silent,
        players,
    })
}

// ---------------------------------------------------------------------------------------------
// Battlegrounds and arenas: client packets
// ---------------------------------------------------------------------------------------------

/// `CMSG_BATTLEMASTER_JOIN` (`HandleBattlemasterJoinOpcode`, `BattleGroundHandler.cpp:74-86`;
/// client `CGBattlefieldInfo::JoinBattlefield`, `BattlefieldInfo.cpp:849-872`):
/// `u64 battlemaster, u32 bg_type, u32 instance_id, u8 join_as_group`.
pub fn encode_battlemaster_join(
    battlemaster: u64,
    bg_type: u32,
    instance_id: u32,
    join_as_group: bool,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(17);
    p.extend_from_slice(&battlemaster.to_le_bytes());
    p.extend_from_slice(&bg_type.to_le_bytes());
    p.extend_from_slice(&instance_id.to_le_bytes());
    p.push(u8::from(join_as_group));
    p
}

/// `CMSG_BATTLEMASTER_JOIN_ARENA` (`HandleBattlemasterJoinArena`, `BattleGroundHandler.cpp:670-715`;
/// client `BattlefieldInfo.cpp:853-869`): `u64 battlemaster, u8 arena_slot, u8 as_group,
/// u8 is_rated`.
///
/// `arena_slot` is the client's 0/1/2 index for 2v2/3v3/5v5, not the `ARENA_TYPE_*` value; the
/// server maps it (`BattleGroundHandler.cpp:701-715`).
pub fn encode_battlemaster_join_arena(
    battlemaster: u64,
    arena_slot: u8,
    as_group: bool,
    is_rated: bool,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(11);
    p.extend_from_slice(&battlemaster.to_le_bytes());
    p.push(arena_slot);
    p.push(u8::from(as_group));
    p.push(u8::from(is_rated));
    p
}

/// `CMSG_BATTLEFIELD_PORT` (`HandleBattleFieldPortOpcode`, `BattleGroundHandler.cpp:393-401`):
/// `u8 arena_type, u8 unk2, u32 bg_type, u16 marker (0x1F90), u8 action`.
///
/// The handler reads exactly these five fields and nothing else. `arena_type` is the arena size
/// (0 for a battleground) and `action` is 1 to enter or 0 to leave the queue. There is no
/// instance id in this packet, unlike `CMSG_BATTLEMASTER_JOIN`.
pub fn encode_battlefield_port(arena_type: u8, unk2: u8, bg_type: u32, action: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.push(arena_type);
    p.push(unk2);
    p.extend_from_slice(&bg_type.to_le_bytes());
    p.extend_from_slice(&BATTLEFIELD_PORT_MARKER.to_le_bytes());
    p.push(action);
    p
}

// ---------------------------------------------------------------------------------------------
// Battlegrounds and arenas: server packets
// ---------------------------------------------------------------------------------------------

/// The tail of `SMSG_BATTLEFIELD_STATUS` for one status word
/// (`BattlegroundMgr::BuildBattlegroundStatusPacket`, `BattlegroundMgr.cpp:225-245`; client
/// `BattlefieldInfo.cpp:467-538`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattlefieldStatusArm {
    /// `STATUS_WAIT_QUEUE` (1): `u32 average_wait_ms, u32 queued_ms`.
    Queued {
        average_wait_ms: u32,
        queued_ms: u32,
    },
    /// `STATUS_WAIT_JOIN` (2): `u32 map_id, u64 join_data, u32 port_expiration_ms`.
    Confirm {
        map_id: u32,
        join_data: u64,
        port_expiration_ms: u32,
    },
    /// `STATUS_IN_PROGRESS` (3): `u32 map_id, u64 join_data, u32 auto_leave_ms,
    /// u32 started_ms, u8 arena_faction`.
    Active {
        map_id: u32,
        join_data: u64,
        auto_leave_ms: u32,
        started_ms: u32,
        arena_faction: u8,
    },
    /// `STATUS_WAIT_LEAVE` (4) and any other word: the writer adds nothing
    /// (`BattlegroundMgr.cpp:243-244`).
    Inactive,
}

/// The queue word and status of one `SMSG_BATTLEFIELD_STATUS`
/// (`BattlegroundMgr.cpp:209-224`; client `BattlefieldInfo.cpp:423-465`).
///
/// The writer composes the client's `uint64 m_queueData` out of four fields: `u8 arena_type`,
/// `u8 arena_flag` (0xE for an arena), `u32 bg_type` and the `u16` marker 0x1F90. The client
/// splits it back the same way and derives its team size from the low seven bits of the first
/// byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BattlefieldQueueData {
    pub arena_type: u8,
    /// The second queue byte: 0x0E for an arena, 0 otherwise.
    pub arena_flag: u8,
    pub bg_type: u32,
    pub min_level: u8,
    pub max_level: u8,
    /// The client-side instance id (`GetClientInstanceID`).
    pub instance_id: u32,
    /// The writer's fixed marker word, 0x1F90 (`BattleGroundHandler.cpp:398` reads the same
    /// constant back out of `CMSG_BATTLEFIELD_PORT`).
    pub marker: u16,
    /// 1 for a rated arena (`BattlegroundMgr.cpp:222`).
    pub rated: bool,
    /// `STATUS_*` word (`Battleground.h:190-194`).
    pub status: u32,
    pub arm: BattlefieldStatusArm,
}

/// `SMSG_BATTLEFIELD_STATUS` (`BattlegroundMgr.cpp:196-246`; client
/// `BattlefieldInfo.cpp:412-542`): `u32 queue_slot` and then either the bare `u64 0` of the
/// "no battleground" arm or the queue word plus its status tail.
///
/// `queue` is [`None`] exactly for the writer's `STATUS_NONE` arm, which sends 12 bytes and ends;
/// every other arm is a full [`BattlefieldQueueData`]. The client rejects a slot index above 1
/// by draining the packet, so a caller must treat slot 2+ as a value it cannot show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BattlefieldStatus {
    pub queue_slot: u32,
    pub queue: Option<BattlefieldQueueData>,
}

pub fn read_battlefield_status(body: &[u8]) -> Result<BattlefieldStatus> {
    let mut r = Reader::new(body);
    let queue_slot = r.u32()?;
    let queue_word = r.u64()?;
    if queue_word == 0 {
        finish(&r)?;
        return Ok(BattlefieldStatus {
            queue_slot,
            queue: None,
        });
    }
    let packed = queue_word.to_le_bytes();
    let arena_type = packed[0];
    let arena_flag = packed[1];
    let bg_type = u32::from_le_bytes([packed[2], packed[3], packed[4], packed[5]]);
    let marker = u16::from_le_bytes([packed[6], packed[7]]);
    let min_level = r.u8()?;
    let max_level = r.u8()?;
    let instance_id = r.u32()?;
    let rated = r.u8()? != 0;
    let status = r.u32()?;
    let arm = match status {
        1 => BattlefieldStatusArm::Queued {
            average_wait_ms: r.u32()?,
            queued_ms: r.u32()?,
        },
        2 => BattlefieldStatusArm::Confirm {
            map_id: r.u32()?,
            join_data: r.u64()?,
            port_expiration_ms: r.u32()?,
        },
        3 => BattlefieldStatusArm::Active {
            map_id: r.u32()?,
            join_data: r.u64()?,
            auto_leave_ms: r.u32()?,
            started_ms: r.u32()?,
            arena_faction: r.u8()?,
        },
        4 => BattlefieldStatusArm::Inactive,
        _ => {
            return Err(DecodeError::invalid_value("unknown battlefield status word")
                .with_value(status)
                .into());
        }
    };
    finish(&r)?;
    Ok(BattlefieldStatus {
        queue_slot,
        queue: Some(BattlefieldQueueData {
            arena_type,
            arena_flag,
            bg_type,
            min_level,
            max_level,
            instance_id,
            marker,
            rated,
            status,
            arm,
        }),
    })
}

/// One `MSG_PVP_LOG_DATA` score row. The two arms are different records: a battleground row is
/// `BattlegroundScore::AppendToPacket` (`Battleground.cpp:128-140`) with honorable kills, deaths
/// and bonus honor, an arena row is `ArenaScore::AppendToPacket` (`Arena.cpp:32-42`) with a team
/// byte instead. Both end with the objectives block `(u32 count, u32 value[count])`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PvpScore {
    pub guid: u64,
    pub killing_blows: u32,
    /// Battleground arm only.
    pub honorable_kills: Option<u32>,
    /// Battleground arm only.
    pub deaths: Option<u32>,
    /// Battleground arm only.
    pub bonus_honor: Option<u32>,
    /// Arena arm only: `PVP_TEAM_*`.
    pub team: Option<u8>,
    pub damage_done: u32,
    pub healing_done: u32,
    /// The per-battleground objectives (`BuildObjectivesBlock`); the arena arm always sends none.
    pub objectives: Vec<u32>,
}

/// The arena-only head of `MSG_PVP_LOG_DATA` (`Arena.cpp:49-63`; client
/// `BattlefieldInfo.cpp:1627-1637`): two teams of `(rating_lost, rating_won, matchmaker_rating)`
/// then two team names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaTail {
    pub ratings: [(u32, u32, u32); 2],
    pub team_names: [String; 2],
}

/// `MSG_PVP_LOG_DATA` (`Battleground::BuildPvPLogDataPacket`, `Battleground.cpp:1393-1421`;
/// client `BattlefieldInfo.cpp:1617-1694`).
///
/// The `arena` byte comes first and gates the rating/name block before the ended/winner pair, so
/// the winner exists only when the battleground has ended. `ended` is the writer's
/// `GetStatus() == STATUS_WAIT_LEAVE` byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PvpLogData {
    /// [`None`] for a battleground, the rating and name block for an arena.
    pub arena: Option<ArenaTail>,
    pub ended: bool,
    /// Present only when [`PvpLogData::ended`] is set.
    pub winner: Option<u8>,
    pub scores: Vec<PvpScore>,
}

pub fn read_pvp_log_data(body: &[u8]) -> Result<PvpLogData> {
    let mut r = Reader::new(body);
    let is_arena = r.u8()? != 0;
    let mut arena = None;
    if is_arena {
        let mut ratings = [(0u32, 0u32, 0u32); 2];
        for row in &mut ratings {
            *row = (r.u32()?, r.u32()?, r.u32()?);
        }
        arena = Some(ArenaTail {
            ratings,
            team_names: [r.cstring()?, r.cstring()?],
        });
    }
    let ended = r.u8()? != 0;
    let winner = if ended { Some(r.u8()?) } else { None };
    let count = bounded(r.u32()? as usize, MAX_PVP_SCORES, "PVP log score count")?;
    let mut scores = Vec::with_capacity(count);
    for _ in 0..count {
        let guid = r.u64()?;
        let killing_blows = r.u32()?;
        let (honorable_kills, deaths, bonus_honor, team) = if is_arena {
            (None, None, None, Some(r.u8()?))
        } else {
            (Some(r.u32()?), Some(r.u32()?), Some(r.u32()?), None)
        };
        let damage_done = r.u32()?;
        let healing_done = r.u32()?;
        let objective_count = bounded(
            r.u32()? as usize,
            MAX_PVP_OBJECTIVES,
            "PVP log objective count",
        )?;
        let mut objectives = Vec::with_capacity(objective_count);
        for _ in 0..objective_count {
            objectives.push(r.u32()?);
        }
        scores.push(PvpScore {
            guid,
            killing_blows,
            honorable_kills,
            deaths,
            bonus_honor,
            team,
            damage_done,
            healing_done,
            objectives,
        });
    }
    finish(&r)?;
    Ok(PvpLogData {
        arena,
        ended,
        winner,
        scores,
    })
}

// ---------------------------------------------------------------------------------------------
// Calendar: client packets
// ---------------------------------------------------------------------------------------------

/// `CMSG_CALENDAR_GET_CALENDAR` (`HandleCalendarGetCalendar`, `CalendarHandler.cpp:53-56`; client
/// `CGCalendar::GetCalendar`, `Calendar.cpp:1997-2003`): empty body.
pub fn encode_calendar_get_calendar() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_CALENDAR_EVENT_SIGNUP` (`HandleCalendarEventSignup`, `CalendarHandler.cpp:604-628`):
/// `u64 event_id, u8 tentative`. Signing up creates the invite, which is how a player joins a
/// guild or public event it was not invited to.
pub fn encode_calendar_event_signup(event_id: u64, tentative: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.extend_from_slice(&event_id.to_le_bytes());
    p.push(u8::from(tentative));
    p
}

/// `CMSG_CALENDAR_EVENT_RSVP` (`HandleCalendarEventRsvp`, `CalendarHandler.cpp:630-664`; client
/// `Calendar.cpp:1984-1992`): `u64 event_id, u64 invite_id, u32 status`.
///
/// `status` is a `CALENDAR_STATUS_*` byte widened to a `u32` by the client
/// (`msg.Put(status)`), matching the server's `uint32 status` read.
pub fn encode_calendar_event_rsvp(event_id: u64, invite_id: u64, status: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(20);
    p.extend_from_slice(&event_id.to_le_bytes());
    p.extend_from_slice(&invite_id.to_le_bytes());
    p.extend_from_slice(&u32::from(status).to_le_bytes());
    p
}

// ---------------------------------------------------------------------------------------------
// Calendar: server packets
// ---------------------------------------------------------------------------------------------

/// One pending-invite row of `SMSG_CALENDAR_SEND_CALENDAR` (`CalendarHandler.cpp:63-81`; client
/// `Calendar.cpp:3186-3214`): `u64 event_id, u64 invite_id, u8 status, u8 rank, u8 invite_type,
/// packed guid inviter`.
///
/// The invite type is the event's own `IsGuildEvent()` when the event still exists, otherwise 0;
/// the guid is the event creator's in the first case and the invite sender's in the second. The
/// client calls the byte `inviteType` and keys the creator display off it. Invite notes/text are
/// not in this packet: they arrive separately in `SMSG_CALENDAR_EVENT_INVITE_NOTES`
/// (`CalendarHandler.cpp:21`), which this server never sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarInvite {
    pub event_id: u64,
    pub invite_id: u64,
    /// `CALENDAR_STATUS_*`.
    pub status: u8,
    /// `CALENDAR_RANK_*`.
    pub rank: u8,
    /// The client's `m_inviteType`; 1 marks a guild event.
    pub invite_type: u8,
    /// The event creator, or the invite sender when the event no longer exists.
    pub inviter: u64,
}

/// One event row of `SMSG_CALENDAR_SEND_CALENDAR` (`CalendarHandler.cpp:85-96`; client
/// `Calendar.cpp:3225-3313`): `u64 event_id, cstring title, u32 type, u32 packed_start,
/// u32 flags, i32 dungeon_id, packed guid creator`.
///
/// The start time is one packed `u32` (`ByteBuffer::AppendPackedTime`,
/// `ByteBuffer.cpp:137-141`), not the 1.12 two-word date. The description, repeat type and
/// invite limit are *not* here; they belong to `SMSG_CALENDAR_SEND_EVENT`
/// (`CalendarMgr.cpp:540-560`). The dungeon id is signed because the writer sends `-1` for an
/// event without a dungeon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    pub event_id: u64,
    pub title: String,
    /// `CALENDAR_TYPE_*`; the writer widens its `uint8` to a word.
    pub event_type: u32,
    /// `AppendPackedTime` of the event start.
    pub time: u32,
    pub flags: u32,
    pub dungeon_id: i32,
    pub creator: u64,
}

/// One permanent instance save in `SMSG_CALENDAR_SEND_CALENDAR`
/// (`CalendarHandler.cpp:101-122`): `u32 map_id, u32 difficulty, u32 time_left, u64 instance_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarRaidLockout {
    pub map_id: u32,
    pub difficulty: u32,
    /// Seconds until the save's reset, `0` when it has already passed.
    pub time_left: u32,
    /// The instance save id, the client's unique copy id.
    pub instance_id: u64,
}

/// One raid reset period row in `SMSG_CALENDAR_SEND_CALENDAR`
/// (`CalendarHandler.cpp:133-154`): `i32 map_id, i32 period, i32 offset`.
///
/// The period is the reset length in seconds and the offset is the writer's always-zero
/// alignment field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarRaidReset {
    pub map_id: i32,
    pub period: i32,
    pub offset: i32,
}

/// One dynamic holiday row in `SMSG_CALENDAR_SEND_CALENDAR` (`CalendarHandler.cpp:157-183`;
/// client `Calendar.cpp:3397-3449`): `u32 id, u32 region, u32 looping, u32 priority,
/// u32 filter_type, u32 dates[26], u32 durations[10], u32 flags[10], cstring texture`.
///
/// The three array lengths are `MAX_HOLIDAY_DATES`/`MAX_HOLIDAY_DURATIONS`/`MAX_HOLIDAY_FLAGS`
/// (`DBCStructure.h:1158-1160`) and the client's own local arrays agree with them. The
/// `filter_type` is a signed `int32` on the server but is written as the same word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarHoliday {
    pub id: u32,
    pub region: u32,
    pub looping: u32,
    pub priority: u32,
    pub filter_type: u32,
    pub dates: [u32; 26],
    pub durations: [u32; 10],
    pub flags: [u32; 10],
    /// The holiday's texture path; the client stores 64 bytes.
    pub texture: String,
}

/// `SMSG_CALENDAR_SEND_CALENDAR` (`HandleCalendarGetCalendar`, `CalendarHandler.cpp:53-186`;
/// client `PH_SMSG_CALENDAR_SEND_CALENDAR`, `Calendar.cpp:3182-3459`).
///
/// The wire order is invites, events, server time, zone time, permanent saves, the reset
/// reference timestamp, raid reset periods, then dynamic holidays; the struct lists its fields
/// in a readable order and the decoder walks the wire order.
///
/// The writer declares the holiday count as the size of the server's modified-holiday set and
/// then *skips* disabled entries inside the loop (`CalendarHandler.cpp:157-165`), so a server
/// with a disabled holiday sends fewer rows than it declares. That is a writer bug this decoder
/// reports as truncation rather than papering over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSendCalendar {
    /// The server's current time as seconds since the epoch (`currTime`).
    pub server_time: u32,
    /// The same instant as one packed `u32` time.
    pub zone_time: u32,
    pub invites: Vec<CalendarInvite>,
    pub events: Vec<CalendarEvent>,
    pub raid_lockouts: Vec<CalendarRaidLockout>,
    /// The reset reference timestamp the client uses to align weekly resets.
    pub relative_time: u32,
    pub raid_resets: Vec<CalendarRaidReset>,
    pub holidays: Vec<CalendarHoliday>,
}

pub fn read_calendar_send_calendar(body: &[u8]) -> Result<CalendarSendCalendar> {
    let mut r = Reader::new(body);

    let invite_count = bounded(
        r.u32()? as usize,
        MAX_CALENDAR_INVITES,
        "calendar invite count",
    )?;
    let mut invites = Vec::with_capacity(invite_count);
    for _ in 0..invite_count {
        invites.push(CalendarInvite {
            event_id: r.u64()?,
            invite_id: r.u64()?,
            status: r.u8()?,
            rank: r.u8()?,
            invite_type: r.u8()?,
            inviter: r.packed_guid()?,
        });
    }

    let event_count = bounded(r.u32()? as usize, MAX_CALENDAR_EVENTS, "calendar event count")?;
    let mut events = Vec::with_capacity(event_count);
    for _ in 0..event_count {
        let event_id = r.u64()?;
        let title = r.cstring()?;
        if title.len() > MAX_CALENDAR_TITLE {
            return Err(
                DecodeError::invalid_value("calendar title exceeds the client's 128-byte buffer")
                    .with_value(title.len() as u32)
                    .into(),
            );
        }
        events.push(CalendarEvent {
            event_id,
            title,
            event_type: r.u32()?,
            time: r.u32()?,
            flags: r.u32()?,
            dungeon_id: r.i32()?,
            creator: r.packed_guid()?,
        });
    }

    let server_time = r.u32()?;
    let zone_time = r.u32()?;

    let lockout_count = bounded(
        r.u32()? as usize,
        MAX_CALENDAR_LOCKOUTS,
        "calendar instance lockout count",
    )?;
    let mut raid_lockouts = Vec::with_capacity(lockout_count);
    for _ in 0..lockout_count {
        raid_lockouts.push(CalendarRaidLockout {
            map_id: r.u32()?,
            difficulty: r.u32()?,
            time_left: r.u32()?,
            instance_id: r.u64()?,
        });
    }

    let relative_time = r.u32()?;

    let reset_count = bounded(
        r.u32()? as usize,
        MAX_CALENDAR_RESETS,
        "calendar raid reset count",
    )?;
    let mut raid_resets = Vec::with_capacity(reset_count);
    for _ in 0..reset_count {
        raid_resets.push(CalendarRaidReset {
            map_id: r.i32()?,
            period: r.i32()?,
            offset: r.i32()?,
        });
    }

    let holiday_count = bounded(
        r.u32()? as usize,
        MAX_CALENDAR_HOLIDAYS,
        "calendar holiday count",
    )?;
    let mut holidays = Vec::with_capacity(holiday_count);
    for _ in 0..holiday_count {
        let id = r.u32()?;
        let region = r.u32()?;
        let looping = r.u32()?;
        let priority = r.u32()?;
        let filter_type = r.u32()?;
        let mut dates = [0u32; 26];
        for date in &mut dates {
            *date = r.u32()?;
        }
        let mut durations = [0u32; 10];
        for duration in &mut durations {
            *duration = r.u32()?;
        }
        let mut flags = [0u32; 10];
        for flag in &mut flags {
            *flag = r.u32()?;
        }
        let texture = r.cstring()?;
        if texture.len() > MAX_CALENDAR_TEXTURE {
            return Err(DecodeError::invalid_value(
                "calendar holiday texture exceeds the client's 64-byte buffer",
            )
            .with_value(texture.len() as u32)
            .into());
        }
        holidays.push(CalendarHoliday {
            id,
            region,
            looping,
            priority,
            filter_type,
            dates,
            durations,
            flags,
            texture,
        });
    }

    finish(&r)?;
    Ok(CalendarSendCalendar {
        server_time,
        zone_time,
        invites,
        events,
        raid_lockouts,
        relative_time,
        raid_resets,
        holidays,
    })
}

/// `SMSG_CALENDAR_COMMAND_RESULT` (`CalendarMgr::SendCalendarCommandResult`,
/// `CalendarMgr.cpp:696-719`): `u32 command, u8 unk, (cstring param | u8 0), u32 error`.
///
/// The writer's two branches are byte-identical when `param` is empty, so the decoder reads one
/// NUL-terminated string in both cases and reports [`None`] for the empty one. Every stock call
/// site passes the default branch except the three errors that name a player
/// (`CalendarMgr.cpp:703-713`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarCommandResult {
    /// The writer's always-zero leading word.
    pub command: u32,
    /// The writer's second always-zero byte.
    pub reserved: u8,
    pub error: u32,
    /// The named player or event, when the writer sent one.
    pub param: Option<String>,
}

pub fn read_calendar_command_result(body: &[u8]) -> Result<CalendarCommandResult> {
    let mut r = Reader::new(body);
    let command = r.u32()?;
    let reserved = r.u8()?;
    let text = r.cstring()?;
    let error = r.u32()?;
    finish(&r)?;
    Ok(CalendarCommandResult {
        command,
        reserved,
        error,
        param: if text.is_empty() { None } else { Some(text) },
    })
}

// ---------------------------------------------------------------------------------------------
// Runes
// ---------------------------------------------------------------------------------------------

/// `SMSG_CONVERT_RUNE` (`Player::ConvertRune`, `Player.cpp:13621-13629`; client
/// `Player_C.cpp:9811-9818`): `u8 rune, u8 rune_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuneConvert {
    /// Rune index, `0..MAX_RUNES`.
    pub rune: u8,
    /// `RuneType`: 0 blood, 1 unholy, 2 frost, 3 death (`Player.h:394-401`).
    pub rune_type: u8,
}

pub fn read_convert_rune(body: &[u8]) -> Result<RuneConvert> {
    let mut r = Reader::new(body);
    let conversion = RuneConvert {
        rune: r.u8()?,
        rune_type: r.u8()?,
    };
    finish(&r)?;
    Ok(conversion)
}

/// One rune row of `SMSG_RESYNC_RUNES` (`Player.cpp:13637-13639`; client
/// `CGRuneInfo::ResyncRunes`, `RuneInfo.cpp:395-415`): the current rune type and the passed
/// share of its cooldown as a byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuneSlot {
    /// `RuneType`: 0 blood, 1 unholy, 2 frost, 3 death.
    pub rune_type: u8,
    /// `255 - cooldown * 51`: the writer's 0..255 progress fraction, not milliseconds.
    pub cooldown: u8,
}

/// `SMSG_RESYNC_RUNES` (`Player::ResyncRunes`, `Player.cpp:13631-13641`; client
/// `RuneInfo.cpp:395-415`): `u32 count` then `(u8 rune_type, u8 cooldown)` per rune.
///
/// The writer sends its `count` argument, which is `MAX_RUNES` (six) for a death knight, and
/// the client stores up to eight (`CGRuneInfo::m_runes[8]`). There are no millisecond arrays in
/// this packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuneState {
    pub runes: Vec<RuneSlot>,
}

pub fn read_resync_runes(body: &[u8]) -> Result<RuneState> {
    let mut r = Reader::new(body);
    let count = bounded(r.u32()? as usize, MAX_RUNES, "rune count")?;
    let mut runes = Vec::with_capacity(count);
    for _ in 0..count {
        runes.push(RuneSlot {
            rune_type: r.u8()?,
            cooldown: r.u8()?,
        });
    }
    finish(&r)?;
    Ok(RuneState { runes })
}

/// `SMSG_ADD_RUNE_POWER` (`Player::AddRunePower`, `Player.cpp:13643-13648`; client
/// `CGRuneInfo::AddRunePower`, `RuneInfo.cpp:419-432`): `u32 mask`, the bit of every rune that
/// became usable. The writer sends `1 << index`, so a single-rune update still carries a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunePower {
    pub mask: u32,
}

pub fn read_add_rune_power(body: &[u8]) -> Result<RunePower> {
    let mut r = Reader::new(body);
    let power = RunePower { mask: r.u32()? };
    finish(&r)?;
    Ok(power)
}

// ---------------------------------------------------------------------------------------------
// Vehicles
// ---------------------------------------------------------------------------------------------

/// The transport block of a 3.3.5 `MovementInfo` (`MovementInfo::TransportInfo`,
/// `Object.h:296-313`; read at `WorldSession.cpp:1038-1048`): a packed guid, the
/// vehicle-local position, the time, the seat and, when
/// `MOVEMENTFLAG2_INTERPOLATED_MOVEMENT` is set, the second time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleTransport {
    pub guid: u64,
    pub position: [f32; 3],
    pub orientation: f32,
    pub time: u32,
    /// `int8` on the server: -1 for "no seat".
    pub seat: i8,
}

/// The `MovementInfo` body a vehicle message carries (`WorldSession::ReadMovementInfo`,
/// `WorldSession.cpp:1031-1064`): `u32 flags, u16 flags2, u32 time, f32 x, f32 y, f32 z,
/// f32 o`, then the transport block when ONTRANSPORT is set, a pitch when swimming/flying or
/// when `MOVEMENTFLAG2_ALWAYS_ALLOW_PITCHING` is set, a `u32 fall_time`, the four-float jump
/// tail when FALLING is set, and a `f32 spline_elevation` when SPLINE_ELEVATION is set.
///
/// The optional tails are derived from the flags word on the wire. [`ON_TRANSPORT`] is set by
/// the encoder exactly when [`Self::transport`] is present; the other three must agree with the
/// flags the caller set, and the encoder refuses a body that would desynchronise the reader.
///
/// [`ON_TRANSPORT`]: VehicleMovementInfo::transport
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleMovementInfo {
    pub flags: u32,
    pub flags2: u16,
    pub time: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub transport: Option<VehicleTransport>,
    /// Written only when SWIMMING or FLYING is set, or `MOVEMENTFLAG2_ALWAYS_ALLOW_PITCHING`.
    pub pitch: Option<f32>,
    pub fall_time: u32,
    /// `[zspeed, sin_angle, cos_angle, xy_speed]`.
    pub jump: Option<[f32; 4]>,
    pub spline_elevation: Option<f32>,
}

fn write_vehicle_movement(p: &mut Vec<u8>, movement: &VehicleMovementInfo) -> Result<()> {
    let mut flags = movement.flags;
    if movement.transport.is_some() {
        flags |= MOVEMENT_FLAG_ON_TRANSPORT;
    } else {
        flags &= !MOVEMENT_FLAG_ON_TRANSPORT;
    }
    let reads_pitch = flags & (MOVEMENT_FLAG_SWIMMING | MOVEMENT_FLAG_FLYING) != 0
        || movement.flags2 & MOVEMENT_FLAG2_ALWAYS_ALLOW_PITCHING != 0;
    if movement.pitch.is_some() != reads_pitch {
        anyhow::bail!(
            "vehicle movement pitch does not match its SWIMMING/FLYING/ALWAYS_ALLOW_PITCHING gate"
        );
    }
    if movement.jump.is_some() != (flags & MOVEMENT_FLAG_FALLING != 0) {
        anyhow::bail!("vehicle movement jump tail does not match MOVEMENTFLAG_FALLING");
    }
    if movement.spline_elevation.is_some() != (flags & MOVEMENT_FLAG_SPLINE_ELEVATION != 0) {
        anyhow::bail!("vehicle movement spline elevation does not match MOVEMENTFLAG_SPLINE_ELEVATION");
    }

    p.extend_from_slice(&flags.to_le_bytes());
    p.extend_from_slice(&movement.flags2.to_le_bytes());
    p.extend_from_slice(&movement.time.to_le_bytes());
    for value in movement.position {
        p.extend_from_slice(&value.to_le_bytes());
    }
    p.extend_from_slice(&movement.orientation.to_le_bytes());
    if let Some(transport) = movement.transport {
        p.extend_from_slice(&pack_guid(transport.guid));
        for value in transport.position {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.extend_from_slice(&transport.orientation.to_le_bytes());
        p.extend_from_slice(&transport.time.to_le_bytes());
        p.push(transport.seat as u8);
        if movement.flags2 & MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT != 0 {
            p.extend_from_slice(&transport.time.to_le_bytes());
        }
    }
    if let Some(pitch) = movement.pitch {
        p.extend_from_slice(&pitch.to_le_bytes());
    }
    p.extend_from_slice(&movement.fall_time.to_le_bytes());
    if let Some(jump) = movement.jump {
        for value in jump {
            p.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Some(elevation) = movement.spline_elevation {
        p.extend_from_slice(&elevation.to_le_bytes());
    }
    Ok(())
}

/// `CMSG_DISMISS_CONTROLLED_VEHICLE` (`HandleDismissControlledVehicle`,
/// `VehicleHandler.cpp:26-59`): `packed guid vehicle` then the current `MovementInfo`.
///
/// The handler reads the packed guid, then `ReadMovementInfo` (`WorldSession.cpp:1031`), then
/// exits the vehicle; it compares the guid against the player's mover. The client sends this
/// through its movement harness (`Movement.cpp:5125`), so the pose is a full movement body, not
/// an empty acknowledge. The encoder returns an error rather than emitting a body whose
/// optional tails disagree with its flags.
pub fn encode_dismiss_controlled_vehicle(
    vehicle: u64,
    movement: &VehicleMovementInfo,
) -> Result<Vec<u8>> {
    let mut p = pack_guid(vehicle);
    write_vehicle_movement(&mut p, movement)?;
    Ok(p)
}

/// `CMSG_REQUEST_VEHICLE_EXIT` (`HandleRequestVehicleExit`, `VehicleHandler.cpp:229-244`; client
/// `CGUnit_C::VehicleExit`, `UnitVehicle_C.cpp:744-747`): empty body.
pub fn encode_request_vehicle_exit() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_REQUEST_VEHICLE_PREV_SEAT` (`HandleChangeSeatsOnControlledVehicle`,
/// `VehicleHandler.cpp:83-85`; client `UnitVehicle_C.cpp:765-768`): empty body. The handler
/// acts on the opcode alone.
pub fn encode_request_vehicle_prev_seat() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_REQUEST_VEHICLE_NEXT_SEAT` (`VehicleHandler.cpp:86-88`; client
/// `UnitVehicle_C.cpp:786-789`): empty body.
pub fn encode_request_vehicle_next_seat() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_REQUEST_VEHICLE_SWITCH_SEAT` (`VehicleHandler.cpp:122-137`; client
/// `CGUnit_C::SwitchToVehicleSeat`, `UnitVehicle_C.cpp:809-818`): `packed guid vehicle,
/// int8 seat`.
///
/// The guid is the vehicle whose seat is wanted, packed, and the seat is signed on the server.
pub fn encode_request_vehicle_switch_seat(vehicle: u64, seat: i8) -> Vec<u8> {
    let mut p = pack_guid(vehicle);
    p.push(seat as u8);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{classify_decode_failure, DecodeFailure};

    fn u16b(value: u16) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn u32b(value: u32) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn i32b(value: i32) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn u64b(value: u64) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn f32b(value: f32) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn cstr(text: &str) -> Vec<u8> {
        let mut out = text.as_bytes().to_vec();
        out.push(0);
        out
    }

    /// The client's `uint64 m_queueData`: `u8 arena_type, u8 flag, u32 bg_type, u16 0x1F90`.
    fn queue_word(arena_type: u8, arena_flag: u8, bg_type: u32) -> u64 {
        u64::from(arena_type)
            | (u64::from(arena_flag) << 8)
            | (u64::from(bg_type) << 16)
            | (u64::from(BATTLEFIELD_PORT_MARKER) << 48)
    }

    /// Every strict prefix of a body must be rejected: each byte is part of a required field.
    fn assert_prefixes_rejected(read: impl Fn(&[u8]) -> Result<()>, body: &[u8]) {
        assert_prefixes_rejected_from(read, body, 0);
    }

    /// The same, for a packet whose optional tail begins at `head`: the `head`-byte prefix is a
    /// complete packet of its own, so it is the one cut that must decode.
    fn assert_prefixes_rejected_from(
        read: impl Fn(&[u8]) -> Result<()>,
        body: &[u8],
        head: usize,
    ) {
        for end in 0..body.len() {
            if head != 0 && end == head {
                continue;
            }
            assert!(
                read(&body[..end]).is_err(),
                "a {end}-byte prefix decoded as a complete packet"
            );
        }
        assert!(read(body).is_ok(), "the whole body did not decode");
    }

    /// An extra byte after a complete record must be reported, not ignored.
    fn assert_trailing_rejected(read: impl Fn(&[u8]) -> Result<()>, body: &[u8]) {
        let mut longer = body.to_vec();
        longer.push(0);
        let error = read(&longer).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::TrailingBytes);
    }

    // -- LFG encoders -------------------------------------------------------------------------

    #[test]
    fn lfg_join_writes_the_12340_field_order() {
        let body = encode_lfg_join(&[0x0300_0010, 0x0400_0020], "hi", 0x06);
        assert_eq!(
            body,
            vec![
                0x06, 0x00, 0x00, 0x00, // roles: a u32, not a byte
                0x00, // no partial clear
                0x00, // achievements
                0x02, // slot count
                0x10, 0x00, 0x00, 0x03, // slot (id | type << 24)
                0x20, 0x00, 0x00, 0x04, // slot
                0x03, // needs count, hardcoded 3
                0x00, 0x00, 0x00, // needs
                b'h', b'i', 0x00, // comment
            ]
        );
    }

    #[test]
    fn lfg_join_request_carries_flags_and_needs() {
        let request = LfgJoinRequest {
            slots: vec![0x0000_00F0],
            comment: String::new(),
            roles: 0x08,
            no_partial_clear: true,
            achievements: true,
            needs: [1, 2, 3],
        };
        let body = encode_lfg_join_request(&request);
        assert_eq!(
            body,
            vec![
                0x08, 0x00, 0x00, 0x00, //
                0x01, 0x01, //
                0x01, //
                0xF0, 0x00, 0x00, 0x00, //
                0x03, //
                0x01, 0x02, 0x03, //
                0x00,
            ]
        );
    }

    #[test]
    fn lfg_small_encoders_match_their_bodies() {
        assert!(encode_lfg_leave().is_empty());
        assert_eq!(
            encode_lfg_proposal_result(0x0102_0304, true),
            vec![0x04, 0x03, 0x02, 0x01, 1]
        );
        assert_eq!(
            encode_lfg_proposal_result(7, false),
            vec![0x07, 0x00, 0x00, 0x00, 0]
        );
        assert_eq!(encode_set_lfg_comment("tank"), vec![b't', b'a', b'n', b'k', 0]);
        assert_eq!(encode_lfg_set_roles(0x06), vec![0x06]);
    }

    // -- LFG readers --------------------------------------------------------------------------

    #[test]
    fn lfg_join_result_ok_has_no_party_tail() {
        let mut body = u32b(0);
        body.extend_from_slice(&u32b(0));
        let result = read_lfg_join_result(&body).unwrap();
        assert_eq!(
            result,
            LfgJoinResult {
                result: 0,
                state: 0,
                party_lock: Vec::new(),
            }
        );
        assert_prefixes_rejected(|b| read_lfg_join_result(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_lfg_join_result(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_join_result_carries_the_party_lock_block() {
        let mut body = u32b(LFG_JOIN_PARTY_NOT_MEET_REQS);
        body.extend_from_slice(&u32b(0));
        body.push(1); // one party member
        body.extend_from_slice(&u64b(0x1011_1213_1415_1617));
        body.extend_from_slice(&u32b(2)); // two locked dungeons
        body.extend_from_slice(&u32b(0x0000_00F0));
        body.extend_from_slice(&u32b(2)); // LFG_LOCKSTATUS_TOO_LOW_LEVEL
        body.extend_from_slice(&u32b(0x0100_00F1));
        body.extend_from_slice(&u32b(6)); // LFG_LOCKSTATUS_RAID_LOCKED

        let result = read_lfg_join_result(&body).unwrap();
        assert_eq!(result.result, 6);
        assert_eq!(
            result.party_lock,
            vec![LfgPartyLock {
                guid: 0x1011_1213_1415_1617,
                locks: vec![(0x0000_00F0, 2), (0x0100_00F1, 6)],
            }]
        );
        assert_prefixes_rejected_from(|b| read_lfg_join_result(b).map(|_| ()), &body, 8);
        assert_trailing_rejected(|b| read_lfg_join_result(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_join_result_rejects_more_than_a_group_of_locks() {
        let mut body = u32b(LFG_JOIN_PARTY_NOT_MEET_REQS);
        body.extend_from_slice(&u32b(0));
        body.push(6); // six party members: impossible for MAXGROUPSIZE
        let error = read_lfg_join_result(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);

        let mut empty = u32b(LFG_JOIN_PARTY_NOT_MEET_REQS);
        empty.extend_from_slice(&u32b(0));
        empty.push(0); // the writer never emits an empty block
        let error = read_lfg_join_result(&empty).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn lfg_queue_status_reads_avg_before_own_wait() {
        let mut body = u32b(7);
        body.extend_from_slice(&i32b(100)); // average
        body.extend_from_slice(&i32b(200)); // own wait
        body.extend_from_slice(&i32b(-1)); // tank
        body.extend_from_slice(&i32b(300)); // healer
        body.extend_from_slice(&i32b(400)); // dps
        body.push(1);
        body.push(2);
        body.push(3);
        body.extend_from_slice(&u32b(1234));

        let status = read_lfg_queue_status(&body).unwrap();
        assert_eq!(
            status,
            LfgQueueStatus {
                dungeon_id: 7,
                wait_time_avg: 100,
                wait_time: 200,
                wait_time_tank: -1,
                wait_time_healer: 300,
                wait_time_dps: 400,
                tanks_needed: 1,
                healers_needed: 2,
                dps_needed: 3,
                queued_time: 1234,
            }
        );
        assert_prefixes_rejected(|b| read_lfg_queue_status(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_lfg_queue_status(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_update_player_without_a_tail_is_two_bytes() {
        let body = vec![7u8, 0];
        let update = read_lfg_update_player(&body).unwrap();
        assert_eq!(
            update,
            LfgUpdatePlayer {
                update_type: 7,
                joined: false,
                queued: None,
                no_partial_clear: None,
                achievements: None,
                dungeon_ids: Vec::new(),
                comment: None,
            }
        );
        assert!(read_lfg_update_player(&body[..1]).is_err());
        assert_trailing_rejected(|b| read_lfg_update_player(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_update_player_carries_the_gated_tail() {
        let mut body = vec![14u8, 1];
        body.push(1); // queued
        body.push(0); // no partial clear
        body.push(1); // achievements
        body.push(2); // two dungeons
        body.extend_from_slice(&u32b(0x0000_00F0));
        body.extend_from_slice(&u32b(0x0300_00F1));
        body.extend_from_slice(&cstr("need a tank"));

        let update = read_lfg_update_player(&body).unwrap();
        assert_eq!(update.update_type, 14);
        assert!(update.joined);
        assert_eq!(update.queued, Some(true));
        assert_eq!(update.no_partial_clear, Some(false));
        assert_eq!(update.achievements, Some(true));
        assert_eq!(update.dungeon_ids, vec![0x0000_00F0, 0x0300_00F1]);
        assert_eq!(update.comment.as_deref(), Some("need a tank"));
        assert_prefixes_rejected(|b| read_lfg_update_player(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_lfg_update_player(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_update_player_empty_comment_is_present_not_absent() {
        let mut body = vec![5u8, 1, 1, 0, 0, 0];
        body.extend_from_slice(&cstr(""));
        let update = read_lfg_update_player(&body).unwrap();
        assert_eq!(update.comment.as_deref(), Some(""));
    }

    #[test]
    fn lfg_update_player_rejects_an_oversize_comment() {
        let mut body = vec![5u8, 1, 1, 0, 0, 0];
        body.extend_from_slice(&cstr(&"x".repeat(MAX_LFG_COMMENT + 1)));
        let error = read_lfg_update_player(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn lfg_update_party_without_a_tail_is_two_bytes() {
        let body = vec![10u8, 0];
        let update = read_lfg_update_party(&body).unwrap();
        assert_eq!(update.update_type, 10);
        assert!(!update.joined);
        assert_eq!(update.roles, None);
        assert_eq!(update.comment, None);
        assert!(read_lfg_update_party(&body[..1]).is_err());
        assert_trailing_rejected(|b| read_lfg_update_party(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_update_party_carries_roles_and_dungeons() {
        let mut body = vec![12u8, 1];
        body.push(1); // lfg joined
        body.push(1); // queued
        body.push(0); // no partial clear
        body.push(0); // achievements
        body.extend_from_slice(&[0, 0, 0]); // the writer's zero role bytes
        body.push(1);
        body.extend_from_slice(&u32b(0x0200_00AA));
        body.extend_from_slice(&cstr(""));

        let update = read_lfg_update_party(&body).unwrap();
        assert_eq!(update.update_type, 12);
        assert!(update.joined);
        assert_eq!(update.lfg_joined, Some(true));
        assert_eq!(update.queued, Some(true));
        assert_eq!(update.roles, Some([0, 0, 0]));
        assert_eq!(update.dungeon_ids, vec![0x0200_00AA]);
        assert_eq!(update.comment.as_deref(), Some(""));
        assert_prefixes_rejected(|b| read_lfg_update_party(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_lfg_update_party(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_proposal_update_reads_two_members_and_roles() {
        let mut body = u32b(0x0000_00F0); // dungeon
        body.push(3); // state
        body.extend_from_slice(&u32b(0x0102_0304)); // proposal id
        body.extend_from_slice(&u32b(6)); // completed mask
        body.push(0); // silent
        body.push(2); // two members
        body.extend_from_slice(&u32b(0x02));
        body.extend_from_slice(&[1, 0, 0, 1, 1]);
        body.extend_from_slice(&u32b(0x08));
        body.extend_from_slice(&[0, 1, 1, 0, 0]);

        let update = read_lfg_proposal_update(&body).unwrap();
        assert_eq!(update.dungeon_id, 0x0000_00F0);
        assert_eq!(update.state, 3);
        assert_eq!(update.proposal_id, 0x0102_0304);
        assert_eq!(update.completed_mask, 6);
        assert!(!update.silent);
        assert_eq!(
            update.players,
            vec![
                LfgProposalPlayer {
                    role: 0x02,
                    is_local: true,
                    in_dungeon: false,
                    in_same_group: false,
                    responded: true,
                    accepted: true,
                },
                LfgProposalPlayer {
                    role: 0x08,
                    is_local: false,
                    in_dungeon: true,
                    in_same_group: true,
                    responded: false,
                    accepted: false,
                },
            ]
        );
        assert_prefixes_rejected(|b| read_lfg_proposal_update(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_lfg_proposal_update(b).map(|_| ()), &body);
    }

    #[test]
    fn lfg_proposal_update_rejects_a_sixth_member() {
        let mut body = u32b(1);
        body.push(2);
        body.extend_from_slice(&u32b(1));
        body.extend_from_slice(&u32b(0));
        body.push(0);
        body.push(6);
        let error = read_lfg_proposal_update(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    // -- battlefields -------------------------------------------------------------------------

    #[test]
    fn battlemaster_join_encoders_write_the_client_bodies() {
        assert_eq!(
            encode_battlemaster_join(0x0102_0304_0506_0708, 32, 0x1122_3344, false),
            [
                vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01],
                u32b(32),
                u32b(0x1122_3344),
                vec![0x00],
            ]
            .concat()
        );
        assert_eq!(
            encode_battlemaster_join_arena(0xAA, 1, true, true),
            vec![0xAA, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1]
        );
    }

    #[test]
    fn battlefield_port_writes_the_marker_word() {
        let body = encode_battlefield_port(0, 1, 30, 1);
        assert_eq!(
            body,
            vec![0x00, 0x01, 0x1E, 0x00, 0x00, 0x00, 0x90, 0x1F, 0x01]
        );
        assert_eq!(&body[6..8], &BATTLEFIELD_PORT_MARKER.to_le_bytes());
    }

    #[test]
    fn battlefield_status_none_is_the_bare_word() {
        let mut body = u32b(1);
        body.extend_from_slice(&u64b(0));
        let status = read_battlefield_status(&body).unwrap();
        assert_eq!(
            status,
            BattlefieldStatus {
                queue_slot: 1,
                queue: None,
            }
        );
        assert_prefixes_rejected(|b| read_battlefield_status(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_battlefield_status(b).map(|_| ()), &body);
    }

    #[test]
    fn battlefield_status_queued_arm() {
        let mut body = u32b(0);
        body.extend_from_slice(&u64b(queue_word(0, 0, 32)));
        body.push(10); // min level
        body.push(19); // max level
        body.extend_from_slice(&u32b(0x1122_3344));
        body.push(0); // not rated
        body.extend_from_slice(&u32b(1)); // STATUS_WAIT_QUEUE
        body.extend_from_slice(&u32b(30_000));
        body.extend_from_slice(&u32b(5_000));

        let status = read_battlefield_status(&body).unwrap();
        let queue = status.queue.unwrap();
        assert_eq!(status.queue_slot, 0);
        assert_eq!(queue.arena_type, 0);
        assert_eq!(queue.arena_flag, 0);
        assert_eq!(queue.bg_type, 32);
        assert_eq!(queue.min_level, 10);
        assert_eq!(queue.max_level, 19);
        assert_eq!(queue.instance_id, 0x1122_3344);
        assert_eq!(queue.marker, BATTLEFIELD_PORT_MARKER);
        assert!(!queue.rated);
        assert_eq!(queue.status, 1);
        assert_eq!(
            queue.arm,
            BattlefieldStatusArm::Queued {
                average_wait_ms: 30_000,
                queued_ms: 5_000,
            }
        );
        assert_prefixes_rejected(|b| read_battlefield_status(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_battlefield_status(b).map(|_| ()), &body);
    }

    #[test]
    fn battlefield_status_confirm_and_active_arms() {
        let mut confirm = u32b(0);
        confirm.extend_from_slice(&u64b(queue_word(0x0E, 0x21, 0)));
        confirm.extend_from_slice(&[10, 19]);
        confirm.extend_from_slice(&u32b(7));
        confirm.push(1); // rated
        confirm.extend_from_slice(&u32b(2));
        confirm.extend_from_slice(&u32b(571)); // map
        confirm.extend_from_slice(&u64b(0));
        confirm.extend_from_slice(&u32b(120_000));
        let status = read_battlefield_status(&confirm).unwrap();
        let queue = status.queue.unwrap();
        assert_eq!(queue.arena_type, 0x0E);
        assert_eq!(queue.arena_flag, 0x21);
        assert_eq!(queue.bg_type, 0);
        assert!(queue.rated);
        assert_eq!(
            queue.arm,
            BattlefieldStatusArm::Confirm {
                map_id: 571,
                join_data: 0,
                port_expiration_ms: 120_000,
            }
        );
        assert_prefixes_rejected(|b| read_battlefield_status(b).map(|_| ()), &confirm);

        let mut active = u32b(0);
        active.extend_from_slice(&u64b(queue_word(0, 0, 1)));
        active.extend_from_slice(&[80, 80]);
        active.extend_from_slice(&u32b(9));
        active.push(0);
        active.extend_from_slice(&u32b(3));
        active.extend_from_slice(&u32b(571));
        active.extend_from_slice(&u64b(0));
        active.extend_from_slice(&u32b(120_000));
        active.extend_from_slice(&u32b(5_000));
        active.push(1);
        let status = read_battlefield_status(&active).unwrap();
        assert_eq!(
            status.queue.unwrap().arm,
            BattlefieldStatusArm::Active {
                map_id: 571,
                join_data: 0,
                auto_leave_ms: 120_000,
                started_ms: 5_000,
                arena_faction: 1,
            }
        );
        assert_prefixes_rejected(|b| read_battlefield_status(b).map(|_| ()), &active);
        assert_trailing_rejected(|b| read_battlefield_status(b).map(|_| ()), &active);
    }

    #[test]
    fn battlefield_status_rejects_an_unknown_status_word() {
        let mut body = u32b(0);
        body.extend_from_slice(&u64b(queue_word(0, 0, 32)));
        body.extend_from_slice(&[10, 19]);
        body.extend_from_slice(&u32b(1));
        body.push(0);
        body.extend_from_slice(&u32b(9));
        let error = read_battlefield_status(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn pvp_log_data_battleground_arm() {
        let mut body = vec![0u8]; // battleground
        body.push(1); // ended
        body.push(1); // winner
        body.extend_from_slice(&u32b(1));
        body.extend_from_slice(&u64b(0xAAAA_BBBB_CCCC_DDDD));
        body.extend_from_slice(&u32b(3)); // killing blows
        body.extend_from_slice(&u32b(4)); // honorable kills
        body.extend_from_slice(&u32b(5)); // deaths
        body.extend_from_slice(&u32b(6)); // bonus honor
        body.extend_from_slice(&u32b(7)); // damage
        body.extend_from_slice(&u32b(8)); // healing
        body.extend_from_slice(&u32b(2)); // objectives
        body.extend_from_slice(&u32b(9));
        body.extend_from_slice(&u32b(10));

        let log = read_pvp_log_data(&body).unwrap();
        assert_eq!(log.arena, None);
        assert!(log.ended);
        assert_eq!(log.winner, Some(1));
        assert_eq!(
            log.scores,
            vec![PvpScore {
                guid: 0xAAAA_BBBB_CCCC_DDDD,
                killing_blows: 3,
                honorable_kills: Some(4),
                deaths: Some(5),
                bonus_honor: Some(6),
                team: None,
                damage_done: 7,
                healing_done: 8,
                objectives: vec![9, 10],
            }]
        );
        assert_prefixes_rejected(|b| read_pvp_log_data(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_pvp_log_data(b).map(|_| ()), &body);
    }

    #[test]
    fn pvp_log_data_arena_tail() {
        let mut body = vec![1u8]; // arena
        body.extend_from_slice(&u32b(1500)); // rating lost
        body.extend_from_slice(&u32b(1516)); // rating won
        body.extend_from_slice(&u32b(1490)); // matchmaker
        body.extend_from_slice(&u32b(1600));
        body.extend_from_slice(&u32b(1600));
        body.extend_from_slice(&u32b(1595));
        body.extend_from_slice(&cstr("Bloodlust"));
        body.extend_from_slice(&cstr("Gladiators"));
        body.push(0); // not ended
        body.extend_from_slice(&u32b(1));
        body.extend_from_slice(&u64b(0x1111_2222_3333_4444));
        body.extend_from_slice(&u32b(2)); // killing blows
        body.push(1); // team
        body.extend_from_slice(&u32b(20_000)); // damage
        body.extend_from_slice(&u32b(1_500)); // healing
        body.extend_from_slice(&u32b(0)); // no objectives in the arena arm

        let log = read_pvp_log_data(&body).unwrap();
        let arena = log.arena.unwrap();
        assert_eq!(arena.ratings[0], (1500, 1516, 1490));
        assert_eq!(arena.ratings[1], (1600, 1600, 1595));
        assert_eq!(arena.team_names[0], "Bloodlust");
        assert_eq!(arena.team_names[1], "Gladiators");
        assert!(!log.ended);
        assert_eq!(log.winner, None);
        let score = &log.scores[0];
        assert_eq!(score.team, Some(1));
        assert_eq!(score.honorable_kills, None);
        assert_eq!(score.deaths, None);
        assert_eq!(score.bonus_honor, None);
        assert_eq!(score.damage_done, 20_000);
        assert_eq!(score.healing_done, 1_500);
        assert!(score.objectives.is_empty());
        assert_prefixes_rejected(|b| read_pvp_log_data(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_pvp_log_data(b).map(|_| ()), &body);
    }

    #[test]
    fn pvp_log_data_rejects_eighty_one_scores() {
        let mut body = vec![0u8, 0];
        body.extend_from_slice(&u32b((MAX_PVP_SCORES as u32) + 1));
        let error = read_pvp_log_data(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    // -- calendar -----------------------------------------------------------------------------

    fn calendar_event_row() -> Vec<u8> {
        let mut row = u64b(0x0000_0000_0000_0005);
        row.extend_from_slice(&cstr("Onyxia"));
        row.extend_from_slice(&u32b(0)); // CALENDAR_TYPE_RAID
        row.extend_from_slice(&u32b(0x0A1B_2C3D)); // packed start
        row.extend_from_slice(&u32b(0x0000_0011)); // flags
        row.extend_from_slice(&i32b(-1)); // no dungeon
        row.extend_from_slice(&pack_guid(0x0200_0000_0000_0001));
        row
    }

    fn calendar_invite_row() -> Vec<u8> {
        let mut row = u64b(0x0000_0000_0000_0005);
        row.extend_from_slice(&u64b(0x0000_0000_0000_0009));
        row.push(1); // CALENDAR_STATUS_ACCEPTED
        row.push(1); // CALENDAR_RANK_MODERATOR
        row.push(1); // guild event
        row.extend_from_slice(&pack_guid(0x0100_0000_0000_0002));
        row
    }

    fn calendar_holiday_row() -> Vec<u8> {
        let mut row = u32b(141); // id
        row.extend_from_slice(&u32b(1)); // region
        row.extend_from_slice(&u32b(0)); // looping
        row.extend_from_slice(&u32b(2)); // priority
        row.extend_from_slice(&u32b(2)); // filter type
        for i in 0..26u32 {
            row.extend_from_slice(&u32b(i));
        }
        for i in 0..10u32 {
            row.extend_from_slice(&u32b(100 + i));
        }
        for i in 0..10u32 {
            row.extend_from_slice(&u32b(200 + i));
        }
        row.extend_from_slice(&cstr("Interface\\Calendar\\Holiday"));
        row
    }

    fn calendar_body() -> Vec<u8> {
        let mut body = u32b(1); // one invite
        body.extend_from_slice(&calendar_invite_row());
        body.extend_from_slice(&u32b(1)); // one event
        body.extend_from_slice(&calendar_event_row());
        body.extend_from_slice(&u32b(1_700_000_000)); // server time
        body.extend_from_slice(&u32b(0x0A1B_2C3D)); // zone time
        body.extend_from_slice(&u32b(1)); // one lockout
        body.extend_from_slice(&u32b(571)); // map
        body.extend_from_slice(&u32b(3)); // difficulty
        body.extend_from_slice(&u32b(3_600)); // time left
        body.extend_from_slice(&u64b(0x0000_0000_0000_00FF)); // instance id
        body.extend_from_slice(&u32b(1_134_028_800)); // relative time
        body.extend_from_slice(&u32b(1)); // one reset
        body.extend_from_slice(&i32b(533)); // Naxxramas
        body.extend_from_slice(&i32b(604_800)); // period
        body.extend_from_slice(&i32b(0)); // offset
        body.extend_from_slice(&u32b(1)); // one holiday
        body.extend_from_slice(&calendar_holiday_row());
        body
    }

    #[test]
    fn calendar_send_calendar_round_trips() {
        let body = calendar_body();
        let calendar = read_calendar_send_calendar(&body).unwrap();
        assert_eq!(calendar.invites.len(), 1);
        assert_eq!(
            calendar.invites[0],
            CalendarInvite {
                event_id: 5,
                invite_id: 9,
                status: 1,
                rank: 1,
                invite_type: 1,
                inviter: 0x0100_0000_0000_0002,
            }
        );
        assert_eq!(calendar.events.len(), 1);
        assert_eq!(calendar.events[0].title, "Onyxia");
        assert_eq!(calendar.events[0].time, 0x0A1B_2C3D);
        assert_eq!(calendar.events[0].dungeon_id, -1);
        assert_eq!(calendar.events[0].creator, 0x0200_0000_0000_0001);
        assert_eq!(calendar.server_time, 1_700_000_000);
        assert_eq!(calendar.zone_time, 0x0A1B_2C3D);
        assert_eq!(
            calendar.raid_lockouts,
            vec![CalendarRaidLockout {
                map_id: 571,
                difficulty: 3,
                time_left: 3_600,
                instance_id: 0xFF,
            }]
        );
        assert_eq!(calendar.relative_time, 1_134_028_800);
        assert_eq!(
            calendar.raid_resets,
            vec![CalendarRaidReset {
                map_id: 533,
                period: 604_800,
                offset: 0,
            }]
        );
        assert_eq!(calendar.holidays.len(), 1);
        assert_eq!(calendar.holidays[0].id, 141);
        assert_eq!(calendar.holidays[0].dates[25], 25);
        assert_eq!(calendar.holidays[0].durations[9], 109);
        assert_eq!(calendar.holidays[0].flags[9], 209);
        assert_eq!(calendar.holidays[0].texture, "Interface\\Calendar\\Holiday");
        assert_prefixes_rejected(|b| read_calendar_send_calendar(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_calendar_send_calendar(b).map(|_| ()), &body);
    }

    #[test]
    fn calendar_send_calendar_accepts_an_empty_calendar() {
        let mut body = u32b(0);
        body.extend_from_slice(&u32b(0));
        body.extend_from_slice(&u32b(1_700_000_000));
        body.extend_from_slice(&u32b(0x0A1B_2C3D));
        body.extend_from_slice(&u32b(0));
        body.extend_from_slice(&u32b(1_134_028_800));
        body.extend_from_slice(&u32b(0));
        body.extend_from_slice(&u32b(0));
        let calendar = read_calendar_send_calendar(&body).unwrap();
        assert!(calendar.invites.is_empty());
        assert!(calendar.events.is_empty());
        assert!(calendar.raid_lockouts.is_empty());
        assert!(calendar.raid_resets.is_empty());
        assert!(calendar.holidays.is_empty());
        assert_prefixes_rejected(|b| read_calendar_send_calendar(b).map(|_| ()), &body);
    }

    #[test]
    fn calendar_send_calendar_rejects_impossible_counts() {
        let mut body = u32b((MAX_CALENDAR_INVITES as u32) + 1);
        let error = read_calendar_send_calendar(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);

        body = u32b(0);
        body.extend_from_slice(&u32b((MAX_CALENDAR_EVENTS as u32) + 1));
        let error = read_calendar_send_calendar(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    #[test]
    fn calendar_send_calendar_rejects_an_oversize_title() {
        let mut body = u32b(0);
        body.extend_from_slice(&u32b(1));
        body.extend_from_slice(&u64b(5));
        body.extend_from_slice(&cstr(&"t".repeat(MAX_CALENDAR_TITLE + 1)));
        let error = read_calendar_send_calendar(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn calendar_command_result_reads_both_writer_branches() {
        let plain = [u32b(0), vec![0u8], vec![0u8], u32b(21)].concat();
        assert_eq!(
            read_calendar_command_result(&plain).unwrap(),
            CalendarCommandResult {
                command: 0,
                reserved: 0,
                error: 21,
                param: None,
            }
        );
        assert_prefixes_rejected(|b| read_calendar_command_result(b).map(|_| ()), &plain);
        assert_trailing_rejected(|b| read_calendar_command_result(b).map(|_| ()), &plain);

        let named = [u32b(0), vec![0u8], cstr("Firna"), u32b(2)].concat();
        assert_eq!(
            read_calendar_command_result(&named).unwrap(),
            CalendarCommandResult {
                command: 0,
                reserved: 0,
                error: 2,
                param: Some("Firna".into()),
            }
        );
        assert_prefixes_rejected(|b| read_calendar_command_result(b).map(|_| ()), &named);
    }

    #[test]
    fn calendar_encoders_match_the_handler_reads() {
        assert!(encode_calendar_get_calendar().is_empty());
        assert_eq!(
            encode_calendar_event_signup(0x0102_0304_0506_0708, true),
            [vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01], vec![1]].concat()
        );
        assert_eq!(
            encode_calendar_event_rsvp(5, 9, 1),
            [
                vec![5, 0, 0, 0, 0, 0, 0, 0],
                vec![9, 0, 0, 0, 0, 0, 0, 0],
                vec![1, 0, 0, 0],
            ]
            .concat()
        );
    }

    // -- runes --------------------------------------------------------------------------------

    #[test]
    fn convert_rune_is_two_bytes() {
        let body = vec![3u8, 2];
        assert_eq!(
            read_convert_rune(&body).unwrap(),
            RuneConvert {
                rune: 3,
                rune_type: 2,
            }
        );
        assert_prefixes_rejected(|b| read_convert_rune(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_convert_rune(b).map(|_| ()), &body);
    }

    #[test]
    fn resync_runes_reads_progress_bytes() {
        let mut body = u32b(2);
        body.extend_from_slice(&[0, 255, 2, 128]);
        let state = read_resync_runes(&body).unwrap();
        assert_eq!(
            state.runes,
            vec![
                RuneSlot {
                    rune_type: 0,
                    cooldown: 255,
                },
                RuneSlot {
                    rune_type: 2,
                    cooldown: 128,
                },
            ]
        );
        assert_prefixes_rejected(|b| read_resync_runes(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_resync_runes(b).map(|_| ()), &body);

        let mut full = u32b(MAX_RUNES as u32);
        for _ in 0..MAX_RUNES {
            full.extend_from_slice(&[1, 0]);
        }
        assert_eq!(read_resync_runes(&full).unwrap().runes.len(), MAX_RUNES);
    }

    #[test]
    fn resync_runes_rejects_a_seventh_rune() {
        let mut body = u32b((MAX_RUNES as u32) + 1);
        body.extend_from_slice(&[0, 0]);
        let error = read_resync_runes(&body).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    #[test]
    fn add_rune_power_carries_a_mask() {
        let body = u32b(1 << 4);
        assert_eq!(read_add_rune_power(&body).unwrap(), RunePower { mask: 16 });
        assert_prefixes_rejected(|b| read_add_rune_power(b).map(|_| ()), &body);
        assert_trailing_rejected(|b| read_add_rune_power(b).map(|_| ()), &body);
    }

    // -- vehicles -----------------------------------------------------------------------------

    #[test]
    fn seat_and_exit_encoders_match_their_bodies() {
        assert!(encode_request_vehicle_exit().is_empty());
        assert!(encode_request_vehicle_prev_seat().is_empty());
        assert!(encode_request_vehicle_next_seat().is_empty());
        assert_eq!(
            encode_request_vehicle_switch_seat(0x0000_0000_0000_00AA, 2),
            vec![0x01, 0xAA, 0x02]
        );
        assert_eq!(
            encode_request_vehicle_switch_seat(0x0102_0304_0506_0708, -1),
            vec![0xFF, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xFF]
        );
    }

    #[test]
    fn dismiss_controlled_vehicle_on_a_passenger_seat() {
        let movement = VehicleMovementInfo {
            flags: 0, // ONTRANSPORT is derived from the transport block
            flags2: 0,
            time: 0x0000_3039,
            position: [1.0, 2.0, 3.0],
            orientation: 0.5,
            transport: Some(VehicleTransport {
                guid: 0x0000_0000_0000_00AA,
                position: [0.25, 0.5, 0.75],
                orientation: 1.5,
                time: 0x0000_3038,
                seat: 3,
            }),
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        };
        let body = encode_dismiss_controlled_vehicle(0x0000_0000_0000_00AA, &movement).unwrap();
        let expected = [
            vec![0x01, 0xAA], // packed vehicle guid
            u32b(MOVEMENT_FLAG_ON_TRANSPORT), // flags, with ONTRANSPORT set by the encoder
            u16b(0),                          // flags2
            u32b(0x0000_3039),                // time
            f32b(1.0),
            f32b(2.0),
            f32b(3.0),
            f32b(0.5),                 // world orientation
            vec![0x01, 0xAA],          // packed transport guid
            f32b(0.25),
            f32b(0.5),
            f32b(0.75),
            f32b(1.5),                 // transport orientation
            u32b(0x0000_3038),         // transport time
            vec![3],                   // seat
            u32b(0),                   // fall time
        ]
        .concat();
        assert_eq!(body, expected);
    }

    #[test]
    fn dismiss_controlled_vehicle_without_a_transport_block() {
        let movement = VehicleMovementInfo {
            flags: MOVEMENT_FLAG_ON_TRANSPORT, // cleared: no transport block is present
            flags2: 0,
            time: 7,
            position: [0.0; 3],
            orientation: 0.0,
            transport: None,
            pitch: None,
            fall_time: 12,
            jump: None,
            spline_elevation: None,
        };
        let body = encode_dismiss_controlled_vehicle(0xAA, &movement).unwrap();
        assert_eq!(&body[..2], &[0x01, 0xAA]);
        assert_eq!(&body[2..6], &0u32.to_le_bytes()); // ONTRANSPORT cleared
        assert_eq!(body.len(), 2 + 4 + 2 + 4 + 16 + 4);
    }

    #[test]
    fn dismiss_controlled_vehicle_refuses_a_desynchronised_tail() {
        let movement = VehicleMovementInfo {
            flags: MOVEMENT_FLAG_FALLING,
            flags2: 0,
            time: 1,
            position: [0.0; 3],
            orientation: 0.0,
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None, // FALLING is set, so the reader expects the four floats
            spline_elevation: None,
        };
        assert!(encode_dismiss_controlled_vehicle(0xAA, &movement).is_err());

        let swims_without_pitch = VehicleMovementInfo {
            flags: MOVEMENT_FLAG_SWIMMING,
            ..VehicleMovementInfo::default()
        };
        assert!(encode_dismiss_controlled_vehicle(0xAA, &swims_without_pitch).is_err());
    }

    #[test]
    fn dismiss_controlled_vehicle_writes_the_jump_tail_when_falling() {
        let movement = VehicleMovementInfo {
            flags: MOVEMENT_FLAG_FALLING,
            flags2: 0,
            time: 1,
            position: [0.0; 3],
            orientation: 0.0,
            transport: None,
            pitch: None,
            fall_time: 250,
            jump: Some([-12.0, 0.6, 0.8, 25.0]),
            spline_elevation: None,
        };
        let body = encode_dismiss_controlled_vehicle(0xAA, &movement).unwrap();
        let expected = [
            vec![0x01, 0xAA],
            u32b(MOVEMENT_FLAG_FALLING),
            u16b(0),
            u32b(1),
            f32b(0.0),
            f32b(0.0),
            f32b(0.0),
            f32b(0.0),
            u32b(250),
            f32b(-12.0),
            f32b(0.6),
            f32b(0.8),
            f32b(25.0),
        ]
        .concat();
        assert_eq!(body, expected);
    }
}
