# Native WM convergence consumer contract (v2 — historical)

> **Superseded for the wire/health contract:** see `docs/opencode/bridge-health-contract-v2.1.md`.
> This file remains the v2 API/transport history and the producer/consumer ownership rationale.

Updated: 2026-10-25. Status: **SUPERSEDED BY v2.1, UNCOMPILED, RUNTIME-UNVERIFIED**. This is the
stable producer/consumer contract for GLM's native session-state producer and OpenCode's existing
WM observation bridge.

This document supersedes `docs/opencode/wm-native-bridge.md` for the API and wire contract. The
bridge-side conversion trait/function is implemented against GLM's actual source semantics; GLM's
published producer note and application-side trait impl remain outstanding. No live session data
was fabricated, no UPDATE_OBJECT parser was duplicated, and no production WM file was edited.

## What changed from schema v1

Schema `wm.native.observation.v1` could not express the required convergence semantics. It is
replaced by `wm.native.observation.v2` on both sides; v1 observations are rejected.

1. `target_guid: Option<u64>` collapsed "not observed" and "observed no target". v2 uses an explicit
   tagged `target` object with `unknown`, `none`, and `target` states.
2. `visible_guids` alone could not say whether the list was complete. v2 adds explicit
   `visibility.complete` and the pre-bound `visibility.total_guid_count`; over-bound exports are
   rejected before construction and must be explicitly marked incomplete.
3. A freshly stamped `observed_at_unix_ms` could renew unchanged state. v2 adds
   `state_revision` and `exported_at_unix_ms`, and both Rust and Python reject an unchanged revision
   with a new source timestamp.
4. `session_epoch` is now explicitly nonzero and transport/process lifecycle rules are defined in
   both implementations.

## Source provenance

- OpenCode bridge checkpoint: uncommitted working tree in
  `D:/Projects/project-b-opencode`. The hashes below identify this checkpoint; they are not a commit.
- GLM producer inspected read-only at `D:/Projects/project-b-client`, now clean at `6ae14b3b`
  following the v2 producer commit `71aefc2e` and cleanup `6ae14b3b`.
- `docs/convergence/glm-producer.md` is published and was read with the current `session.rs`. The
  bridge-side conversion and producer pump are implemented from that actual API; only the small
  local newtype impl in GLM's application crate and WM startup glue remain owner-applied.
- Production WM was inspected read-only at
  `D:/WOW/wm-project/src/wm/context/{builder,pack}.py` and
  `D:/WOW/wm-project/src/wm/autoplay/world_context.py`.
- No source was copied from GLM's checkout, and no C++/WM/source-owner file was modified.

### OpenCode checkpoint SHA-256

```text
crates/benilla-wm-native/Cargo.toml                              c01302382f9aecb0ad9af6d60bed1f0fa662fc172abcb085697500136fcfa74f
crates/benilla-wm-native/src/lib.rs                              99e30d1c15acbf9217b6547a436062a0d68bfc0b2c2b32e1ef03b2813312aec0
crates/benilla-wm-native/src/auth.rs                             fd17a2c7345dbd9add52bc5fda88e41cbcf2c9ca069e654417185f806cd8619a
crates/benilla-wm-native/src/client.rs                           b80eb012d4a8df503d78993567c1e90da5e4ee7a10e7e5b2707ef935c832bf8c
crates/benilla-wm-native/src/error.rs                            8d539fd42aba38679cd324f54783d19b1b76c281db08d161d9e2ae5f277f7d7b
crates/benilla-wm-native/src/frame.rs                            2a7e879b663aa4a7f077d061394b2def9bf7fb7ea44f557b41ac3fbb9e794519
crates/benilla-wm-native/src/observation.rs                      464bcb93270cfb3ba44ebcfde4779acc943fecc615e5059bc161d93fa5193f3b
crates/benilla-wm-native/src/producer.rs                         65f171a93978e80db1d8b9e7924d86b4d0c8869e9217fcedec3d89e570d4ff05
crates/benilla-wm-native/src/publisher.rs                        589ba7195e094d3731f7fda2bef9958c0a0baa50f77f34c95d6f03b5406419e3
crates/benilla-wm-native/src/pump.rs                             500145af5ed21e533597e4fdf4f15b71a8cbf442c4a0a5a30738b5f2d9c3aad8
integrations/wm-native/src/wm_native/constants.py                e64232226ba59dc232b5e4bd5cf9940469b4f248262e9e8a91a2af34371ce8f8
integrations/wm-native/src/wm_native/models.py                   0ccdde61345a57dd830c61b1f0a9bfaf2311906b19acb8d7ee09ab150ff764ca
integrations/wm-native/src/wm_native/protocol.py                 abd63c1f6527bd71d84ebf5974d5828e9197a820e9ead56741d138b865d1cd46
integrations/wm-native/src/wm_native/store.py                    7a71b019eda467c29607865166438e2738e13e5447ab8f2b57335b457434d164
integrations/wm-native/src/wm_native/receiver.py                 25da127567b391608602406391e1934cadc98da4419444e7380ff883f7d580d0
integrations/wm-native/src/wm_native/context_adapter.py          dac78aef49933cd9254d2fc43a6c4fed8f8f0c9ee5fe2f92445bfd67c1a4b9c0
integrations/wm-native/src/wm_native/__init__.py                 2dca5b4ca02241649613c0e27327b101d2c091e376b8a6bb2496d3ff6b9df771
integrations/wm-native/src/wm_native/__main__.py                 5c18c98f8064b7664346cab46ce63c36d2b5fe40c3f7f65e649cf8643b65d494
integrations/wm-native/src/wm_native/errors.py                   f7509d9c8fca51a877b5a6e9acc320923457fb90376e283bc363ea882ca68da4
integrations/wm-native/src/wm_native/runtime.py                  72c122e35009372f0508b8bd00d5ddd246c91587c6b71d4ee8ba061818f24984
```

