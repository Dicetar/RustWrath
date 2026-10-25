# 26 — Complete a quest, loot and preserve the reward

**Status:** ready-for-agent

**Plan reference:** C11. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The player accepts a real quest, completes its objective, loots and turns it in, then reconnects with the correct reward and inventory.

**Blocked by:** [25 — Perform normal attacks and one real spell](25-ordinary-combat-slice.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Pin an existing nearby quest and initial character/inventory state.
- [ ] Implement the narrow gossip, quest log, objective, loot, inventory and reward UI/protocol path.
- [ ] Show objective and reward state come from the server; failed/repeated interactions do not duplicate reward or loot.
- [ ] Reconnect after the reward and confirm inventory/quest state persists.
- [ ] Record protocol correctness, UI completeness and gameplay outcome separately; list uncovered quest types.

## Boundaries and handoff

Do not construct a client-only demo quest or modify production content merely to make the test pass.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


