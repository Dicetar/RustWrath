# Clippy: drop an unused error binding in the realm dial

Status: ready to merge
Owner: OpenCode DeepSeek overseer
Branch: oc/clippy-wrath-e
Issue: none
Next: land after green CI

## What changed

`wrath.rs` realm-dial arm `Err(e) if config.login_generation.is_some() && parked.is_some()`
does not read `e`; it is now `Err(_)`. The adjacent idle arm keeps its `e` for `is_idle(&e)`,
and the fallback arm keeps `Err(e) => return Err(e)`. No behavior change.

## Checks

No local Cargo/build/test/client/server; CI is the compile authority.

## Not proven

Only the lint removal; no runtime change.
