# Wrath wire audit: movement flags, update tails and opcode constants

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/wrath-wire-audit
Issue: none
Next: integrator reads the diff and the note; the one end-of-task CI run covers `benilla-protocol-wrath`

## Scope

Audit of `crates/benilla-protocol-wrath` (`updates.rs`, `reducer.rs`, `gameplay.rs`,
`gameplay/*.rs`, `session.rs`, plus the `opcode.rs` constant table) for the class of bug fixed
in `029c41f98`: a flag/bit constant or an optional-field gate that does not match what the local
AzerothCore tree writes. Server source read-only at
`D:\Projects\wow\server\Azerothcore_WoTLK_Rebuild\source\azerothcore\src\server\game`;
client reference at `D:/Projects/wow/reference/wowdecomp-reference` pin `e67b6ffef`. No server,
client or app file was modified; `benilla-app` was not touched.

Also read: the integration checkout's local session-failure log directory. It contains exactly
one failure, `unknown update block type 42` on the compressed update (`0x1F6`), already covered
by `029c41f98` (transport `time2` gate). No second decode failure is recorded there.

## Real mismatches fixed

### 1. Opcode constants (`src/opcode.rs`)

Compared every constant that shares a name with `Server/Protocol/Opcodes.h`. Eight values were
wrong; the first seven were found by the name/value diff, the ready-check one by inspecting the
`_SERVER` aliases (client-only names).

| constant | was | now | AC source |
|---|---|---|---|
| `SMSG_FORCE_SWIM_BACK_SPEED_CHANGE` | 0x13A | 0x2DC | Opcodes.h:762 |
| `CMSG_FORCE_SWIM_BACK_SPEED_CHANGE_ACK` | 0x13B | 0x2DD | Opcodes.h:763 |
| `SMSG_FORCE_TURN_RATE_CHANGE` | 0x13C | 0x2DE | Opcodes.h:764 |
| `CMSG_FORCE_TURN_RATE_CHANGE_ACK` | 0x13D | 0x2DF | Opcodes.h:765 |
| `CMSG_MOVE_SPLINE_DONE` | 0x2E9 | 0x2C9 | Opcodes.h:743 |
| `CMSG_MOVE_TIME_SKIPPED` | 0x319 | 0x2CE | Opcodes.h:748 |
| `CMSG_BATTLEFIELD_LIST` | 0x2D8 | 0x23C | Opcodes.h:602 |
| `MSG_RAID_READY_CHECK_CONFIRM_SERVER` | 0x323 | 0x3AE | Opcodes.h:972 |

The old 0x13A-0x13D values are `MSG_CHANNEL_UPDATE`, `CMSG_CANCEL_CHANNELLING`,
`SMSG_AI_REACTION` and `CMSG_SET_SELECTION`. `SMSG_AI_REACTION` is matched earlier in
`gameplay::parse`, so AI reactions still decoded; the cost was that the force-swim-back and
force-turn-rate orders were silently unrecognised and the three client requests went out under
another message's opcode. `0x2E9` is `MSG_BATTLEGROUND_PLAYER_POSITIONS`, `0x319` is
`MSG_MOVE_TIME_SKIPPED`, `0x2D8` is `CMSG_MOVE_START_SWIM_CHEAT`, `0x323` is
`CMSG_LUA_USAGE`.

Live consumers (inspection only): `SMSG_FORCE_TURN_RATE_CHANGE` is dispatched by
`gameplay/mover_control.rs:87`; the three `CMSG_*` values are used by
`crates/benilla-app/src/wrath.rs:8977,9193,9221` (fixed through the shared table, app untouched).

Test: `opcode::tests::speed_ack_and_server_opcodes_match_the_azerothcore_table`
(`src/opcode.rs:772`) pins the eight values and asserts the old collisions no longer exist.

### 2. Environmental damage: resisted/absorbed order (`src/gameplay.rs:4101`)

The writer emits `victim, type, damage, Resisted, Absorbed`
(`WorldPackets::CombatLog::EnvironmentalDamageLog::Write`, `Server/Packets/CombatLogPackets.cpp:20-27`;
`Player::EnvironmentalDamage` fills them at `Player.cpp:799-804`). The decoder read absorbed
before resisted, copied from the 12340 client handler (which reads the two the other way). The
words are now read in the writer's order, so the combat-log absorb flag no longer reports the
resist value and vice versa.

Test: `gameplay::tests::environmental_damage_reads_the_writers_resisted_then_absorbed`
(`src/gameplay.rs:7808`) with the writer's exact 21-byte body.

### 3. Periodic aura: over-read past the crit byte (`src/gameplay.rs:4191`)

`Unit::SendPeriodicAuraLog` ends a damage tick at `uint8(critical)`
(`Unit.cpp:6803-6808`; heal arm at `:6811-6816`). The decoder read one more `hitInfo` byte
because the 12340 client handler does. Since the server always sends `count = 1` and the packet
ends there, every periodic-damage packet failed as truncated; with more than one tick it also
shifted the next tick. `hit_info` is now derived (2 on crit, else 0), which is the reference's
own rewrite, without consuming a byte this server does not write.

