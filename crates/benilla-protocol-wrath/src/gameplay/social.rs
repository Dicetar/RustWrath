//! 12340 social, group, raid, channel and guild server replies.
//!
//! Every reader validates one packet body against the pinned AzerothCore build-12340 writer
//! named beside it and the pinned 3.3.5a client decomp. These are 12340 layouts: the group list
//! carries a 32-bit sequence counter, the LFG tail is gated by bit `0x8` of the first group-type
//! byte, the party statistics mask ends at the vehicle seat, and `SMSG_WHOIS` is one free-form
//! string rather than the 1.12 structured record. This module owns no game state: it returns
//! typed rows for the app's existing social/group/guild owners, and rejects truncation, counts
//! the client cannot store, values the record's own contract forbids, and trailing bytes instead
//! of skipping or clamping them.
//!
//! Source anchors (pinned `D:/WOW/WM_BridgeLab/src/azerothcore` at
//! `5e741c37364f4ec5e3bc2860bcf21f13812fb66d` and
//! `D:/Projects/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`):
//!
//! * group list: `src/server/game/Groups/Group.cpp:1696-1745`,
//!   `WoW/Source/Object/ObjectClient/Player_C.cpp:11572-11812`
//! * group notices: `WoW/Source/Object/ObjectClient/Player_C.cpp:3287-3315`,
//!   `:4253-4280`, `:4970-4978`, `:5536-5555`
//! * party statistics: `src/server/game/Handlers/GroupHandler.cpp:756-1074`,
//!   `src/server/game/Groups/Group.h:97-122`,
//!   `WoW/Source/Object/ObjectClient/Player_C.cpp:12960-13136`
//! * who/contacts: `src/server/game/Handlers/MiscHandler.cpp:291-422`,
//!   `src/server/game/Entities/Player/SocialMgr.cpp:124-177`,
//!   `:250-292`, `WoW/Source/WowSvcs/WowSvcsClient/FriendList.cpp:161-289`,
//!   `:977-1046`, `:1608-1687`, `:1928-1939`, `:1955-1960`
//! * channels: `src/server/game/Chat/Channels/Channel.cpp:700-722`, `:946-1270`,
//!   `src/server/game/Chat/Channels/Channel.h:30-70`,
//!   `WoW/Source/Ui/ChatFrame.cpp:2464-2846`, `:3355-3396`, `:5489-5545`
//! * guild: `src/server/game/Guilds/Guild.cpp:1205-1250`,
//!   `src/server/game/Server/Packets/GuildPackets.cpp:50-178`, `:194-201`,
//!   `WoW/Source/Object/ObjectClient/Player_C.cpp:5199-5214`, `:5227-5233`,
//!   `:11899-12040`, `WoW/Source/Ui/GuildInfo.cpp:269-356`
//! * raid markers/ready check: `src/server/game/Groups/Group.cpp:1636-1670`,
//!   `src/server/game/Handlers/GroupHandler.cpp:724-739`,
//!   `WoW/Source/Ui/RaidInfo.cpp:386-439`, `:572-576`

use super::{DecodeError, Reader};
use anyhow::Result;

/// The 3.3.5 raid roster size (`WOWGUID raidGuids[40]`, client `Player_C.cpp:11623`); a
/// `SMSG_GROUP_LIST` member count above this cannot be stored.
pub const MAX_GROUP_MEMBERS: u32 = 40;

/// `CGGuildInfo::m_roster` grows to the declared count (`GuildInfo.cpp:275-281`). The client has
/// no fixed guild cap, so this module keeps a conservative ceiling well above real guilds.
pub const MAX_GUILD_MEMBERS: u32 = 1000;

/// `CGGuildInfo::m_rankInfo[10]` (`GuildInfo.h:74`): the client stores ten rank rows.
pub const MAX_GUILD_RANKS: u32 = 10;

/// `GUILD_BANK_MAX_TABS` (`GuildPackets.h:126`): six bank-tab rights per rank.
pub const GUILD_BANK_TABS: usize = 6;

/// The client reads three guild-event strings into `strings[3]` (`Player_C.cpp:11902-11909`).
pub const MAX_GUILD_EVENT_PARAMS: u8 = 3;

/// `TARGETICONCOUNT` (`Group.cpp:1660`): eight raid marker icons.
pub const MAX_RAID_TARGET_ICONS: u8 = 8;

/// `MAX_AURAS_GROUP_UPDATE` (`SpellAuraDefines.h:22`): one aura mask is 64 slots.
pub const MAX_GROUP_AURAS: usize = 64;

/// `GROUPTYPE_LFG` (`Group.h:91`) is the first-byte bit that gates the LFG tail. The client
/// uses the same literal (`groupType & 0x8`, `Player_C.cpp:11586`).
pub const GROUP_TYPE_LFG: u8 = 0x08;

/// `SMSG_GROUP_INVITE` (`GroupHandler.cpp:206-212`, client `Player_C.cpp:4253-4280`): the
/// accept flag, inviter name, an LFG dungeon-type word, the variable dungeon list and a
/// trailing unknown word. The stock server writes an empty dungeon list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupInvite {
    /// 1 means the player may accept; 0 is the "already in a group" notice.
    pub can_accept: bool,
    pub inviter: String,
    /// The client names this `dungeonType`; the stock writer sends 0.
    pub lfg_unknown: u32,
    /// `count` following u32 dungeon ids; the stock writer sends zero rows.
    pub dungeons: Vec<u32>,
    /// The final `unk` word the client reads after the dungeon rows.
    pub unknown: u32,
}

pub fn read_group_invite(body: &[u8]) -> Result<GroupInvite> {
    let mut r = Reader::new(body);
    let can_accept = r.u8()? != 0;
    let inviter = r.cstring()?;
    let lfg_unknown = r.u32()?;
    let count = r.u8()?;
    let mut dungeons = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        dungeons.push(r.u32()?);
    }
    let unknown = r.u32()?;
    finish(&r)?;
    Ok(GroupInvite {
        can_accept,
        inviter,
        lfg_unknown,
        dungeons,
        unknown,
    })
}

/// `SMSG_GROUP_DECLINE` (`Player_C.cpp:3287-3292`): the declining player's name.
pub fn read_group_decline(body: &[u8]) -> Result<String> {
    let mut r = Reader::new(body);
    let name = r.cstring()?;
    finish(&r)?;
    Ok(name)
}

/// `SMSG_GROUP_UNINVITE` (`Player_C.cpp:3296-3301`): empty body.
pub fn read_group_uninvite(body: &[u8]) -> Result<()> {
    empty(body)
}

/// `SMSG_GROUP_SET_LEADER` (`Player_C.cpp:5536-5555`): the new leader's name.
pub fn read_group_set_leader(body: &[u8]) -> Result<String> {
    let mut r = Reader::new(body);
    let name = r.cstring()?;
    finish(&r)?;
    Ok(name)
}

/// `SMSG_GROUP_DESTROYED` (`Player_C.cpp:3306-3315`): empty body.
pub fn read_group_destroyed(body: &[u8]) -> Result<()> {
    empty(body)
}

/// One `SMSG_GROUP_LIST` LFG tail (`Group.cpp:1701-1705`): the LFG state byte and dungeon id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LfgGroupTail {
    pub status: u8,
    pub dungeon_id: u32,
}

/// One non-self group member row (`Group.cpp:1720-1725`): name, full guid, online/PvP state,
/// subgroup, member flags and LFG roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupMember {
    pub name: String,
    pub guid: u64,
    /// `MEMBER_STATUS_*` bits (`Group.h:60-68`), not a boolean.
    pub status: u8,
    pub sub_group: u8,
    pub flags: u8,
    pub roles: u8,
}

/// The group loot/difficulty tail (`Group.cpp:1730-1743`, client `Player_C.cpp:11790-11809`).
/// The client also reads the 3.3 dynamic-difficulty byte, so it is preserved here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupLoot {
    pub method: u8,
    pub master: u64,
    pub threshold: u8,
    pub dungeon_difficulty: u8,
    pub raid_difficulty: u8,
    pub dynamic_difficulty: u8,
}

