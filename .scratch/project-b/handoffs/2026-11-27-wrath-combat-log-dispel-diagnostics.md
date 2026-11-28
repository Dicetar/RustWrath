# Wrath combat-log failed-dispel diagnostics

- Branch: `refactor/benilla-tooltip`
- Worktree: `D:/Projects/wow/worktrees/refactor-benilla-tooltip`
- Scope: `benilla-app` only
- Status: implementation complete; Cargo and client validation remain owner-run

## Source-backed gap

`WrathWorldView` already bounded `SMSG_DISPEL_FAILED` rows and counted both queue overflow and terminal unresolved-name delivery. The Wrath combat-log feed consumed those rows and emitted `SPELL_DISPEL_FAILED`, but `CombatLogMemo` did not snapshot `spell_dispel_failed_dropped` or `spell_dispel_failed_unresolved`. As a result, this event family was silent in the feed's once-per-scope capacity and name-resolution diagnostics while sibling families reported the same conditions.

## Change

`crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs` now:

- stores failed-dispel dropped/unresolved counters in `CombatLogMemo`;
- snapshots and logs both counters through `update_failed_dispel_memo` during the feed pass; and
- tests both counter transitions with `failed_dispel_diagnostics_snapshot_queue_loss_and_name_fallback`.

No Wrath protocol/UI stub, `benilla-ui`, `ScriptCallHost`, or developer-crate files were changed.

## Checks

- `rustfmt --edition 2021 --check crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: passed.
- `git diff --check`: passed.
- Cargo check/tests were not run because the repository's Cargo slot is reserved for the owner.
- Client launch and hot-patch replay measurement were not run because the client slot is reserved.

## Follow-up

When the Cargo slot is available, run the focused `benilla-app` test covering the new diagnostic helper together with the existing Wrath combat-log feed checks. Keep the addon-harness and performance-split validation commands from their separate handoffs unchanged.
