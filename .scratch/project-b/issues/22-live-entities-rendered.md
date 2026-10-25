# 22 — Render the live player and nearby replicated entities

**Status:** ready-for-agent

**Plan reference:** C07. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The graphical application enters the lab world and displays the actual player and nearby entities from the Wrath session replica.

**Blocked by:** [20 — Render and animate a route WotLK actor](20-wotlk-actor-rendered.md) Existing object reducer from [07 — B-005](07-b005-object-reducer.md) is also required and recorded complete.

**Related existing issues:** [08](08-b006-movement-world-join.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Wire the real Wrath session/reducer into the application rather than duplicating protocol logic from the renderless driver.
- [ ] Keep full server GUID identity separate from transient rendering/entity IDs.
- [ ] Show create/update/move/remove, target changes and visibility loss using source-observed state.
- [ ] Handle despawn/recreate without binding a stale target or presentation to a new object generation.
- [ ] Reconnect clears old scope and repopulates the rendered scene; unknown/unavailable fields are not invented.
- [ ] Preserve native observations from the same session-owned state and baseline behavior when the bridge is disabled.

## Boundaries and handoff

Stationary live world join is the acceptance scope. Locomotion comes next; do not declare the walk milestone complete here.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


