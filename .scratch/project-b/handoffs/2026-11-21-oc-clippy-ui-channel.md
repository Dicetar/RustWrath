# Player-clippy debt: crates/benilla-ui/src/script/channel.rs

Status: ready to merge
Owner: OpenCode / Beam Free
Branch: oc/clippy-ui-channel
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Findings: clippy needless_borrows_for_generic_args on lua.create_string(&localized("CHANNEL_CATEGORY_{GROUP,WORLD,CUSTOM}")) at ~761, ~774, ~791.
Fix: removed the `&` before localized(...) in all three rows; no other changes.
CI: not run yet
no behavior change; no new test; the existing the channel category/list tests are the check
