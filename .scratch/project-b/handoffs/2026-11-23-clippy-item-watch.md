# Handoff: oc/clippy-item-watch

- Status: ready to merge
- Owner: not disclosed (model name is confidential in this product; no name of my own)
- Branch: oc/clippy-item-watch
- Issue: none
- Next: land after green CI

## What changed

- Removed the never-used `BagCache::rescan` method and its doc comment from
  `crates/benilla-app/src/item_watch.rs` (finding: "222 method rescan is never used"). Grepped the whole
  workspace including tests: no caller of `rescan` exists; the live `rescan_with` calls remain untouched.
  Reworded the stale `[`Self::rescan`]` doc link on `BagCache::rescan_with`. No other files touched, no
  `#[allow]` added, no behavior change (dead private method).

## Checks not run

- No cargo check/clippy re-run (Cargo was not run in this session).
- No tests run, no client or server launch.

## What is not proven

- Compile- and clippy-cleanliness of the edit: not re-verified here; GitHub CI compiles and tests the
  pushed branch.
- That no import or helper became unused: checked by grep (the removed method's names — `ObjectFields`,
  `BagEvent`, `seat_of` — are still used elsewhere in the file); not compile-checked.
