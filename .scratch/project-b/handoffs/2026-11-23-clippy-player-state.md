# Player state clippy: feature-gate hotpatch-only accessors

Status: ready to merge
Owner: OpenCode / Big Pickle
Branch: oc/clippy-player-state
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-app/src/player/state.rs`: added `#[cfg(feature = "hotpatch")]` to
  `Player::velocity` (line 647) and `Player::pitch` (line 652). Two attributes only: no logic,
  signature, call-site or behavior change. No `#[allow]`.

## What was kept and why

- A workspace grep (source, tests, docs) found the only non-test callers in
  `crates/benilla-app/src/hotpatch.rs:105` (`let velocity = player.velocity()`) and
  `:123` (`pitch: player.pitch()`). That module is compiled only under
  `#[cfg(feature = "hotpatch")]` (`crates/benilla-app/src/lib.rs:58-59`; `hotpatch` is not a
  default feature in `crates/benilla-app/Cargo.toml`).
- The reported finding comes from the `player-clippy` gate
  (`cargo clippy -p benilla -p benilla-app --no-default-features -- -D warnings`): with that
  feature set the hotpatch module and its callers are absent, so these two `pub(crate)` accessors
  are dead code there.
- A real non-test consumer does exist, so the accessors were kept and gated on the feature that
  compiles that consumer, the same way a test-only item would be gated on `cfg(test)`. The
  hotpatch build is unchanged; the player build no longer compiles the unused accessors.
- No other caller was found, and no private helper or import became unused.

## Checks not run

- No Cargo, clippy, build, test, client or server run: forbidden by the task and by the owner's
  Cargo/client slot rules. CI has not yet been observed for this push.

## What is not proven

- Not proven locally that `player-clippy` is clean; the conclusion is source-only, from the gate's
  feature set and the enumerated call sites.
- Not proven that the other CI gates (fmt, workspace clippy, Windows check, tests) remain green;
  no local or CI result was read.
- No runtime or gameplay verification was attempted or claimed; no behavior change is intended.
