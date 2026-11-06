# OpenCode queue 02 status

Updated: 2026-10-25 (work package 01; source-integrated, not executed). All work remains uncommitted
in `D:/Projects/project-b-opencode`, branch `opencode/project-b-profile-reload`.

## Work package 01 — PARTIAL source integration

- `docs/opencode/bridge-health-contract-v2.1.md` is the current wire/health checkpoint. It adds
  `world_generation`, position provenance, ACK-based receiver acceptance, explicit source
  world-session health frames, publisher/receiver health, monotonic age/retry deadlines, bounded
  stale retry aging, and the exact GLM file/adapter handoff.
- Quiet-state policy: source timestamps never refresh; health frames keep a healthy quiet transport
  alive; source-observed values remain valid while world/session health and scope are valid; WM
  action classes use the separately exposed position age. Transferring/disconnected health
  invalidates old-scope eligibility.
- WM production source is now wired: one vendored `src/wm_native` package, one runtime in
  `AutoplayService.run_forever`, loader pass-through in service/pack/world-context, DB fallback,
  status health, idempotent shutdown, and no-token behavior.
- Route asset work is isolated at `D:/Projects/project-b-opencode-assets` from base `1f8d33b8`.
  `register_profile_source` puts the manifest profile behind the real Bevy `mpq://`/`mpq-loose://`
  asset sources; `Profile::reload` is now candidate-first.
- C2 v264 external skins and C3 MH2O/route collision remain source blockers: no verified layout was
  guessed. The app profile-source call is a precise GLM-owned handoff, not an edited primary file.
- No build, import, test, linter, socket, receiver/WM start, DB, game or visual check ran.

## Convergence consumer - SOURCE-INTEGRATED, UNCOMPILED, RUNTIME-UNVERIFIED

- `docs/opencode/convergence-consumer.md` is the stable producer/consumer contract and supersedes
  `docs/opencode/wm-native-bridge.md` for API and wire shape.
- Observation schema is now `wm.native.observation.v2`: explicit `target` unknown/none/target,
  explicit bounded `visibility` completeness, source `state_revision` plus `observed_at` separated
  from `exported_at`, and nonzero `session_epoch`.
- Rust adds `ObservationPublisher`: a latest-only bounded handoff with `try_lock`, one background
  I/O worker, lazy connect, explicit reconnect backoff, and no socket work in `publish`. A busy
  handoff returns a bounded `RetainedObservation` with unchanged source time/revision; the bridge
  supports caller retry, while the integration pump uses the producer as sole retry owner.
  `retire`/`spawn_after` order replacement workers without joining on the packet path.
- The bridge now has `ProducerSnapshot`/`observation_from_producer` and `ProducerSession`/
  `pump_once`, mapping GLM's published v2 API without a mirrored layout: direct `state_revision`,
  all three `SelfTarget` states, full `SelfVisibility` fields, `position.o` as orientation, and
  passed-through optional zone/area. `Retained` is not acknowledged; only `HandedOff` calls
  `mark_published`, and `Superseded` is an ordering error.
- Python adds per-connection ordering reset, disconnect/idle/protocol/shutdown invalidation,
  source-freshness enforcement that rejects unchanged-state timestamp renewal, and matching v2
  validation/context payload.
- `integrations/wm-native/src/wm_native/runtime.py` adds one reusable lifecycle composition:
  one store, one receiver, one loader, idempotent shutdown, typed startup failure, and fallback
  composition. It wraps the existing receiver/store/adapter rather than adding another service.
- GLM v2 is clean at `6ae14b3b` (`71aefc2e` producer, `6ae14b3b` cleanup), with
  `docs/convergence/glm-producer.md` published. The bridge conversion and pump are implemented; the
  small GLM application newtype/impl and WM startup glue remain owner-applied.
- No production WM source was edited. The receiver entrypoint and exact `ContextPackBuilder`
  insertion remain documented but unapplied; the bridge is not WM-integrated.
- No cargo/Python execution, import, build, test, formatter, linter, type check, socket, or live
  session was performed. Source-integrated is not compiled or runtime-verified.
- Convergence handoff defers further live-change orchestration and combat expansion. Remaining
  convergence work is the owner-applied GLM application impl/driver crate and WM startup glue.

## Preserved F08

- `crates/benilla-profile/src/lib.rs` keeps the completed candidate-first reload: manifest and all
  archives are prepared before either active field is replaced; any failure leaves the old profile
  in place. This file is outside queue 02's new paths and was not edited further.

## A - combat donor assessment: COMPLETE (source-observed)

- `docs/opencode/combat-donor-assessment.md`.
- Primary donor `Funny-Bones/ELDEN-RING-Combat-Rewrite` at
  `177a3b5ec14a9bde39b9ab458a3400b4038224e3`, MIT. Comparator `TeaYololo/soulslike` at
  `5824f4b885ff77fd024171b2b8d7aed198de7ce0`, Apache-2.0.
- Documents inspected mechanisms, absent generated data/assets, Bevy/client-authority coupling,
  licence/provenance, and a per-subsystem reuse/port/avoid decision.
- No donor or third-party source was copied, executed, or built.

## B - combat core: IMPLEMENTED, source-review only

