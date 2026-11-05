//! Transcript opcodes, values taken verbatim from the local `Opcodes.h` (build 12340 table).

#![allow(dead_code)]

pub const CMSG_CHAR_ENUM: u32 = 0x037;
pub const CMSG_REALM_SPLIT: u32 = 0x38C;
pub const SMSG_REALM_SPLIT: u16 = 0x38B;
pub const CMSG_CHAR_CREATE: u32 = 0x036;
pub const CMSG_CHAR_DELETE: u32 = 0x038;
pub const SMSG_CHAR_CREATE: u16 = 0x03A;
pub const SMSG_CHAR_DELETE: u16 = 0x03C;
pub const SMSG_CHAR_ENUM: u16 = 0x03B;
pub const CMSG_PLAYER_LOGIN: u32 = 0x03D;
pub const SMSG_LOGIN_SETTIMESPEED: u16 = 0x042;
/// The empty handler: the stock client never sends it (Astra F04; MiscHandler.cpp:485).
pub const CMSG_PLAYER_LOGOUT: u32 = 0x04A;
/// The real logout request -> HandleLogoutRequestOpcode (MiscHandler.cpp:425).
pub const CMSG_LOGOUT_REQUEST: u32 = 0x04B;
pub const SMSG_LOGOUT_RESPONSE: u16 = 0x04C;
pub const SMSG_LOGOUT_COMPLETE: u16 = 0x04D;
pub const CMSG_LOGOUT_CANCEL: u32 = 0x04E;
pub const SMSG_LOGOUT_CANCEL_ACK: u16 = 0x04F;
pub const CMSG_SET_ACTIVE_MOVER: u32 = 0x26A;
pub const CMSG_GAMEOBJ_USE: u32 = 0x0B1;
/// World-session auth success code (SharedDefines.h:3586); not 0 (Astra F01).
pub const AUTH_OK: u8 = 0x0C;
pub const SMSG_UPDATE_OBJECT: u16 = 0x0A9;
/// Object destruction outside the update-block path (`Object.cpp:278-286`):
/// full u64 GUID + u8 on-death flag.
pub const SMSG_DESTROY_OBJECT: u16 = 0x0AA;
/// Far-teleport sequence, step 1 (`Player.cpp:1593`): u32 mapid, optionally u32 transport
/// entry + u32 current map when on a transport. The player is still in the old world.
pub const SMSG_TRANSFER_PENDING: u16 = 0x03F;
/// Far-teleport sequence, step 2 (`Player.cpp:1613`): u32 mapid + f32 x/y/z/o destination.
/// Receiving this begins the new world scope; the old world's replicated state is discarded.
pub const SMSG_NEW_WORLD: u16 = 0x03E;
/// Near teleport, server to client (`Unit.cpp:16198`): packed GUID + standalone movement
/// payload (no update-flags prefix). Same world scope.
pub const MSG_MOVE_TELEPORT: u16 = 0x0C5;
/// Near teleport, server to the CONTROLLED player (`Player.cpp:1375`,
/// `SendTeleportAckPacket`): packed GUID + u32 order counter + standalone movement payload.
/// The client replies with the same opcode carrying packed GUID + u32 flags + u32 time
/// (`MovementHandler.cpp:277-282` — the ack handler reads exactly those three fields).
pub const MSG_MOVE_TELEPORT_ACK: u16 = 0x0C7;
/// Worldport acknowledgment, client to server (`MovementHandler.cpp:44`): empty payload,
/// processed only while a far teleport is outstanding.
pub const CMSG_WORLD_MOVE_WORLDPORT_ACK: u32 = 0x0DC;

