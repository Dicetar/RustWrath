# 25 — Perform normal attacks and one real spell

**Status:** ready-for-agent

**Plan reference:** C10. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The selected character targets one enemy, uses baseline attacks and one selected spell, and sees authoritative outcomes.

**Blocked by:** [24 — Complete route travel, chat and reconnect](24-route-travel-and-reconnect.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Pin the race/class/loadout, target and spell for this first slice.
- [ ] Implement the necessary action availability, targeting, cast lifecycle, cooldown/resource, aura and outcome messages through the actual UI/input path.
- [ ] Show server-owned damage, enemy reaction/death and relevant resources; never simulate authoritative credit in the client.
- [ ] Display rejected/out-of-range/insufficient-resource outcomes meaningfully for the selected actions.
- [ ] Keep normal baseline combat functional without experimental capability negotiation.
- [ ] List remaining class/spell/UI surfaces in the full replacement ledger.

## Boundaries and handoff

A working selected spell proves only this slice. Do not delay it for a complete Souls system or universal spell effects.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


