# Repair the main-branch field-assert compile

Status: in progress
Owner: OpenCode / DeepSeek 4.1 Flash (overseer)
Branch: oc/fix-main-field-test
Issue: none
Next: land after the gates/windows run is green

## What exists now

`18b51e97` ("finally loaded") added field-audit tests in `crates/benilla-app/src/wrath.rs` that
call `ObjectFields::get_u32`, which was private to `benilla-protocol`. The gates job on main
(44656735007) fails to compile with E0624, so every branch based on main is red before its own
code is judged.

The raw reader is now public with its existing semantics and a doc comment. The first attempt
changed one test to `raw_fields()` but the commit has dozens of other `get_u32` asserts, so the
accessor is the smaller and consistent repair. No behavior changes.

## Checks

- 44660800241 red: the first attempt did not cover the other `get_u32` asserts.
- Not run locally: no Cargo by owner instruction.
- The next CI run on this branch is the compile/test evidence.

## Not proven

- The field-audit test's live meaning is unchanged from the commit that added it; only visibility
  changed.