GLM source identity inspected read-only (clean):

```text
D:/Projects/project-b-client @ 6ae14b3b (71aefc2e v2 producer; 6ae14b3b cleanup)
crates/benilla-protocol-wrath/src/session.rs          8e7bc567acb22d6b2e5dfe63a8e7bd56e7c8bddfee61335140a1699ee5a92c67
crates/benilla-protocol-wrath/src/reducer.rs          dca959f8f7a9d73f92a0651c1b68008c61e543d2c579713bacb632c237686918
crates/benilla-protocol-wrath/src/updates.rs          2724473ff166d23cdab7370040e0510b7613207bb28e14a21e5821c367b4442d
crates/benilla-protocol-wrath/src/fields.rs           d3e08e7049134d3ca7f35e1b1cca718d5ac202ed7ab0a72119c486dd79d91de9
crates/benilla-protocol-wrath/src/bin/wrath-world.rs  5491203c42f48fb1a4cb9179766ceae47cc95c23db30c484bf5f4e1f62256317
```

These hashes identify the clean producer checkpoint. Re-read them after any further edit; the
adapter consumes semantics, not a copied directory.

## Public Rust API at this checkpoint

Crate: `benilla-wm-native`. No dependency on GLM, WM, Bevy, or a packet decoder; only `serde` and
`serde_json`, both already used by the workspace. No root manifest or lockfile edit was made.

### Types

```rust
pub const OBSERVATION_SCHEMA_VERSION: &str = "wm.native.observation.v2";
pub const MAX_VISIBLE_GUIDS: usize = 256;
pub const PROTOCOL_NAME: &str = "benilla.wm_native";
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const DEFAULT_PORT: u16 = 48_765;

pub struct Position { pub x: f32, pub y: f32, pub z: f32, pub orientation: f32 }

pub enum TargetObservation {
    Unknown,
    None,
    Target { guid: u64 },
}

pub struct Visibility {
    pub complete: bool,
    pub total_guid_count: u64,
    pub guids: Vec<u64>,
}

pub struct Observation {
    pub session_epoch: u64,
    pub player_guid: u64,
    pub sequence: u64,
    pub state_revision: u64,
    pub observed_at_unix_ms: i64,
    pub exported_at_unix_ms: i64,
    pub client_uptime_ms: u64,
    pub map_id: u32,
    pub zone_id: Option<u32>,
    pub area_id: Option<u32>,
    pub position: Position,
    pub target: TargetObservation,
    pub visibility: Visibility,
}
```

`schema_version` is private inside `Observation` and is set only by `Observation::new`; producers
must not invent it.

### Constructors and methods

