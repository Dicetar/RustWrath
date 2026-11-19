# Player-clippy debt: crates/benilla-protocol-wrath/src/world.rs

Status: ready to merge
Owner: OpenCode / mimo-v2.6-flash-free
Branch: oc/clippy-proto-world
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Finding: player-clippy failed on `warning: function `head_bytes` is never used` (world.rs, ~line 92);
grep for `head_bytes` found only the definition in the crate.
Fix: deleted the dead `fn head_bytes(v: &[u8]) -> Vec<u8>`; no callers, no allow attributes.
CI: not run yet
no behavior change; no new test