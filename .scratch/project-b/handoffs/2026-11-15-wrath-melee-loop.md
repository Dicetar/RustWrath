# Wrath melee loop: target, auto-attack, kill

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/wrath-melee-loop
Issue: none
Next: owner launch checks only; source review is complete and no Cargo, test or client run was made

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
| swing no-hit-sound | `0x200000` | `0x80000` |
| rage gain (amount only in 12340) | `0x800000` | no 1.12 word |
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

- `wrath::vanilla_hit_info` translates 12340's melee `HitInfo` into the 1.12 bit layout,
  including swing no-hit-sound (`0x200000` -> `0x80000`), and drops 12340's
  `0x4000`/`0x8000` words so they cannot masquerade as vanilla glancing/crushing, plus its
  `0x800000` rage-gain bit, which has no 1.12 word.
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
- `SMSG_AI_REACTION` and `SMSG_CANCEL_AUTO_REPEAT`: decoded; the aggro reaction reaches the
  hostile reaction cue, while the auto-repeat packet's target guid is dropped because the 1.12
  event carries none (the shared cancel still fires).
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
- Creature stance between swings: reviewed in `creature_anim`. `SMSG_ATTACKSTART` inserts
  `Engaged`; only `SMSG_ATTACKSTOP` and the unit's death remove it (`creature_anim/net.rs`
  `attack_start`/`attack_stop`), and a finished swing clip returns to `Mode::Gait`, whose
  `ready` pick is `ReadyUnarmed/1H/2H/2HL` while engaged and stationary
  (`creature_anim/driver/mode.rs`, `select.rs::claimed_candidates`). A unit created mid-fight
  is seeded from the movement block's `HAS_TARGET` packed guid
  (`benilla-protocol-wrath/src/updates.rs`, `wrath.rs` spawn), the case `82879cbc` fixed on
  main. No client change is needed for the deployed `UnitAI::DoMeleeAttackIfReady` patch; the
  owner's bandit check is the receiver evidence.

## Decoded but unapplied (named, not silently dropped)

- `SMSG_ATTACKERSTATEUPDATE`: `Overkill` is decoded and dropped (the 1.12 chat line and
  floating number do not use it); the `HITINFO_RAGE_GAIN` amount and bit are read and dropped
  because the 1.12 seam has no rage word; the `HITINFO_UNK1` debug tail is left unread (it is
  the packet's last field, so no later field shifts).
- `SMSG_ATTACKSTOP`: AC's trailing `u32` (the victim-is-dead value, which 12340 reads as
  `faceTarget`) is left unread and dropped; it only drives a visual facing snap on stop.
- `SMSG_CANCEL_AUTO_REPEAT`: the packed target guid is decoded and dropped; the 1.12 event
  carries no target, and the cancel itself is applied.
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

## Overseer source review (2026-11-17)

