# Wrath Quest Log Row Order & World-Entry Rebuild Fix

Branch: fix/wrath-quest-log-row-order
Base: origin/main (9dc36e48)
Status: ready to merge
Next: source review, then manual `cargo test -p benilla-app ui_quest_log` verification of all 7 previously failing tests plus the new Wrath seat regression test

- Scope:
  - `crates/benilla-app/src/ui_quest_log.rs`
- Related parked branch: `origin/fix/quest-log-row-order` (handoff `.scratch/project-b/handoffs/2026-11-14-quest-log-row-order.md`)

## Findings & Root Cause

All 7 failing tests in `crates/benilla-app/src/ui_quest_log.rs` shared one regression introduced in `76e108da`: `feed_quest_log` switched its initial VM `seat` path from `script.set_quest_slots(slot_views)` to `script.quest_log_changed(slot_views)`.

- `feed_quest_log` runs in `WorldEnterSeats`, strictly before `feed_units` (`.after(WorldEnterSeats)`), which invokes `script.fire_world_enter_head(...)` -> `script.quest_log_enter_world()` to run the single initial `rebuild()` and emit `QUEST_LOG_UPDATE` once (`0x4de430`).
- Calling `script.quest_log_changed(slot_views)` inside `if seat` ran an eager first `rebuild()` and queued `QUEST_LOG_UPDATE` on the seat frame itself (before `PLAYER_LOGIN`/`PLAYER_ENTERING_WORLD`):
  1. `the_log_signals_at_the_world_entry_and_once_per_block_that_moves_it`: `rig.frame()` saw `["QUEST_LOG_UPDATE:nil"]` instead of `[]`.
  2. `the_last_awaited_record_rebuilds_the_list_once` and `an_item_added_before_the_quest_record_lands_toasts_on_its_arrival`: Rebuild #1 in `seat` set `record_waits[id] = 1`; Rebuild #2 in `quest_log_enter_world()` reset `pending = 0` then incremented `record_waits[id]` to `2`, causing `land_quest_records` to decrement `pending` twice per landing (premature rebuild on a 2-quest wait and duplicate `QUEST_LOG_UPDATE`).
  3. `a_fold_between_mark_and_confirm_does_not_retarget_the_abandon`, `a_completed_quest_toasts_its_end_text`, `a_walking_addon_on_twenty_cached_quests_walks_once_at_each_world_entry`, and `creature_names_in_flight_signal_once_each_as_they_land`: In `crates/benilla-ui/src/script/quest_log.rs:313-325` (`0x4de5b0`-`0x4de5c4`), `rebuild()` snapshots expanded/collapsed header IDs by negating each header's ID (`0.wrapping_neg() == 0`). When `seat` ran Rebuild #1 and populated `quest_log_headers` with group `0`, `quest_log_enter_world()` ran as Rebuild #2 and immediately folded group `0`, hiding quest rows / changing row indices (and causing `qsort` to reorder tied collapsed zone-0 quests) while also triggering an extra `GetNumQuestLogEntries` walk.

## Fix

- In `crates/benilla-app/src/ui_quest_log.rs::feed_quest_log`:
  - Restored `script.set_quest_slots(slot_views)` in the `if seat` branch so seating stores the slots silently ahead of `feed_units`'s `quest_log_enter_world()` cascade.
  - Added `rows_traced: bool` to `FeedMemo` (reset on `seat`, observed once on the subsequent frame if `quest_trace` is present) so `JournalRowInModel` trace milestones still record once post-world-entry rebuilds populate the model rows without changing model or event behavior.
  - Added `wrath_seat_is_silent_and_keeps_zone_zero_rows_expanded_at_world_entry` unit test covering Wrath world-entry silence (`rig.frame() == []`), single `QUEST_LOG_UPDATE` on `rig.enter_world()`, and expanded group-0 row order.
- Left `crates/benilla-ui/src/script/quest_log.rs` untouched (its `0x4de5b0` fold snapshot and `0x4de430` single-rebuild world-entry contract match 1.12/Wrath behavior).

## Verification

- Static caller/consumer audit completed across `crates/benilla-app/src/ui_quest_log.rs`, `crates/benilla-ui/src/script/quest_log.rs`, `crates/benilla-ui/src/script/session.rs`, `crates/benilla-app/src/ui_unit.rs`, and `crates/benilla-app/src/wrath.rs`.
- Cargo build and test suite not executed in this cloud sandbox (`cargo`/`rustc` toolchain not installed); ready for manual `cargo test -p benilla-app ui_quest_log` verification.
