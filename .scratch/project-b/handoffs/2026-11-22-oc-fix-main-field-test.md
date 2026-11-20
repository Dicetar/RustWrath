# Repair the main-branch field-assert compile

Status: in progress
Owner: OpenCode / DeepSeek 4.1 Flash (overseer)
Branch: oc/fix-main-field-test
Issue: none
Next: land after the gates/windows run is green

## What exists now

`18b51e97` ("finally loaded") added a test in `crates/benilla-app/src/wrath.rs` that calls
`ObjectFields::get_u32`, which is private to `benilla-protocol`. The gates job on main
(44656735007) fails to compile with E0624, so every branch based on main is red before its own
code is judged.

The test now reads the same fields through the public `raw_fields()` iterator and still asserts
the `Object` gate: mapped fields (2, 3, 4) are present with their narrowed values, and the
unit-only/gated fields (5, 22, 34, 36, 46) are absent rather than zero.

## Checks

- Not run locally: no Cargo by owner instruction.
- Source only: one test helper in one file. No behavior change.

## Not proven

- The CI run for this branch is the compile/test evidence; the field-audit test's live meaning is
  unchanged from the commit that added it.
