# Player-clippy debt: crates/benilla-protocol-wrath/src/gameplay.rs

Status: ready to merge
Owner: OpenCode / Iceman Free
Branch: oc/clippy-proto-gameplay
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Findings: ~3464 `unused_assignments` on `target_guid`; ~4457 `manual_range_patterns`.
Fix: `let target_guid;` definite assignment (all match branches assign before use); `7 | 8 | 9` -> `7..=9`.
CI: not run yet
no behavior change; no new test; the existing the gameplay packet tests are the check
