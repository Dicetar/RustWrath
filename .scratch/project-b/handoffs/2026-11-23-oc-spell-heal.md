# SMSG_SPELLHEALLOG combat-log subevent (SPELL_HEAL)

Status: ready to merge
Owner: Iceman Free
Branch: oc/spell-heal

## Summary
Added the Wrath combat-log subevent `SPELL_HEAL` from the existing `SMSG_SPELLHEALLOG` decode, copying the landed `SPELL_DAMAGE` slice: decoded overheal/absorbed, queued/delivered heal rows in the Wrath world/feed path, and posted `COMBAT_LOG_EVENT_UNFILTERED` as `SPELL_HEAL` with the 12340 0x20 heal block.

## Source packet and reference addresses
- Packet: `SMSG_SPELL_HEAL_LOG` = `0x150` (`Net/Types.h`), body packed target, packed caster, u32 spellId, u32 amount, u32 overheal, u32 absorbed, u8 crit, u8 unused (`HandleSpellHealLog` at `UnitCombatLog_C.cpp:1931`).
- `UnitCombatLog_C.cpp:1811` `UnitCombatLogHeal`: returns without an entry when amount, overheal and absorbed are all zero; otherwise calls `SendHealEvent` with periodic flag (`spell.m_attributesExC & 0x2000000`).
- `UnitCombatLog_C.cpp:2001` `SendHealEvent`: sets `m_infoFlags |= 0x20` with values amount, overheal, absorbed, crit.
- `UnitCombatLog_C.cpp:1241` `PushEvent`: emits 8 header values, spellId/spellName/spellSchoolMask (3), then the 0x20 block amount, overheal, absorbed, critical as 1.0 or nil (4), total 15 for a shown heal. Missing spell row: nil name, school 0.

## Changes
- `crates/benilla-protocol-wrath/src/gameplay.rs`: extended `ServerEvent::SpellHealLog` with `overheal: u32` and `absorbed: u32`; `read_spell_heal_log` now decodes overheal/absorbed (and consumes trailing unused byte) instead of discarding.
- `crates/benilla-app/src/wrath.rs`: added `WrathSpellHeal` (target, caster, spell_id, amount, overheal, absorbed, critical, session/world scope, revision, received_at), `WrathFeedEvent::SpellHeal`, bounded `spell_heals` queue/counters, `apply_spell_heal` (drops rows where amount, overheal and absorbed are all zero, per `UnitCombatLogHeal :1811`), clear/feed apply handling, `translate_live` mapping updated for the extended decode, and session-loop dispatch of `WrathGameplayEvent::SpellHealLog` into the feed plus translated session events.
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: drains `spell_heals` like `spell_damages` and posts `SPELL_HEAL` (header + spell trio + amount, overheal, absorbed, critical 1.0/nil). Added `spell_heal_event_has_the_fifteen_wrath_values`, `spell_heal_zero_row_is_dropped`, `spell_heal_missing_catalog_posts_nil_name_and_zero_school`.

## Tests
- Added: `spell_heal_event_has_the_fifteen_wrath_values`, `spell_heal_zero_row_is_dropped`, `spell_heal_missing_catalog_posts_nil_name_and_zero_school` in `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`.
- Not run (NO Cargo/build/client/server per task constraints; CI is the compile/test authority).

## Not proven
- Compilation and tests not run locally; GitHub CI result not observed in this session.
- No live client/server verification of `SMSG_SPELLHEALLOG` delivery, name resolution timing, or in-game `COMBAT_LOG_EVENT_UNFILTERED`/addon-visible `SPELL_HEAL` behavior.
- Only the Wrath UI combat-log feed slice was wired (mirroring landed `SPELL_DAMAGE`); base-protocol/chat heal paths were not changed and remain on their existing shapes.
