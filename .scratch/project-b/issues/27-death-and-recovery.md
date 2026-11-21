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

## Source gap table (2026-11-22)

| Acceptance / behavior | Status | Source owner (file:function) | Gap or ticket |
|---|---|---|---|
| Death, corpse/ghost state, and the valid resurrection/recovery paths (acceptance 1) | Covered | crates/benilla-app/src/death/mod.rs (feed_death, signal_death, DeathSteps::apply); crates/benilla-app/src/death/net.rs (on_corpse_query, on_resurrect_request, on_spirit_healer_confirm, on_death_release_loc, note_corpse/recheck_corpse/forget_corpse); crates/benilla-app/src/target/died.rs (death legs: target clear, cast cancel, loot release, signal_death_state) | None in source; unit-tested in death/mod.rs, death/net.rs and target/died.rs. A live demonstration (death to release to resurrect or corpse reclaim) is owner-run against a live server. |
| Reconnect while dead and after recovery; restore authoritative state without duplicate entities (acceptance 2) | Blocked: needs owner run | crates/benilla-app/src/net/session.rs (disconnected keeps the self avatar, despawns other entities once, clears session-scoped caches); crates/benilla-app/src/death/net.rs (on_session_end resets DeathNet; end_session_death_feed clears DeathFeedState; corpse-guid latch note_corpse/recheck_corpse/forget_corpse) | Duplicate-entity protection (single retained self avatar; index.retain) is covered and tested in session.rs. Gap: no code re-sends MSG_CORPSE_QUERY on a reconnect while dead — feed_death queries only on death-to-ghost, ghost-to-alive and login-ghost edges. Corpse position/range after a dead reconnect depends on AzerothCore re-pushing corpse state; unproven without a live client+server. |
| Rejected actions refused before and after a state transition: swing (dead target, out of range, bad facing), cast while dead unless castable-while-dead, cast cancelled on own death (acceptance 3, refusal half) | Covered | crates/benilla-app/src/swing_refusal.rs (apply_swing_refusals, show_swing_refusal); crates/benilla-app/src/spell/usable.rs:385 (is_dead_or_ghost gate with ATTR_CASTABLE_WHILE_DEAD); crates/benilla-app/src/target/died.rs (unit_died/own_death_cast cancel_reaping); crates/benilla-app/src/death/mod.rs (DeathSteps::use_soulstone abort_cast) | None for the refusal itself; unit-tested (swing_refusal latch/error keys, target/died.rs cast-popped-on-death test). |
| Resources/outcomes remain correct before and after the transition (acceptance 3, outcome half) | Blocked: needs owner run | Refusal paths above | Source shows the refusal; proving health, durability, soulstone and cooldown outcomes remain correct across death-to-recovery requires exercising a rejected action pre- and post-transition on a live client+server. Not provable from source. |
| Clear stale casts, targets, predicted movement and agent intents when their lifecycle expires (acceptance 4) | Covered | crates/benilla-app/src/target/died.rs (unit_died deselects the selection; cancels an in-flight cast with REASON_TARGETS_DEAD; own_death_head cancels with REASON_ERROR); crates/benilla-app/src/death/net.rs (on_clear_target to DeselectGuid from SMSG_CLEAR_TARGET); crates/benilla-app/src/combat_playback.rs (advance_playback: die clears the engine's running action and held request on player death; revival rebuilds a fresh engine); crates/benilla-combat-core/src/core.rs (reconnect clears buffer, pending and gesture, emits Reconnected) | Casts/targets unit-tested in target/died.rs. Predicted-movement clearing covered via combat_playback; combat-core reconnect intent clearing is source-only, no test yet. |
| Record remaining death/instance/pet edge cases in the compatibility ledger (acceptance 5) | Missing | No source compatibility ledger exists; the ledger is .scratch/project-b/compatibility/first-loop-behaviors.md, which records death rows D01-D06 (all Partial) anchored to e0be0d95 | Instance-bound corpse and pet-death edge cases are not recorded anywhere; no source function to record them into. Documentation task needing owner and live evidence. SMSG_RESURRECT_FAILED silence (D05) and instance/pet refusal edges remain open. |

Row totals: Covered 3, Missing 1, Blocked 2.

## Small tickets

- **Re-query the corpse on a reconnect while dead.** Behavior: on the first frame the returning self body is dead or a ghost, send exactly one MSG_CORPSE_QUERY so corpse position and range are restored after a dead reconnect. Files: crates/benilla-app/src/death/mod.rs (feed_death), crates/benilla-app/src/net/session.rs (world/reconnect entry); well under 300 lines added. Acceptance: (1) exactly one CorpseQuery is sent on dead-or-ghost return after a reconnect; (2) no duplicate query while an answer is pending; (3) the post-release ghost-to-alive query is unchanged. Named tests: `reconnect_while_dead_resends_corpse_query` and `no_duplicate_corpse_query_after_reconnect` in death/mod.rs tests, driven by the headless client with a captured NetCommands channel.
- **Unit-test combat-intent clearing on reconnect. Blocked: ownership boundary.** Behavior: core::reconnect clears the input buffer, pending inputs and gesture and emits Reconnected, so stale intents cannot replay in the new epoch. File: crates/benilla-combat-core/src/core.rs (reconnect) is owned by Astra/Claude tonight and must not be edited by this swarm; hand this test-only ticket to that owner. Acceptance: (1) reconnect with a newer epoch resets session/player/clock and clears buffer, pending and gesture; (2) an older or equal epoch is rejected with SessionNotAdvanced; (3) the Reconnected event carries the new epoch, session and player. Named tests: `reconnect_clears_buffer_pending_and_gesture`, `reconnect_rejects_stale_epoch` in core.rs tests.
- **Record instance and pet death edge cases in the compatibility ledger.** Behavior: add instance-bound corpse (corpse map differs from display map) and pet-death edge rows to the death section. File: .scratch/project-b/compatibility/first-loop-behaviors.md (doc only, roughly 30 lines); no source change. Acceptance: (1) new rows with N/R/T/P cases; (2) each row marked Partial/Missing/Blocked with a source owner; (3) no live claim — evidence is live-only. Named test: none (doc ticket); the live check is listed below instead.
- **Live-only checks (not source tickets).** Corpse restoration after a dead reconnect, health/durability/soulstone/cooldown outcomes before and after a rejected action across a transition, the instance-bound corpse marker/route, and a real dead-pet revive require a live client+server run and remain blocked in this audit.

## Evidence refresh (2026-11-24)

The death/recovery rows retain their source-only classifications. Landed evidence `4904db731` and
`ede7c8d2f` preserves the issue-27 audit and adds compatibility-ledger rows D07 (instance-bound
corpse) and D08 (pet death/revive). D07 remains Partial with live proof required; D08 remains Partial
for dead-pet refusal/gating and Missing for the Revive Pet command. No runtime behavior is promoted
to Covered.