/// `SMSG_GROUP_LIST` (`Group::SendUpdateToPlayer`, `Group.cpp:1696-1745`; client
/// `Packet_SMSG_GROUP_LIST`, `Player_C.cpp:11572-11812`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupList {
    /// `Group.h:84-94` flags: BG `0x01`, raid `0x02`, LFG-restricted `0x04`, LFG `0x08`.
    pub group_type: u8,
    /// The receiving player's own subgroup.
    pub sub_group: u8,
    /// The receiving player's member flags.
    pub flags: u8,
    /// The receiving player's LFG roles.
    pub roles: u8,
    /// Present only when [`GroupList::group_type`] has [`GROUP_TYPE_LFG`].
    pub lfg: Option<LfgGroupTail>,
    pub group_guid: u64,
    pub counter: u32,
    /// Members other than the receiver; the stock writer sends `GetMembersCount() - 1`.
    pub members: Vec<GroupMember>,
    pub leader_guid: u64,
    /// Present only when at least one other member exists.
    pub loot: Option<GroupLoot>,
}

pub fn read_group_list(body: &[u8]) -> Result<GroupList> {
    let mut r = Reader::new(body);
    let group_type = r.u8()?;
    let sub_group = r.u8()?;
    let flags = r.u8()?;
    let roles = r.u8()?;

    let lfg = if group_type & GROUP_TYPE_LFG != 0 {
        Some(LfgGroupTail {
            status: r.u8()?,
            dungeon_id: r.u32()?,
        })
    } else {
        None
    };

    let group_guid = r.u64()?;
    let counter = r.u32()?;
    let member_count = r.u32()?;
    if member_count > MAX_GROUP_MEMBERS {
        return Err(DecodeError::invalid_count("group member count exceeds 40")
            .with_value(member_count)
            .into());
    }

    let mut members = Vec::with_capacity((member_count as usize).min(r.remaining() / 13));
    for _ in 0..member_count {
        members.push(GroupMember {
            name: r.cstring()?,
            guid: r.u64()?,
            status: r.u8()?,
            sub_group: r.u8()?,
            flags: r.u8()?,
            roles: r.u8()?,
        });
    }

    let leader_guid = r.u64()?;

    let loot = if member_count != 0 {
        Some(GroupLoot {
            method: r.u8()?,
            master: r.u64()?,
            threshold: r.u8()?,
            dungeon_difficulty: r.u8()?,
            raid_difficulty: r.u8()?,
            dynamic_difficulty: r.u8()?,
        })
    } else {
        None
    };

    finish(&r)?;
    Ok(GroupList {
        group_type,
        sub_group,
        flags,
        roles,
        lfg,
        group_guid,
        counter,
        members,
        leader_guid,
        loot,
    })
}

/// `SMSG_PARTY_COMMAND_RESULT` (`GroupHandler.cpp:53-62`; client `Player_C.cpp:4970-4978`):
/// operation, member name, result, then the LFD cooldown/value word the 1.12 command lacks.
pub fn read_party_command_result(body: &[u8]) -> Result<(u32, String, u32, u32)> {
    let mut r = Reader::new(body);
    let operation = r.u32()?;
    let member = r.cstring()?;
    let result = r.u32()?;
    let value = r.u32()?;
    finish(&r)?;
    Ok((operation, member, result, value))
}

/// One aura row inside a party statistics aura mask (`GroupHandler.cpp:832-845`): the slot the
/// mask bit selected, the spell id and the client-side aura flags byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartyAura {
    pub slot: u8,
    pub spell_id: u32,
    pub flags: u8,
}

/// One 64-bit aura mask and exactly the rows whose bits it sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyAuras {
    pub mask: u64,
    pub auras: Vec<PartyAura>,
}

/// The decoded `GROUP_UPDATE_FLAG_*` fields (`Group.h:97-122`). A field is `Some` only when the
/// writer actually emitted it; absent mask bits stay `None` rather than a fabricated default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartyMemberFields {
    /// The raw mask, including bit 31's online-name marker and any zero-width high bits.
    pub mask: u32,
    /// `0x00000001`: `MEMBER_STATUS_*` bits, `u16`.
    pub status: Option<u16>,
    /// `0x00000002`.
    pub cur_hp: Option<u32>,
    /// `0x00000004`.
    pub max_hp: Option<u32>,
    /// `0x00000008`.
    pub power_type: Option<u8>,
    /// `0x00000010`.
    pub cur_power: Option<u16>,
    /// `0x00000020`.
    pub max_power: Option<u16>,
    /// `0x00000040`.
    pub level: Option<u16>,
    /// `0x00000080`.
    pub zone: Option<u16>,
    /// `0x00000100`: `(x, y)` as the client stores them, `u16` each.
    pub position: Option<(u16, u16)>,
    /// `0x00000200`.
    pub auras: Option<PartyAuras>,
    /// `0x00000400`.
    pub pet_guid: Option<u64>,
    /// `0x00000800`.
    pub pet_name: Option<String>,
    /// `0x00001000`.
    pub pet_model_id: Option<u16>,
    /// `0x00002000`.
    pub pet_cur_hp: Option<u32>,
    /// `0x00004000`.
    pub pet_max_hp: Option<u32>,
    /// `0x00008000`.
    pub pet_power_type: Option<u8>,
    /// `0x00010000`.
    pub pet_cur_power: Option<u16>,
    /// `0x00020000`.
    pub pet_max_power: Option<u16>,
    /// `0x00040000`.
    pub pet_auras: Option<PartyAuras>,
    /// `0x00080000`: `VehicleSeat.dbc` id (`Player_C.cpp:13105-13107`).
    pub vehicle_seat: Option<u32>,
}

/// `SMSG_PARTY_MEMBER_STATS` / `SMSG_PARTY_MEMBER_STATS_FULL`
/// (`GroupHandler.cpp:756-1074`; client `Player_C.cpp:12960-13136`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyMemberStats {
    pub guid: u64,
    /// True for the `_FULL` form, which carries a leading arena/background byte.
    pub full: bool,
    /// The `_FULL` form's leading arena flag; `None` for the delta form, where the byte is not
    /// on the wire.
    pub arena: Option<u8>,
    pub fields: PartyMemberFields,
}

pub fn read_party_member_stats(body: &[u8]) -> Result<PartyMemberStats> {
    read_party_member_stats_inner(body, false)
}

pub fn read_party_member_stats_full(body: &[u8]) -> Result<PartyMemberStats> {
    read_party_member_stats_inner(body, true)
}

fn read_party_member_stats_inner(body: &[u8], full: bool) -> Result<PartyMemberStats> {
    let mut r = Reader::new(body);
    // `SMSG_PARTY_MEMBER_STATS_FULL` prepends one arena byte (`GroupHandler.cpp:951/963`,
    // `Player_C.cpp:12961-12965`); the delta form starts at the packed guid.
    let arena = if full { Some(r.u8()?) } else { None };
    let guid = r.packed_guid()?;
    let mask = r.u32()?;
    let mut fields = PartyMemberFields {
        mask,
        ..PartyMemberFields::default()
    };

    if mask & 0x0000_0001 != 0 {
        fields.status = Some(r.u16()?);
    }
    if mask & 0x0000_0002 != 0 {
        fields.cur_hp = Some(r.u32()?);
    }
    if mask & 0x0000_0004 != 0 {
        fields.max_hp = Some(r.u32()?);
    }
    if mask & 0x0000_0008 != 0 {
        fields.power_type = Some(r.u8()?);
    }
    if mask & 0x0000_0010 != 0 {
        fields.cur_power = Some(r.u16()?);
    }
    if mask & 0x0000_0020 != 0 {
        fields.max_power = Some(r.u16()?);
    }
    if mask & 0x0000_0040 != 0 {
        fields.level = Some(r.u16()?);
    }
    if mask & 0x0000_0080 != 0 {
        fields.zone = Some(r.u16()?);
    }
    if mask & 0x0000_0100 != 0 {
        fields.position = Some((r.u16()?, r.u16()?));
    }
    if mask & 0x0000_0200 != 0 {
        fields.auras = Some(read_aura_block(&mut r)?);
    }
    if mask & 0x0000_0400 != 0 {
        fields.pet_guid = Some(r.u64()?);
    }
    if mask & 0x0000_0800 != 0 {
        fields.pet_name = Some(r.cstring()?);
    }
    if mask & 0x0000_1000 != 0 {
        fields.pet_model_id = Some(r.u16()?);
    }
    if mask & 0x0000_2000 != 0 {
        fields.pet_cur_hp = Some(r.u32()?);
    }
    if mask & 0x0000_4000 != 0 {
        fields.pet_max_hp = Some(r.u32()?);
    }
    if mask & 0x0000_8000 != 0 {
        fields.pet_power_type = Some(r.u8()?);
    }
    if mask & 0x0001_0000 != 0 {
        fields.pet_cur_power = Some(r.u16()?);
    }
    if mask & 0x0002_0000 != 0 {
        fields.pet_max_power = Some(r.u16()?);
    }
    if mask & 0x0004_0000 != 0 {
        fields.pet_auras = Some(read_aura_block(&mut r)?);
    }
    if mask & 0x0008_0000 != 0 {
        fields.vehicle_seat = Some(r.u32()?);
    }

    // Bits 20-30 and bit 31 carry no bytes in the 3.3.5 client (`Player_C.cpp:13005-13009` and
    // the absence of any further `msg->Get`); the raw mask above preserves them for the app.
    finish(&r)?;
    Ok(PartyMemberStats {
        guid,
        full,
        arena,
        fields,
    })
}

