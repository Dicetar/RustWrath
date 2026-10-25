# 23 — Walk, turn, jump and reconcile on one tile

**Status:** ready-for-agent

**Plan reference:** C08. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** Player input moves the actual server-connected character with coherent local presentation and server corrections.

**Blocked by:** [22 — Render the live player and nearby replicated entities](22-live-entities-rendered.md)

**Related existing issues:** [08](08-b006-movement-world-join.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Use source-grounded movement messages and the shared application input/action path.
- [ ] Demonstrate walk, stop, turn and jump on the selected tile; required movement flags and acknowledgments are correct.
- [ ] Interpolate remote movement and apply server correction without inventing authority.
- [ ] Choose and record discrepancy/latency tolerances before live comparison; keep source-observed and predicted position distinct.
- [ ] Honor required forced movement/root/speed state for the selected scenario and inventory uncovered mechanisms.
- [ ] Prevent reconnect/epoch changes from reusing stale movement or input.

## Boundaries and handoff

Do not infer custom combat authority from the server accepting baseline movement. Do not enable speculative new opcodes.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