Test: `gameplay::tests::periodic_damage_tick_ends_at_the_crit_byte`
(`src/gameplay.rs:7837`): two back-to-back ticks built from the writer layout.

### 4. Relayed-movement gate (`src/session.rs:859`, `src/opcode.rs:310-312`)

`is_relayed_movement_opcode` listed `0x2CE..=0x2D0` (client->server time-skip/feather/water
acks, never relayed) and omitted `0x359/0x35A/0x3A7`, which `HandleMovementOpcodes` does relay
with a packed mover guid and `MovementInfo` (`Opcodes.cpp:988-989,1066`;
`MovementHandler.cpp:320-395`). The ascends/descent of flying units therefore never updated the
replicated position. Added the three constants and dropped the client-only range; the time-skip
broadcast is `MSG_MOVE_TIME_SKIPPED` 0x319 with a packed guid + u32, not a movement block, so it
stays out.

Test: `ascend_and_descend_relays_update_an_observer_position`
(`crates/benilla-protocol-wrath/tests/session.rs:140`).

### 5. Neighbouring test's clippy deny (merged from `origin/main`)

The `029c41f98` regression test used `3.14` as an unused speed sentinel; `clippy::approx_constant`
is deny-by-default and failed the crate's test target. `origin/main` fixed it in `0e1c59530`
(`3.5`); this branch carried the same fix and the end-of-task merge keeps main's value. Test
intent unchanged.

## Checked and correct (no change)

Update path:

- `UPDATEFLAG_*` values (`UpdateData.h:36-47`) and the tail write order UNKNOWN, LOWGUID,
  HAS_TARGET, TRANSPORT, VEHICLE, ROTATION (`Object.cpp:439-486`). The reader's order matches.
- Update block types 0/1/2/3/4 and the out-of-range block's `u32 count + packed guids`
  (`UpdateData.cpp:54-70`); the block count is `u32`.
- Living movement: `Unit::BuildMovementPacket` (`Unit.cpp:15867-15903`) then nine speeds then
  the spline (`Object.cpp:349-358`). Gates: ON_TRANSPORT 0x200, FALLING 0x1000,
  SPLINE_ELEVATION 0x0400_0000, SPLINE_ENABLED 0x0800_0000, pitch on SWIMMING|FLYING (0x0220_0000)
  or `MOVEMENTFLAG2_ALWAYS_ALLOW_PITCHING` 0x20; transport `time2` only on
  `MOVEMENTFLAG2_INTERPOLATED_MOVEMENT` 0x400 (`UnitDefines.h:435,440`, `Unit.cpp:15894`).
  Today's fix is correct and complete.
- Spline create block (`MovementPacketBuilder.cpp WriteCreate`): duration/time/id, the two
  duration mods, vertical accel, effect start, node count, mode and final destination; facing
  checks Final_Angle 0x20000, Final_Target 0x10000, Final_Point 0x8000 in the writer's order
  (`MoveSplineFlag.h:46-48`). The reader matches, including the u64 target.
- Non-living POSITION/STATIONARY blocks (`Object.cpp:360-437`) including the corpse extra
  float and the raw-u64 transport guid replaced by `uint8(0)`.
- Values block: `u8` word count, one `u32` mask per word, ascending set bits
  (`Object.cpp:508-512`, `UpdateMask`). Unchanged fields are not fabricated.
- `SMSG_DESTROY_OBJECT` raw u64 + on-death byte (`Object.cpp:278-286`);
  `SMSG_COMPRESSED_UPDATE_OBJECT` `u32 size + zlib` (`WorldSocket.cpp:97-119`).
- `MSG_MOVE_TELEPORT` packed guid + `BuildMovementPacket` (`Unit.cpp:16198`);
  `MSG_MOVE_TELEPORT_ACK` packed guid + `u32 counter` + movement (`Player.cpp:1375`);
  relayed movement bodies `WorldSession::WriteMovementInfo` (`WorldSession.cpp:1145-1184`).
- `SMSG_PLAYER_VEHICLE_DATA` packed guid + `u32` (`Unit.cpp:10504-10506`).
- `SMSG_MONSTER_MOVE`/`_TRANSPORT`: packed mover (and packed transport + seat), the movement
  byte, start point, spline id, facing kind, flags, optional animation pair, duration, optional
  parabolic pair, then catmull-rom or linear path, mode and final destination
  (`MoveSplineInit.cpp:114-177`, `MovementPacketBuilder.cpp`).
- Aura flags `AFLAG_CASTER` 0x08 and `AFLAG_DURATION` 0x20 (`SpellAuraDefines.h:26-34`,
  `SpellAuras.cpp:187-218`).

Gameplay/session surfaces:

- Character enum 23 visual slots and the first-login zone rule (`Player.cpp:1106-1299`).
- Item query response (`ItemHandler.cpp:413-536`): `StatsCount` (max 10), two damages, seven
  resistances, five spell rows, the seven-slot socket block, `RequiredDisenchantSkill`,
  `ArmorDamageModifier`, duration and limit category; only `HolidayId` is left unread at the end.
