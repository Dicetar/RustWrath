# GLM producer contract — native session-state snapshots for the WM observation bridge

Updated: 2026-10-25. Status: **PRODUCER IMPLEMENTED AND LIVE-VERIFIED; BRIDGE INTEGRATED AND
LIVE-EXERCISED (no receiver); RECEIVER AND WM CONTEXT NOT EXERCISED.** Consumer contract:
`D:/Projects/project-b-opencode/docs/opencode/convergence-consumer.md` (schema
`wm.native.observation.v2`).

## Source checkpoint (SHA-256, working tree at this writing — re-read before wiring)

```text
D:/Projects/project-b-client, branch wotlk
crates/benilla-protocol-wrath/src/session.rs        88fc1392d5c2989c6689685bd492ebf8cb6c8d15bb9f964dc31d457350a71f49
crates/benilla-wm-driver/src/main.rs              cc9ce7a583640e57382aef43d55672acae489868cdbb33642420a4e17674e2b1
(crates/benilla-protocol-wrath/src/session.rs stays as hashed above)

Integrated bridge crate (copied from OpenCode's checkout into crates/benilla-wm-native):
crates/benilla-wm-native/src/lib.rs                 99e30d1c15acbf9217b6547a436062a0d68bfc0b2c2b32e1ef03b2813312aec0
crates/benilla-wm-native/src/observation.rs         464bcb93270cfb3ba44ebcfde4779acc943fecc615e5059bc161d93fa5193f3b
crates/benilla-wm-native/src/publisher.rs           589ba7195e094d3731f7fda2bef9958c0a0baa50f77f34c95d6f03b5406419e3
crates/benilla-wm-native/src/pump.rs                500145af5ed21e533597e4fdf4f15b71a8cbf442c4a0a5a30738b5f2d9c3aad8
crates/benilla-wm-native/src/producer.rs            65f171a93978e80db1d8b9e7924d86b4d0c8869e9217fcedec3d89e570d4ff05
```

**Checkpoint drift notice for OpenCode:** the hashes published in
`convergence-consumer.md` (lib `e546fdd6…`, observation `f2b9953f…`, publisher `133ed4bb…`,
producer `1bcbeec2…`) no longer match OpenCode's own current tree, and `pump.rs` is absent from
that doc's hash list. This contract pins the tree as integrated here; OpenCode must re-publish a
fresh checkpoint before relying on the doc's hashes. Only `benilla-wm-native` was copied — none of
OpenCode's combat, F08, or configuration work.

## Exact public API (producer)

```rust
pub const MAX_VISIBLE_GUIDS: usize = 256;

pub enum SkipReason { NoPlayer, NoPosition }     // Display + std::error::Error
pub enum SelfTarget { Unknown, None, Target(u64) }
pub struct SelfVisibility { pub complete: bool, pub total_guid_count: u64, pub guids: Vec<u64> }
pub struct PositionSnapshot { pub x: f32, pub y: f32, pub z: f32, pub o: f32 }

pub struct SelfObservation {
    pub session_epoch: u64,
    pub player_guid: u64,
    pub sequence: u64,                    // export counter; advances on every Some export
    pub state_revision: u64,              // source-change counter; unchanged re-export keeps it
    pub observed_at_unix_ms: i64,         // arrival time of the packet that produced the revision
    pub client_uptime_ms: u64,
    pub map_id: u32,
    pub zone_id: Option<u32>,             // always None today; never guessed
    pub area_id: Option<u32>,             // always None today; never guessed
    pub position: PositionSnapshot,
    pub position_observed_at_unix_ms: i64, // diagnostic provenance (proposed for schema v2.1)
    pub target: SelfTarget,
    pub visibility: SelfVisibility,
}

pub enum LifeEvent { MapTransition { from: u32, to: u32 } }

impl SessionState {
    pub fn new() -> Self;
    pub fn epoch(&self) -> u64;
    pub fn start_session(&mut self, player_guid: u64);   // bumps epoch; resets every scope
    pub fn finish_session(&mut self);                    // logout/disconnect: drops state
    pub fn reduce(
        &mut self, op: u16, payload: &[u8],
        observed_at_unix_ms: i64,                        // driver-supplied packet arrival time
    ) -> anyhow::Result<Vec<ChangeEvent>>;
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent>;
    pub fn export_due(&self) -> bool;
    pub fn export(
        &mut self,
        exported_at_unix_ms: i64,                        // driver-supplied handoff time
        client_uptime_ms: u64,                           // driver-supplied monotonic uptime
    ) -> anyhow::Result<Option<SelfObservation>>;
    pub fn mark_published(&mut self);                    // acknowledges the preceding export
    pub fn unsent(&self) -> Option<&SelfObservation>;
}
```

## Semantics (unchanged from the v2 design, now pump-integrated)

- **Revision/freshness separation:** `state_revision` increments only on observation-relevant
  change (new field values — identical rewrites are detected and ignored; movement
  position/flags/speeds changes — a periodic heartbeat is a clock tick, not state; creates;
  visibility loss; map establishment/transition). `observed_at_unix_ms` is the arrival time of the
  packet that produced the revision and never moves on unchanged re-exports. Unrelated packets
  (pong, chat, addon info) advance nothing.