```rust
impl Position {
    pub fn new(x: f32, y: f32, z: f32, orientation: f32) -> Result<Self>;
    pub fn validate(&self) -> Result<()>;
}

impl TargetObservation {
    pub fn target(guid: u64) -> Result<Self>; // zero is rejected
    pub fn validate(&self) -> Result<()>;
}

impl Visibility {
    pub fn new(complete: bool, total_guid_count: u64, guids: Vec<u64>) -> Result<Self>;
    pub fn validate(&self) -> Result<()>;
}

impl Observation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_epoch: u64,
        player_guid: u64,
        sequence: u64,
        state_revision: u64,
        observed_at_unix_ms: i64,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
        map_id: u32,
        zone_id: Option<u32>,
        area_id: Option<u32>,
        position: Position,
        target: TargetObservation,
        visibility: Visibility,
    ) -> Result<Self>;

    pub fn schema_version(&self) -> &str;
    pub fn validate(&self) -> Result<()>;
    pub fn same_state(&self, other: &Self) -> bool;
    pub fn validate_after(&self, previous: &Self) -> Result<()>;
}

impl AuthToken {
    pub fn new(token: impl Into<String>) -> Result<Self>;
}

impl LoopbackConfig {
    pub fn new(addr: SocketAddr, token: AuthToken) -> Result<Self>;
    pub fn with_timeouts(
        addr: SocketAddr,
        token: AuthToken,
        connect_timeout: Duration,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Result<Self>;
}

impl PublisherConfig {
    pub fn new(loopback: LoopbackConfig) -> Self;
    pub fn with_reconnect_backoff(
        loopback: LoopbackConfig,
        reconnect_initial_delay: Duration,
        reconnect_max_delay: Duration,
    ) -> Result<Self>;
}

impl ObservationPublisher {
    pub fn spawn(config: PublisherConfig) -> Result<Self>;
    pub fn spawn_after(
        config: PublisherConfig,
        previous: &PublisherRetirement,
    ) -> Result<Self>;
    pub fn publish(&self, observation: Observation) -> Result<PublishOutcome>;
    pub fn retry(&self, retained: RetainedObservation) -> Result<PublishOutcome>;
    pub fn retire(self) -> PublisherRetirement;
    pub fn close(self); // blocking lifecycle path only
}

pub enum PublishOutcome {
    HandedOff(PublishReceipt),
    Retained(RetainedObservation),
    Superseded { sequence: u64, latest_sequence: u64 },
}

pub struct RetainedObservation { /* exact observation + retry_at */ }

impl RetainedObservation {
    pub fn reason(&self) -> RetainReason;
    pub fn observation(&self) -> &Observation;
    pub fn sequence(&self) -> u64;
    pub fn state_revision(&self) -> u64;
    pub fn observed_at_unix_ms(&self) -> i64;
    pub fn retry_at(&self) -> Instant;
    pub fn into_observation(self) -> Observation;
}

pub struct PublisherRetirement { /* old worker completion handle */ }

impl PublisherRetirement {
    pub fn is_finished(&self) -> bool;
    pub fn wait_timeout(&self, timeout: Duration) -> bool;
    pub fn join(self); // blocking lifecycle path only
}

pub struct PublishReceipt {
    pub sequence: u64,
    pub state_revision: u64,
    pub coalesced: bool,
}

pub trait ProducerSnapshot {
    fn session_epoch(&self) -> u64;
    fn player_guid(&self) -> u64;
    fn sequence(&self) -> u64;
    fn state_revision(&self) -> u64;
    fn observed_at_unix_ms(&self) -> i64;
    fn client_uptime_ms(&self) -> u64;
    fn map_id(&self) -> u32;
    fn position(&self) -> ProducerPosition;
    fn target(&self) -> Result<TargetObservation>;
    fn visibility(&self) -> Result<Visibility>;
    fn zone_id(&self) -> Option<u32> { None }
    fn area_id(&self) -> Option<u32> { None }
}

pub fn observation_from_producer<P: ProducerSnapshot + ?Sized>(
    source: &P,
    exported_at_unix_ms: i64,
) -> Result<Observation>;

pub trait ProducerSession {
    type Snapshot: ProducerSnapshot;
    fn export_due(&self) -> bool;
    fn export(
        &mut self,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
    ) -> std::result::Result<Option<Self::Snapshot>, ProducerExportError>;
    fn mark_published(&mut self);
}

pub fn pump_once<S: ProducerSession>(
    publisher: &ObservationPublisher,
    producer: &mut S,
    exported_at_unix_ms: i64,
    client_uptime_ms: u64,
) -> std::result::Result<PumpResult, PumpError>;

pub enum PumpResult { Idle, HandedOff(PublishReceipt), Retained }
```

`LoopbackSink` remains available as the synchronous one-shot low-level API. It is not the
game-session handoff: `connect`, handshake, and `send` can block up to their configured timeouts.
Use `ObservationPublisher` in the application boundary.

## Stable JSON wire shape

Transport: loopback TCP, 4-byte big-endian length prefix, UTF-8 JSON, one active receiver client.
Handshake is unchanged except for the v2 schema string.

```json
{
  "type": "hello",
  "protocol": "benilla.wm_native",
  "protocol_version": 1,
  "schema_version": "wm.native.observation.v2",
  "token": "<owner-provided token>"
}
```

```json
{
  "type": "hello_ack",
  "protocol": "benilla.wm_native",
  "protocol_version": 1,
  "schema_version": "wm.native.observation.v2",
  "max_frame_bytes": 65536
}
```

Observation envelope:

```json
{
  "type": "observation",
  "observation": {
    "schema_version": "wm.native.observation.v2",
    "session_epoch": 7,
    "player_guid": 12345,
    "sequence": 42,
    "state_revision": 17,
    "observed_at_unix_ms": 1760000000000,
    "exported_at_unix_ms": 1760000000012,
    "client_uptime_ms": 123456,
    "map_id": 0,
    "zone_id": 12,
    "area_id": 34,
    "position": {"x": -8912.5, "y": -132.25, "z": 82.0, "orientation": 1.57},
    "target": {"state": "target", "guid": 67890},
    "visibility": {
      "complete": true,
      "total_guid_count": 2,
      "guids": [67890, 67891]
    }
  }
}
```

Target states:

```json
{"state": "unknown"}
{"state": "none"}
{"state": "target", "guid": 67890}
```

- `unknown` = the source has not observed target state. It is **not** no-target.
- `none` = the source observed that the player currently has no target.
- `target` = the source observed a nonzero full GUID.
- GLM's WIP `session.rs` reads the delivered `UNIT_FIELD_TARGET`; an undelivered field is skipped,
  so a successful current export has explicit observed no-target (`None`) or target (`Some`). The
  bridge's `unknown` is for a producer that exports before source target state is available; do not
  use `none` for unknown.

Visibility states:

- `guids` is sorted by full `u64` GUID, unique, nonzero, excludes the player's own GUID, and is at
  most 256 entries.
- `total_guid_count` is the producer's total before the export bound and must be at least
  `guids.len()`.
- `complete: true` requires `total_guid_count == guids.len()`. If a producer cannot assert complete
  visibility, it must send `complete: false`, even when the bounded list happens to contain every
  known object.
- A complete over-bound export is rejected. It must not be silently truncated.
- WM consumers must check `visibility.complete` before interpreting absence of a GUID as
  out-of-visibility.

JSON keys are exact; unknown or missing keys are rejected. There is no backward-compatible v1
decoder.

## Freshness and ordering semantics

