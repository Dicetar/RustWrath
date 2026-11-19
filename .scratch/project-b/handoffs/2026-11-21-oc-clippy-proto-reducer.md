# Player-clippy debt: crates/benilla-protocol-wrath/src/reducer.rs

Status: ready to merge
Owner: OpenCode / Iceman Free (opencode:iceman-free)
Branch: oc/clippy-proto-reducer
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Finding: `player-clippy` reported unused private `trait ReadBytes` and its blanket impl in `reducer.rs` (no callers).
Fix: Verified `grep` hits for `ReadBytes`/`bytes_read` in crate are only the trait definition and impl; removed both.
CI: not run yet
no behavior change; no new test
