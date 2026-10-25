# OpenCode checkpoint - death and recovery wire path (O4)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `430c4ce7` (inventory repair)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O4.
Purpose: wire the 12340 death/release/ghost/resurrection messages to the existing death owner
through the stock `ClientCommand`/`SessionEvent` seam, and project the ghost flag the owner
reads. This file is the file claim and the delivery record.

## File claim

Editing:

- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: `crates/benilla-app/src/death/**` (the existing owner; only its inputs change),
Astra's procgen/compatibility/architecture paths.

## Authoritative 12340 sources read

- `MiscHandler.cpp:58-86` (`CMSG_REPOP_REQUEST`): the handler reads one byte.
- `MiscHandler.cpp:642-645` (`CMSG_RECLAIM_CORPSE`): raw `u64` corpse guid.
- `MiscHandler.cpp:677-698` (`CMSG_RESURRECT_RESPONSE`): raw `u64` caster and a status byte.
- `SpellHandler.cpp:705-719` (`CMSG_SELF_RES`): empty.
- `NPCHandler.cpp:233-252` (`CMSG_SPIRIT_HEALER_ACTIVATE`): raw `u64` healer guid.
- `QueryHandler.cpp:226-266` (`MSG_CORPSE_QUERY`): empty request; answer is found byte, i32 map,
  three floats, u32 corpse map.
- `Player.cpp:13007-13012` (`SMSG_CORPSE_RECLAIM_DELAY`): u32 ms.
- `Spell.cpp:5191-5200` (`SMSG_RESURRECT_REQUEST`): raw caster, u32 name length, name + NUL,
  sickness byte, and a u32 instant-res override only for `SPELL_ATTR3_NO_RES_TIMER`.
- `spell_generic.cpp:2735` (`SMSG_SPIRIT_HEALER_CONFIRM`): raw `u64` healer guid.
- `Opcodes.h`: 0x15A, 0x15B, 0x15C, 0x1D2, 0x216, 0x222, 0x269, 0x2B3, 0x21C.

## Repairs

- The six death `ClientCommand`s that the existing death owner emits now encode into the 3.3.5
  session instead of being dropped by the tap: release (one zero byte, as 12340 reads), the
  empty corpse query, reclaim (raw corpse guid), self-res (empty), spirit healer activate (raw
  healer guid) and resurrect response (raw caster + accept byte).
- The four death server messages parse and translate onto their existing seam events with the
  same values: corpse query (entrance-adjusted map and position, real corpse map), reclaim
  delay, resurrect request (with the 12340-only instant-res word mapped to `has_timer = false`)
  and the spirit healer confirm.
- `PLAYER_FLAGS` is now projected from 3.3.5 index 150 to 1.12 field 190, so the ghost bit
  (0x10) reaches the existing death owner; without it, releasing a corpse could never put the
  client into the ghost world.
- Nothing is fabricated: every value is the packet's; the world-transfer acknowledgement and
  scope teardown paths from the earlier slices are unchanged and still authoritative.

## Remaining gaps after this slice

- `SMSG_RESURRECT_FAILED` (0x252) has no stock event or UI arm; a refused resurrection is
  currently silent.
- Duel death, battleground spirit-healer timers and vehicle/graveyard edge cases remain
  ledger children (the stock `AreaSpiritHealerTime` path is not parsed by any wrath arm).
- The item durability-loss and `SMSG_PLAYER_SKINNED` notices are not parsed by the wrath path.

## Pending checks

Authorized source work only: no build, test, formatter, linter, graphical, socket, WM, receiver
or live-server execution. Gate C remains unverified. Fixtures still needed: release → ghost
flag delta → corpse query → reclaim; a player-cast resurrect offer; a spirit healer confirm;
and a death during a cast (the death owner's cancel edge).

Live death/recovery acceptance remains execution-gated.
