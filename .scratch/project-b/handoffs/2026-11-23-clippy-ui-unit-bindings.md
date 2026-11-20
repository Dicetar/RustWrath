# Clippy fix: `crates/benilla-ui/src/script/unit/bindings.rs`

Status: ready to merge
Owner: Tidepool Alpha Free
Branch: oc/clippy-ui-unit-bindings
Issue: none
Next: land after green CI

## What changed
- `crates/benilla-ui/src/script/unit/bindings.rs` line 1104, `UnitDetailedThreatSituation` binding: `args.get(0)` → `args.first()`. Clippy finding from main run 44770772111: "1104 accessing first element with `args.get(0)`".
- Scanned the file: no other match for `isolate_lowest_one`/`lowest_one`/`low_bit`; no other `args.get(0)` sites; no manual range checks (`>=`/`<=` absent).
- No behavior change; no `#[allow]`.

## Checks not run
- No cargo/clippy build or test run, no client or server launch — per the task constraint (no Cargo, no build, no client/server). GitHub CI should be watched after push; report its result per docs/agents/sync.md.

## Not proven
- The clippy finding is not re-verified by running clippy (not permitted in this task).
- `UnitDetailedThreatSituation` behavior is unchanged by construction: `MultiValue::get(0)` and `MultiValue::first()` both yield the leading element (`Option<&Value>`; `None` on empty); the `.cloned().unwrap_or(mlua::Value::Nil)` chain is untouched.