/// `SMSG_COMPRESSED_UPDATE_OBJECT` (`WorldSocket.cpp:97-119`): the payload is
/// `[u32 uncompressed_size][zlib(update-object payload)]` — no inner opcode; the decompressed
/// bytes are the plain update body. (The fixture first misled us into reading an inner opcode;
/// the "0x0016" was a 22-block world snapshot's count field.)
pub const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x1F6;
pub const MSG_MOVE_START_FORWARD: u32 = 0x0B5;
pub const MSG_MOVE_START_BACKWARD: u32 = 0x0B6;
pub const MSG_MOVE_STOP: u32 = 0x0B7;
pub const MSG_MOVE_START_STRAFE_LEFT: u32 = 0x0B8;
pub const MSG_MOVE_START_STRAFE_RIGHT: u32 = 0x0B9;
pub const MSG_MOVE_STOP_STRAFE: u32 = 0x0BA;
pub const MSG_MOVE_JUMP: u32 = 0x0BB;
pub const SMSG_MONSTER_MOVE: u16 = 0x0DD;
pub const SMSG_MONSTER_MOVE_TRANSPORT: u16 = 0x2AE;
pub const MSG_MOVE_START_TURN_LEFT: u32 = 0x0BC;
pub const MSG_MOVE_START_TURN_RIGHT: u32 = 0x0BD;
pub const MSG_MOVE_STOP_TURN: u32 = 0x0BE;
pub const MSG_MOVE_START_PITCH_UP: u32 = 0x0BF;
pub const MSG_MOVE_START_PITCH_DOWN: u32 = 0x0C0;
pub const MSG_MOVE_STOP_PITCH: u32 = 0x0C1;
pub const MSG_MOVE_FALL_LAND: u32 = 0x0C9;
pub const MSG_MOVE_SET_FACING: u32 = 0x0DA;
pub const MSG_MOVE_HEARTBEAT: u32 = 0x0EE;
pub const CMSG_SET_SELECTION: u32 = 0x13D;
/// Chat (`ChatHandler.cpp:59-300`, `Chat.cpp:374-443`): 12340 has a uniform body; the
/// GM variant adds an inline sender name.
/// Item use and bag movement (`ItemHandler.cpp`). 12340 `CMSG_USE_ITEM` carries the item guid
/// and spell id; the other verbs carry only wire bag/slot positions.
pub const CMSG_USE_ITEM: u32 = 0x0AB;
pub const CMSG_OPEN_ITEM: u32 = 0x0AC;
pub const CMSG_AUTOEQUIP_ITEM: u32 = 0x10A;
pub const CMSG_AUTOSTORE_BAG_ITEM: u32 = 0x10B;
pub const CMSG_SWAP_ITEM: u32 = 0x10C;
pub const CMSG_SWAP_INV_ITEM: u32 = 0x10D;
pub const CMSG_SPLIT_ITEM: u32 = 0x10E;
pub const CMSG_DESTROYITEM: u32 = 0x111;
pub const CMSG_SET_AMMO: u32 = 0x268;
pub const CMSG_MESSAGECHAT: u32 = 0x095;
pub const SMSG_MESSAGECHAT: u16 = 0x096;
pub const SMSG_GM_MESSAGECHAT: u16 = 0x3B3;
/// Chat-adjacent notices (MiscHandler/ChatHandler): one cstring each.
pub const SMSG_CHAT_PLAYER_NOT_FOUND: u16 = 0x2A9;
pub const SMSG_NOTIFICATION: u16 = 0x1CB;
pub const SMSG_AI_REACTION: u16 = 0x13C;
pub const CMSG_ATTACKSWING: u32 = 0x141;
pub const CMSG_ATTACKSTOP: u32 = 0x142;
pub const SMSG_ATTACKSTART: u16 = 0x143;
pub const SMSG_ATTACKSTOP: u16 = 0x144;
pub const SMSG_ATTACKSWING_NOTINRANGE: u16 = 0x145;
pub const SMSG_ATTACKSWING_BADFACING: u16 = 0x146;
pub const SMSG_ATTACKSWING_DEADTARGET: u16 = 0x148;
pub const SMSG_ATTACKSWING_CANT_ATTACK: u16 = 0x149;
pub const SMSG_ATTACKERSTATEUPDATE: u16 = 0x14A;
pub const SMSG_CANCEL_COMBAT: u16 = 0x14E;
pub const SMSG_PLAY_SPELL_VISUAL: u16 = 0x1F3;
pub const SMSG_PERIODICAURALOG: u16 = 0x24E;
pub const SMSG_SPELLNONMELEEDAMAGELOG: u16 = 0x250;
pub const SMSG_CANCEL_AUTO_REPEAT: u16 = 0x29C;
pub const CMSG_CAST_SPELL: u32 = 0x12E;
pub const SMSG_CAST_FAILED: u16 = 0x130;
pub const SMSG_SPELL_START: u16 = 0x131;
pub const SMSG_SPELL_GO: u16 = 0x132;
pub const SMSG_SPELL_FAILURE: u16 = 0x133;
/// Server-pushed cooldowns: raw caster, a 12340-only flags byte, then `(spell, ms)` pairs
/// (`Unit::BuildCooldownPacket`, `Unit.cpp:17017-17036`).
pub const SMSG_SPELL_COOLDOWN: u16 = 0x134;
/// `(u32 spell, u64 caster)` (`Player.cpp:11052-11055`).
pub const SMSG_COOLDOWN_EVENT: u16 = 0x135;
/// Release spirit (`MiscHandler.cpp:58-86`); the handler reads one byte.
pub const CMSG_REPOP_REQUEST: u32 = 0x15A;
/// A resurrect offer (`Spell.cpp:5191-5200`): raw caster, name length, name + NUL, sickness,
/// and a u32 delay override only for `SPELL_ATTR3_NO_RES_TIMER`.
pub const SMSG_RESURRECT_REQUEST: u16 = 0x15B;
/// `(u64 caster, u8 accept)` (`MiscHandler.cpp:677-698`).
pub const CMSG_RESURRECT_RESPONSE: u32 = 0x15C;
/// The corpse's raw guid (`MiscHandler.cpp:642-645`).
pub const CMSG_RECLAIM_CORPSE: u32 = 0x1D2;
/// The corpse query, both ways (`QueryHandler.cpp:226-266`): the request is empty, the answer
/// is found byte, map, position and corpse map.
pub const MSG_CORPSE_QUERY: u16 = 0x216;
/// The spirit healer's confirm guid (`NPCHandler.cpp:233-253`, `spell_generic.cpp:2735`).
pub const SMSG_SPIRIT_HEALER_CONFIRM: u16 = 0x222;
/// The healer's raw guid (`NPCHandler.cpp:233-252`).
pub const CMSG_SPIRIT_HEALER_ACTIVATE: u32 = 0x21C;
/// Empty body (`SpellHandler.cpp:705-719`).
pub const CMSG_SELF_RES: u32 = 0x2B3;
/// `u32` milliseconds (`Player.cpp:13007-13012`).
pub const SMSG_CORPSE_RECLAIM_DELAY: u16 = 0x269;
/// `(u32 spell, u64 caster)` (`Player.cpp:14778-14781`).
pub const SMSG_CLEAR_COOLDOWN: u16 = 0x1DE;
/// `(u64 item, u32 spell)` (`Player.cpp:11787-11790`).
pub const SMSG_ITEM_COOLDOWN: u16 = 0x0B0;
/// Packed target + every visible aura block (`Player.cpp:12053-12063`).
pub const SMSG_AURA_UPDATE_ALL: u16 = 0x495;
/// Packed target + one aura block (`SpellAuras.cpp:187-235`).
pub const SMSG_AURA_UPDATE: u16 = 0x496;
/// Packed caster, u32 spell, u32 duration (`Spell.cpp:5158-5177`).
pub const MSG_CHANNEL_START: u16 = 0x139;
/// Packed caster, u32 time left (`Spell.cpp:5143-5156`).
pub const MSG_CHANNEL_UPDATE: u16 = 0x13A;
/// The 1.12 seam's observed-interruption edge (`Spell.cpp:5126-5141` sends it with 0x133).
pub const SMSG_SPELL_FAILED_OTHER: u16 = 0x2A6;
/// Cast pushback: packed caster + u32 milliseconds (`Spell.cpp:7824-7828`).
pub const SMSG_SPELL_DELAYED: u16 = 0x1E2;
pub const SMSG_ACTION_BUTTONS: u16 = 0x129;
pub const SMSG_INITIAL_SPELLS: u16 = 0x12A;
pub const SMSG_LEARNED_SPELL: u16 = 0x12B;
pub const SMSG_QUEST_QUERY_RESPONSE: u16 = 0x05D;
pub const CMSG_GOSSIP_HELLO: u32 = 0x17B;
pub const CMSG_GOSSIP_SELECT_OPTION: u32 = 0x17C;
pub const SMSG_GOSSIP_MESSAGE: u16 = 0x17D;
pub const SMSG_GOSSIP_COMPLETE: u16 = 0x17E;
pub const CMSG_NPC_TEXT_QUERY: u32 = 0x17F;
pub const SMSG_NPC_TEXT_UPDATE: u16 = 0x180;
pub const SMSG_QUESTGIVER_STATUS: u16 = 0x183;
pub const CMSG_QUESTGIVER_HELLO: u32 = 0x184;
pub const SMSG_QUESTGIVER_QUEST_LIST: u16 = 0x185;
pub const CMSG_QUESTGIVER_QUERY_QUEST: u32 = 0x186;
pub const SMSG_QUESTGIVER_QUEST_DETAILS: u16 = 0x188;
pub const CMSG_QUESTGIVER_ACCEPT_QUEST: u32 = 0x189;
pub const CMSG_QUESTGIVER_COMPLETE_QUEST: u32 = 0x18A;
pub const SMSG_QUESTGIVER_REQUEST_ITEMS: u16 = 0x18B;
pub const CMSG_QUESTGIVER_REQUEST_REWARD: u32 = 0x18C;
pub const SMSG_QUESTGIVER_OFFER_REWARD: u16 = 0x18D;
pub const CMSG_QUESTGIVER_CHOOSE_REWARD: u32 = 0x18E;
pub const CMSG_QUESTGIVER_STATUS_QUERY: u32 = 0x182;
pub const SMSG_QUESTGIVER_QUEST_INVALID: u16 = 0x18F;
pub const SMSG_QUESTGIVER_QUEST_COMPLETE: u16 = 0x191;
pub const SMSG_QUESTGIVER_QUEST_FAILED: u16 = 0x192;
pub const SMSG_QUESTLOG_FULL: u16 = 0x195;
pub const SMSG_QUESTUPDATE_FAILED: u16 = 0x196;
pub const SMSG_QUESTUPDATE_FAILEDTIMER: u16 = 0x197;
pub const SMSG_QUESTUPDATE_COMPLETE: u16 = 0x198;
pub const SMSG_QUESTUPDATE_ADD_KILL: u16 = 0x199;
pub const SMSG_QUESTUPDATE_ADD_ITEM: u16 = 0x19A;
pub const CMSG_QUEST_QUERY: u32 = 0x05C;
pub const CMSG_NAME_QUERY: u32 = 0x050;
pub const SMSG_NAME_QUERY_RESPONSE: u16 = 0x051;
pub const CMSG_ITEM_QUERY_SINGLE: u32 = 0x056;
pub const CMSG_CREATURE_QUERY: u32 = 0x060;
pub const SMSG_CREATURE_QUERY_RESPONSE: u16 = 0x061;
pub const CMSG_GAMEOBJECT_QUERY: u32 = 0x05E;
pub const SMSG_GAMEOBJECT_QUERY_RESPONSE: u16 = 0x05F;
pub const SMSG_ITEM_QUERY_SINGLE_RESPONSE: u16 = 0x058;
pub const CMSG_LOOT: u32 = 0x15D;
pub const CMSG_LOOT_MONEY: u32 = 0x15E;
pub const CMSG_LOOT_RELEASE: u32 = 0x15F;
/// Group-loot roll family (`Opcodes.h:700-704`). The roll guid is the server's allocated item
/// guid, not the looted creature, and every guid in this family is a raw `u64`.
pub const SMSG_LOOT_ALL_PASSED: u16 = 0x29E;
pub const SMSG_LOOT_ROLL_WON: u16 = 0x29F;
pub const CMSG_LOOT_ROLL: u32 = 0x2A0;
pub const SMSG_LOOT_START_ROLL: u16 = 0x2A1;
pub const SMSG_LOOT_ROLL: u16 = 0x2A2;
/// The player's pass-on-loot preference: one u32 flag (`GroupHandler.cpp:1082`).
pub const CMSG_OPT_OUT_OF_LOOT: u32 = 0x409;
/// The client's dungeon-difficulty selection: one u32 mode (`MiscHandler.cpp:1264`,
/// `SetDungeonDifficultyClient::Read`). The server echoes the same opcode with the accepted mode.
pub const MSG_SET_DUNGEON_DIFFICULTY: u32 = 0x329;
/// The raid selection: one u32 mode (`MSG_SET_RAID_DIFFICULTY`), echoed the same way.
pub const MSG_SET_RAID_DIFFICULTY: u32 = 0x4EB;
pub const SMSG_LOOT_RESPONSE: u16 = 0x160;
pub const SMSG_LOOT_RELEASE_RESPONSE: u16 = 0x161;
pub const SMSG_LOOT_REMOVED: u16 = 0x162;
pub const SMSG_LOOT_MONEY_NOTIFY: u16 = 0x163;
pub const SMSG_LOOT_CLEAR_MONEY: u16 = 0x165;
pub const SMSG_ITEM_PUSH_RESULT: u16 = 0x166;
pub const CMSG_AUTOSTORE_LOOT_ITEM: u32 = 0x108;
/// Vendor window (ItemHandler.cpp:844 / :854): the 12340 row carries the extended cost
/// (8th word) that 1.12 does not.
pub const CMSG_LIST_INVENTORY: u32 = 0x19E;
pub const SMSG_LIST_INVENTORY: u16 = 0x19F;
pub const CMSG_SELL_ITEM: u32 = 0x1A0;
pub const SMSG_SELL_ITEM: u16 = 0x1A1;
/// 12340 BuyItem::Read (ItemPackets.cpp:94): guid, item, 1-based slot, u32 count, u8 unk.
pub const CMSG_BUY_ITEM: u32 = 0x1A2;
pub const CMSG_BUY_ITEM_IN_SLOT: u32 = 0x1A3;
pub const SMSG_BUY_ITEM: u16 = 0x1A4;
pub const SMSG_BUY_FAILED: u16 = 0x1A5;
pub const CMSG_BUYBACK_ITEM: u32 = 0x290;
pub const CMSG_REPAIR_ITEM: u32 = 0x2A8;
/// Trainer window (`NPCHandler.cpp:81-135`, `NPCPackets.cpp:25-58`): 12340 rows carry two
/// point-cost words and three required abilities, where 1.12 carries two booleans.
/// Bank window (`BankHandler.cpp:44-190`, `BankPackets.cpp:20-47`): the stock owner's bag
/// indices 63-68 are rebased to 12340's 67-73 by the tap.
pub const CMSG_BANKER_ACTIVATE: u32 = 0x1B7;
pub const SMSG_SHOW_BANK: u16 = 0x1B8;
pub const CMSG_BUY_BANK_SLOT: u32 = 0x1B9;
pub const SMSG_BUY_BANK_SLOT_RESULT: u16 = 0x1BA;
pub const CMSG_AUTOSTORE_BANK_ITEM: u32 = 0x282;
pub const CMSG_AUTOBANK_ITEM: u32 = 0x283;
pub const CMSG_TRAINER_LIST: u32 = 0x1B0;
pub const SMSG_TRAINER_LIST: u16 = 0x1B1;
pub const CMSG_TRAINER_BUY_SPELL: u32 = 0x1B2;
pub const SMSG_TRAINER_BUY_SUCCEEDED: u16 = 0x1B3;
pub const SMSG_TRAINER_BUY_FAILED: u16 = 0x1B4;
/// Cancel verbs (`SpellHandler.cpp:552-660`): 12340 `CMSG_CANCEL_CAST` leads with an ignored
/// counter byte, the others carry only the spell id or nothing.
pub const CMSG_CANCEL_CAST: u32 = 0x12F;
pub const CMSG_CANCEL_AURA: u32 = 0x136;
pub const CMSG_CANCEL_CHANNELLING: u32 = 0x13B;
pub const CMSG_CANCEL_AUTO_REPEAT_SPELL: u32 = 0x26D;
/// Posture and presentation verbs (`MiscHandler.cpp:569`, `CombatHandler.cpp:73`,
/// `MovementHandler.cpp:803`): stand/sit state and sheathe are u32, mount special empty,
/// action-bar toggles one byte.
pub const CMSG_STANDSTATECHANGE: u32 = 0x101;
pub const CMSG_SET_SHEATHED: u32 = 0x1E0;
pub const CMSG_MOUNTSPECIAL_ANIM: u32 = 0x171;
pub const CMSG_SET_ACTIONBAR_TOGGLES: u32 = 0x2BF;
/// One-shot service and query verbs (`MiscHandler`/`NPCHandler`/`SkillHandler`/`QueryHandler`).
pub const CMSG_BINDER_ACTIVATE: u32 = 0x1B5;
pub const SMSG_BINDER_CONFIRM: u16 = 0x2EB;
pub const CMSG_SUMMON_RESPONSE: u32 = 0x2AC;
pub const SMSG_SUMMON_REQUEST: u16 = 0x2AB;
pub const MSG_TALENT_WIPE_CONFIRM: u16 = 0x2AA;
pub const CMSG_PLAYED_TIME: u32 = 0x1CC;
pub const SMSG_PLAYED_TIME: u16 = 0x1CD;
pub const SMSG_QUERY_TIME_RESPONSE: u16 = 0x1CF;
pub const MSG_RANDOM_ROLL: u16 = 0x1FB;
pub const CMSG_PAGE_TEXT_QUERY: u32 = 0x05A;
pub const SMSG_PAGE_TEXT_QUERY_RESPONSE: u16 = 0x05B;
/// `SMSG_INVENTORY_CHANGE_FAILURE` (`PlayerStorage.cpp:4113-4154`).
pub const SMSG_INVENTORY_CHANGE_FAILURE: u16 = 0x112;
/// Item inscription text (`ItemHandler.cpp:1463-1483`): request by item guid, reply with a
/// present/absent byte, the guid and the text.
pub const CMSG_ITEM_TEXT_QUERY: u32 = 0x243;
pub const SMSG_ITEM_TEXT_QUERY_RESPONSE: u16 = 0x244;
/// Area triggers (`MiscHandler.cpp:700`) and quest-log/share verbs (`QuestHandler.cpp:400-635`).
pub const CMSG_AREATRIGGER: u32 = 0x0B4;
pub const SMSG_AREA_TRIGGER_MESSAGE: u16 = 0x2B8;
pub const CMSG_QUESTLOG_REMOVE_QUEST: u32 = 0x194;
pub const CMSG_QUEST_CONFIRM_ACCEPT: u32 = 0x19B;
pub const SMSG_QUEST_CONFIRM_ACCEPT: u16 = 0x19C;
pub const CMSG_PUSHQUESTTOPARTY: u32 = 0x19D;
pub const MSG_QUEST_PUSH_RESULT: u16 = 0x276;
pub const SMSG_BINDPOINTUPDATE: u16 = 0x155;
pub const CMSG_QUERY_TIME: u32 = 0x1CE;
pub const CMSG_PING: u32 = 0x1DC;
pub const SMSG_PONG: u16 = 0x1DD;
pub const SMSG_AUTH_CHALLENGE: u16 = 0x1EC;
pub const CMSG_AUTH_SESSION: u32 = 0x1ED;
pub const SMSG_AUTH_RESPONSE: u16 = 0x1EE;
pub const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x236;
pub const SMSG_ADDON_INFO: u16 = 0x2EF;
/// An AzerothCore-lineage unofficial opcode this server sends during the login burst
/// (`Opcodes.h:1340`): the uncompressed form of SMSG_COMPRESSED_MOVES carrying compound
/// movement-force states. Not in any stock codec; decoded or tolerated locally.
pub const SMSG_MULTIPLE_MOVES: u16 = 0x51E;

