# 20 — Render and animate a route WotLK actor

**Status:** ready-for-agent

**Plan reference:** C05. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A source-identified WotLK actor appears with the correct mesh, skin, textures and animation in the same application route.

**Blocked by:** [19 — Render route terrain from a selected manifest profile](19-profile-route-in-application.md)

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Use the sampled v264 model and external skin format; verify actual header/array/layout requirements rather than just widening version acceptance.
- [ ] Select one required creature and the selected player's model variant; handle their required texture/bone/animation dependencies.
- [ ] Read external skins through the manifest asset source and feed the existing renderer-facing types where appropriate.
- [ ] Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile.
- [ ] Malformed/truncated dependencies fail with useful diagnostics; preserve currently supported older formats.
- [ ] List any other model/animation variants as compatibility ledger gaps instead of claiming universal model support.

## Boundaries and handoff

No donor engine replacement or unsupported all-version promise. Future checks require execution authorization; source implementation alone remains PARTIAL.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


