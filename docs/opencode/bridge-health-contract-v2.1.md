# WM observation bridge v2.1 — validity, receiver acceptance and health

Updated: 2026-10-25. Status: **SOURCE-INTEGRATED, UNCOMPILED, RUNTIME-UNVERIFIED**. This supersedes
the v2 wire/health portions of `docs/opencode/convergence-consumer.md`.

## Checkpoint identity

OpenCode bridge crate:

```text
crates/benilla-wm-native/src/observation.rs  6522f4e42f014501f6431f80c1f8ba423f431176eb36249bdc6df618bb91b00f
crates/benilla-wm-native/src/client.rs       4271c52a955794c3d402cdaa98e4aef97ada2554654a73587088a998da6f86b4
crates/benilla-wm-native/src/frame.rs        adf0d74927ff198bb099b96c4f5ba93131d09f1fe5163a299a1d7d3a28d79468
crates/benilla-wm-native/src/error.rs        7837506c86134bb69addcbe79d23887f4f13371eaf79240a776db7dcba89184a
crates/benilla-wm-native/src/health.rs       3bb5b712dc8d32914072d82e8a703988ad3d4a87cce90a3c4314e531c9ce8e72
crates/benilla-wm-native/src/producer.rs     0f7ce32e7cdd76717488cb9e02e2f91f41a3af1b82d448f449430ac32a73b43e
crates/benilla-wm-native/src/pump.rs         1d9a31d361e70dbc52d9683ea9e850fd50893318967255731f0b28ada117c7a7
crates/benilla-wm-native/src/publisher.rs    16c3f5d08c257f237f69e76f75c30b7385131d76ae12b9e53290b3f39d0f0a00
crates/benilla-wm-native/src/lib.rs          3ea585c272e6d72053a84d31d9089fbf3d4ab83a7ac6021e23c1a007fb8b1405
```

Python receiver/runtime:

```text
integrations/wm-native/src/wm_native/constants.py       26619f78e6b3af57d70b03297309c64c0790da451accb7fedb851bb464275b6f
integrations/wm-native/src/wm_native/models.py           24f05b10e5d7aedc69abe9431533295354c209b407dda73a351ca5a7fdf79682
integrations/wm-native/src/wm_native/protocol.py         4827d5fadbeb7637896e689a0553858129aafa30f3a0e3ecd184e71f5c498051
integrations/wm-native/src/wm_native/receiver.py         75bb7e98724502561622cf27aef315b8095589762a90e2c3e193441fbef59b5d
integrations/wm-native/src/wm_native/store.py            de26ef4cfb52e9948a76cbfe20d889c100602a5251e0f8245d64e627d91dbdfc
integrations/wm-native/src/wm_native/context_adapter.py  d73444cb2289f4880d7302c478ff70ce56d4ee57515192ec4db58ce5559f8f94
integrations/wm-native/src/wm_native/runtime.py          de552a3ddb39663a8c94a09b7cf82f688b190b576fd1b25546b99dc84f6ea931
integrations/wm-native/src/wm_native/errors.py           f7509d9c8fca51a877b5a6e9acc320923457fb90376e283bc363ea882ca68da4
integrations/wm-native/src/wm_native/__init__.py         8dfcf45db5783081e7a1506a852cf9a9806e4db0d7e81a8e8f9984e8debe3b45
```

GLM integration base inspected read-only: `D:/Projects/project-b-client` at `1f8d33b8` (clean).
Its integrated bridge is v2 and must consume this v2.1 checkpoint before an updated receiver is
used.

## Schema v2.1

Version: `wm.native.observation.v2.1`. Added fields:

- `world_generation: u64` — nonzero world-scope identity. It advances at session start and every
  real world transfer, including same-map instance changes; map ID alone is not scope.
- `position_observed_at_unix_ms: i64` — arrival time of the source packet that last changed the
  exported player position. It is provenance, not a lease; it does not move for unrelated NPC
  activity or unchanged re-exports.

