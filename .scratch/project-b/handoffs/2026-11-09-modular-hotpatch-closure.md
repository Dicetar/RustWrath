# Modular hotpatch closure — 2026-11-09

Scope: source-only modular/hotpatch seam repairs. No Cargo check/build/test, clippy, fmt,
client/server launch or visual acceptance was run for any item below; `git diff --check`
plus source inspection are the only checks.

## Closed on `origin/main`

### 1. Movement seam capability gate — `653c104e`
Branch `fix/modular-seam-gate-a`.

- Problem: `ModularGameplayRegistry` was dead code. `Host` had no registry and
  `Host::stage_movement_processor` read `movement.toml` and opened the artifact regardless of
  the Movement seam, contrary to ADR-0002 §4.3.5 ("before `Library::new`").
- Fix: `Host` owns `seams`; `Host::set_seam_enabled`/`is_seam_enabled`;
  `MovementRefusal::SeamDisabled`; gate runs on a present manifest before any artifact bytes
  or `Library::new`. A disabled publication stays latched so retention pins it, repeated
  unchanged polls are quiet, disabling evicts, enabling clears the latch, and
  `movement_proposal`/`commit_movement_processor` refuse while disabled.
- ZAEBAL: two independent auditors plus post-fix re-checks; closed at source/API. The
  production movement adapter remains absent (P1, separate).

### 2. Terminal pose poll latch — `fc22d880`
Branch `fix/hotpatch-terminal-poll-latch-a`.

- Problem: `Host::poll` cleared `observed` on every `Loaded::open` failure, so a
  deterministic bad artifact was re-read and `LoadLibrary`-ed every second.
- Fix: `transient_io` allowlist (`NotFound`, `PermissionDenied`, `Interrupted`, `WouldBlock`,
  `TimedOut`, Windows sharing/lock 32/33) and a held `publish.lock` stay retryable; malformed
  TOML and deterministic `verify_publication`/`Loaded::open` failures latch `observed`, return
  `Err` once, then stay quiet until the manifest text changes. The shared candidate rejection
  channel is untouched.
- ZAEBAL: closed at source/API.

### 3. Bounded pose digest quarantine — `7e625a8a`
Branch `fix/hotpatch-digest-quarantine-a`.

- Problem: the text-only latch let a `B -> C -> B` rewrite re-read and load the same
  deterministically bad digest.
- Fix: `verify_publication` is split from `Loaded::open`; only a digest proved against the
  artifact bytes can enter the bounded in-memory `Host.quarantined_digests`
  (`VecDeque<String>`, cap 32, dedupe, oldest eviction). The gate runs after TOML parsing and
  before verification/`Library::new`; unchanged text is quiet. Pre-verification
  basename/revision/size/digest/filename failures latch text only, and transient I/O, a held
  `publish.lock` and malformed TOML neither latch nor quarantine.
- ZAEBAL: two independent auditors; final re-check `CLOSED` at source/API (build/test
  evidence `UNVERIFIED`).

### 4. Movement terminal latch and quarantine — `059f5aa7`
Branch `fix/hotpatch-movement-latch-a`.

- Problem: `stage_movement_processor` cleared `movement_observed` before parsing and never
  re-latched a deterministic TOML or `MovementLoaded::open` failure, so the same bad artifact
  was re-read and `Library::new`-ed at 1 Hz; a `B -> C -> B` rewrite reloaded the same digest.
- Fix: `MovementLoadFailure` carries `transient` and `verified_digest`; verification failures
  can never quarantine. A terminal post-verification failure adds the bytes-verified digest to
  the bounded `movement_quarantined_digests` (`VecDeque<String>`, cap 32, dedupe, oldest
  eviction), refused before any artifact read/`Library::new`. Deterministic parse and open
  failures latch `movement_observed`; transient I/O and a held `publish.lock` retry unchanged;
  a failed candidate leaves an active processor running.
- ZAEBAL: two independent auditors; `CLOSED` at source/API. The Unix `libloading` terminality
  and malformed-TOML-under-lock wording were corrected in the ADR. No direct test for the
  missing-entry/null-descriptor/ABI branches (requires a built module); build/test
  `UNVERIFIED`.

### 5. Pose evaluation quarantine — `d37d7778`
Branch `fix/hotpatch-canary-quarantine-a`.

- Problem: a pose module that loaded but failed its canary batch, a later active frame, or a
  restore evaluation was never added to the bounded digest quarantine, so a `B -> C -> B`
  rewrite could restage a module that had already produced invalid output. A failing restore
  predecessor could also be retried.
- Fix: quarantine the verified digest on canary rejection, active-frame failure and failing
  restore predecessor; drop retained copies; skip an already-quarantined predecessor before
  invoking it. Add `#[cfg(test)] Loaded::probe` (mirroring `MovementLoaded`) so the path is
  testable without a native DLL. Validate finite frame time, the i32 parent-index cap, and
  parent bounds/precedence within each rig span before dispatch so malformed caller frames
  cannot cause a false quarantine.
- ZAEBAL: two independent auditors; `CLOSED` at source/API. Parent precedence is stricter than
  the baseline's tolerant forward-parent composition and is documented. Single-strike
  quarantine is the ADR `max_consecutive_frame_failures = 1` disposition; health counters and
  span coverage remain H03. Build/test `UNVERIFIED`.

### 6. Movement proposal-failure quarantine — `27953226`
Branch `fix/hotpatch-movement-eval-quarantine-a`.

- Problem: `movement_proposal` dropped the movement slots on a module `Status`/output-validation
  failure without quarantining the selected digest, so a `B -> C -> B` manifest could restage a
  processor that already produced an invalid proposal.
