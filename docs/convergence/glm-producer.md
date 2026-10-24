# GLM producer contract — native session-state snapshots for the WM observation bridge

Updated: 2026-10-25. Status: **IMPLEMENTED, TESTED, LIVE-EXERCISED**. This is the stable producer
contract for OpenCode's conversion adapter (schema `wm.native.observation.v2`). Consumer contract:
`D:/Projects/project-b-opencode/docs/opencode/convergence-consumer.md`.

## Source checkpoint (SHA-256, working tree at this writing)

```text
D:/Projects/project-b-client, branch wotlk
crates/benilla-protocol-wrath/src/session.rs       8e7bc567acb22d6b2e5dfe63a8e7bd56e7c8bddfee61335140a1699ee5a92c67
crates/benilla-protocol-wrath/src/reducer.rs       dca959f8f7a9d73f92a0651c1b68008c61e543d2c579713bacb632c237686918
crates/benilla-protocol-wrath/src/updates.rs       2724473ff166d23cdab7370040e0510b7613207bb28e14a21e5821c367b4442d
crates/benilla-protocol-wrath/src/fields.rs        d3e08e7049134d3ca7f35e1b1cca718d5ac202ed7ab0a72119c486dd79d91de9
crates/benilla-protocol-wrath/src/bin/wrath-world.rs 5491203c42f48fb1a4cb9179766ceae47cc95c23db30c484bf5f4e1f62256317
```

These hashes will change with any edit; re-read before wiring the adapter. The producer module is
`crates/benilla-protocol-wrath/src/session.rs`; it has no serde, socket, WM, or clock dependency.

## Exact public API

```rust
pub const MAX_VISIBLE_GUIDS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason { NoPlayer, NoPosition }          // implements Display + std::error::Error

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelfTarget { Unknown, None, Target(u64) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfVisibility {
    pub complete: bool,
    pub total_guid_count: u64,
    pub guids: Vec<u64>,        // sorted ascending by u64, unique, nonzero, excludes the player
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSnapshot { pub x: f32, pub y: f32, pub z: f32, pub o: f32 }

#[derive(Clone, Debug, PartialEq)]
pub struct SelfObservation {
    pub session_epoch: u64,
    pub player_guid: u64,
    pub sequence: u64,                  // export counter; advances on every Some export
    pub state_revision: u64,            // source-change counter; unchanged re-export keeps it
    pub observed_at_unix_ms: i64,       // arrival time of the packet that produced state_revision
    pub client_uptime_ms: u64,
    pub map_id: u32,
    pub zone_id: Option<u32>,           // always None today; never guessed
    pub area_id: Option<u32>,           // always None today; never guessed
    pub position: PositionSnapshot,
    pub position_observed_at_unix_ms: i64, // diagnostic provenance (proposed for schema v2.1)
    pub target: SelfTarget,
    pub visibility: SelfVisibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeEvent { MapTransition { from: u32, to: u32 } }

pub struct SessionState { /* private */ }

impl Default for SessionState { fn default() -> Self; }
impl SessionState {
    pub fn new() -> Self;
    pub fn epoch(&self) -> u64;
    pub fn start_session(&mut self, player_guid: u64);   // bumps epoch; resets every scope
    pub fn finish_session(&mut self);                    // logout/disconnect: drops state
    pub fn reduce(
        &mut self,
        op: u16,
        payload: &[u8],
        observed_at_unix_ms: i64,                        // driver-supplied packet arrival time
    ) -> anyhow::Result<Vec<ChangeEvent>>;
    pub fn take_lifecycle(&mut self) -> Vec<LifeEvent>;
    pub fn export_due(&self) -> bool;
    pub fn export(
        &mut self,
        exported_at_unix_ms: i64,                        // driver-supplied handoff time
        client_uptime_ms: u64,                           // driver-supplied monotonic uptime
    ) -> anyhow::Result<Option<SelfObservation>>;
    pub fn mark_published(&mut self);                    // driver reports bridge accepted it
    pub fn unsent(&self) -> Option<&SelfObservation>;    // diagnostics
}
```

`ChangeEvent` (from the reducer) is `Created|Updated|Moved|Removed { guid: u64 }` — the per-packet
event list is diagnostic; the observation carries state, not events.

## Semantics

### Revision and freshness (v2 core)