Existing fields keep their v2 meaning: `state_revision` is source-change-only, `sequence` is the
export counter, `observed_at_unix_ms` is the aggregate source-change time, and
`exported_at_unix_ms` is handoff time.

```json
{
  "type": "observation",
  "observation": {
    "schema_version": "wm.native.observation.v2.1",
    "session_epoch": 7,
    "world_generation": 3,
    "player_guid": 12345,
    "sequence": 42,
    "state_revision": 17,
    "observed_at_unix_ms": 1760000000000,
    "exported_at_unix_ms": 1760000000012,
    "client_uptime_ms": 123456,
    "map_id": 0,
    "zone_id": null,
    "area_id": null,
    "position_observed_at_unix_ms": 1759999999000,
    "position": {"x": -8912.5, "y": -132.25, "z": 82.0, "orientation": 1.57},
    "target": {"state": "unknown"},
    "visibility": {"complete": false, "total_guid_count": 318, "guids": [/* 256 sorted */]}
  }
}
```

Validation additions:

- world generation is nonzero and cannot regress within one session epoch;
- position provenance is within `[0, observed_at_unix_ms]` and cannot regress within one world
  generation;
- unchanged `state_revision` still requires identical state and identical aggregate source time;
  changing only NPC activity may advance revision/aggregate time but leaves position provenance
  unchanged;
- complete visibility still requires `total_guid_count == guids.len()`; incomplete visibility keeps
  the pre-bound total.

## Receiver acceptance and ACK

The receiver sends one bounded ACK after it validates and stores an observation:

```json
{"type": "ack", "sequence": 42, "state_revision": 17, "world_generation": 3}
```

The sender now reads that ACK after each write. Four facts remain distinct:

| Fact | Owner | Evidence |
|---|---|---|
| handoff | producer/pump/publisher | `PublishOutcome::HandedOff` |
| socket write | publisher worker | `PublisherHealth.writes` |
| receiver acceptance | receiver store + ACK | `PublisherHealth.accepted`, `ReceiverHealth.last_accepted` |
| WM availability | receiver store / runtime | `NativeObservationHealth.snapshot` / `latest_for` |

`HandedOff` is not receiver acceptance. `PublishOutcome::Retained` is not accepted handoff. The
producer remains the one retry owner; `pump_once` acknowledges the producer only on `HandedOff` and
never retries a bridge-owned copy alongside the producer's `unsent` slot.

### Quiet-state world-session health

The sender also emits bounded `health` frames, separate from observations:

```json
{
  "type": "health",
  "health": {
    "session_epoch": 7,
    "world_generation": 3,
    "player_guid": 12345,
    "state": "in_world",
    "last_server_packet_at_unix_ms": 1760000000000
  }
}
```

The receiver answers with:

```json
{
  "type": "health_ack",
  "session_epoch": 7,
  "world_generation": 3,
  "player_guid": 12345
}
```

`state` is `in_world`, `transferring` or `disconnected`. Health frames never change source
observation timestamps or revision. They keep the transport alive every 5 s (so the 30 s receiver
idle timeout does not close a quiet healthy session), expose world-session state, and let the
publisher reconnect/resynchronize without inventing freshness.

Validity policy:

- Replicated values stay **known/valid** while health is `in_world`, health scope matches the
  observation, and health itself is received within 15 s. Aggregate and position ages continue to
  grow from their actual source timestamps; they are not leases.
- `transferring` keeps the transport but makes old-scope action state stale. A new
  `world_generation` clears the prior scope.
- `disconnected` clears the latest observation immediately. A live socket cannot keep an abandoned
  world session eligible.
- WM action classes impose their own position-age limits using the exposed `position_age_ms`; the
  bridge does not claim a freshly re-observed position when the source timestamp is old.
- A quiet healthy session therefore remains available to WM with aging timestamps, not a
  manufactured fresh timestamp. A stale transport/health falls back through the existing loader.

The producer must supply `ProducerSession::source_health() -> Option<SourceHealth>` and send health
on every `pump_once` tick. `None` is valid before a player scope exists. This is an explicit
producer-side change; the consumer does not invent world-session health.

