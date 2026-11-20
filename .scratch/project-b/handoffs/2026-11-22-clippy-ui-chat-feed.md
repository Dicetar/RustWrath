Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-ui-chat-feed
Issue: none
Next: land after CI

## What changed

- crates/benilla-app/src/ui_chat/feed.rs: gated `ChatLog::push_addon` with `#[cfg(test)]`. The method is only called from the crate's test module; the production path uses `push_addon_for_client` with the wrath flag from the environment. Gating keeps the test convenience helper available to tests while removing it from the non-test library build that clippy's dead_code lint reported.

## Checks not run

- No Cargo build/test/clippy run (per owner workflow; CI is the build). No client or server launched.

## What is not proven

- Not verified locally that CI clippy no longer reports the finding or that tests still compile with the gate; CI will confirm.
