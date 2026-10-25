# OpenCode checkpoint - cooldowns and channel timers (O2c claim and delivery)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `3755ed88` (combat-outcomes delivery)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O2.
Purpose: wire the 12340 cooldown and channel-timer packets the existing spell/cooldown owners
already consume, closing the cooldown half of the selected cast path. This file is the file
claim and the delivery record; the commit that adds it releases the claim.

## File claim

Editing:

- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: Astra's `crates/benilla-procgen/**`, `docs/procgen/**`,
`.scratch/project-b/compatibility/**`, `docs/architecture/**`; the stock 1.12 parsers and
cooldown/channel owners.

## Authoritative 12340 sources read

- `Unit::BuildCooldownPacket` (`Unit.cpp:17017-17036`): raw `u64` caster, a
  `SPELL_COOLDOWN_FLAG_*` byte, then `(u32 spell, u32 cooldown_ms)` pairs. The 1.12 packet has
  no flags byte.
- `Player::SendCooldownEvent` (`Player.cpp:11052-11055`) and `Player::SendClearCooldown`
  (`Player.cpp:14778-14781`): `(u32 spell, u64 caster)`.
- `Player::SendItemCooldown` (`Player.cpp:11787-11790`): `(u64 item, u32 spell)`; the client's
  fixed 30 s convention is unchanged.
- `Spell::SendChannelStart` (`Spell.cpp:5158-5177`) and `SendChannelUpdate`
  (`Spell.cpp:5143-5156`): packed caster guid, then spell/duration or time left. 12340
  broadcast to the set; the 1.12 events are self-only.
- `Opcodes.h`: 0x134 SPELL_COOLDOWN, 0x135 COOLDOWN_EVENT, 0x1DE CLEAR_COOLDOWN,
  0x0B0 ITEM_COOLDOWN, 0x139 CHANNEL_START, 0x13A CHANNEL_UPDATE. `SMSG_COOLDOWN_CHEAT`
  (0x1E1) is `STATUS_NEVER` in this core and is not wired.

## Repairs

- `SMSG_SPELL_COOLDOWN` parses the caster, consumes the 12340 flag byte (no 1.12 counterpart),
  and maps the pair list to `SessionEvent::SpellCooldowns`.
- `SMSG_COOLDOWN_EVENT`, `SMSG_CLEAR_COOLDOWN` and `SMSG_ITEM_COOLDOWN` map onto their
  existing events unchanged.
- `MSG_CHANNEL_START`/`MSG_CHANNEL_UPDATE` parse the packed caster guid and map to the
  self-only seam events only when the caster is the player; another unit's channel is dropped
  instead of starting the player's cast bar. The translation now takes the session's player
  guid to perform that gate.

## Remaining variants after this slice

- Auras (`SMSG_AURA_UPDATE[_ALL]`) remain the last named O2 child; they need an app-side
  reducer that synthesizes the 1.12 `UNIT_FIELD_AURA` fields and the `AuraFieldsChanged` edge,
  since 12340 sends auras as packets while the 1.12 bar is descriptor-driven.
- `SMSG_UPDATE_AURA_DURATION` does not exist in the 12340 table; aura durations ride
  `SMSG_AURA_UPDATE`'s duration flag and will land with that child.
- `SMSG_COOLDOWN_CHEAT` is never sent by this core.

## Pending checks

Authorized source work only: no build, test, formatter, linter, graphical, socket, WM, receiver
or live-server execution for this slice. Gate C remains unverified for this commit. Fixtures
still needed: a spell-cooldown list, a cooldown event/clear, an item cooldown, our own channel
start/update, and another unit's channel (must not move the player's bar).