- `session_epoch`: nonzero process-local play/reducer generation. GLM's `start_session` increments
  it on a world reconnect/re-begin. A transport-only reconnect does not change it. A process restart
  begins a new authenticated connection and may restart the numeric epoch. A map transition clears
  replicated state; GLM currently keeps the epoch, so the application must retire the publisher on
  `map_id` change to invalidate immediately (or GLM may bump the epoch in a later producer).
- `sequence`: strictly increases for every exported observation within one epoch. It may advance
  when the same source state is re-exported.
- `state_revision`: increases only when source-observed state changes. Re-exporting unchanged state
  keeps it unchanged.
- `observed_at_unix_ms`: source-observation time associated with `state_revision`. It must not
  advance when the state and `state_revision` are unchanged.
- `exported_at_unix_ms`: handoff/export time. It must be greater than or equal to the source time;
  it is diagnostic and does not renew freshness.
- `client_uptime_ms`: monotonic client uptime within a session. It may advance for an unchanged
  state export.

Within one epoch, both sides enforce:

- player GUID does not change;
- sequence strictly increases;
- uptime does not go backwards;
- source observation time does not go backwards;
- state revision does not go backwards;
- equal revision means identical source state and exactly the same `observed_at_unix_ms`;
- a larger revision may carry the same exported content because a latest-only sender can coalesce
  A -> B -> A.

Receiver freshness is based on `observed_at_unix_ms` only: default maximum age 5000 ms, maximum
future skew 2000 ms (checked for both observed and export time). Re-publishing unchanged state with
a newer export timestamp or sequence does not extend `latest_for`. A stable state with no new
source update is allowed to age out; the bridge does not fake a fresh observation. WM then falls
back through its existing context loader.

A larger epoch starts a new ordering scope. A smaller epoch is rejected within an authenticated
connection. The receiver starts a new ordering scope on every new authenticated connection, so a
process restart whose epoch starts again at 1 is accepted after reconnect; the previous process's
state is not carried across.

## Bounded sender handoff and reconnect policy

`ObservationPublisher` owns all blocking socket work in one worker:

- `publish` validates locally, takes the handoff mutex with `try_lock`, stores at most one pending
  observation, and returns a `PublishOutcome`. It never connects, handshakes, reads, or writes.
- `HandedOff` means the snapshot is pending. `coalesced` is true when it replaced an older pending
  snapshot. One latest observation may also be in flight; there is no history or unbounded queue.
- If the short critical section is contended, `publish` returns `Retained` with the **same**
  observation, source timestamp, sequence, and state revision. There is exactly one retry owner.
  The production pump uses the producer's single `unsent` slot as that owner and drops the bridge
  value. A standalone caller can instead keep at most one bridge value and retry via `retry` no
  earlier than `retry_at` (`DEFAULT_BUSY_RETRY_DELAY` is 1 ms; the next application update may be
  later). Never do both.
- If a newer snapshot was already handed off before a caller-owned retained snapshot is retried,
  `retry` returns `Superseded`; drop the old retained snapshot. Do not mutate a retained snapshot.
  The production pump never retries a bridge value, so its producer export is always the current
  snapshot and cannot acknowledge an older one.
- The worker connects lazily on the first publication. Connect/handshake/write failures close the
  sink, retain the newest observation, and retry with exponential backoff: 100 ms initial, doubling
  to 2 s maximum by default. A newer publication replaces the retry payload.
- `retire` signals shutdown without joining and returns a `PublisherRetirement`. `Drop` signals the
  same shutdown and detaches the worker; it does not call `close` or `join`. Neither immediately
  closes an in-flight socket; the worker closes it after its current bounded operation.
- `close` and `PublisherRetirement::join` block only on lifecycle/shutdown paths. `retire`,
  `spawn_after`, and `publish` do not wait for socket I/O.
- No automatic reconnect occurs after `close`/drop. A new publisher is required.

Replacement ordering:

- Retire the old publisher, then call `ObservationPublisher::spawn_after(config, &retirement)`.
  The new call returns immediately; its worker waits until the old worker has stopped and closed
  its socket before attempting a connection.
- The old worker is signalled first. It wakes immediately if idle, or finishes at most one already
  started connect/handshake/write operation, shuts the socket, then marks itself stopped. A close
  signalled while connecting prevents the subsequent write. If a write is already in progress it may
  complete. With default timeouts the worst case is approximately `connect_timeout` (2 s) +
  `read_timeout` (2 s) + `write_timeout` (250 ms) plus scheduler delay: about 4.25 s. If idle, the
  wake is immediate.
- During that bounded interval, the receiver may still hold the old snapshot; WM can read it until
  the receiver observes close/EOF and invalidates, or until the 5 s freshness bound expires. After
  the old worker marks itself stopped, it cannot write again.
- The receiver invalidates the native snapshot when it observes the old socket close/EOF, idle
  timeout, protocol failure, or shutdown. The new worker waits for the old worker to stop, but the
  receiver may not yet have processed the old EOF; it can reject the first new connection as
  `busy`, and the new worker retries with backoff. There is never a second accepted client. No new
  observation is accepted from the old epoch after invalidation.
- Keep `retirement` long enough to call `spawn_after`; after that it may be dropped. The old worker
  still completes and marks itself stopped. Use `wait_timeout` or `join` only off the packet path.

Receiver policy:

- One authenticated client at a time; another connection is rejected as `busy`.
- A successful hello starts a new store ordering scope and immediately invalidates any previous
  native snapshot.
