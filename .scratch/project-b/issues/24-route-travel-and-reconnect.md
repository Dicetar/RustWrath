# 24 — Complete route travel, chat and reconnect

**Status:** ready-for-agent

**Plan reference:** C09. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The player completes the Northshire/Elwynn route across tile boundaries, communicates, and reconnects into a coherent world.

**Blocked by:** [21 — Make route water and collision usable](21-route-water-and-collision.md); [23 — Walk, turn, jump and reconcile on one tile](23-local-movement-and-correction.md)

**Related existing issues:** [08](08-b006-movement-world-join.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Complete the agreed ten-minute walk/stops/turns/jumps route with tile streaming and the chosen discrepancy tolerance.
- [ ] Exercise required water/structure transitions, entity streaming and basic selection/chat.
- [ ] Demonstrate disconnect/return and a supported map transition with old state and publisher scope retired correctly.
- [ ] Show no parser/session failure, unbounded streaming buildup or old-map entities after transition.
- [ ] Record client/server/profile identity and raw scenario evidence; separate route completion from full movement-family compatibility.

## Boundaries and handoff

A route reduction must be explicitly recorded and cannot silently remove its boundary/transition acceptance.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


## Source gap list (2026-11-21, OpenCode swarm)

Method: read issue 24, AGENTS.md, docs/agents/sync.md, handoffs 2026-10-28-oc-session-handoff.md, 2026-10-27-oc-native-state-consumer-contract.md, 2026-11-05-bounded-session-delivery.md, 2026-11-17-argon-say-chat-log.md and 2026-11-20-oc-issues-25-27-gaps.md, issues 08 and 23, and owning source under crates/benilla-app, crates/benilla-protocol-wrath, crates/benilla-world, crates/benilla-wm-driver; 12340 reference consulted only for layout questions. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Complete the agreed ten-minute walk/stops/turns/jumps route with tile streaming and the chosen discrepancy tolerance. | `crates/benilla-app/src/player/wire_in.rs:133:apply_server_moves`; `crates/benilla-app/src/player/wire_in.rs:476:apply_self_move`; `crates/benilla-world/src/terrain_stream/residency.rs:28:TerrainResidency`; `crates/benilla-world/src/terrain_stream/window.rs:StreamWindow` (`wanted_tiles`) | Movement reduction and a bounded tile residency/stream window exist, but no discrepancy/latency tolerance is recorded and there is no ten-minute route scenario or capture; live completion is unverified. |
| Exercise required water/structure transitions, entity streaming and basic selection/chat. | `crates/benilla-world/src/liquid/query.rs:456:liquid_at`; `crates/benilla-world/src/terrain_stream/collider.rs:223:placement_collider_data`; `crates/benilla-app/src/net/objects.rs:1133:packet`; `crates/benilla-app/src/target/mod.rs:102:Selection`; `crates/benilla-app/src/ui_chat.rs:69:run_chat` | The individual owners exist and are unit-tested, but no source scenario exercises water, structure, entity streaming, selection and chat together across tile boundaries; visible transitions need a live run. |
| Demonstrate disconnect/return and a supported map transition with old state and publisher scope retired correctly. | `crates/benilla-protocol-wrath/src/session.rs:328:reduce` (SMSG_NEW_WORLD at :395; worldport_acknowledged at :305); `crates/benilla-app/src/glue_session/reconnect.rs:20:answer_reconnect`; `crates/benilla-wm-driver/src/main.rs:583` (map transition retires publisher); `crates/benilla-wm-native/src/publisher.rs:527:retire_publisher` | Scope bump, worldport ack, reconnect glue and WM publisher retirement are implemented, but interactive disconnect/return and a server map change are not exercised; retirement ordering and old-state clearance are source/unit-tested only. |
| Show no parser/session failure, unbounded streaming buildup or old-map entities after transition. | `crates/benilla-protocol-wrath/src/session.rs:328:reduce` (Loading tail discard); `crates/benilla-protocol-wrath/tests/session.rs:478` (old-world tail discarded; old entities cannot leak); `crates/benilla-world/src/terrain_stream/window.rs:StreamWindow` (bounded wanted tiles); `crates/benilla-app/src/net/session.rs:449:on_worldport` | Protocol scope rules and bounded stream windows exist, but there is no parser-failure capture, streaming-memory bound assertion across a transition, or old-map entity check in the route scenario; live run required. |
| Record client/server/profile identity and raw scenario evidence; separate route completion from full movement-family compatibility. | `crates/benilla-ui/src/script/client.rs:142:GetBuildInfo`; `crates/benilla-protocol-wrath/src/session.rs:615:export`; `crates/benilla-world/src/build_id.rs` | Build identity and observation export owners exist, but server/profile identity and raw scenario evidence are run artifacts not produced by source; route completion is not yet separated from full movement-family compatibility in any recorded result. |

### Tickets opened by this survey

1. `Blocked: needs owner run` — acceptance bullets 1 and 2 (ten-minute route with tile streaming/discrepancy tolerance; water/structure/entity/selection/chat exercise) require a live client and server and cannot be judged from source.
2. `Blocked: needs owner run` — acceptance bullet 3 (disconnect/return plus a supported map transition) requires a live session and server map change; source has the hooks but no demonstrated retirement.
3. `Blocked: needs owner run` — acceptance bullet 4 (parser/session failure, streaming buildup, old-map entities) requires live observation; only protocol unit tests exist.
4. `Blocked: needs owner run` — acceptance bullet 5 (client/server/profile identity and raw scenario evidence) is a run artifact; source only supplies build identity and observation export.
