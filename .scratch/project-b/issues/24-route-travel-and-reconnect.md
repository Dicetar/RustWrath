# 24 — Complete route travel, chat and reconnect

**Status:** ready-for-agent

**Plan reference:** C09. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The player completes the Northshire/Elwynn route across tile boundaries, communicates, and reconnects into a coherent world.

**Blocked by:** [21 — Make route water and collision usable](21-route-water-and-collision.md); [23 — Walk, turn, jump and reconcile on one tile](23-local-movement-and-correction.md)

**Related existing issues:** [08](08-b006-movement-world-join.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Complete the agreed ten-minute walk/stops/turns/jumps route with tile streaming and the chosen discrepancy tolerance.
- [ ] Exercise required water/structure transitions, entity streaming and basic selection/chat.
- [ ] Demonstrate disconnect/return and a supported map transition with old state and publisher scope retired correctly.
- [ ] Show no parser/session failure, unbounded streaming buildup or old-map entities after transition.
- [ ] Record client/server/profile identity and raw scenario evidence; separate route completion from full movement-family compatibility.

## Boundaries and handoff

A route reduction must be explicitly recorded and cannot silently remove its boundary/transition acceptance.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