/// Talents (`TalentHandler.cpp:27`): `[u32 talent_id, u32 requested_rank]`.
pub const CMSG_LEARN_TALENT: u32 = 0x251;

/// Pet orders, spellbook and stable verbs (`PetHandler.cpp`, `NPCHandler.cpp`).
pub const CMSG_PET_SET_ACTION: u32 = 0x174;
pub const CMSG_PET_ACTION: u32 = 0x175;
pub const CMSG_PET_ABANDON: u32 = 0x176;
pub const CMSG_PET_RENAME: u32 = 0x177;
pub const CMSG_PET_CAST_SPELL: u32 = 0x1F0;
pub const CMSG_PET_CANCEL_AURA: u32 = 0x26B;
pub const CMSG_REQUEST_PET_INFO: u32 = 0x279;
pub const CMSG_PET_STOP_ATTACK: u32 = 0x2EA;
pub const CMSG_PET_UNLEARN: u32 = 0x2F0;
pub const CMSG_PET_SPELL_AUTOCAST: u32 = 0x2F3;
pub const MSG_LIST_STABLED_PETS: u32 = 0x26F;
pub const CMSG_STABLE_PET: u32 = 0x270;
pub const CMSG_UNSTABLE_PET: u32 = 0x271;
pub const CMSG_BUY_STABLE_SLOT: u32 = 0x272;
pub const CMSG_STABLE_SWAP_PET: u32 = 0x275;

