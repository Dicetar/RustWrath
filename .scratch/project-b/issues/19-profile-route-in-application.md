# 19 — Render route terrain from a selected manifest profile

**Status:** ready-for-agent

**Plan reference:** C04. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The application's asset path renders the fixed route terrain from the selected profile and explains exactly which archive supplied each required asset.

**Blocked by:** None — can start immediately.

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode in a safely synchronized client checkout.

## Acceptance

- [ ] Pin the route start and boundary, required tiles/neighbors, WMO/doodad dependencies and relevant DBC/texture data from actual samples.
- [ ] Connect the existing manifest/profile implementation to the application's real asset-loading path; a separate file-listing CLI is insufficient.
- [ ] Preserve owner disabled toggles and read-only source install behavior. Profile selection is explicit; no hidden discovery/reordering.
- [ ] Render terrain from the base profile with correct scale/orientation; identify owner-profile differences through provenance.
- [ ] Missing or invalid dependencies yield actionable diagnostics and do not replace a working active profile with partial state.
- [ ] Record unresolved model/liquid dependencies as the blockers of C05/C06, without claiming the complete route gate.

## Boundaries and handoff

Do not rewrite MPQ reading when current sample evidence supports it. Do not broaden to a new asset pipeline unless the real integration exposes a specific gap.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


