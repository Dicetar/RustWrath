# 32 — WM publishes a client-content revision and rolls it back

**Status:** ready-for-agent

**Plan reference:** C17. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM publishes a versioned client-content revision to the intended client, receives application status and can restore the previous revision.

**Blocked by:** [17 — WM consumes native state with fallback](17-wm-consumes-native-state.md); [31 — Reload a client asset and Lua behavior with rollback](31-client-live-content-reload.md)

**Related existing issues:** [15](15-client-hotpatch.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode owns publication; GLM owns application adoption.

## Acceptance

- [ ] Use existing WM validation/publication and an explicit content delivery interface; observation-only transport is not already a publication protocol.
- [ ] Bind package identity, version, content hash, compatibility and actor/session scope.
- [ ] Stage and validate before application; interrupted transfer or malformed content preserves the current revision.
- [ ] Distinguish delivered, validated, applied and failed; WM observes the client's actual revision.
- [ ] Demonstrate live adoption and rollback with no client restart for the selected asset/Lua capability.
- [ ] Preserve baseline mode and do not admit arbitrary native code execution as content.

## Boundaries and handoff

Transport selection follows payload/lifecycle needs. Avoid adding a generic bidirectional framework solely because the observation bridge exists.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


