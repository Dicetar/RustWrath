Status: ready to merge
Owner: Iceman Free
Branch: oc/spell-damage-feed
Next: review/merge if CI is green; verify live SPELL_DAMAGE feed in a 12340 Wrath session

## What changed
- `crates/benilla-protocol-wrath/src/gameplay.rs`: added `overkill: u32` to `ServerEvent::SpellDamage`, decoded the existing overkill word (no longer discarded), and updated the `SMSG_SPELLNONMELEEDAMAGELOG` constructor.
- `crates/benilla-app/src/wrath.rs`: added `SPELL_LOG_CAPACITY`, `WrathSpellDamage`, and `WrathFeedEvent::SpellDamage`; added `spell_damages`/revision/drop/unresolved fields, `apply_spell_damage` (mirrors `apply_swing`, monotonic revision, bounded queue, oldest-drop), and cleared them in `clear_rune_calendar`. Session loop now pushes the spell-damage feed event with session scope and still runs `translate_live` as before. Feed-event consumer adopts the row with the same `scope_is_older` gating as `AttackerState`. `WrathGameplayEvent::SpellDamage` pattern matches/constructors updated for the new `overkill` field (translation to the vanilla `SpellDamageLog` seam unchanged).
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: drains `world.spell_damages` with scope checks, endpoint-name resolution/retry, and unknown-endpoint fallback after `COMBAT_LOG_NAME_RETRY_SECONDS`; posts `COMBAT_LOG_EVENT_UNFILTERED` / `SPELL_DAMAGE` with 12340's 20-value shape (8 header + spellId/name/schoolMask + 9-value 0x10 block). Spell trio uses `SpellCatalog::get`/`school_mask` (nil name and 0 school when missing). Damage school is the raw packet mask byte; absent resisted/blocked/absorbed are nil; crit is 1.0/nil from `hitInfo & 0x2` (`HandleSpellNonMeleeDamageLog` :2520) and glancing/crushing are nil in this non-melee path.

## 12340 reference facts (quoted in comments/code)
- `PushEvent` (`UnitCombatLog_C.cpp:1241-1361`) sends 8 header values, then spellId/spellName/spellSchoolMask when a spell id exists (3), then the 0x10 block: damage, overkill, school, resisted, blocked, absorbed, then critical, glancing, crushing as 1.0 or nil (9), total 20 for `SPELL_DAMAGE`.
- Absent resisted/blocked/absorbed are nil, not 0. Crit/glance/crush are 1.0 or nil.
- If the spell record is missing, `PushEvent` pushes nil name and 0 school (`:1241-1361`).
- `SendDamageEvent` (`:2373`) builds the 0x10 block; `HandleSpellNonMeleeDamageLog` (`:2520`) reads crit from `hitInfo & 0x2` and `UnitCombatLogSpellDamage` passes glancing/crushing as false to `SendDamageEvent`.
- 12340's packet school byte is a school mask; kept raw here (unlike the 1.12 chat translation, which converts to an index).

## Tests added
- `wrath_combat_log_feed`: `spell_damage_event_has_the_twenty_wrath_values`, `spell_damage_missing_catalog_posts_nil_name_and_zero_school`, `feed_delivers_a_spell_damage_row` (prove overkill passthrough, raw packet school mask, nonzero resist/block/absorb as numbers, absent as nil, crit/glance/crush as 1.0/nil, and retirement after delivery; missing-catalog nil name/0 school mask).

## Not proven
- No local Cargo build/test run and no client/server launch (per task constraints). Compile/test authority is CI on the pushed branch.
- Live in-world `SMSG_SPELLNONMELEEDAMAGELOG` delivery/visibility in the Wrath combat log UI is not verified here.
- Spell-name/school-mask trio with a real populated `Spells` catalog row is not exercised by a new test (feed takes `Option<Res<Spells>>`; tests cover the missing-catalog/nil path and queue/feed mechanics). Behavior follows `SpellCatalog::get`/`school_mask` per spec.
- Glancing/crushing are emitted as nil for this non-melee path (consistent with `UnitCombatLogSpellDamage` passing false to `SendDamageEvent`); no packet-driven non-nil glance/crush case is proven.