- Disconnect, idle timeout (30 s default), protocol/validation failure, and receiver shutdown all
  invalidate the latest native snapshot immediately. `latest_for` returns `None`, so WM falls back
  to its existing DB snapshot loader rather than serving stale native state.
- A transport-only reconnect may reuse the same epoch and next sequence. The new connection resets
  the receiver's numeric epoch comparison, then accepts the current-state publication.
- A world-session reconnect or process restart must retire the publisher and use `spawn_after` for
  replacement. Start with a new nonzero `session_epoch` before publishing the next source-observed
  state. A map transition must also retire the publisher; GLM's current `SessionState` keeps the
  same epoch, so the next publisher may reuse that epoch and the next sequence, or GLM may bump it.
  Do not publish coordinates for the new map before the source has observed them.

## Exact Python consumer

Receiver module/entrypoint: `wm_native.receiver:main`, also reachable as `python -m wm_native`.
The package is source-only at this checkpoint; no install occurred.

PowerShell invocation after the owner supplies a token (not executed here):

```powershell
$env:PYTHONPATH = "D:/Projects/project-b-opencode/integrations/wm-native/src"
$env:WM_NATIVE_OBSERVATION_TOKEN = "<owner-provided token, 16..512 printable ASCII>"
python -m wm_native --host 127.0.0.1 --port 48765
```

Optional args: `--player-guid`, `--max-age-ms` (default 5000), `--max-future-skew-ms`
(default 2000), `--idle-timeout` (default 30), `--log-level`. The receiver binds loopback only and
never logs the token.

Lifecycle helper (actual source: `integrations/wm-native/src/wm_native/runtime.py`):

```python
from wm_native import NativeObservationRuntime, NativeObservationStartupError

try:
    runtime = NativeObservationRuntime.start_from_env(
        allowed_player_guid=player_guid,
    )
except NativeObservationStartupError:
    runtime = None  # keep the existing DB loader
try:
    snapshot = runtime.loader.load_latest(player_guid=player_guid) if runtime else None
finally:
    if runtime is not None:
        runtime.close()
```

`NativeObservationRuntime` creates exactly one `ObservationStore`, one `ObservationServer`, and one
`NativeClientObservationLoader`. It returns `None` without a token, raises one typed error on
startup failure after cleanup, and `close()` is idempotent and invalidates the store.

`NativeClientObservationLoader` implements WM's existing
`wm.context.builder.NativeContextSnapshotLoader.load_latest(*, player_guid)` contract. It returns
`None` when no current observation is in scope and fresh. Its payload contains `state_revision`,
`observed_at_unix_ms`, `exported_at_unix_ms`, `target`, and `visibility`; it does not expose a
collapsed target boolean or a completeness-free GUID list.

## Existing WM context insertion point

Read-only production shape at inspection:

- Protocol: `D:/WOW/wm-project/src/wm/context/builder.py:89` (`NativeContextSnapshotLoader`).
- DB loader: `builder.py:111` (`LatestNativeContextSnapshotLoader`).
- Builder injection point: `builder.py:158` (`native_snapshot_loader` parameter).
- Existing session-pack call: `D:/WOW/wm-project/src/wm/context/pack.py:96` constructs
  `LatestNativeContextSnapshotLoader` directly.
- Existing world-context call: `D:/WOW/wm-project/src/wm/autoplay/world_context.py:52` calls
  `build_session_context_pack`.

Proposed owner-applied glue, not applied:

```python
from wm.context.builder import LatestNativeContextSnapshotLoader
from wm_native import (
    FallbackNativeContextSnapshotLoader,
    NativeClientObservationLoader,
)

db_snapshot_loader = LatestNativeContextSnapshotLoader(client=client, settings=settings)
native_snapshot_loader = NativeClientObservationLoader(store)  # one process-wide store
snapshot_loader = FallbackNativeContextSnapshotLoader(
    primary=native_snapshot_loader,
    fallback=db_snapshot_loader,
)
native_context_snapshot = snapshot_loader.load_latest(player_guid=int(player_guid))
```

This replaces only the native-snapshot selection inside the existing context owner. The receiver
and store must be created once in the WM service lifecycle, not once per context pack. Do not add a
second director, registry, command path, or DB writer. Until the owner starts the receiver and
injects this loader, the bridge is **not WM-integrated**; it is a standalone consumer seam.

## GLM producer source state and consumed semantics

GLM's clean v2 producer (`71aefc2e`, cleanup `6ae14b3b`) publishes exactly:

```rust
impl SessionState {
    pub fn new() -> Self;
    pub fn epoch(&self) -> u64;
    pub fn start_session(&mut self, player_guid: u64);
    pub fn finish_session(&mut self);
    pub fn reduce(
        &mut self,
        op: u16,
        payload: &[u8],
        observed_at_unix_ms: i64,
    ) -> anyhow::Result<Vec<ChangeEvent>>;
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent>;
    pub fn export_due(&self) -> bool;
    pub fn export(
        &mut self,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
    ) -> anyhow::Result<Option<SelfObservation>>;
    pub fn mark_published(&mut self);
    pub fn unsent(&self) -> Option<&SelfObservation>;
}

pub enum SkipReason { NoPlayer, NoPosition }
pub enum SelfTarget { Unknown, None, Target(u64) }
pub enum LifeEvent { MapTransition { from: u32, to: u32 } }

pub struct SelfVisibility {
    pub complete: bool,
    pub total_guid_count: u64,
    pub guids: Vec<u64>,
}

pub struct SelfObservation {
    pub session_epoch: u64,
    pub player_guid: u64,
    pub sequence: u64,
    pub state_revision: u64,
    pub observed_at_unix_ms: i64,
    pub client_uptime_ms: u64,
    pub map_id: u32,
    pub zone_id: Option<u32>,
    pub area_id: Option<u32>,
    pub position: PositionSnapshot,
    pub position_observed_at_unix_ms: i64,
    pub target: SelfTarget,
    pub visibility: SelfVisibility,
}
```