fn read_aura_block(r: &mut Reader<'_>) -> Result<PartyAuras> {
    let mask = r.u64()?;
    let mut auras = Vec::with_capacity(mask.count_ones() as usize);
    for slot in 0..MAX_GROUP_AURAS as u8 {
        if mask & (1u64 << slot) != 0 {
            auras.push(PartyAura {
                slot,
                spell_id: r.u32()?,
                flags: r.u8()?,
            });
        }
    }
    Ok(PartyAuras { mask, auras })
}

/// One `SMSG_WHO` row (`MiscHandler.cpp:407-413`, client `FriendList.cpp:998-1012`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoPlayer {
    pub name: String,
    pub guild: String,
    pub level: u32,
    pub class: u32,
    pub race: u32,
    pub zone: u32,
    pub gender: u8,
}

/// `SMSG_WHO` (`MiscHandler.cpp:291-422`, client `FriendList.cpp:977-1046`): displayed count,
/// matching count, then that many rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoResults {
    pub shown: u32,
    pub total: u32,
    pub players: Vec<WhoPlayer>,
}

pub fn read_who(body: &[u8]) -> Result<WhoResults> {
    let mut r = Reader::new(body);
    let shown = r.u32()?;
    let total = r.u32()?;
    // A row's smallest encoding is two one-byte cstrings plus 17 fixed bytes.
    let mut players = Vec::with_capacity((shown as usize).min(r.remaining() / 19));
    for _ in 0..shown {
        players.push(WhoPlayer {
            name: r.cstring()?,
            guild: r.cstring()?,
            level: r.u32()?,
            class: r.u32()?,
            race: r.u32()?,
            gender: r.u8()?,
            zone: r.u32()?,
        });
    }
    finish(&r)?;
    Ok(WhoResults {
        shown,
        total,
        players,
    })
}

/// `SMSG_WHOIS` (`MiscHandler.cpp:1141-1145`, client `FriendList.cpp:1955-1960`): the 3.3.5
/// packet is one human-readable cstring. There is no structured player row and no guild tail;
/// the 1.12 `WhoisResult` shape does not exist in 12340.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoisResult {
    pub text: String,
}

pub fn read_whois(body: &[u8]) -> Result<WhoisResult> {
    let mut r = Reader::new(body);
    let text = r.cstring()?;
    finish(&r)?;
    Ok(WhoisResult { text })
}

/// The friend tail inside `SMSG_CONTACT_LIST` (`SocialMgr.cpp:162-170`): the status byte is
/// always present for a friend entry, and area/level/class only when that status is non-zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendInfo {
    pub status: u8,
    pub area: Option<u32>,
    pub level: Option<u32>,
    pub class: Option<u32>,
}

/// One `SMSG_CONTACT_LIST` row (`SocialMgr.cpp:159-171`, client `FriendList.cpp:186-289`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub guid: u64,
    /// `SOCIAL_FLAG_FRIEND` `0x1`, `SOCIAL_FLAG_IGNORED` `0x2`, `SOCIAL_FLAG_MUTED` `0x4`.
    pub flags: u32,
    pub note: String,
    /// Present only for a friend entry; its `status` is always read, the rest conditionally.
    pub friend: Option<FriendInfo>,
}

/// `SMSG_CONTACT_LIST` (`SocialMgr.cpp:124-177`, client `FriendList.cpp:161-289`): the request
/// mask, the contact count, then the rows. Ignored and muted rows are preserved, not filtered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactList {
    pub flags: u32,
    pub contacts: Vec<Contact>,
}

pub fn read_contact_list(body: &[u8]) -> Result<ContactList> {
    let mut r = Reader::new(body);
    let flags = r.u32()?;
    let count = r.u32()?;
    let mut contacts = Vec::with_capacity((count as usize).min(r.remaining() / 13));
    for _ in 0..count {
        let guid = r.u64()?;
        let entry_flags = r.u32()?;
        let note = r.cstring()?;
        let friend = if entry_flags & 0x1 != 0 {
            let status = r.u8()?;
            if status != 0 {
                Some(FriendInfo {
                    status,
                    area: Some(r.u32()?),
                    level: Some(r.u32()?),
                    class: Some(r.u32()?),
                })
            } else {
                Some(FriendInfo {
                    status,
                    area: None,
                    level: None,
                    class: None,
                })
            }
        } else {
            None
        };
        contacts.push(Contact {
            guid,
            flags: entry_flags,
            note,
            friend,
        });
    }
    finish(&r)?;
    Ok(ContactList { flags, contacts })
}

/// `SMSG_FRIEND_STATUS` (`SocialMgr.cpp:250-292`, client `FriendList.cpp:1608-1687`,
/// `:1928-1939`). The result selects the tail: `FRIEND_ADDED_OFFLINE`/`FRIEND_ADDED_ONLINE`
/// append the note, and `FRIEND_ONLINE`/`FRIEND_ADDED_ONLINE` append the online status block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendStatus {
    pub result: u8,
    pub guid: u64,
    /// The contact note, for result 6 (`FRIEND_ADDED_ONLINE`) or 7 (`FRIEND_ADDED_OFFLINE`).
    pub note: Option<String>,
    /// The online block, for result 2 (`FRIEND_ONLINE`) or 6.
    pub online: Option<FriendInfo>,
}

pub fn read_friend_status(body: &[u8]) -> Result<FriendStatus> {
    const FRIEND_ONLINE: u8 = 2;
    const FRIEND_ADDED_ONLINE: u8 = 6;
    const FRIEND_ADDED_OFFLINE: u8 = 7;

    let mut r = Reader::new(body);
    let result = r.u8()?;
    let guid = r.u64()?;
    let note = if result == FRIEND_ADDED_ONLINE || result == FRIEND_ADDED_OFFLINE {
        Some(r.cstring()?)
    } else {
        None
    };
    let online = if result == FRIEND_ONLINE || result == FRIEND_ADDED_ONLINE {
        Some(FriendInfo {
            status: r.u8()?,
            area: Some(r.u32()?),
            level: Some(r.u32()?),
            class: Some(r.u32()?),
        })
    } else {
        None
    };
    finish(&r)?;
    Ok(FriendStatus {
        result,
        guid,
        note,
        online,
    })
}

/// The tail of `SMSG_CHANNEL_NOTIFY`, selected by its notice byte
/// (`Channel.h:30-70`, client `ChatFrame.cpp:2489-2829`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelNotifyTail {
    /// Notices with no body after the channel name.
    None,
    /// Notices carrying a single full guid.
    Guid { guid: u64 },
    /// `CHAT_YOU_JOINED_NOTICE` (`0x02`): channel flags, zone-channel id, client channel flags,
    /// and an optional trailing display string the client reads when bytes remain.
    Joined {
        flags: u8,
        zone_channel_id: u32,
        channel_flags: u32,
        extra: Option<String>,
    },
    /// `CHAT_YOU_LEFT_NOTICE` (`0x03`): zone-channel id and the suspended/constant byte.
    Left {
        zone_channel_id: u32,
        suspended: u8,
    },
    /// A player name string (`CHAT_PLAYER_NOT_FOUND`, `CHAT_CHANNEL_OWNER`, and the
    /// invited/not-banned notices).
    PlayerName { name: String },
    /// A player/actor guid pair (`CHAT_PLAYER_KICKED`, `_BANNED`, `_UNBANNED`).
    Kicked { player: u64, actor: u64 },
    /// `CHAT_MODE_CHANGE_NOTICE` (`0x0C`): guid, old rights byte, new rights byte.
    ModeChange {
        guid: u64,
        old_flags: u8,
        new_flags: u8,
    },
}

