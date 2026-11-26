# Wrath 3.3.5a (12340) Death Knight Rune Affordability/Cooldowns & Shaman Totem/Multi-Cast Bar

**Date:** 2026-11-26  
**Scope:** Issue 18 stock-replacement gaps — Death Knight `SpellRuneCost.dbc` join, `CGRuneInfo::HasRunesForSpell` (`0x005EE5B0`), `CGRuneInfo::ComputeRuneSpellCooldown` (`0x005EE7E0`), `GetRuneCount` (`0x0060EB40`), Shaman `SMSG_TOTEM_CREATED` (`0x3C4`), `CMSG_TOTEM_DESTROYED` (`0x414`), `PLAYER_TOTEM_UPDATE`, `GetTotemInfo`, `GetTotemTimeLeft`, `DestroyTotem`, `GetMultiCastBarOffset`, `SetMultiCastSpell`, and `GetMultiCastTotemSpells`.

## Changed Paths

### 1. Death Knight `SpellRuneCost.dbc`, Rune Affordability, Cooldowns, and `GetRuneCount`
- `crates/benilla-formats/src/spells/layout.rs`: Added `SpellLayout::rune_cost_id` reading column `226` (`m_runeCostID`) on 234-column Wrath `Spell.dbc`.
- `crates/benilla-formats/src/spells/display.rs`: Extended `SpellDisplay` with `rune_cost: [u8; 3]` (`[Blood, Unholy, Frost]`), `rune_power_gain: u32`, and `has_rune_cost()`.
- `crates/benilla-formats/src/spells/mod.rs`: Joined `DBFilesClient\SpellRuneCost.dbc` (`ID`, `Blood`, `Unholy`, `Frost`, `RunePowerGain`) in `load_spell_catalog`.
- `crates/benilla-app/src/spell/usable.rs`:
  - Implemented `has_runes_for_spell` (`CGRuneInfo::HasRunesForSpell` `0x005EE5B0`): counts ready typed runes (`0..3`) and ready Death runes (`3`), covering typed deficits from available Death runes.
  - Implemented `compute_rune_spell_cooldown` (`CGRuneInfo::ComputeRuneSpellCooldown` `0x005EE7E0`) and `merge_rune_cooldown`: computes the earliest `(start, duration)` window when a rune-costing spell lacks required ready runes and merges it with any longer normal/category cooldown.
  - Updated `can_afford` to enforce `has_runes_for_spell` so `IsUsableSpell` and `IsUsableAction` report `noMana = true` when required runes are recharging.
  - Added focused unit test `rune_affordability_and_cooldown_cover_typed_and_death_runes`.
- `crates/benilla-app/src/spell/cast_send.rs`: Added `ui_clock` to `KnownReads` and checked `has_runes_for_spell` in `send_spell_cast`'s power/rune gate (`0x4d` `SPELL_FAILED_NO_POWER`).
- `crates/benilla-app/src/ui_action/state.rs`: Passed `WrathWorldView` runes into `can_afford` and merged rune cooldowns into `ActionState.cooldown` for spell actions.
- `crates/benilla-app/src/ui_script/wrath_queries.rs`: Merged rune cooldowns into `SpellQueries::cooldown` (`GetSpellCooldown`).
- `crates/benilla-ui/src/script/wrath_natives.rs`: Registered `GetRuneCount(slot)` (`0x0060EB40`) returning `1` when ready and `0` while cooling down (`wrath_rune_natives_read_resync_convert_and_power`).

### 2. Shaman Totems (`SMSG_TOTEM_CREATED`, `CMSG_TOTEM_DESTROYED`) and Multi-Cast Action Bar
- `crates/benilla-protocol-wrath/src/opcode.rs`: Added `SMSG_TOTEM_CREATED = 0x3C4` and `CMSG_TOTEM_DESTROYED = 0x414`.
- `crates/benilla-protocol-wrath/src/gameplay/wrath_systems.rs`: Added `MAX_TOTEM_SLOTS = 4`, `TotemCreated`, `read_totem_created`, `encode_totem_destroyed`, and focused unit test `totem_created_and_destroyed_round_trip_wire_layout`.
- `crates/benilla-protocol-wrath/src/gameplay.rs`: Routed `SMSG_TOTEM_CREATED` through `DecoderFamily::WrathSystems` and `ServerEvent::TotemCreated`.
- `crates/benilla-app/src/net.rs` & `crates/benilla-app/src/net/io.rs`: Added scoped `ClientCommand::DestroyTotem { slot, session_epoch, world_generation }`.
- `crates/benilla-app/src/wrath.rs`:
  - Added `WrathFeedEvent::TotemCreated`, `WrathTotemState`, `WrathWorldView::totems: [Option<WrathTotemState>; 4]`, `apply_totem_created`, and `clear_totem_by_guid` (invoked when an entity GUID is removed from the world).
  - Wired `ClientCommand::DestroyTotem` to encode `CMSG_TOTEM_DESTROYED` (`0x414`).
  - Added focused unit test `totem_created_and_entity_destroy_update_world_view_slots`.
- `crates/benilla-app/src/ui_script/wrath_rune_calendar_feed.rs`: Projected `world.totems` into `UiScript::set_wrath_totem`, resolved spell name/icon from `Spells`, fired `PLAYER_TOTEM_UPDATE` with 1-based slot on change, and forwarded `WrathScopedCommand::DestroyTotem`.
- `crates/benilla-ui/src/script/model.rs`, `crates/benilla-ui/src/script/mod.rs`, `crates/benilla-ui/src/script/action.rs`, `crates/benilla-ui/src/script/wrath_natives.rs`:
  - Updated `GetMultiCastBarOffset()` to return `6` (`NUM_ACTIONBAR_PAGES`) on `UiTarget::Wrath335` and `0` on `UiTarget::Vanilla112`.
  - Implemented state-backed `GetTotemInfo(slot)`, `GetTotemTimeLeft(slot)`, `DestroyTotem(slot)`, `SetMultiCastSpell(actionID, spellID)`, and `GetMultiCastTotemSpells(slot)`.
  - Added focused unit test `wrath_totem_and_multicast_natives_project_created_totems_and_destroy_requests`.

## Verification Status
- **Source & focused unit tests authored:**
  - `benilla_app::spell::usable::tests::rune_affordability_and_cooldown_cover_typed_and_death_runes`
  - `benilla_protocol_wrath::gameplay::wrath_systems::tests::totem_created_and_destroyed_round_trip_wire_layout`
  - `benilla_app::wrath::field_audit_tests::totem_created_and_entity_destroy_update_world_view_slots`
  - `benilla_ui::script::wrath_natives::tests::wrath_totem_and_multicast_natives_project_created_totems_and_destroy_requests`
  - `benilla_ui::script::wrath_natives::tests::wrath_rune_natives_read_resync_convert_and_power`
- **Cargo compilation / test execution:** Not run in this cloud workspace (no Rust toolchain installed in container).
