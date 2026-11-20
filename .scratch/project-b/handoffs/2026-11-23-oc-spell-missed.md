# SPELL_MISSED from SMSG_SPELLLOGMISS

Status: ready to merge
Owner: Iceman Free (protocol/wrath first pass) / DeepSeek V4.1 Flash overseer (feed repair)
Branch: oc/spell-missed-2
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-protocol-wrath/src/opcode.rs`: `SMSG_SPELL_MISS_LOG = 0x24B` (`Net/Types.h:595`).
- `crates/benilla-protocol-wrath/src/gameplay.rs`: `ServerEvent::SpellMiss { caster, spell_id,
  misses }`, `SpellMissTarget { target, miss_type }`, and `read_spell_miss`. Body and reference:
  `UnitCombatLog_C.cpp:1888` `HandleSpellLogMiss` (u32 spell, u64 caster, u8 debug, u32 count;
  u64 target, u8 missType per target, plus two discarded f32 debug words when `debug != 0`).
- `crates/benilla-app/src/wrath.rs`: bounded `WrathSpellMiss` queue with scope/revision/drop/
  unresolved counters, `WrathFeedEvent::SpellMiss`, `apply_spell_miss`, clear and session-loop
  adoption, mirroring `SPELL_DAMAGE`.
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: drains the queue and posts
  `SPELL_MISSED` with 8 header values, spellId/spellName/spellSchoolMask, the miss string from
  `g_spellMissStrings` (`:150`), and the integer-0 amount slot for missType 10/2/5
  (`SetMissInfo` `:311`, `PushEvent` `:1241`). Missing spell row: nil name, school 0.
  The decompiled `HandleSpellLogMiss` passes `a5=true` and is labelled `DAMAGE_SHIELD_MISSED`
  (event 41); this slice implements the user-facing `SPELL_MISSED` (event 8) and says so in the
  helper comment.

## Tests

- `spell_missed_posts_the_twelve_wrath_values`
- `spell_missed_absorb_appends_the_zero_amount`
- `spell_missed_missing_catalog_posts_nil_name_and_zero_school`

The first free worker truncated the existing test tail and nested its new tests inside
`combat_log_flags`; that branch was discarded rather than force-pushed. This branch takes its
protocol and view work and rebuilds the feed slice against `origin/main`.

## Not proven

No Cargo, client or server run. CI is the compile/test evidence; a live 12340 miss row in the
stock combat-log frame is unproven.
