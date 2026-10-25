# 29 — Publish one immutable Spellwright revision through WM

**Status:** ready-for-agent

**Plan reference:** C14. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM validates and publishes a selected graph through a typed existing-owner path and receives its immutable identity or a meaningful refusal.

**Blocked by:** Existing [03 — Live-rule source contract](03-b001b1-live-rule-source-contract.md), recorded complete. No new queue blocker for source work; deployment requiring new native code remains gated by the restart-boundary decision and applicable authorization.

**Related existing issues:** [03](03-b001b1-live-rule-source-contract.md), [10](10-b001b2-live-change-proof.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode after explicit WM/server source assignment.

## Acceptance

- [ ] Inspect current Spellwright implementation and WM owners before choosing the missing publication adapter.
- [ ] Resolve the actual SubmitCommand/dispatcher gap without freeform mutation paths or reusing stock spell identities.
- [ ] Preserve server resource/quote/validation behavior and immutable revision pinning.
- [ ] Use observed deadlines: chunk upload has a 30-second inactivity timeout refreshed per accepted chunk; quote window is separately 120 seconds, subject to re-reading current source.
- [ ] Demonstrate a valid publication and invalid graph rejection without damaging the prior valid revision.
- [ ] Report any required binary deployment as a separate capability/deployment decision; do not disguise it as an already live hot change.

## Boundaries and handoff

This slice may have multiple implementation locations but one narrow end-to-end behavior. If the necessary dispatcher is larger than one context, split its exact vertical behavior before execution.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


