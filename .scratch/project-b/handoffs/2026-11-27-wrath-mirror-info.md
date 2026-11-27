# Match Wrath GetMirrorTimerInfo server-value semantics

Status: ready to merge
Owner: Claude Opus 5.5
Branch: fix/wrath-mirror-info
Issue: none
Next: run the focused mirror-timer test in the owner's Cargo slot, then review/merge

## Change

`GetMirrorTimerInfo(index)` now returns the timer's stored server value (`value_ms`). `GetMirrorTimerProgress(timer)` continues to interpolate from the packet timestamp, matching the 3.3.5a client split between the info and progress getters.

## Reproduction

The existing focus fixture starts `BREATH` at 60,000 ms, advances the clock by five seconds without another packet, and calls both getters. Before this change, both returned 55,000. Wrath returns 60,000 from `GetMirrorTimerInfo(2)` and 55,000 from `GetMirrorTimerProgress("BREATH")`.

## Verification

- `git diff --check`: passed.
- `rustfmt --edition 2021 --check crates/benilla-ui/src/script/wrath_natives.rs`: reports unrelated pre-existing formatting in nearby code; no formatting changes were applied outside this fix.
- Cargo tests/builds were not run because the repository reserves the Cargo slot for the owner.
- Client/server were not launched; no game data, credentials, or runtime state were accessed.
