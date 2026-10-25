# OpenCode checkpoint - Wrath event-scope rejection

Date: 2026-10-25
Owner: OpenCode (gameplay/integration)
Checkout: `wotlk`, base HEAD `ec8d1db0` (the item/container slice); this checkpoint's commit adds
event-scope rejection.
Scope: source-only lifecycle guard for translated Wrath gameplay replies. No build, test,
renderer, WM, socket, database or live-server execution.

## Problem

`WrathWorldView` already rejects stale **digests** by `world_generation` and `state_revision`.
Translated gameplay replies, however, travelled through the stock `NetEventSink` channel with no
scope tag. A reply parsed before a transfer could therefore be dispatched after the feed had
adopted the new world, applying an old tooltip, quest, loot or item result to the replacement
scope. The gossip-menu side channel had the same race.

## Source change

`crates/benilla-app/src/wrath.rs` only:

- Adds `WrathSessionEvent { session_epoch, world_generation, event, force }`.
- Creates a dedicated Wrath reply channel per worker start; the stock 1.12 inbound channel is no
  longer used for Wrath replies.
- Tags every translated gameplay reply with `SessionState::epoch()` and
  `SessionState::world_generation()` at parse time.
- Adds `wrath_dispatch_events`, chained after `wrath_net_drain`/`wrath_apply_world` in
  `WorldStage::Net`. It rejects an event only when its scope is **older** than the adopted view;
  a newer event is kept because its digest may still be in flight.
- Adds session/world scope to the `GossipMenu` feed event and drops it when it belongs to an
  already-retired scope.
- Marks `LoggedOut` and the worker's terminal `Disconnected` as forced lifecycle events, so scope
  invalidation cannot swallow the session edge. Non-forced events are dropped once the session is
  ended.

## Rejection rule

For the same session epoch, an event is stale when its `world_generation` is lower than
`WrathWorldView.world_generation`. A lower session epoch is stale as well. A newer epoch/world
generation is not rejected: the packet-before-digest order remains legal and the next digest
adopts it.

## Not covered

- Automatic reconnect/restart is not implemented here. The worker still ends cleanly on logout,
  and `WrathControl` stays ended. Reconnect must add a deliberate start edge and clear the
  event-scope state before accepting a new epoch.
- No rendered/live acceptance was performed. The current source is UNKNOWN at compile/test time;
  the earlier build/test reports predate this slice and the final item/adapter state.
- No item, quest, loot, movement or UI behavior is claimed to have been executed.

## Next

The reconnect lifecycle is the remaining half of this work: define the start/stop edge, reset
session/world scope and pending reply state on the new epoch, and prove no old event or entity
survives into the replacement session. That slice needs a build/test authorization before any
acceptance claim.
