# 18 — Inventory the complete stock replacement gate

**Status:** ready-for-agent

**Plan reference:** C03. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A complete, reviewable compatibility ledger makes it impossible to label the fork a stock-client replacement while required behavior is missing.

**Blocked by:** None — can start immediately.

**Related existing issues:** [14](14-g1-expansion-gate.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** Astra with owner review; workers supply existing coverage facts.

## Acceptance

- [ ] Inventory pinned client-facing server messages, stock UI/API/events, asset variants, settings and installed addon builds; reconcile against all families in the plan.
- [ ] Record one row per independently assessable behavior with owner, dependencies, reference evidence, normal/error/reconnect/persistence scenarios and status.
- [ ] Separate baseline 3.3.5 features, owner-installed extensions, and experimental capabilities without dropping any mandatory release requirement.
- [ ] Record all required rows as WORKING, PARTIAL, BROKEN or UNKNOWN with source-observed/reported/reproduced evidence labels.
- [ ] Identify server-disabled or unavailable scenarios as blocked requirements; do not call them unsupported-by-choice without owner agreement.
- [ ] Produce the first dependency-ordered compatibility slices and a measurable fixed denominator for completion reporting.
- [ ] Record full stock-client replacement as a mandatory release gate; no subset milestone closes it.

## Boundaries and handoff

This is a bounded inventory/decision deliverable, not a claim that the broad compatibility implementation fits one agent context. Do not modify the owner's scope through inference.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


