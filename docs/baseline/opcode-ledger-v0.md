# Opcode ledger v0 — minimum session transcript (ticket B-002 pass 1)

2026-10-24. All file:line references verified in the local AC tree this session. Codec coverage verified **compile-time** via probe crate (`D:/Projects/codec-probe`, wow_world_messages 0.3, feature `wrath`) — every transcript message imported as a struct must resolve; this is stronger than docs-page inspection.

## 1. Minimum transcript with local owners

### Auth → world session
| Message | Direction | Local owner (source-observed) | wrath codec | Adoption |
|---|---|---|---|---|
| SMSG_AUTH_CHALLENGE | S→C | `WorldSocket.cpp:224` (40-byte challenge) | ✅ struct | adopt+verify |
| CMSG_AUTH_SESSION | C→S | dispatch `WorldSocket.cpp:429`, handler `:529`, async callback `:555`; digest read `:543`, SHA-1 verify `:615-623` | ✅ struct | adopt+verify |
| CMSG_PING / SMSG_PONG | C↔S | dispatch `:416`, `HandlePing` `:734` (also latency + kick-on-overspeed logic) | ✅ struct | adopt+verify |
| Crypto transition | — | HMAC-SHA1 directional keys + ARC4-drop1024: `AuthCrypt.cpp:25-31`; session-key handoff `WorldSocket.cpp:582` area | n/a | implement (complement directions) |

### Character screen → entering world
| Message | Direction | Local owner | wrath codec | Adoption |
|---|---|---|---|---|
| CMSG_CHAR_ENUM → SMSG_CHAR_ENUM | C↔S | `CharacterHandler.cpp:246` HandleCharEnumOpcode | ✅ both | adopt+verify |
| CMSG_PLAYER_LOGIN | C→S | `CharacterHandler.cpp:665` HandlePlayerLoginOpcode → `HandlePlayerLoginFromDB` (`:784`+) | ✅ struct | adopt+verify |
| SMSG_LOGIN_VERIFY_WORLD | S→C | `CharacterHandler.cpp:806` — **before** the Player initial-packet helpers | ✅ struct | adopt+verify |
| SMSG_FEATURE_SYSTEM_STATUS | S→C | login holder (`:818` rel.) | ✅ (probe: not checked — verify in pass 2) | verify |
| SMSG_LEARNED_DANCE_MOVES | S→C | login holder (`:856` rel.) | not probed | verify in pass 2 |

### Login burst (Player initial packets)
| Message | Direction | Local owner | wrath codec | Adoption |
|---|---|---|---|---|
| SMSG_BINDPOINTUPDATE | S→C | `Player.cpp:11672` (SendInitialPacketsBeforeAddToMap) | ✅ struct | adopt+verify |
| SMSG_INSTANCE_DIFFICULTY, SMSG_SEND_UNLEARN_SPELLS | S→C | same function (`:11686`, `:1193` rel.) | not probed | verify in pass 2 |
| SMSG_LOGIN_SETTIMESPEED | S→C | same (`:1203` rel.) | ✅ struct | adopt+verify |
| SMSG_TIME_SYNC_REQ | S→C | `GetSession()->SendTimeSync()` (`:11726` rel.) | ✅ struct (SMSG_TIME_SYNC_REQ) | adopt+verify |
| **SMSG_MULTIPLE_MOVES (0x51E)** | S→C | `Player.cpp:11760` rel. — bundles SMSG_FORCE_MOVE_ROOT / MOVE_FEATHER_FALL / MOVE_WATER_WALK / MOVE_SET_HOVER compound states | ❌ **unofficial AC-lineage opcode — cannot be in the crate; fork implements natively** | local only |
| (remaining AfterAddToMap sends: conditional — SMSG_TRANSFER_ABORTED, SMSG_RAID_INSTANCE_MESSAGE, etc.) | S→C | `Player.cpp:11845+` | per-message | pass 2 |

