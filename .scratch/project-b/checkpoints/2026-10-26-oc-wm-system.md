# OpenCode checkpoint - native WM observation and player-action system

Date: 2026-10-26
Owner: OpenCode (client/protocol/bridge integration and WM consumer/action implementation)
Base HEAD at claim (Project B): `5a6eaaa4`; WM checkout base `011a7b3`
Commits: `0a1d471b` (observation v2.2), `7a0ccc21` (typed action seam), `e435706f`
(action-matching and admission hardening); WM-side edits remain
uncommitted because `src/wm_native/` is untracked in that checkout (owner WIP) — see Provenance.
Work: `.scratch/project-b/work-packages/2026-10-26-system-goals.md` (OpenCode 1-7).

## File claim (as executed)

Project B: `crates/benilla-wm-native/src/{action.rs,lib.rs,frame.rs,client.rs,publisher.rs,error.rs,observation.rs,producer.rs}`,
`crates/benilla-protocol-wrath/src/session.rs`, `crates/benilla-app/src/wrath.rs`,
`crates/benilla-wm-driver/src/main.rs`, this checkpoint.
WM repo (untracked): `src/wm_native/{constants.py,models.py,context_adapter.py,protocol.py,receiver.py,runtime.py,actions.py,__init__.py}`.
Not touched: Astra's procgen/compatibility/architecture paths, WM's dirty tracked files, the GLM
patch helper.

## Observation path (authoritative Wrath session -> WM context)

- Schema v2.2 adds three sourced typed groups to the existing observation:
  `combat {state}` from `UNIT_FIELD_FLAGS` bit 0x0008_0000, `resources {health,max_health,power_type,power}`
  from the replicated health, `UNIT_FIELD_BYTES_0` and `UNIT_FIELD_POWER1+type`, and
  `death {state}` where the player ghost flag (0x10) precedes zero health. Unknown is preserved
  separately from observed out-of-combat, no-resource and alive states.
- `SessionState::build_snapshot` computes them from the self `ObjectEntry` with `field_delivered`,
  so an undelivered field stays `None`/`Unknown` and cannot masquerade as a zero.
- The graphical app and the renderless driver both implement the new `ProducerSnapshot`
  accessors; the existing pump/publisher/receiver/store chain carries them unchanged.
- WM validates the exact v2.2 key set, exposes the values in the native context payload, and the
  runtime health continues to report observation age, position age, source session state, health
  age, clock skew and store counters.
- Scope/ordering rules are unchanged: one latest observation per player, session epoch/world
  generation/sequence/revision fences, stale/future skew rejection, and `None` from the loader
  when there is no fresh, in-scope observation (the fallback loader still degrades to the DB
  snapshot).

## Action path (WM intent -> client owners -> authoritative result -> WM)

- Wire (protocol v2, same authenticated loopback connection): `action_request` (typed intent +
  request id + scope + deadline), `action_cancel` (admission-level), `action_result` (status,
  scope, optional observed revision, short detail). Requests are validated on both sides.
- Client sink: reads server frames while waiting for observation/health ACKs and while idle;
  queues intents (they never consume or impersonate an ACK), and writes results. The publisher
  carries an optional `ActionBridge`; without it the driver stays observation-only.
- App adapter (`wrath_wm_action_intake`, Net stage after world translation, before the command
  tap): fences intents against the adopted `WrathWorldView` player/epoch/world and `InWorld`
  phase, refuses expired/duplicate/unsupported intents, maps the permitted intents onto the
  existing `ClientCommand` owners (target, attack start/stop, cast/cancel, gossip interact, quest
  accept/complete/reward, loot open/take/money/release), answers `Accepted` at admission, and
  records the bounded in-flight set.
- Outcomes (`route_wm_action_outcomes`, before the stock handlers): cast result / spell go /
  interrupted, attack start + swing refusal, gossip reply, quest completion/failure, loot
  removed/coin/release/refusal, and session disconnect. A dispatched intent with no reply by its
  deadline is `Unknown`; a scope end or session end is `Unknown`; a cancel that arrives after
  dispatch is `Unknown` (the command may execute); a cancel before dispatch is `Cancelled`.
  Queue admission is never reported as gameplay success.
