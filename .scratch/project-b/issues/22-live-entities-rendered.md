# 22 — Render the live player and nearby replicated entities

**Status:** ready-for-agent

**Plan reference:** C07. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The graphical application enters the lab world and displays the actual player and nearby entities from the Wrath session replica.

**Blocked by:** [20 — Render and animate a route WotLK actor](20-wotlk-actor-rendered.md) Existing object reducer from [07 — B-005](07-b005-object-reducer.md) is also required and recorded complete.

**Related existing issues:** [08](08-b006-movement-world-join.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Wire the real Wrath session/reducer into the application rather than duplicating protocol logic from the renderless driver.
- [ ] Keep full server GUID identity separate from transient rendering/entity IDs.
- [ ] Show create/update/move/remove, target changes and visibility loss using source-observed state.
- [ ] Handle despawn/recreate without binding a stale target or presentation to a new object generation.
- [ ] Reconnect clears old scope and repopulates the rendered scene; unknown/unavailable fields are not invented.
- [ ] Preserve native observations from the same session-owned state and baseline behavior when the bridge is disabled.

## Boundaries and handoff

Stationary live world join is the acceptance scope. Locomotion comes next; do not declare the walk milestone complete here.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.



## Scheduling contract (Astra R5, implemented in `benilla-app/src/wrath.rs`)

The Wrath session feed follows the established io-thread pattern and the existing
`WorldStage` order (`benilla-world/src/schedule.rs`: Net → Input → Stream → Present):

1. The session loop (logon → world → reduce → producer → `pump_once`) runs on its own IO thread
   (`wrath::run_session`). The renderless driver's blocking `recv` loop stays on that thread and
   never enters a Bevy `Update` system.
2. It publishes latest-only self snapshots through `WrathLatest` (single slot) and bounded
   lifecycle/phase events through `WrathEvents` (capacity 16, oldest dropped). A full slot never
   blocks the packet loop.
3. The Net-stage drain system (issue 22's remaining work) drains at most
   `WRATH_DRAIN_LIMIT` (64) events per frame, applies world-scope invalidation **before** Input,
   and updates the typed state Input/Stream/Present read. Same-frame teardown/loading follows the
   existing `PendingTransfer` semantics.
4. Local fixed-step prediction (issue 23), variable-rate rendering, and authoritative server
   updates remain distinct; no fixed rate is imposed by this contract. Budgets to measure before
   performance claims: network backlog, frame drain count, coalescing rate, retry age, input
   latency.

Remaining for this issue: the entity/display translation from typed snapshots into the existing
`GuidIndex`/`Objects`/`FieldChanged` path (the 1.12 field layout differs from 3.3.5 — the
translation is per-field, not a struct copy), and registration of `WrathNetPlugin` alongside or
in place of `NetPlugin` by the app config owner.

## Source gap list (2026-11-21, OpenCode swarm)

Method: Read `crates/benilla-app/src/wrath.rs` (wrath_net_drain, wrath_apply_world, WrathEntity, WorldDigest, WrathNetPlugin), `crates/benilla-app/src/entities.rs` (stamp_arrivals, NetEntity, Arrival), `crates/benilla-app/src/net.rs` (NetPlugin, apply_net_updates), `crates/benilla-app/src/net/handlers.rs` (dispatch), `crates/benilla-app/src/net/objects/update.rs` (on_object_create, on_object_values, on_objects_removed), `.scratch/project-b/handoffs/2026-10-29-astra-to-oc.md`. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Wire the real Wrath session/reducer into the application rather than duplicating protocol logic from the renderless driver | `crates/benilla-app/src/wrath.rs:2569` (`WrathNetPlugin::build`), `crates/benilla-app/src/wrath.rs:8611` (`wrath_net_drain`), `crates/benilla-app/src/wrath.rs:10019` (`wrath_apply_world`), `crates/benilla-app/src/wrath.rs:13622` (`build_digest`) | The IO thread (`run_session`) publishes `WorldDigest` via `WrathLatest` and lifecycle events via `WrathEvents`; `wrath_net_drain` drains them in `WorldStage::Net` and `wrath_apply_world` translates `WrathEntity` into the `GuidIndex`/`ObjectStore` path. However, the **entity/display translation from typed snapshots into the existing `GuidIndex`/`Objects`/`FieldChanged` path** is documented as "issue 22's remaining work" in the module comment (`wrath.rs:18-20`). The per-field translation from 3.3.5 to 1.12 field indices is implemented in `wrath_apply_world` (lines 10019–11058) but the **ECS entity spawn/despawn and visual attachment for Wrath-streamed entities** is not connected: `WrathStreamed` component exists (`wrath.rs:9461`) but no system consumes it to attach models via `entities.rs` display/attach pipeline. |
| Keep full server GUID identity separate from transient rendering/entity IDs | `crates/benilla-app/src/wrath.rs:569` (`WrathEntity::guid`), `crates/benilla-app/src/wrath.rs:9461` (`WrathStreamed` component), `crates/benilla-app/src/entities.rs:15` (`NetEntity`), `crates/benilla-app/src/net/objects/update.rs:469` (`on_object_create` uses `GuidIndex`) | `WrathEntity` preserves the full 64-bit server GUID and `generation` (object generation from reducer). The `GuidIndex` maps GUID → ECS Entity. `WrathStreamed` marks entities from the Wrath path. However, **no consumer of `WrathStreamed` exists** to drive the visual pipeline (`entities.rs::display`/`attach`) using the preserved GUID. The existing `NetEntity` path (1.12) is separate; Wrath entities need their own visual translation that respects the GUID→ECS mapping without conflating the two. |
| Show create/update/move/remove, target changes and visibility loss using source-observed state | `crates/benilla-app/src/wrath.rs:9475` (`WrathWorldView::last_changes` with `created`/`updated`/`removed`), `crates/benilla-app/src/wrath.rs:10019` (`wrath_apply_world` processes `changes.created/updated/removed`), `crates/benilla-app/src/wrath.rs:3719` (`translate_live` for field diffs), `crates/benilla-app/src/net/objects/update.rs:469` (`on_object_create`), `crates/benilla-app/src/net/objects/update.rs:583` (`on_object_values`), `crates/benilla-app/src/net/objects/update.rs:700` (`on_objects_removed`) | `wrath_net_drain` computes `WrathChanges` (created/updated/removed) from `WorldDigest` entities diff. `wrath_apply_world` applies these to `GuidIndex` and emits `FieldChanged` messages via `Scene.field_changes`. Target changes are tracked in `WrathEntity::target` and projected to 1.12 fields 16/17 in `wrath_apply_world` (line 10380). **Visibility loss** (out-of-range) is handled by the 1.12 `Released` table in `objects/update.rs` but **Wrath path has no equivalent**: the reducer's `entities` map only contains in-range objects; despawn is inferred by absence from digest. The `WrathWorldView::entities.clear()` on scope loss (`wrath_net_drain:8850`) clears all, but **per-entity visibility transitions** (create/destroy without scope change) are not surfaced to the visual pipeline. |
| Handle despawn/recreate without binding a stale target or presentation to a new object generation | `crates/benilla-app/src/wrath.rs:569` (`WrathEntity::generation`), `crates/benilla-app/src/wrath.rs:10500` (`wrath_apply_world` checks `WrathGeneration` mismatch and despawns), `crates/benilla-app/src/wrath.rs:8850` (`wrath_net_drain` clears entities on `WorldTransfer`/`SessionEnded`), `crates/benilla-app/src/wrath.rs:2425` (`WrathWorldView::apply_party_kill`), `crates/benilla-app/src/wrath.rs:2450` (`WrathWorldView::apply_swing`) | `WrathEntity::generation` is preserved from the reducer. `wrath_apply_world` (line 10500) checks `WrathGeneration` component on existing ECS entity and **despawns if generation changed** (despawn/recreate handling). On `WorldTransfer`/`SessionEnded`, `wrath_net_drain` clears `view.entities` and despawns all `WrathStreamed` entities. Target changes (`WrathEntity::target`) are updated per digest. **Gap**: the visual pipeline (`entities.rs` display/attach) has no generation awareness — a recreated entity with same GUID but new generation gets a new ECS entity (correct), but **presentation aura anchors (`WrathAuraAnchors`) are cleared only on scope loss**, not on per-entity generation change. Also, **stale target binding** in UI (target ring, nameplates) is not explicitly cleared on entity generation change. |
| Reconnect clears old scope and repopulates the rendered scene; unknown/unavailable fields are not invented | `crates/benilla-app/src/wrath.rs:8611` (`wrath_net_drain` handles `SessionEnded`/`WorldTransfer`), `crates/benilla-app/src/wrath.rs:8850` (clears `view.entities`, `view.player_guid`, `view.vehicle`, etc.), `crates/benilla-app/src/wrath.rs:10019` (`wrath_apply_world` only projects `remote.*.is_some()` fields), `crates/benilla-app/src/wrath.rs:9725` (`push_corpse_fields` only copies delivered fields), `crates/benilla-app/src/wrath.rs:9796` (`push_item_fields` only projects delivered dwords) | On `SessionEnded` (clean logout or lost connection), `wrath_net_drain` clears all Wrath-scoped state (`view.entities.clear()`, resets epoch/generation). On new `WorldDigest`, `wrath_apply_world` repopulates from scratch. Field projection uses `Option` semantics: `is_some()` checks mean **only delivered fields are projected**; absent fields leave donor descriptor at 0 (not invented). **Gap**: the **visual pipeline** (model loading, display attachment) has no explicit "unknown/unavailable" handling — a missing `display_id` falls back to race/sex body display for players (`wrath_apply_world:10350`) but for NPCs/creatures, a missing display_id results in no model (cube fallback in `entities.rs::display`). This is acceptable behavior (reference does the same), but not explicitly documented as intentional. |
| Preserve native observations from the same session-owned state and baseline behavior when the bridge is disabled | `crates/benilla-app/src/wrath.rs:848` (`selected_from_env`), `crates/benilla-app/src/wrath.rs:854` (`interactive_from_env`), `crates/benilla-app/src/wrath.rs:2569` (`WrathNetPlugin::build` only activates if `config.is_some()`), `crates/benilla-app/src/game_plugins.rs:136` (registers `WrathNetPlugin` conditionally), `crates/benilla-app/src/preflight.rs:16` (documents Wrath path selection) | The Wrath session is **opt-in via `WOW_WRATH` env var** (`selected_from_env`). When disabled, `WrathNetPlugin` is inert (`config: None`), the stock `NetPlugin` runs (1.12 path), and no Wrath resources/systems are added. The `benilla-protocol-wrath` session/reducer runs only on the IO thread when selected. **Baseline behavior preserved**: the 1.12 `NetPlugin`/`apply_net_updates`/`objects` path is unchanged. **Gap**: the **WM native observation bridge** (`WrathControl::wm_actions`, `wrath_native::invalidate_presentation`) is only installed when Wrath is selected, but the **stock 1.12 presentation** (cast bars, channels, auras) remains active and unaffected. No conflict observed in source. |

### Tickets opened by this survey

1. `oc/wrath-entity-visual-pipeline` — Connect `WrathStreamed` entities to the visual pipeline: spawn/despawn display models via `entities.rs::display`/`attach` using `WrathEntity::display_id` and `WrathEntity::guid`, keyed by `GuidIndex` mapping. Acceptance: a `WrathStreamed` entity with valid `display_id` gets a `DisplayModel` attached; despawn removes it. Files: `crates/benilla-app/src/wrath.rs` (add system after `wrath_apply_world`), `crates/benilla-app/src/entities.rs` (consume `WrathStreamed`). Tests: `wrath_streamed_entity_gets_display_model`, `wrath_streamed_entity_despawn_clears_visual`.
2. `oc/wrath-per-entity-visibility-transitions` — Surface per-entity create/destroy (not scope-wide) to visual pipeline: when `WrathChanges.removed` contains a GUID, despawn its visual; when `created` appears, spawn visual. Acceptance: an NPC leaving range (removed from digest) loses its model; re-entering range (created in digest) gains it, without world transfer. Files: `crates/benilla-app/src/wrath.rs` (extend `wrath_apply_world` or add system), `crates/benilla-app/src/entities.rs`. Tests: `wrath_npc_leave_range_despawns_visual`, `wrath_npc_enter_range_spawns_visual`.
3. `oc/wrath-generation-aware-presentation-cleanup` — Clear presentation state (aura anchors, target UI) on per-entity generation change, not just scope loss. Acceptance: when `WrathGeneration` mismatch despawns an entity, its `WrathAuraAnchors` entry is removed and any target ring/nameplate binding to that GUID is cleared. Files: `crates/benilla-app/src/wrath.rs` (line 10500 generation check), `crates/benilla-app/src/wrath.rs:2509` (`WrathAuraAnchors`), `crates/benilla-app/src/target/mod.rs`. Tests: `wrath_generation_change_clears_aura_anchors`, `wrath_generation_change_clears_target_binding`.
4. `Blocked: needs owner run` — **Acceptance line 3 (target changes and visibility loss visible in-game)** and **Acceptance line 5 (reconnect repopulates scene visually)** require a live client/server to judge: source shows the data pipeline is wired, but whether the *rendered scene* actually displays entities, clears them on despawn, and repopulates on reconnect can only be verified by running the client against AzerothCore and observing the viewport.
