# clippy: gate unused reagent_totem_refusal — oc/clippy-ui-action-errors

Status: ready to merge
Owner: Tidepool Alpha Free
Branch: oc/clippy-ui-action-errors
Issue: none
Next: land after green CI

## Finding
`cargo clippy` (player build): `511 function reagent_totem_refusal is never used`.

## What changed / kept
Changed:
- `crates/benilla-app/src/ui_action/errors.rs`: `#[cfg(test)]` added to
  `reagent_totem_refusal` (the `PlayerInventory` adapter that calls
  `reagent_totem_refusal_in_player`).
- `crates/benilla-app/src/ui_action/errors.rs:523`: the doc link to the now-test-only
  adapter on `reagent_totem_refusal_in_player` is de-linked to plain text, so it does not
  dangle in a non-test rustdoc build.
- `crates/benilla-app/src/ui_action/mod.rs:63`: the `reagent_totem_refusal` name removed
  from the `pub(crate) use errors::{...}` re-export list — nothing referenced it, and an
  import of a cfg'd-out item would break the non-test build.

Kept:
- `reagent_totem_refusal_in_player` (the live pre-send gate): its real caller is
  `crates/benilla-app/src/spell/cast_send.rs:849`; its helpers `first_missing_totem`,
  `first_short_reagent`, `first_missing_totem_in_player`, `first_short_reagent_in_player`
  stay (used by `crates/benilla-app/src/ui_action/cast_lines.rs` and tests).
- All other `errors.rs` exports (`attack_actor_refusal`, `keyed_line`, `show_messages`,
  `UiErrorKeys`, ...) unchanged.
- The six assertions in `mod totem_reagent_tests` (errors.rs:978-1007) still call the
  adapter through `#[cfg(test)]` builds.

Diff: 3 insertions / 2 deletions across the two files.

## Checks not run
- Clippy — no Cargo/build allowed this session; the player-build dead_code finding is
  gone by construction (the adapter is cfg'd out), to be confirmed by CI.
- Tests — no Cargo allowed; the gated adapter and its assertions compile only in test
  builds, to be confirmed by CI/the owner's test run.
- CI status — not observed by this agent; the branch is pushed for the normal CI run.

## Not proven
- Runtime behavior: unchanged — the adapter had no non-test callers, so non-test builds
  behave as before and the removed re-export name had no users.
- No clippy clean or green CI proven here; expected after merge via CI.
