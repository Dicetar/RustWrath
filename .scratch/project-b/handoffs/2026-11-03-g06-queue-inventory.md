# G06 — queue pressure and lifecycle inventory

2026-11-03, source inspection at HEAD `236b1a7a` plus dirty work. Scope: production channels
created by `wrath.rs`, the shared command ingress it consumes from `net/io.rs`, and
`benilla-wm-native` publisher interfaces. Test-only constructors are excluded. All eight
production Crossbeam channel constructions in wrath.rs are unbounded. Native publisher
accepts three of these channels through ActionBridge rather than allocating bounded ones.
Related donor channels are included below so the mode boundary is explicit.

No stopped-consumer stress test was run. Pressure effects below are source deductions;
no throughput, maximum observed memory or latency measurements are claimed. No gameplay
channel was silently changed to lossy telemetry. Each row links an independently scoped
follow-up with its own acceptance condition.

| Follow-up / channel | Producer → consumer, drain | Lifecycle and class | Bound / stalled-consumer risk | Proposed policy |
|---|---|---|---|---|
| [G06-01](../issues/G06-01-parked-commands.md) WrathParkedCommand | park_serve/pick admission → run_session_with_cancel parked try_iter | Per parked worker; realm/roster/create/delete controls | Unbounded; stale controls accumulate if socket worker stalls | Bounded nonblocking admission with explicit refusal; coalesce only identical read-only refreshes |
| [G06-02](../issues/G06-02-session-events.md) WrathSessionEvent | Worker and terminal synthesizer → take_session_events, 64/frame | Per worker, tagged epoch/world/login; gameplay replies and terminal edges | Unbounded; drain limit bounds frame work, not storage | Ordered bounded reliable handoff; terminal capacity and controlled disconnect on overload; no arbitrary drop |
| [G06-03](../issues/G06-03-publisher-retirement.md) PublisherRetirement | Worker publisher manager → ownership tick try_iter | Per worker; handles block replacement until retired | Unbounded handles retained; must never lose ownership | Bound admission of new publishers and retain handles in one owner; do not block shutdown sender |
| [G06-04](../issues/G06-04-character-picks.md) ParkRequest / pick | CharPick, RealmChoice → wrath_pick_admission try_iter | App lifetime; UI control, generation checks | Unbounded; uncapped frame drain | Explicit bounded refusal for mutation/pick; coalescing only redundant reads, preserve order |
| [G06-05](../issues/G06-05-login-requests.md) LoginRequest | LoginSubmit → wrath_login_admission collect/replace pending | App lifetime, login generation; contains credentials | Unbounded queue; stale secrets may remain until drain | Latest-generation bounded handoff, intentional cancellation, never log payloads |
| [G06-06](../issues/G06-06-wm-requests.md) ActionRequest | publisher pump_actions → wrath_wm_admit collect | App lifetime across publisher swaps; gameplay intent | Unbounded before 16-entry in-flight check; full collect can spike frame time | Bounded admission at publisher, explicit broker refusal, per-frame budget |
| [G06-07](../issues/G06-07-wm-cancels.md) String cancellation IDs | publisher pump_actions → wrath_wm_admit collect | App lifetime; cancels before request admission | Unbounded channel and cancelled-ID retention | Bounded ID/deadline retention; overload cannot silently authorize cancelled action |
| [G06-08](../issues/G06-08-wm-results.md) ActionResult | app answer → publisher pump_actions, 32/tick | App lifetime; result evidence, connection swaps | Unbounded when publisher is absent/slow; result socket failures counted, not retried | Bounded terminal-result retention with acknowledgement/expiry; explicit loss/outcome policy |
| [G06-09](../issues/G06-09-command-ingress.md) ClientCommand | NetCommands UI/actions → wrath_tap_commands until empty, or donor writer | Shared app-lifetime ingress; ordered gameplay and motion | Unbounded upstream of 256-entry outbox; uncapped frame drain | Typed admission for all critical verbs; movement coalescing separate, bounded frame drain |
| [G06-10](../issues/G06-10-donor-events.md) donor SessionEvent | donor reader → net drain | Donor connection lifecycle; unused transport when Wrath owns session | Unbounded; mode-specific consumers must be preserved | Reliable event capacity and terminal policy in donor owner, separate from Wrath scope tags |
| [G06-11](../issues/G06-11-donor-park.md) donor ParkRequest | CharPick/RealmChoice → Parks read loop | Donor app-lifetime control; Wrath rebinds resources | Unbounded; stalled read loop accumulates controls | Same explicit admission rules as pick channel; preserve writer handoff |
| [G06-12](../issues/G06-12-donor-login.md) donor LoginRequest | LoginSubmit → Parks login loop | Donor app-lifetime credentials; Wrath rebinds resource | Unbounded; abandoned credentials retained until drain | Bounded generation-aware replacement without credential logging |
| [G06-13](../issues/G06-13-donor-writer-swap.md) WriterSwap | donor read lifecycle → writer_loop | Only allocated with connect=true; owns socket writer | Unbounded ownership transfers; cannot discard live writer casually | Bound connection admission/retirement with acknowledged writer transfer |

## Existing bounded paths and loss semantics

- `WrathLatest`: one replaceable full WorldDigest; last value wins, not an event history.
- `WrathEvents`: 16 entries, oldest dropped; lifecycle hints need scrutiny because terminal
  ordering is not interchangeable with sampled telemetry. `take_events` drains up to 64.
- Native feed: 64 entries, explicit dropped counter; presentation invalidation handles gaps.
- Outbox: 256 entries; movement pressure replaces only movement at its 64 threshold.
  Critical actions are refused at capacity, never evicted by movement. G05 exposes that
  refusal for tracked UI accepts; other action callers still need equivalent receipts.
- G05 receipt: two slots per accept, at most 16 retained receivers. Polling consumes the
  entire tiny receipt; stalled warnings do not release the guard or retry the command.
- Native observation publisher: mutex/condvar latest pending slot, worker retry slot and
  producer pump retention. Latest-only replacement is appropriate for full observations,
  not action results. The result drain's 32/tick limit does not bound channel memory.

## Admission rules for follow-ups

Choose a capacity only with a workload assumption and an authorized stopped-consumer case.
Do not replace `unbounded()` with blocking `bounded().send()` on the render thread or while
holding a lock. Reliable gameplay overflow must produce explicit refusal/disconnect, keep
terminal evidence and preserve order. Observation/telemetry may coalesce or drop only with
gap counters. Credentials and chat bodies must not enter diagnostics. No numeric memory
budget is adopted here. Follow-ups are proposed and unassigned, not extra work in this batch.