- Fix: `movement_process` returns the selected verified digest; `Status` and
  `validate_movement_output` failures quarantine it before `drop_movement`. `FrameInput`,
  `NoModule`, `SeamDisabled`, valid proposals/commits do not quarantine. Validate caller frame
  scalars (except the `liquid_height` `-INFINITY` sentinel), bounds (non-negative, `+INFINITY`
  unbounded sentinel), and caller-seeded facing/speeds before dispatch. Input `move_flags` may
  carry baseline/server state; only output flags are checked.
- ZAEBAL: two independent auditors; `CLOSED` at source/API after removing an input-flags
  over-rejection and adding active-slot coverage. Frozen: a staged proposal failure and a
  `FrameInput` error still drop both slots (including a healthy active processor), unlike pose.
  Build/test `UNVERIFIED`.

### 7. Same-text artifact repair — `875f4ac3`
Branch `fix/hotpatch-same-text-repair-a`.

- Problem: pose/movement terminal failures latched only the manifest text, so replacing the DLL
  bytes in place without changing `ready.toml`/`movement.toml` was never retried.
- Fix: record an in-memory `ArtifactStamp` (normal basename, length, optional mtime) on a
  terminal failure with a parsed publication, captured before verification reads the bytes. An
  identical text clears the latch and retries once when length/mtime changed or the file is
  missing; a matching stamp stays quiet. Malformed TOML, the digest gate and the disabled-seam
  path stay text-only. A missing non-quarantined artifact retries transiently and suspends the
  sweep while unlatched; a missing quarantined artifact is gated, re-latched text-only, and the
  sweep runs. Tests cover length-only, mtime-only, missing, quarantined-missing, unchanged,
  malformed and basename cases for both seams.
- ZAEBAL: two independent auditors; source/API `CLOSED` for the named `(text, len, mtime)`
  detector. Remaining: same-length + unchanged/unavailable mtime can still be missed, a
  capture-time `stat` failure leaves a text-only latch, and the stamp is in-memory only. Build/test
  `UNVERIFIED`.

### 8. Movement retention on caller/staged failures — `7fd994bb`
Branch `fix/hotpatch-movement-retain-a`.

- Problem: `movement_proposal` cleared both movement slots on a caller `FrameInput` error and on
  a failing staged candidate, evicting a healthy active processor unlike pose's candidate
  rejection.
- Fix: `FrameInput`/`NoModule` leave the staged/active selection intact; a staged module
  `Status`/output-validation failure quarantines the failed digest, clears only `movement_pending`
  and retains a healthy `movement_active` unless its digest matches the failed content. An active
  failure still clears both because the movement seam keeps no retained predecessor.
  `probe_with_digest` enables distinct staged/active test identities.
- ZAEBAL: two independent auditors; host source/API `CLOSED`. Integration caveat: the only
  in-tree controller (`crates/benilla-app/src/player/controller.rs`) is single-slot and cannot
  express a refused staged candidate beside a retained active; a production adapter remains
  future work. `commit_movement_processor` also still trusts the caller not to commit without an
  accepted proposal (pre-existing). Build/test `UNVERIFIED`.

### 9. Pose rig-span tiling — `f3899be4`
Branch `fix/hotpatch-pose-span-tiling`.

- Problem: `Host::evaluate` accepted any in-bounds `RigSpan` list, so gaps, overlaps, reordered
  spans or an empty span list with bones could reach a module; unspanned bones received only the
  batch-wide parent bound.
- Fix: for a non-empty bone batch, spans must tile the batch in order (`first == covered`,
  checked count addition, final `covered == bones.len()`); zero-count spans at the current
  coverage advance nothing. The check runs after bounds/finite checks and before
  `pending`/`active` or module dispatch, so malformed caller frames never quarantine.
- ZAEBAL: two independent auditors; source/API `CLOSED`; only latent API hardening (no in-tree
  producer emitted malformed spans). Build/test `UNVERIFIED`.

### 10. Movement commit gate — `8fc2a9d8`
Branch `fix/hotpatch-movement-commit-gate-a`.

- Problem: `commit_movement_processor` promoted any staged candidate without a successful
  proposal; `FrameInput` retention made that reachable after a failed frame.
- Fix: `movement_proposal_ready` is cleared on every proposal, stage/selection change, unload,
  refusal, rejection and seam toggle; only a valid `Staged` proposal arms it. Commit requires
  the arm and leaves a retained candidate staged when unarmed. `withdraw_movement_proposal`
  gives the controller a rejection handback that disarms without dropping the candidate.
- ZAEBAL: two independent auditors; reported commit-without-proposal defect `CLOSED` at
  source/API. Residual: controller acceptance is still caller-enforced (the adapter must call
  `withdraw` on rejection/drain); no production adapter; the withdrawal is unscoped (no proposal
  generation token). Build/test `UNVERIFIED`.

### 11. Pose health snapshot — `41ba7093`
Branch `fix/hotpatch-pose-health-snapshot`.

- Problem: H03 specifies per-active-module health counters/timing and a read-only
  `PatchHealthSnapshot`; `Host` exposed neither.
- Fix: private `PatchHealth` + non-exhaustive public snapshot (active revision/digest,
  consecutive/total failures, last/max active-eval microseconds, pose quarantine count). Only
  steady-state active evaluation is timed; success resets consecutive, failure increments both
  saturating counters, activation/restore resets the new module; no-active keeps the last counters
  with `None` identity. Candidate canary failure does not itself touch active health; the
  same-frame retained-active fallback still does.
- ZAEBAL: two independent auditors; `CLOSED` at source/API for the scoped four-field in-memory
  slice. Full H03 remains: persistence, threshold enforcement, typed reasons,
  `canary_frames_remaining`, movement health, build/test.

### 12. Configurable pose health thresholds — `fe9bf436`
Branch `fix/hotpatch-pose-health-policy-a`.