/// Queries, emotes, inspection, skills, factions, flags, tutorials, and cinematics.
pub const CMSG_PET_NAME_QUERY: u32 = 0x052;
pub const CMSG_GUILD_QUERY: u32 = 0x054;
pub const CMSG_ITEM_NAME_QUERY: u32 = 0x2C4;
pub const CMSG_NEXT_CINEMATIC_CAMERA: u32 = 0x0FB;
pub const CMSG_COMPLETE_CINEMATIC: u32 = 0x0FC;
pub const CMSG_TUTORIAL_FLAG: u32 = 0x0FE;
pub const CMSG_TUTORIAL_CLEAR: u32 = 0x0FF;
pub const CMSG_TUTORIAL_RESET: u32 = 0x100;
pub const CMSG_EMOTE: u32 = 0x102;
pub const CMSG_TEXT_EMOTE: u32 = 0x104;
pub const CMSG_INSPECT: u32 = 0x114;
pub const MSG_INSPECT_HONOR_STATS: u32 = 0x2D6;
pub const CMSG_SET_FACTION_ATWAR: u32 = 0x125;
pub const CMSG_UNLEARN_SKILL: u32 = 0x202;
pub const CMSG_TOGGLE_PVP: u32 = 0x253;
pub const CMSG_SHOWING_HELM: u32 = 0x2B9;
pub const CMSG_SHOWING_CLOAK: u32 = 0x2BA;
pub const CMSG_REQUEST_RAID_INFO: u32 = 0x2CD;
pub const CMSG_SET_FACTION_INACTIVE: u32 = 0x317;
pub const CMSG_SET_WATCHED_FACTION: u32 = 0x318;
pub const CMSG_RESET_INSTANCES: u32 = 0x31D;

