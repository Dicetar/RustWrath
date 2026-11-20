Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-ui-auction
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-ui/src/script/auction.rs`: truncated the excessive-precision f32 literal on the auction deposit calculation from `0.004_166_666_9_f32` to `0.004_166_667_f32`, the shortest literal that preserves the same f32 value.

No behavior change: the parsed f32 value is identical, so `deposit_for` returns the same result.

## Checks not run

No Cargo, build, client or server runs (per owner policy). Compilation/tests are left to CI.

## What is not proven

That clippy no longer reports this finding in CI's exact target/configuration set; only the single excessive-precision literal was changed.