- `crates/benilla-combat-core/Cargo.toml`
- `crates/benilla-combat-core/src/{lib,time,identity,error,data,stamina,player,enemy,input,core}.rs`
- `docs/opencode/combat-core-integration.md`
- Dependency-free, integer-tick offline model: one player, one telegraphed enemy, release dodge
  versus hold sprint, one-slot newest-wins buffer with TTL, input/cancel/i-frame windows,
  stamina spend/clamp/regen delay, interruption/death/reset/reconnect, sequence/session checks,
  bounded deferred input, snapshots and strict reconciliation.
- Server call graph recorded with exact read-only paths/lines for melee, direct spell, periodic,
  split, shield/absorb, and environmental damage, plus irreversible outputs and the reason a
  global damage early-return is forbidden.
- No server file was patched. No rendered combat, collision, animation retargeting, network
  carrier, or live dodge proof exists.

### B limitations / required next actions

- No tests, build, formatter, linter, type check, benchmark, or execution was allowed, so the new
  crate has **not been compiled**. Review was manual.
- Root `Cargo.lock` was deliberately not touched. A later allowed build must add the new workspace
  member; `--locked` will fail until that owner-approved step.
- `CombatConfig` values are caller-authored. None is an extracted Elden Ring value.

## C - native WM bridge: IMPLEMENTED SEAM, PARTIAL END-TO-END

- `docs/opencode/wm-native-bridge.md`.
- `crates/benilla-wm-native` implements a dependency-light, loopback-only, length-prefixed JSON
  sink: strict `hello`/`hello_ack`, required owner token, bounded frames, versioned observation,
  player/session/sequence/uptime validation. The low-level `LoopbackSink` remains one-shot; the
  convergence checkpoint adds a separate bounded `ObservationPublisher` with a background I/O
  worker and explicit reconnect backoff.
- `integrations/wm-native/src/wm_native` implements a one-client receiver, strict handshake and
  frame parsing, latest-only store, receipt/read-time freshness checks, and the
  `NativeContextSnapshotLoader`-compatible adapter plus a read-only native-first/DB-fallback
  composition.
- Transport choice is recorded in the bridge doc: raw loopback TCP rather than ticket 11's
  named-pipe/WebSocket examples, because WM has no WebSocket dependency and the stdlib-only stream
  avoids new Python deps. This deviation is explicit; the observation shape and WM adapter are
  transport-independent.
- No production WM file was edited. Exact proposed diffs for `ContextPackBuilder`, `pack.py`, and
  `autoplay/world_context.py` are in the bridge doc, not applied.
- The native client's ticket-08 reducer export does not exist in this checkout, so no live session
  is read end-to-end and no client source was wired. C remains PARTIAL; ticket 11's live-stream and
  no-second-director acceptance is not yet established.
- The new crate has not been compiled and the Python package has not been imported/run. Root
  `Cargo.lock` remains untouched; an owner build must add the new member/deps, and `--locked` will
  fail until then.

## D - live-change proof preparation: COMPLETE (source-observed preparation only; deferred)

- `docs/opencode/live-change-integration.md`.
- Corrected the ticket-03 contract's upload-expiry claim: `Now()` is
  `Multiclass::BonusXpNow()`, which returns steady-clock milliseconds
  (`MulticlassBonusXp.cpp:317-320`). `BEGIN` and each accepted `CHUNK` set `Now()+30000`
  (`SpellmakingV3.cpp:346-347`); therefore upload expiry is a **30 s inactivity deadline refreshed
  per chunk**, not 300 s. The fixed quote window remains `Now()+120000` = 120 s (`:350`). The
  contract correction is proposed in the D document, not applied because `docs/live-rules/` is
  outside this queue's edit allowlist.
- Pinned and recorded graph A/B SHA-256 values, SpellmakingV3/Graph/BonusXp source hashes, and the
  byte-identical `wm-project` / `WM_BridgeLab` C++ copies at inspection time.
- Recorded separate source-observed lanes: `SAVE` inserts a library revision only; `BUY` allocates
  a new immutable spell/carrier identity and calls `Configure` immediately; instances pin their
  `shared_ptr<Design>`; no DB-plus-reload verb exists; `DELETE` retires B and ends its instances,
  not A.
- Defined the exact Step 0-7 proof sequence, four-point evidence split (1-2 platform versus 3-4
  WM/selection), invalid-B fixture/rejection, timestamped duplicate-pulse key, no-reboot identity
  fields, and a schema with all proof values left unpopulated.
- Documented the two missing gates: server-side `SpellmakingV3::SubmitCommand` / bridge verb, and
  WM-owned `logical_action_id -> spell_id` binding. Manual addon authoring is not a WM publication
  adapter.
- No proof runner was added: with both publication/selection gates missing, a runner would raise
  immediately or fake progress. The D document instead fixes the injected port contract for a later
  implementation.
- No server, DB, WM, client, addon, or game process was started. No quote, purchase, save, delete,
  cast, selection, runtime server PID, running-binary hash, or pulse evidence was observed.
- Runtime ticket-10 proof remains pending under explicit owner authorization.

## Next authorized item

- None under this queue's current constraints. The next work is the runtime B-001B-2 / ticket-10
  proof, which requires an owner-authorized lab run plus the missing dispatcher and WM-binding
  adapters. Do not treat this source-review preparation as that proof.

## Unexecuted validation (entire queue)

Everything is source-level. No `cargo` command, Python run, test, probe, packet capture, server
launch, DB/SOAP call, game launch, or WM mutation has been performed. F08, the combat core, the WM
bridge, and any later proof-preparation code remain unvalidated at runtime.