/// Social, group, raid, duel, taxi, trade, mail, auction, channel, guild and petition verbs.
pub const CMSG_WHO: u32 = 0x062;
pub const CMSG_WHOIS: u32 = 0x064;
pub const CMSG_CONTACT_LIST: u32 = 0x066;
pub const CMSG_ADD_FRIEND: u32 = 0x069;
pub const CMSG_DEL_FRIEND: u32 = 0x06A;
pub const CMSG_ADD_IGNORE: u32 = 0x06C;
pub const CMSG_DEL_IGNORE: u32 = 0x06D;
pub const CMSG_CHAT_IGNORED: u32 = 0x225;

pub const CMSG_GROUP_INVITE: u32 = 0x06E;
pub const CMSG_GROUP_ACCEPT: u32 = 0x072;
pub const CMSG_GROUP_DECLINE: u32 = 0x073;
pub const CMSG_GROUP_UNINVITE: u32 = 0x075;
pub const CMSG_GROUP_UNINVITE_GUID: u32 = 0x076;
pub const CMSG_LOOT_METHOD: u32 = 0x07A;
pub const CMSG_GROUP_DISBAND: u32 = 0x07B;
pub const MSG_MINIMAP_PING: u32 = 0x1D5;
pub const CMSG_GROUP_CHANGE_SUB_GROUP: u32 = 0x27E;
pub const CMSG_REQUEST_PARTY_MEMBER_STATS: u32 = 0x27F;
pub const CMSG_GROUP_SWAP_SUB_GROUP: u32 = 0x280;
pub const CMSG_GROUP_RAID_CONVERT: u32 = 0x28E;
pub const CMSG_LOOT_MASTER_GIVE: u32 = 0x2A3;
pub const MSG_RAID_TARGET_UPDATE: u32 = 0x321;
pub const MSG_RAID_READY_CHECK: u32 = 0x322;

