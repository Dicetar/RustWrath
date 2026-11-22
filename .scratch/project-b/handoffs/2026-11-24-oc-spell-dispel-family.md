# Dispel combat-log family: SPELL_DISPEL / SPELL_STOLEN / SPELL_DISPEL_FAILED

Status: ready to merge
Owner: Beam Free
Branch: oc/spell-dispel-family
Issue: none
Next: none
CI run 44978107411 failed: the feed did not import `WrathSpellDispelLog`/`WrathSpellDispelFailed`, and the decoder carried an unused duplicate `DispelFailed` variant that made `translate_gameplay` non-exhaustive. The overseer added the imports and removed the duplicate. Re-run pending. The same branch also fixes `combat_log_flags` for 3.3.5's `0xF010` gameobject high guid (the 1.12 helper only knows `0xF110`), with a new assertion in the existing flag test.

## Scope

Add `SPELL_DISPEL`, `SPELL_STOLEN` and `SPELL_DISPEL_FAILED` to the Wrath combat-log feed
from `SMSG_SPELL_DISPELL_LOG` (`0x27B`), `SMSG_SPELL_STEAL_LOG` (`0x333`) and
`SMSG_DISPEL_FAILED` (`0x262`). Do not edit `crates/benilla-ui/src/script/combat_log.rs`,
`wrath_native.rs`, `ui_script/wrath_cast.rs`, trade-skill, target or spellbook files.

## Reference

- `CombatLog::HandleSpellRemoveLog` OFFSET `0x006CE2B0` (`UnitCombatLog_C.cpp:2489`):
  packed victim, packed caster, u32 dispelSpellId, u8 unused, u32 count; per row u32 spellId,
  u8 harmful.
  - `SMSG_SPELL_DISPELL_LOG` -> `AddAuraReplacedEntry` OFFSET `0x00752930` with
    `stolen=false`, event 26 `SPELL_DISPEL`.
  - `SMSG_SPELL_STEAL_LOG` -> the same push with `stolen=true`, event 27 `SPELL_STOLEN`.
  - Payload: 8 header + spell trio of the dispel spell + `0x100` block (replaced spell id,
    name, school) + `0x400` aura type `"DEBUFF"` when harmful else `"BUFF"` = 15 values.
    `CombatLogEntry::PushEvent` OFFSET `0x0074E290`.
  - The reference drops the row unless both the dispel spell and the replaced spell have a
    named, non-`Attributes & 0x180` record.
- `Packet_SMSG_DISPEL_FAILED` OFFSET `0x00750EA0` (`UnitCombatLog_C.cpp:1126`):
  u64 caster, u64 victim, u32 dispelSpellId, then u32 failed spell ids to the end of the body.
  One event 39 row per failed id: 8 header + spell trio of the dispel spell + `0x100` block
  (failed spell id, name, school) = 14 values. Drop the row when either record is unnamed or
  hidden.
- AzerothCore sends all three (`SpellEffects.cpp:2600/2660/5659`).

## Behaviour

- One bounded queue row per successful aura/failed id, in wire order.
- Source is the caster; destination is the victim. Spell name/school from the existing
  `Spells` catalog. Missing record: drop the row, as the reference does.
- Use the existing drop/unresolved counters and clear them on scope reset.

## Checks

No Cargo, no client/server launch, no `target` directory. CI is the authority. Named tests
(do not delete existing tests):
- decode: `spell_dispel_log_reads_harmful_rows_and_dispel_failed_reads_the_tail`
- feed: `spell_dispel_posts_the_replaced_aura_and_debuff_type`
- feed: `spell_stolen_uses_the_stolen_event`
- feed: `spell_dispel_failed_posts_one_row_per_aura`