/// `SMSG_CHANNEL_NOTIFY` (`Channel.cpp:946-1270`, client `ChatFrame.cpp:2464-2846`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelNotify {
    pub notice: u8,
    pub channel: String,
    pub tail: ChannelNotifyTail,
}

pub fn read_channel_notify(body: &[u8]) -> Result<ChannelNotify> {
    let mut r = Reader::new(body);
    let notice = r.u8()?;
    let channel = r.cstring()?;
    let tail = match notice {
        // 0x00 JOINED, 0x01 LEFT, 0x07 PASSWORD_CHANGED, 0x08 OWNER_CHANGED, 0x0D..0x10
        // announcements/moderation, 0x17 already-member, 0x18 invite, 0x22/0x23 voice,
        // and 0x24 (handled by the client, unassigned in the pinned server enum).
        0x00 | 0x01 | 0x07 | 0x08 | 0x0D | 0x0E | 0x0F | 0x10 | 0x17 | 0x18 | 0x22 | 0x23
        | 0x24 => ChannelNotifyTail::Guid { guid: r.u64()? },
        // 0x02 YOU_JOINED: flags, zone channel id, client channel flags, optional display text.
        0x02 => {
            let flags = r.u8()?;
            let zone_channel_id = r.u32()?;
            let channel_flags = r.u32()?;
            let extra = if r.remaining() > 0 {
                Some(r.cstring()?)
            } else {
                None
            };
            ChannelNotifyTail::Joined {
                flags,
                zone_channel_id,
                channel_flags,
                extra,
            }
        }
        // 0x03 YOU_LEFT: zone channel id and the constant/suspended byte.
        0x03 => ChannelNotifyTail::Left {
            zone_channel_id: r.u32()?,
            suspended: r.u8()?,
        },
        // 0x04 wrong password, 0x05 not member, 0x06 not moderator, 0x0A not owner,
        // 0x11 muted, 0x13 banned, 0x19 wrong faction, 0x1A wrong faction, 0x1B invalid name,
        // 0x1C not moderated, 0x1F throttled, 0x20 not in area, 0x21 not in LFG.
        0x04 | 0x05 | 0x06 | 0x0A | 0x11 | 0x13 | 0x19 | 0x1A | 0x1B | 0x1C | 0x1F | 0x20
        | 0x21 => ChannelNotifyTail::None,
        // 0x09 player-not-found, 0x0B channel owner, 0x16 player-not-banned,
        // 0x1D player invited, 0x1E player invite banned.
        0x09 | 0x0B | 0x16 | 0x1D | 0x1E => ChannelNotifyTail::PlayerName {
            name: r.cstring()?,
        },
        // 0x0C mode change: guid plus old and new rights bytes.
        0x0C => ChannelNotifyTail::ModeChange {
            guid: r.u64()?,
            old_flags: r.u8()?,
            new_flags: r.u8()?,
        },
        // 0x12 kicked, 0x14 banned, 0x15 unbanned: the affected player then the actor.
        0x12 | 0x14 | 0x15 => ChannelNotifyTail::Kicked {
            player: r.u64()?,
            actor: r.u64()?,
        },
        _ => {
            return Err(DecodeError::invalid_value("unknown channel notice")
                .with_value(u32::from(notice))
                .into())
        }
    };
    finish(&r)?;
    Ok(ChannelNotify {
        notice,
        channel,
        tail,
    })
}

/// `SMSG_CHANNEL_LIST` (`Channel.cpp:700-722`, client `ChatFrame.cpp:3355-3396` and
/// `:5489-5545`). Both client branches read one type/display byte, the channel name, the
/// channel flags, a `u32` member count and `(guid, member flags)` rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelList {
    /// The leading type/display byte; 0 selects the joined-channel branch and 1 the display
    /// channel branch, but both share this row layout.
    pub channel_type: u8,
    pub channel: String,
    pub flags: u8,
    pub members: Vec<(u64, u8)>,
}

pub fn read_channel_list(body: &[u8]) -> Result<ChannelList> {
    let mut r = Reader::new(body);
    let channel_type = r.u8()?;
    let channel = r.cstring()?;
    let flags = r.u8()?;
    let count = r.u32()?;
    let mut members = Vec::with_capacity((count as usize).min(r.remaining() / 9));
    for _ in 0..count {
        members.push((r.u64()?, r.u8()?));
    }
    finish(&r)?;
    Ok(ChannelList {
        channel_type,
        channel,
        flags,
        members,
    })
}

/// `SMSG_GUILD_INFO` (`GuildPackets.cpp:50-58`, client `Player_C.cpp:5199-5214`): guild name,
/// packed creation time, member count and account count. 12340 has no MOTD or guild-info text
/// here; those travel in the roster (`SMSG_GUILD_ROSTER`) and the MOTD guild event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildInfoWire {
    pub name: String,
    pub created_at: u32,
    pub member_count: u32,
    pub account_count: u32,
}

pub fn read_guild_info(body: &[u8]) -> Result<GuildInfoWire> {
    let mut r = Reader::new(body);
    let name = r.cstring()?;
    let created_at = r.u32()?;
    let member_count = r.u32()?;
    let account_count = r.u32()?;
    finish(&r)?;
    Ok(GuildInfoWire {
        name,
        created_at,
        member_count,
        account_count,
    })
}

/// One `SMSG_GUILD_ROSTER` member row (`GuildPackets.cpp:103-120`,
/// client `GuildInfo.cpp:307-346`). 12340 carries no per-member bank-tab block; bank rights are
/// per rank in [`GuildRankWire`].
#[derive(Clone, Debug, PartialEq)]
pub struct GuildMemberWire {
    pub guid: u64,
    /// `status != 0` in the client (`GuildInfo.cpp:312`).
    pub online: bool,
    /// The raw status byte the client keeps in `m_status`.
    pub status: u8,
    pub name: String,
    pub rank: u32,
    pub level: u32,
    pub class: u32,
    pub gender: u8,
    pub zone: u32,
    pub note: String,
    pub officer_note: String,
    /// Days since last login; present only for an offline row (`GuildPackets.cpp:113-114`).
    pub last_online_days: Option<f32>,
}

/// One `SMSG_GUILD_ROSTER` rank row (`GuildPackets.cpp:194-201`, client
/// `GuildInfo.cpp:292-301`): rights, daily gold limit, then six bank-tab rights and their
/// per-day withdrawal limits. The 12340 writer emits both six-word arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuildRankWire {
    pub rights: u32,
    pub gold_limit_per_day: u32,
    pub bank_tab_flags: [u32; GUILD_BANK_TABS],
    pub bank_tab_withdraw_limits: [u32; GUILD_BANK_TABS],
}

/// `SMSG_GUILD_ROSTER` (`GuildPackets.cpp:60-74`, client `GuildInfo.cpp:269-356`).
#[derive(Clone, Debug, PartialEq)]
pub struct GuildRosterWire {
    pub motd: String,
    pub guild_info: String,
    pub member_count: u32,
    pub members: Vec<GuildMemberWire>,
    pub ranks: Vec<GuildRankWire>,
}

pub fn read_guild_roster(body: &[u8]) -> Result<GuildRosterWire> {
    let mut r = Reader::new(body);
    let member_count = r.u32()?;
    if member_count > MAX_GUILD_MEMBERS {
        return Err(DecodeError::invalid_count("guild member count exceeds 1000")
            .with_value(member_count)
            .into());
    }
    let motd = r.cstring()?;
    let guild_info = r.cstring()?;
    let rank_count = r.u32()?;
    if rank_count > MAX_GUILD_RANKS {
        return Err(DecodeError::invalid_count("guild rank count exceeds 10")
            .with_value(rank_count)
            .into());
    }

    let mut ranks = Vec::with_capacity(rank_count as usize);
    for _ in 0..rank_count {
        let mut bank_tab_flags = [0u32; GUILD_BANK_TABS];
        let mut bank_tab_withdraw_limits = [0u32; GUILD_BANK_TABS];
        let rights = r.u32()?;
        let gold_limit_per_day = r.u32()?;
        for tab in 0..GUILD_BANK_TABS {
            bank_tab_flags[tab] = r.u32()?;
            bank_tab_withdraw_limits[tab] = r.u32()?;
        }
        ranks.push(GuildRankWire {
            rights,
            gold_limit_per_day,
            bank_tab_flags,
            bank_tab_withdraw_limits,
        });
    }

    // A member row's smallest encoding is the guid, status, one-byte name, rank, the level /
    // class / gender bytes, zone and two one-byte notes.
    let mut members = Vec::with_capacity((member_count as usize).min(r.remaining() / 23));
    for _ in 0..member_count {
        let guid = r.u64()?;
        let status = r.u8()?;
        let online = status != 0;
        let name = r.cstring()?;
        let rank = r.u32()?;
        let level = u32::from(r.u8()?);
        let class = u32::from(r.u8()?);
        let gender = r.u8()?;
        let zone = r.u32()?;
        let last_online_days = if online { None } else { Some(r.f32()?) };
        let note = r.cstring()?;
        let officer_note = r.cstring()?;
        members.push(GuildMemberWire {
            guid,
            online,
            status,
            name,
            rank,
            level,
            class,
            gender,
            zone,
            note,
            officer_note,
            last_online_days,
        });
    }

    finish(&r)?;
    Ok(GuildRosterWire {
        motd,
        guild_info,
        member_count,
        members,
        ranks,
    })
}

