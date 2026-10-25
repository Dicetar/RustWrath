# 27 — Recover from death and reject invalid actions

**Status:** ready-for-agent

**Plan reference:** C12. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The player can die, release/recover/resurrect in the selected scenario, and sees correct refusal of actions that are unavailable.

**Blocked by:** [25 — Perform normal attacks and one real spell](25-ordinary-combat-slice.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Demonstrate death, corpse/ghost state and the selected valid resurrection/recovery path.
- [ ] Reconnect while dead and after recovery; restore the authoritative state without duplicate entities.
- [ ] Exercise one rejected action before and after a state transition and show resources/outcomes remain correct.
- [ ] Clear stale casts, targets, predicted movement and agent intents when their lifecycle expires.
- [ ] Record remaining death/instance/pet edge cases in the compatibility ledger.

## Boundaries and handoff

Use a disposable declared lab character under authorization. Do not extrapolate one recovery path to all class/instance mechanics.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