- Creature/game-object query responses (`QueryHandler.cpp:112-235`).
- Spell start/go (`Spell.cpp:4669-4830`): two packed casters, cast count, spell, cast flags,
  timer/timestamp, hit/miss arrays with the reflect byte at miss 11, `SpellCastTargets::Write`,
  power/ammo/unknown-23/rune/adjust-missile/visual-chain/dest-marker order. Target flag values
  match `SpellInfo.h:45-66`.
- Cast-failure argument words (`Spell.cpp:4521-4632`): the one-word reasons 29-31, 54, 55, 100,
  101, 102, 129, 147, 150, 172. Two-word and totem cases leave the extra word unread, as before.
- Initial spells (`Player.cpp:2730-2800`): the writer declares `m_spellCooldowns.size()` but skips
  `needSendToClient == false` rows; the reader walks complete 16-byte rows and reports the
  declared count separately.
- Party member stats mask 0x1-0x80000 and alien aura rows (`Group.h:97-122`,
  `GroupHandler.cpp:756-1000`); group list LFG gate 0x08 and the loot tail gated on
  `members - 1 != 0` (`Group.cpp:1696-1745`); guild roster offline `lastSave` tail
  (`GuildPackets.cpp:103-120`).
- Contact/friend, friend-status tails (`SocialMgr.cpp:124-177,250-292`); channel notify/list.
- Trade status conditional tails and the 72-byte trade row (`TradeHandler.cpp:33-118`); mail
  rows and attachment rows (`MailHandler.cpp:642-750`); auction row and command tail
  (`AuctionHouseSearcher.cpp:434-459`, `AuctionHouseHandler.cpp:76-85`).
- Trainer/vendor/bank/equipment-set/talent/pet-spell/stable bodies.
- Combat log: heal, energize, spell damage, spell miss (reflect 11), dispel/steal/dispel-failed,
  execute-log effect ids (`SpellEffects.cpp` handler table), instakill, enchant, damage shield,
  attacker state with `HITINFO_*` 0x20/0x40/0x80/0x100/0x2000/0x0080_0000 (`Unit.h:97-122`),
  party kill.
- LFG join/queue/update/proposal, battlefield status, PvP log arena/bg arms, calendar, runes,
  and the vehicle movement encoders.
- `src/fields.rs`: all 381 indices equal `UpdateFields.h` (automated diff, zero mismatches).
- `opcode.rs`: after the fix every name shared with `Opcodes.h` has the same value. Remaining
  same-value aliases are the `_SERVER`/donor names and two custom multiclass opcodes.

## Observed, not changed

- `SMSG_ATTACKSTOP` writes a trailing `u32 victim->isDead()` when a victim is present
  (`Unit.cpp:3242-3250`, `CombatHandler.cpp:86-95`). The decoder reads the packed victim and
  keeps the suffix as unread tail, so nothing desyncs; the dead flag is simply not surfaced.
- `SMSG_TRANSFER_PENDING`'s optional transport entry + current map tail is unread; the map word
  is what the session needs.
- `MSG_MOVE_SET_COLLISION_HGT` (0x518) is relayed with a movement body plus a float
  (`MovementHandler.cpp:704-707`) and is not reduced; niche, and not part of the movement gate
  above.
- The custom multiclass opcodes 0x51A/0x51B collide with AC's
  `CMSG_CLEAR_HOLIDAY_BG_WIN_TIME`/`CMSG_COMMENTATOR_SKIRMISH_QUEUE_COMMAND`; they are
  unreferenced by this AC tree. Left as the WM extension's own values, outside the 12340 path.
- Pre-existing clippy warnings remain (`sha1s` unused in the cryptprobe bin, `.err().expect()`
  in `auth.rs:274`, match-as-let in `gameplay.rs:7201`). Not introduced here and not errors.

## Verification

- `cargo test -p benilla-protocol-wrath` (own worktree, `CARGO_TARGET_DIR=F:\AI\tmp\opencode\target-wrath-wire-audit`,
  sccache wrapper active via the full path): lib 152 passed / 1 ignored (the pre-existing
  `#[ignore = "known failure on main (2026-11-18 CI baseline)"]`), replay 2 passed, session 16
  passed. Named new tests listed above all pass.
- `cargo clippy -p benilla-protocol-wrath --all-targets`: no errors; only the pre-existing
  warnings above.
- Client not run and no server change made; the audit is source-level only.
- CI: not run yet. The task's one run is the final `HEAD:main` push.

## Files changed

- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-protocol-wrath/src/session.rs`
- `crates/benilla-protocol-wrath/src/updates.rs` (clippy-only test constant; merge takes main's `0e1c59530`)
- `crates/benilla-protocol-wrath/tests/session.rs`
- `.scratch/project-b/handoffs/2026-11-22-wrath-wire-audit.md`

## Next

Integrator review and merge. The three protocol fixes are local to the Wrath wire layer; no app,
server, asset or runtime state changed. The open behavioural check is still a live client session
in a vehicle, in a spell-damage fight, and in an LFG/battleground flow, which only the owner can
produce.