## Recovery and bounded retries

- The publisher monitors the idle peer every 500 ms with a bounded `peek` probe.
- A closed reader, receiver restart, ACK timeout/mismatch, write failure or explicit receiver
  rejection drops the socket and schedules the newest observation for bounded retry.
- Retry lifetime defaults to 60 s measured with publisher-local monotonic time from the first
  handoff attempt, and is checked before a retry attempt rather than only after another failure.
  An unusable snapshot increments `PublisherHealth.retry_expired` instead of reconnecting forever.
- Receiver rejection increments `PublisherHealth.rejected` and records `last_error`; retry remains
  bounded, so a stale rejected frame cannot loop indefinitely after a receiver restart.
- A reconnect first sends current `health`, then re-sends the last written snapshot. Old source
  timestamps stay old; the receiver accepts the state if the scope/session health is valid. This
  closes the quiet-world receiver-restart gap without durable exactly-once delivery.
- Health frames are sent every 5 s while connected, independently of semantic exports.
- New source state supersedes an older retry immediately. No socket work or join occurs on the
  packet-processing path.

## Health surfaces

`ObservationPublisher::health() -> PublisherHealth` exposes worker/link state and the four counters,
last handoff/write/accept stamps, connection count, retry-expiry count, and the last safe error
string (never the token).

`ObservationServer::health() -> ReceiverHealth` derives from the current thread and client state:
`running`, `thread_alive`, `failed`, `client_connected`, bound port, accepted/rejected counts,
last accepted `(sequence, revision, world_generation)`, and last error code. A stopped/failed worker
no longer appears healthy from a past successful start.

`NativeObservationRuntime.health()` combines receiver health, store stats (including world
generation and aggregate/position ages), and the current snapshot classification.

`ObservationStore.status_for(player_guid, max_position_age_ms=...)` returns exactly one of:

- `unknown` — no observation yet, or a different player;
- `valid` — in-scope aggregate-fresh snapshot; `position_age_ms` is separate and may be bounded by
  the caller's action-class policy;
- `stale` — aggregate-freshness or the caller's position bound expired it;
- `disconnected` — a prior observation existed but disconnect/shutdown invalidated the store.

`latest_for` returns only `valid`. Stale/disconnected state is observable while WM keeps the
existing loader fallback. An NPC update cannot make an old player position look fresh because the
position provenance is returned separately.

Clock policy: the receiver records the source wall age once at acceptance (rejecting excessive
future skew), then advances aggregate/position ages from `time.monotonic_ns()`. Health age and
retry lifetimes are monotonic. Wall timestamps remain for cross-process correlation and initial
skew checks. A producer wall-clock backward jump cannot reorder state (counter ordering) and is
rejected by the future-time check; no observation timestamp is repaired or refreshed synthetically.

## GLM bridge handoff (exact)

Bridge files to replace in GLM's integrated `crates/benilla-wm-native`:

```text
Cargo.toml (no dependency change expected; still serde + serde_json)
src/lib.rs
src/observation.rs
src/client.rs
src/frame.rs
src/error.rs
src/producer.rs
src/pump.rs
src/publisher.rs
```

Required GLM adapter changes (both the renderless driver and `benilla-app/src/wrath.rs` snapshot
implementations):

```rust
// ProducerSnapshot
fn world_generation(&self) -> u64 { self.0.world_generation }
fn position_observed_at_unix_ms(&self) -> i64 {
    self.0.position_observed_at_unix_ms
}
```

`ProducerSession` gains:

```rust
fn source_health(&self) -> Option<SourceHealth>;
```

GLM's local adapter maps `SessionState::phase()`:
- `InWorld` => `SourceSessionState::InWorld`;
- `TransferPending|Loading` => `SourceSessionState::Transferring`;
- `CharacterSelection|EnteringWorld|Disconnected` => `SourceSessionState::Disconnected`.