### Object updates
| Message | Direction | Local owner | wrath codec | Adoption |
|---|---|---|---|---|
| SMSG_UPDATE_OBJECT | S→C | `Updates/UpdateData.cpp:66`; field layout `Updates/UpdateFields.h:21` (12340); flags `UpdateFieldFlags.cpp` | ✅ struct | adopt+verify (**diff crate's update-object field layout against local UpdateFields.h via fixtures before trusting**) |
| SMSG_COMPRESSED_UPDATE_OBJECT | S→C | UpdateData compression path | ✅ struct | adopt+verify (compression bounds = fixture case) |

### Movement (in-world)
| Message | Direction | Local owner | wrath codec | Adoption |
|---|---|---|---|---|
| MSG_MOVE_START_FORWARD / BACKWARD / STOP / STRAFE_L / STRAFE_R / STOP_STRAFE / JUMP / TURN_L / TURN_R / STOP_TURN / PITCH_* / RUN|WALK_MODE / FALL_LAND / START_SWIM / STOP_SWIM / HEARTBEAT | C→S | one handler: `MovementHandler.cpp:344` HandleMovementOpcodes; table `Opcodes.cpp:312-333+` (0x0B5…, all PROCESS_THREADSAFE); serializer `WorldSession::ReadMovementInfo` | ✅ structs (START_FORWARD, HEARTBEAT, JUMP, FALL_LAND probed; rest follow the same family) | adopt+verify |
| MSG_MOVE_TELEPORT_ACK | C→S | `MovementHandler.cpp:276` (HandleMoveTeleportAck; table `:330`) | ✅ struct | adopt+verify |
| MSG_MOVE_WORLDPORT_ACK | C→S | `MovementHandler.cpp:47` | ✅ struct | adopt+verify |
| CMSG_MOVE_KNOCK_BACK_ACK | C→S | `:813`; server sends MSG_MOVE_KNOCK_BACK `:838` | ✅ struct | adopt+verify |
| CMSG_MOVE_TIME_SKIPPED | C→S | `:877` (server mirrors `:901`) | not probed | verify pass 2 |
| CMSG_MOVE_NOT_ACTIVE_MOVER | C→S | `:784` | not probed | verify pass 2 |
| CMSG_MOVE_FALL_RESET / CMSG_MOVE_CHNG_TRANSPORT | C→S | special-cased `:389` | not probed | verify pass 2 |
| MSG_MOVE_SET_COLLISION_HGT (+ACK) | C↔S | `:701/:703` | not probed | verify pass 2 |
| SMSG_MONSTER_MOVE | S→C | MovementHandler send path (unit splines) | ✅ struct | adopt+verify |
| MovementInfo | payload | `WorldSession` reader; optional blocks (transport/velocity/swing/unk) per 3.3.5 flags | ✅ **struct confirmed** | adopt+verify (optional-block flags = fixture case) |

### Chat / time (diagnostics)
| Message | Direction | Local owner | wrath codec | Adoption |
|---|---|---|---|---|
| CMSG/SMSG_MESSAGECHAT | C↔S | ChatHandler | ✅ both | adopt+verify |
| CMSG_QUERY_TIME | C→S | (time sync loop) | ✅ struct | adopt+verify |

## 2. Codec adoption decision (v0)

- `wow_world_messages` 0.3 `wrath` feature: **adopt as reference codec behind `benilla-protocol`'s message boundary**, MIT/Apache. All 24 probed transcript structs + MovementInfo resolve compile-time. The crate exposes per-message structs (no enum we need) — fits benilla's typed-decode style.
- Everything adopted is **adopt+verify**: local AC serializers + captured PKT fixtures are authoritative; on any mismatch the crate yields.
- **Not adoptable by construction:** SMSG_MULTIPLE_MOVES (0x51E, unofficial AC-lineage — the stock client's handling is itself a verify item) and CMSG_WM_MULTICLASS (0x51B, ours).

## 3. Extension-ID ledger (updated — source-observed in Opcodes.h this session)

| Slot | Name | Defined at | Notes |
|---|---|---|---|
| 0x51B | `CMSG_WM_MULTICLASS` | `Opcodes.h:1343` area — **in the core opcode table**, comment "Scoped protocol: receive slot 0x51A is empty in both Glue and world client" | multiclass client extension; client-pin.json agrees |
| 0x51A | (receive slot) | same comment | "confirmed empty in both Glue and the active world client"; client "refuses a nonempty receive slot" |
| 0x51E | `SMSG_MULTIPLE_MOVES` | `Opcodes.h:1340`, "uncompressed version of SMSG_COMPRESSED_MOVES" | AC-lineage unofficial; actively sent in login burst (`Player.cpp` AfterAddToMap) — the fork must tolerate/decode it |

Implication for Project B extension allocation: any new fork opcode must avoid 0x51A/0x51B (owned) and account for 0x51E (in use); allocation policy itself deferred to ticket B-010/G-1 per plan §7.

## 4. Risk register (adopted codecs)

1. Update-object field layout: crate's structs vs local `UpdateFields.h` (both claim 3.3.5.12340) — settle with round-trip fixtures from live captures before trusting either.
2. MovementInfo optional blocks (3.3.5 transport/velocity flags) — fixture-verify which blocks the stock client emits per opcode.
3. Compressed update bounds — dedicated fixture (multi-packet + compressed).
4. SMSG_MULTIPLE_MOVES semantics — capture from live server login; decide fork handling (decode-and-ignore vs apply compound states).
5. Large-packet framing — local WorldSocket framing is the spec; fixture both header sizes.

## 5. Open items (pass 2 — during B-003 implementation)

- [ ] Full AfterAddToMap send list (conditional messages) once captures exist
- [ ] Probe pass 2 for: SMSG_FEATURE_SYSTEM_STATUS, SMSG_LEARNED_DANCE_MOVES, SMSG_INSTANCE_DIFFICULTY, SMSG_SEND_UNLEARN_SPELLS, CMSG_MOVE_TIME_SKIPPED, CMSG_MOVE_NOT_ACTIVE_MOVER, CMSG_MOVE_FALL_RESET, CMSG_MOVE_CHNG_TRANSPORT, MSG_MOVE_SET_COLLISION_HGT, remaining MSG_MOVE family
- [ ] addon-message channel handling (SMSG_MESSAGECHAT LANG_ADDON — the WM_FORGE2/3 transport) in the fork