- `state_revision` starts at 0 per epoch and increments exactly when observation-relevant state
  changes: a values block delivering a **new** value (identical rewrites are detected and ignored),
  a movement block changing position/movement-flags/extra-flags/speeds (a periodic heartbeat with
  an identical position only advances the wire timestamp — a clock, not state), any create, any
  visibility loss (out-of-range block or `SMSG_DESTROY_OBJECT`), a `LOGIN_VERIFY_WORLD` (map and
  position established; a map **change** additionally clears replicated state), or a session
  (`re`)start resets revision to 0 with the new epoch.
- `observed_at_unix_ms` is the arrival time of the packet that produced the current revision. It is
  remembered by the producer at `reduce` time and **does not move** on unchanged re-exports.
- `exported_at_unix_ms` is not in `SelfObservation`: the driver stamps it at the
  `Observation::new` call so every retry carries a true handoff time. It never renews freshness.
- Unrelated packets (pong, chat, addon info, malformed-but-rejected input) advance neither
  `state_revision` nor any freshness value. `SessionState::reduce` returns the parse error for
  malformed update payloads; the caller treats it as fatal per the session-layer policy.

### Target (v2 tagged states)

`UNIT_FIELD_TARGET` (2 u32 words) is read from the player's own replicated entry with the
delivered-bit check (`field_delivered`), so client-view zeros are never mistaken for observations:

- field delivered, value 0 → `SelfTarget::None` (observed no-target);
- field delivered, value nonzero → `SelfTarget::Target(guid)`;
- field undelivered or no self entry yet → `SelfTarget::Unknown` (exported explicitly — an
  observation without target knowledge is still usable and is **not** suppressed).

### Visibility