It supplies `session.epoch()`, `session.world_generation()`, `session.player_guid()`, and the
driver's last server-packet wall time. It returns `None` before a player GUID exists. `pump_once`
calls `publish_health` on every tick, including ticks with no semantic export. The driver must update
the last-packet time on every successful `world.recv()`, and send a final `Disconnected` health via
a pump tick before retiring the publisher on logout/failure.

`Observation::new` now takes, in order:

```text
session_epoch, world_generation, player_guid, sequence, state_revision,
observed_at_unix_ms, exported_at_unix_ms, client_uptime_ms, map_id,
zone_id, area_id, position_observed_at_unix_ms, position, target, visibility
```

No driver code should call `LoopbackSink::send` directly; `pump_once`/publisher wait for the ACK.
The receiver must be updated to v2.1 and to send `ack` frames in the same integration slice, or
every publication will time out waiting for acceptance. `PublisherHealth` is diagnostic; the
producer acknowledgement contract is unchanged.

## WM source integration (actual)

The Python package is vendored into the real WM package tree at `D:/WOW/wm-project/src/wm_native`
(discovered by the existing setuptools `src` layout). No `PYTHONPATH` to an agent checkout is used.

Applied WM lifecycle/context edits:

- `src/wm/autoplay/service.py` creates one `NativeObservationRuntime` in `run_forever`, exposes its
  health in status, closes it in `finally`, and passes its loader to the three existing context
  call sites.
- `src/wm/context/pack.py` accepts the optional loader and wraps it with the existing DB loader via
  `FallbackNativeContextSnapshotLoader`.
- `src/wm/autoplay/world_context.py` passes the optional loader through.
- No new director, registry, command bus or mutation lane was added.

No-token startup returns `None` and preserves DB-only behavior. Startup failure is recorded as a
`native_receiver_unavailable` issue and falls back. Repeated close is idempotent. Wrong-player
observations are rejected when `allowed_player_guid` is set. A receiver thread failure invalidates
the store and is visible as `receiver.failed`/`running=false`; there is no automatic supervisor
restart, so operator recovery is explicit.

## Route asset integration increment

A profile-backed Bevy asset source now exists in the synchronized asset worktree at
`D:/Projects/project-b-opencode-assets`, base `1f8d33b8`:

```text
crates/benilla-assets/Cargo.toml             f5687c10805b37f72c44abf383418e6d1294aefdcf2429b343e6cea4d2f8ea2f
crates/benilla-assets/src/lib.rs             59c06bfc7fd263884bbb518988afef5a8d1cd78330f76fe674df9f33f665c421
crates/benilla-assets/src/profile_source.rs  e6a067585e6fc4746973e739d7bee8deb5a0015b6f44c269b326d83de2faa88e
crates/benilla-profile/src/lib.rs            330aaf6d86af4ebf60e5479114483bdb581caa00fabc418ba6c97be1402c0a21
```

`register_profile_source(app, data_dir, manifest_path, profile)` registers `mpq://` and
`mpq-loose://` over the manifest profile. Candidate-first `Profile::reload` now opens the complete
new archive set before replacing the live one. The existing loaders keep using the same URLs, so
terrain/model/WMO/resource paths do not need a second asset abstraction.

Application handoff (GLM-owned file, not edited): before `AssetPlugin` in
`crates/benilla-app/src/lib.rs`, choose **one** source registration. For the Wrath route call
`register_profile_source` with an explicit profile/manifest; for the vanilla path keep
`register_mpq_source`. Registering both `mpq` sources on one app is rejected. The route base profile
and 30..33 x 48 tiles are pinned in `docs/baseline/route-assets.md`; the exact walk boundary and
Northshire WMO/doodad set remain unproven. C2 has the authoritative `SKIN` parser but still needs
the v264 MD20 bones/animation/layout and actor loader; C3 has the MH2O chunk parser but still needs
the LiquidType/material-to-`LiquidMesh` conversion and route collision integration. No unverified
layout or liquid mapping was guessed.

## Unexecuted validation

No build, import, test, linter, type check, receiver/WM process start, socket, DB, game or visual
probe was run. Source-integrated is not compiled, receiver-accepted, WM-consumed or
graphically demonstrated.
