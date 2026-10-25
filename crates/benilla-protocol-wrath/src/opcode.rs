//! Transcript opcodes, values taken verbatim from the local `Opcodes.h` (build 12340 table).

#![allow(dead_code)]

pub const CMSG_CHAR_ENUM: u32 = 0x037;
pub const SMSG_CHAR_ENUM: u16 = 0x03B;
pub const CMSG_PLAYER_LOGIN: u32 = 0x03D;
pub const SMSG_LOGIN_SETTIMESPEED: u16 = 0x042;
/// The empty handler: the stock client never sends it (Astra F04; MiscHandler.cpp:485).
pub const CMSG_PLAYER_LOGOUT: u32 = 0x04A;
/// The real logout request -> HandleLogoutRequestOpcode (MiscHandler.cpp:425).
pub const CMSG_LOGOUT_REQUEST: u32 = 0x04B;
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

/// `SMSG_COMPRESSED_UPDATE_OBJECT` (`WorldSocket.cpp:97-119`): the payload is
/// `[u32 uncompressed_size][zlib(update-object payload)]` — no inner opcode; the decompressed
/// bytes are the plain update body. (The fixture first misled us into reading an inner opcode;
/// the "0x0016" was a 22-block world snapshot's count field.)
pub const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x1F6;
pub const MSG_MOVE_HEARTBEAT: u32 = 0x0EE;
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