`guids` = every GUID currently in replicated state except the player, sorted ascending (the
reducer's `BTreeMap` order), unique by construction, never invented. `total_guid_count` is the
pre-bound total. Over the 256 bound the export proceeds with the first 256 (sorted) and
`complete: false` — explicitly incomplete, never silently truncated, and never a skip: an
over-bound world is still a usable observation.

### Skip conditions (the only fabrication guards left)

`Err(SkipReason::NoPlayer)` — export before `start_session`; `Err(SkipReason::NoPosition)` —
neither a `LOGIN_VERIFY_WORLD` position nor a replicated self position exists. Both are
retryable: the un-handed-off revision is retained, and a later `export` after the state moves on
exports the newer state. `TargetUnknown` and `VisibilityOverflow` skips from the earlier WIP are
**removed** by the v2 semantics above.

### Retention / retry (PublisherBusy coordination)

`export` retains its snapshot until `mark_published`. Therefore:

| Event | Driver action | Producer behavior |
|---|---|---|
| `export` returns `Some` | attempt `publisher.publish(&observation)` | snapshot retained in `unsent` |
| publish `Ok` | call `session.mark_published()` | `handoff_revision = snapshot.state_revision`; retention cleared |
| publish `PublisherBusy` | do nothing; loop continues | next `export` re-issues the retained snapshot: new `sequence` + `exported_at`, same revision + source time |
| newer state while retained | nothing special | next `export` replaces the retained snapshot (latest-only coalescing); the older one is dropped by design |
| no change, all handed off | nothing | `export` returns `Ok(None)`; the producer never re-stamps old state to fake freshness |

A stable state with no new source updates simply stops being re-exported and is allowed to age out
on the receiver (which falls back to WM's DB loader) — unchanged-but-valid is distinct from stale,
and stale is distinct from disconnected:

- **unchanged-but-valid**: `state_revision` unchanged; receiver may have expired it by `max_age`;
  the producer does not renew it.
- **stale**: expired at the receiver by age; producer state may still be valid — the receiver's
  `latest_for` returning `None` is the correct signal to fall back.
- **disconnected / epoch-changed**: `finish_session` + next `start_session` (new epoch) or a map
  transition (same epoch, `LifeEvent::MapTransition`) — the driver **retires the publisher**, the
  receiver invalidates immediately, and no old-scope traffic is sent.

### Lifecycle map

| Driver event | Producer call | Bridge duty |
|---|---|---|
| character GUID resolved (char enum) | `start_session(guid)` | new epoch; new publisher allowed |
| every world packet received | `reduce(op, payload, arrival_unix_ms)` | — |
| `LifeEvent::MapTransition` drained | — (state already cleared) | retire publisher immediately; the epoch is reused, next `sequence` continues |
| export due, each loop tick | `export(exported_at, uptime)` | publish per the table above |
| `LOGOUT_COMPLETE` / disconnect | `finish_session()` (then `start_session` on reconnect) | retire publisher; new epoch on reconnect |

## Driver call site (live, `src/bin/wrath-world.rs`)

The renderless driver wires the producer exactly as specified; `wrath-world <account> <password>
[dwell_seconds]` now prints one `[wm] seq N rev N ...` line per export with position, target state,
and visibility completeness. The integration point is marked in source:

```rust
// after every world.recv() in the post-login loop:
let arrival = unix_now_ms();
session.reduce(p.opcode, &p.payload, arrival)?;

// per loop tick:
for event in session.take_lifecycle() {
    println!("[wm] lifecycle {event:?}: retire the WM publisher");
}
if session.export_due() {
    if let Some(snapshot) = session.export(unix_now_ms(), session_start.elapsed().as_millis() as u64)? {
        // prints the snapshot, then:
        session.mark_published(); // <- replace with publisher.publish(...) per the conversion below
    }
}
```

### Exact conversion to `benilla_wm_native` (OpenCode's checkpoint, v2)

`benilla-wm-native` is not a workspace member of this checkout (it lives in OpenCode's tree), so
the conversion lives at the application boundary once the crate is importable. The call, using only
checkpoint-verified APIs:

```rust
use benilla_wm_native::{
    AuthToken, LoopbackConfig, Observation, ObservationPublisher, Position, PublisherConfig,
    TargetObservation, Visibility,
};

let token = AuthToken::new(std::env::var("WM_NATIVE_OBSERVATION_TOKEN")?)?;
let publisher = ObservationPublisher::spawn(PublisherConfig::new(LoopbackConfig::new(
    std::net::SocketAddr::from(([127, 0, 0, 1], benilla_wm_native::DEFAULT_PORT)),
    token,
))?)?;

match session.export(unix_now_ms(), uptime_ms)? {
    None => {}
    Some(snapshot) => {
        let position = Position::new(
            snapshot.position.x, snapshot.position.y,
            snapshot.position.z, snapshot.position.o,
        )?;
        let target = match snapshot.target {
            SelfTarget::Unknown => TargetObservation::Unknown,
            SelfTarget::None => TargetObservation::None,
            SelfTarget::Target(g) => TargetObservation::target(g)?,
        };
        let visibility = Visibility::new(
            snapshot.visibility.complete,
            snapshot.visibility.total_guid_count,
            snapshot.visibility.guids.clone(),
        )?;
        let observation = Observation::new(
            snapshot.session_epoch, snapshot.player_guid, snapshot.sequence,
            snapshot.state_revision, snapshot.observed_at_unix_ms,
            unix_now_ms(), /* exported_at: real handoff time, not observed_at */
            snapshot.client_uptime_ms, snapshot.map_id,
            snapshot.zone_id, snapshot.area_id,
            position, target, visibility,
        )?;
        match publisher.publish(&observation) {
            Ok(_) => session.mark_published(),
            Err(benilla_wm_native::Error::PublisherBusy) => {} // retained; retry next tick
            Err(e) => { /* log; retire + respawn publisher per contract on fatal errors */ }
        }
    }
}
```

Never call `publisher.publish` from the packet decode path; the publisher's worker owns all
blocking socket work and coalesces. On world reconnect or map transition: `publisher.close()` (or
drop), then respawn; the producer's epoch/lifecycle events are the trigger.

## Test and evidence status

- 28 tests green in `benilla-protocol-wrath`: 15 unit (crypto/framing/digest/reducer + published
  RC4 KATs), 11 session-producer tests (revision/freshness separation, unrelated-traffic
  immutability, heartbeat-and-identical-rewrite suppression, target states, both visibility-loss
  paths, epoch reset, overflow-incomplete export, map transition + lifecycle event,
  PublisherBusy retention/retry, caller-owned clocks), 2 live-fixture replay tests.
- Live: `wrath-world` against the lab (project-b probe account, disposable character) printed real
  exports — source-observed position, `unknown` target before object state, and an over-bound
  world exported as `complete=false, total 318, guids 256` — with clean logout.
- `UPDATE_OBJECT` reception plus the above still does **not** claim a complete world mirror:
  zone/area are unobserved (None), spline *movement updates* (as opposed to create data) are
  parsed at create-time only, and client-side prediction/movement (B-006) is not wired yet.
