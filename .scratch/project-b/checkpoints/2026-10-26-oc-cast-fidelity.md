# OpenCode checkpoint - cast packet fidelity (O2a claim and delivery)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `df8c7527` (lifecycle consolidation delivery); `1d6c5a50` work package intact
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O2.
Purpose: make `SMSG_SPELL_START`/`SMSG_SPELL_GO` carry their full 12340 target block and cast-flag
payloads onto the existing 1.12 spell seam, and add the pushback/interrupt edges the cast bar
already consumes. This file is the file claim (written before the source edits) and the delivery
record; the commit that adds it releases the claim.

## File claim

Editing:

- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: `crates/benilla-procgen/**`, `docs/procgen/**`,
`.scratch/project-b/compatibility/**` (Astra), the stock 1.12 parsers and spell owners, the
`benilla-wm-native` sources.

## Authoritative 12340 sources read

- `Spell::SendSpellStart` / `SendSpellGo` / `WriteAmmoToPacket` (`Spell.cpp:4669-4975`):
  first slot is the cast item's packed guid or the caster's, second is the caster, then the
  u8 cast count, u32 spell, u32 cast flags, and (START) the i32 remaining timer; targets follow.
  Optional payload order is fixed by the flag tests in those functions.
- `SpellCastTargets::Write` (`Spell.cpp:180-215`): u32 mask; one packed object guid for
  UNIT/CORPSE_ALLY/GAMEOBJECT/CORPSE_ENEMY/UNIT_MINIPET; item/trade-item packed guid; source
  and dest as a packed transport guid plus three floats (world position when the transport is
  empty); string target.
- `SpellCastTargetFlags` (`SpellInfo.h:45-67`) and `SpellCastFlags` (`Spell.h:55-84`).
- `Spell::SendInterrupted` (`Spell.cpp:5126-5141`): 0x133 and 0x2A6 carry the same
  caster/cast-count/spell/reason body; the 1.12 seam's event is `SMSG_SPELL_FAILED_OTHER`.
- `Spell::Delayed` (`Spell.cpp:7824-7828`): packed caster plus u32 pushback.
- `Opcodes.h` (12340 table): `SMSG_SPELL_DELAYED` 0x1E2, `SMSG_SPELL_FAILED_OTHER` 0x2A6.

## Repairs

- `SMSG_SPELL_START`/`SMSG_SPELL_GO` now parse the full `SpellCastTargets`: u32 mask, the
  object/client guid in the source's priority (UNIT or MINIPET -> unit, else GAMEOBJECT -> go),
  the target item guid, source/dest points with their transport guids, and the string target.
- Cast-flag payloads are consumed in the source's order: START `POWER_LEFT_SELF`, `PROJECTILE`
  ammo (display + inventory type), `UNKNOWN_23`; GO `POWER_LEFT_SELF`, `RUNE_LIST`, `ADJUST_MISSILE`,
  `PROJECTILE` ammo, `VISUAL_CHAIN`, and the DEST_LOCATION marker byte. The rune list's per-rune
  byte count is derived from the before/after masks exactly as the writer loops it.
- The wrath event keeps the full u32 cast flags and u32 target mask; the translation onto the
  1.12 seam maps bits explicitly instead of truncating: pending 0x1 shares vanilla's
  hidden-combat-log bit, projectile 0x20 shares vanilla's ammo bit, a GO always carries the
  vanilla 0x100 launch marker, and the mini-pet target bit maps onto vanilla UNIT. The vanilla
  target mask is the shared low 16 bits with that one exception; 3.3.5-only bits are not copied.
- A source/dest point whose transport guid is non-zero is transport-relative: it is not turned
  into a world point (no fabricated coordinates); the transport guid is kept in the wrath
  target record and the seam sees no point.
- `SMSG_SPELL_DELAYED` now maps to `SessionEvent::SpellDelayed` (pushback).
- `SMSG_SPELL_FAILED_OTHER` (0x2A6) replaces `SMSG_SPELL_FAILURE` (0x133) as the parser for the
  observed-interruption edge: 12340 sends both in `SendInterrupted`, and mapping both would
  double the `SpellFailedOther`/`CastEvent::Fail` edge. The caster's own refused cast still rides
  `SMSG_CAST_FAILED` (0x130) as before. 0x133 is now deliberately untranslated.
- `SMSG_CAST_FAILED` argument words now follow 12340 `WriteCastResultInfo`: one word for
  TOO_MANY_OF_ITEM (129) and CUSTOM_ERROR (172) added; NEED_MORE_ITEMS (55) and MIN_SKILL (150)
  send two words and only the first is read, matching the seam's single `arg`; the totem reasons
  (130, 131) send zero, one or two words depending on the spell's DBC, so no word is read for
  them (bounded gap recorded below).

## Remaining variants after this slice

- `SMSG_AURA_UPDATE[_ALL]` (aura add/update/remove) is not wired yet. The 1.12 aura bar is
  descriptor-driven (`UNIT_FIELD_AURA`, fields 47-100), and 12340 sends auras as packets, so
  this needs its own app-side reducer that synthesizes those fields and the
  `AuraFieldsChanged` edge. It is the next O2 child.
- Cooldown packets (`SMSG_SPELL_COOLDOWN` 0x134, `SMSG_COOLDOWN_EVENT` 0x135,
  `SMSG_CLEAR_COOLDOWN` 0x1DE, `SMSG_ITEM_COOLDOWN` 0x0B0) and the channel timers
  (`MSG_CHANNEL_START`/`UPDATE`) are parsed by no wrath arm yet; the existing owners accept the
  1.12 events unchanged. Next O2 child after auras.
- 3.3.5 `SMSG_SPELL_FAILURE` (0x133) is intentionally not translated (see above).
- Cast-result totem reasons drop their argument words instead of guessing the DBC count.
- `m_timer` is the remaining cast time at send; it is used as the seam's `cast_time_ms`, as the
  existing translation already did.

## Pending checks

Authorized source work only: no build, test, formatter, linter, graphical, socket, WM, receiver
or live-server execution for this slice. Gate C remains unverified for this commit. The parsers
still need fixture/replay coverage for: a unit-target Fireball START and GO, a GO-targeted cast,
a source/dest ground-targeted cast, an interrupted cast (0x2A6), and a pushed-back cast (0x1E2).
