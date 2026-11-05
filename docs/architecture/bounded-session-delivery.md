# Bounded session delivery and terminal recovery (R04)

2026-11-05. Narrow implementation contract for the Wrath session reply path, not approval of
ADR-0001 and not a claim of runtime acceptance. Source reviewed and changed at baseline
`46fd2f0c2af476b3659116f0fc8c95900bde95a0`; no build, test, client or server run was performed.

## Problem

`WrathControl::start` created the translated-reply channel with
`crossbeam_channel::unbounded()` (`wrath.rs:1009` before this change). The worker appends a
`WrathSessionEvent` for every translated gameplay reply and for every lifecycle edge; the app
drains at most `WRATH_DRAIN_LIMIT` (64) per frame in `wrath_dispatch_events`. Nothing bounded the
queue, so a stalled main loop (an occluded window, a debugger stop, an asset hitch; the repo's
`WOW_STALL` probe reproduces it deterministically) let it grow without limit. Because delivery was
strict FIFO, the worker's terminal edge also sat behind whatever backlog existed, and
`session_events_empty()` gates both `start` and the login/pick admission, so a deep backlog
delayed the visible session end and the next session as well. This is review finding R04; the
2026-11-05 release plan raises it to P1.

The naive fix — `bounded(n)` plus blocking `send` — was rejected: a full queue would stop the
packet loop (including `CMSG_PING`), so a healthy link would eventually fail the app's own
60-second response deadline, and `stop`'s two-second join cannot release a producer blocked in
`send` until the receiver is dropped after the join.

## Ownership and boundaries

`run_session_with_cancel` remains the only producer. `wrath_apply_world` remains the only world
writer and is still chained after `wrath_net_drain` and before Input; this change never makes it
conditional and never skips it on overload. `wrath_dispatch_events` remains the only consumer
entry point, and `net::handlers::dispatch` still receives the batch in delivery order. The stock
1.12 `NetEventSink` channel is untouched (it is not used on the Wrath path). The separate
`WrathFeedEvent` lifecycle queue and the native cast/item-text queue keep their own bounds and
consumers; `WrathItemText` never travels this queue.

## Classes

| Class | Events | Rule |
|---|---|---|
| Terminal | `LoggedOut`, `Disconnected`, `LoginFailed` | Reserved slots, never evicted or coalesced, delivered before the FIFO, and the batch ends there. One edge per kind: a repeated terminal is dropped. |
| Lifecycle | `LoginStage`, `RealmList`, `RealmSplit`, `LoginQueued`, `WorldAuthenticated`, `AddonInfo`, `CharacterList`, `RosterFailed`, `CharActionResult`, `CharRenamed`, `CharacterLoginFailed`, `LogoutResponse`, `LogoutCancelled`, `Connected`, `Worldport`, `TransferPending`, `TransferAborted` | Strict FIFO, never coalesced. One-shot edges that no later event can reconstruct. |
| Reply | everything else the translator emits (quest, loot, vendor, item push, inventory failure, trainer, spell/cast/cooldown, attack/damage/aura, chat, corpse/summon/binder/bank-slot, spline moves, notifications) | Strict FIFO, never coalesced. Each mutates an owning model or is a user-visible one-shot. |
| Observation | `ServerUnixTime`, `PlayedTime`, `GossipMenu{npc}`, `VendorInventory{vendor}` | Absolute snapshots, coalesced **in place** by key. The entry keeps its position, so reply order is unchanged; the newest value wins. |

A clean logout legitimately emits **two** terminal edges: `LoggedOut` from the packet path, then
`Disconnected { end: LoggedOut }` from the worker's exit closure. They are not duplicates —
`net::session::logged_out` writes `LoggedOutMessage` and despawns the avatar, while
`net::session::disconnected` performs the world teardown, clears the name/item/cooldown caches and
writes `DisconnectedMessage`. Both are delivered, in that order; only a *repeated* edge of the
same kind is dropped. `force` is not the terminal predicate: `send_glue` marks every login edge
forced, and `force` keeps its existing meaning (delivered even when stale or ended).

## Capacity

Named constants in `wrath.rs`; all are reviewed design choices, not measurements.

| Constant | Value | Meaning |
|---|---|---|
| `WRATH_SESSION_REPLY_CAP` | 4096 | Hard cap on the ordered FIFO (lifecycle + replies). |
| `WRATH_SESSION_REPLY_HIGH_WATER` | 2048 | Producer pauses reading above this depth. |
| `WRATH_SESSION_REPLY_LOW_WATER` | 1024 | Producer resumes at or below this depth (latching, so it does not flap). |
| `WRATH_SESSION_TERMINAL_CAP` | 2 | Reserved terminal slots, outside the FIFO cap. |
| `WRATH_DRAIN_CEILING` | 512 | Largest single drain pass when the queue is deep. |
| `WRATH_DRAIN_BUDGET` | 2 ms | Wall budget for one drain pass. |
| `WRATH_REPLY_BACKPRESSURE_SLICE` | 50 ms | Longest single producer pause, always between complete packets. |

Observation coalescing needs no capacity constant: at most four keys can be present at once.

## Overload ladder

1. **Coalesce** observations in place (no loss, no growth).
2. **Backpressure**: above high water the worker stops calling `world.recv()` in ≤50 ms slices,
   re-checking `cancel` and the response deadline each slice. The pause always begins between
   complete packets, so the framing contract (`read_exact` straddling a packet is fatal) is never
   entered; pings and outbound commands keep flowing.
3. **Controlled terminal**: at the hard cap the worker exits with a distinct bounded-backlog
   error, which runs the one existing terminal path (`SessionEnded` plus the forced
   `Disconnected`). The terminal uses the reserved slots, so it fits even at the cap. Ordered
   replies are never silently discarded while the session continues.

## Drain and retirement

`take` returns terminals first and discards the FIFO in the same call: a terminal is a session
barrier, and replies queued behind it belong to a finished session. Otherwise it drains
`max(64, …)` up to 512 entries within the 2 ms budget. On `WorldTransfer`, entries already
strictly older than the adopted scope are pruned (forced edges excepted — they keep their
"deliver even when stale" contract); this only removes work the dispatcher would reject.
`scope_is_older`, `force` and `login_generation` filtering are unchanged, so no stale state can
reach a new world through this queue. `stop` closes the queue before the join: later pushes are
counted, not delivered, and cannot fabricate an overflow terminal. A fresh queue per worker is
what keeps a replacement worker's restarted epoch from aliasing a retired worker's events.

## Evidence

| Evidence | Meaning |
|---|---|
| Source review | The unbounded queue, per-frame drain, and admission gates above |
| Source change | Non-blocking bounded queue, class rules, terminal slots, drain budget, prune |
| Unit cases | Added in `wrath.rs::reply_queue_tests`; **not run** |
| Compilation | **Not run** |
| Receiver / visible gameplay | **Not run** |

## Remaining gaps

- No measurement of drain cost, peak depth, or the caps against real traffic; the numbers are
  review choices and may need revision once a run exists.
- The producer pause can delay a packet read by up to 50 ms while congested. A sustained pause
  beyond the 60-second response window still ends the session as lost, which is intended.
- A worker blocked in the 10-second socket read remains in `retiring` after `AppExit`; that is
  pre-existing and unchanged.
- The stock 1.12 event channel (`net/io.rs`, `net/apply.rs`) is also unbounded and drains without
  a per-frame limit; it is out of scope for this package.
