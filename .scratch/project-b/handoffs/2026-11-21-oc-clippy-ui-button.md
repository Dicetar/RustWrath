# Player-clippy debt: crates/benilla-ui/src/script/button.rs

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer; the free models returned free-limit before working)
Branch: oc/clippy-ui-button
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Finding: `let mut is_check = false;` (~1307) is overwritten at ~1336 before any read
(clippy `unused_assignments`).
Fix: `let is_check;` with the single assignment before its first read; the click path and the
stored `engine_click` value are unchanged.
CI: not run yet. No behavior change; no new test; the existing button click/check tests are the
check. No Cargo, test, client or server run.