- Problem: `Host::evaluate` demoted/quarantined an active module on the first failure
  unconditionally; H03's `max_consecutive_frame_failures`/`max_total_frame_failures` policy was
  not represented.
- Fix: public `PatchHealthPolicy` (defaults 1/3, preserving single-strike; `0` disables a
  threshold) with Host accessors. Below both thresholds an active failure keeps the active and
  previous slots, does not quarantine, clears the failed output, records `active_failure` and
  returns `Ok(false)` so the caller renders baseline for that frame; reaching either threshold
  keeps the existing quarantine/demote/restore path.
- ZAEBAL: pending post-merge audit (worker/reviewer complete). Persistence, typed reasons,
  `max_eval_micros` enforcement, canary windows and movement health remain H03.

- Follow-up `1b0c330d`: the policy audit found the Host branch correct but the ADR still
  described unconditional demotion/no `max_total` enforcement, and a tolerated failure did not
  force a retry. The ADR current-behavior paragraphs, symbol/test rows and stale test comment
  were reconciled; `apply_pose` now sets `runtime.refresh` for a tolerated failure with no
  restore outcome so the retained module is evaluated again on the next frame.
- ZAEBAL (policy): source/API `CLOSED` after the follow-up; remaining gaps are persistence,
  manifest/app policy wiring, per-revision cumulative totals, `max_eval_micros`, canary windows,
  typed reasons, movement health and build/test.

### 13. Persistent digest quarantine — `2e1870f7`
Branch `fix/hotpatch-persistent-quarantine-a`.

- Problem: both digest quarantines were in-memory only, so a restart forgot a deterministically
  bad digest and a `B -> C -> B` publication could reload it.
- Fix: digest-only `<root>/quarantine.toml` loaded by `HotpatchPlugin` at startup and saved after
  the 1 Hz poll. Exact 40-hex normalized entries, invalid/path-like/duplicates skipped, newest-32
  window, cap/dedupe/eviction through the existing stores, 64 KiB read cap, temp-write + sync +
  rename (no delete window). Load is fail-open with a dedicated warning slot; save failure never
  stops the poll or seams. Tests cover round-trip, missing/malformed/oversized, invalid/duplicate,
  cap/reload, stale temp, failed replacement and merge; ADR documents save window, single-client
  scope and manual-delete recovery.
- ZAEBAL: source/API `CLOSED` for the digest-list slice; full H03 remains partial (persistent
  health/policy/artifact stamps, sentinel, typed reasons, per-revision totals, movement health,
  receiver/build/test). `AUTO_swarm` fallback: Flint and Beam both failed on free usage limit;
  DeepSeek 4.1 Flash completed the two-agent batch.

### 14. Pose loading + canary crash sentinel — `6e978262`

Branch `fix/hotpatch-active-sentinel`; merged into `origin/main` by `f1692be1` on top of the
other owner's `771eed34` (WMBridge/LFG/talent work), so the modular commit is not a linear main
tip.

- Problem: a native fault inside `Library::new`/entry/header validation or inside the staged
  candidate's canary `process` call left no durable marker, so the next launch could retry the
  same bad digest. H03 specifies `<root>/active-sentinel.toml` for exactly those windows.
- Fix: digest-only `{ revision, sha1, kind = "pose", stage = "loading" | "canary" }` marker.
  `Host::poll` writes the loading marker after `verify_publication` and before `Loaded::open`,
  clears it immediately after open returns; `Host::evaluate` writes the canary marker immediately
  before the staged candidate's `Loaded::evaluate`, clears it immediately after that call returns
  on every regular path (promotion, rejection, output validation). Temp write + `sync_all` +
  rename, no library path or OS text; 16 KiB read/write cap; write failures are fail-open and
  never skip a load; a clear only follows a marker this call wrote, and both errors use the
  dedicated `Host.sentinel_error` slot.
- Recovery: `Host::load_active_sentinel` accepts pose `loading`/`canary`, adds the digest to the
  bounded pose quarantine, and leaves the marker pending. `persist_quarantine` commits the store
  first and only then clears the marker/pending; a persist failure retains the marker, a later
  load commits any pending recovery before replacing it, and `poll` cannot clear a retained
  recovery marker after a failed write. Malformed/unknown-field/invalid-digest/oversized files and
  a future movement kind stay errors with the file and both stores unchanged; plugin startup warns
  once and never fails.
- Tests: dirty loading/canary recovery + persist, missing, malformed/unknown/invalid/future-kind/
  oversized, writer cap, write/clear/missing-clear, persist-failure marker retention, pending
  recovery not overwritten, poll keeps a pending marker after its commit fails, successful and
  rejected canaries clear the marker, a panic inside the canary leaves a recoverable marker, and a
  regular pose load failure leaves no marker.
- ZAEBAL: two independent audits found and closed two real ordering defects (clear-before-persist
  and `poll` clearing a retained recovery marker). Source/API `CLOSED` for the pose loading and
  canary windows. Remaining H03: native faults during steady-state active evaluation and the
  predecessor restore probe, the movement sentinel, clean-shutdown marker semantics, typed
  reasons, persistent health/policy/artifact stamps, per-revision totals and build/test evidence.
  No Cargo/build/test/client run was performed.

### 15. Pose canary-window crash sentinel — `5dc0a766`

Branch `fix/hotpatch-canary-sentinel-a`; merged into `origin/main` by fast-forward `f1692be1..5dc0a766`.

- Problem: the loading marker was cleared when `Loaded::open` returned, so a native fault inside the
  staged candidate's canary `process` call in `Host::evaluate` left no marker and the next launch
  could retry the bad digest.
