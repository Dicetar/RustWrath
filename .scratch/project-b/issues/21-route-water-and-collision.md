# 21 — Make route water and collision usable

**Status:** ready-for-agent

**Plan reference:** C06. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The selected route's terrain, water and required structures have coherent rendering and collision behavior in the application.

**Blocked by:** [19 — Render route terrain from a selected manifest profile](19-profile-route-in-application.md)

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Implement the route's observed MH2O data through existing liquid/rendering structures; do not treat missing legacy MCLQ as dry terrain.
- [ ] Pin required structure/doodad dependencies and use existing collision owners.
- [ ] Demonstrate correct water extent/height and transitions for the selected route, plus no falls through required ground/structures.
- [ ] Check both the required base profile and material owner-profile overrides where they affect collision/visibility.
- [ ] Record remaining unsupported liquid/material or structure variants in C03's ledger.

## Boundaries and handoff

This slice closes the chosen route only. Dynamic building navigation is a separate capability and must not be implied by route collision.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


