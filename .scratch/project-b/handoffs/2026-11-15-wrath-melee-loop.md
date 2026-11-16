# Wrath melee loop: target, auto-attack, kill

Status: in progress
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/wrath-melee-loop
Issue: none
Next: overseer source review and focused run; then the owner check list below

Matrix row: F05 (combat, resources and death). Source-only change: nothing here was compiled,
no test was executed and no client or server was started.

## What was wrong

The Wrath packets themselves decoded correctly, but the shared 1.12 combat consumers were fed
12340's raw words. Two encodings differ:

| Field | 12340 | 1.12 seam |
|---|---|---|
| melee `HitInfo` crit | `0x200` | `0x80` |
| full/partial absorb | `0x20` / `0x40` | `0x20` |
| full/partial resist | `0x80` / `0x100` | `0x40` |
| block | `0x2000` | `0x800` |
| glancing / crushing | `0x10000` / `0x20000` | `0x4000` / `0x8000` |
| no animation | `0x40000` | `0x10000` |
| sub-damage school | mask (`1`, `2`, `4`, ...) | index (`0`..`6`) |

Consequences before this change: every Wrath crit was an ordinary hit (no big floating number,
wrong `COMBATHIT` family), a full resist produced no line at all, a block reached the portrait
as a blank descriptor, a glancing blow suppressed the swing animation, every hit on a player
victim looked like a crushing blow to the sound layer, and a normal wolf swing's school mask
`1` read as non-physical Holy damage in the chat line and portrait.

The Wrath snapshot adapter also bypassed the 1.12 values-block watchers for
`UNIT_DYNAMIC_FLAGS` and `UNIT_FLAGS`. The target frame therefore did not receive the
`UNIT_DYNAMIC_FLAGS` event when a mob became tapped or lootable, so its tapped/lootable state
could stay stale. `UNIT_FLAGS` had the same missing event, although the player's
`PLAYER_REGEN_DISABLED/ENABLED` edge was already emitted explicitly.

## What changed

- `wrath::vanilla_hit_info` translates 12340's melee `HitInfo` into the 1.12 bit layout and
  drops 12340's `0x4000`/`0x8000` words so they cannot masquerade as vanilla glancing/crushing.
- `wrath::vanilla_school` turns 12340's spell-school mask into the 1.12 `0`..`6` index for
  melee swings, `SMSG_SPELLNONMELEEDAMAGELOG` and periodic damage ticks.
- `translate_gameplay` applies both at the Wrath->1.12 seam, so the existing combat log,
  floating text, portrait feedback, animation and sound owners keep their 1.12 laws.
- The Wrath world projection now emits `UNIT_DYNAMIC_FLAGS` on any dynamic-flags edge, plus
  `UNIT_FACTION` on the tap bit and `UNIT_HEALTH` on the dead bit, and `UNIT_FLAGS` on any
  unit-flags edge. This mirrors the values-block watcher the snapshot adapter bypasses.
- Added five parser cases in `benilla-protocol-wrath` and five seam cases in `benilla-app`.

## Message-by-message status

- `SMSG_ATTACKSTART`: raw attacker/victim guids already decoded; sets `Engaged` and raises
  `PLAYER_ENTER_COMBAT` for the active player.
- `SMSG_ATTACKSTOP`: packed attacker/victim already decoded; clears `Engaged`, flushes the
  pending swing, and raises `PLAYER_LEAVE_COMBAT` for the active player.
- `SMSG_ATTACKSWING_NOTINRANGE` / `_BADFACING`: decoded and latched; the UI error text is
  `ERR_BADATTACKPOS` / `ERR_BADATTACKFACING`, repeating on the reference's 4 s latch.
- `SMSG_ATTACKSWING_DEADTARGET` / `_CANT_ATTACK`: decoded into the shared silent-stop arm.
  12340's handler only calls `SetCombatMode()` (stop the attack, cancel the melee spell); it
  shows no error text, so the 1.12 silent stop is kept. The stop does not carry the 12340
  `SetCombatMode` auto-ranged/melee-spell cancellation; see the gaps below.
- `SMSG_ATTACKERSTATEUPDATE`: decoded, then hit info + school translated. Hit, miss, dodge,
  parry, block, crit, absorb and resist now reach the chat line, the floating number/word,
  portrait `UNIT_COMBAT`, swing animation and sound owners with their 1.12 meanings.
- `SMSG_CANCEL_COMBAT`: decoded and stops the attack through the same refusal seam.
- `PLAYER_REGEN_DISABLED` / `PLAYER_REGEN_ENABLED`: derived from the player's
  `UNIT_FIELD_FLAGS` bit `0x80000` on each projected update.
- Target lost on death: the existing health-edge handler clears the selection, stops the attack
  and cancels auto-repeat; this matches 12340's `CGUnit_C::DropTargetOnDeath`.
- Target frame tapped/lootable: `UNIT_DYNAMIC_FLAGS` now fires on a Wrath snapshot update; the
  live `UnitIsTapped`/`UnitIsTappedByPlayer` reads still come from fields 143's projection
  of 12340's field 79.
