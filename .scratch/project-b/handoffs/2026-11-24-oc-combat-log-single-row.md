# Single-row combat-log packets and damage classifications

Status: in progress
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: oc/combat-log-single-row
Issue: none
Next: push the repaired decode/queue/feed layer, read the CI run, and fix until green. Then mark ready to merge and record the CI run id.

## Delivery this round

- Rebased the parked branch on `origin/main` (`ff0205077`) and finished the partial layer.
- Single-row packets: `SPELL_INSTAKILL` (`0x32F`), `ENCHANT_APPLIED`/`ENCHANT_REMOVED`
  (`0x1D7`), `ENVIRONMENTAL_DAMAGE` (`0x1FC`). Added the WrathWorldView queues,
  revisions, dropped/unresolved counters, defaults and scope clears, the `EnchantRemoved`
  feed variant, and the `translate_gameplay`/`wrath_net_drain` arms.
- Damage classifications: `DAMAGE_SHIELD` (`0x24F`), `DAMAGE_SPLIT` (`0x250`
  `hitInfo & 0x8`) and `RANGE_DAMAGE` (`0x250` nonzero row whose spell has
  `AttributesEx3 & 0x8000`). Added a separate damage-shield queue; split/range stay on the
  existing 0x250 spell-damage queue and are selected by the reference's own tests in the feed.
- Tests: `instakill_enchant_and_environmental_decode_in_their_reference_shapes`,
  `damage_shield_decodes_victim_caster_and_school_mask`, `instakill_has_the_eleven_values`,
  `enchant_applied_names_the_enchant_and_item_and_fade_is_removed`,
  `environmental_damage_names_the_type_and_school`,
  `zero_environmental_rows_are_dropped`,
  `damage_shield_has_the_twenty_values_in_source_victim_order`,
  `damage_split_uses_the_split_damage_row`,
  `range_damage_uses_the_ranged_spell_attribute`.
- Verification: rustfmt on the four edited files; no Cargo, client or server was run. CI: not
  run yet. Nothing has been observed in the stock combat-log frame.

## Scope

Add `SPELL_INSTAKILL`, `ENCHANT_APPLIED`, `ENCHANT_REMOVED` and `ENVIRONMENTAL_DAMAGE` to
the Wrath combat-log feed from `SMSG_SPELL_INSTAKILL_LOG` (`0x32F`),
`SMSG_ENCHANTMENT_LOG` (`0x1D7`) and `SMSG_ENVIRONMENTAL_DAMAGE_LOG` (`0x1FC`).
Do not edit `crates/benilla-ui/src/script/combat_log.rs`, `wrath_native.rs`,
`ui_script/wrath_cast.rs`, trade-skill, target or spellbook files.

## Reference

- Instakill: `Packet_SMSG_SPELLINSTAKILLLOG` OFFSET `0x006CE260` (`Player_C.cpp`), body
  u64 caster, u64 target, u32 spellId; `UnitCombatLogInstakill` OFFSET `0x00752210`, event 14,
  8 header + spell trio = 11 values. Drop a hidden/unnamed spell.
- Enchant: `Packet_SMSG_ENCHANTMENTLOG` OFFSET `0x00751050` (`UnitCombatLog_C.cpp:1530`):
  packed target, packed caster, u32 itemId, u32 enchantId. Event 31 `ENCHANT_APPLIED` when
  caster != 0, else 32 `ENCHANT_REMOVED`. Payload: 8 header + `0x1000` enchant name +
  `0x200` block (itemId, item name) = 11 values. A `SpellItemEnchantment.Flags & 0x2` row is
  dropped. Item name from `crate::item_names::ItemNames`; enchant name and the flag from
  `crate::items::Enchants` (`EnchantCatalog::name`, `tooltip_hides_name`).
- Environmental: `CGUnit_C::HandleEnvironmentDamage` OFFSET `0x00755270`
  (`UnitCombat_C.cpp:57`) -> `UnitCombatLog` OFFSET `0x00751150`
  (`UnitCombatLog_C.cpp:3151`). Body: u64 victim, u8 type, u32 damage, u32 absorbed,
  u32 resisted; drop when damage, absorbed and resisted are all zero. Event 0, 8 header with a
  nil source and the victim as destination, then the `0x8` type string and the `0x10` block:
  damage, overkill 0, school mask, resisted or nil, blocked nil, absorbed or nil, critical /
  glancing / crushing nil = 18 values. Type strings: FATIGUE, DROWNING, FALLING, LAVA, SLIME,
  FIRE. School masks: `1 << {0,0,0,2,3,2}[type]`.
  - The local AzerothCore writer emits resisted before absorbed; the 12340 client reads
    absorbed then resisted. Decode per the reference and state the difference in the note.
- `CombatLogEntry::PushEvent` OFFSET `0x0074E290`.
- Damage shield: `Packet_SMSG_SPELLDAMAGESHIELD` OFFSET `0x006D3750`
  (`Player_C.cpp:3630`), body u64 victim, u64 caster, u32 spellId, u32 damage, u32 overkill,
  u32 schoolMask. `UnitCombatLogDamageShield` OFFSET `0x00752C90`
  (`UnitCombatLog_C.cpp:1183`), event 40. Its `GetCombatLogEntry(victim, caster)` makes the
  victim the source and the caster the destination; the 0x10 block is damage, overkill,
  school mask, then nil/nil/nil, and crit/glance/crush are nil. 20 values.
- Damage split: `HandleSpellNonMeleeDamageLog` OFFSET `0x006D3840`
  (`UnitCombatLog_C.cpp:2520`) tests `hitInfo & 0x8` first and calls
  `UnitCombatLogSplitDamage` OFFSET `0x00752DE0` (event 42). Same 20-value 0x10 block as a
  spell-damage row, with source caster and destination target.
- Range damage: the same handler uses event 3 when the spell's `m_attributesExC & 0x8000`
  (Spell.dbc `AttributesEx3`, our `SpellDisplay::attributes_ex3` /
  `melee_white_damage()`) is set and damage is nonzero. Same 20-value block. A zero-damage
  row would be event 4 `RANGE_MISSED`, outside this slice.
- Instakill/Enchant/Environmental are unchanged from the reference block above. The local
  AzerothCore environmental writer still emits resisted before absorbed while 12340 reads
  absorbed then resisted; the decoder follows 12340. This is recorded here rather than
  silently normalised.

## Behaviour

- One bounded queue row per packet (instakill, enchant), or per environmental packet.
- Source/destination as named above. Missing spell record: drop, as the reference does.
- Use the existing drop/unresolved counters and clear them on scope reset.

## Checks

No Cargo, no client/server launch, no `target` directory. CI is the authority. Named tests
(do not delete existing tests):
- decode: `instakill_enchant_and_environmental_decode_in_their_reference_shapes`
- feed: `instakill_has_the_eleven_values`
- feed: `enchant_applied_names_the_enchant_and_item_and_fade_is_removed`
- feed: `environmental_damage_names_the_type_and_school`
- feed: `zero_environmental_rows_are_dropped`
