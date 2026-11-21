# SPELL_PERIODIC_* combat-log feed

Status: ready to merge
Owner: OpenCode / Iceman Free
Branch: oc/spell-periodic
Issue: none
Next: integrator review; CI should compile and run the named periodic tests

## Done

- Extended `SMSG_PERIODICAURALOG` decode in `crates/benilla-protocol-wrath/src/gameplay.rs` to keep the reference's full tick fields: damage gets overkill/school/absorb/resist/crit/hitInfo (hitInfo rewritten to 2 on crit), heal gets amount/overheal/absorb/crit, energize/leech keep their wire fields.
- Added periodic combat-log row structs, feed events, WrathWorldView queues/revisions/dropped/unresolved counters, apply methods, scope clear, reducer handling, and PeriodicAura dispatch that classifies ticks into SPELL_PERIODIC_DAMAGE/MISSED/HEAL/ENERGIZE/DRAIN/LEECH rows while preserving the existing 1.12 PeriodicAuraLog seam in `translate_live`.
- Added combat-log feed draining/arg builders for the six periodic subevents in `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`, including 0x10/0x20/0x40/0x80 block shapes, power display scaling, drain/leech split on `trunc(amount * multiplier)`, and zero-row drops.

## Files changed

- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`
- `.scratch/project-b/handoffs/2026-11-24-oc-spell-periodic.md`

## Named tests

- `periodic_aura_decode_keeps_overkill_crit_overheal_and_absorbed` (protocol, `crates/benilla-protocol-wrath/src/gameplay.rs`)
- `spell_periodic_damage_has_the_twenty_wrath_values` (feed)
- `spell_periodic_zero_damage_absorb_or_resist_posts_missed` (feed)
- `spell_periodic_heal_and_energize_use_their_reference_blocks` (feed)
- `spell_periodic_drain_and_leech_split_on_the_multiplier` (feed)
- `spell_periodic_zero_amount_rows_are_dropped` (feed)

CI: run 44928577145 failed `fmt` only; the overseer applied `rustfmt`, made the periodic-miss amount an integer like `PushEvent` 0x2000/0x4000/0x8000, made a zero divided drain extra nil, and read crit from `hitInfo` bit 1 as well as the crit byte. Run 44929021935 then failed on ten borrow errors: the memo tuple arrays borrowed `memo` twice, and the match arm moved `event` while `ticks` was borrowed. The overseer replaced the memo arrays with explicit per-field blocks and destructured `PeriodicAura` by value, then re-ran `rustfmt`.

## Scope

Implement the 3.3.5 combat-log periodic family from `SMSG_PERIODICAURALOG` (`0x24E`):
`SPELL_PERIODIC_DAMAGE`, `SPELL_PERIODIC_MISSED`, `SPELL_PERIODIC_HEAL`,
`SPELL_PERIODIC_ENERGIZE`, `SPELL_PERIODIC_DRAIN` and `SPELL_PERIODIC_LEECH`.
Use the existing bounded combat-log queue and feed pattern of `SPELL_HEAL`/`SPELL_ENERGIZE`.
Do not edit `crates/benilla-ui/src/script/combat_log.rs`.

## Reference

- Packet `SMSG_PERIODIC_AURA_LOG = 0x24E` (`Net/Types.h:598`), handler
  `Player_C.cpp:3768` -> `CombatLog::HandlePeriodicAuraLog` (`UnitCombatLog_C.cpp:2627`,
  OFFSET `0x007531F0`). Body: packed target, packed caster, u32 spellId, u32 count; per tick
  u32 auraType, then:
  - 3 / 89: u32 damage, u32 overkill, u32 school mask, u32 absorbed, u32 resisted, u8 crit,
    u8 hitInfo (reference rewrites hitInfo to 2 when crit).
  - 8 / 20: u32 amount, u32 overheal, u32 absorbed, u8 crit.
  - 21 / 24: u32 powerType, u32 amount.
  - 64: u32 powerType, u32 amount, f32 multiplier.
- Pushes: `UnitCombatLogSpellDamage` OFFSET `0x00751C40` (`UnitCombatLog_C.cpp:2414`),
  `UnitCombatLogHeal` `0x00751F70` (`:1811`), `UnitCombatLogEnergize` `0x00752040`
  (`:1831`), `UnitCombatLogPowerDrain` `0x00752100` (`:2082`);
  `SendDamageEvent` `0x00750660`, `SendHealEvent` `0x00750860`,
  `SendEnergizeEvent` `0x00750A90`, `AddPowerDrainEntry` `0x00750B60`;
  `CombatLogEntry::PushEvent` `0x0074E290` (`:1241`).
- Event numbers: 33 `SPELL_PERIODIC_MISSED`, 34 `SPELL_PERIODIC_DAMAGE`,
  35 `SPELL_PERIODIC_HEAL`, 36 `SPELL_PERIODIC_ENERGIZE`, 37 `SPELL_PERIODIC_DRAIN`,
  38 `SPELL_PERIODIC_LEECH` (`s_combatLogEventNames`, `UnitCombatLog_C.cpp:78-83`).

## Behaviour to implement

- Damage with damage != 0: event 34, `SendDamageEvent` 0x10 block. Order: damage, overkill,
  school mask, resisted or nil, blocked or nil (always 0/nil from this packet), absorbed or
  nil, critical 1.0/nil, glancing nil, crushing nil. 20 values.
- Damage == 0: absorbed != 0 -> event 33 with miss string `ABSORB` and the absorbed amount;
  else resisted != 0 -> `RESIST` and the resisted amount; else no entry. 13 values.
- Heal: event 35, 0x20 block amount, overheal, absorbed, critical 1.0/nil. Drop the tick when
  all three amounts are zero. 15 values.
- Energize: event 36, 0x40 block `amount / power_display_scale(power_type)` (integer) and
  powerType. Drop amount 0. 13 values.
- Drain/leech: drop amount 0 and power type 5 (POWER_HAPPINESS; reference logs only the pet
  happiness chat line). `extra = trunc(amount * multiplier)`; event 38 when extra != 0, else
  37. 0x80 block: divided amount, powerType, divided extra or nil. 14 values.
- Source is the caster, destination is the target; spell name/school come from the existing
  `Spells` catalog (nil name, school 0 when absent); missing spell row is not a reason to drop
  the row unless the reference drops it.
- Every tick goes through the bounded queue in `WrathWorldView` and the existing
  `COMBAT_LOG_EVENT_UNFILTERED` queue; add drop/unresolved counters and clear them on scope
  reset, matching the heal/energize slices.
- Keep `translate_live`'s existing `PeriodicAura` path for the 1.12 seam; the new feed rows are
  additional.

## Checks

No Cargo, no client/server launch, no `target` directory. CI is the compile/test authority.
Named tests (at most 20; do not delete any existing test):
- protocol: the periodic decode keeps overkill, crit, overheal and absorbed.
- feed: `spell_periodic_damage_has_the_twenty_wrath_values`
- feed: `spell_periodic_zero_damage_absorb_or_resist_posts_missed`
- feed: `spell_periodic_heal_and_energize_use_their_reference_blocks`
- feed: `spell_periodic_drain_and_leech_split_on_the_multiplier`
- feed: `spell_periodic_zero_amount_rows_are_dropped`
