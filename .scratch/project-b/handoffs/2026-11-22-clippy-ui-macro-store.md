# Clippy: gate the test-only macro parser

Status: ready to merge
Owner: Hy4 flash Free (investigation) / OpenCode DeepSeek overseer (one-line gate)
Branch: oc/clippy-ui-macro-store
Issue: none
Next: land after Windows check

## What changed

`ui_macro::store::parse` is only called from the module's `#[cfg(test)]` tests and
`ui_macro/tests.rs` (also test-only); production calls `parse_with_cap` directly. The
lib-target dead-code lint therefore fired. The function is now `#[cfg(test)]` rather than
removed, so all 22 test call sites stay valid. No behavior change.

## Checks

- No local Cargo/build/test/client/server; CI is the compile authority.
- Source only: one attribute on one function.

## Not proven

- The CI lint count is the evidence; the macro parser behavior is unchanged.
