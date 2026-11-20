# Collapsible talent if

Status: ready to merge
Owner: Big Pickle (opencode/big-pickle)
Branch: oc/clippy-ui-talent
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-ui/src/script/talent.rs`: in `GetTalentTabInfo`, merged the nested pet-fallback
  `if` into its parent condition. The outer test `is_pet && pet_tabs.is_empty() && i == 1` now
  also carries `(!pet_talents.is_empty() || pet_unspent_points > 0)`, with the `||` group kept in
  parentheses so `&&` still binds tighter. The single body is unchanged and only one branch runs,
  so this is the idiomatic clippy `collapsible_if` rewrite with no behavior change. No `#[allow]`,
  no signature or call-site change. Addresses the finding at line 376 of main run 44770772111.

## Checks not run

- Cargo build, clippy and tests: not run locally (per task rules; CI is the build).
- rustfmt check, local client and server: not run.

## What is not proven

- That CI clippy is clean for this file beyond the reported finding; only `collapsible_if` at line
  376 was addressed.
- That the branch's CI run is green; it has not completed yet.
