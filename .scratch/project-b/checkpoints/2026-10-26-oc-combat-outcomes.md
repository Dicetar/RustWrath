# OpenCode checkpoint - combat outcomes (O2b claim and delivery)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `ccb25a5a` (cast fidelity delivery)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O2.
Purpose: wire the 12340 combat-outcome packets the stock combat log, swing animation, aura tick
and auto-repeat owners already consume, so the pinned Northshire kill path produces server-
authoritative swings, damage, ticks and cancels. This file is the file claim and the delivery
record; the commit that adds it releases the claim.

## File claim

Editing:

- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: Astra's `crates/benilla-procgen/**`, `docs/procgen/**`,
`.scratch/project-b/compatibility/**`, `docs/architecture/**` (Astra's claim commit `a44c9686`);
the stock 1.12 parsers and event owners.

## Authoritative 12340 sources read

- `Unit::SendAttackStateUpdate` (`Unit.cpp:6870-6960`): `HitInfo`, packed attacker and victim,
  `TotalDamage`, `Overkill`, a `u8` sub-damage count of `{schoolMask u32, float damage, u32
  damage}`, conditional `u32` absorb words (`HITINFO_FULL_ABSORB|PARTIAL_ABSORB`), conditional
  `u32` resist words (`HITINFO_FULL_RESIST|PARTIAL_RESIST`), `TargetState u8`, two `u32` words,
  the melee spell id, then conditional `u32` block, `u32` rage gain and the debug tail
  (`HITINFO_UNK1`). This differs from 1.12, where absorb/resist sit inside every sub-block and
  `TargetState` is a `u32`.
- `Unit::SendSpellNonMeleeDamageLog` (`Unit.cpp:6679-6724`): packed target and attacker, spell,
  damage, `Overkill u32`, `schoolMask u8`, absorb, resist, physical-log byte, unused byte,
  blocked, `HitInfo`, debug byte.
- `Unit::SendPeriodicAuraLog` (`Unit.cpp:6780-6842`): packed target and caster, spell, count,
  aura type, then type-shaped fields: damage (`amount, overkill, schoolMask u32, absorb, resist,
  critical u8`), heal (`amount, overheal, absorb, critical u8`), energize (`power, amount`),
  mana leech (`power, amount, multiplier f32`).
- `Unit::SendPlaySpellVisual` (`Unit.cpp:15149-15162`): raw `u64` guid and kit id.
- `Unit::SendAIReaction` (`Unit.cpp:12995-12998`): raw `u64` guid and `u32` reaction.
- `Player::SendAttackSwingCancelAttack` (`PlayerMisc.cpp:141-145`): empty `SMSG_CANCEL_COMBAT`.
- `Player::SendAutoRepeatCancel` (`PlayerMisc.cpp:153-158`): a packed target guid.
- `Opcodes.h`: 0x13C AI_REACTION, 0x14A ATTACKERSTATEUPDATE, 0x14E CANCEL_COMBAT,
  0x1F3 PLAY_SPELL_VISUAL, 0x24E PERIODICAURALOG, 0x250 SPELLNONMELEEDAMAGELOG,
  0x29C CANCEL_AUTO_REPEAT.

## Repairs

- `SMSG_ATTACKERSTATEUPDATE` parses the 12340 body and maps to the existing `AttackerState`
  seam: sub-block school from the first block, absorb/resist only from their conditional
  blocks, `TargetState u8`, the melee spell id, and `blocked` only when `HITINFO_BLOCK` is set.
  `Overkill` and the two unknown words are consumed and dropped; the `HITINFO_UNK1` debug tail
  is left unread (the packet is not re-parsed) rather than guessed.
- `SMSG_SPELLNONMELEEDAMAGELOG` maps to the existing `SpellDamageLog` seam; the 12340
  `physicalLog` byte is not the seam's `periodic` flag, so `periodic` stays false and periodic
  damage keeps riding `SMSG_PERIODICAURALOG`.
- `SMSG_PERIODICAURALOG` maps to the existing `PeriodicAuraLog` seam, walking its counted tick
  list and dropping the 3.3.5-only overkill/critical fields; an unknown aura type is a parse
  error (its payload width is unknowable), as the 1.12 reader already treats it.
- `SMSG_AI_REACTION`, `SMSG_PLAY_SPELL_VISUAL` and `SMSG_CANCEL_COMBAT` map straight onto the
  existing events; `SMSG_CANCEL_AUTO_REPEAT` consumes its packed target guid and maps to the
  guid-less 1.12 event.
- No success, damage or state is synthesized: every value comes from the packet.

## Remaining variants after this slice

- Cooldowns and channel timers (`SMSG_SPELL_COOLDOWN`, `SMSG_COOLDOWN_EVENT`,
  `SMSG_CLEAR_COOLDOWN`, `SMSG_ITEM_COOLDOWN`, `MSG_CHANNEL_START`, `MSG_CHANNEL_UPDATE`) and
  auras (`SMSG_AURA_UPDATE[_ALL]`) remain the next O2 children.
- `SMSG_SPELLLOGMISS`, `SMSG_SPELLHEALLOG`, `SMSG_SPELLENERGIZELOG`, `SMSG_SPELLDAMAGESHIELD`
  and the environmental-damage log are not parsed by any wrath arm yet; they map onto existing
  seam events when wired.
- The `HITINFO_UNK1` debug tail is not consumed (never set by a stock core).

## Pending checks

Authorized source work only: no build, test, formatter, linter, graphical, socket, WM, receiver
or live-server execution for this slice. Gate C remains unverified for this commit. Fixtures
still needed: a normal melee swing, a blocked swing, an absorbed spell hit, a periodic damage
tick, an AI reaction and a cancelled auto-repeat.
