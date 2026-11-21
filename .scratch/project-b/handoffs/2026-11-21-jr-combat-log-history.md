# Combat-log history

Status: landed; documentation refreshed 2026-11-24
Owner: ClaudeJr / Claude Opus 5.5
Branch: jr/combat-log-history
Next: Owner in-game check of combat-log history scrolling and filters; source producers now cover SPELL_HEAL and SPELL_ENERGIZE as well.

## What changed

- `crates/benilla-ui/src/script/combat_log.rs` keeps a raw history of every
  `COMBAT_LOG_EVENT_UNFILTERED` tuple, pruned by the `combatLogRetentionTime` CVar
  (stock default 300 s), as `UnitCombatLog_C.cpp:813-819` does.
- `CombatLogGetNumEntries`, `CombatLogSetCurrentEntry`, `CombatLogGetCurrentEntry`,
  `CombatLogAdvanceEntry` and `CombatLogClearEntries` walk that history with the filter
  list, following `UnitCombatLog_C_Scripts.cpp` (`0x0074FA70`, `0x0074FAE0`, `0x0074F2B0`,
  `0x0074FC20`, `0x00751120`). Index `<= 0` counts back from the newest match; Advance
  clears the cursor when no further match exists.
- Filters match the event list, then source/dest selectors. A GUID selector compares
  `srcGUID`/`dstGUID` (args 3 and 6), and a mask selector compares `srcFlags`/`dstFlags`
  (args 5 and 8) using the reference's per-category completeness rule.
- `crates/benilla-ui/src/script/tick.rs`: `fire_event_into` records each unfiltered event
  and forwards it as `COMBAT_LOG_EVENT` when it matches the filters. This preserves the
  separate unfiltered history/event stream and filter-matched event stream.
- The later landed Wrath feed commits `519095eb3`, `9b8ba646c`, `cd95c37d9`, and `bdf7391cd`
  add source-backed `SPELL_DAMAGE`, `SPELL_MISSED`, `SPELL_HEAL`, and `SPELL_ENERGIZE` producers;
  `f902a884c` and `7e4628ca1` add the swing damage/miss rows.

- This is retention-managed history, not a fixed-size ring: the 12340 reference uses linked
  `s_entries` and prunes entries by age in `UnitCombatLog_C.cpp:813-819`; the default
  `combatLogRetentionTime` is 300 seconds (`GameUI_CVar.cpp:633`). The native cursor indexes
  through list links and filtering (`UnitCombatLog_C_Scripts.cpp:139-257`), rather than using
  a ring-buffer cursor.
- The current Rust implementation uses a `VecDeque` to model the reference's
  retention-managed history; this is an implementation detail, not a claim that the 12340 client
  used a fixed-size ring. `fire_event_into` is the producer seam, so all currently emitted feed
  rows enter the same history.

## Current refresh evidence

- Current source inspection confirms `wrath_combat_log_feed.rs::feed` posts
  `PARTY_KILL`, `SWING_DAMAGE`, `SWING_MISSED`, `SPELL_DAMAGE`, `SPELL_MISSED`, `SPELL_HEAL`,
  and `SPELL_ENERGIZE`; `fire_event_into` records every unfiltered tuple before filtered delivery.
- Reference anchors remain `UnitCombatLog_C.cpp:813-819`,
  `UnitCombatLog_C_Scripts.cpp:139-257`, and `UnitCombatLog_C.cpp:1241-1276`.
- No CI run was started for this documentation refresh; no local Cargo, client, server, login or
  in-game execution was authorized.

## CI history

- 44836134253: tests did not compile (two test scripts not `mut`). Fixed in 541852e5a.
- 44843655549 / 44846614022: two new tests failed. Causes: selectors read the flags args
  for GUID filters, the dest mask `2048` was incomplete (fixed in f704f0273 by switching to
  GUID selectors), the cursor test expected Advance to keep the cursor, and the heal amount
  index was wrong (Lua `Filtered[12]`). All fixed in 8a78e95ef.

## Verification

- Focused tests: 4 combat-log history cases in `combat_log::tests` (within the 20-case limit).
- Local Cargo, client launch and server launch: not run per ticket rules.
- In-game check is not proven: open the combat log, fight, and confirm the history scrolls
  and filters.
- 44854212279 on 79ac2ba9e (adds 4b8313569 clippy fix and a current-main merge): windows and gates both success. Landed with `git push origin HEAD:main`.
