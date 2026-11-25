# net names no feature: the handler registry inverted

Status: merged into main; final CI passed on the branch merge (`45254574028`)
Owner: Codex / GPT-5
Branch: refactor/net-handler-registry
Issue: none
Next: none; the consolidation push carries it

Step 3 of the build-speed plan (`2026-11-22-split-benilla-app.md`): make the core's `net` module
stop naming feature modules, so features can leave `crates/benilla-app` for crates that compile in
parallel. No gameplay change: packets are handled in the same order, with the same results.