- Fix: `Host::write_active_sentinel` now takes `(revision, sha1, stage)`; `Host::evaluate` writes
  `stage = "canary"` immediately before the staged candidate's `Loaded::evaluate` and clears it
  immediately after that call returns on every regular path, gated on the marker actually being
  written. Startup recovery accepts pose `loading` and `canary`, quarantines the digest into the
  bounded pose store, and retains the marker until `persist_quarantine` commits; a pending recovery
  is committed before any replacement marker and is never overwritten on failure. A failed canary
  write is fail-open and reported through `sentinel_error`; a failed clear leaves a stale marker
  that can quarantine a healthy digest and is documented.
- Tests: canary recovery/persist, successful and rejected canary clears, an observing probe that
  parses the marker with the candidate revision/stage/digest during the module call and asserts it
  is gone afterwards, blocked-writer fail-open with no clear attempt, pending-recovery retention
  during a canary, plus the existing loading tests.
- ZAEBAL: two independent source/API audits confirmed the canary `process()` window closed for the
  normal write path. Remaining H03: native faults during steady-state active evaluation, predecessor
  restore and `Loaded` drop/`FreeLibrary`/DLL-detach; canary-rejection digest persistence is still
  the next 1 Hz save; movement sentinel, typed reasons, persistent health/policy/artifact stamps,
  per-revision totals and build/test evidence. No Cargo/build/test/client run was performed.

### 16. Movement loading-window crash sentinel — `c46e4bb8`

Branch `fix/hotpatch-movement-sentinel-a`; merged into `origin/main` by fast-forward `5dc0a766..c46e4bb8`.

- Problem: `MovementLoaded::open` verified the artifact and called `Library::new`/`entry()` with
  no marker, so a native fault during a movement load left no durable marker and the next launch
  could retry the bad movement digest.
- Fix: split `MovementLoaded` into `verify` (exact former `verify_publication` mapping) and
  `open(root, path, digest, publication)`; `Host::stage_movement_processor` writes
  `{ revision, sha1, kind = "movement", stage = "loading" }` after verification and before
  `MovementLoaded::open`, clears it when open returns, and gates the clear on the marker actually
  being written. Startup recovery routes `Movement`/`Loading` into
  `movement_quarantined_digests`, retains the marker until `persist_quarantine` commits, and rejects
  `Movement`/`Canary` fail-safe (future slice at the time). Write/clear failures are fail-open and
  use the shared `sentinel_error` slot; the writer gained an explicit `kind` parameter.
- Tests: movement recovery/persist, future-canary rejection, digest-only writer content,
  regular-load-failure cleanup, blocked-writer fail-open, recovered-digest gate before artifact
  read, and pose/movement routing isolation.
- ZAEBAL: two source/API audits closed the movement loading window. Named gaps: no production
  caller for `stage_movement_processor` (so the marker is unreachable in the alpha until the
  movement adapter lands), no in-window movement probe (presence is inspection-only),
  `Host::movement_proposal`'s `process` window, pose active/restore/drop native faults,
  canary-rejection save ordering, typed reasons, persistent health/policy/artifact stamps and
  build/test. No Cargo/build/test/client run was performed.

### 17. Movement staged-proposal canary sentinel — `1b995444`

Branch `fix/hotpatch-movement-canary-a`; merged into `origin/main` by fast-forward `c46e4bb8..1b995444`.

- Problem: `Host::movement_proposal` called the selected movement `process` pointer with no
  marker, so a native fault during the first staged (canary) proposal left no durable marker and
  the next launch could retry the bad movement digest.
- Fix: a private one-shot `movement_canary_attempted` guard; `Host::movement_proposal` writes
  `{ revision, sha1, kind = "movement", stage = "canary" }` immediately before the first staged
  `process` call and clears it immediately after the call returns, before status/output handling.
  Later staged proposals and active per-frame calls stay unmarked; a failed write is fail-open,
  consumes the one-shot, and never clears a pending-recovery marker. Startup recovery accepts
  `Movement`/`Loading` and `Movement`/`Canary`, routes both to the movement quarantine, and rejects
  unknown stages fail-safe.
- Tests: observing probe parses the marker with the candidate identity during the first staged
  call and asserts it is gone afterwards; one-shot second proposal writes none; active proposal
  observes no marker during the call; rejection clears/quarantines; blocked write is fail-open and
  not retried; unknown stage fails safe; unload resets the one-shot; recovery/persist covers the
  movement canary; the observing tests share a serializing test lock and ordinary movement probes
  use unique roots.
- ZAEBAL: two source/API audits closed the first-staged-canary window. Remaining: later pre-commit
  staged proposals, active movement per-frame `process` markers, movement health, pose
  active-frame/restore/drop native faults, clean-shutdown marker semantics, typed reasons,
  persistent health/policy/artifact stamps, the missing production movement adapter and build/test.
  No Cargo/build/test/client run was performed.

### 18. Clean-shutdown sentinel safety net — `c53eb976`

Branch `fix/hotpatch-shutdown-sentinel-a`; merged into `origin/main` by fast-forward `1b995444..c53eb976`.

- Problem: a failed call-scoped sentinel clear could leave `active-sentinel.toml` behind; a normal
  client exit then left it for the next launch to quarantine a healthy digest.
- Fix: `Host::clear_active_sentinel_on_clean_shutdown` retains a marker while
  `sentinel_recovery_pending` is set or while the marker was neither written by this host nor
  accepted by `load_active_sentinel`; otherwise it clears. `Host.sentinel_marker_owned` tracks
  writes, validated recovery, failed reloads and clears, so an unvalidated/unloaded/foreign marker
  is never deleted by the clean-exit path. `Runtime::drop` calls the method on an orderly `App`
  drop, skips `std::thread::panicking()`, warns on retained/error outcomes and never fails shutdown.
  A non-`NotFound` metadata error is returned rather than treated as absence.
- Tests: stale owned clear, pending-recovery retention then persist, unvalidated retention,
  unloaded retention, missing marker, failed-reload ownership reset, and no `sentinel_error` on the
  clean-exit path.
