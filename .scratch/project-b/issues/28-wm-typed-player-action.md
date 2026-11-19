# 28 — WM performs one typed player action with human override

**Status:** ready-for-agent

**Plan reference:** C13. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM observes a scoped opportunity, requests one permitted player action and receives its actual outcome while the player can take control.

**Blocked by:** [17 — WM consumes native state with fallback](17-wm-consumes-native-state.md); [25 — Perform normal attacks and one real spell](25-ordinary-combat-slice.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md), [11](11-b008-wm-observation-adapter.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode owns WM integration; GLM owns the client adapter.

## Acceptance

- [ ] Reuse existing WM policy/request/result owners and the client action path already used by input/UI.
- [ ] Bind intent to player/session, request identity, deadline and relevant expected state; reject duplicates, expired and wrong-scope requests.
- [ ] Distinguish queued, accepted, rejected, completed and expired outcomes; network send success is not gameplay completion.
- [ ] Demonstrate one real action and one refusal using the corresponding server result.
- [ ] Human control revokes conflicting agent control and reconnect cannot replay an old request.
- [ ] No freeform shell/SQL/GM/config mutation lane or second director is introduced.

## Boundaries and handoff

This requires an action transport/contract, not extending the observation payload into an arbitrary executable-command container.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


## Source gap list (2026-11-21, OpenCode swarm)

Method: Read the issue, AGENTS.md, docs/agents/sync.md, handoffs under .scratch/project-b/handoffs/ (including 2026-11-20-oc-issues-25-27-gaps.md), and the owning source in benilla-wm-native (action.rs, publisher.rs, producer.rs), benilla-wm-driver (main.rs), benilla-app/src/wrath_native.rs, benilla-app/src/wrath.rs (WrathWmActions, wrath_wm_action_intake, wrath_dispatch_events, route_wm_action_outcomes), benilla-app/src/net.rs (ClientCommand), and benilla-protocol-wrath. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Reuse existing WM policy/request/result owners and the client action path already used by input/UI | `crates/benilla-app/src/wrath.rs:1158:WrathWmActions`, `crates/benilla-app/src/wrath.rs:3026:wrath_wm_action_intake`, `crates/benilla-app/src/wrath.rs:8504:route_wm_action_outcomes`, `crates/benilla-wm-native/src/publisher.rs:100:ActionBridge`, `crates/benilla-app/src/net.rs:1024:ClientCommand` | The WM action path reuses `ClientCommand` for dispatch and `ActionBridge` for transport. `WrathWmActions` owns intake/pending/results. `wrath_wm_action_intake` maps `ActionIntent` to `ClientCommand`. `route_wm_action_outcomes` correlates server replies. No gap in the wiring; the owners exist and are connected. |
| Bind intent to player/session, request identity, deadline and relevant expected state; reject duplicates, expired and wrong-scope requests | `crates/benilla-wm-native/src/action.rs:130:ActionRequest` (fields: request_id, session_epoch, world_generation, player_guid, deadline_unix_ms, intent), `crates/benilla-app/src/wrath.rs:3026:wrath_wm_action_intake` (validates player_guid, session_epoch, world_generation, phase, deadline, duplicates, cancelled-before-admission, observed conflicts) | Validation logic is present and comprehensive. The `ActionRequest` struct carries all required binding fields. `wrath_wm_action_intake` rejects on scope mismatch, deadline expiry, duplicate request_id, pre-admission cancellation, and observed conflicts (dead, active cast/channel). No gap. |
| Distinguish queued, accepted, rejected, completed and expired outcomes; network send success is not gameplay completion | `crates/benilla-wm-native/src/action.rs:180:ActionStatus` (Accepted, Sent, Launched, Succeeded, Failed, Rejected, Cancelled, Expired, Unsupported, Unknown), `crates/benilla-app/src/wrath.rs:1196:answer`, `crates/benilla-app/src/wrath.rs:1352:bind_native_cast` (emits Sent), `crates/benilla-app/src/wrath.rs:1454:observe_native` (emits Launched/Succeeded/Failed/Rejected), `crates/benilla-app/src/wrath.rs:3026:wrath_wm_action_intake` (emits Accepted/Rejected/Expired/Cancelled/Unsupported/Unknown), `crates/benilla-app/src/wrath.rs:8504:route_wm_action_outcomes` (emits Succeeded/Failed/Unknown) | Status enum is complete and used. `Accepted` on admission, `Sent` only for casts when START edge observed, `Launched`/`Succeeded`/`Failed`/`Rejected` from native cast correlation or server replies, `Expired`/`Cancelled`/`Rejected`/`Unsupported`/`Unknown` from intake/refusal/scope loss. Gap: non-cast actions (AttackStart, Interact, GossipSelect, Quest*, Loot*) have no `Sent` milestone — they go `Accepted` → terminal via server reply only. This is a design choice (no wire-level ack for those opcodes) but means the `Sent` state is cast-only. |
| Demonstrate one real action and one refusal using the corresponding server result | Code paths exist: `AttackStart` → `AttackSwingError`/`AttackStart` (`route_wm_action_outcomes:8524`), `Cast` → `CastFailed`/`SpellGo` (`observe_native:1454`), `Interact` → `GossipMenu` (`route_wm_action_outcomes:8538`), `QuestAccept/Complete/ChooseReward` → `QuestComplete`/`QuestGiverFailed` (`route_wm_action_outcomes:8545`), `LootTake/Money/Release` → `LootRemoved`/`LootMoneyNotify`/`LootReleaseResponse`/`LootError` (`route_wm_action_outcomes:8559`) | This acceptance line requires a live client/server run to demonstrate. The source wiring is complete for the mapped actions, but visible gameplay confirmation (server refusal text, combat log, quest progress, loot received) can only be judged in a live session. Blocked: needs owner run. |
| Human control revokes conflicting agent control and reconnect cannot replay an old request | `crates/benilla-wm-native/src/action.rs:130:ActionRequest` (session_epoch, world_generation scope fields), `crates/benilla-app/src/wrath.rs:1238:retire_all`, `crates/benilla-app/src/wrath.rs:1263:retire_scope` (answer pending with Unknown on session end/world transfer), `crates/benilla-app/src/wrath.rs:3026:wrath_wm_action_intake` (rejects wrong-scope requests), `crates/benilla-wm-native/src/publisher.rs:666:pump_actions` (forwards intents from publisher) | Scoping via `session_epoch`/`world_generation` is enforced at intake and on scope loss (`retire_all`/`retire_scope` answer `Unknown`). Reconnect (new epoch/generation) causes old requests to be rejected at intake. Human revocation: the `cancels` channel in `ActionBridge` allows explicit cancellation; `wrath_wm_action_intake` processes cancels before dispatch (answers `Cancelled`) and after dispatch (answers `Unknown`). No automatic "human takes control" detection exists — it relies on scope change or explicit cancel. This meets the requirement but the human-override path is implicit via scope invalidation, not an explicit revocation signal. |
| No freeform shell/SQL/GM/config mutation lane or second director is introduced | `crates/benilla-wm-native/src/action.rs:30:ActionIntent` (closed enum: SetTarget, AttackStart, AttackStop, Cast, CastCancel, Interact, GossipSelect, UseObject, QuestAccept, QuestComplete, QuestChooseReward, LootTake, LootMoney, LootRelease, MoveStart, MoveStop) | The `ActionIntent` enum is closed and specific. No generic "execute command", "run SQL", "GM command", or arbitrary payload variants exist. The transport only carries typed intents that map 1:1 to existing `ClientCommand` owners. No second director or freeform lane is introduced. No gap. |

### Tickets opened by this survey

1. `oc/wm-action-sent-milestone` — Add a `Sent` milestone for non-cast actions (AttackStart, Interact, GossipSelect, Quest*, Loot*) by emitting `ActionStatus::Sent` after the command owner accepts the `ClientCommand` (i.e., after `net.0.send` succeeds in `wrath_wm_action_intake`). 1-2 files: `crates/benilla-app/src/wrath.rs` (modify `wrath_wm_action_intake` around line 3350), `crates/benilla-wm-native/src/action.rs` (document that `Sent` is now emitted for all action types). Checkable acceptance: (a) `ActionStatus::Sent` is emitted for an `AttackStart` intent before the server reply arrives; (b) the `Sent` result carries `ActionCorrelation::Direct` and no `attempt_serial`. Named test: `wm_action_sent_milestone_non_cast` in `benilla-app` or `benilla-wm-native`.

2. `Blocked: needs owner run` — Acceptance line 4 ("Demonstrate one real action and one refusal using the corresponding server result") requires a live client and server to observe the actual server replies (e.g., `SMSG_ATTACKSWING_ERROR`, `SMSG_CAST_FAILED`, `SMSG_GOSSIP_MESSAGE`, `SMSG_QUESTGIVER_OFFER_REWARD`, `SMSG_LOOT_RESPONSE`) and confirm the WM result correlates correctly. Only a live run can verify the visible gameplay outcome (combat log entry, quest progress, loot received, refusal dialog text).