- Kill: `SMSG_PARTYKILLLOG` is already decoded and reaches the chat line and
  `COMBAT_LOG_EVENT_UNFILTERED` `PARTY_KILL`. `SMSG_LOG_XPGAIN` is decoded, keeps the victim
  and base award, and drives the XP chat line and `XpAwarded` corpse mark; XP fields 634/635
  project to 716/717 and raise `PLAYER_XP_UPDATE`.
- Corpse lootable: the projected `UNIT_DYNAMIC_FLAGS` bit `0x1` is what the click/loot gate
  and the target frame read; the new event makes the frame repaint on the edge.

## Decoded but unapplied (named, not silently dropped)

- `SMSG_ATTACKERSTATEUPDATE`: `Overkill` is decoded and dropped (the 1.12 chat line and
  floating number do not use it); the `HITINFO_RAGE_GAIN` amount is read and dropped because the
  1.12 seam has no rage word; the `HITINFO_UNK1` debug tail is left unread.
- `SMSG_ATTACKSTOP`: the trailing `faceTarget` word and the victim-is-dead word are decoded
  and dropped; 12340 uses `faceTarget` only for a visual snap on stop.
- `SMSG_ATTACKSWING_DEADTARGET` and `_CANT_ATTACK` are collapsed into one variant because
  12340 handles them in one arm; neither shows error text.
- `SMSG_LOG_XPGAIN`: the trailing recruit-a-friend byte is read and dropped; the 1.12 seam has
  no RaF field.
- `SMSG_PERIODICAURALOG`: the 3.1.2 critical byte on a damage tick is decoded and dropped; the
  1.12 seam has no periodic-crit field.
- `SMSG_CLEAR_TARGET` (`0x3BF`, used by `EffectForceDeselect`) is not decoded by the Wrath
  codec. It is not part of the melee loop and is left named here.
- The Wrath stock combat-log *history* ring is still not built: only `PARTY_KILL` reaches
  `COMBAT_LOG_EVENT_UNFILTERED`, while hit results reach the chat-frame combat lines. The
  filtered `COMBAT_LOG_EVENT` history and `CombatLogGetNumEntries` remain the documented
  `Blizzard_CombatLog` gap, not a silent loss.

## Focused cases to run (10)

`benilla-protocol-wrath`, `gameplay::tests`:

- `attacker_state_reads_the_12340_overkill_and_conditional_blocks`
- `attacker_state_reads_blocked_only_with_the_block_bit`
- `attacker_state_sums_the_conditional_absorb_and_resist_blocks`
- `wrath_attack_swing_refusals_read_as_their_variants`
- `wrath_log_xp_gain_reads_the_kill_base_and_rested_bonus`

`benilla-app`, `wrath::tests`:

- `wrath_melee_school_masks_become_vanilla_school_indices`
- `wrath_melee_hit_info_becomes_the_vanilla_bit_layout`
- `wrath_melee_crit_and_full_resist_reach_the_vanilla_classifier`
- `wrath_spell_damage_and_periodic_school_masks_become_indices`
- `wrath_kill_xp_keeps_its_victim_and_base_award`

## What was checked, and what was not

Checked: the 12340 reference pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` for
`ATTACKROUNDINFO::Unpack`, `UnitCombatLog`, `CanAttackNow`, `VerifyAttackIconPressed`,
`DropTargetOnDeath`, `OnAttackStop` and the event table; AzerothCore `Unit.cpp`
`SendAttackStateUpdate` / `SendSpellNonMeleeDamageLog` / `SendPeriodicAuraLog`,
`PlayerMisc.cpp` swing refusals, `CombatHandler.cpp` and `Player::SendLogXPGain`, read-only.
`git diff --check` passed.

Not checked: no Cargo build, no focused test execution, no `cargo fmt`, no client launch and no
server interaction. The dynamic-flags/`UNIT_FLAGS` event edges are source-reviewed only.
The owner's live Northshire run is the first receiver/gameplay evidence.

## Owner check list

1. Auto-attack a wolf from range: the red out-of-range error appears (`ERR_BADATTACKPOS`).
2. Close to melee and auto-attack: swings land with the right hit/miss/dodge/parry/block/crit
   line, floating number/word, and both health bars drop.
3. Kill the wolf: the kill line and XP gain line appear, the XP bar moves, and the corpse shows
   lootable/tapped correctly.
4. Pull a wolf and run until it evades/resets: the attack stops, combat ends, and the evade
   line/word appears without a stale target or combat state.

Status: in progress


## Verification state (2026-11-16)

Not verified. One `cargo check -p benilla --features hotpatch` run reported an error in the
`benilla-app` lib that was not captured before Cargo runs stopped, so the branch may not build.
The `benilla-app` test build did compile and the app-side filter groups passed
(`wrath_melee` 3, `wrath_spell_damage` 1, `wrath_kill_xp` 1); `benilla-protocol-wrath` was
133 passed / 1 failed (`forced_speeds_carry_the_run_only_byte_and_all_nine_kinds`, untouched by
this branch). Treat the branch as source review only until the owner's build.