Consumed semantics:

- `state_revision` is source-change-only and mapped directly. `sequence` is the export counter and
  is never used as the revision.
- `observed_at_unix_ms` is the packet-arrival time of the source change and does not advance on
  unchanged re-exports.
- `SelfTarget` exports all three states: `Unknown`, observed `None`, and `Target(full_u64_guid)`.
- `SelfVisibility` preserves `complete`, `total_guid_count`, and sorted player-excluding `guids`.
  Over-bound visibility exports the first 256 with `complete: false`; the adapter must retain that
  incompleteness and total.
- `zone_id`/`area_id` are optional and may be `None`; the adapter passes them through.
- `PositionSnapshot::o` maps to bridge `orientation`.
- The producer owns one `unsent` snapshot until `mark_published`. While retained, a later `export`
  reissues it with a fresh sequence/export time and the same revision/source time. The bridge pump
  uses that producer retention as the sole retry owner.
- A map transition keeps the epoch but retires the publisher; `take_lifecycle` reports it.
- Skip reasons are only `NoPlayer` and `NoPosition`. Unknown target and incomplete visibility are
  exportable, not skipped.

No producer struct change is required. The bridge trait is an accessor adapter over the actual
values; it is not a mirrored layout.

## Exact conversion and application call site

GLM published the actual v2 producer contract at commit `71aefc2e`
(`docs/convergence/glm-producer.md`) and cleanup `6ae14b3b`. The adapter maps its actual fields
directly and keeps cross-checkout imports out of the bridge dependency graph:

- `state_revision`: read `SelfObservation::state_revision` directly; never derive it from `sequence`.
- `target`: map all three `SelfTarget::{Unknown, None, Target(guid)}` variants.
- `visibility`: preserve `complete`, `total_guid_count`, and `guids` exactly.
- `zone_id`/`area_id`: pass through their optional values; `None` is valid.
- `sequence` remains the export counter; `observed_at_unix_ms` remains source time.
- `exported_at_unix_ms` is supplied by the driver at the pump call.

Actual pump source is owned by this checkout:
`crates/benilla-wm-native/src/pump.rs` (`ProducerSession`, `pump_once`, `PumpResult`, `PumpError`).
The producer owns the single `unsent` retry slot. `PumpResult::Retained` is **not** an accepted
handoff and does not call `mark_published`; the next tick re-exports the producer's retained
snapshot even when no packets arrive. The pump never keeps a second bridge-side retry copy, so an
older retry cannot acknowledge a newer producer snapshot.

GLM's app crate adds these local newtypes/impls; the pure `benilla-protocol-wrath` crate does not
depend on the bridge and no path dependency crosses checkouts:

```rust
use benilla_protocol_wrath::session::{SelfObservation, SelfTarget, SessionState};
use benilla_wm_native::{
    observation_from_producer, pump_once, ProducerExportError, ProducerPosition, ProducerSession,
    ProducerSnapshot, TargetObservation, Visibility,
};

struct WmProducerSnapshot(SelfObservation);

impl ProducerSnapshot for WmProducerSnapshot {
    fn session_epoch(&self) -> u64 { self.0.session_epoch }
    fn player_guid(&self) -> u64 { self.0.player_guid }
    fn sequence(&self) -> u64 { self.0.sequence }
    fn state_revision(&self) -> u64 { self.0.state_revision }
    fn observed_at_unix_ms(&self) -> i64 { self.0.observed_at_unix_ms }
    fn client_uptime_ms(&self) -> u64 { self.0.client_uptime_ms }
    fn map_id(&self) -> u32 { self.0.map_id }
    fn zone_id(&self) -> Option<u32> { self.0.zone_id }
    fn area_id(&self) -> Option<u32> { self.0.area_id }
    fn position(&self) -> ProducerPosition {
        ProducerPosition {
            x: self.0.position.x,
            y: self.0.position.y,
            z: self.0.position.z,
            orientation: self.0.position.o,
        }
    }
    fn target(&self) -> benilla_wm_native::Result<TargetObservation> {
        match &self.0.target {
            SelfTarget::Unknown => Ok(TargetObservation::Unknown),
            SelfTarget::None => Ok(TargetObservation::None),
            SelfTarget::Target(guid) => TargetObservation::target(*guid),
        }
    }
    fn visibility(&self) -> benilla_wm_native::Result<Visibility> {
        let visibility = &self.0.visibility;
        Visibility::new(
            visibility.complete,
            visibility.total_guid_count,
            visibility.guids.clone(),
        )
    }
}

struct WmSession(SessionState);

impl ProducerSession for WmSession {
    type Snapshot = WmProducerSnapshot;

    fn export_due(&self) -> bool { self.0.export_due() }

    fn export(
        &mut self,
        exported_at_unix_ms: i64,
        client_uptime_ms: u64,
    ) -> std::result::Result<Option<Self::Snapshot>, ProducerExportError> {
        self.0
            .export(exported_at_unix_ms, client_uptime_ms)
            .map(|snapshot| snapshot.map(WmProducerSnapshot))
            .map_err(|error| ProducerExportError::new(error.to_string()))
    }

    fn mark_published(&mut self) { self.0.mark_published(); }
}
```

