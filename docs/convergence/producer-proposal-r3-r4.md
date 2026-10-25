# Producer proposal — observation validity, provenance, and health (review R3/R4)

Author: GLM. Date: 2026-10-25. Status: **PROPOSAL for consumer agreement — the wire is not
changed by this document.** Target schema amendment: `wm.native.observation.v2.1` (or v3), only
after OpenCode implements the receiver side and both contracts are re-published.

Answers the review's R3 (aggregate freshness ≠ field freshness/stream health) and R4 (receiver
outcome unobserved after accepted handoff), per work package 01 Phase B.

## 1. The three concepts, separated

| Concept | Definition | Owner today |
|---|---|---|
| **Semantic state revision** | Increments when source-observed state changes (values, knowledge transitions, movement, visibility loss, world scope). Unchanged re-exports keep it. | Producer (`state_revision`) — implemented |
| **Provenance/validity per group** | When each observation group was last actually observed, and whether it is known at all. | Producer-side truth; wire carries only position's today |
| **Liveness/health** | Whether the streams are alive: client↔server session, client↔receiver transport, receiver process, WM read. Four separate signals, four separate counters. | Driver + publisher + receiver (split; see §4) |

The producer's aggregate `observed_at_unix_ms` stays exactly what it is: the arrival time of the
packet that produced the current `state_revision`. It is the stream-freshness input for the
receiver and nothing else.

## 2. Validity policy (the recommended direction)

**Replicated values remain explicitly known until invalidated by their own lifecycle. Source
timestamps are their actual observation times. Health/lease renewal is separate from
reobserving fields.**

Groups, their provenance, and their invalidation:

| Group | Provenance | Valid until |
|---|---|---|
| identity (`session_epoch`, `player_guid`, `world_generation`) | session/epoch/world lifecycle | invalidated only by epoch or world-scope change |
| position (+orientation) | arrival time of the packet that last changed the self position (`position_observed_at_unix_ms`, already produced) | invalidated by world transfer; otherwise valid-but-aging |
| target | delivery knowledge of `UNIT_FIELD_TARGET` (both words, atomic — R1) | invalidated by world transfer; recomputed on every target delivery |
| visibility | the replicated set at export time, `complete`/`total` explicit | a snapshot property, never a lease |

Observable behavior (what WM can rely on):

1. An idle player on a healthy session keeps a **valid but aging** position: `state_revision` and
   `observed_at_unix_ms` do not move, `position_observed_at_unix_ms` does not move, and the bridge
   does **not** manufacture freshness. If the receiver's age policy expires it, WM falls back —
   per the agreed expiry policy, not because anything became invalid at the source.
2. Nearby NPC activity never refreshes the player group: unrelated changes already advance
   `state_revision` without touching `position_observed_at_unix_ms` (enforced and tested).
3. A world transfer invalidates position, target, and visibility at the source (`WorldTransfer`
   lifecycle event; the producer drops the old world's pending publication). WM must not act on
   pre-transfer coordinates.
4. **WM action eligibility** is a policy over: identity valid AND world scope current AND the
   action class's required group ages within that class's bound (movement-class and cast-class
   bounds are WM's to set; the producer supplies the timestamps). A bridge heartbeat — or any
   transport liveness signal — is never eligibility evidence.

## 3. World/session identity

Implemented producer-side this package: `world_generation` increments at session start and at
every `SMSG_NEW_WORLD` (far teleport **and** same-map instance transfer — map id alone is not
world identity), with `LifeEvent::WorldTransfer` driving publisher retirement.

**Proposed wire addition (v2.1):** `world_generation: u64` per observation. Receiver rule: a
changed world generation invalidates the previous snapshot's position/target/visibility for action
purposes while identity remains scoped by `session_epoch`. Until the field exists, the consumer
approximates scope by `map_id` changes — which misses same-map instance transfers; flagged as a
known approximation, not a correct behavior.

## 4. Health and the four delivery counters (R4)

Four counters, four owners, never merged:

1. **handoff** — producer→publisher acceptance (publisher `PublishOutcome::HandedOff`; producer
   acknowledges only this). Driver-owned.
2. **socket write** — the publisher worker's `LoopbackSink::send` result. Publisher-owned.
3. **receiver acceptance** — the receiver validated and stored the frame. Requires a receiver→
   sender acknowledgment channel: **proposal** — the receiver replies with a bounded
   `{"type":"ack","sequence":N,"state_revision":R}` frame per accepted observation (or per N
   accepted frames); the publisher worker reads it and exposes `last_accepted` in a health query.
   Latest-only semantics are preserved; the ack is health, not delivery guarantee.
4. **WM availability** — `latest_for(player)` returning a live observation. Receiver/WM-owned.

Recovery policy (the small explicit protocol R4 asks for):

- On publisher (re)connect after a drop, the **producer performs one resync re-export** of the
  current known state: unchanged `state_revision` and `observed_at`, fresh `sequence` — an
  unchanged re-export the contract already permits, triggered exactly once per connection instead
  of continuously. Requires the publisher to expose "this is the first handoff after a (re)connect"
  (a receipt flag or a publisher event — OpenCode's side). Without it, a quiet world after a
  receiver restart stays dark until the next source change; that is the gap this closes.
- Retained (busy) snapshots already expire by coalescing; add a bounded retry age (proposal: 60 s)
  after which a superseded-generation snapshot is dropped rather than retried.
- Client↔server session health is separate and driver-owned: connection state (the new session
  `Phase` machine) + last server packet arrival. WM reads it from the driver/session diagnostics,
  never from the observation bridge.

## 5. Clock policy

- Receiver age/deadline math uses its own monotonic clock; wall-clock (`observed_at_unix_ms`) is
  for cross-process correlation and skew checks only.
- Producer wall-clock discontinuity: the driver stamps arrival times from `SystemTime`. A
  backward jump cannot reorder producer state (ordering keys are counters, not time), and the
  receiver's future-skew check will reject the affected frame rather than corrupt state. Documented
  behavior; no synthetic time correction.

## 6. What each side implements next

- **GLM (done this package, producer-side):** knowledge transitions (R1), world scope + transfer
  lifecycle (R2), `world_generation` + position provenance in `SelfObservation` (wire-ready), the
  `Phase` state machine, unchanged-re-export freshness guarantees.
- **OpenCode:** receiver per-group freshness policy (or the v2.1 fields), the ack channel +
  `last_accepted` health, reconnect-resync trigger from the publisher, R7 receiver health
  reporting.
- **Joint:** agree the v2.1 field set, then both contracts re-publish with new hashes and the wire
  changes land in one coordinated pair.
