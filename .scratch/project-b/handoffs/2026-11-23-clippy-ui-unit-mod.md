Status: ready to merge
Owner: nemotron-3.5-super-vl-free
Branch: oc/clippy-ui-unit-mod
Issue: none
Next: land after green CI

## What changed

Fixed clippy finding `function check_unit_token is never used` (main run 44770772111, reported at `crates/benilla-ui/src/script/unit/mod.rs:594`).

Removed the never-used `pub(crate) fn check_unit_token(token: &Option<String>) -> mlua::Result<()>` wrapper and its doc comment from `crates/benilla-ui/src/script/unit/mod.rs`. It only delegated to `check_unit_token_for(token, false)` and had no callers anywhere in the workspace — the actual production callers (bindings.rs, char_stats.rs, guild.rs, inspect.rs, quest_log.rs, pvp/mod.rs, tooltip_unit.rs, trade.rs, tooltip_item/mod.rs, and `with_unit` in this file) all call `check_unit_token_in` or `check_unit_token_for`, which are untouched.

No behavior change, no `#[allow]`, no imports removed (none became unused; `mlua` types are still used throughout the file). The remaining textual mentions of `check_unit_token` are a doc link on `check_unit_token_for` and a comment in `unit::tests.rs`; neither is a call site.

## Checks not run

No Cargo build, check, clippy, test, or client/server run was performed (out of scope for this session; the owner runs Cargo by hand, CI compiles and tests every pushed branch).

## What is not proven

- The crate compiles and clippy is green after this edit (to be confirmed by CI / the owner's launch).
- No cfg-gated or out-of-workspace caller of `check_unit_token` exists beyond the workspace grep (grep covered the whole checkout; only doc/comment mentions remain).
