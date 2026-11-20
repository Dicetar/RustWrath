Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-wm-observation
Issue: none
Next: land after green CI

## What changed

- crates/benilla-wm-native/src/observation.rs: collapsed the nested `if let Some(active) = *active` into the outer `if let Some(active) = self.powers.get(...)` using `.and_then`, removing one redundant nesting level.
- crates/benilla-wm-native/src/observation.rs: converted six `Option::ok_or_else(|| Error::InvalidObservation(...))` sites (active channel spell id, duration, remaining time, start clock, update clock; terminal cast edge spell id) to `Option::ok_or(Error::InvalidObservation(...))`, since the error value is constant.

## Checks not run

- No Cargo build/test/clippy run (per owner workflow; CI is the build). No client or server launched.

## What is not proven

- Not verified locally that CI clippy no longer reports the eight findings or that the crate still compiles; CI will confirm. The edits are mechanical and preserve short-circuit/constant-error behavior.