pub const CMSG_GUILD_INVITE: u32 = 0x082;
pub const CMSG_GUILD_ACCEPT: u32 = 0x084;
pub const CMSG_GUILD_DECLINE: u32 = 0x085;
pub const CMSG_GUILD_INFO: u32 = 0x087;
pub const CMSG_GUILD_ROSTER: u32 = 0x089;
pub const CMSG_GUILD_PROMOTE: u32 = 0x08B;
pub const CMSG_GUILD_DEMOTE: u32 = 0x08C;
pub const CMSG_GUILD_LEAVE: u32 = 0x08D;
pub const CMSG_GUILD_REMOVE: u32 = 0x08E;
pub const CMSG_GUILD_DISBAND: u32 = 0x08F;
pub const CMSG_GUILD_LEADER: u32 = 0x090;
pub const CMSG_GUILD_MOTD: u32 = 0x091;
pub const CMSG_SAVE_GUILD_EMBLEM: u32 = 0x1F1;
pub const CMSG_GUILD_RANK: u32 = 0x231;
pub const CMSG_GUILD_ADD_RANK: u32 = 0x232;
pub const CMSG_GUILD_DEL_RANK: u32 = 0x233;
pub const CMSG_GUILD_SET_PUBLIC_NOTE: u32 = 0x234;
pub const CMSG_GUILD_SET_OFFICER_NOTE: u32 = 0x235;
pub const CMSG_GUILD_INFO_TEXT: u32 = 0x2FC;