/// `SMSG_GUILD_EVENT` (`GuildPackets.cpp:122-142`, client `Player_C.cpp:11899-12040`): the
/// event byte, a string count, that many strings, and a guid only for JOINED (3), LEFT (4),
/// SIGNED_ON (12) and SIGNED_OFF (13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildEventWire {
    pub event: u8,
    pub args: Vec<String>,
    pub guid: Option<u64>,
}

pub fn read_guild_event(body: &[u8]) -> Result<GuildEventWire> {
    let mut r = Reader::new(body);
    let event = r.u8()?;
    let count = r.u8()?;
    if count > MAX_GUILD_EVENT_PARAMS {
        return Err(DecodeError::invalid_count("guild event has more than three strings")
            .with_value(u32::from(count))
            .into());
    }
    let mut args = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        args.push(r.cstring()?);
    }
    let guid = match event {
        3 | 4 | 12 | 13 => Some(r.u64()?),
        _ => None,
    };
    finish(&r)?;
    Ok(GuildEventWire { event, args, guid })
}

/// `SMSG_GUILD_COMMAND_RESULT` (`GuildPackets.cpp:81-88`, client `Player_C.cpp:5227-5233`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildCommandResultWire {
    pub command: u32,
    pub name: String,
    pub error: u32,
}

pub fn read_guild_command_result(body: &[u8]) -> Result<GuildCommandResultWire> {
    let mut r = Reader::new(body);
    let command = r.u32()?;
    let name = r.cstring()?;
    let error = r.u32()?;
    finish(&r)?;
    Ok(GuildCommandResultWire {
        command,
        name,
        error,
    })
}

/// `MSG_RAID_TARGET_UPDATE` (`Group.cpp:1644-1669`, client `RaidInfo.cpp:386-428`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaidTargetUpdate {
    /// The marker delta: the setter's guid and the one `(icon, target)` pair the writer emits.
    Set { setter: u64, icon: u8, guid: u64 },
    /// The full marker list; entries run to the end of the packet.
    List { entries: Vec<(u8, u64)> },
}

pub fn read_raid_target_update(body: &[u8]) -> Result<RaidTargetUpdate> {
    let mut r = Reader::new(body);
    match r.u8()? {
        0 => {
            let setter = r.u64()?;
            let icon = r.u8()?;
            let guid = r.u64()?;
            if icon >= MAX_RAID_TARGET_ICONS {
                return Err(DecodeError::invalid_value("raid target icon out of range")
                    .with_value(u32::from(icon))
                    .into());
            }
            finish(&r)?;
            Ok(RaidTargetUpdate::Set { setter, icon, guid })
        }
        1 => {
            let mut entries = Vec::new();
            while r.remaining() > 0 {
                let icon = r.u8()?;
                let guid = r.u64()?;
                if icon >= MAX_RAID_TARGET_ICONS {
                    return Err(DecodeError::invalid_value("raid target icon out of range")
                        .with_value(u32::from(icon))
                        .into());
                }
                entries.push((icon, guid));
            }
            if entries.len() > usize::from(MAX_RAID_TARGET_ICONS) {
                return Err(DecodeError::invalid_count("raid target list exceeds eight icons")
                    .with_value(entries.len() as u32)
                    .into());
            }
            Ok(RaidTargetUpdate::List { entries })
        }
        other => Err(DecodeError::invalid_value("raid target update kind is neither 0 nor 1")
            .with_value(u32::from(other))
            .into()),
    }
}

/// `MSG_RAID_READY_CHECK` (request) and `MSG_RAID_READY_CHECK_CONFIRM` (answer)
/// (`GroupHandler.cpp:724-739`, client `RaidInfo.cpp:432-439` and `:572-576`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaidReadyCheck {
    /// The request carries only the initiating player's guid.
    Request { initiator: u64 },
    /// The confirmation carries the answering player's guid and the ready byte.
    Answer { guid: u64, ready: u8 },
}

/// Read either ready-check form by its wire length. The request is exactly the initiator guid
/// (8 bytes); the confirmation is that guid plus the ready byte (9 bytes), which the pinned
/// server always writes. An 8-byte confirmation cannot be distinguished from a request, so the
/// 8-byte form is the request.
pub fn read_raid_ready_check(body: &[u8]) -> Result<RaidReadyCheck> {
    let mut r = Reader::new(body);
    let guid = r.u64()?;
    if r.remaining() == 0 {
        return Ok(RaidReadyCheck::Request { initiator: guid });
    }
    if r.remaining() == 1 {
        let ready = r.u8()?;
        return Ok(RaidReadyCheck::Answer { guid, ready });
    }
    Err(DecodeError::trailing(r.remaining()).into())
}

/// Reject any body a zero-body notice left behind.
fn empty(body: &[u8]) -> Result<()> {
    if body.is_empty() {
        Ok(())
    } else {
        Err(DecodeError::trailing(body.len()).into())
    }
}

