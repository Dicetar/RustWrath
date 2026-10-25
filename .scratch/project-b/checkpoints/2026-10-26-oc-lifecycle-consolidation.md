# OpenCode checkpoint - lifecycle consolidation (O1 claim and delivery)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `1d6c5a50` (work package commit; `32e1a8f6` and earlier are intact)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O1.
Purpose: review the parked entry path as one ownership chain and repair only the gaps the
review demonstrates. This file is both the file claim (written before the source edits) and the
delivery record (the commit that adds it releases the claim).

## File claim

Editing:

- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: `crates/benilla-procgen/**`, `docs/procgen/**`, `.scratch/project-b/compatibility/**`
(Astra), any `benilla-wm-native` source (read only below), the stock glue/session state
machines, `tools/patch_procgen_dungeon.py`.

## Review of the chain, before edits

Chain: stock screen ask -> `CharPick` (rebound in `WrathNetPlugin::build`) -> `WrathPickQueue`
-> `wrath_pick_admission` -> `WrathControl::start` -> parked enumeration -> picked-GUID
validation -> `CMSG_PLAYER_LOGIN` -> new epoch.

Already correct in source, no rewrite made:

- One active/retiring worker: `start` refuses while a worker or a retirement handle exists; a
  bounded stop that misses its two-second window stays in `retiring` and blocks admission.
- Join-before-admit ordering: `PreUpdate` chains `wrath_session_ownership` (joins a finished
  worker, records the terminal edge) before `wrath_pick_admission`, so an ask cannot race the
  join inside one frame.
- Fresh channels: every `start` builds a fresh feed, tagged-event channel and (for parked) a
  fresh ask channel; `Auto` clears the parked channel.
- Terminal ordering: admission requires the previous terminal feed drained
  (`terminal_ready`), the tagged reply queue empty (`session_events_empty`), and no
  not-yet-joined worker, so a forced old terminal is delivered in the Net stage before a new
  session can be admitted; a replacement cannot be torn down by its predecessor's terminal.
- Stale picks and replies: the pick admission drops asks outside `CharSelect`; the parked loop
  refuses an `Enter` GUID that is not on its fresh roster with a warning and keeps serving; the
  event dispatch drops replies older than the adopted scope.
- Dead parked link: the `SessionEnded` arm refuses to arm a retry when the session served a
  screen (`park_commands` is `Some`), so the loss stays visible and no invisible redial starts.
- Character selection: the clean-logout `LoggedOut` handler still drives
  `ClientState::CharSelect`; re-enumeration only republishes the roster and never picks.
- Empty roster and rejected GUIDs leave the screen usable (empty list published; stale pick
  refused without a teardown).
- No create/delete/rename is manufactured; those asks are refused with a warning.
- Cancellation is observed from parked, socket-read and initial-enumeration states; the retry
  state holds no worker.

## Demonstrated gaps (repaired in this slice)

1. **A panicked worker produced no visible terminal edge.** `JoinHandle::join` errors only set
   `last_error`; the worker never sent its forced `Disconnected`, so the app's session state
   could stay at `InWorld` with a cleared view and `terminal_ready` stuck false. Repair: on a
   join error, synthesize the forced `SessionEvent::Disconnected { Lost }` through the same
   tagged channel the worker uses (delivered by `wrath_dispatch_events`, handled by the stock
   disconnect handler and its glue edge), clear the restart budget, and mark the terminal edge
   ready so the refused replacement is a policy decision, not a stuck flag.
2. **A publisher exceeding its 200 ms stop budget was dropped, not tracked.** The worker-local
   `PublisherOwner` waited per retirement and then dropped unfinished handles, so a replacement
   could be admitted while the old publisher was still running. Repair: unfinished
   `PublisherRetirement` handles are handed back to `WrathControl` over a per-worker channel;
   the ownership tick retains them until they report stopped, and admission requires that list
   empty. `benilla-wm-native` itself is not changed.
3. **A forced terminal still queued could land on a replacement.** `wrath_pick_admission` can
   call `WrathControl::start` without passing through the ownership system's
   `session_events_empty` gate; a clean end's forced `LoggedOut` sent after the Net stage had
   already run could therefore be delivered into the just-admitted session. Repair: `start`
   itself now refuses while the tagged reply queue is non-empty, so both admission paths share
   the ordering guarantee. A screen ask refused in that one-frame window is dropped; the
   screen's next `GetCharacterListUpdate` recovers it.

## Pending checks

No build, test, formatter, linter, graphical, socket, WM, receiver or live-server execution was
performed for this slice: the work package authorizes none, and the earlier Phase 0 grant does
not extend to it. Gate C for this commit remains unverified. Panic and publisher-stop paths
still need focused checks (a fatal worker panic, and a publisher that misses its stop window)
before they can be called runtime-verified.
