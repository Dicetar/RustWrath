# Quest tracker duplicate and the standard combat walk

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer), two AutoSwarm `opencode` children
Branch: fix/overseer-gameplay-2026-11-15
Issue: #2995
Next: land on source review behind `fix/overseer-obvious-2026-11-15`; the tracker itself still needs one owner launch with two tracked quests

Based on `fix/overseer-obvious-2026-11-15` (three commits, already pushed). This branch stacks on
it, so land that one first.

## What exists now

| File | Change | Why |
|---|---|---|
| `crates/benilla-ui/src/script/quest_log.rs` | `watch_target` refuses quest id 0, so a header row or an index past the log can never become a watch; the `AddQuestWatch` comment now names both stock 3.3.5 callers and the 12340 dedup | `Script_AddQuestWatch` maps both cases to 0 and its `if (questId)` (`QuestLog_Scripts.cpp:996`-`:1014`) refuses. A 0 in the list made `GetQuestIndexForWatch` hand the tracker a header row, which it drew as a quest block |
| `crates/benilla-ui/src/script/quest_log_detail_tests.rs` | `the_wrath_watch_list_holds_a_quest_once_however_often_it_is_armed`, `a_header_row_is_never_watched_on_either_target` | The dedup had no regression test; the 0 refusal had none at all |
| `crates/benilla-app/src/target/died.rs` | The own-death case's `Debug` comparison gains the `trace: None` field `CancelCast` now carries | Test-only; the case compared a rendered command string |
| `crates/benilla-app/src/target/ring.rs` | The ring scan's control unit is marked `InheritedVisibility::VISIBLE` in the headless fixture | The harness runs no visibility propagation, so the scene gate refused the control and the test proved nothing |

## Issue #2995

The report is a duplicate **watch block**, not a duplicated line: stock 3.3.5 `WatchFrame.lua`
(`WatchFrame_DisplayTrackedQuests`, `:772`, `:799`-`:846`) draws one block per
`GetNumQuestWatches()`, a title then its objectives. The reported defect is already fixed on `main`
by `2084c232`: 3.3.5's log arms `AddQuestWatch` from both `QUEST_ACCEPTED`
(`QuestLogFrame.lua:260`-`:264`) and `QUEST_WATCH_UPDATE` (`:265`-`:271`), and before that commit
the binding appended the same id twice instead of refreshing it as
`CGQuestLog::AddQuestWatch` does (`QuestLog.cpp:545`-`:553`). Issue filed 2026-11-06,
`2084c232` dated 2026-11-07, ancestor of `HEAD`.

Ruled out with evidence: duplicate quest-log rows (the tracker iterates watches;
`GetQuestIndexForWatch` resolves the first row per id), doubled objectives (fixed four-slot array,
`HashMap` replace, and the screenshot repeats the title), tracker append-without-clear
(`ResetQuestLines` at `:797`, pooled lines).

## Checks actually run

Pinned toolchain 1.98.1, `CARGO_INCREMENTAL=0`, sccache wrapper active.

| Check | Result |
|---|---|
| `cargo test -p benilla-ui --lib watch` | 15 passed, 0 failed |
| the same suite with the dedup guard reverted to the pre-`2084c232` body | first new test fails `left: 3 / right: 1` — red/green proven |
| the same suite with the id-0 refusal removed | second new test fails `left: 1 / right: 0` — red/green proven |
| `cargo test -p benilla-app --lib target::died` | 20 passed, 0 failed |
| `cargo test -p benilla-app --lib target::ring` | 10 passed, 1 failed: `enemy_and_ally_players_read_by_attackability_on_the_real_dbc`, a data-gated case that reads the real `Faction.dbc`; the repaired scan case passes |
| `cargo clippy --no-deps -p benilla-ui --all-targets` (quests agent) | exit 0, no diagnostics in the changed files |

The combat walk itself found **no player-facing product defect** in target, attack, kill or death:
the tested paths are green (`creature_anim` 265, `combat_log` 71, `death` 16, swing/selection 64
cases were run by the agent before the two repairs). What it did find is flagged below.

## Not done, not proven

- **No client run.** The tracker block count is pinned at the watch list, not on screen; the issue
  needs one owner launch with two tracked quests in Westfall to close.
- `cargo test -p benilla-app --lib ui_quest_log` is 20 passed / 7 failed on this tree, all in
  `ui_quest_log::tests`. Failures and their actual-vs-expected, for the next owner:
  | Test | Left (actual) | Right (expected) |
  |---|---|---|
  | `the_log_signals_at_the_world_entry_and_once_per_block_that_moves_it` | `["QUEST_LOG_UPDATE:nil"]` | `[]` |
  | `the_last_awaited_record_rebuilds_the_list_once` | `["QUEST_LOG_UPDATE:nil"]` | `[]` |
  | `an_item_added_before_the_quest_record_lands_toasts_on_its_arrival` | one extra `QUEST_LOG_UPDATE` | one |
  | `a_completed_quest_toasts_its_end_text` | `QUEST_WATCH_UPDATE:3` | `QUEST_WATCH_UPDATE:2` |
  | `a_walking_addon_on_twenty_cached_quests_walks_once_at_each_world_entry` | `Beast 10 slain: 0/5` | `Beast 1 slain: 0/5` |
  | `creature_names_in_flight_signal_once_each_as_they_land` | `22` | `21` |
  | `a_fold_between_mark_and_confirm_does_not_retarget_the_abandon` | `GetAbandonQuestName()` = `C` | `B` |

  Two clusters are visible. The three `QUEST_LOG_UPDATE` ones are one extra signal at the seat:
  the code seats the slots through the watcher so it fires once (`76e108da`, 2026-10-30, "Carry the
  12340 quest exchange into the stock frames"), while the test's doc comment still says "the seat
  is silent" (from `fcb42ab4`, 2026-10-21). That is either stale tests or a seat that should not
  signal — decide it against `0x4de430` before touching code. The watch index, the row title and
  the abandon name all point one row later than the fixture expects (`3` for `2`, `C` for `B`,
  `Beast 10` for `Beast 1`), which reads as one shared row-mapping fault in the rebuild rather
  than three separate bugs. Start there.
- One was re-run with this branch's functional change removed and still failed, so none of the
  seven is caused by it.
- The 1.12 tracker test (`questlog_tests`) loads the installed 3.3.5a FrameXML into a 1.12 VM and
  dies at `UIParent.lua:329` — the recorded environment condition, unchanged.
- Out of scope and flagged by the combat walk: 24 deterministic loot-suite failures
  (shift-click sweep and loot verbs), and the 64-slot reputation fixture against Wrath
  `Faction.dbc`.
- The workspace `fmt`/`clippy -D warnings` gates are still red on `main` for debt outside these
  files; see the previous branch's note for the measurement.
