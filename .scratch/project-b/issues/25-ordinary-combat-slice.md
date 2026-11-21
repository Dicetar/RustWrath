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

## Source gap table (2026-11-22)

Read-only source audit of branch `oc/gap-25-combat` at `52749c0f`. Every cited symbol was
grepped in the current tree before it was written; no Cargo, test, client or server was run.
"Covered" means the source path exists and is wired end to end, not that it was observed
running. No row claims visible gameplay.

| Acceptance / behavior | Status | Source owner (file:function) | Gap or ticket |
|---|---|---|---|
| Pin the race/class/loadout, target and spell for this first slice | Covered | `.scratch/project-b/handoffs/2026-11-20-oc-issues-25-27-gaps.md:Pinned slice` | The pin (Firna, human warrior; Young Wolf target; Heroic Strike rank 1 = spell 78; starting sword/shield) is a note, not source; its live eligibility (level, spell known, gear) is an owner-run check and was not run. |
| Action availability (usable/cooldown/range state) through the UI/input path | Covered | `crates/benilla-app/src/ui_action/state.rs:feed_action_state`; `crates/benilla-app/src/spell/usable.rs:spell_usable` | Source only; no client frame run. |
| Targeting through the UI/input path | Covered | `crates/benilla-app/src/spell/cast_target.rs:CastTargeting::context`; `crates/benilla-app/src/wrath.rs:wrath_wm_action_intake` | — |
| Normal attack sent from the UI path (CMSG_ATTACKSWING) | Covered | `crates/benilla-app/src/ui_action/drain.rs:use_action` (Attack slot → `UseOutcome::Attack`); `crates/benilla-app/src/wrath.rs:wrath_tap_commands` | — |
| Selected spell cast sent from the UI path (CMSG_CAST_SPELL) | Covered | `crates/benilla-app/src/spell/cast_send.rs:CastLadder::send_spell`; `crates/benilla-app/src/wrath.rs:wrath_tap_commands` | — |
| Cast lifecycle messages (start/go/result → cast bar and stock events) | Covered | `crates/benilla-app/src/spell/net.rs:on_cast_result`; `crates/benilla-app/src/ui_cast.rs:CastReports::bar`; `crates/benilla-app/src/ui_script/wrath_cast.rs:feed` | — |
| Cooldown and resource gating for the selected actions | Covered | `crates/benilla-app/src/spell/net.rs:on_cooldown_packet`; `crates/benilla-app/src/spell/cooldowns.rs:apply_spell_cooldowns`; `crates/benilla-app/src/spell/usable.rs:can_afford` | — |
| Aura messages through the UI path | Covered | `crates/benilla-app/src/spell/net.rs:on_aura_duration`; `crates/benilla-app/src/ui_aura.rs:feed_auras` | — |
| Outcome messages through the UI path (chat combat log) | Covered | `crates/benilla-app/src/combat_log/mod.rs:on_attacker_state`; `crates/benilla-app/src/ui_chat/combat/outcome.rs:melee_outcome` | — |
| Server-owned damage shown (swing figures) | Covered | `crates/benilla-app/src/wrath.rs:WrathWorldView::apply_swing`; `crates/benilla-app/src/combat_log/mod.rs:on_attacker_state` | The client queues the server's `SMSG_ATTACKERSTATEUPDATE` and formats it; no damage is computed locally. |
| Enemy reaction/death shown | Covered | `crates/benilla-app/src/creature_anim/net.rs:on_attack_start`; `crates/benilla-app/src/ui_chat/combat/watch.rs:death_lines` | — |
| Relevant resources shown (health/power) | Covered | `crates/benilla-ui/src/script/wrath_unit.rs:power`; `crates/benilla-ui/src/script/unit/bindings.rs:install` | — |
| Stock combat-log frame event coverage (`COMBAT_LOG_EVENT_UNFILTERED`) | Covered (source) | `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs:feed` | The current feed posts `PARTY_KILL`, `SWING_DAMAGE`, `SWING_MISSED`, `SPELL_DAMAGE`, `SPELL_MISSED`, `SPELL_HEAL`, and `SPELL_ENERGIZE`; packet-to-event coverage is source-backed, but no live client frame run was authorized. Landed evidence: `f902a884c`, `7e4628ca1`, `519095eb3`, `9b8ba646c`, `cd95c37d9`, `bdf7391cd`. |
| Combat-log history ring for the stock frame | Covered (source) | `crates/benilla-ui/src/script/combat_log.rs:CombatLogState`, `record_unfiltered`; `crates/benilla-ui/src/script/tick.rs:fire_event_into` | `COMBAT_LOG_EVENT_UNFILTERED` tuples are retained in a `VecDeque`, pruned by `combatLogRetentionTime`, filtered by the current selectors, and exposed through the stock cursor API. Landed evidence: `681426a2b` added the history producer/query path; `8a78e95ef` corrected GUID selector matching. No runtime frame walk was authorized. |
| Rejected / out-of-range / insufficient-resource outcomes (source path) | Covered | `crates/benilla-app/src/ui_action/cast_fail.rs:report_reaches_pop`; `crates/benilla-app/src/swing_refusal.rs`; `crates/benilla-app/src/spell/net.rs:cast_result` | Keys `ERR_SPELL_OUT_OF_RANGE`, `ERR_OUT_OF_MANA`, `ERR_OUT_OF_RAGE`, `ERR_BADATTACKPOS` and `ERR_BADATTACKFACING` are wired from the server's reason bytes. |
| Rejection text actually visible in a running client | Blocked: needs owner run | — | The mapping is source-verified; the rendered line needs a live client against the server. |
| Baseline combat without experimental capability negotiation | Covered | `crates/benilla-app/src/combat_mode.rs:evaluate_combat_intent` (capability defaults to `Missing`, effective mode stays `Baseline`) | No experimental packet is emitted; `CombatModeView` still carries `#![allow(dead_code)]` because the UI host slice has not consumed it. |
| Normal baseline combat works against the server | Blocked: needs owner run | — | Proof is a live session: the swing is accepted, the cast is accepted and the server's damage/XP land. |
| Remaining spell/UI surfaces listed in the full replacement ledger | Covered | `.scratch/project-b/issues/18-stock-replacement-inventory.md:2026-11-20 stock-area inventory` | The issue 18 combat-log rows are refreshed below from the landed feed and history commits; live stock-frame behavior remains unproven. |
| Remaining class-specific combat surfaces listed in the ledger | Missing | — | No ledger row enumerates class ability/rage/stance combat behavior; extend issue 18. |

Totals: 17 Covered (source), 1 Missing, 2 Blocked: needs owner run.

## Small tickets

1. **Feed spell damage/miss rows to the stock combat-log frame.** Landed in `f902a884c`, `7e4628ca1`, `519095eb3`, `9b8ba646c`, `cd95c37d9`, and `bdf7391cd`; the current source feed covers `PARTY_KILL`, swing damage/miss, spell damage/miss/heal/energize. Live frame delivery remains an owner-run check.
2. **Give the stock combat-log frame a real history API.** Landed in `681426a2b` and corrected by `8a78e95ef`; `CombatLogState` retains unfiltered tuples with retention, filtering, and cursor queries. Live stock history traversal remains an owner-run check.
3. **Refresh and extend the replacement ledger for combat.** The issue-18 rows are refreshed in this branch from landed source evidence. The class-specific ledger is recorded by the companion `pi/class-combat-ledger` branch; class-specific server/live behavior remains UNKNOWN.

Live-only checks, not source tickets (owner run): Firna's level/spell/loadout eligibility for the pinned slice; auto-attack and Heroic Strike accepted with damage and XP landing; the visible out-of-range/insufficient-resource text; the cast bar and aura frame during a real cast; stock combat-log frame delivery and history traversal.


