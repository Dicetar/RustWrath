# Clippy: clear the remaining test-target warnings

Status: ready to merge
Owner: OpenCode DeepSeek overseer
Branch: oc/clippy-test-warnings
Issue: none
Next: land after green CI

## What changed

- `wrath_combat_log_feed.rs`: two test-local `let mut script` bindings lost the unused `mut`
  (`UiScript::run` takes `&self`).
- `test_ui.rs`: the loader-provider prose above `thread_local!` is a plain comment, not a
  doc comment.
- `honor_frame_tests.rs`: removed the unused `load_ui as load_xml` import.
- `entities/attach/redress.rs`: the test helper's unused `geosets` parameter is `_geosets`.

No behavior change; these were the last five warnings in the `benilla-app` lib-test target after
the run eab2260bf player-clippy pass.

## Checks

No local Cargo/build/test/client/server; CI is the compile authority.

## Not proven

Only the lint removal; no runtime change.
