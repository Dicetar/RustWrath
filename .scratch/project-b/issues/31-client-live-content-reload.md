# 31 — Reload a client asset and Lua behavior with rollback

**Status:** ready-for-agent

**Plan reference:** C16. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A running client adopts a staged asset and Lua behavior revision and returns to its previous working version when validation fails or rollback is requested.

**Blocked by:** [20 — Render and animate a route WotLK actor](20-wotlk-actor-rendered.md) The selected Lua/UI file must load through the actual WotLK host; missing behavior becomes an explicit local blocker before this slice can close.

**Related existing issues:** [15](15-client-hotpatch.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Use a revisioned staging path outside the read-only source install; prepare the complete replacement before switching.
- [ ] Carry asset reload through the real GPU/dependency cache lifecycle; archive reload alone is insufficient.
- [ ] Use the existing UI/addon reload lifecycle and preserve declared state/SavedVariables behavior.
- [ ] Show the visible asset and actual Lua behavior change without restarting the client.
- [ ] Reject a corrupt/incompatible revision and keep the prior working content; prove rollback.
- [ ] Record active revision and affected scope; do not claim native code hot swapping.

## Boundaries and handoff

If the required WotLK UI host surface is unavailable, add the smallest behavior slice as a blocker rather than substituting an unrelated script demo.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


