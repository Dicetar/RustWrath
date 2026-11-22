# SMSG_SPELLLOGEXECUTE combat-log subevents

Status: ready to merge
Owner: OpenCode / Iceman Free
Branch: oc/spell-execute-log
Issue: none
Next: merge after CI; owner verifies in live 3.3.5 session if desired.

## Scope

Decode `SMSG_SPELL_EXECUTE_LOG` (`0x24C`) once and add the nine subevents it produces to the
Wrath combat-log feed. Do not edit `crates/benilla-ui/src/script/combat_log.rs`,
`wrath_native.rs`, `ui_script/wrath_cast.rs`, trade-skill, target or spellbook files.

## Reference

- Packet `SMSG_SPELL_EXECUTE_LOG = 0x24C` (`Net/Types.h`), AzerothCore
  `Spell::SendLogExecute` (`Spell.cpp:5024`).
- Handler `CombatLog::HandleSpellLogExecute` OFFSET `0x006D3220`
  (`UnitCombatLog_C.cpp:1684`). Body: packed caster, u32 spellId, u32 effectCount; per effect
  u32 effect, u32 targetCount, then rows. Every target guid is packed (3.3.5).
- Effect 68: packed target, u32 interruptedSpellId -> `UnitCombatLogInterrupt` OFFSET
  `0x007523D0`, event 17, `0x100` block (interrupted id, name, school). 14 values.
- Effect 19: packed target, u32 amount -> `AddSpellDamageEntry` OFFSET `0x00752520`, event 18
  `SPELL_EXTRA_ATTACKS`, `0x1` block amount. 12 values.
- Effects 18 / 113: packed target -> `AddSpellResurrectEntry` OFFSET `0x00752340`, event 46.
  11 values.
- Effects 28 / 50 / 56 / 76 / 81 / 83 / 104 / 105 / 106 / 107: packed target ->
  `AddSpellCastEntry` OFFSET `0x007522A0`; event 16 `SPELL_CREATE` when the target is a
  gameobject (`0xF0100000` type bits), else event 15 `SPELL_SUMMON`. 11 values.
- Effects 8 / 62: packed target, u32 amount, u32 powerType, f32 multiplier ->
  `UnitCombatLogPowerDrain` OFFSET `0x00752100`. Drop amount 0 and power type 5
  (POWER_HAPPINESS). `extra = trunc(amount * multiplier)`; event 13 `SPELL_LEECH` when
  extra != 0, else 12 `SPELL_DRAIN`; `AddPowerDrainEntry` OFFSET `0x00750B60`, `0x80`
  block divided amount, powerType, divided extra or nil. 14 values.
- Effect 111: packed target, i32 itemId, i32 slot.
  - Both itemId and slot `0xFFFFFFFF`: `AddSpellEnergizeEntry` OFFSET `0x00752680`, event
    20 `SPELL_DURABILITY_DAMAGE_ALL`, no extra block, 11 values.
  - Otherwise `AddSpellExecuteEntry` OFFSET `0x007525E0`, event 19
    `SPELL_DURABILITY_DAMAGE`, `0x200` block itemId then item name; 13 values, nil item name
    when the item store has no record.
- Effects 24 / 59 / 157 (create item), 33 (open lock), 101 / 102 (pet feed/dismiss) produce no
  combat-log row; do not emit one.
- `CombatLogEntry::PushEvent` OFFSET `0x0074E290`. The push drops a spell with
  `Attributes & 0x180`, an empty name, or a learn-spell-only row.

## Behaviour

- One bounded queue row per logged effect row, in wire order across effects.
- Source is the caster; destination is the effect target. Spell name/school come from the
  existing `Spells` catalog. Missing spell row: nil name, school 0.
- Power display scaling uses `benilla_protocol::messages::power_display_scale`; item names use
  `crate::item_names::ItemNames::name(entry, ask)`.
- Use the existing drop/unresolved counters and clear them on scope reset.

## Verification

CI run 44959418911 failed on one compile error: the decoder used `benilla_protocol::guid::is_gameobject`, which is a 1.12 value and not a dependency of this crate. The overseer replaced it with 12340's `(guid >> 32) & 0xF0F00000 == 0xF0100000` test. Run 44963481359 then failed with test-build exit 143 (runner termination) and two unused-mut warnings in the merged cast tests; the overseer removed the two mut bindings. Re-run pending. (no Cargo/client run in this session).

## Checks

No Cargo, no client/server launch, no `target` directory. CI is the authority. Named tests
(do not delete existing tests):
- decode: `spell_execute_log_reads_target_and_amount_effects_in_wire_order`
- feed: `spell_interrupt_has_the_interrupted_spell_trio`
- feed: `spell_extra_attacks_carries_the_amount`
- feed: `gameobject_summon_posts_spell_create_and_unit_summon_posts_spell_summon`
- feed: `spell_resurrect_has_the_spell_trio`
- feed: `spell_execute_drain_and_leech_split_on_the_multiplier`
- feed: `spell_durability_damage_names_the_item_and_all_has_no_item`
- feed: `spell_power_drain_drops_zero_amount_and_happiness`
