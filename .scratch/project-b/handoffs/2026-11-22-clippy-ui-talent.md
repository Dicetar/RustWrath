Status: ready to merge
Owner: Beam Free (opencode/beam-free)
Branch: oc/clippy-ui-talent
Issue: none
Next: land after CI

## What changed

- `crates/benilla-app/src/ui_talent.rs`: renamed the unused `known` parameter of `build_wrath_talent_groups` to `_`. The value was never read inside the function (non-active pages use an empty spell set and the active page comes from the snapshot), so the binding was dead. Signature and call sites are unchanged; no behavior change.

## Checks not run

- Cargo build/clippy/tests not run locally (CI is the build). No local client/server run.

## What is not proven

- That CI clippy is fully clean for this file beyond the reported finding; only the reported unused-variable warning was addressed.
