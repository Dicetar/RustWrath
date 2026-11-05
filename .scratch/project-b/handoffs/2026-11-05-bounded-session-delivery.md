# 2026-11-05 — bounded session delivery (package B, R04)

Owner: Worker B (`session-delivery`, AutoSwarm call `call_30e443118721451195dc9342af27f344`).
Baseline: `46fd2f0c2af476b3659116f0fc8c95900bde95a0`, branch `feat/bounded-session-delivery`.
Scope: source-only. No build, test, linter, type check, client or server run was performed.

## Changed paths

- `crates/benilla-app/src/wrath.rs` — bounded reply queue, class rules, reserved terminals,
  time-budgeted drain, transfer prune, producer backpressure, diagnostics, unit cases.
- `docs/architecture/bounded-session-delivery.md` — new contract.
- `docs/architecture/architecture-traceability.md` — C09 and `Release operations` evidence, new
  "Operational delivery (R04)" section with rows R04.a/R04.b.
- `docs/architecture/architecture-review-2026-10-26.md` — additive R04 status note only; the dated
  finding and its evidence line are unchanged.
- This handoff.

Not touched, per the phase-2 constraints: `quest_trace.rs` (no `Stage` variant), `benilla-hotpatch*`,
`docs/architecture/release-plan-2026-11-05.md` (absent here), the integration checkout, and the
inventory projection/readers (`push_player_inventory_fields`, `push_item_fields`,
`apply_item_world`, bank/bag readers). The only inventory-adjacent code touched is the generic
producer arm that now calls `push_session_event`; that was the required call-site adaptation and
it changes no payload, opcode or ordering.

## Behaviour

- Producer: `push_session_event` / `push_forced_event` take a `Mutex` and append; they never block
  and never wait on the consumer. A closed queue counts pushes and discards them.
- Classes: terminal (`LoggedOut`, `Disconnected`, `LoginFailed`) in reserved slots, delivered
  first, one per kind; lifecycle and replies strict FIFO and never coalesced; observations
  (`ServerUnixTime`, `PlayedTime`, `GossipMenu{npc}`, `VendorInventory{vendor}`) replaced in place
  by key, keeping their queue position.
- Overload: coalesce → ≤50 ms cancel-aware read pause above 2048 → controlled terminal at the
  4096 cap (the worker exits through the one existing terminal path).
- Drain: floor 64 / ceiling 512 / ~2 ms; a terminal ends the batch and discards the backlog; a
  transfer prunes entries already older than the adopted scope.
- Shutdown: `stop` closes the queue before the join, so teardown pushes cannot fabricate a
  terminal; a fresh queue per worker keeps a restarted epoch from aliasing a retired worker.

## Caller/consumer audit

Producer `run_session_with_cancel` (wire loop, `send_glue`, `publish_roster`, terminal closure);
app-side panic synthesis in `wrath_session_ownership`; consumer `take_session_events` →
`wrath_dispatch_events` → `route_wm_action_outcomes` → `net::handlers::dispatch` → glue/UI
messages. `wrath_apply_world` stays exclusive, after `wrath_net_drain`, before Input, and is never
skipped. The `WrathFeedEvent` and native queues are untouched.

## Evidence

| Level | Status |
|---|---|
| Source review (original defect, every send site, drain, consumer, cancellation path) | done |
| Source change | done |
| Unit cases added (`wrath.rs::reply_queue_tests`, 10 cases) | **not run** |
| Compilation | **not run** |
| Focused check | **not run** |
| Receiver / visible gameplay | **not run** |

## Pending runtime gate

One owner-driven session on an authorized client slot with the existing
`WOW_STALL="1000,20,60,10"` injector: enter Northshire as Firna, take one quest and one loot, then
let the stall fire and, separately, force a disconnect during a stall. Accept when the client
survives the stall with no lost quest/loot reply, the terminal is delivered promptly, and the
relog/reconnect path is admitted without waiting for a reply backlog. Record source revision,
binary hash, profile, stall log lines, queue peak/dropped counters and the visible result.

## Remaining uncertainty

The capacity numbers are review choices with no measurement behind them. The producer pause can
delay a read by up to 50 ms while congested, and a pause sustained past the 60-second response
window still ends the session as lost. The stock 1.12 event channel remains unbounded and is out
of scope. Whether `LoggedOut` plus the closure's `Disconnected` is the right visible pair for
every logout path is settled at source here (both handlers do different work) but not observed at
runtime.
