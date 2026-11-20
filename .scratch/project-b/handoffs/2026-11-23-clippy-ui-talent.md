# Collapsible talent if

Status: ready to merge
Owner: DeepSeek V4.1 Flash Free (opencode/deepseek-v4.1-flash-free)
Branch: oc/clippy-ui-talent
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-ui/src/script/talent.rs`: merged the nested `if` in `GetTalentTabInfo` (the
  pet-fallback branch) into one condition: `is_pet && pet_tabs.is_empty() && i == 1` now also
  carries the `!pet_talents.is_empty() || pet_unspent_points > 0` test, with the same body and the
  same short-circuit order. This addresses clippy's `collapsible_if` finding at line 376 of main
  run 44770772111. No `#[allow]`, no signature or call-site change, no behavior change.

## Checks not run

- Cargo build, clippy and tests not run locally (per task rules; CI is the build).
- `rustfmt --check --edition 2021 crates/benilla-ui/src/script/talent.rs` passes.
- No local client or server run.

## What is not proven

- That CI clippy is clean for this file beyond the reported finding; only the reported
  `collapsible_if` was addressed.
- That the branch's CI run is green; it has not completed yet.