Read the branch's changed files against AC and the 12340 pin. `origin/main` was merged before the
review (`2ec920e4`) and again after it (`ed14dd6c`, today's `b13c163d`), without conflicts; the
branch is source-only.

Paths used: worktree `D:\Projects\wow\worktrees\a2-melee-loop`; AC
`D:\Projects\wow\server\Azerothcore_WoTLK_Rebuild\source\azerothcore`; 12340 reference
`D:\Projects\wow\reference\wowdecomp-reference` (pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`).

- `ATTACKROUNDINFO::Unpack` (`UnitCombat_C.cpp:311`) reads absorb on `0x60`, resist on
  `0x180`, block on `0x2000`, rage on `0x800000` and the debug tail on `0x1`; AC
  `Unit::SendAttackStateUpdate` writes the same order (`Overkill`, per-block masks, then the
  conditional tails). Decoder and translation match; the rage bit is now dropped with its amount.
- 12340 `PlayVictimRound` uses `0x8000` for the blood-spurt flag, `0x20000` for crushing and
  `0x200` for crit (`UnitCombat_C.cpp:138-214`); the 1.12 bit meanings are from vmangos's
  `HitInfo` (`>1.9.4`: crit `0x80`, glancing `0x4000`, crushing `0x8000`, no-action
  `0x10000`, swing-no-hit-sound `0x80000`). The translation now covers the last of those too.
  Existing donor note: `creature_anim/blood.rs` tests `0x2000` for the large spurt row; that is
  a pre-existing donor reading, not caused by this branch (12340's `0x2000` is BLOCK).
- `SMSG_ATTACKSTART` is raw guids and `SMSG_ATTACKSTOP` packed guids in AC
  (`Unit.cpp:3242-3251`, `CombatHandler.cpp:86-96`); both decoders match, including the
  optional victim. `SMSG_ATTACKSWING_*` opcodes and the 12340 handler's four arms match
  (`0x145`/`0x146` latch, `0x148`/`0x149` silent stop); AC never sends `0x147`.
- Kill/XP: AC sends `SMSG_PARTYKILLLOG` to the killing player even solo
  (`Unit.cpp:13991-14005`) and `SMSG_LOG_XPGAIN` with victim, total, kill byte, base and
  group rate (`Player.cpp:2332-2348`). The decoder and seaming are correct; the RaF byte is
  named below as unapplied.
- Leaving combat: `SMSG_ATTACKSTOP` removes `Engaged` and raises `PLAYER_LEAVE_COMBAT`;
  the player's `UNIT_FLAGS` in-combat bit raises `PLAYER_REGEN_ENABLED` on the projected
  update. The snapshot adapter's missing `UNIT_FLAGS`/`UNIT_DYNAMIC_FLAGS` events are now
  emitted; the dead bit raises the health event, matching the normal watcher's health leg.
- Stance between swings: checked in `creature_anim` (see the message status above); `Engaged`
  survives between swings and the finished swing clip returns to the Ready idle. No client
  change needed for the AC `UnitAI::DoMeleeAttackIfReady` patch; the owner's bandit check is
  the receiver evidence.

Not fixed, named: the stock combat-log history ring (only `PARTY_KILL` reaches
`COMBAT_LOG_EVENT_UNFILTERED`), `SMSG_CLEAR_TARGET`, and the 12340-only words in the decoded
list above. None blocks the target/auto-attack/kill/XP/combat-end loop.

## What was checked, and what was not

Checked: the 12340 reference pin for `ATTACKROUNDINFO::Unpack`, `PlayVictimRound`,
`UnitCombatLog`, `CanAttackNow`, `VerifyAttackIconPressed`, `DropTargetOnDeath`,
`OnAttackStop` and the event table; AzerothCore `Unit.cpp` `SendAttackStateUpdate` /
`SendSpellNonMeleeDamageLog` / `SendPeriodicAuraLog` and `AttackStop`,
`CombatHandler.cpp`, `PlayerMisc.cpp` swing refusals and `Player::SendLogXPGain`, read-only.
`git diff --check` passed on the merged tree.

Not checked: no Cargo build, no test execution, no `cargo fmt`, no client launch and no server
interaction in this session. The dynamic-flags/`UNIT_FLAGS` event edges are source-reviewed
only. The owner's live Northshire run is the first receiver/gameplay evidence.

## Verification state (2026-11-16, before the merge)

Not verified at that commit. One `cargo check -p benilla --features hotpatch` run reported an
error in the `benilla-app` lib that was not captured before Cargo runs stopped, so the branch
may not build. The `benilla-app` test build did compile and the app-side filter groups passed
(`wrath_melee` 3, `wrath_spell_damage` 1, `wrath_kill_xp` 1); `benilla-protocol-wrath` was
133 passed / 1 failed (`forced_speeds_carry_the_run_only_byte_and_all_nine_kinds`, untouched by
this branch).

The 2026-11-17 review merged `origin/main` and changed the hit-info mapping; no Cargo build,
test or client run was made afterwards. Compile and tests are not run.

Status: ready to merge

## Owner check list

1. Target a Northshire wolf and auto-attack it to death.
2. Read the misses/dodges in the combat log.
3. Check the XP gain line and the XP bar.
4. Check combat ends (leaves combat, no stale target).
5. Check the bandits keep their combat stance between swings.
