# OpenCode checkpoint - Wrath session retirement and bounded admission

Date: 2026-10-25
Owner: OpenCode (gameplay/integration)
Checkout: `wotlk`, source slice based on `b21d0d92`; this checkpoint's commit landed after the
GLM procgen commits `f0513f54` and `3563bc35`, which are not part of this slice.
Scope: source-only Wrath worker/session retirement and fresh-session admission after a clean
logout. No build, test, renderer, WM, socket, database or live-server execution.

## Owned files

Changed by this slice:

- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not changed:

- `crates/benilla-procgen` or its dirty `dungeon.rs`/`lib.rs`/`spawns.rs` work (GLM-owned)
- the stock glue/session state machines; this slice consumes their existing owners
- item/container and event-scope files beyond the lifecycle additions in `wrath.rs`

## Lifecycle behaviour

- **Clean logout:** the worker finishes, publishes its terminal feed, retires its publisher, and
  the existing `SessionEvent::LoggedOut` → `LoggedOutMessage` → `GlueSession::back_on_logout`
  edge drives `ClientState::CharSelect`. No Wrath code invents a second screen or state machine.
- **Terminal retirement:** the old worker's terminal feed is drained in `WorldStage::Net`; its
  entities, `BagCache`, `ItemLane`, self identity, outbox and view scope are cleared before a
  replacement is allowed. The epoch, world-generation and revision counters are reset so a new
  worker's producer counters cannot be mistaken for a stale old scope. The old worker's
  `JoinHandle` is only cleared after `join`; a stop that exceeds its two-second grace stays in
  `retiring` and blocks admission. Admission also waits until the old tagged reply queue is
  empty, so a terminal `LoggedOut`/`Disconnected` cannot be dropped by replacing the channel.
- **Fresh admission:** the first/new session is admitted from `ClientState::InWorld` after the
  existing lifecycle has visibly returned to character selection (`left_world`). A bounded
  admission retry may continue while the glue shows login after a failed pre-world attempt, then
  stops or returns to the normal lifecycle when the budget is exhausted. The old worker must have
  joined, `terminal_ready` must be set, and no `retiring` handle may remain. A world entry is also
  read from the view, because the entry message can still be in the Bevy queue when a terminal
  packet arrives.
- **Bounded retries:** only fresh-session admission after a clean logout gets a small budget
  (`WRATH_RESTART_ATTEMPTS = 2`, `WRATH_RESTART_DELAY = 750 ms`). Retries are polled from the
  frame loop, cleared by `stop`, and never admit a worker while another is running or retiring.
  A failure before any world entry can consume the budget; a failure after a world entry is
  treated as connection loss and does not retry.
- **Connection loss:** separate. A lost worker sets `restart_allowed = false`; it is not retried
  by this lifecycle and does not reuse a clean-logout admission. The existing session/glue
  disconnect path remains the owner of the visible lost-session edge.
- **Scope rejection:** the `WrathSessionEvent` tagging and older-scope rejection from
  `b21d0d92` remain in force. A replacement gets a fresh reply channel and feed; old events are
  not admitted through the new scope.

## Publisher retirement

`PublisherOwner` wraps the WM observation publisher for the worker. Every exit path, including
an early socket error, calls `retire_publisher` and waits up to 200 ms per retirement. Normal
logout already retires explicitly; the guard makes abnormal exits follow the same bounded stop.
A still-blocked publisher worker is not joined on the packet path.

## Remaining gaps

- The Wrath direct-login path still has no character-select/roster action wired to start a new
  Wrath worker after `ClientState::CharSelect`. The lifecycle guard is implemented and ready for
  that owner, but the graphical Wrath path currently needs a relaunch or an external `InWorld`
  transition to exercise a replacement. Wiring the existing select/pick owner to Wrath is a
  separate integration slice.
- Connection-loss recovery (including unattended reconnect policy) is deliberately not added.
- No rendered/live acceptance was performed; current compile/test status is UNKNOWN. The earlier
  build/test reports predate the item, event-scope and lifecycle slices.
- A worker that panics before publishing `SessionEnded` leaves `terminal_ready` false, so the
  guard refuses a replacement rather than risking overlap. That terminal diagnostic remains a
  gap.
- Publisher waiting is bounded (200 ms per retirement); an unresponsive publisher may outlive its
  worker, though it has been signalled closed and cannot be overlapped by a new Wrath worker.

## Verification

- `git diff --check` clean for the owned file.
- No build, test, formatter, renderer, WM, socket, database or live-server run after this slice.
- Runtime, rendered and reconnect acceptance remain UNKNOWN.
