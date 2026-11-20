# Clippy: unused reagent_totem_refusal re-export

Status: ready to merge
Owner: Big Pickle (opencode/big-pickle)
Branch: oc/clippy-ui-action-import
Issue: none
Next: land after green CI

## What changed

Exactly one source file: `crates/benilla-app/src/ui_action/mod.rs`.

Removed `reagent_totem_refusal` from the `pub(crate) use errors::{...}` re-export list that
clippy flagged at line 63 as unused (W0611 / unused import). No `#[allow]`, no behavior change.
The remaining three lines of the import list were reflowed by `rustfmt` after the removal.

## Audit

- A workspace grep for the exact name (excluding `reagent_totem_refusal_in_player`) found no
  callers outside `crates/benilla-app/src/ui_action/errors.rs`: the function definition, its doc
  link, and six calls in that file's own tests. No module imports the name from `ui_action`;
  there are no glob imports of `ui_action`.
- `reagent_totem_refusal_in_player` is a distinct function, re-exported on the same line and
  consumed by `crates/benilla-app/src/spell/cast_send.rs`. It stays exported and unchanged.
- Removing the re-export does not delete or change the function itself, so its existing tests keep
  calling it directly in `errors.rs`; no `#[cfg(test)]` gating or unrelated cleanup was needed.

## Checks not run

- No Cargo, no build, no clippy run, no tests, no client or server (per task).
- Only `rustfmt` (formatter, not a build) was run on the edited file; it reported no changes
  afterwards and the diff is limited to the import list.
- CI: not read yet. CI runs on the pushed branch; report its result when available.

## What is not proven

- No local compilation or clippy confirmation: CI must validate that the import removal is
  complete and that no dependent code used the re-export.
- No runtime/behavioral verification; the change only removes an unused name from a re-export.
