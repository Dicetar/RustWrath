# 30 — Change WM selection A to B and roll back live

**Status:** ready-for-agent

**Plan reference:** C15. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM selects revision B for a logical action while active revision A instances obey their pinning contract, then rolls the selection back without restarting the server.

**Blocked by:** [29 — Publish one immutable Spellwright revision through WM](29-typed-live-graph-publication.md) Existing WM/server evidence paths suffice; native observation delivery is not a prerequisite.

**Related existing issues:** [03](03-b001b1-live-rule-source-contract.md), [10](10-b001b2-live-change-proof.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode with lab owner execution scope.

## Acceptance

- [ ] Use a relation/behavior change between A and B, not only a numeric slider.
- [ ] Implement the WM-owned logical-action-to-owned-spell binding through existing selection owners.
- [ ] Capture separate evidence for A behavior, B behavior, actual WM selection and the real resulting action.
- [ ] Demonstrate invalid-B rejection with A usable and rollback with coherent active-instance policy.
- [ ] Record identical running server PID and binary identity across the routine A/B/rollback proof, plus content revisions and timestamps.
- [ ] Keep platform-level authoring/casting proof separate from WM-owned selection proof; no fabricated proof fields.

## Boundaries and handoff

A one-time prior deployment, if explicitly approved, does not waive the no-restart acceptance for the demonstrated live change itself.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


