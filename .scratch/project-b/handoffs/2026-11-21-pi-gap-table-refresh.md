# Gap table refresh

Status: ready to land; synchronized with `origin/main` at `98bacdd50`
Owner: Pi
Branch: pi/gap-table-refresh
Issue: 25, 26, 27 and issue-18 referenced rows
Next: Push `HEAD:main`, record the resulting landing commit, and delete the remote topic branch.

What changed: Refreshed issue 25's combat-log coverage and history rows using landed commits
`f902a884c`, `7e4628ca1`, `519095eb3`, `9b8ba646c`, `cd95c37d9`, `bdf7391cd`, `681426a2b`, and
`8a78e95ef`. Updated the referenced issue-18 combat-log rows, preserved issue-26's uncovered
quest-type inventory, and extended issue-27's live-only edge list using `5c4786a91`, `4904db731`,
and `ede7c8d2f`. Corrected the combat-log history handoff to describe retention-managed records
and the `fire_event_into`/`record_unfiltered` producer seam.

Reference addresses: `UnitCombatLog_C.cpp:813-819` (age pruning), `UnitCombatLog_C.cpp:1241-1276`
(tuple construction), `UnitCombatLog_C_Scripts.cpp:139-257` (history/filter APIs),
`Spell_C.cpp:1000-1075` (spell log reference context), and current
`wrath_combat_log_feed.rs::feed`, `tick.rs::fire_event_into`, and `combat_log.rs::record_unfiltered`.

CI run id and result: No CI run; these changes are Markdown-only and the current workflow does not
trigger CI for documentation-only commits. Permitted static check `git diff --check` passed.

What is not proven in game: No local Cargo, client, server, login or in-game run was authorized.
The source rows and handoff corrections establish no rendered stock-frame, server, persistence, or
live gameplay result.