pub const CMSG_JOIN_CHANNEL: u32 = 0x097;
pub const CMSG_LEAVE_CHANNEL: u32 = 0x098;
pub const CMSG_CHANNEL_LIST: u32 = 0x09A;
pub const CMSG_CHANNEL_PASSWORD: u32 = 0x09C;
pub const CMSG_CHANNEL_SET_OWNER: u32 = 0x09D;
pub const CMSG_CHANNEL_OWNER: u32 = 0x09E;
pub const CMSG_CHANNEL_MODERATOR: u32 = 0x09F;
pub const CMSG_CHANNEL_UNMODERATOR: u32 = 0x0A0;
pub const CMSG_CHANNEL_MUTE: u32 = 0x0A1;
pub const CMSG_CHANNEL_UNMUTE: u32 = 0x0A2;
pub const CMSG_CHANNEL_INVITE: u32 = 0x0A3;
pub const CMSG_CHANNEL_KICK: u32 = 0x0A4;
pub const CMSG_CHANNEL_BAN: u32 = 0x0A5;
pub const CMSG_CHANNEL_UNBAN: u32 = 0x0A6;
pub const CMSG_CHANNEL_ANNOUNCEMENTS: u32 = 0x0A7;
pub const CMSG_CHANNEL_MODERATE: u32 = 0x0A8;

pub const CMSG_INITIATE_TRADE: u32 = 0x116;
pub const CMSG_BEGIN_TRADE: u32 = 0x117;
pub const CMSG_BUSY_TRADE: u32 = 0x118;
pub const CMSG_IGNORE_TRADE: u32 = 0x119;
pub const CMSG_ACCEPT_TRADE: u32 = 0x11A;
pub const CMSG_UNACCEPT_TRADE: u32 = 0x11B;
pub const CMSG_CANCEL_TRADE: u32 = 0x11C;
pub const CMSG_SET_TRADE_ITEM: u32 = 0x11D;
pub const CMSG_CLEAR_TRADE_ITEM: u32 = 0x11E;
pub const CMSG_SET_TRADE_GOLD: u32 = 0x11F;

pub const CMSG_DUEL_ACCEPTED: u32 = 0x16C;
pub const CMSG_DUEL_CANCELLED: u32 = 0x16D;

pub const CMSG_TAXINODE_STATUS_QUERY: u32 = 0x1AA;
pub const CMSG_TAXIQUERYAVAILABLENODES: u32 = 0x1AC;
pub const CMSG_ACTIVATETAXI: u32 = 0x1AD;
pub const CMSG_ACTIVATETAXIEXPRESS: u32 = 0x312;

pub const CMSG_PETITION_SHOWLIST: u32 = 0x1BB;
pub const CMSG_PETITION_BUY: u32 = 0x1BD;
pub const CMSG_PETITION_SHOW_SIGNATURES: u32 = 0x1BE;
pub const CMSG_PETITION_SIGN: u32 = 0x1C0;
pub const MSG_PETITION_DECLINE: u32 = 0x1C2;
pub const CMSG_OFFER_PETITION: u32 = 0x1C3;
pub const CMSG_TURN_IN_PETITION: u32 = 0x1C4;
pub const CMSG_PETITION_QUERY: u32 = 0x1C6;
pub const MSG_PETITION_RENAME: u32 = 0x2C1;

