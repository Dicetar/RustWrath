# 28 — WM performs one typed player action with human override

**Status:** ready-for-agent

**Plan reference:** C13. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM observes a scoped opportunity, requests one permitted player action and receives its actual outcome while the player can take control.

**Blocked by:** [17 — WM consumes native state with fallback](17-wm-consumes-native-state.md); [25 — Perform normal attacks and one real spell](25-ordinary-combat-slice.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md), [11](11-b008-wm-observation-adapter.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode owns WM integration; GLM owns the client adapter.

## Acceptance

- [ ] Reuse existing WM policy/request/result owners and the client action path already used by input/UI.
- [ ] Bind intent to player/session, request identity, deadline and relevant expected state; reject duplicates, expired and wrong-scope requests.
- [ ] Distinguish queued, accepted, rejected, completed and expired outcomes; network send success is not gameplay completion.
- [ ] Demonstrate one real action and one refusal using the corresponding server result.
- [ ] Human control revokes conflicting agent control and reconnect cannot replay an old request.
- [ ] No freeform shell/SQL/GM/config mutation lane or second director is introduced.

## Boundaries and handoff

This requires an action transport/contract, not extending the observation payload into an arbitrary executable-command container.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