- Move start/stop is refused `Unsupported` with an explicit reason: the existing movement owner
  is the human input/camera controller and has no remote-command surface. This is a deliberate
  bounded refusal, not a silent drop.
- WM broker (`wm_native/actions.py`): bounded at 16 pending, typed argument tables per action,
  idempotency dedup with a bounded recent set, deadline clamp/expiry, cancel, and connection-loss
  handling that marks sent intents `Unknown` and never replays them. Safest retry classes are
  target selection and attack/loot-window opens (idempotent reads); cast/quest/loot-take are never
  auto-retried. The receiver owns one writer queue per connection so intents are delivered while
  the client publishes observations/health.

## Lifecycle and diagnostics

- Receiver loss: pending sent intents become `Unknown`; the observation store is invalidated;
  no reconnect or replay loop was added.
- Session end, world-generation change and terminal drain: the app marks pending actions
  `Unknown` before clearing its view; the broker expires anything still unanswered.
- Single-session/single-publisher ownership and the O1 termination rules (join before admit,
  tracked publisher retirement, panic diagnostic) are unchanged.
- Diagnostics: WM `runtime.health().actions` exposes pending capacity/states, recent final
  states and submit/refuse/send/result/duplicate/expiry/cancel/connection-loss counters; the Rust
  publisher health exposes action receive/result/error counters; context payload exposes
  observation freshness/scope. No tokens or payloads are placed in diagnostics or details.
- Cross-host settings reload: not implemented in the client. The client reads the WM token once
  at plugin build and has no settings channel; the WM repo has no settings-push/reload contract
  for the native seam. This is an external dependency (a WM-owned settings contract) rather than
  a client-only omission. Asset profile reload (`Profile::reload`) remains separate.

## Evidence levels

- Source integration: implemented and committed in Project B (observation v2.2, action seam).
- WM-side source: edited in the working tree only; the checkout does not track `src/wm_native`,
  so committing would absorb the owner's untracked receiver work (see Provenance).
- Compilation, focused tests, transport receipt, WM consumption, authoritative action acceptance
  and rendered gameplay: NOT RUN (execution restriction). All gates remain UNKNOWN.

## Prepared acceptance scenarios

1. Start the WM runtime with a token; launch the client; confirm hello protocol v2/schema v2.2
   and a fresh observation with combat/resources/death.
2. Submit `set_target`, `attack_start`, `cast`, `loot_take`, `quest_accept` through the broker;
   confirm `Accepted` then the authoritative terminal status, and that duplicate idempotency keys
   return the same request.
3. Submit `move_start`; confirm `Unsupported` with the movement-owner reason.
4. Submit with a stale epoch/world or an expired deadline; confirm `Rejected`/`Expired` and no
   command tap traffic.
5. Kill the receiver mid-intent; confirm `Unknown`, the observation invalidation and no replay.
6. Cancel before dispatch (`Cancelled`) and after dispatch (`Unknown`).
7. Disconnect/reconnect the client and verify no automatic resend of non-idempotent intents.
8. Logout to character select, re-enter, and verify old pending intents were retired and a fresh
   scope observes new combat/resources/death.

## Limitations

- Remote movement is unsupported (explicit).
- Quest accept has no direct 1.12-style acknowledgement in the seam; it resolves as `Unknown` at
  the deadline unless another observed event matches. Quest complete/choose-reward resolve on
  `QuestComplete`.
- Loot take resolves on `LootRemoved` by slot (the event carries no source guid); a slot taken by
  another looter could in principle match. The target loot source is still sent to the server.
- The action transport adds request/result frames to the existing connection; it does not add a
  second receiver, publisher, context store or execution bus.
- The renderless driver remains observation-only; receive-side action frames from WM expire at
  the broker.
- Human override is the stock input owner's precedence: WM intents are ordinary
  `ClientCommand`s and never bypass it, and an explicit remote-input latch is not added.
  Cancellation is admission-level only; no dispatch is recalled.

## Provenance

The WM checkout's `src/wm_native/` directory is untracked at `011a7b3`; the owner maintains it as
working-tree code. This session edited only the files listed above there and did not stage or
commit them, preserving the unrelated dirty tracked files and the untracked receiver. The Project
B commits contain only the Project B paths quoted above. The shared-index disclosure from the
earlier death slice remains as recorded by Astra; history was not rewritten.