pub const CMSG_SEND_MAIL: u32 = 0x238;
pub const CMSG_GET_MAIL_LIST: u32 = 0x23A;
pub const CMSG_MAIL_TAKE_MONEY: u32 = 0x245;
pub const CMSG_MAIL_TAKE_ITEM: u32 = 0x246;
pub const CMSG_MAIL_MARK_AS_READ: u32 = 0x247;
pub const CMSG_MAIL_RETURN_TO_SENDER: u32 = 0x248;
pub const CMSG_MAIL_DELETE: u32 = 0x249;
pub const CMSG_MAIL_CREATE_TEXT_ITEM: u32 = 0x24A;
pub const MSG_QUERY_NEXT_MAIL_TIME: u32 = 0x284;

pub const MSG_AUCTION_HELLO: u32 = 0x255;
pub const CMSG_AUCTION_REMOVE_ITEM: u32 = 0x257;
pub const CMSG_AUCTION_LIST_OWNER_ITEMS: u32 = 0x259;
pub const CMSG_AUCTION_PLACE_BID: u32 = 0x25A;
pub const CMSG_AUCTION_LIST_BIDDER_ITEMS: u32 = 0x264;

/// Negotiated WM_Multiclass extension opcodes (`MulticlassProtocol.h`: WMC1 v3, 28-byte request on
/// `0x51B`, reply capped at 4096 bytes on `0x51A`). Never emitted in stock baseline without an
/// explicit `MulticlassRequest` call.
pub const SMSG_MULTICLASS_REPLY: u16 = 0x51A;
pub const CMSG_MULTICLASS_REQUEST: u32 = 0x51B;

/// 12340 pet, talent, stable and progression replies (`Opcodes.h`; producers named beside the
/// decoders in `gameplay/pet_talent.rs`). The pet bar is a whole-state packet with the Wrath-only
/// creature-family word; talent info carries both player dual-spec groups and pet talents.
pub const SMSG_PET_SPELLS: u16 = 0x179;
pub const SMSG_PET_MODE: u16 = 0x17A;
pub const SMSG_PET_ACTION_FEEDBACK: u16 = 0x2C6;
pub const SMSG_PET_ACTION_SOUND: u16 = 0x324;
pub const SMSG_PET_DISMISS_SOUND: u16 = 0x325;
pub const SMSG_PET_LEARNED_SPELL: u16 = 0x499;
pub const SMSG_PET_REMOVED_SPELL: u16 = 0x49A;
pub const SMSG_PET_GUIDS: u16 = 0x4AA;
/// `MSG_LIST_STABLED_PETS` is bidirectional. The client verb keeps the `u32` constant above; the
/// server reply arm matches this `u16` constant so `gameplay::parse` can type-check it directly.
pub const SMSG_LIST_STABLED_PETS: u16 = 0x26F;
pub const SMSG_STABLE_RESULT: u16 = 0x273;
pub const SMSG_TALENTS_INFO: u16 = 0x4C0;
pub const SMSG_TALENTS_INVOLUNTARILY_RESET: u16 = 0x4FA;
pub const SMSG_SUPERCEDED_SPELL: u16 = 0x12C;
pub const SMSG_REMOVED_SPELL: u16 = 0x203;

/// 12340 economy replies: trade, mail and auction (`Opcodes.h`; layouts in
/// `gameplay/economy.rs`). `MSG_QUERY_NEXT_MAIL_TIME` and `MSG_AUCTION_HELLO` are bidirectional,
/// so the server-reply arms get distinct `u16` names for the same numeric opcodes.
pub const SMSG_TRADE_STATUS: u16 = 0x120;
pub const SMSG_TRADE_STATUS_EXTENDED: u16 = 0x121;
pub const SMSG_SEND_MAIL_RESULT: u16 = 0x239;
pub const SMSG_MAIL_LIST_RESULT: u16 = 0x23B;
pub const MSG_QUERY_NEXT_MAIL_TIME_SERVER: u16 = 0x284;
pub const SMSG_RECEIVED_MAIL: u16 = 0x285;
pub const MSG_AUCTION_HELLO_SERVER: u16 = 0x255;
pub const SMSG_AUCTION_COMMAND_RESULT: u16 = 0x25B;
pub const SMSG_AUCTION_LIST_RESULT: u16 = 0x25C;
pub const SMSG_AUCTION_OWNER_LIST_RESULT: u16 = 0x25D;
pub const SMSG_AUCTION_BIDDER_LIST_RESULT: u16 = 0x265;
