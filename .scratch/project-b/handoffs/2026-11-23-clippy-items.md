Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-items
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-app/src/items.rs`: fixed clippy dead-code findings by removing two donor wrappers with no production or test consumer and gating two test-only wrappers:
  - removed `creator_guids` (fields wrapper; no caller — consumers use `creator_guids_item`)
  - removed `live_enchant` (fields wrapper; no caller — consumers use `live_enchant_item`)
  - gated `gift_creator_name` with `#[cfg(test)]` (only test callers; implementation delegates to `gift_creator_name_item`)
  - gated `already_bound` with `#[cfg(test)]` (only test callers; implementation delegates to `already_bound_item`)

No behavior change: removed wrappers only delegated to their `*_item` counterparts, and the gated wrappers remain available to tests.

## Findings removed vs kept

Removed (no real consumer beyond their own definition):
- line 289 `creator_guids`
- line 358 `live_enchant`

Gated with `#[cfg(test)]` (test-referenced):
- line 267 `gift_creator_name`
- line 314 `already_bound`

Kept (real non-test consumers):
- `gift_creator_name_item`, `creator_guids_item`, `already_bound_item`, `live_enchant_item`

## Checks not run

No Cargo, build, client or server runs (per owner policy). Compilation/tests are left to CI.

## What is not proven

Whether the gated wrappers still compile in test builds and whether any other clippy lints remain in this file; CI's target set and clippy configuration were not available locally.