Exact driver call site (every loop tick, even with no packets):

```rust
use benilla_wm_native::pump_once;

// After reducing each packet with its arrival time:
// session.0.reduce(op, payload, arrival_unix_ms)?;

// Per loop tick, regardless of whether a packet arrived:
pump_once(
    &publisher,
    &mut wm_session,
    unix_now_ms(),                 // exported_at
    uptime_ms,                     // client_uptime_ms
)?;
```

Acknowledgment mapping inside `pump_once`:

- `HandedOff` -> call `mark_published` exactly once for that exported snapshot.
- `Retained` -> do not acknowledge; drop the bridge value because the producer retains the one
  snapshot and will re-export it next tick.
- `Superseded` -> do not acknowledge; return `PumpError::Superseded`. This is an invariant failure
  for a single pump and must be treated as a bridge/producer ordering error, not success.

Map/lifecycle handling remains separate: drain `SessionState::take_lifecycle()`, retire the
publisher on `MapTransition`, and keep the same epoch for the map change per GLM's contract.

Replacement on session/map/process reset:

```rust
let retirement = publisher.retire();           // non-blocking signal
let next = ObservationPublisher::spawn_after( // returns immediately
    next_config,
    &retirement,
)?;                                            // worker waits for old socket close
// `retirement` may now be dropped; use wait_timeout/join only off the packet path.
```

Application-owned values:

- `observed_at_unix_ms` and `client_uptime_ms`: passed into `SessionState::export` by the driver;
  the producer is time-free.
- `exported_at_unix_ms`: actual export time, not a copy of source time when the state is old.
- Publisher lifetime: retire it on world-session reconnect, map transition, or process restart.
- Drop any retained snapshot that belongs to the retired session/map; do not retry it into the new
  epoch.

On a busy handoff, keep the returned `Retained` snapshot and retry at `retry_at`, even without a
new world-state change. Replace it only with a newer source snapshot. `HandedOff` means handoff, not
socket delivery or WM acceptance; `Superseded` means a newer snapshot already won and the older
retained snapshot must be dropped.

## Remaining WM startup glue

Production WM is read-only here. The owner must apply the following glue; it is not applied.

### 1. Long-lived receiver lifecycle

`AutoplayService` is the existing long-lived owner used by `python -m wm.autoplay run`
(`src/wm/autoplay/service.py:248`). Do not add another service. The reusable composition lives in
this checkout at `integrations/wm-native/src/wm_native/runtime.py` and wraps the existing store,
receiver, and loader.

Initialize one attribute in `AutoplayService.__init__`:

```python
self.native_observation_runtime = None
```

Inside `run_forever`, after the runtime marker and before the tick loop, start it once:

```python
from wm_native import NativeObservationRuntime, NativeObservationStartupError

try:
    self.native_observation_runtime = NativeObservationRuntime.start_from_env(
        allowed_player_guid=config.player_guid,
    )
except NativeObservationStartupError as exc:
    self.native_observation_runtime = None
    self.store.add_issue({
        "reason": "native_receiver_unavailable",
        "kind": "native",
        "detail": str(exc)[:500],
    })
```

`start_from_env` returns `None` when `WM_NATIVE_OBSERVATION_TOKEN` is unset or empty. When the
token is present, it creates exactly one store and one loader, then starts the one receiver. Invalid
configuration or bind/start failure is cleaned up and raised as
`NativeObservationStartupError`.

In the existing `run_forever` `finally` block, before the runtime-stopped marker:

```python
if self.native_observation_runtime is not None:
    self.native_observation_runtime.close()
self.native_observation_runtime = None
```

`close` is idempotent, stops the receiver, and invalidates the shared store so WM immediately
falls back to its existing DB loader. Startup and shutdown run on the service lifecycle, not the
packet path. If the runtime is absent or fails to start, WM keeps its current DB-only behavior.

### 2. Pass the loader through the existing context calls

`build_session_context_pack` (`src/wm/context/pack.py:56`) gains an optional
`native_snapshot_loader` argument. At the existing line 96, select:

```python
from wm_native import FallbackNativeContextSnapshotLoader

db_loader = LatestNativeContextSnapshotLoader(client=client, settings=settings)
if native_snapshot_loader is not None:
    native_context_snapshot = FallbackNativeContextSnapshotLoader(
        primary=native_snapshot_loader,
        fallback=db_loader,
    ).load_latest(player_guid=int(player_guid))
else:
    native_context_snapshot = db_loader.load_latest(player_guid=int(player_guid))
```

`build_chat_world_context` (`src/wm/autoplay/world_context.py:13`) gains an optional
`native_snapshot_loader` argument and passes it to `build_session_context_pack` at line 52.

Add one small accessor to `AutoplayService`:

```python
def _native_snapshot_loader(self):
    runtime = self.native_observation_runtime
    return runtime.loader if runtime is not None else None
```

Update the three live-service call sites to pass `native_snapshot_loader=self._native_snapshot_loader()`:

- `src/wm/autoplay/service.py:623` (`build_chat_world_context`);
- `src/wm/autoplay/service.py:1958` (`build_session_context_pack`);
- `src/wm/autoplay/service.py:2141` (`build_chat_world_context`).

The fallback composition is read-only and preserves DB behavior whenever the native store has no
in-scope fresh observation.

## Exact GLM integration handoff

Bridge files required by GLM's application crate:

```text
crates/benilla-wm-native/Cargo.toml
crates/benilla-wm-native/src/lib.rs
crates/benilla-wm-native/src/auth.rs
crates/benilla-wm-native/src/client.rs
crates/benilla-wm-native/src/error.rs
crates/benilla-wm-native/src/frame.rs
crates/benilla-wm-native/src/observation.rs
crates/benilla-wm-native/src/producer.rs
crates/benilla-wm-native/src/publisher.rs
crates/benilla-wm-native/src/pump.rs
```

Dependency design:

- Add `benilla-wm-native` to the final GLM workspace as a normal local workspace crate. Do **not**
  import `D:/Projects/project-b-opencode/...` by absolute path and do not add a cross-checkout path
  dependency.
- `benilla-protocol-wrath` remains unchanged and has **no** bridge, socket, WM, or serde dependency.
- Put the `WmProducerSnapshot` / `WmSession` impls and `pump_once` call in a new GLM application
  crate, recommended as `crates/benilla-wm-driver` (`src/main.rs` or `src/lib.rs`), or in GLM's
  existing application/driver crate. It must depend on both producer and bridge:

```toml
[dependencies]
benilla-protocol-wrath = { path = "../benilla-protocol-wrath" }
benilla-wm-native = { path = "../benilla-wm-native" }
anyhow = "1"
```

- `benilla-wm-native` keeps only its existing `serde` and `serde_json` dependencies; it does not
  depend on GLM or WM.
- The owner must update the workspace members and `Cargo.lock` through the normal build/lock step;
  this checkout did not do so.
- The renderless `wrath-world` producer call site can move its publish step into that application
  crate. Do not make `benilla-protocol-wrath` import the bridge merely to keep the demo binary there.

## Actual source versus remaining application glue

Actual source in this checkout:

- Rust bridge: `observation.rs`, `producer.rs`, `pump.rs`, `publisher.rs`, and their crate exports.
- Python receiver path: `receiver.py`, `store.py`, `context_adapter.py`, `runtime.py`, and exports.
- Runtime helper behavior: one store, one receiver, one loader; typed startup failure; idempotent
  shutdown; fallback loader composition.

Owner-applied application glue:

- GLM app crate: `WmProducerSnapshot`, `WmSession`, and `pump_once` call every tick.
- WM `AutoplayService`: create/close the runtime and pass its loader through the three existing
  context call sites.
- WM `pack.py`/`world_context.py`: optional loader argument and existing-loader fallback.
- Workspace/Cargo membership and lockfile update, through the allowed build step.

No production WM, GLM, config, combat, or F08 file was edited by this work.

## Integration status

- **Source-integrated:** schema v2 and validation on both sides, explicit target and visibility
  states, source-vs-export freshness, session-generation rules, bounded publisher handoff with
  retained busy snapshots, nonblocking retirement/replacement ordering, disconnect invalidation,
  actual v2 producer adapter, producer-owned pump with single retry ownership, Python
  receiver/store/adapter/runtime lifecycle helper, and WM startup documentation.
- **Not implemented:** the local `WmProducerSnapshot`/`WmSession` impls and `pump_once` call in
  GLM's application crate; production WM receiver startup/loader injection (exact glue above);
  server/WM hooks for any combat intent/result path.
- **Uncompiled:** no cargo build, Python import, test, formatter, linter, or type check was run.
- **Runtime-unverified:** no socket, handshake, session, reconnect, stale-state rejection, retained
  retry, replacement ordering, invalidation delay, PID, or live WM context read was exercised. No
  live input was fabricated.

## Short GLM handoff

1. Read this file and the listed checkpoint hashes. Do not infer the API from the older
   `wm-native-bridge.md` or schema v1.
2. The producer contract is already published at `docs/convergence/glm-producer.md`. No producer
   struct-layout changes are needed; the adapter consumes its actual fields directly.
3. Keep `SessionState` and bridge I/O out of the packet decoder. The driver stamps each packet's
   arrival time into `reduce` and passes export/uptime clocks into `export`; the producer remains
   clock-free.
4. In GLM's app crate, add `WmProducerSnapshot` and `WmSession` exactly as shown. Map
   `state_revision` directly, all three `SelfTarget` variants, and every `SelfVisibility` field;
   pass zone/area through. Do not add the bridge dependency to the pure reducer crate.
5. Per loop tick, call `pump_once(&publisher, &mut wm_session, exported_at, uptime)`. Do not call
   `publisher.publish` from the packet decode path.
6. `HandedOff` acknowledges via `mark_published`; `Retained` does not acknowledge and leaves the
   producer's one `unsent` snapshot to re-export on the next tick even with no packets;
   `Superseded` does not acknowledge and is an ordering error for a single pump.
7. On world reconnect, map change, or process restart, call `retire` and use `spawn_after` for the
   replacement; keep `join` off the packet path. Drain `take_lifecycle`.
8. Apply the bridge file/dependency handoff below and the WM startup glue above.
