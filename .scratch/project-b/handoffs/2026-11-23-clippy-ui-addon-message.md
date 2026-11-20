# clippy-ui-addon-message

- Status: ready to merge
- Owner: Big Pickle (opencode/big-pickle)
- Branch: oc/clippy-ui-addon-message
- Issue: none
- Next: land after green CI

## What changed

- `crates/benilla-ui/src/script/addon_message.rs`: replaced the manual `min(150).max(1)` bounds on the WM_FORGE3 whisper chunk size with `.clamp(1, 150)` (clippy `manual_clamp` / clamp-like pattern), preserving identical behavior.

## Checks

- Not run (per task): Cargo/build/tests, client/server.

## Not proven

- No compilation, lint, or runtime verification performed in this session; CI is the source of truth.
