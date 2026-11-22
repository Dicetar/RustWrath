# Remaining single-row combat-log packets: INSTAKILL / ENCHANT / ENVIRONMENTAL_DAMAGE

Status: blocked
Owner: OpenCode / Nemotron 3.5 Lightning Free (partial)
Branch: oc/combat-log-single-row
Issue: none
Next: the branch does not compile (CI 44990691706). Add the missing WrathWorldView instakill/enchant/environmental queues and counters with defaults and clears, add the EnchantRemoved feed variant, and handle the four new ServerEvent/WrathFeedEvent variants in translate_gameplay and wrath_net_drain. Then rerun CI.

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