- **Target:** delivered zero → `None`; delivered nonzero → `Target(guid)`; undelivered → `Unknown`
  (exported explicitly — an otherwise-usable observation is not suppressed).
- **Visibility:** sorted/unique/excludes-player; over the 256 bound the export proceeds with 256
  entries and `complete: false` plus the pre-bound total — explicit incompleteness, never silent
  truncation, never a skip.
- **Skip conditions (the only remaining fabrication guards):** `NoPlayer`, `NoPosition`. Both are
  retryable; the un-handed-off revision is retained.
- **Zone/area:** `None` until an AreaTable-backed source exists.

## Bridge integration (this checkout, live-exercised)

Boundary choice: the application layer is a thin driver crate, `crates/benilla-wm-driver` — the
only place protocol, producer, and bridge meet. `benilla-protocol-wrath` has **no** bridge, socket,
WM, or serde dependency (verified: its manifest carries no bridge path). The integrated
`benilla-wm-native` crate provides `ProducerSession` + `pump_once` (`src/pump.rs`); the driver
implements `ProducerSession` on an adapter over `SessionState` (`SnapshotOwned` implements the
bridge's `ProducerSnapshot` trait), and one `pump_once` call per driver tick — including idle
ticks — performs the bounded retry/export/handoff step with the ordering invariant enforced
bridge-side. OpenCode's consumer doc proposed exactly this shape (`WmSession`/`WmProducerSnapshot`);
the implementation matches it, named `ProducerAdapter`/`SnapshotOwned`.

```rust
// per tick (dwell loop, after packets are drained):
match pump_once(publisher, &mut adapter, unix_now_ms(), uptime_ms) {
    Ok(PumpResult::Idle) => {}                     // fully handed off, nothing changed
    Ok(PumpResult::HandedOff(receipt)) => {        // bridge accepted the snapshot
        exports += 1;
        println!("[wm] handed off seq {} rev {}", receipt.sequence, receipt.state_revision);
    }
    Ok(PumpResult::Retained) => {}                 // busy handoff: producer retains; next tick re-exports
    Err(PumpError::Producer(e)) => { /* unavailable value; logged, retried on state change */ }
    Err(PumpError::Conversion(e)) => { /* snapshot -> observation conversion failure */ }
    Err(PumpError::Superseded { .. }) => { /* ordering violation under a single pump: error */ }
    Err(PumpError::Publisher(Error::Disconnected)) => { /* retire + spawn_after respawn */ }
    Err(PumpError::Publisher(e)) => { /* log */ }
}
```

Acknowledgment rule: only the snapshot the bridge actually accepted (`HandedOff`) is
`mark_published()`ed — and the pump calls it internally for exactly the preceding export. A
`Retained` outcome acknowledges nothing: the producer's own retention (its `unsent` snapshot,
original source timestamp and revision preserved) is the single retry owner; the next tick
re-exports it with a fresh `sequence` and export time. A `Superseded` outcome under one pump is an
ordering-invariant violation and is reported as an error, never silently acknowledged.

### Lifecycle: retire / spawn_after (replacement ordering)

- Map transition (`LifeEvent::MapTransition`) and publisher `Disconnected`: the current publisher
  is `retire()`d (signal without join) and the replacement is spawned with
  `ObservationPublisher::spawn_after(config, &retirement)`, whose worker waits until the old
  worker has fully stopped and closed its socket. Nothing on the packet path blocks on socket I/O.
- Logout (`LOGOUT_COMPLETE`): `finish_session()` + explicit `retire()` with a bounded
  `wait_timeout` off the packet path.
- **No immediate-receiver-invalidation claim:** retirement orders the client side; when the
  receiver observes the resulting disconnect is the receiver's own connection handling (see
  OpenCode's contract). This document makes no claim about receiver-side timing.
- Enablement: the bridge exists only when the owner sets `WM_NATIVE_OBSERVATION_TOKEN`; disabled,
  there is no publisher, no socket, and no traffic — baseline login is untouched (verified live).

## Evidence status (separate claims)

| Claim | Status |
|---|---|
| Producer working | **YES** — 11 session tests + live exports (source position, unknown target, incomplete visibility at bound, revision/freshness semantics) |
| Bridge wired | **YES (source-integrated + compiled + live handoffs)** — crate integrated at the hashes above, `pump_once` live-verified with `HanedOff` receipts and background retry while no receiver listened; bridge-disabled baseline login verified clean |
| Receiver exercised | **NO** — OpenCode's Python receiver was not started; no handshake, no store, no freshness rejection has been observed |
| WM context integrated | **NO** — no receiver startup in the WM service, no loader injection into `wm.context.builder`; still a standalone consumer seam |

Until a receiver run exists, this chain is **not** complete end to end.

## Test status

`benilla-protocol-wrath`: 28 tests green (15 unit incl. published RC4 KATs; 11 session-producer
v2 tests; 2 live-fixture replay). `benilla-wm-native`: compiles clean, ships no tests of its own.
Live: renderless session with the bridge enabled (token set, receiver absent) — snapshots handed
off, worker background-retrying, session healthy, clean logout; and with the bridge disabled —
baseline login unaffected.
