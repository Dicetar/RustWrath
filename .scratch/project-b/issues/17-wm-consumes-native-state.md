# 17 — WM consumes native state with fallback

**Status:** ready-for-agent

**Plan reference:** C02. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The existing WM context reads a live client observation, and safely uses its existing fallback when native state is unavailable.

**Blocked by:** [16 — Real session to native receiver](16-real-session-to-receiver.md)

**Related existing issues:** [11](11-b008-wm-observation-adapter.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode once explicitly assigned WM production-source edit scope; GLM provides the client checkpoint.

## Acceptance

- [ ] Reuse one store, receiver and loader composed by the runtime helper in the existing WM lifecycle.
- [ ] Thread the optional loader through all existing relevant context call sites, rather than adding a second orchestration path.
- [ ] Show a real context contains the accepted player's native state and preserves unknown target/incomplete visibility semantics.
- [ ] Show no-token startup, invalid configuration/bind failure, expired state, wrong player and disconnect leave the existing fallback behavior intact.
- [ ] Shutdown invalidates state and releases owned resources; repeated close is safe. Report actual receiver health rather than merely a prior successful-start flag.
- [ ] Package/import the integration from the owned project rather than an absolute dependency on an agent worktree.
- [ ] Record actual context-read evidence, startup/shutdown behavior and a reversible integration checkpoint.

## Boundaries and handoff

Prepare source changes under assigned WM scope; deployment/live operation needs applicable authorization. Read current WM workflow and lifecycle conventions before edits.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