- ZAEBAL: two source/API audits closed the scoped single-host, non-panicking `App`-drop safety net.
  Remaining residuals: explicit `AppExit`-edge detection (Drop also runs on bare `App` drops),
  cross-process/file-identity ownership, a second clear failure still leaves the marker,
  `std::process::exit`/hard kills bypass Drop, later pre-commit staged and active per-frame native
  markers, pose active/restore/drop, typed reasons, persistent health/policy/artifact stamps, the
  production movement adapter and build/test. No Cargo/build/test/client run was performed.

### 19. Explicit AppExit sentinel edge — `ae0df8b8`

Branch `fix/hotpatch-appexit-sentinel-a`; merged into `origin/main` by fast-forward `c53eb976..ae0df8b8`.

- Problem: the clean-shutdown sentinel safety net ran only from `Runtime::drop`; probe/capture
  harnesses that announce `AppExit` and then call `std::process::exit` bypass Drop, and a pose
  panic on the exit frame could skip the clear.
- Fix: a `Last` system `clear_active_sentinel_on_app_exit` reads `MessageReader<AppExit>` and calls
  `Host::clear_active_sentinel_on_clean_shutdown`, ordered before `capture_pose` (the pose chain
  already orders capture before restore). `Ok(false)` stays quiet because pending/unowned markers
  are retained and Drop is the loud normal-exit path; `Err` warns. Drop remains the fallback for a
  bare `App` drop and keeps the `std::thread::panicking()` guard.
- Docs: corrected the AppExit schedule (window close/console `quit` -> `WindowCloseRequested` ->
  `PostUpdate`; chat `/quit`/Exit Game -> `Update`; dev-only `sched_census`/`drive_capture` write
  in `Last` and are not guaranteed to be seen), named the probe hard-exit `Ok(false)` silence and
  the panic-before-clear residual.
- ZAEBAL: two source/API audits closed the explicit player-exit edge. Remaining: dev-only `Last`
  writers, a panic in another `Last` system before the clear, `process::exit`/hard kills that never
  announce, cross-process/file-identity ownership, a second clear failure, later/active native
  markers, pose active/restore/drop, typed reasons, persistent health/stamps, production adapter
  and build/test. No Cargo/build/test/client run was performed.

### 20. Content-bound sentinel ownership — `b607834e`

Branch `fix/hotpatch-sentinel-identity-a`; merged into `origin/main` by fast-forward `ae0df8b8..b607834e`.

- Problem: `sentinel_marker_owned` was a sticky bool. After this host wrote a marker, a foreign
  replacement was still treated as owned, so clean shutdown and the `persist_quarantine`
  recovery-commit retry could delete it.
- Fix: `ActiveSentinelFile` derives `Clone`/`PartialEq`/`Eq`; ownership is
  `Option<ActiveSentinelFile>` set only after a successful write or validated load, cleared on a
  successful/NotFound clear. Both the clean-shutdown and recovery-commit paths call
  `clear_owned_active_sentinel_if_matching`, which caps the re-read at 16 KiB, parses and compares
  revision/sha1/kind/stage, clears only an exact match, retains foreign/malformed/oversized
  markers, and treats a missing owned file as clean while dropping ownership.
- Tests: matching clear, same-digest/different-revision retention, different-digest retention,
  malformed + oversized retention, persist-foreign retention, missing-owned clean, plus the
  existing pending/unvalidated/unloaded/failed-reload cases.
- ZAEBAL: two source/API audits closed the content-bound ownership slice. Remaining: read/compare/
  unlink TOCTOU, a same-parsed-identity (reformatted) replacement, path-based call-scoped clears,
  cross-process root ownership, a second clear failure, AppExit dev-only `Last` writers, later/
  active native markers, pose active/restore/drop, typed/persistence, production adapter and
  build/test. No Cargo/build/test/client run was performed.

### 21. Probe sentinel/quarantine parity — `4fafb1ab`

Branch `fix/hotpatch-probe-sentinel-parity-a`; merged into `origin/main` by fast-forward `b607834e..4fafb1ab`.

- Problem: the standalone `hotpatch-probe` dev instrument created `Host::new` and never loaded the
  persistent digest quarantine, recovered a dirty `active-sentinel.toml`, or saved the quarantine;
  its behavior diverged from `HotpatchPlugin`.
- Fix: load `quarantine.toml`, call `load_active_sentinel`, persist a recovered digest, drain the
  recovery clear error immediately (including 0/1 s runs), persist and drain deduped quarantine/
  sentinel errors at 1 Hz, and perform a final flush. Retention/persistence failures are printed
  and no longer fail the probe. The probe docs/ADR warn that it has no root lock and must use an
  isolated module root.
- ZAEBAL: two source/API audits closed the probe-parity issue. Remaining: dev-only probe, no
  runtime/automated test or build, shared-root hazard documented but not mechanically prevented,
  and the broader H03 list below. No Cargo/build/test/client run was performed.

### 22. Typed pose publication-rejection reasons — `c73647ce`

Branch `fix/hotpatch-publication-reasons-a`; merged into `origin/main` by fast-forward `4fafb1ab..c73647ce`.

- Problem: `HotpatchEvent::PublicationRejected` always carried the coarse
  `HotpatchReason::Publication`; the host exposed only free-form errors.
- Fix: public `PatchRejectReason` (`TransientIoPending`, `ManifestUnreadable`,
  `ManifestMalformed`, `ArtifactRejected`, `Quarantined`, `LoadFailed`, `Unknown` fallback),
  `Host.publication_reason`/`take_publication_reason`, and assignment on every `Host::poll` error
  path. The reason is cleared at poll start; quiet `Ok(false)` leaves it `None`. The Bevy event
  carries `detail: PatchRejectReason` alongside the coarse reason and emits when either the
  message text or the typed detail changes, so a transient-to-terminal transition with identical
  text is not swallowed. `benilla-app` ignores the detail with `..`.
