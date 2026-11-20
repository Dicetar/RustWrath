# SPELL_ENERGIZE combat-log subevent

- Status: ready to merge
- Owner: opencode/iceman-free
- Branch: oc/spell-energize

## Summary

Added the combat-log subevent `SPELL_ENERGIZE` from the existing `SMSG_SPELLENERGIZELOG` decode (`benilla_protocol_wrath::gameplay::ServerEvent::SpellEnergizeLog`, decoded in `crates/benilla-protocol-wrath/src/gameplay.rs` around 2790 / `read_spell_energize_log` around 3305), mirroring the landed `SPELL_HEAL` / `SPELL_DAMAGE` slices.

## Source packet and 12340 reference

- Packet: `SMSG_SPELL_ENERGIZE_LOG = 0x151` (`Net/Types.h` in 12340 reference).
- Body: packed target, packed caster, u32 spellId, u32 powerType, u32 amount (`UnitCombatLog_C.cpp:1974` `HandleSpellLogEnergizeLog`, `read_spell_energize_log` at `crates/benilla-protocol-wrath/src/gameplay.rs:3305`).
- `UnitCombatLogEnergize` (`UnitCombatLog_C.cpp:1831`): returns when amount is zero.
- `SendEnergizeEvent` (`UnitCombatLog_C.cpp:2045`): divides amount by `Spell_C_GetPowerDisplayMod(powerType)` (this client uses `benilla_protocol::messages::power_display_scale(power_type)` as in wrath queries) with integer division before queueing the 0x40 block.
- `PushEvent` (`UnitCombatLog_C.cpp:1241`, 0x40 at `:1365`): appends amount, powerType (2 values). Shown energize total 13: 8 header + spellId/spellName/spellSchoolMask + 2.
- Missing spell row: nil name, school 0 (PushEvent fallback).
- Also noted: `Player_C.cpp:3670`.

## Changes

- `crates/benilla-app/src/wrath.rs`: `WrathSpellEnergize` (target, caster, spell_id, power_type, amount, session_epoch/world_generation scope, revision, received_at), bounded queue/counters (`spell_energizes`, `spell_energize_revision/dropped/unresolved`), `apply_spell_energize` (skips `amount == 0` per `UnitCombatLogEnergize`), clear on scope reset, `WrathFeedEvent::SpellEnergize`, Net-stage dispatch/apply, and session-loop arm pushing the combat-log feed event and translating the existing live/session event.
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: drain `spell_energizes`, post `COMBAT_LOG_EVENT_UNFILTERED` with header + spell trio + divided amount + powerType; memo drop/unresolved reporting; `spell_energize_args`.

## Tests added (not run)

- `spell_energize_event_has_the_thirteen_wrath_values` (rage power type 1, display scale 10 => amount/10; and mana power type 0, scale 1)
- `spell_energize_zero_row_is_dropped`
- `spell_energize_missing_catalog_posts_nil_name_and_zero_school`

## Not proven

- Not compiled, not tested, no client/server run (forbidden in this task).
- No live in-game `SMSG_SPELL_ENERGIZE_LOG` receipt/UI observation.
- CI not observed from this session.