/// Reject bytes after a record's last field.
fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(DecodeError::trailing(r.remaining()).into())
    }
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

    fn push_u64(p: &mut Vec<u8>, value: u64) {
        p.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(p: &mut Vec<u8>, value: f32) {
        p.extend_from_slice(&value.to_le_bytes());
    }

    fn push_str(p: &mut Vec<u8>, value: &str) {
        p.extend_from_slice(value.as_bytes());
        p.push(0);
    }

    fn push_packed_guid(p: &mut Vec<u8>, guid: u64) {
        let mut mask = 0u8;
        let mut bytes = [0u8; 8];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = (guid >> (i * 8)) as u8;
            if *byte != 0 {
                mask |= 1 << i;
            }
        }
        p.push(mask);
        for byte in bytes {
            if byte != 0 {
                p.push(byte);
            }
        }
    }

    fn assert_truncated<T>(read: impl Fn(&[u8]) -> Result<T>, body: &[u8]) {
        for end in 0..body.len() {
            assert!(
                read(&body[..end]).is_err(),
                "truncation at {end} of {} bytes was accepted",
                body.len()
            );
        }
    }

    fn assert_trailing_rejected<T>(read: impl Fn(&[u8]) -> Result<T>, body: &[u8]) {
        let mut extended = body.to_vec();
        extended.push(0);
        assert!(read(&extended).is_err(), "trailing byte was accepted");
    }

    fn group_list_full_body() -> Vec<u8> {
        let mut body = vec![0x0A, 0x02, 0x04, 0x05]; // raid | lfg, subgroup, flags, roles
        body.push(0x02); // LFG status
        push_u32(&mut body, 574); // LFG dungeon id
        push_u64(&mut body, 0x1122_3344_5566_7788); // group guid
        push_u32(&mut body, 7); // counter
        push_u32(&mut body, 1); // member count
        push_str(&mut body, "Thrall");
        push_u64(&mut body, 0x8877_6655_4433_2211);
        body.push(0x01); // online
        body.push(0x01); // subgroup
        body.push(0x02); // member flags
        body.push(0x08); // roles
        push_u64(&mut body, 0x1122_3344_5566_7788); // leader
        body.push(2); // master loot
        push_u64(&mut body, 0x8877_6655_4433_2211); // master looter
        body.push(3); // threshold
        body.push(1); // dungeon difficulty
        body.push(2); // raid difficulty
        body.push(1); // dynamic difficulty
        body
    }

    fn group_list_empty_body() -> Vec<u8> {
        let mut body = vec![0x10, 0x00, 0x00, 0x00];
        push_u64(&mut body, 0x1122_3344_5566_7788);
        push_u32(&mut body, 3);
        push_u32(&mut body, 0);
        push_u64(&mut body, 0);
        body
    }

    #[test]
    fn group_list_full_preserves_lfg_loot_and_member() {
        let body = group_list_full_body();
        let list = read_group_list(&body).unwrap();
        assert_eq!(list.group_type, 0x0A);
        assert_eq!(list.sub_group, 2);
        assert_eq!(list.flags, 4);
        assert_eq!(list.roles, 5);
        assert_eq!(list.lfg, Some(LfgGroupTail { status: 2, dungeon_id: 574 }));
        assert_eq!(list.group_guid, 0x1122_3344_5566_7788);
        assert_eq!(list.counter, 7);
        assert_eq!(list.members.len(), 1);
        assert_eq!(
            list.members[0],
            GroupMember {
                name: "Thrall".into(),
                guid: 0x8877_6655_4433_2211,
                status: 1,
                sub_group: 1,
                flags: 2,
                roles: 8,
            }
        );
        assert_eq!(list.leader_guid, 0x1122_3344_5566_7788);
        assert_eq!(
            list.loot,
            Some(GroupLoot {
                method: 2,
                master: 0x8877_6655_4433_2211,
                threshold: 3,
                dungeon_difficulty: 1,
                raid_difficulty: 2,
                dynamic_difficulty: 1,
            })
        );
        assert_truncated(read_group_list, &body);
        assert_trailing_rejected(read_group_list, &body);

        let mut too_many = vec![0, 0, 0, 0];
        push_u64(&mut too_many, 1);
        push_u32(&mut too_many, 1);
        push_u32(&mut too_many, MAX_GROUP_MEMBERS + 1);
        let error = read_group_list(&too_many).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    #[test]
    fn group_list_empty_is_a_valid_leave_record() {
        let body = group_list_empty_body();
        let list = read_group_list(&body).unwrap();
        assert_eq!(list.group_type, 0x10);
        assert_eq!(list.lfg, None);
        assert_eq!(list.group_guid, 0x1122_3344_5566_7788);
        assert_eq!(list.counter, 3);
        assert!(list.members.is_empty());
        assert_eq!(list.leader_guid, 0);
        assert_eq!(list.loot, None);
        assert_truncated(read_group_list, &body);
        assert_trailing_rejected(read_group_list, &body);
    }

    #[test]
    fn group_notices_keep_their_12340_shapes() {
        let mut invite = vec![1];
        push_str(&mut invite, "Thrall");
        push_u32(&mut invite, 0);
        invite.push(0); // empty dungeon list
        push_u32(&mut invite, 0);
        assert_eq!(
            read_group_invite(&invite).unwrap(),
            GroupInvite {
                can_accept: true,
                inviter: "Thrall".into(),
                lfg_unknown: 0,
                dungeons: Vec::new(),
                unknown: 0,
            }
        );
        assert_truncated(read_group_invite, &invite);
        assert_trailing_rejected(read_group_invite, &invite);

        let decline = b"Thrall\0".to_vec();
        assert_eq!(read_group_decline(&decline).unwrap(), "Thrall");
        assert_truncated(read_group_decline, &decline);
        assert_trailing_rejected(read_group_decline, &decline);

        assert!(read_group_uninvite(&[]).is_ok());
        assert!(read_group_uninvite(&[0]).is_err());
        assert!(read_group_destroyed(&[]).is_ok());
        assert!(read_group_destroyed(&[0]).is_err());

        let leader = b"Firna\0".to_vec();
        assert_eq!(read_group_set_leader(&leader).unwrap(), "Firna");
        assert_truncated(read_group_set_leader, &leader);
        assert_trailing_rejected(read_group_set_leader, &leader);

        let mut result = Vec::new();
        push_u32(&mut result, 1);
        push_str(&mut result, "Firna");
        push_u32(&mut result, 0);
        push_u32(&mut result, 30);
        assert_eq!(
            read_party_command_result(&result).unwrap(),
            (1, "Firna".to_string(), 0, 30)
        );
        assert_truncated(read_party_command_result, &result);
        assert_trailing_rejected(read_party_command_result, &result);
    }

    fn party_stats_body(full: bool) -> Vec<u8> {
        let mut body = Vec::new();
        if full {
            body.push(0); // arena flag
        }
        push_packed_guid(&mut body, 0x0102_0304_0506_0708);
        push_u32(&mut body, 0x000F_FFFF); // every defined field
        push_u16(&mut body, 0x0043); // status
        push_u32(&mut body, 12_345); // cur hp
        push_u32(&mut body, 20_000); // max hp
        body.push(3); // power type
        push_u16(&mut body, 500); // cur power
        push_u16(&mut body, 1000); // max power
        push_u16(&mut body, 80); // level
        push_u16(&mut body, 1519); // zone
        push_u16(&mut body, 100); // pos x
        push_u16(&mut body, 200); // pos y
        push_u64(&mut body, 0b101); // aura mask
        push_u32(&mut body, 1234);
        body.push(1);
        push_u32(&mut body, 5678);
        body.push(2);
        push_u64(&mut body, 0x1111_2222_3333_4444); // pet guid
        push_str(&mut body, "Wolf");
        push_u16(&mut body, 42); // pet model
        push_u32(&mut body, 900); // pet cur hp
        push_u32(&mut body, 1000); // pet max hp
        body.push(1); // pet power type
        push_u16(&mut body, 50); // pet cur power
        push_u16(&mut body, 100); // pet max power
        push_u64(&mut body, 0b1); // pet aura mask
        push_u32(&mut body, 99);
        body.push(1);
        push_u32(&mut body, 5); // vehicle seat
        body
    }

    #[test]
    fn party_member_stats_full_reads_every_defined_bit() {
        let body = party_stats_body(true);
        let stats = read_party_member_stats_full(&body).unwrap();
        assert_eq!(stats.guid, 0x0102_0304_0506_0708);
        assert!(stats.full);
        assert_eq!(stats.arena, Some(0));
        let f = &stats.fields;
        assert_eq!(f.mask, 0x000F_FFFF);
        assert_eq!(f.status, Some(0x0043));
        assert_eq!(f.cur_hp, Some(12_345));
        assert_eq!(f.max_hp, Some(20_000));
        assert_eq!(f.power_type, Some(3));
        assert_eq!(f.cur_power, Some(500));
        assert_eq!(f.max_power, Some(1000));
        assert_eq!(f.level, Some(80));
        assert_eq!(f.zone, Some(1519));
        assert_eq!(f.position, Some((100, 200)));
        assert_eq!(
            f.auras,
            Some(PartyAuras {
                mask: 0b101,
                auras: vec![
                    PartyAura { slot: 0, spell_id: 1234, flags: 1 },
                    PartyAura { slot: 2, spell_id: 5678, flags: 2 },
                ],
            })
        );
        assert_eq!(f.pet_guid, Some(0x1111_2222_3333_4444));
        assert_eq!(f.pet_name.as_deref(), Some("Wolf"));
        assert_eq!(f.pet_model_id, Some(42));
        assert_eq!(f.pet_cur_hp, Some(900));
        assert_eq!(f.pet_max_hp, Some(1000));
        assert_eq!(f.pet_power_type, Some(1));
        assert_eq!(f.pet_cur_power, Some(50));
        assert_eq!(f.pet_max_power, Some(100));
        assert_eq!(
            f.pet_auras,
            Some(PartyAuras {
                mask: 1,
                auras: vec![PartyAura { slot: 0, spell_id: 99, flags: 1 }],
            })
        );
        assert_eq!(f.vehicle_seat, Some(5));
        assert_truncated(read_party_member_stats_full, &body);
        assert_trailing_rejected(read_party_member_stats_full, &body);
    }

    #[test]
    fn party_member_stats_delta_leaves_absent_fields_empty() {
        let mut body = Vec::new();
        push_packed_guid(&mut body, 0x0102_0304_0506_0708);
        push_u32(&mut body, 0x0000_0003); // status + cur hp
        push_u16(&mut body, 1);
        push_u32(&mut body, 777);
        let stats = read_party_member_stats(&body).unwrap();
        assert!(!stats.full);
        assert_eq!(stats.arena, None);
        assert_eq!(stats.fields.status, Some(1));
        assert_eq!(stats.fields.cur_hp, Some(777));
        assert_eq!(stats.fields.max_hp, None);
        assert_eq!(stats.fields.auras, None);
        assert_eq!(stats.fields.vehicle_seat, None);
        assert_truncated(read_party_member_stats, &body);
        assert_trailing_rejected(read_party_member_stats, &body);
    }

    fn who_body() -> Vec<u8> {
        let mut body = Vec::new();
        push_u32(&mut body, 2);
        push_u32(&mut body, 7);
        push_str(&mut body, "Firna");
        push_str(&mut body, "RustWrath");
        push_u32(&mut body, 80);
        push_u32(&mut body, 2);
        push_u32(&mut body, 1);
        body.push(0);
        push_u32(&mut body, 1519);
        push_str(&mut body, "Thrall");
        push_str(&mut body, "");
        push_u32(&mut body, 80);
        push_u32(&mut body, 1);
        push_u32(&mut body, 2);
        body.push(1);
        push_u32(&mut body, 1637);
        body
    }

    #[test]
    fn who_rows_keep_12340_order() {
        let body = who_body();
        let who = read_who(&body).unwrap();
        assert_eq!(who.shown, 2);
        assert_eq!(who.total, 7);
        assert_eq!(who.players.len(), 2);
        assert_eq!(who.players[0].name, "Firna");
        assert_eq!(who.players[0].guild, "RustWrath");
        assert_eq!(who.players[0].level, 80);
        assert_eq!(who.players[0].class, 2);
        assert_eq!(who.players[0].race, 1);
        assert_eq!(who.players[0].gender, 0);
        assert_eq!(who.players[0].zone, 1519);
        assert_eq!(who.players[1].guild, "");
        assert_truncated(read_who, &body);
        assert_trailing_rejected(read_who, &body);

        let whois = b"Firna's account is X, e-mail: y, last ip: z\0".to_vec();
        assert_eq!(
            read_whois(&whois).unwrap(),
            WhoisResult {
                text: "Firna's account is X, e-mail: y, last ip: z".into()
            }
        );
        assert_truncated(read_whois, &whois);
        assert_trailing_rejected(read_whois, &whois);
    }

    fn contact_list_body() -> Vec<u8> {
        let mut body = Vec::new();
        push_u32(&mut body, 0x07); // friends | ignored | muted
        push_u32(&mut body, 4);

        push_u64(&mut body, 0x1000_0000_0000_0001); // online friend
        push_u32(&mut body, 0x1);
        push_str(&mut body, "best healer");
        body.push(1); // status online
        push_u32(&mut body, 1519);
        push_u32(&mut body, 80);
        push_u32(&mut body, 2);

        push_u64(&mut body, 0x1000_0000_0000_0002); // offline friend
        push_u32(&mut body, 0x1);
        push_str(&mut body, "old friend");
        body.push(0); // status offline, no area tail

        push_u64(&mut body, 0x1000_0000_0000_0003); // ignored
        push_u32(&mut body, 0x2);
        push_str(&mut body, "");

        push_u64(&mut body, 0x1000_0000_0000_0004); // muted
        push_u32(&mut body, 0x4);
        push_str(&mut body, "spam");
        body
    }

    #[test]
    fn contact_list_preserves_friend_ignore_and_mute_rows() {
        let body = contact_list_body();
        let list = read_contact_list(&body).unwrap();
        assert_eq!(list.flags, 0x07);
        assert_eq!(list.contacts.len(), 4);
        assert_eq!(list.contacts[0].flags, 0x1);
        assert_eq!(list.contacts[0].note, "best healer");
        assert_eq!(
            list.contacts[0].friend,
            Some(FriendInfo {
                status: 1,
                area: Some(1519),
                level: Some(80),
                class: Some(2),
            })
        );
        assert_eq!(
            list.contacts[1].friend,
            Some(FriendInfo {
                status: 0,
                area: None,
                level: None,
                class: None,
            })
        );
        assert_eq!(list.contacts[2].flags, 0x2);
        assert_eq!(list.contacts[2].note, "");
        assert_eq!(list.contacts[2].friend, None);
        assert_eq!(list.contacts[3].flags, 0x4);
        assert_eq!(list.contacts[3].note, "spam");
        assert_truncated(read_contact_list, &body);
        assert_trailing_rejected(read_contact_list, &body);
    }

    #[test]
    fn friend_status_tails_follow_the_result() {
        let mut added = vec![6];
        push_u64(&mut added, 0x1000_0000_0000_0001);
        push_str(&mut added, "note");
        added.push(1);
        push_u32(&mut added, 1519);
        push_u32(&mut added, 80);
        push_u32(&mut added, 2);
        let status = read_friend_status(&added).unwrap();
        assert_eq!(status.result, 6);
        assert_eq!(status.note.as_deref(), Some("note"));
        assert_eq!(
            status.online,
            Some(FriendInfo {
                status: 1,
                area: Some(1519),
                level: Some(80),
                class: Some(2),
            })
        );
        assert_truncated(read_friend_status, &added);
        assert_trailing_rejected(read_friend_status, &added);

        let mut offline = vec![3];
        push_u64(&mut offline, 0x1000_0000_0000_0002);
        let status = read_friend_status(&offline).unwrap();
        assert_eq!(status.note, None);
        assert_eq!(status.online, None);
        assert_truncated(read_friend_status, &offline);
        assert_trailing_rejected(read_friend_status, &offline);
    }

    #[test]
    fn channel_notify_tails_follow_the_notice() {
        let mut guid_tail = vec![0x00];
        push_str(&mut guid_tail, "General");
        push_u64(&mut guid_tail, 0x1000_0000_0000_0001);
        assert_eq!(
            read_channel_notify(&guid_tail).unwrap(),
            ChannelNotify {
                notice: 0,
                channel: "General".into(),
                tail: ChannelNotifyTail::Guid {
                    guid: 0x1000_0000_0000_0001
                },
            }
        );
        assert_truncated(read_channel_notify, &guid_tail);
        assert_trailing_rejected(read_channel_notify, &guid_tail);

        let mut joined = vec![0x02];
        push_str(&mut joined, "Trade");
        joined.push(0x04);
        push_u32(&mut joined, 77);
        push_u32(&mut joined, 0x18);
        push_str(&mut joined, "welcome");
        let notice = read_channel_notify(&joined).unwrap();
        assert_eq!(
            notice.tail,
            ChannelNotifyTail::Joined {
                flags: 0x04,
                zone_channel_id: 77,
                channel_flags: 0x18,
                extra: Some("welcome".into()),
            }
        );
        assert_trailing_rejected(read_channel_notify, &joined);

        let mut changed = vec![0x0C];
        push_str(&mut changed, "Trade");
        push_u64(&mut changed, 0x1000_0000_0000_0003);
        changed.push(0x01);
        changed.push(0x02);
        assert_eq!(
            read_channel_notify(&changed).unwrap().tail,
            ChannelNotifyTail::ModeChange {
                guid: 0x1000_0000_0000_0003,
                old_flags: 1,
                new_flags: 2,
            }
        );
        assert_truncated(read_channel_notify, &changed);
        assert_trailing_rejected(read_channel_notify, &changed);

        let mut unknown = vec![0x25];
        push_str(&mut unknown, "General");
        let error = read_channel_notify(&unknown).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn channel_list_reads_the_leading_type_and_u32_count() {
        let mut body = vec![1];
        push_str(&mut body, "Trade");
        body.push(0x04);
        push_u32(&mut body, 2);
        push_u64(&mut body, 0x1000_0000_0000_0001);
        body.push(0x01);
        push_u64(&mut body, 0x1000_0000_0000_0002);
        body.push(0x02);
        let list = read_channel_list(&body).unwrap();
        assert_eq!(list.channel_type, 1);
        assert_eq!(list.channel, "Trade");
        assert_eq!(list.flags, 0x04);
        assert_eq!(
            list.members,
            vec![
                (0x1000_0000_0000_0001, 0x01),
                (0x1000_0000_0000_0002, 0x02),
            ]
        );
        assert_truncated(read_channel_list, &body);
        assert_trailing_rejected(read_channel_list, &body);
    }

    fn guild_roster_body() -> Vec<u8> {
        let mut body = Vec::new();
        push_u32(&mut body, 2); // members
        push_str(&mut body, "Welcome to RustWrath");
        push_str(&mut body, "Casual guild");
        push_u32(&mut body, 10); // ranks

        for rank in 0..10u32 {
            push_u32(&mut body, 0x100 + rank); // rights
            push_u32(&mut body, 1_000 * rank); // gold limit per day
            for tab in 0..GUILD_BANK_TABS as u32 {
                push_u32(&mut body, 0x200 + tab); // tab flags
                push_u32(&mut body, 5 + tab); // withdraw limit
            }
        }

        // Online member: no last-online field.
        push_u64(&mut body, 0x2000_0000_0000_0001);
        body.push(1);
        push_str(&mut body, "Firna");
        push_u32(&mut body, 0);
        body.push(80);
        body.push(2);
        body.push(0);
        push_u32(&mut body, 1519);
        push_str(&mut body, "public note");
        push_str(&mut body, "officer note");

        // Offline member: the last-online float is present.
        push_u64(&mut body, 0x2000_0000_0000_0002);
        body.push(0);
        push_str(&mut body, "Thrall");
        push_u32(&mut body, 3);
        body.push(80);
        body.push(1);
        body.push(1);
        push_u32(&mut body, 1637);
        push_f32(&mut body, 1.5);
        push_str(&mut body, "");
        push_str(&mut body, "");
        body
    }

    #[test]
    fn guild_roster_reads_ten_ranks_and_online_offline_members() {
        let body = guild_roster_body();
        let roster = read_guild_roster(&body).unwrap();
        assert_eq!(roster.motd, "Welcome to RustWrath");
        assert_eq!(roster.guild_info, "Casual guild");
        assert_eq!(roster.member_count, 2);
        assert_eq!(roster.ranks.len(), 10);
        assert_eq!(roster.ranks[3].rights, 0x103);
        assert_eq!(roster.ranks[3].gold_limit_per_day, 3000);
        assert_eq!(roster.ranks[3].bank_tab_flags, [0x200, 0x201, 0x202, 0x203, 0x204, 0x205]);
        assert_eq!(
            roster.ranks[3].bank_tab_withdraw_limits,
            [5, 6, 7, 8, 9, 10]
        );

        assert_eq!(roster.members.len(), 2);
        let online = &roster.members[0];
        assert!(online.online);
        assert_eq!(online.status, 1);
        assert_eq!(online.name, "Firna");
        assert_eq!(online.rank, 0);
        assert_eq!(online.level, 80);
        assert_eq!(online.class, 2);
        assert_eq!(online.gender, 0);
        assert_eq!(online.zone, 1519);
        assert_eq!(online.note, "public note");
        assert_eq!(online.officer_note, "officer note");
        assert_eq!(online.last_online_days, None);

        let offline = &roster.members[1];
        assert!(!offline.online);
        assert_eq!(offline.status, 0);
        assert_eq!(offline.last_online_days, Some(1.5));
        assert_eq!(offline.note, "");
        assert_eq!(offline.officer_note, "");

        assert_truncated(read_guild_roster, &body);
        assert_trailing_rejected(read_guild_roster, &body);

        let mut eleven_ranks = Vec::new();
        push_u32(&mut eleven_ranks, 0);
        push_str(&mut eleven_ranks, "");
        push_str(&mut eleven_ranks, "");
        push_u32(&mut eleven_ranks, 11);
        let error = read_guild_roster(&eleven_ranks).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidCount);
    }

    #[test]
    fn guild_info_event_and_command_result() {
        let mut body = Vec::new();
        push_str(&mut body, "RustWrath");
        push_u32(&mut body, 0x1234_5678);
        push_u32(&mut body, 42);
        push_u32(&mut body, 7);
        assert_eq!(
            read_guild_info(&body).unwrap(),
            GuildInfoWire {
                name: "RustWrath".into(),
                created_at: 0x1234_5678,
                member_count: 42,
                account_count: 7,
            }
        );
        assert_truncated(read_guild_info, &body);
        assert_trailing_rejected(read_guild_info, &body);

        let mut event_body = vec![12, 1];
        push_str(&mut event_body, "Firna");
        push_u64(&mut event_body, 0x3000_0000_0000_0001);
        let event = read_guild_event(&event_body).unwrap();
        assert_eq!(event.event, 12);
        assert_eq!(event.args, vec!["Firna".to_string()]);
        assert_eq!(event.guid, Some(0x3000_0000_0000_0001));
        assert_truncated(read_guild_event, &event_body);
        assert_trailing_rejected(read_guild_event, &event_body);

        let mut motd = vec![2, 1];
        push_str(&mut motd, "new motd");
        let event = read_guild_event(&motd).unwrap();
        assert_eq!(event.guid, None);
        assert_eq!(event.args, vec!["new motd".to_string()]);
        assert_truncated(read_guild_event, &motd);
        assert_trailing_rejected(read_guild_event, &motd);

        let mut command = Vec::new();
        push_u32(&mut command, 1);
        push_str(&mut command, "Firna");
        push_u32(&mut command, 0);
        assert_eq!(
            read_guild_command_result(&command).unwrap(),
            GuildCommandResultWire {
                command: 1,
                name: "Firna".into(),
                error: 0,
            }
        );
        assert_truncated(read_guild_command_result, &command);
        assert_trailing_rejected(read_guild_command_result, &command);
    }

    #[test]
    fn raid_target_update_full_and_delta() {
        let mut full = vec![1, 0];
        push_u64(&mut full, 0x4000_0000_0000_0001);
        full.push(3);
        push_u64(&mut full, 0x4000_0000_0000_0002);
        assert_eq!(
            read_raid_target_update(&full).unwrap(),
            RaidTargetUpdate::List {
                entries: vec![
                    (0, 0x4000_0000_0000_0001),
                    (3, 0x4000_0000_0000_0002),
                ],
            }
        );
        // A full list stops at the last complete pair, so only partial-pair cuts must fail.
        for end in 0..full.len() {
            if end % 9 != 1 {
                assert!(read_raid_target_update(&full[..end]).is_err(), "cut at {end}");
            }
        }
        let mut full_trailing = full.clone();
        full_trailing.push(0);
        assert!(read_raid_target_update(&full_trailing).is_err());

        let mut delta = vec![0];
        push_u64(&mut delta, 0x4000_0000_0000_0003);
        delta.push(4);
        push_u64(&mut delta, 0x4000_0000_0000_0004);
        assert_eq!(
            read_raid_target_update(&delta).unwrap(),
            RaidTargetUpdate::Set {
                setter: 0x4000_0000_0000_0003,
                icon: 4,
                guid: 0x4000_0000_0000_0004,
            }
        );
        assert_truncated(read_raid_target_update, &delta);
        assert_trailing_rejected(read_raid_target_update, &delta);

        let mut bad_kind = vec![2];
        push_u64(&mut bad_kind, 1);
        let error = read_raid_target_update(&bad_kind).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);

        let mut bad_icon = vec![0];
        push_u64(&mut bad_icon, 1);
        bad_icon.push(8);
        push_u64(&mut bad_icon, 2);
        let error = read_raid_target_update(&bad_icon).unwrap_err();
        assert_eq!(classify_decode_failure(&error), DecodeFailure::InvalidValue);
    }

    #[test]
    fn raid_ready_check_request_and_answer_shapes() {
        let mut request = Vec::new();
        push_u64(&mut request, 0x5000_0000_0000_0001);
        assert_eq!(
            read_raid_ready_check(&request).unwrap(),
            RaidReadyCheck::Request {
                initiator: 0x5000_0000_0000_0001
            }
        );
        for end in 0..request.len() {
            assert!(read_raid_ready_check(&request[..end]).is_err());
        }

        let mut answer = request.clone();
        answer.push(1);
        assert_eq!(
            read_raid_ready_check(&answer).unwrap(),
            RaidReadyCheck::Answer {
                guid: 0x5000_0000_0000_0001,
                ready: 1,
            }
        );
        for end in 0..8 {
            assert!(read_raid_ready_check(&answer[..end]).is_err());
        }
        // The 8-byte prefix of an answer is, by length, the request form.
        assert_eq!(
            read_raid_ready_check(&answer[..8]).unwrap(),
            RaidReadyCheck::Request {
                initiator: 0x5000_0000_0000_0001
            }
        );
        let mut too_long = answer.clone();
        too_long.push(0);
        assert!(read_raid_ready_check(&too_long).is_err());
    }
}