- Tests: malformed TOML, non-UTF-8, missing artifact transient, quarantine gate, digest mismatch,
  non-PE load failure, take-clears, and no-manifest quiet. `PatchRejectReason` is
  `#[non_exhaustive]`; the event variant is rustfmt-width formatted.
- ZAEBAL: two source/API audits closed the pose publication-detail slice. Remaining: canary/
  active/movement/rollback typed reasons, finer target codes (`DigestMismatch`, `AbiMismatch`,
  `MissingDependency`, etc.), `IncidentOutcome`/quest-trace detail (the app still discards the
  detail), persistent health/policy/artifact stamps, an adapter event test and build/test.
  No Cargo/build/test/client run was performed.

### 23. Typed publication reasons in the recorder — `67fce080`

Branch `fix/hotpatch-reason-trace-a`; merged into `origin/main` by `f81122ee` (which also integrates
the other owner's `2f6ad07b` / `85f66fe0` pet-talent/whisper work).

- Problem: `benilla-app` discarded `HotpatchEvent::PublicationRejected.detail`, so
  `quest_trace::HotpatchReason` stayed coarse and the recorder/incident output lost the typed
  publication reason.
- Fix: re-export `PatchRejectReason` from `benilla-hotpatch-bevy`; add the detailed variants to
  `quest_trace::HotpatchReason`; map the event detail into `PatchEvent::PublicationRejected` with
  a coarse fallback only for `Unknown`/future variants. The detail reaches the recent trace,
  incident `terminal_stage` and human summary. Publication rejections are attempt-less, so they do
  not appear in the attempt-correlated Hotpatch Attempts summary; the serialized `reason` value
  changes from `"Publication"` to the detailed variant.
- ZAEBAL: two source/API audits closed the trace-carry slice. Remaining: canary/active/movement/
  rollback typed reasons, finer target codes (`DigestMismatch`, `AbiMismatch`, `MissingDependency`,
  etc.), non-publication `IncidentOutcome` detail, persistent health/policy/artifact stamps,
  attempt correlation for publication rejections, mapping tests and build/test. No
  Cargo/build/test/client run was performed.

### 24. Fine publication rejection codes — `370383c6`

Branch `fix/hotpatch-fine-reasons-a`; merged into `origin/main` by fast-forward
`f81122ee..370383c6`.

- Problem: `PatchRejectReason` had only the coarse `ArtifactRejected`/`LoadFailed`, so every
  `verify_publication` and `Loaded::open` failure produced the same recorder code.
- Fix: private `PatchRejection { reason, detail, inner }`. `verify_publication` maps invalid
  basename, empty revision, oversized artifact, digest mismatch and wrong immutable filename to
  `InvalidBasename`, `EmptyRevision`, `ArtifactTooLarge`, `DigestMismatch` and
  `ImmutableFilename`; `Loaded::open` maps `Library::new`, missing entry point,
  null/incompatible header and missing processor to `ModuleLoadFailed`, `MissingEntryPoint`,
  `AbiMismatch` and `MissingProcessor`. Filesystem I/O stays a raw `std::io::Error`.
  `Host::poll` keeps the transient/lock gate first, then extracts the typed reason; the coarse
  values remain untyped fallbacks. `Display` is the previous context label and the wrapped load
  error stays on `source()`, so `{}`/`{:#}` text and `transient_io` classification are unchanged
  (an embedded inner in `Display` would double the `{:#}` chain and was rejected in audit).
  Movement verify/open keep the same refusal, transient flag and `verified_digest` semantics;
  `benilla-app`/`quest_trace` carry the nine new variants and keep the wildcard.
- Tests: updated digest/load expectations; new fine basename/revision/size/digest/filename cases
  with byte-exact messages, a wrapped `PermissionDenied` transient/reason test, quiet-latch
  `take_publication_reason() == None`, take-clears, and a movement verify compatibility case.
  Native-only missing-entry/null-or-bad-header/missing-processor branches and a successful
  `Ok(true)` staging poll are named inspection-only.
- ZAEBAL: two independent source/API auditors; both final gates `CLOSED` after the `Display` and
  ADR findings were fixed. Remaining: canary/active/movement/rollback typed reasons, the
  trust/dependency/capability/manifest-version codes, non-publication `IncidentOutcome` detail,
  persistent health/policy/artifact stamps, the pre-existing `Unknown` -> coarse wildcard, and
  build/test. No Cargo/build/test/client run was performed.

### 25. Canary/active typed evaluation reasons — `ffdd690f`

Branch `fix/hotpatch-eval-reasons-a`; merged into `origin/main` by fast-forward
`370383c6..ffdd690f`.

- Problem: `Loaded::evaluate` returned free-form `anyhow` errors, `Host` stored only strings in
  `rejection`/`active_failure`, and `CandidateRejected`/`FrameFailed` carried only the coarse
  `HotpatchReason::{Evaluation, Frame}`. The recorder could not distinguish a module status from
  invalid output or a topology change, and a restore/predecessor failure was only in the log.
- Fix: public `#[non_exhaustive]` `PatchEvaluationReason` (`Status(i32)`, `InvalidOutput`,
  `TopologyChanged`, `FrameTooLarge`, `HostFrameRejected`, `Unknown`); private
  `EvaluationFailure { reason, detail }` with unchanged `Display` text; `Loaded::evaluate` maps
  oversized counts, non-zero status, invalid transforms and changed parents while preserving the
  status/combined messages and transform-before-topology precedence. `Host` pairs the existing
  strings with typed slots; `take_rejection`/`take_active_failure` keep the string API and clear
  the pair; `take_rejection_detail`/`take_active_failure_detail` return the pair and degrade a
  missing reason to `Unknown`. `RestoreOutcome::Unavailable` carries the last predecessor (or
  active) reason; a successful restore retains no reason. `CandidateRejected`/`FrameFailed` carry
  `detail`, `RestoreUnavailable` carries the outcome reason, the host-frame `Err` path uses
  `HostFrameRejected`, and the dedupe includes `Runtime.last_frame_detail`. `benilla-app`/
  `quest_trace` add the fine `HotpatchReason` variants, a pure mapping with coarse fallback, and
  `PatchEvent::RestoreUnavailable.reason` through attempt/terminal/summary/incident paths.
- Tests: status/invalid/zero-norm/topology and invalid+topology precedence with exact messages;
  candidate/active pair take/clear; missing-reason -> `Unknown`; `{:#}` display equality;
  below-threshold retention; threshold/restore and predecessor reasons; active-vs-predecessor
  `FrameFailed` helper; app mapping; dedupe helper. `FrameTooLarge` is inspection-only because
  `Host::evaluate` rejects oversized counts first. No Bevy-App-level restore event test and no
  `last_frame_detail` reset test; both are named residuals.
- ZAEBAL: two independent source/API auditors; both final gates `CLOSED` after the reviewer's
  `FrameFailed` active/predecessor fix, the doc/test/format cleanups and the latent
  `restore_frame_detail` fallback removal. Remaining: movement/rollback evaluation reasons,
  persistent health/policy/artifact stamps, the native marker windows, the production movement
  adapter, and build/test. No Cargo/build/test/client run was performed.

### 26. Persistent per-revision health, policy and artifact stamps — `978779da`

Branch `fix/hotpatch-persist-state-a`; merged into `origin/main` by `95740455`, which integrates the
other owner's `4c203d4d` addon/secure-template work.

- Problem: `PatchHealth`, `PatchHealthPolicy` and the pose/movement `ArtifactStamp` were in-memory
  only. A restart forgot a revision's cumulative failures, the demotion policy and the same-text
  artifact-repair detector.
- Fix: bounded `<root>/patch-state.toml` (`schema_version = 1`, `#[serde(deny_unknown_fields)]`,
  optional `[policy]`, `[[health.revisions]]` and `[stamps.pose]`/`[stamps.movement]`). Health is
  digest-keyed, deduped, capped at 16 and evicted oldest-first; the persisted high-water mark is
  clamped to `i64::MAX` for TOML. Each stamp carries a bounded exact `latch` manifest text
  (`<= 4096` bytes) that must parse as a `Publication` with the same basename, a non-empty revision
  and a 40-hex digest; a stamp without a valid latch is omitted, so a restored stamp can never be
  orphaned from the text that consumes it. `load_persistent_state` parses the whole capped document
  before replacing policy, health, stamps and `observed`/`movement_observed`; missing files are
  clean and malformed/oversized/unknown-field/unsupported-version files leave the running state
  untouched. `persist_state` writes only while dirty via temp + `write_all` + `sync_all` + rename;
  failure keeps the previous file/dirty flag and never stops the poll/seams. Candidate promotion and
  successful predecessor restore reset the consecutive streak and seed `total_frame_failures`/
  `max_eval_micros`; only steady-state active failures/evaluations update the record. A normal
  `B -> C -> B` is refused by the digest quarantine; the stored total seeds after explicit
  re-admission (cap eviction/manual removal). Plugin and probe load at startup and save at 1 Hz
  through separate deduped error slots; a stamp without a modification time compares by length only.
- Tests: round-trip policy/health/stamps/latches; missing/malformed/unknown-field/version/oversized;
  invalid entries and latches; dedupe/cap/eviction on load and save; per-revision seeding on
  promotion and restore; cumulative demotion after reload and re-admission; restored latch driving
  the same-text `poll` retry; saturated high-water clamp; dirty-only save; failed save preserving the
  previous file/dirty. Empty-revision latch exclusion, no movement `stage_movement_processor` restore
  test, no plugin exit flush, and end-to-end rather than raw-file save-cap coverage are named
  residuals. No Cargo/build/test/client run was performed.
- ZAEBAL: two independent source/API auditors. Gate 2 found the persisted stamp was write-only in a
  fresh process (the latch text was not persisted); the bounded latch field and restore path fixed
  it. Both final gates `CLOSED` after the latch, `B -> C -> B` wording/test, high-water clamp,
  ADR/test-list and residual-doc fixes. Remaining: movement health, movement/rollback evaluation
  reasons, the native marker windows, the production movement adapter, and build/test.

### 27. Movement health and active retention policy — `61980f8d`

Branch `fix/hotpatch-movement-health-a`; merged into `origin/main` by `6d5c8425`, which integrates
the other owner's `7e91b794` secure-template conditions work.

- Problem: the movement seam treated every active module-produced `Status`/output failure as a
  single strike and had no counters, timings, snapshot or persisted health.
- Fix: public `MovementHealthSnapshot`, `MovementHealthPolicy` (default `1/3`, `0` disables) and
  `MovementRefusal::Tolerated`; private `MovementHealth` and bounded
  `persisted_movement_health`. Only active steady-state `movement_proposal` `process` calls are
  timed/counted; staged canary, `FrameInput`, `NoModule`, withdraw and seam-disable paths preserve
  or reset the record per the documented lifecycle. Below both thresholds an active failure keeps
  the processor/slot, does not quarantine, returns `Tolerated`, and preserves the precise refusal
  in `last_refusal` plus the original detail. At threshold the verified digest is quarantined and
  the active slot cleared as before. Commit seeds total/high-water from the persisted digest;
  unload/seam disable reset the live record, including the disabled-stage branch. A successful
  active proposal and a handback clear the stale refusal/detail channel.
- Persistence: independent bounded `[movement_health]` in `patch-state.toml` (16 digest-keyed
  entries, dedupe/move-to-newest/oldest eviction, label bounds, `i64` high-water clamp); a schema-1
  file without the section loads empty. The movement policy is a live override and is not
  serialized, so changing it does not dirty the state file.
- Tests: active success timing/counter reset; active `Status`/output below threshold; output at
  threshold; `0/0`; total threshold after a success; staged canary isolation; lifecycle and
  disabled-stage reset; commit seeding; re-admission after quarantine removal; state
  round-trip/dedupe/cap/invalid/schema-1/independence; stale refusal/detail clearing.
- ZAEBAL: two independent source/API audits; final gates `CLOSED` after the stale
  `Tolerated`/detail channel, disabled-stage reset bypass, policy/state-dirty mismatch, docs and
  added tests were fixed. Remaining: movement/rollback typed reasons, a persisted last-good module
  identity, native marker windows, the production movement adapter/handback, and all
  build/test/receiver/visible evidence. No Cargo/build/test/client run was performed.

## Release verification attempt — 2026-11-11 (blocked)

Cargo slot owner: overseer. No client/server/visual run was made.

- `cargo check -p benilla-hotpatch -p benilla-hotpatch-bevy` passed. The rustc command line
  confirmed the `sccache.exe rustc ...` wrapper; `sccache --show-stats` remained at 0 compile
  requests, so no cache-hit claim is made.
- 15 focused `benilla-hotpatch` tests passed after correcting one test assertion: the movement
  state-file round-trip expected `[movement_health]`, but TOML serializes the array-of-tables as
  `[[movement_health.revisions]]` (`7d147f15`). The other 14 passed first time.
- `cargo check -p benilla-app --features hotpatch` is still blocked by 21 pre-existing errors
  outside the hotpatch files: `ui_quest::is_shown`, the `wrath.rs` PvP surface
  (`BattlefieldStatus` fields, `PvpLogRow::bonus_honor`, `SessionEvent::PvpLogData`
  `winner`/`rows`), and boxed `LfgJoinResult`/`LfgProposalUpdate` mismatches.
- Four mechanical main build repairs were made and committed separately (`7145198b`):
  `secure.rs` `ObjectLike` import, `pvp` `MultiValue` returns, the `ui_gm_ticket` raw-string
  delimiter, and the stale `EntityKind` path. They reduced the app error list but did not make the
  app compile-clean.
- Logs: `.scratch/project-b/release-verify-2026-11-11.log`,
  `.scratch/project-b/release-tests-2026-11-11.log`.
- Release status: **not ready**. The app/protocol build errors belong to the owners of those
  surfaces; the hotpatch source/tests pass, but there is no app build, client run, receiver or
  visible-gameplay acceptance.

## Remaining modular gaps

- Single-slot movement controller adapter: the in-tree `MovementProcessor` cannot express a
  refused staged candidate beside a retained host active, so the retention contract has no
  production consumer. The adapter must call `withdraw_movement_proposal` on a controller
  rejection/drain; the host cannot observe the controller verdict. The withdrawal is unscoped
  (no proposal generation/token), so a delayed rejection could disarm a newer proposal.
- H03 persistent store: `quarantine.toml`, the pose `active-sentinel.toml` and
  `patch-state.toml` now persist the digest quarantines, pose call-scoped sentinels, demotion
  policies, per-revision cumulative pose/movement health and artifact stamps (with bounded latch
  text). Publication `PatchRejectReason` codes, pose `PatchEvaluationReason` codes and the
  movement health/policy/snapshot are implemented. Movement/rollback typed reasons and a persisted
  last-good module identity remain.
- The pose sentinel covers only the `Loaded::open` and staged-canary `Loaded::evaluate` call
  windows. A native fault during steady-state active evaluation, the predecessor restore probe or
  `Loaded` drop/`FreeLibrary`/DLL-detach has no marker; a staged candidate waiting between `poll`
  and its first evaluation is unmarked (no module code runs in that gap), and a canary rejection's
  digest is persisted only by the next 1 Hz save.
- Artifact stamp heuristic: a same-length repair with an unchanged or unavailable mtime can still
  be missed; a capture-time `stat` failure leaves a text-only latch; the stamp is lost on restart.
- Production movement adapter/Bevy runner: `stage_movement_processor`/`movement_proposal` have
  no production caller (`docs/architecture/release-plan-2026-11-05.md`). This is the main
  remaining modular integration item and needs a design decision plus build/launch validation.
- Persistent/typed movement canary reasons and health/rollback counters remain H03.
- Build/test/runtime acceptance for the modular closure commits, including the sentinel slice,
  remains unperformed.

## Next concrete step

Release verification was recorded at `e12dc650`: the hotpatch crates check clean and 15 focused
hotpatch tests pass; `benilla-app --features hotpatch` was blocked by 21 pre-existing
`benilla-app`/protocol/UI errors outside the hotpatch files (see the verification section above).
`origin/main` has since advanced to `329a5151` (`fix(app,ui): resolve benilla-app compile blockers
and tick secure state drivers`), which claims to repair those blockers but has not been re-checked
by this line. Rerun the app check and app mapping tests on `329a5151`, then the owner-driven
login → roster → Firna/Northshire run. The production movement controller adapter/handback still
needs cross-crate design and launch validation. Remaining source-only polish: movement/rollback
typed reasons, a persisted last-good module identity, and the native marker windows. No release
claim is possible until the app build and receiver/visible gates pass.

Detailed AutoSwarm/ZAEBAL next-session runbook (worktrees, model fallback Flint → Beam →
DeepSeek, prompt templates, merge/push checklist):
`.scratch/project-b/handoffs/2026-11-11-autoswarm-overseer-next-session.md`.
