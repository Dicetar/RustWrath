# Clippy: gate test-only `seat_localized_class_names_from_chain`

Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-chr-classes
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-app/src/chr_classes.rs`: gated `seat_localized_class_names_from_chain` with
  `#[cfg(test)]`. The function is `pub(crate)` but is only called from the test module
  (`crates/benilla-app/src/ui_script/wrath_ui_slice_tests.rs`); in a non-test build clippy reports
  it as never used. Gating removes the finding without changing behavior.

## Findings removed

- `dead_code`/never-used `seat_localized_class_names_from_chain` (player-clippy finding).

## Caller audit

- Grepped the whole workspace for `seat_localized_class_names_from_chain`: only the one test caller.
  No production caller; no other helper or import became unused (the private helpers it uses are still
  used by the production `seat_localized_class_names` path).

## Checks not run

- No Cargo, no build, no clippy run, no tests, no client/server (per task).

## What is not proven

- No local compilation or clippy confirmation; CI must validate the fix and that the test build still
  resolves the gated function.
- No runtime/behavioral verification; the change is a compile-gate only and intended to be
  behavior-neutral.
