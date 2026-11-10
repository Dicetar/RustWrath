# ADR-0002 Live client presentation modules and content revisions

## Status

**Proposed (H02 revision trust and capability contract — pending owner review, 2026-11-03).**

Historical baseline status (preserved): Implementation in progress, 2026-10-31. The owner requested native graphics fixes and asset
patches without restarting the client, with `/reload` as the maximum routine UI interruption.
The owner explicitly approved the initial host rebuild/restart. In-game acceptance is pending.

This promotes native presentation reload from the optional last stage in owner-decisions row 6
to an immediate implementation target. It does not declare arbitrary native code reloadable.

The 2026-11-03 H02 additions below specify the **Proposed** revision manifest (`PatchRevisionV1`),
pre-load capability and trust gates, activation decision table, failure/rollback sequences, and
trust boundaries that `H03` ("Add patch health and last-good rollback"), `H04` (UI overlay reload),
`H05` (asset revision dependencies), and `X04` (negotiated WM patch pilot) rely on. Sections
describing existing source behavior at `main` are explicitly labeled **Current behavior**;
sections specifying the target contract for `H03` onward are explicitly labeled **Proposed contract (H02)**.

## Current evidence

- Bevy 0.18.1; the launcher already forwards opt-in features into benilla-app.
- UI VM replacement exists in `ui_script/lifecycle.rs::run_pending_reload` and keeps the game
  session outside the UI lifecycle. Its continued connected behavior still needs acceptance.
- Project shaders use `embedded_asset!`; Bevy's `embedded_watcher` enables source shader reload.
- `Profile::reload` and `ProfileAssetReader::reload` open candidates before replacement, but
  explicitly leave previously loaded content intact. Reopening archives does not refresh live
  models, palettes, sprites, decoded DBC catalogs or material caches.
- `rig_anim::PosePost` is an existing public scheduling seam after animation evaluation and
  before model composition. Its writers save locals; a new native pass must run after those
  writers. `RigPose.locals`/parents/dirty flag are already public.
- The owner subsequently assigned graphics and rig integration to Astra. `PoseModulePass`
  now follows `PosePost` before composition; the adapter restores post-edit input locals
  after downstream composition so native output cannot accumulate into unkeyed channels.
- The focused Bevy pose roundtrip check passes: repeated native root offsets do not accumulate,
  and replacing the module restores the original pose. Actual in-game activation is pending.
- **H00 update (2026-11-03, current behavior; transient/terminal split 2026-11-09):**
  `benilla_hotpatch::Host::poll` leaves `Host.observed = None` for transient filesystem failures
  (`std::io::ErrorKind::{NotFound, PermissionDenied, Interrupted, WouldBlock, TimedOut}` anywhere
  in the cause chain, plus Windows sharing/lock raw OS errors 32/33), or while `publish.lock` is
  held, so an incomplete copy is retried unchanged; a `ready.toml` that is not valid UTF-8 also
  retries the cheap read without reading or opening the DLL. A malformed TOML manifest or a
  deterministic `verify_publication`/`Loaded::open` content/contract error instead latches
  `Host.observed = Some(text)` and returns `Err` once (the shared candidate-rejection channel is
  not written); the unchanged manifest is then a quiet `Ok(false)` until the text changes **or** its
  recorded artifact stamp no longer matches the file. A terminal `verify_publication` or
  `Loaded::open` failure records `Host.pose_failure_stamp` (`ArtifactStamp { library, len,
  modified }`, read from `fs::metadata`; no re-hash) when the named library is one normal basename
  inside the publication root and the artifact can be stat'd. The stamp is captured **before**
  verification reads the bytes, so a publisher finishing a write during the failed read cannot be
  recorded as the durable stamp and mask the next identical-text retry. On the next identical-text
  poll a changed length or modification time clears `observed` and the stamp and retries once; a
  missing or unreadable artifact also mismatches and retries, so while it stays unavailable the
  unchanged manifest returns `Err` at 1 Hz with `observed` unset and `collect_retired` skips its
  sweep until the artifact reappears or the text changes. This missing-artifact retry applies to a
  digest that is not quarantined; a quarantined digest is refused at the gate before the read, with
  the text latch and no stamp, so the sweep can still run. Malformed TOML and a non-basename library
  name record no stamp (the latter is repairable only by the text), and the digest gate records none
  either. A post-verification `Loaded::open` failure still records a stamp because the terminal-load
  latch requires it even though its digest is quarantined; a later byte change then clears the latch
  and produces exactly one quarantine refusal before re-latching, so the digest quarantine itself is
  unchanged and cannot be bypassed. The stamp is in-memory only and lost on restart. A same-length
  repair whose modification time is unavailable or unchanged can still be missed; a `None` modified
  time compares by length only. A terminal **post-verification**
  `Loaded::open` failure (the bytes matched `publication.sha1` and the immutable filename) adds that
  verified digest to the bounded pose-digest quarantine
  (`Host.quarantined_digests`, `VecDeque<String>`, cap `MAX_QUARANTINED_DIGESTS = 32`, persisted to
  `quarantine.toml`; see the H03 persistent update below): a later
  manifest text that names the same digest is refused before `Library::new` even after a
  `B -> C -> B` rewrite, while a genuinely new digest is still evaluated. A digest/filename/size
  failure before verification latches the text but does not quarantine the declared digest, which
  was not proved against the bytes. Transient I/O, a held `publish.lock` and malformed TOML do not
  quarantine. The same bounded store covers failures after a successful load: a rejected canary
  batch, a failing active module and a predecessor that fails during restore add their verified
  digest, retained copies of that digest are dropped, and the same-frame restore loop skips a
  predecessor whose digest is already quarantined.
  Host/caller errors (oversized counts, malformed rig spans that do not tile the
  bone batch in order (gaps, overlaps, reordering, or empty span lists with bones), non-finite/zero-norm
  input bones, a non-finite frame time, or a non-negative parent that is out of bounds or does not
  precede its child within its rig span) never quarantine; an empty bone batch is a quiet
  `Ok(false)` no-op, not an error. A changed manifest resets the latch
  and any superseded `Host.pending` candidate before evaluation
  (`crates/benilla-hotpatch/src/lib.rs`, verified by focused tests
  `poll_retries_unchanged_manifest_after_transient_load_failure`,
  `malformed_manifest_latches_after_one_error_and_retries_when_rewritten`,
  `transient_io_classifies_only_filesystem_availability`,
  `quarantined_digest_is_refused_before_loaded_open_across_manifest_rewrites`,
  `digest_mismatch_latches_without_quarantining_the_declared_digest`,
  `wrong_immutable_filename_latches_without_quarantining_the_verified_digest`,
  `publish_lock_keeps_a_post_verification_failure_retryable_until_the_publisher_finishes`,
  `missing_dll_transient_failure_does_not_quarantine`,
  `malformed_manifest_does_not_quarantine_a_digest` and
  `quarantine_store_dedupes_and_is_bounded`).
- **Movement seam update (2026-11-09, current behavior):**
  `benilla_hotpatch::Host::stage_movement_processor` applies the same transient/terminal split to the
  optional `movement.toml` publication, independently of `Host.observed`. A deterministic TOML parse,
  pre-load verification (basename/revision/size/digest/filename) or post-verification
  `MovementLoaded::open` failure latches `Host.movement_observed = Some(text)`, so the unchanged
  manifest is a quiet `Ok(false)` until the text changes or its artifact stamp no longer matches. A terminal failure that occurred *after*
  `verify_publication` proved the bytes also adds the verified digest to the bounded
  `Host.movement_quarantined_digests` (`VecDeque<String>`, cap `MAX_QUARANTINED_DIGESTS = 32`,
  deduplicate, oldest eviction), so a changed manifest that still names that artifact (`B -> C -> B`)
  is refused as `MovementRefusal::Quarantined` before any artifact read or `Library::new`, even when
  the DLL was deleted. `MovementLoadFailure` carries `transient` (transient filesystem availability
  anywhere in the `libloading`/`anyhow` cause chain, or a held `publish.lock`) and `verified_digest`
  (`Some` only after verification), so transient failures retry without latching or quarantining and
  pre-verification digest/filename/size failures latch without quarantining; a non-UTF-8
  `movement.toml` takes the cheap read-error path and retries without reading or opening the DLL.
  A deterministic movement verification or load failure with a parsed publication records
  `Host.movement_failure_stamp` from that artifact (same `ArtifactStamp` shape), captured before
  `MovementLoaded::verify` reads the bytes and only for a normal basename inside the root. An
  identical-text stage call whose length or modification time no longer matches, or whose artifact
  is missing, clears `Host.movement_observed` and the stamp and retries once; a matching stamp stays
  a quiet `Ok(false)`. While the artifact stays missing or unreadable the unchanged manifest returns
  `Err` at 1 Hz with the latch unset, so the retention sweep is skipped until the artifact
  reappears, unless the digest is quarantined: the gate runs before the read, latches the text with
  no stamp and still allows the sweep. Malformed movement TOML and a quarantined-digest gate record no stamp; a
  post-verification load failure records one despite quarantining its digest, so a later byte change
  yields exactly one `Err(Quarantined)` before re-latching. A malformed latched movement manifest
  still aborts `collect_retired` as a malformed latched pose manifest does. `Host::set_seam_enabled`
  clears the stamp with the latch when it enables the seam and keeps the manifest latch while the
  seam is disabled so the retention sweep still pins the refused publication; the stamp is not
  needed for pinning and a disabled-seam stage call clears it. The stamp is in-memory only.
  A post-verification Windows load error with no transient `io::Error` (for example raw 126 missing
  dependent DLL or 193 bad image) is terminal and therefore quarantines the proved digest, including
  for a revision-only manifest change naming the same digest; repairing that dependency requires a
  new digest (restart no longer clears it now that the store is persisted). On Unix,
  `libloading::Error::DlOpen` carries no `io` source, so
  every load-time error is terminal and only the verification reads can be transient. The store is
  persisted to `<root>/quarantine.toml` (see the persistent quarantine update below) and deliberately
  separate from the
  pose store (same bytes under a different ABI cannot suppress the other seam; H03 may key a shared
  store by `(ModuleKind, digest)`). `Host::movement_process` selects the staged or active slot and
  returns its processor, revision, verified digest and staged flag. `Host::movement_proposal` adds
  that selected digest to `Host.movement_quarantined_digests` when the module returns a non-zero
  `Status` or its output fails `validate_movement_output`, then drops only the failing candidate
  through `Host::reject_movement_candidate`: a failing staged candidate clears `movement_pending`
  and retains a healthy `movement_active` unless it carries the same digest, while a failing active
  processor clears both slots. The failed frame falls back to the baseline controller for one tick;
  a retained active serves the next valid proposal. A `B -> C -> B` movement-manifest rewrite is
  refused as `MovementRefusal::Quarantined` before any artifact read or `Library::new`.
  `Host::commit_movement_processor` now requires a successful `Host::movement_proposal` from the
  current staged candidate. The private per-call `Host.movement_proposal_ready` flag is cleared at
  `Host::new`; by any `Host::stage_movement_processor` path that changes or drops the selected/staged
  state (missing manifest, disabled seam, changed manifest, successful staging); by every typed
  refusal path (`Host::refuse_movement`, `Host::reject_movement_candidate`, and the top-of-call
  clear that also covers `NoModule`); by `Host::set_seam_enabled` on a seam transition; by
  `Host::unload_movement_processor`; and at the start of every proposal call. Only a valid `Staged`
  proposal sets it, while a valid `Active` proposal clears it. `Host::withdraw_movement_proposal`
  clears it after a controller-side intent rejection while keeping the candidate staged for a later
  frame. Staging alone, or a `FrameInput` that
  retains the candidate for a later valid frame, therefore cannot promote it until a later
  successful staged proposal; `NoModule` has no candidate to promote. A `Status` or
  output-validation failure instead drops the failing candidate through
  `Host::reject_movement_candidate`, so there is no pending candidate left to gate. A commit with no
  armed proposal returns `false` and leaves a retained candidate staged when the seam is enabled; a
  commit with no pending
  candidate clears the flag. Readiness records only that the current staged candidate produced a
  host-validated proposal; the controller must call `Host::withdraw_movement_proposal` when its
  intent evaluation rejects that proposal, which clears the gate while keeping the candidate
  staged for a later frame. The host cannot observe the controller's private verdict, so a caller
  that ignores the withdrawal can still commit; the API now exposes both the arm and the
  rejection handback rather than a single caller-trust path. This is
  the single-strike tradeoff: an input-dependent module failure, including output refused against
  valid caller-supplied `MovementBounds`, refuses that digest while it remains in the 32-entry
  store, and only cap eviction or deleting `quarantine.toml` while the client is stopped clears it
  (the store is persisted, so a restart does not). `FrameInput` (a non-finite or non-positive
  `dt_seconds`,
  any other non-finite scalar frame field except the `liquid_height` `-INFINITY` no-liquid
  sentinel, invalid bounds, or caller-seeded facing/speed outside those bounds), `NoModule`,
  `SeamDisabled` and host/caller errors never quarantine, and a valid proposal/commit never
  quarantines; `FrameInput` also returns without touching `movement_pending` or `movement_active`,
  so the same processor serves the next valid frame. `NoModule` is equally non-destructive, while
  `SeamDisabled` evicts both slots. The pre-dispatch `movement_frame_valid` check covers position,
  velocity, facing, pitch, camera yaw, the speed/ground/stamina scalars, the ground normal,
  non-negative bounds (`+INFINITY` remains the unbounded sentinel), and caller-seeded
  facing/speeds. Input `move_flags` may carry root/falling/swimming/transport state that the host
  does not delegate; only the module's *output* flags are checked against `permitted_flags`, so the
  module must clear them. A module failure caused by an in-bounds but extreme or inconsistent
  finite input is still treated as a module failure and quarantines. Focused unit tests cannot load
  a native movement DLL, so the successful `Ok(true)` stage path is verified by inspection; the
  readiness state machine is exercised with in-process probes and deterministic filesystem stage
  paths. The pose canary/frame path is current behavior.
- **D00/D01–D07 update (2026-11-03, current behavior):** `benilla_app::hotpatch::capture_hotpatch_events`
  bridges `benilla_hotpatch_bevy::HotpatchEvent` into the resident recorder (`quest_trace::Stage::Hotpatch`
  wrapping `quest_trace::PatchEvent`), tracking `History.last_observed_patch_digest` on activation
  and recording `IncidentOutcome::ObservedFailure` (`IncidentConfidence::Exact`) on rejected
  publications, rejected candidates, frame failures, or retention errors.
- **H03 in-memory health update (2026-11-09, current behavior):** `benilla_hotpatch::Host` owns a
  private `PatchHealth` and exposes `Host::health_snapshot() -> PatchHealthSnapshot`. The snapshot
  reports the active module's `revision`/`digest` (`None` when the active slot is empty),
  `consecutive_frame_failures`, `total_frame_failures`, `last_eval_micros`, `max_eval_micros`, and
  `quarantined_digests` (the current pose quarantine length). Only the active
  `Loaded::evaluate` call is timed with `std::time::Instant`; the candidate canary and predecessor
  restore probes are not, so the activation frame itself is unmeasured and both timings stay 0
  until the first post-activation evaluation. Every active evaluation updates `last_eval_micros`
  and the `max_eval_micros` high-water mark (saturating `u128`→`u64`); an active `Ok` resets
  `consecutive_frame_failures` and keeps `total_frame_failures`, while an active `Err`
  saturating-increments both. `total_frame_failures` is per-activation in this slice; per-revision
  cumulative accounting for `max_total_frame_failures` remains H03. A candidate canary rejection or
  failure does not itself touch the active counters; the same-frame retained-active fallback still
  evaluates the active module and updates its timing/counters. Candidate promotion and a successful predecessor
  restore reset all four values for the newly active module. After a demotion with no valid
  predecessor the last counters/timings remain visible while `revision`/`digest` are `None` until
  the next activation; `take_active_failure`/`take_restore_outcomes` still carry the failed
  identity. Health counters remain in-memory only: persistence, typed reasons and the policy gaps
  below remain H03.
- **H03 in-memory health policy update (2026-11-10, current behavior):**
  `benilla_hotpatch::Host` owns a `PatchHealthPolicy` (private `Host.health_policy`, public
  `Host::set_health_policy`/`Host::health_policy`) with `max_consecutive_frame_failures` and
  `max_total_frame_failures`. `PatchHealthPolicy::default()` is `1/3`, preserving the previous
  single-strike disposition. Both thresholds compare the active module's per-activation counters
  (`PatchHealth::threshold_reached`): `(max_consecutive != 0 && consecutive >= max_consecutive) ||
  (max_total != 0 && total >= max_total)`, so either enabled threshold demotes and a value of `0`
  disables that threshold (`0/0` never demotes on health counters). After
  `PatchHealth::record_failure()` on an active `Err`, a below-threshold failure keeps `Host.active`
  and `Host.previous` unchanged, leaves the digest out of the quarantine store, clears the caller's
  `output` (the failed module may have written partial/invalid values), sets
  `Host.active_failure` so the adapter logs the failure, pushes **no** `RestoreOutcome`, and returns
  `Ok(false)`; the caller renders the baseline pose for that frame and the same module is evaluated
  on the next frame. A threshold-reaching failure keeps the existing demote/quarantine/newest-first
  restore behavior and `Restored`/`Unavailable` outcomes unchanged. Candidate canary failures and
  predecessor restore probes are not counted. Successful evaluations reset only the consecutive
  streak; the cumulative count survives until the next activation. A caller override affects only
  failures recorded after `set_health_policy`, and the manifest does not yet feed the policy.
  Persistent health/policy/artifact stamps, typed `PatchRejectReason` values, per-revision cumulative
  accounting, `max_eval_micros` enforcement, `canary_frames_remaining`, movement health, the
  movement active per-frame process marker, and the relevant build, focused-test and receiver/visible evidence remain
  open H03 work. The digest quarantines themselves are persisted (see the next bullet).
- **H03 persistent digest quarantine update (2026-11-10, current behavior):** both bounded digest
  quarantines are persisted beside the publications in `<root>/quarantine.toml`, a fixed
  `#[serde(deny_unknown_fields)]` `QuarantineFile { pose: Vec<String>, movement: Vec<String> }`
  (both lists `#[serde(default)]`, oldest first). `Host::persist_quarantine` serializes exactly the
  two lists — no library path, revision, timestamp or OS error text — to `quarantine.toml.tmp`
  (create/truncate, `write_all`, `sync_all`), then renames it over the target in one
  `std::fs::rename`, which replaces an existing file on both Unix and Windows
  (`MOVEFILE_REPLACE_EXISTING`; the crate's `local_state::write_atomic_bytes` relies on the same
  behavior). There is no delete-then-rename window: a reader sees the previous complete file or the
  new one, a failure before the rename leaves the previous file byte-identical, and only the inert
  temp is removed. The contents are flushed; the directory entry is not synced, so this is atomic
  replacement, not a power-loss durability guarantee. A save failure never stops the poll or either
  seam. An empty store writes nothing, so a fresh install never creates or rewrites an empty file and
  an unreadable store stays on disk for diagnosis; once either list is non-empty every 1 Hz poll
  call rewrites both lists (accepted write amplification for the unconditional poll wiring; nothing
  runs per frame). Evaluation-path quarantines are saved on the next 1 Hz poll, so a crash or exit
  before that can lose the last second's digest; it is re-quarantined on the next failure. The
  poll-path load-failure quarantine is saved in the same tick. The file is single-client scope: a
  fixed temp name, no interprocess lock and last-writer-wins replacement, so a root shared by
  concurrent clients is unsupported.
  `HotpatchPlugin::build` calls `Host::load_persistent_quarantine` once after `Host::new` and before
  the first poll: a missing file is `Ok(0)`, while a read/parse/unknown-field error is logged once
  with `warn!` and recorded in `runtime.last_quarantine_error`, and startup continues fail-open with
  empty stores (the corrupt file is not overwritten while the store stays empty). The 1 Hz `poll`
  calls `persist_quarantine` after `host.poll()`/`collect_retired()` and reports a save failure
  through the same dedicated slot (`report_quarantine`), which only a successful save clears. That
  slot is separate from `runtime.last_error`, so a persistent save failure neither re-emits
  `PublicationRejected`/`RetentionFailed` every tick nor is masked by a successful staging that
  clears `last_error`. Loading parses the whole
  document before merging, so a malformed file returns `Err` and leaves both stores unchanged. Each
  entry must be exactly 40 ASCII hex digits and is normalized to lowercase; empty, short, over-long,
  non-hex, nested/path-like and duplicate entries are skipped silently and are never joined to a path
  or used as a filename. This validation is shape/cap hygiene, not a traversal or trust boundary: an
  accepted entry is only a string key compared against an already verified digest. A list longer than
  the cap keeps only its newest 32 entries, so a reload cannot rotate the store with the file's
  evicted prefix. Valid entries merge through
  `Host::quarantine_digest`/`Host::quarantine_movement_digest`, preserving cap 32, dedupe and
  oldest-first eviction; loaded entries append after whatever the running host already holds, so an
  existing digest stays older and is evicted first. The call returns the number of newly inserted
  entries (already-present, invalid and beyond-cap entries are not counted), so reloading the same
  file returns 0. Because `deny_unknown_fields` rejects an unknown key, a future
  `schema_version`/health field makes this binary fail open empty for that run rather than load a
  partially understood store; the file is preserved while both stores stay empty, and a later
  quarantine rewrites it. Focused tests for this slice are
  `persistent_quarantine_round_trips_pose_and_movement_digests`,
  `persistent_quarantine_rejects_an_oversized_file`,
  `persistent_quarantine_missing_file_is_an_empty_success` (empty store writes no file),
  `persistent_quarantine_malformed_file_is_an_error_and_keeps_both_stores`,
  `persistent_quarantine_skips_invalid_and_duplicate_entries`,
  `persistent_quarantine_load_respects_the_cap_and_oldest_eviction` (32-entry window, reload returns
  0), `persisted_quarantine_contains_only_digests_and_reloads_idempotently`,
  `persistent_quarantine_merges_into_existing_stores_without_clearing`,
  `persisted_quarantine_replaces_the_previous_file_and_a_stale_temp` and
  `failed_quarantine_replacement_keeps_the_previous_store_and_cleans_the_temp`; they are
  source-reviewed only here (this slice ran no Cargo build or test), and the plugin build/1 Hz wiring
  is verified by inspection. **Residual risk and recovery:** a digest quarantined for an
  input-dependent canary, active-frame, restore-predecessor or movement-`Status`/output failure now
  survives restart, so the only recovery is 32 newer digests evicting it or deleting
  `<root>/quarantine.toml` while the client is stopped (a restart alone no longer clears it).
  The store is read with a 64 KiB size cap; a larger file returns `Err` with both stores unchanged. Entry validation is quadratic in the list length, but bounded by that file cap. Persistent health/policy/artifact stamps, typed reasons, per-revision totals, movement health, the movement active per-frame process marker, later pre-commit staged movement proposals, pose active-frame/restore/drop native faults, the production adapter and build/test evidence remain H03. The pose loading and staged-canary sentinels, the movement loading and staged-canary sentinels, and the clean-shutdown stale-marker safety net (see the clean-shutdown sentinel update below) are implemented, while `process::exit`/hard kills and cross-process/file-identity root ownership remain H03.
- **H03 pose crash-sentinel update (2026-11-10, current behavior):** the pose crash-sentinel slice
  now covers both pose call-scoped windows. After `verify_publication` proved the artifact bytes,
  `Host::poll` writes `<root>/active-sentinel.toml` (`{ revision, sha1, kind = "pose", stage =
  "loading" }`, digest-only, no library path and no OS error text) to `active-sentinel.toml.tmp`
  (create, `write_all`, `sync_all`) and then renames it over the target in one `std::fs::rename`
  (no delete-then-rename window), before the irreversible `Loaded::open`/`Library::new`; the open
  returning (`Ok` or `Err`) clears the marker through the best-effort `Host::clear_active_sentinel`
  (a `NotFound` target is `Ok(())`), and a write/clear failure is recorded in the dedicated
  `Host::sentinel_error` slot without failing or skipping the module load. If the marker write
  succeeded, a native fault during the load leaves it for the next launch; a regular failure clears
  it, and the existing transient/terminal latch, artifact-stamp retry, digest-quarantine and rollback
  behavior is unchanged. A failed marker write leaves no new marker (fail-open; the load proceeds), and a
  failure to clear after a successful open can leave a stale marker that quarantines a healthy digest
  on the next launch; both are accepted best-effort limitations of this slice.
  `Host::evaluate` writes the same marker with `stage = "canary"` immediately before the staged
  candidate's `Loaded::evaluate` (`candidate.evaluate(...)`), using `candidate.revision` and
  `candidate.digest`, and clears it immediately after that call returns on every regular path
  (success, rejection, host output-validation error); a native fault during the canary `process()`
  call leaves the marker for the next launch, while a fault after a successful call but before the
  clear can quarantine a healthy digest (the same narrow best-effort window as the loading path). The
  canary write/clear are best-effort and gated on `sentinel_written`, so a retained pending-recovery
  marker is never cleared when the write fails. If a pending recovery is committed but the marker
  write then fails, no new marker remains and a later fault is uncaught (same best-effort limitation
  as the loading path). A regular canary rejection clears the marker before the in-memory digest
  quarantine is persisted by the next 1 Hz save, so an unclean exit in that window of up to ~1 s can
  retry the digest. No marker is written around active-frame evaluation, predecessor restore, or
  `Loaded` drop/`FreeLibrary`/DLL-detach in this slice.
  `HotpatchPlugin::build` calls `Host::load_active_sentinel` once after `Host::new` and before the
  first poll: a missing file is `Ok(None)`; a `Pose`/`Loading` or `Pose`/`Canary` marker is added
  to the bounded pose quarantine through the existing dedupe/cap helpers and the marker is **left in
  place**. `Host::persist_quarantine` commits the store and only then clears the marker, retaining
  it if the commit fails; a later pose or movement load first commits any pending recovery before
  writing a replacement marker, and the writer enforces the same 16 KiB cap before writing. A malformed
  document, unknown field, invalid digest, unsupported kind/stage combination or file larger than 16 KiB is an error
  that leaves the file and both stores unchanged (fail-safe); the startup outcome is warned once and
  never fails the plugin. The 1 Hz `poll` drains `Host::take_sentinel_error` and reports through the
  dedicated `Runtime.last_sentinel_error` slot, separate from `last_error` and
  `last_quarantine_error`. Native faults during steady-state active evaluation, the predecessor
  restore probe, and `Loaded` drop/`FreeLibrary`/DLL-detach, canary-rejection save ordering, the
  movement active per-frame proposal window, later pre-commit staged movement proposals, active
  pose/movement per-frame native calls, persistent health/policy/artifact stamps, typed reasons,
  per-revision totals, the production adapter and build/test evidence remain H03; the
  clean-shutdown stale-marker safety net is implemented in the clean-shutdown sentinel update
  below, while `process::exit`/hard kills and cross-process/file-identity root ownership remain H03.


- **H03 movement crash-sentinel update (2026-11-10, current behavior):** the movement loading-window sentinel covers `MovementLoaded::open`, and the movement staged-canary sentinel covers the first staged proposal. After `MovementLoaded::verify` proved the artifact bytes, `Host::stage_movement_processor` writes `<root>/active-sentinel.toml` (`{ revision, sha1, kind = "movement", stage = "loading" }`, digest-only, no library path and no OS error text) to `active-sentinel.toml.tmp` (create, `write_all`, `sync_all`) and then renames it over the target in one `std::fs::rename`, immediately before `MovementLoaded::open`/`Library::new`; the open returning (`Ok` or `Err`) clears it. `Host::movement_proposal` writes the same document with `stage = "canary"` immediately before the first staged `process` call, using the selected revision and verified digest, and clears it when that call returns on every regular path; a private one-shot guard means a second staged proposal and every active per-frame proposal write nothing, and a new staging, unload, commit or staged rejection resets the guard. A failed marker write is fail-open (the load/proposal proceeds) but consumes the one-shot, so a failed write on the first staged proposal leaves that candidate's pre-commit proposals unmarked; a failed clear or a process death after a successful call but before the clear can leave a stale marker that quarantines a healthy digest on the next launch, and a pending-recovery commit that succeeds before a failed replacement write leaves that call unmarked; all are accepted best-effort limitations. Staged movement proposals after the first and all active per-frame proposals are unmarked and remain H03. `HotpatchPlugin::build` startup recovery routes `Movement`/`Loading` and `Movement`/`Canary` markers to the bounded movement quarantine through the existing dedupe/cap helpers, leaves the marker for `Host::persist_quarantine` to clear after the store commit, and rejects unknown stages as deserialize errors with the file and both stores unchanged. The loading-window marker presence is verified by inspecting the verify/write/open/clear ordering; the staged-canary marker is verified by a test-only observing probe that parses `active-sentinel.toml` during the first staged `process` call and asserts it is gone afterwards, with a second staged proposal writing none. The movement staging path still has no production caller, so the markers are not exercised by the alpha until the production movement adapter lands. Remaining H03 for this seam: active movement per-frame `process` markers, later pre-commit staged movement proposals, movement health counters, pose active-frame/restore/drop native faults, `Loaded` drop/`FreeLibrary`/DLL-detach, canary-rejection save ordering, persistent health/policy/artifact stamps, typed reasons, per-revision totals, the production adapter and build/test evidence. The clean-shutdown stale-marker safety net is covered by the clean-shutdown sentinel update below (the explicit `AppExit` edge is now covered; `process::exit`/hard kills and cross-process/file-identity root ownership remain H03).

- **H03 clean-shutdown sentinel update (2026-11-10, current behavior):**
  `Host::clear_active_sentinel_on_clean_shutdown(&mut self) -> Result<bool>` is the clean-exit safety
  net for a stale `active-sentinel.toml`, separate from the call-scoped clears after regular
  loads/proposals and never a crash path. If `Host.sentinel_recovery_pending` is `Some`, the marker
  is the only durable record of a digest recovered at startup whose `quarantine.toml` commit has
  not succeeded, so the method returns `Ok(false)` immediately and leaves the marker untouched;
  only `Host::persist_quarantine` may clear it. Ownership is content-bound, not a sticky flag:
  `Host.sentinel_marker_owned` is `Option<ActiveSentinelFile>` holding the exact
  `{ revision, sha1, kind, stage }` identity this host last wrote (`Host::write_active_sentinel`,
  only after its rename succeeds, so a failed write leaves the previous ownership unchanged) or
  accepted (`Host::load_active_sentinel`, which resets the slot to `None` at the start of every
  attempt and stores the validated file at the end, so an error leaves `None`). With no ownership
  the marker may be an unvalidated previous-run marker, a malformed file or another process's
  marker, so it is also retained (`Ok(false)` when present, `Ok(true)` when absent; another
  metadata error is returned). With ownership the shared `clear_owned_active_sentinel_if_matching`
  helper stats, reads and parses the marker: `NotFound` is `Ok(true)` (a missing owned file is a
  clean state); a file larger than `MAX_ACTIVE_SENTINEL_BYTES` is retained unread (`Ok(false)`); a
  read error, including a non-UTF-8 file, or other I/O error is returned as `Err` and warned by the
  caller; a TOML parse error or a parsed identity that differs from the owned one is `Ok(false)`,
  and the foreign/unvalidated file is retained and **never deleted** by this method; only an exact
  content match calls `Host::clear_active_sentinel` and returns `Ok(true)`. Identity is the parsed
  `{ revision, sha1, kind, stage }`, so any same-identity TOML formatting difference is also
  indistinguishable, while a different revision/sha1/kind/stage is not owned. Clear errors are
  returned to the caller and never populate `Host.sentinel_error`; the `AppExit` and Drop callers
  warn. Identity binding applies to this method and the `Host::persist_quarantine` post-commit
  retry, both of which call `clear_owned_active_sentinel_if_matching`; the call-scoped clears still
  remove the marker this operation wrote directly. The read/compare/unlink sequence is not atomic,
  so a replacement landing after the compare is a named residual (below).
  `benilla_hotpatch_bevy` calls this method from two clean-exit edges, both on the same host method.
  The explicit `AppExit` edge is the `Last` system `clear_active_sentinel_on_app_exit`, whose
  parameters are `mut exits: MessageReader<AppExit>` and `mut runtime: ResMut<Runtime>`. It is
  registered by `HotpatchPlugin::build` as
  `.add_systems(Last, clear_active_sentinel_on_app_exit.before(capture_pose))`, declared ahead of
  the pose chain because all three systems take `ResMut<Runtime>` and a panic in that chain on the
  exit frame must not skip the clear (`capture_pose` is chained ahead of `restore_pose` by the
  preceding registration, so ordering before `capture_pose` covers both; the tuple form is not
  usable because Bevy 0.18's `before` takes a single `IntoSystemSet`, which only a system or a set
  implements). A window close and console `quit` raise `WindowCloseRequested`, which
  `bevy_window::exit_on_all_closed` turns into `AppExit` in `PostUpdate`; the chat `/quit` /
  Exit Game / logout path (`ui_logout::exit_on_logout_complete`) writes `AppExit` directly in
  `Update`. The runner checks
  `should_exit()` only after that `app.update()` (`crates/benilla-app/src/shutdown.rs`), so a `Last`
  system sees either in the same frame. The dev-only census probe
  (`crates/benilla-app/src/capture/probes/sched_census.rs`) and `CapturePlugin::drive_capture`
  (`crates/benilla-app/src/capture/mod.rs`) also write `AppExit` in `Last`; the edge is not
  guaranteed to see those depending on `Last` ordering, but a normal capture exit drops the App
  and `Runtime::drop` covers it. This is a named residual. The message type is the
  engine's own (registered by `App::default` at
  `bevy_app-0.18.1/src/app.rs:131`), not a new one. It calls the
  host method only when a message is present: `Ok(true)` does nothing, `Ok(false)` returns quietly
  because a pending recovery or an unowned/unvalidated marker is expected to be retained there
  (the Drop fallback stays the loud path for that), and `Err` warns with the clear error. The system
  does not intentionally panic or block; its file operations can still fail and are reported.
  Probe/capture harnesses that announce `AppExit` from `Update` and then arm a 5 s
  `std::process::exit(0)` backstop if teardown hangs (`crates/benilla-app/src/capture/probes/run.rs`,
  `crates/benilla-app/src/capture/probe_bank.rs`) still run this `Last` edge (but bypass Drop),
  which is the main gain over the Drop-only safety net. There is no rising-edge latch, so repeated
  `AppExit` announcements (the macOS extra pass; `exit_on_all_closed` re-announcing with no window)
  call the idempotent clear again, and a persistently failing clear warns once per exit frame and
  again from Drop. On Windows the winit runner owns the `App` and drops it when `run_app` returns
  (no `process::exit` in that backend), so `Runtime::drop` still runs after the `Last` edge:
  `Ok(false)` therefore warns exactly once on a normal exit for a retained marker even though the
  edge returned quietly. On a probe/capture hard exit that reaches `process::exit` before Drop, a
  retained `Ok(false)` marker is silent; this diagnostic residual is source-reviewed only. `benilla_app::shutdown::on_app_exit`/
  `OnAppExit` already expresses this edge, but it lives in `benilla-app`, which depends on
  `benilla-hotpatch-bevy` behind its optional `hotpatch` feature; calling it from here would invert
  that dependency into a cycle.
  `Runtime::drop` remains the fallback for a bare/non-panicking `App` drop: `Ok(true)` does
  nothing, `Ok(false)` warns once that the marker was retained for the next launch, and `Err` warns
  with the clear error; Drop does not intentionally panic or block and does not fail shutdown, and
  never clears a pending recovery marker. Rust runs `Drop` during panic unwind too, so the
  destructor first returns without clearing while `std::thread::panicking()` is true: a panic
  inside a marker-covered call (`Loaded::open`, the canary `evaluate`, movement `process`) must not
  delete the marker for the operation that just panicked, or the next launch would reload the same
  digest and crash-loop. The Drop fallback still owns that unwind guard because a system never runs
  during an unwind. A stale marker left by a failed call-scoped clear is retried at either clean
  exit edge; a failed retry retains it (and the next launch can still quarantine a healthy digest),
  while a startup recovery survives to the next launch until `persist_quarantine` commits it.
  `std::process::exit` and hard kills bypass both edges: no `app.update()`/`AppExit` frame and no
  `Drop` run, so only a normal `app.run()` close runs the safety net. Focused tests are
  `clean_shutdown_clears_a_stale_marker`, `clean_shutdown_keeps_a_pending_recovery_marker`
  (asserts `persist_quarantine` remains the only clearing path and commits the store),
  `clean_shutdown_without_a_marker_is_ok`, `clean_shutdown_keeps_an_unvalidated_marker`,
  `clean_shutdown_keeps_an_unloaded_marker`, `failed_reload_drops_marker_ownership`,
  `clean_shutdown_clears_only_the_matching_owned_marker`,
  `clean_shutdown_treats_a_missing_owned_file_as_clean`,
  `persist_quarantine_keeps_a_foreign_replacement_marker` and
  `clean_shutdown_keeps_an_unparseable_replacement_of_an_owned_marker`; the discriminating identity
  case is `clean_shutdown_keeps_a_foreign_replacement_marker` (an owned marker is overwritten first
  with the same sha1 and a different revision, then with a different sha1, and the shutdown path
  returns `Ok(false)` each time, leaving the foreign bytes and digest on disk and ownership
  `Some`). They are source-reviewed only here (this slice ran no Cargo
  build or test). The `Runtime::drop` and `clear_active_sentinel_on_app_exit` wiring is
  source-reviewed only too. The clear-success path is exercised in-crate because the tests can call
  the private `Host::write_active_sentinel`; `Host::load_active_sentinel` also sets
  `sentinel_recovery_pending`, which deliberately retains, so it is not a success path by itself.
  Proving the *bevy edge* still needs a `#[cfg(test)]` ownership hook on the plugin harness (write a
  marker into the temp root, `write_message(AppExit::Success)`, `app.update()`, assert it is gone).
  Because the system is declared ahead of the pose chain, the residual is a panic in another
  `Last` system that the executor runs before it (or before `Last`): the schedule aborts, Drop then
  skips while panicking, and a stale marker can survive to quarantine a healthy digest on the next
  launch. Drop still cannot distinguish an orderly shutdown from an arbitrary `App` drop, so it
  remains the fallback rather than the primary signal. Remaining H03: `process::exit` and hard
  kills, cross-process/file-identity root ownership (the comparison is in-process only), a foreign
  replacement with the same parsed identity (it differs only in TOML formatting, so it is
  indistinguishable from this host's marker), the read/compare/unlink TOCTOU inside the shared
  compare-and-clear helper, the path-based call-scoped clears (a replacement landing between an
  operation's own marker write and its immediate clear can still be removed), `AppExit` written
  by the dev-only census/capture `Last` writers (ordering), a
  panic in an earlier `Last`/chain system before the clear, a second clear failure at either edge,
  markers around later pre-commit staged movement
  proposals, active movement/pose per-frame native calls, pose active-frame/restore and `Loaded`
  drop/`FreeLibrary`/DLL-detach native faults, canary-rejection save ordering, typed reasons,
  persistent health/policy/artifact stamps, per-revision totals, the production adapter and
  build/test evidence.

## Decision

Keep the live session, authoritative received state, GUID/entity identity, clocks, input and
GPU ownership in the resident host. Replace presentation implementations behind versioned,
explicit interfaces. A Bevy Plugin is installed once; repeatedly loading/unloading Bevy plugins
or exchanging a Rust World across DLLs is not the module interface.

First native interface: a synchronous, stateless pose postprocessor, using C-layout scalar
arrays. The host copies the complete dirty-rig batch, invokes the candidate, validates every
result and only then writes any live local transforms. Skeleton topology cannot change here.
The evaluator's clocks and event cursors are untouched: reloading must not replay footsteps,
spell events or interaction gestures. This seam does not replace M2 import or track decoding.

The ABI crate has no Bevy dependency. No Vec, String, Rust references, traits, Bevy handles,
allocators or job callbacks cross the interface. Input/output memory belongs to the host for
the duration of the call. Modules may not retain pointers, unwind, spawn escaping threads or
own gameplay state. Native modules are trusted local executable code; descriptor/output checks
cannot contain an access violation or arbitrary native memory corruption.

## Publication and activation (current behavior)

1. Build only the changed module; a Git commit records source but does not itself compile code.
2. Publish an immutable digest-named DLL and atomically replace `ready.toml` last.
3. The host checks digest, entry point, ABI version, layout sizes and callable processor.
4. Keep the previous active library pinned while the candidate processes a complete live batch.
5. Reject invalid results before changing the scene. On rejection, use the previous processor
   for that frame. Promote only a fully validated candidate.
6. Keep the active version and at most three previous loaded versions, plus one staged
   publication. A shared publication lock prevents retention cleanup racing a publisher.
7. Delete only retired regular `pose-<digest>.dll` children of the resolved local module folder.
   Never remove active code or scan/delete outside that folder.

The pose host validates the finite frame time, rig spans, local transforms and parent bounds and
precedence within each rig span before invoking a module. For a non-empty bone batch, rig spans must
tile the whole batch
in order: each span's `first` must equal the previous coverage, a zero-count span advances nothing,
and the final coverage must equal `bones.len()`. A gap, overlap, reordered span or empty span list
with a non-empty bone batch is a host/caller error and never quarantines a digest. A malformed input batch leaves the staged, active and retained revisions
in place. On an active-module failure, predecessors are tried newest first on the same input.
Only a predecessor that produces valid output becomes active. If none succeeds, composition
uses the baseline pose. History holds up to three distinct predecessor digests, so switching
back to a previously active DLL does not consume a rollback slot for the same code.

SHA-1 here is a local content identity/incomplete-copy check, not an authentication protocol.
Remote/WM distribution needs a separate authenticated publication contract and is not enabled.

---

## H02 Specification — Revision Trust, Capability Contract, and Failure Semantics (Proposed)

### 0. End-to-End Candidate Trace in Current Source (`main`)

Before specifying the `H02` contract for `H03`, the table below traces a single candidate from
manifest discovery through loading, evaluation, promotion/rejection, and retention in the existing
codebase:

1. **Manifest discovery (`benilla_hotpatch_bevy::poll` -> `benilla_hotpatch::Host::poll`)**:
   - In `Update`, `benilla_hotpatch_bevy::poll` runs at 1 Hz (`time.elapsed_secs_f64() >= runtime.next_poll`).
   - `Host::poll` reads `<root>/ready.toml`. Missing `ready.toml` (`ErrorKind::NotFound`) returns `Ok(false)`.
   - If `self.observed.as_ref() == Some(&text)`, `Host::poll` returns `Ok(false)` (unchanged manifest already staged, active, or terminally rejected) unless a terminal failure's `ArtifactStamp` no longer matches the artifact, in which case the latch is cleared and the same text is retried once.
   - If an unpromoted candidate is still sitting in `self.pending` when new manifest text arrives (`self.pending.take().is_some()`), `self.observed` is cleared to `None` so reverting `ready.toml` to the prior manifest text can re-stage it.
   - `toml::from_str::<Publication>(&text)` parses `{ revision, library, sha1 }` (`#[serde(deny_unknown_fields)]`).
   - If `self.active` already has `m.digest == publication.sha1`, `self.observed = Some(text)` is latched and `Host::poll` returns `Ok(false)`.
2. **Pre-load verification & library open (`benilla_hotpatch::verify_publication` + `Loaded::open`)**:
   - Enforces `publication.library` is a single normal path component (`Component::Normal(_)`) inside `root` and `!publication.revision.is_empty()`.
   - Verifies file size `<= MAX_LIBRARY_BYTES` (`64 * 1024 * 1024`).
   - Reads the file bytes, computes lowercase hex `Sha1::digest(&bytes)`, and checks `digest == publication.sha1` and `publication.library == format!("pose-{digest}.dll")`.
   - Calls `unsafe { Library::new(&path) }`, resolves `ENTRY` (`b"project_b_pose_module_v1\0"`), invokes `entry()`, checks `!api.is_null()`, copies `Header`, checks `header.compatible()` against `Header::current()`, and extracts `(*api).process: Option<ProcessPose>`.
   - **Retry vs latch (H00 current behavior)**: Only after `Loaded::open` returns `Ok(pending)` does `Host::poll` assign `self.pending = Some(pending)` and `self.observed = Some(text)`. A transient filesystem failure anywhere in the cause chain (`NotFound`, `PermissionDenied`, `Interrupted`, `WouldBlock`, `TimedOut`, or Windows sharing/lock raw OS errors 32/33), or a held `publish.lock`, leaves `self.observed = None`, so the unchanged manifest is retried on the next 1 Hz poll. A malformed-TOML `ready.toml`, a deterministic pre-verification `verify_publication` failure (digest mismatch, wrong immutable filename, size/basename) or a post-verification `Loaded::open` failure (missing entry point, ABI, library load) instead latches `self.observed = Some(text)` and returns `Err` once; the unchanged manifest is thereafter a quiet `Ok(false)` until its recorded `ArtifactStamp` no longer matches the artifact (a changed length or modification time, or a missing file, clears the latch and retries once; for a non-quarantined digest a missing artifact then retries as the transient `Err` path at 1 Hz with the latch unset, while a quarantined digest is gated before the read and re-latches text-only), `self.active` is untouched, and no publication error is written to the shared candidate `self.rejection`. A terminal post-verification `Loaded::open` failure additionally quarantines the verified digest in the bounded `Host.quarantined_digests` store (persisted to `quarantine.toml`), so a changed manifest that still names the same bad digest is refused before any DLL read or `Library::new`; a pre-verification digest/filename/size failure only latches the text and records an artifact stamp when the named library is a stattable basename, and transient I/O, a held `publish.lock` and malformed TOML do not quarantine. A `ready.toml` that is not valid UTF-8 still takes the cheap read-error path and is retried without reading or opening the DLL. `HotpatchEvent::PublicationRejected` is emitted when `runtime.last_error` changes. H03 still adds typed `PatchRejectReason` values and persistence of the artifact stamp; the digest store itself is persisted to `quarantine.toml` (see the persistent quarantine update above).
3. **First-frame canary evaluation, promotion, or rejection (`benilla_hotpatch_bevy::apply_pose` -> `benilla_hotpatch::Host::evaluate`)**:
   - Scheduled in `PostUpdate` in `PoseModulePass` (after `PosePost` and before hierarchy/world composition).
   - `apply_pose` exits if `!runtime.host.has_module()`. Otherwise it sanitizes dirty (or `runtime.refresh`-forced) rigs via `rig.sanitize_locals()`, packs host buffers `runtime.bones: Vec<BoneLocal>` and `runtime.spans: Vec<RigSpan>`, and calls `runtime.host.evaluate(&runtime.bones, &runtime.spans, time.elapsed_secs_f64(), &mut runtime.output)`.
   - Inside `Host::evaluate`:
     - If `bones.is_empty()`, returns `Ok(false)` without taking `self.pending` (candidate stays in `self.pending` until a non-empty rig batch exists).
      - Oversized counts, an out-of-bounds rig span, rig spans that do not tile the bone batch in order (a gap, overlap, reordered span, or empty span list with bones), a non-finite input bone, a non-finite frame time, or a parent that is out of bounds or does not precede its child within its rig span return `Err` before `self.pending`/`self.active` are touched; these host/caller errors never quarantine a digest.
     - If `let Some(candidate) = self.pending.take()`, calls `candidate.evaluate(bones, rigs, seconds, output)`.
     - `Loaded::evaluate` copies `bones` to `output`, builds `PoseFrame`, invokes `(self.process)(&frame, output.as_mut_ptr(), output.len() as u32)`, checks `status == 0`, and verifies `output.iter().zip(bones).all(|(after, before)| after.valid() && after.parent == before.parent)`.
     - **On candidate `Ok(())`**: promotes `candidate` into `self.active` (`self.active.replace(candidate)`), pushes displaced `old` onto `self.previous` (`VecDeque<Loaded>`), trims `self.previous` to `<= 3` (`pop_front()`), and returns `Ok(true)`. `apply_pose` detects the changed revision/digest, emits `HotpatchEvent::Activated { digest }`, writes `runtime.output` into `rig.locals`, and sets `runtime.restore_pending = true`.
     - **On candidate `Err(error)` (current behavior)**: adds `candidate.digest` to `self.quarantined_digests`, drops retained `self.previous` entries with the same digest, drops `candidate`, stores `self.rejection = Some(format!("candidate rejected; active module retained: {error:#}"))`, and immediately falls through in the **same frame** to the retained `self.active` module, evaluating it so the scene does not suffer a one-frame unpatched snap. `apply_pose` drains `take_rejection()`, emits `HotpatchEvent::CandidateRejected`, and applies `self.active`'s output. A successful canary never quarantines.
     - **On active module `Err(error)` (current behavior)**: `Host::evaluate` records the failure and compares it with `PatchHealthPolicy` (defaults 1/3; `0` disables a threshold). Below both thresholds it keeps the active and previous slots, leaves the digest out of quarantine, clears `output`, records `active_failure`, forces a retry refresh in `apply_pose`, and returns `Ok(false)` for a baseline frame. At either threshold it adds `module.digest` to `self.quarantined_digests`, drops retained `self.previous` entries with the same digest, demotes the failed active module, walks `self.previous` newest-first, skips any predecessor whose digest is already quarantined, evaluates the first predecessor that produces a valid batch in the same frame, and returns `Ok(true)`; `apply_pose` emits `FrameFailed` plus `RestoredPrevious`. A predecessor that fails during restore is likewise quarantined, its deeper retained copies are dropped, and the loop continues. With no valid predecessor it clears `self.active`, returns `Ok(false)`, emits `FrameFailed` plus `RestoreUnavailable`, and re-dirties rigs so the baseline pose applies. The bounded pose-digest quarantine now covers terminal `Loaded::open` failures and post-load evaluation failures (canary, active frame, failing restore predecessor) and is persisted to `quarantine.toml`; H03 still adds persisted health/failure counters, typed reasons and persistence of the artifact stamp.
4. **Post-composition local restoration & artifact cleanup (`restore_pose`, `Host::collect_retired`)**:
   - In `Last`, `restore_pose` copies the pre-module `runtime.bones` back into `rig.locals` so unkeyed channels never accumulate patch deltas across frames.
   - During `poll`, `Host::collect_retired` canonicalizes `self.root`, acquires exclusive `publish.lock`, pins filenames from the pose slots (`previous`/`active`/`pending`), the movement slots (`movement_pending`/`movement_active`), and both current manifests, then removes unpinned direct child files matching the pose or movement digest names. A manifest that changed under the sweep, or a malformed latched manifest, aborts the whole sweep rather than guessing.

---

### 1. Proposed Manifest Contract: `PatchRevisionV1`

To support capability gating, publisher authenticity, dependency checks, and bounded automatic
rollback in `H03`–`H05` and `X04`, `ready.toml` is upgraded from the flat 3-field `Publication`
struct to the versioned **`PatchRevisionV1`** schema (`#[serde(deny_unknown_fields)]`).

#### 1.1 Field Specification

| Field / Section | Type | Required | Contract & Validation Rule |
|---|---|---|---|
| `schema_version` | `u32` | Yes | Must equal `1` for `PatchRevisionV1`. Unknown versions are rejected before reading artifact bytes (`UnsupportedManifestVersion`). |
| `revision` | `String` | Yes | Non-empty, bounded (`1..=128` ASCII printable chars) human/build identifier (e.g., `<git-short-sha>-<digest8>-<utc-timestamp>`). |
| `artifact.library` | `String` | Yes | Single `Component::Normal` basename in the publication root. For native modules, must equal `<capability_prefix><sha1>.dll` (e.g., `pose-<40-hex>.dll` via `ModuleKind::prefix`). |
| `artifact.sha1` | `String` | Yes | 40 lowercase ASCII hex digits. Verified against file bytes before loading; identifies content in `HotpatchEvent::Activated { digest }` and `SnapshotV1`. |
| `artifact.sha256` | `Option<String>` | Recommended (Required for signed/WM) | 64 lowercase ASCII hex digits. Cryptographic collision-resistant digest verified alongside `sha1` before `Library::new` and bound into any publisher signature. |
| `artifact.size_bytes` | `u64` | Yes | Exact expected byte size (`1..=MAX_LIBRARY_BYTES`, `64 MiB` cap for DLLs). Mismatch against `fs::metadata(&path)?.len()` is rejected before reading/loading. |
| `trust.origin` | `TrustOrigin` (`"local_unsigned_dev"` \| `"local_signed"` \| `"wm_signed_pilot"`) | Yes | Declares publication source. `"local_unsigned_dev"` is valid only when loaded from local `<home>/Hotpatch` in an opted-in developer build (`--features hotpatch`) and never from network/WM ingress. |
| `trust.publisher_id` | ` String` | Yes | Bounded identifier of the builder/signer (e.g., `"local-workstation"`, `"dicetar-owner-key-1"`). |
| `trust.key_id` | `Option<String>` | Required if signed | Fingerprint/ID of the Ed25519 public key in the host's compiled trust store. Unknown `key_id` rejects with `UnknownPublisher`. |
| `trust.signature_ed25519` | `Option<String>` | Required if `origin != "local_unsigned_dev"` | 128-hex-char Ed25519 signature over canonical UTF-8 bytes of `(schema_version, revision, capability, host_abi, artifact.sha1, artifact.sha256, artifact.size_bytes, dependencies)`. |
| `capability.kind` | `CapabilityKind` (`"pose_v1"` \| `"movement_v1"` \| `"combat_v1"` \| `"space_v1"` \| `"story_v1"` \| `"ui_overlay_v1"` \| `"asset_overlay_v1"`) | Yes | Maps native modules to `benilla_hotpatch_abi::ModuleKind` (`Pose = 1`, `Movement = 2`, `Combat = 3`, `Space = 4`, `Story = 5`) or content/UI stages (`H04`/`H05`). |
| `capability.required_mode` | `"baseline_or_wm"` \| `"wm_enhanced"` \| `"experimental"` | Yes | `"pose_v1"` and `"ui_overlay_v1"` may run in `"baseline_or_wm"` (presentation-only). `"movement_v1"`, `"combat_v1"`, `"space_v1"`, and `"story_v1"` require `"wm_enhanced"` or `"experimental"` mode and an enabled seam in `ModularGameplayRegistry`. |
| `capability.mutates_gameplay_state` | `bool` | Yes | Must be `false` for `"pose_v1"`, `"ui_overlay_v1"`, and `"asset_overlay_v1"`. Any candidate setting `true` in Baseline 3.3.5a mode is rejected before load (`UnsupportedCapability`). |
| `host_abi.abi_version` | `u32` | Yes | Must match `benilla_hotpatch_abi::ABI_VERSION` (`1`). |
| `host_abi.magic` | `u64` | Yes | Must match the capability's ABI magic constant (e.g., `0x5042_504F_5345_3031` for `MAGIC` / `Pose`, `0x5042_4D4F_5645_3031` for `MOVEMENT_MAGIC`, etc.). |
| `host_abi.entry_symbol` | `String` | Yes | Must match `ModuleKind::entry_symbol` without trailing NUL (e.g., `"project_b_pose_module_v1"`). |
| `host_abi.pointer_size` | `u32` | Yes | Must match `std::mem::size_of::<usize>() as u32` (`8` on 64-bit Windows). |
| `host_abi.api_size` | `u32` | Yes | Must match `std::mem::size_of::<ModuleApi>() as u32` (`40` bytes on x86_64) before and after calling `EntryPoint`. |
| `host_abi.frame_size` | `u32` | Yes | Must match `std::mem::size_of::<PoseFrame>() as u32` (`32` bytes on x86_64) or the corresponding seam frame struct size. |
| `host_abi.item_sizes` | `[u32; 2]` | Yes | For `pose_v1`, `[bone_size, rig_size]` = `[44, 16]`. Checked against manifest before `Library::new` and against `Header::compatible()` after `entry()`. |
| `dependencies.min_host_commit` | `Option<String>` | No | Optional 7–40 hex Git SHA prefix or buildstamp check; if set and host build is incompatible, reject with `MissingDependency`. |
| `dependencies.asset_profile` | `Option<String>` | No | Optional required active asset profile (`"base"` or `"wm-hd"`). If set and mismatches the running profile, reject with `MissingDependency`. |
| `dependencies.requires_revisions` | `Vec<String>` | Yes (may be `[]`) | Bounded list (`<= 8`) of prerequisite content/UI/module revision IDs or digests that must already be active so multi-artifact families never activate half-staged. |
| `validation.level` | `"canary_frame_batch"` \| `"canary_window"` | Yes | `"canary_frame_batch"` promotes on 1 complete non-empty batch with valid finite outputs and preserved topology. `"canary_window"` keeps candidate in canary observation for `canary_frames` consecutive batches before marking it `last_good`. |
| `validation.canary_frames` | `u32` | Yes | `1` for `"canary_frame_batch"`; `2..=600` for `"canary_window"`. |
| `rollback.max_consecutive_frame_failures` | `u32` | Yes | `1..=8` (recommended `1`); `0` disables the threshold. When the active module returns `Err` this many consecutive frames, the host demotes and quarantines it and restores the newest valid predecessor (`previous.back()`). The manifest does not feed this value yet: the host uses the in-memory `PatchHealthPolicy` default (`1/3`) or `Host::set_health_policy`. |
| `rollback.max_total_frame_failures` | `u32` | Yes | `1..=32` (recommended `3`); `0` disables the threshold. Cumulative frame failures tolerated for a revision before automatic demotion to the previous module. The in-memory `PatchHealthPolicy` enforces this against the per-activation `PatchHealthSnapshot.total_frame_failures`; a per-digest cumulative store across reactivations and manifest wiring remain H03 (see §5.2 #4). |
| `rollback.max_eval_micros` | `u64` | Yes | Per-frame synchronous evaluation budget in microseconds (recommended `2000` µs). Breaches are recorded in patch health; if `demote_on_budget_breach` is active, triggers rollback. |
| `rollback.retain_previous` | `u8` | Yes | `1..=3` (default `3`, matching `Host.previous` capacity). |
| `rollback.quarantine_on_reject` | `bool` | Yes | Must be `true`: a revision that fails validation or is rolled back has its `artifact.sha1` added to the host's bounded quarantine set (persisted to `quarantine.toml`) so unchanged `ready.toml` is not re-staged. |

#### 1.2 Concrete `PatchRevisionV1` Manifest Example (`ready.toml`)

```toml
schema_version = 1
revision = "08eb2cd2f9f2-a94a8fe5-20261103T083500Z"

[artifact]
library = "pose-a94a8fe5ccb19ba61c4c0873d391e987982fbbd3.dll"
sha1 = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3"
sha256 = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
size_bytes = 184320

[trust]
origin = "local_unsigned_dev"
publisher_id = "local-workstation"

[capability]
kind = "pose_v1"
required_mode = "baseline_or_wm"
mutates_gameplay_state = false

[host_abi]
abi_version = 1
magic = 0x5042504F53453031
entry_symbol = "project_b_pose_module_v1"
pointer_size = 8
api_size = 40
frame_size = 32
item_sizes = [44, 16]

[dependencies]
asset_profile = "base"
requires_revisions = []

[validation]
level = "canary_frame_batch"
canary_frames = 1

[rollback]
max_consecutive_frame_failures = 1
max_total_frame_failures = 3
max_eval_micros = 2000
retain_previous = 3
quarantine_on_reject = true
```

*Transition note for H03*: During migration, `Host::poll` may deserialize an untagged enum
`ManifestEnvelope { V1(PatchRevisionV1), LegacyV0(Publication) }` when `local_unsigned_dev` is
permitted, translating `LegacyV0` into a synthesized `PatchRevisionV1` with `capability.kind = "pose_v1"`
and `rollback.max_consecutive_frame_failures = 1` while `tools/publish-pose-module.ps1` is updated
to emit `PatchRevisionV1`.

---

### 2. Activation Decision Table (Proposed for H03)

The table below defines the exact disposition, retained active module, diagnostic reason, and retry
eligibility for every candidate outcome across `Host::poll` and `Host::evaluate`:

| Case | Stage Detected | Candidate Disposition | What Remains Active | Diagnostic Event & Reason Code | Retry Eligible? |
|---|---|---|---|---|---|
| **0. Transient incomplete staging** (DLL not yet visible or locked by publisher, or `ready.toml` mid-replace) | `Host::poll` pre-read / `fs::metadata` / `fs::read` I/O (`NotFound`, `PermissionDenied`, or `size_bytes` still growing while `publish.lock` exists) | **Deferred (`NotStaged`)** — never loaded | Previous `active` module (`A`) or unpatched baseline if none | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::TransientIoPending`); deduplicated in `Runtime.last_error` | **Yes (Automatic at 1 Hz).** `Host.observed` is **not** latched (H00 invariant), so the next poll retries the unchanged `ready.toml`. |
| **1. Valid local candidate** | `Host::poll` -> `Loaded::open` -> `Host::evaluate` (canary batch `Ok(())`) | **`Staged` -> `Validated` -> `Activated`** (promoted to `Host.active`) | Candidate (`B`) becomes `active`; prior `active` (`A`) pushed to `previous` (`last_good`, cap 3) | `HotpatchEvent::CandidateStaged` on `poll`, then `HotpatchEvent::Activated { digest: Some(sha1_bytes) }` on first validated batch | **N/A (Active).** `Host.observed` latches `ready.toml`; subsequent polls return `Ok(false)`. |
| **2. Digest mismatch** (`Sha1::digest(&bytes) != artifact.sha1`, `Sha256` mismatch, or `library != "<prefix><sha1>.dll"`) | `Host::poll` -> `verify_publication` **before** `Loaded::open`/`Library::new` | **`RejectedBeforeLoad`; text latch plus an `ArtifactStamp` when the library is a stattable basename** (unless `publish.lock` is currently held, in which case treat as Case 0 transient). No digest is quarantined: a mismatch does not prove the computed bytes are bad, and a filename/size failure is a manifest error that a corrected same-bytes republish may fix. | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::DigestMismatch`) -> `quest_trace::IncidentOutcome::ObservedFailure` | **No for identical manifest text unless the artifact changed.** The text latch stops the 1 Hz re-read; a changed text retries once. An identical text whose artifact length/modification time no longer matches the recorded `ArtifactStamp` (or whose artifact is missing) clears the latch and retries once; the stamp is captured before verification reads the bytes so a repair completing during the failed read is not masked. H03 may make the stamp persistent. |
| **3. Unknown publisher** (unrecognized `trust.origin`, unknown `trust.key_id`, missing/invalid `signature_ed25519` when required, or remote origin in local-only mode) | `Host::poll` trust gate **before** `Library::new` | **`RejectedBeforeLoad` & `Quarantined`** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::UnknownPublisher`) -> `IncidentOutcome::ObservedFailure` | **No.** `Host.observed` latches the rejected manifest text and quarantines `artifact.sha1` until a new signed manifest is published. |
| **4. Unsupported capability** (unknown `capability.kind`, `mutates_gameplay_state == true` in Baseline mode, seam disabled in `ModularGameplayRegistry`, or `required_mode` not active) | `Host::poll` capability gate **before** `Library::new` | **`RejectedBeforeLoad`** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::UnsupportedCapability`) -> `IncidentOutcome::ObservedFailure` | **No for unchanged mode/seam state.** Latched in `Host.observed`; becomes retryable only if `ready.toml` changes or the host explicitly transitions into the required capability mode/seam. |
| **5. ABI mismatch** (manifest `host_abi` mismatch before load, or post-load missing `EntryPoint`, null `ModuleApi`, `!Header::compatible()`, or `process.is_none()`) | Pre-load in `Host::poll` (`host_abi` table) **or** post-load in `Loaded::open` before staging `pending` | **`RejectedBeforeStage` & `Quarantined`** (if `Library::new` ran, handle is dropped immediately without staging) | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::AbiMismatch`) -> `IncidentOutcome::ObservedFailure` | **No.** Binary is incompatible with the running host executable; `Host.observed` latches and `artifact.sha1` is persisted to `quarantine.toml` and reloaded at startup. |
| **6. Missing dependency** (`min_host_commit` mismatch, `asset_profile` mismatch, or any entry in `requires_revisions` not currently active) | `Host::poll` dependency gate **before** `Library::new` | **`RejectedBeforeLoad` (`DependencyPending`)** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::MissingDependency`) -> `IncidentOutcome::ObservedFailure` | **Conditional (State-driven, not blind 1 Hz spin).** Not retried on ordinary timer ticks while active revisions/profile are unchanged; re-evaluated when a prerequisite revision or profile activates or `ready.toml` updates. |
| **7a. Activation failure (pre-promotion canary batch failure)** (`Loaded::open` succeeded, but first `candidate.evaluate` returns non-zero status, non-finite transform, zero quaternion norm, or changed `parent`) | `PostUpdate` (`PoseModulePass`): `apply_pose` -> `Host::evaluate` on `self.pending.take()` | **Current (2026-11-09): `RejectedBeforePromotion` and the verified digest is quarantined in the bounded store (persisted to `quarantine.toml`)** (`candidate` dropped; retained `previous` copies with the same digest dropped; a `B -> C -> B` rewrite is refused before `Library::new`).<br>**H03:** typed `PatchRejectReason::CanaryEvaluationFailed`, typed refusal reasons over the persisted digest store, and persisted publication metadata. | Previous `active` module (`A`) evaluates **in the same frame**; zero unpatched frame glitch | `HotpatchEvent::CandidateRejected` with the existing free-form evaluation reason; H03 adds `PatchRejectReason::CanaryEvaluationFailed` -> `IncidentOutcome::ObservedFailure` | **No.** `Host.observed` is already latched and `artifact.sha1` is quarantined; `B` will not be re-staged while it remains in the bounded 32-entry store, which is persisted to `quarantine.toml` (a new publication makes the old manifest moot; cap eviction or deleting the store clears the stale entry, not a restart). |
| **7b. Activation / post-promotion frame failure** (`B` promoted to `active`, then returns `Err` on a frame at or after promotion) | `PostUpdate` (`PoseModulePass`): `apply_pose` -> `Host::evaluate` on `self.active` | **Current (2026-11-10):** the in-memory `PatchHealthPolicy` (default `1/3`) compares `B`'s per-activation counters. A failure **below both enabled thresholds** keeps `B` active, leaves `Host.previous` and the quarantine store untouched, clears the caller's `output`, sets `take_active_failure` (log/diagnostic only, no `RestoreOutcome`) and returns `Ok(false)`, so the caller renders the baseline pose for that frame and `B` is evaluated again next frame. A failure that **reaches a threshold** demotes `B` immediately and quarantines its verified digest in the bounded store (persisted to `quarantine.toml`); retained copies of `B` are dropped; the newest retained predecessor whose digest is not quarantined and which evaluates validly is promoted, or the slot is cleared when none is (`FrameFailed` + `RestoredPrevious`/`RestoreUnavailable`). The snapshot records the failed active evaluation with `revision`/`digest` `None` after a demotion, or reset to `0` when a predecessor is restored.<br>**H03:** persisted health counters, per-revision cumulative accounting, typed quarantine reasons, persisted publication metadata, manifest-supplied thresholds and `max_eval_micros` enforcement. | **Current:** the restored predecessor evaluates **in the same frame**; a predecessor that fails during restore is quarantined and the loop continues; no valid predecessor falls back to the baseline pose. A tolerated failure also falls back to the baseline pose for one frame while keeping `B` active. **H03:** persist the rollback/health record. | `HotpatchEvent::FrameFailed` + `RestoredPrevious`/`RestoreUnavailable` on demotion; a tolerated failure is only reported through `take_active_failure`/`report` (warn log, no recorder event). H03 adds structured `HotpatchEvent::RolledBack` reason codes. | **No.** Once a threshold demotes `B`, the bounded `Host.quarantined_digests` covers the frame failure as well as terminal `Loaded::open` failures and is persisted to `quarantine.toml`, so `Host::poll` refuses to re-stage `B` while it remains in the 32-entry store (cap eviction or a manual file delete clears it; a restart does not). A tolerated failure does not quarantine, so an unchanged manifest naming `B` still matches the active digest and is a quiet `Ok(false)`. |

Host/caller errors are outside this table: an empty bone batch (`bones.is_empty()`, `Ok(false)`, candidate remains staged), oversized counts, a malformed/out-of-bounds `RigSpan`, rig spans that do not tile the bone batch in order (a gap, overlap, reordered span, or empty span list with a non-empty bone batch), a non-finite frame time, a parent that is out of bounds or does not precede its child within its rig span, and a non-finite or zero-norm input bone are refused or skipped before module dispatch and never quarantine a digest. Rig spans must cover every bone exactly once and in order: each span's `first` must equal the previous coverage (zero-count spans advance nothing), and the final coverage must equal `bones.len()`. Parent precedence is stricter than the baseline's tolerant composition (which would treat a forward parent as a root), so a malformed forward-parent frame is rejected before dispatch; shipped rigs are parent-before-child.

---

### 3. Failure and Recovery Sequence Walkthroughs

#### Sequence A: Module `A` is active and Candidate `B` fails validation

1. **Initial state**:
   - `Host.active = Some(A)` (`revision = "rev-a"`, `digest = "<sha1-a>"`).
   - `Host.pending = None`, `Host.previous = []`, `Host.observed = Some("<manifest-a>")`.
   - Each frame in `PostUpdate` (`PoseModulePass`), `apply_pose` sanitizes dirty rigs into `runtime.bones`, evaluates `A` into `runtime.output`, writes `runtime.output` to `rig.locals`, composes world matrices, and in `Last` (`restore_pose`) restores `rig.locals` from `runtime.bones`.
2. **Case A1 — `B` fails pre-stage validation during `poll()` (e.g., digest mismatch, unknown publisher, unsupported capability, or ABI header mismatch)**:
   - Publisher writes `pose-<sha1-b>.dll` and atomically replaces `ready.toml` with `<manifest-b>`.
   - On the next 1 Hz `Update` tick, `benilla_hotpatch_bevy::poll` calls `runtime.host.poll()`.
   - `Host::poll` sees `self.observed != Some(&text_b)`. `self.pending` is `None`.
   - Validation fails inside `Host::poll` / `Loaded::open` before `self.pending` is assigned.
   - **Current host recovery**: `Host.active` (`A`) is never touched. `benilla_hotpatch_bevy::poll` emits `HotpatchEvent::PublicationRejected` and logs the error once via `report`. Later in the same frame (`PostUpdate`), `apply_pose` runs `A` as if nothing happened.
   - **Current host recovery (H00)**: Transient filesystem failures (`NotFound`, `PermissionDenied`, interrupted/blocked/timeout reads, and Windows sharing/lock raw OS errors 32/33) keep `self.observed = None` so the next 1 Hz poll retries. Deterministic content/contract failures (digest/filename/ABI/entry/processor mismatch) latch `self.observed = Some(text_b)` and return `Err` once, so `poll()` does not repeatedly read or `LoadLibrary` the broken artifact every second. A terminal post-verification `Loaded::open` failure also quarantines the bytes-verified digest in the bounded `Host.quarantined_digests` store (persisted to `quarantine.toml`), so a `B -> C -> B` rewrite is refused before `Library::new`; a pre-verification mismatch/filename failure latches the text (plus an artifact stamp when the named library is a stattable basename) but does not quarantine. The same bounded store now also quarantines a rejected canary batch, an active-frame failure and a failing restore predecessor (rows 7a/7b) and is persisted to `quarantine.toml`. **H03 additions**: typed `PatchRejectReason` values, persisted frame-failure health counters, trust/capability/dependency gates, persistence of the artifact stamp and `publish.lock`-aware incomplete-copy detection; the pose transient/terminal latch, same-text artifact-stamp retry, canary/frame quarantine and digest persistence are current behavior (2026-11-10).
3. **Case A2 — `B` passes `Loaded::open` (`CandidateStaged`) and fails batch validation on its first frame (`Host::evaluate`)**:
   - `Host::poll` succeeds: `Host.pending = Some(B)`, `Host.observed = Some("<manifest-b>")`, `runtime.refresh = true`, and `HotpatchEvent::CandidateStaged` is emitted. `Host.active` is still `Some(A)`.
   - In `PostUpdate` (`PoseModulePass`), `apply_pose` forces all non-`AnimParked` rigs into `runtime.bones` (because `runtime.refresh == true`) and calls `runtime.host.evaluate(&runtime.bones, &runtime.spans, seconds, &mut runtime.output)`.
   - `Host::evaluate` takes `B` via `self.pending.take()` and runs `B.evaluate(bones, rigs, seconds, output)`.
   - Suppose `B` writes a `NaN` translation, a zero-norm quaternion, mutates `output[i].parent != bones[i].parent`, or returns non-zero status `3`.
   - **Current host recovery (`crates/benilla-hotpatch/src/lib.rs`, `Host::evaluate`)**:
     - `B.evaluate` returns `Err(...)`.
     - `B` is **not** placed into `self.active` and is dropped at the end of the `if let Some(candidate) = self.pending.take()` block. `B.digest` is added to `self.quarantined_digests`, and retained `self.previous` copies with the same digest are dropped.
     - `self.rejection` is set to `Some("candidate rejected; active module retained: ...")`.
     - Execution immediately falls through in the **same `Host::evaluate` call** to the retained `self.active` module, which runs `A.evaluate(bones, rigs, seconds, output)` (Note: `A.evaluate` starts with `output.clear(); output.extend_from_slice(bones);`, completely wiping any partial/invalid writes `B` made to `output`).
     - `Host::evaluate` returns `Ok(true)`.
     - Back in `apply_pose`, `runtime.host.take_rejection()` returns `Some(error)`, emitting `HotpatchEvent::CandidateRejected` (`quest_trace::PatchEvent::CandidateRejected` -> `IncidentOutcome::ObservedFailure`). Because `result` is `Ok(true)` and `host.revision()` is still `A`, `apply_pose` does **not** emit `HotpatchEvent::Activated`; it writes `A`'s valid `runtime.output` into `rig.locals`, and `restore_pose` restores `rig.locals` in `Last`.
   - **Current (2026-11-09)**: the bounded `Host.quarantined_digests` store exists and refuses a bytes-verified digest rejected by a terminal post-verification `Loaded::open` failure; a pre-verification mismatch/filename failure only latches the text and records an artifact stamp when stattable. The same store also quarantines a rejected canary, a failing active frame and a failing restore predecessor, so a `B -> C -> B` rewrite is refused before `Library::new`, and it is loaded from and saved to `quarantine.toml` at startup/1 Hz. **H03 additions:** typed reasons, persisted health counters/thresholds, persistent artifact stamps; the pose loading and staged-canary crash sentinels are implemented in the `Host::poll`/`Host::evaluate`/`HotpatchPlugin::build` sentinel slice.

#### Sequence B: Candidate `B` activates and then fails on its first frame (or a subsequent frame after promotion)

1. **How this sequence arises**:
   - **Scenario B1 (Empty-rig window followed by first active rig frame, or frame-2 input trigger)**:
     - Currently, when `runtime.bones.is_empty()` (e.g., while on a loading screen or scene with zero dirty rigs), `Host::evaluate` returns `Ok(false)` before inspecting `self.pending`. Once at least one rig is evaluated on Frame $T_0$, `B` succeeds on that rig batch and is promoted to `Host.active = Some(B)`, moving `A` into `Host.previous.push_back(A)` and emitting `HotpatchEvent::Activated { digest: Some(sha1_b) }`.
     - On the immediately following frame $T_1$ (or when `validation.level` promotes `B` and `B` encounters a new rig/animation input or time delta `time_seconds`), `B.evaluate` returns `Err(...)` (non-zero status, `NaN`/`Inf` output, or topology violation).
2. **What the current host does (`main`)**:
   - In `Host::evaluate` (`crates/benilla-hotpatch/src/lib.rs`, `Host::evaluate`), `self.pending` is `None`, so it takes `self.active` (`B`) and calls `B.evaluate(bones, rigs, seconds, output)`.
   - `B.evaluate` returns `Err(error)`. `Host::evaluate` records the failure and compares it with `PatchHealthPolicy` (defaults 1/3; `0` disables a threshold). Below both thresholds `B` stays active, `previous` is untouched, the digest is not quarantined, `output` is cleared, `active_failure` is recorded, `apply_pose` forces a retry refresh, and the host returns `Ok(false)` for a baseline frame. At either threshold it adds `B.digest` to `self.quarantined_digests`, drops retained `self.previous` copies of that digest, and demotes `B` (it was already taken from `self.active`). In the same call it pops `self.previous` newest-first, skips any predecessor whose digest is already quarantined, and evaluates the first remaining predecessor. A predecessor that also fails is quarantined and the loop continues; a successful one becomes `self.active` and the host returns `Ok(true)`; with none valid `self.active` stays `None` and the host returns `Ok(false)` with `RestoreOutcome::Unavailable`.
   - In `benilla_hotpatch_bevy::apply_pose` (`crates/benilla-hotpatch-bevy/src/lib.rs`):
     - It drains `take_restore_outcomes()` and `take_active_failure()`, emitting `HotpatchEvent::FrameFailed` plus `RestoredPrevious` or `RestoreUnavailable`; a restored predecessor's valid output is written to `rig.locals`, while no valid predecessor leaves the baseline `PosePost` locals in place. A tolerated below-threshold failure (`active_failure` with no restore outcome) sets `runtime.refresh` so the retained module is evaluated again on the next frame.
     - `runtime.refresh` is set from the restore outcome, `restore_pose` restores the pre-module locals when `restore_pending` was set, and `take_rejection()` is empty because an active-frame failure does not reuse the candidate-rejection channel.
   - **Remaining H03 gap**: the current host compares the active module's failures against the in-memory `PatchHealthPolicy` (default `1/3`) and a below-threshold failure keeps `B` active for a later frame (baseline pose for that frame); a threshold-reaching failure demotes and quarantines. Typed `PatchRejectReason` values, persistent health/policy/artifact stamps, per-revision cumulative accounting, manifest-supplied thresholds, `max_eval_micros` enforcement, movement health and a structured `RolledBack` event remain open. The digest quarantine (persisted to `quarantine.toml`) already prevents a demoted `B` from being re-staged by a `B -> C -> B` manifest rewrite while the digest remains in the bounded 32-entry store.
3. **Current in-frame behavior and remaining H03 additions**:
   - **Current (2026-11-10):** after `PatchHealth::record_failure()` on an active `Err`, `PatchHealth::threshold_reached` compares the per-activation counters with `PatchHealthPolicy`. Below both enabled thresholds the host restores `B` into `self.active`, clears `output`, sets `active_failure` and returns `Ok(false)` with no `RestoreOutcome`, so the caller uses the baseline pose for one frame and retries `B` next frame. At a threshold the host removes `B` from `self.active`, inserts `B.digest` into `self.quarantined_digests`, drops retained copies, pops predecessors newest-first (skipping quarantined digests), and evaluates the first remaining one in the same `Host::evaluate` call; with none valid, it clears `self.active`. The snapshot records the failed evaluation (counters/timing retained) with `revision`/`digest` `None` after the demotion, or resets all four values when a predecessor is restored. The default policy is the original `max_consecutive_frame_failures = 1` disposition; a manifest does not yet supply the thresholds.
   - **H03:** persist health/policy state and artifact stamps, enforce per-revision cumulative `max_total_frame_failures` and `max_eval_micros`/`canary_frames_remaining`, retain `B` in a bounded retired list, emit structured `HotpatchEvent::RolledBack { ... }`, and record movement-seam health. The digest quarantine is already persisted. The pose adapter already re-emits `Activated` for a restored predecessor and updates `runtime.reported_revision`/`reported_digest` and `quest_trace::History.last_observed_patch_digest`.
   - **Next-launch recovery for fatal native faults (`active-sentinel.toml`)** (current behavior in
     the pose and movement crash-sentinel slices):
     - If `B` faults the process (`STATUS_ACCESS_VIOLATION`, illegal instruction, stack overflow, or `abort`) during `Library::new`, `entry()`, or the staged canary `process()`, in-process Rust code cannot recover that process.
     - The host writes `<root>/active-sentinel.toml` containing `{ revision, sha1, kind = "pose", stage = "loading" | "canary" }` (digest-only, no library path). The loading marker is flushed before `Library::new` and cleared immediately after `Loaded::open` returns; the canary marker is written immediately before the staged candidate's `Loaded::evaluate` and cleared immediately after that call returns. A native fault before the matching clear leaves the marker, while a regular failure clears it and keeps the existing transient/terminal handling.
     - On the next client startup, before `Host::poll` reads `ready.toml`, `Host::load_active_sentinel` inspects `active-sentinel.toml`. If an unclean pose marker of either stage or a movement loading marker is present, the host adds its `sha1` to the matching digest quarantine (and persists it to `<root>/quarantine.toml`), so a `B -> C -> B` rewrite is refused before `Library::new`; a malformed document, unknown field, invalid digest or unsupported kind/stage combination is an error that leaves the file and both stores unchanged (fail-safe). Startup recovery warns once and never fails the plugin.
     - Native faults during steady-state active evaluation, predecessor restore, and `Loaded` drop/`FreeLibrary`/DLL-detach are not covered by this slice; a regular canary rejection's digest quarantine is persisted by the next 1 Hz save; the movement loading marker covers `MovementLoaded::open` and the movement staged-canary marker covers the first staged `process` call (staging has no production caller yet); active movement per-frame `process` calls remain future H03 work.
     - On the explicit `AppExit` edge, the `Last` system `clear_active_sentinel_on_app_exit` (declared ahead of `capture_pose`/`restore_pose`) runs `Host::clear_active_sentinel_on_clean_shutdown` while an `AppExit` message is present; `Runtime` drop calls the same method on a bare `App` drop with no panic in flight. A stale marker left by a failed call-scoped clear is retried; a failed retry retains it, but a marker that records an uncommitted startup recovery, an unvalidated `load_active_sentinel` error, a marker this host never accepted, or an owned marker later replaced with a different `{ revision, sha1, kind, stage }` identity is retained for the next launch/diagnosis. During panic unwind the destructor returns before clearing, so a marker for the operation that panicked survives. This is a clean-exit safety net, not a crash path, and does not substitute for the call-scoped clears; `process::exit`/hard kills bypass both edges, Drop cannot distinguish an orderly exit from a bare `App` drop, and a second process sharing the root (cross-process/file-identity ownership) is out of scope (H03).

#### Native Side Effects That Cannot Be Rolled Back

Even with `H03` last-good rollback and `restore_pose` input restoration, the host and `H03`
implementer must treat the following native side effects as **irreversible within the running OS
process**:

1. **`DllMain` and static initializers**: Windows executes `DllMain(DLL_PROCESS_ATTACH)` and C/C++/Rust `.CRT$XCU` static constructors inside `unsafe { Library::new(&path) }` before `Loaded::open` can inspect `ENTRY` or `Header`.
2. **Arbitrary process memory corruption**: Because a native DLL shares the host's virtual address space, rogue or buggy pointer writes outside the provided `output: *mut BoneLocal` slice can corrupt the Rust heap, Bevy ECS `World` storage, stack frames, or global state. Output validation (`BoneLocal::valid()`) cannot detect out-of-bounds writes that already occurred during `process()`.
3. **Escaping threads, TLS, and OS handles**: If a module violates the ABI contract by spawning a background thread, registering a Win32 window hook/vectored exception handler, mutating environment variables, or opening sockets/files, dropping `libloading::Library` (`FreeLibrary`) while a spawned thread still executes instructions in that DLL's code pages will immediately crash the process (`STATUS_ACCESS_VIOLATION`).
4. **Hardware exceptions and FFI unwinds**: Access violations, division-by-zero traps, stack overflows, `std::process::abort()`, or Rust panics unwinding across `extern "C" fn` cannot be caught safely by the host's transform validator and will terminate the client process (requiring the `active-sentinel.toml` next-launch quarantine above).
5. **External filesystem or network side effects**: Any direct OS syscall made by a loaded DLL during its execution cannot be rolled back by the host.

---

### 4. Trust Boundary and Capability Gate

#### 4.1 SHA-1 Content Identity Is Not Publisher Authentication

- In `verify_publication` (`crates/benilla-hotpatch/src/lib.rs`), `Sha1::digest(&bytes)` verifies only that the DLL file on disk matches `publication.sha1` and the `pose-<digest>.dll` filename; `Loaded::open` then loads that verified artifact.
- This protects against partial file writes and accidental path mix-ups, and gives the diagnostic recorder (`HotpatchEvent::Activated { digest }`) a deterministic 20-byte content identifier.
- **It provides zero cryptographic authentication of who built or authorized the DLL.** Any local process or script able to write to `<home>/Hotpatch` can compute the SHA-1 of arbitrary native code and write a matching `ready.toml`, and SHA-1 is not collision-resistant against an attacker.
- Under `H02`, `artifact.sha1` remains the local content-addressing key for backward compatibility with `HotpatchEvent::Activated { digest: Option<[u8; 20]> }` and `SnapshotV1`, while `artifact.sha256` and `trust.signature_ed25519` (verified against a host-pinned Ed25519 public key table) are required whenever `trust.origin != "local_unsigned_dev"`.

#### 4.2 Native DLLs Execute with Full Host-Process Privileges

- Once `Host::poll` invokes `unsafe { Library::new(&path) }`, the operating system maps the binary into `benilla.exe` with the full privileges of the logged-in OS user.
- The loaded code has technical access to the entire process address space (including active network sockets, session state in memory, and user filesystem paths) as well as the GPU/windowing context.
- Consequently, the ABI boundary (`#[repr(C)]` value slices, no `World` pointer) is an **architectural and fault-containment seam for benign/cooperative developer modules**, not a security sandbox against untrusted or malicious native code.

#### 4.3 What the Pre-Load Capability Gate Enforces Before `Library::new`

Because `Library::new` is the irreversible trust boundary, all policy, authenticity, capability,
and dependency checks in `H03` **must execute on the parsed `PatchRevisionV1` manifest and raw file
bytes before calling `Library::new`**:

1. **Crash-sentinel & quarantine check**: Reject any candidate whose `artifact.sha1` appears in `quarantined_digests` or an unclean `active-sentinel.toml` from a prior crashed run; the implemented pose loading/canary and movement loading/staged-canary slices realize this through `Host::load_active_sentinel` startup recovery (quarantine plus persist) before the first `Host::poll`, while the movement active per-frame proposal window remains H03.
2. **Manifest schema & bounds check**: Reject unknown `schema_version`, unknown TOML fields (`#[serde(deny_unknown_fields)]`), non-basename `artifact.library`, or `artifact.size_bytes > MAX_LIBRARY_BYTES` (`64 MiB`).
3. **File size & dual-digest check**: Read `<root>/<artifact.library>` once into a host `Vec<u8>`, verify `bytes.len() as u64 == artifact.size_bytes`, verify `Sha1::digest(&bytes) == artifact.sha1`, and (when present) verify `Sha256::digest(&bytes) == artifact.sha256`.
4. **Trust origin & cryptographic signature check**:
   - If `trust.origin == "local_unsigned_dev"`, verify the host was launched with local developer hotpatching enabled and that the manifest originated from the local filesystem `<home>/Hotpatch` directory.
   - If `trust.origin` is `"local_signed"` or `"wm_signed_pilot"`, or if owner policy requires signed local modules, verify `trust.key_id` is in the host's allowlisted Ed25519 public keys and verify `trust.signature_ed25519` over the canonical manifest + artifact digests **before** touching `Library::new`.
5. **Capability, protocol-mode, and seam-registry check**:
   - Map `capability.kind` to `benilla_hotpatch_abi::ModuleKind`.
   - Verify `artifact.library` starts with `ModuleKind::prefix()` (e.g., `pose-`, `movement-`, `combat-`, `space-`, `story-`).
   - Verify `capability.mutates_gameplay_state == false` whenever the client is in **Baseline 3.3.5a mode** (`CONTEXT.md` regression invariant).
   - Consult `ModularGameplayRegistry::is_seam_enabled(kind)`: if the seam is disabled (or if the host only implements the `Pose` runner in the current slice), reject with `PatchRejectReason::UnsupportedCapability` before `Library::new`.
6. **Pre-load ABI metadata & dependency check**:
   - Verify `host_abi.abi_version`, `host_abi.magic`, `host_abi.entry_symbol`, `host_abi.pointer_size`, `host_abi.api_size`, `host_abi.frame_size`, and `host_abi.item_sizes` match the host's compiled constants for that `ModuleKind`.
   - Verify `dependencies.min_host_commit`, `dependencies.asset_profile`, and `dependencies.requires_revisions` against active host state.

#### 4.4 Patch Classes Left Unsupported by This Proposal

The following patch classes are explicitly **unsupported** by `ADR-0002` / `H02` and must be
rejected or handled via a full client build/restart:

1. **Unauthenticated or server-pushed native code execution**: Accepting native DLLs or executable bytecode over the stock 3.3.5a game socket, addon channels (`WM_FORGE3`), or unauthenticated `wm-native` loopback sockets.
2. **Bevy ECS / Rust ABI plugins across DLLs**: Passing `&mut World`, `Commands`, `Res`/`Query`, trait objects, `Vec`/`String`, or Rust allocators across a dynamic library boundary, or dynamically adding/removing Bevy `Plugin` / `Component` types at runtime.
3. **Gameplay-authority overrides in Baseline 3.3.5a mode**: Any module (`movement_v1`, `combat_v1`, `space_v1`, `story_v1`) activating in Baseline mode or claiming client authority over server-governed health, loot, quest state, copper, inventory, or wire framing without an active WM-enhanced session negotiation.
4. **Stateful or asynchronous native modules**: Modules that spawn worker threads, retain pointers to `PoseFrame` / `BoneLocal` / `RigSpan` after `process()` returns, allocate persistent per-entity heap state without host supervision, or block on I/O.
5. **In-place skeleton topology or bone-count mutation**: Changing `BoneLocal.parent`, adding/removing joints in `PoseModulePass`, or mutating M2 bind matrices without a full model-family reimport revision (`H05`).
6. **Transactional GPU shader rollback via Bevy's default watcher alone**: While Bevy's `embedded_watcher` reloads WGSL source in dev builds, it does not provide multi-asset transactional rollback or signed revision gating.
7. **Core host schema, protocol decoder, or wgpu/Bevy version upgrades**: Requires a declared maintenance rebuild/restart (`owner-decisions.md` row 1).

---

### 5. Existing Symbols, H03 Implementation Contract, and Open Owner Decisions

#### 5.1 Exact Existing Symbols Behind Current-Behavior Claims

Every claim about current behavior in this ADR maps to the following symbols in `main`:

| Crate & File | Existing Symbols | Current Role in `main` |
|---|---|---|
| `crates/benilla-hotpatch/src/lib.rs` | `MAX_LIBRARY_BYTES`, `MAX_QUARANTINED_DIGESTS`, `QUARANTINE_FILE`, `QUARANTINE_TEMP`, `Publication`, `QuarantineFile`, `quarantine_entry_digest`, `accepted_quarantine_digests`, `write_quarantine_temp`, `write_sentinel_temp`, `ACTIVE_SENTINEL_FILE`, `ACTIVE_SENTINEL_TEMP`, `MAX_ACTIVE_SENTINEL_BYTES`, `SentinelKind`, `SentinelStage`, `ActiveSentinelFile`, `Host::write_active_sentinel`, `Host::clear_active_sentinel`, `Host::clear_owned_active_sentinel_if_matching` (private identity compare-and-clear), `Host::clear_active_sentinel_on_clean_shutdown`, `Host::load_active_sentinel`, `Host.sentinel_error` (private), `Host.sentinel_recovery_pending` (private), `Host.sentinel_marker_owned` (private, `Option<ActiveSentinelFile>`: the exact `{ revision, sha1, kind, stage }` identity this host wrote or accepted; not a sticky bool), `Host::take_sentinel_error`, `ArtifactStamp`, `artifact_stamp`, `ArtifactStamp::matches_current`, `verify_publication`, `Loaded`, `Loaded::open`, `Loaded::evaluate`, `Loaded::probe` (test-only), `PatchHealthSnapshot`, `PatchHealth` (private), `PatchHealthPolicy`, `Host`, `Host.health` (private), `Host.health_policy`, `Host.health_snapshot`, `Host::health_policy`, `Host::set_health_policy`, `Host.pose_failure_stamp`, `Host::new`, `Host::load_persistent_quarantine`, `Host::persist_quarantine`, `Host::poll`, `Host::quarantine_digest`, `Host::is_quarantined`, `Host::has_module`, `Host::revision`, `Host::digest`, `Host::take_rejection`, `Host::take_active_failure`, `Host::take_restore_outcomes`, `Host::collect_retired`, `Host::evaluate` | Reads `<root>/ready.toml`, verifies basename, `<= 64 MiB` cap, and SHA-1 (`pose-<sha1>.dll`), opens DLL via `libloading::Library::new`, checks `ENTRY` and `Header::compatible()`, stages `Host.pending`, promotes on first valid `Host::evaluate` batch (whose rig spans must tile the whole bone batch in order, so gaps, overlaps, reordering and empty span lists are refused before dispatch), retains `Host.active` on candidate rejection, demotes a failing active module and restores the newest valid predecessor in the same frame (else the baseline pose), pins up to 3 predecessors in `Host.previous`, and garbage-collects unpinned pose/movement artifacts under `publish.lock`. A terminal post-verification `Loaded::open` failure, a rejected canary batch and a failing active or restore-predecessor evaluation add the bytes-verified digest to the bounded `Host.quarantined_digests` (`VecDeque<String>`, cap 32, persisted to `quarantine.toml`) and drop retained copies, so a changed manifest naming the same digest is refused before `Library::new`; the restore loop skips a predecessor whose digest is already quarantined. A terminal `verify_publication`/`Loaded::open` failure records `Host.pose_failure_stamp` from the parsed publication's artifact (captured before the verification read; only a single normal basename inside the root), so an identical manifest text whose artifact length/modification time changed (or whose artifact is missing) clears the latch and retries once; malformed TOML, a non-basename library name and the digest gate record no stamp, while a post-verification load failure records one despite quarantining the digest. Pre-load verification failures (basename/revision/size/digest/filename) and out-of-bounds/out-of-precedence host/caller frames only latch or return an error without quarantining; rig spans must tile the whole bone batch in order, and a gap, overlap, reorder or empty span list with bones is refused without quarantining. The artifact stamp is in-memory only; both digest quarantines are persisted to `<root>/quarantine.toml` (digest-only `QuarantineFile { pose, movement }`, oldest first) through `Host::persist_quarantine` after each 1 Hz poll and `Host::load_persistent_quarantine` once at plugin startup, while persisted health counters, typed reasons and a persistent artifact stamp remain H03. The in-memory `PatchHealthSnapshot` reports the active module's `revision`/`digest`, frame-failure counters and evaluation timings; only the active `Loaded::evaluate` call is timed. `Loaded::probe` is `#[cfg(test)]` only and constructs the same handle with no pinned native library so these paths are testable without a DLL. The pose crash sentinel writes `<root>/active-sentinel.toml` after digest verification and before `Loaded::open` (stage `loading`) and immediately before the staged candidate's `Loaded::evaluate` (stage `canary`), clears it when the matching call returns, and recovers either stage through `Host::load_active_sentinel`, with best-effort errors through `Host::sentinel_error`. |
| `crates/benilla-hotpatch/src/lib.rs` | `ModularGameplayRegistry`, `ModularGameplayRegistry::new`, `ModularGameplayRegistry::set_seam_enabled`, `ModularGameplayRegistry::is_seam_enabled`, `Host::set_seam_enabled`, `Host::is_seam_enabled`, `MovementRefusal::{SeamDisabled, Publication, Quarantined}`, `MovementLoadFailure`, `MovementLoaded::verify`, `MovementLoaded::open`, `MovementLoaded::probe_with_digest` (test-only), `MovementLoaded::probe` (test-only), `MovementSelection`, `movement_frame_valid`, `Host::stage_movement_processor`, `Host::movement_proposal`, `Host::movement_process`, `Host::commit_movement_processor`, `Host::withdraw_movement_proposal`, `Host::reject_movement_candidate`, `Host::refuse_movement`, `Host.movement_proposal_ready`, `Host.movement_canary_attempted` (private), `Host.movement_quarantined_digests`, `Host::quarantine_movement_digest`, `Host::is_movement_quarantined`, `Host.movement_failure_stamp` | Tracks runtime enablement booleans for `ModuleKind::{Pose, Movement, Combat, Space, Story}` (`Pose` is always `true`; gameplay seams default to `false`). `Host::stage_movement_processor` consults the movement seam before opening the artifact, latches a disabled publication so retention keeps it, and clears that latch on enable so the next stage call can publish the same manifest. Disabling evicts the active/staged processor but keeps the manifest latch until enable, so the GC sweep still pins the publication. A deterministic TOML parse, verification or post-verification load failure latches `Host.movement_observed`; a bytes-verified post-verification `MovementLoaded::open` failure adds its digest to `Host.movement_quarantined_digests` (cap 32, dedupe, oldest eviction), so a changed manifest naming the same artifact (`B -> C -> B`) is refused as `MovementRefusal::Quarantined` before `Library::new`. Transient I/O, or a held `publish.lock` during read/verify/open, retries unchanged; a malformed TOML parse latches without consulting the lock because it never opens an artifact. A terminal verify/load failure with a parsed basename records `Host.movement_failure_stamp` (captured before `MovementLoaded::verify` reads the bytes; only a normal basename inside the root); an identical-text stage call retries once when the artifact length/modification time changed or the file is missing, and enabling the seam clears the stamp with the latch. While the artifact stays missing the unchanged manifest returns the transient `Err` at 1 Hz with the latch unset, unless the digest is quarantined: the gate re-latches with no stamp and the sweep can run. A post-verification load failure records a stamp despite quarantining its digest (a later byte change yields one `Err(Quarantined)`). The stamp is in-memory only. `Host::movement_process` returns the selected slot's processor, revision, verified digest and staged flag as a private `MovementSelection`. `Host::movement_proposal` adds that selected digest to the same store when the module returns a non-zero `Status` or its output fails `validate_movement_output`, then drops only the failing candidate through `Host::reject_movement_candidate` (a failed staged candidate retains a healthy active with a different digest; an active failure clears both slots), so a `B -> C -> B` rewrite is refused before `Library::new`. `Host::movement_proposal` arms the private `Host.movement_proposal_ready` gate only for a valid `Staged` proposal; `Host::commit_movement_processor` promotes only while that flag is set, then clears it, so staging alone or a retained `FrameInput` cannot activate a candidate, while a `Status`/validation failure drops the candidate. `FrameInput` and `NoModule` leave the staged/active selection intact and never quarantine; `SeamDisabled` evicts both without quarantining; a valid proposal/commit never quarantines. This is the single-strike disposition, including output refused against valid caller-supplied `MovementBounds`: an input-dependent failing digest is refused until cap eviction or deleting `quarantine.toml` while the client is stopped (the store is persisted, so a restart does not clear it). `movement_frame_valid` rejects non-finite scalar frame fields (except the `liquid_height` `-INFINITY` sentinel), invalid bounds, and caller-seeded facing/speeds outside those bounds before dispatch, so those caller errors cannot quarantine; input `move_flags` may carry non-delegated root/falling/swimming state and only output flags are checked. Persistent/typed H03 reasons, frame counters, and the production Bevy movement adapter remain future work; both digest stores are persisted together in `<root>/quarantine.toml` (see `Host::load_persistent_quarantine`/`Host::persist_quarantine`); the existing single-slot `MovementProcessor` in `crates/benilla-app/src/player/controller.rs` cannot yet express a refused staged candidate beside a retained host active. The movement loading-window sentinel writes `<root>/active-sentinel.toml` with `kind = "movement"`, `stage = "loading"` after `MovementLoaded::verify` and before `MovementLoaded::open`, clears it when the open returns; the movement staged-canary sentinel writes the same file with `stage = "canary"` immediately before the first staged `process` call and clears it when that call returns (one-shot, fail-open); and startup recovery routes a `Movement`/`Loading` or `Movement`/`Canary` marker to `Host::quarantine_movement_digest`. |
| `crates/benilla-hotpatch/src/lib.rs` | `transient_io`, `publish_in_progress`, `tests::poll_retries_unchanged_manifest_after_transient_load_failure`, `tests::malformed_manifest_latches_after_one_error_and_retries_when_rewritten`, `tests::publish_lock_keeps_a_digest_mismatch_retryable_until_the_publisher_finishes`, `tests::transient_io_classifies_only_filesystem_availability`, `tests::quarantined_digest_is_refused_before_loaded_open_across_manifest_rewrites`, `tests::digest_mismatch_latches_without_quarantining_the_declared_digest`, `tests::wrong_immutable_filename_latches_without_quarantining_the_verified_digest`, `tests::publish_lock_keeps_a_post_verification_failure_retryable_until_the_publisher_finishes`, `tests::missing_dll_transient_failure_does_not_quarantine`, `tests::malformed_manifest_does_not_quarantine_a_digest`, `tests::quarantine_store_dedupes_and_is_bounded`, `tests::active_sentinel_dirty_pose_is_quarantined_and_persisted`, `tests::active_sentinel_canary_stage_is_quarantined_and_persisted`, `tests::active_sentinel_missing_file_is_none`, `tests::active_sentinel_failures_keep_the_file_and_both_stores`, `tests::active_sentinel_write_clears_and_missing_clear_is_ok`, `tests::active_sentinel_marker_survives_a_persist_failure`, `tests::active_sentinel_writer_rejects_an_oversized_serialization`, `tests::clean_shutdown_clears_a_stale_marker`, `tests::clean_shutdown_keeps_a_pending_recovery_marker`, `tests::clean_shutdown_without_a_marker_is_ok`, `tests::clean_shutdown_keeps_an_unvalidated_marker`, `tests::clean_shutdown_keeps_an_unloaded_marker`, `tests::failed_reload_drops_marker_ownership`, `tests::clean_shutdown_clears_only_the_matching_owned_marker`, `tests::clean_shutdown_keeps_a_foreign_replacement_marker`, `tests::clean_shutdown_treats_a_missing_owned_file_as_clean`, `tests::clean_shutdown_keeps_an_unparseable_replacement_of_an_owned_marker`, `tests::pending_sentinel_recovery_is_committed_before_a_new_marker`, `tests::poll_keeps_a_pending_recovery_marker_when_its_commit_fails`, `tests::regular_pose_load_failure_leaves_no_sentinel_after_poll`, `tests::successful_canary_clears_the_marker`, `tests::rejected_canary_clears_the_marker_and_quarantines`, `tests::canary_marker_exists_during_the_canary_call`, `tests::canary_sentinel_write_failure_is_fail_open`, `tests::pending_recovery_blocks_a_canary_marker_without_losing_the_old_one`, `tests::candidate_rejection_quarantines_digest_and_drops_retained_copy`, `tests::quarantined_canary_digest_is_refused_before_the_next_poll_reads_the_artifact`, `tests::active_frame_failure_quarantines_digest_and_reports_unavailable`, `tests::restore_skips_a_quarantined_newest_predecessor_and_restores_the_next`, `tests::failing_restore_predecessor_is_quarantined_and_the_next_valid_one_is_restored`, `tests::successful_canary_promotes_without_quarantining`, `tests::active_frame_failure_records_health_and_quarantine`, `tests::restored_predecessor_resets_health_counters`, `tests::failing_restore_predecessor_does_not_increment_active_failures`, `tests::active_success_after_failure_resets_consecutive_and_keeps_total`, `tests::candidate_canary_failure_does_not_change_active_health`, `tests::candidate_promotion_resets_health_counters`, `tests::health_snapshot_quarantine_count_matches_store`, `tests::health_unchanged_by_empty_batch_and_host_errors`, `tests::health_counters_saturate_and_keep_high_water_mark`, `tests::default_health_policy_demotes_on_the_first_active_failure`, `tests::consecutive_threshold_keeps_the_active_module_for_one_more_frame`, `tests::total_threshold_demotes_with_consecutive_disabled`, `tests::zero_thresholds_disable_demotion`, `tests::snapshot_reflects_policy_counters_and_active_identity`, `tests::host_input_validation_errors_do_not_quarantine_or_dispatch`, `tests::valid_multi_bone_parent_chain_is_dispatched`, `tests::host_rig_span_gap_overlap_reordering_and_empty_rigs_are_rejected_before_dispatch`, `tests::zero_count_tail_span_is_accepted_and_promotes`, `tests::zero_count_spans_are_accepted_at_leading_and_middle_positions`, `tests::pose_same_text_repair_retries_after_the_artifact_changes`, `tests::pose_same_length_repair_with_a_changed_mtime_retries`, `tests::pose_same_mtime_length_change_retries`, `tests::pose_missing_artifact_retries_without_quarantine_and_skips_the_sweep`, `tests::pose_quarantined_missing_artifact_relatches_at_the_gate_and_allows_the_sweep`, `tests::pose_unchanged_artifact_stays_latched_after_terminal_failure`, `tests::pose_malformed_manifest_leaves_no_artifact_stamp`, `tests::artifact_stamp_requires_a_basename_inside_the_publication_root` | Focused unit tests (H00) proving the split: a transient `NotFound` or a held `publish.lock` leaves `Host.observed == None` and retries the unchanged manifest; a malformed-TOML manifest or a deterministic `verify_publication`/`Loaded::open` error latches `Host.observed` and reports `Err` once, then is a quiet `Ok(false)` until the text changes or the recorded artifact stamp mismatches; `transient_io` classifies only `NotFound`, `PermissionDenied`, `Interrupted`, `WouldBlock`, `TimedOut`, plus Windows sharing/lock raw OS errors 32/33. A non-UTF-8 manifest retries the cheap read without opening the DLL. The artifact-stamp cases prove a same-text in-place repair whose length changed is retried, reaches the post-verification `Loaded::open` failure and quarantines the proved digest; a same-length repair detected by a changed modification time also retries and re-verifies, and a same-mtime length change retries too; a missing artifact clears the latch, takes the transient path, leaves a retired artifact uncollected (the sweep is skipped, not run), and a recreated correctly hashed artifact reaches loading and quarantines; a missing already-quarantined artifact is refused at the gate, re-latches text-only and lets the sweep collect the retired artifact; a different still-bad length re-latches with a fresh stamp and then stays quiet; an unchanged artifact keeps `pose_failure_stamp` and stays a quiet `Ok(false)`; a byte change after that quarantine produces exactly one gate refusal with no stamp and then a quiet latch; a malformed TOML records no stamp; and the stamp helper refuses nested/parent paths before stat'ing outside the publication root. The quarantine cases prove a terminal post-verification `Loaded::open` failure quarantines the verified digest so a `B -> C -> B` rewrite is refused before `Library::new` (even with the DLL deleted), while a pre-verification digest mismatch, a wrong immutable filename, a transient missing DLL and malformed TOML do not quarantine, and that the store dedupes and evicts oldest at cap 32. The evaluation cases use `#[cfg(test)] Loaded::probe` handles with in-process pose processors: a rejected canary with a retained active fallback quarantines the candidate digest, drops its retained copy and names the candidate in `take_rejection`; the rejected digest is refused by `poll` before the absent DLL is read and latches `observed`; an active frame failure with no predecessor clears the active slot, quarantines the digest and reports `Unavailable`; the restore loop skips a quarantined newest predecessor and restores the next valid one; a predecessor that fails during restore is quarantined, its deeper copies are dropped and the next valid one is restored; a successful canary promotes without quarantining; the health cases prove the initial snapshot is empty, an active failure records `consecutive_frame_failures`/`total_frame_failures` = 1 with retained timings and `revision`/`digest` `None` when no predecessor remains, a restored predecessor and a promoted candidate reset the counters/timings, a failing restore predecessor does not add a second active failure, an active success clears only the consecutive streak, a candidate canary failure leaves the active counters unchanged and grows the quarantine, the snapshot's quarantine count tracks the pose store and dedupes, an empty batch or host/caller error after a failure leaves health unchanged, and the counter/timing arithmetic saturates and keeps the high-water mark; and host input errors (empty batch, out-of-bounds rig span, rig spans that do not tile the batch in order, non-finite input bone, non-finite frame time, parent out of bounds or not preceding its child within its rig span) return before dispatch with an empty quarantine store and no probe invocation, while valid multi-rig parent chains still promote. The span-tiling cases additionally prove gaps (including a mid-batch gap), overlaps (including a later span starting inside its predecessor), reordered spans, empty rigs with bones, and a misplaced zero-count span are refused before dispatch with untouched pending/active slots, an empty quarantine store and no probe invocation, while zero-count spans at the tail, leading and middle positions advance nothing and still promote. The canary case promotes a distinct C module before re-publishing B, and the frame case re-publishes the failed active digest with the DLL absent; both prove the quarantine gate runs before the artifact read. The pose crash-sentinel cases prove the write/clear/load lifecycle for both stages: a dirty loading or canary marker quarantines its digest and leaves the marker pending until `persist_quarantine` commits the store and clears it, a persist failure keeps the marker, a new load cannot overwrite a pending recovery marker, a canary whose recovery commit is blocked still evaluates while the old marker is retained, the writer refuses a serialization the loader's 16 KiB cap would reject, a missing marker is a clean `Ok(None)`, malformed/unknown-field/invalid-digest/future-kind/oversized markers are errors that keep the file and both stores unchanged, the written marker parses back as a digest-only document, the canary marker is observed with the candidate identity/stage during the module call and gone afterwards, successful/rejected canaries clear it, a failed canary write is fail-open with no clear attempt, and a regular pose load failure clears the marker without a sentinel error. |
| `crates/benilla-hotpatch/src/lib.rs` | `tests::malformed_movement_manifest_latches_and_retries_when_rewritten`, `tests::verified_non_pe_movement_artifacts_are_quarantined_and_refused_before_reloading`, `tests::movement_digest_mismatch_latches_without_quarantining_the_declared_digest`, `tests::movement_wrong_immutable_filename_latches_without_quarantining_the_verified_bytes`, `tests::missing_movement_dll_is_retried_without_latching_or_quarantining`, `tests::movement_publish_lock_keeps_a_post_verification_failure_retryable`, `tests::movement_quarantine_store_dedupes_and_is_bounded`, `tests::terminal_movement_latch_pins_the_refused_artifact_and_still_collects_retired`, `tests::movement_quarantine_survives_disable_enable_and_re_arms_the_refusal`, `tests::terminal_movement_failure_retains_the_active_processor`, `tests::movement_manifest_failure_leaves_the_pose_publication_untouched`, `tests::movement_status_rejection_quarantines_the_selected_digest_and_refuses_a_rewrite`, `tests::movement_output_validation_rejection_quarantines_the_selected_digest`, `tests::movement_frame_input_errors_do_not_quarantine_the_selected_digest`, `tests::movement_frame_input_retains_a_staged_probe_for_the_next_valid_frame`, `tests::movement_frame_input_retains_an_active_probe_for_the_next_valid_frame`, `tests::staged_movement_failure_retains_a_healthy_active_with_a_different_digest`, `tests::staged_movement_failure_without_an_active_leaves_no_processor`, `tests::staged_movement_failure_with_the_same_active_digest_clears_both_slots`, `tests::active_movement_proposal_failure_quarantines_the_selected_digest`, `tests::movement_no_module_and_disabled_seam_do_not_quarantine`, `tests::movement_candidate_is_staged_but_not_active_until_committed`, `tests::movement_commit_requires_a_staged_proposal`, `tests::movement_controller_rejection_withdraws_the_commit_gate`, `tests::movement_changed_manifest_clears_an_armed_commit_gate`, `tests::movement_changed_manifest_arming_the_active_shortcut_clears_the_commit_gate`, `tests::movement_failed_frame_after_a_staged_proposal_blocks_the_commit`, `tests::movement_failed_staged_module_cannot_be_committed`, `tests::movement_active_proposal_does_not_arm_the_commit_gate`, `tests::movement_restaging_after_commit_requires_a_new_staged_proposal`, `tests::movement_stage_path_resets_an_armed_commit_gate`, `tests::movement_same_text_repair_retries_after_the_artifact_changes`, `tests::movement_same_length_repair_with_a_changed_mtime_retries`, `tests::movement_missing_artifact_retries_without_quarantine_and_skips_the_sweep`, `tests::movement_unchanged_artifact_stays_latched_after_terminal_failure`, `tests::movement_loading_sentinel_is_quarantined_and_persisted`, `tests::movement_canary_sentinel_is_quarantined_and_persisted`, `tests::movement_unknown_stage_is_rejected_fail_safe`, `tests::staged_movement_proposal_writes_the_canary_marker_during_process`, `tests::active_movement_proposal_does_not_write_a_marker`, `tests::rejected_staged_movement_proposal_clears_the_canary_marker`, `tests::movement_canary_write_failure_is_fail_open`, `tests::unload_resets_the_movement_canary_one_shot`, `tests::movement_loading_sentinel_writes_only_the_digest`, `tests::regular_movement_load_failure_leaves_no_sentinel`, `tests::movement_sentinel_write_failure_is_fail_open`, `tests::recovered_movement_digest_is_refused_before_the_artifact_read`, `tests::pose_marker_never_fills_movement_quarantine` | Focused movement-seam tests proving the same split without a native library: malformed movement TOML latches once, names the parse error and retries after a rewrite; a transient missing DLL stays unlatched and unquarantined across repeat calls; a wrong immutable filename latches without quarantining, and a correctly named republish of the same bytes reaches the post-verification load and only then quarantines; a digest mismatch latches without quarantining the unverified declared digest; a verified non-PE artifact (`movement-<sha1>.dll`, matching SHA-1) quarantines its digest, C does likewise, and republishing B after deleting its DLL is `Err(Quarantined)` rather than the missing-file path, latched and then quiet; a held `publish.lock` keeps a post-verification failure retryable until the lock clears; the store dedupes and evicts oldest at cap 32 independently of the pose store. The same-text cases prove the movement stamp retries a repaired artifact to the post-verification load failure that quarantines its digest, a same-length mtime-only change also retries and re-verifies, a missing artifact takes the transient path with no latch and skips the sweep before a recreated artifact reaches loading, a different still-bad length re-latches with a fresh stamp and stays quiet, an unchanged artifact keeps the stamp and stays a quiet `Ok(false)`, and a byte change after quarantine produces exactly one `Err(Quarantined)` with no stamp before the latch goes quiet. The quarantine-toggle case also pins a synthetic stamp to prove disabling keeps it and enabling clears it with the latch. The lifecycle cases prove a terminal latch pins its publication while the sweep still collects retired movement and pose artifacts, quarantine survives disable/enable and re-arms exactly one refusal, and a failed staging/load candidate leaves an active processor running. The proposal cases prove a module `Status`/output-validation failure quarantines the failed candidate's probe digest: a staged failure clears `movement_pending`, retains a healthy active with a different digest (its revision/digest remain selected, a failed commit leaves it active, and its subsequent valid proposal reports `Active`), clears both when the active has the same digest, and leaves no processor when there was no active; an active failure clears both slots. A changed manifest naming the quarantined digest is `Err(Quarantined)` before the absent-file path and stays latched/quiet. `FrameInput` (non-positive/NaN `dt_seconds`, each non-finite scalar field, invalid bounds, and caller-seeded facing/speeds outside the bounds, while the `-INFINITY` liquid and `+INFINITY` bounds sentinels are accepted) leaves the staged/active selection intact and never quarantines, and the retained processor serves the next valid proposal as `Staged` or `Active`; `NoModule` is likewise non-destructive and `SeamDisabled` evicts both without quarantining. Non-delegated input `move_flags` are accepted when the module clears them, and a valid proposal/commit leaves the store empty. The commit-gate cases prove staging alone and an `Active` proposal cannot promote; a `FrameInput` failure retains the candidate but stays unarmed until a later valid staged proposal re-arms it; a `Status`/validation failure drops the candidate and disarms; `withdraw_movement_proposal` disarms after a controller rejection while keeping the candidate for a later proposal; a changed manifest clears an armed gate and drops the pending candidate, including a changed manifest that names the active digest and takes the quiet active shortcut; a successful commit disarms, and a re-staged/new-selection candidate must propose again. The pose/movement independence test now asserts the movement text latches while `Host.observed` stays unchanged. The movement sentinel cases cover loading and staged-canary recovery/persist, the one-shot marker observed with the candidate identity during the first staged `process` call, no marker for active proposals or a second staged proposal, rejection and fail-open safety, the unload reset, and the recovered-digest gate before artifact read. |
| `crates/benilla-hotpatch/src/lib.rs` | `QuarantineFile`, `quarantine_entry_digest`, `accepted_quarantine_digests`, `write_quarantine_temp`, `Host::load_persistent_quarantine`, `Host::persist_quarantine`, `tests::persistent_quarantine_round_trips_pose_and_movement_digests`, `tests::persistent_quarantine_missing_file_is_an_empty_success`, `tests::persistent_quarantine_malformed_file_is_an_error_and_keeps_both_stores`, `tests::persistent_quarantine_skips_invalid_and_duplicate_entries`, `tests::persistent_quarantine_load_respects_the_cap_and_oldest_eviction`, `tests::persisted_quarantine_contains_only_digests_and_reloads_idempotently`, `tests::persistent_quarantine_merges_into_existing_stores_without_clearing`, `tests::persisted_quarantine_replaces_the_previous_file_and_a_stale_temp`, `tests::failed_quarantine_replacement_keeps_the_previous_store_and_cleans_the_temp` | Persists the two bounded digest lists to `<root>/quarantine.toml` and restores them at startup. The file is `#[serde(deny_unknown_fields)]` with both lists defaulting empty; loading parses the whole document before merging, so a read/parse/unknown-field error leaves both stores unchanged (a missing file is `Ok(0)`). Entries must be exactly 40 ASCII hex digits (normalized lowercase); empty, short, over-long, non-hex, nested/path-like and duplicate entries are skipped silently and never joined to a path or opened. A list longer than the cap keeps only its newest 32 entries, so a reload returns 0 instead of rotating the window. Valid entries merge through `quarantine_digest`/`quarantine_movement_digest`, preserving cap 32, dedupe and oldest-first eviction, and the call returns only newly inserted digests. Saving writes only digests to `quarantine.toml.tmp` (create, `write_all`, `sync_all`) and replaces the target in one `std::fs::rename` on both platforms; an empty store writes nothing, a failed step removes the temp and leaves the previous file intact, and the call runs from the 1 Hz poll, plugin startup after sentinel recovery, and the pre-load pending-recovery commit. After a successful store write it clears a pending recovered `active-sentinel.toml` marker; a marker-clear failure is reported through the sentinel slot and leaves the marker for a later write. |
| `crates/benilla-hotpatch-abi/src/lib.rs` | `MAGIC`, `ABI_VERSION`, `ENTRY`, `BoneLocal`, `BoneLocal::valid`, `RigSpan`, `PoseFrame`, `ProcessPose`, `Header`, `Header::current`, `Header::compatible`, `ModuleApi`, `EntryPoint` | Defines C-ABI pose seam (`project_b_pose_module_v1`, `0x5042_504F_5345_3031`, version `1`), struct layout size checks, and finite transform + nonzero quaternion validation (`norm_sq > 1e-12`). |
| `crates/benilla-hotpatch-abi/src/lib.rs` | `ModuleKind`, `ModuleKind::prefix`, `ModuleKind::entry_symbol`, `MOVEMENT_MAGIC`, `MOVEMENT_ENTRY`, `MovementFrame`, `MovementOutput::valid`, `MovementModuleV1`, `COMBAT_MAGIC`, `COMBAT_ENTRY`, `CombatFrame`, `CombatOutput::valid`, `CombatModuleV1`, `SPACE_MAGIC`, `SPACE_ENTRY`, `SpaceRequest`, `ProceduralSpawn::valid`, `SpaceModuleV1`, `STORY_MAGIC`, `STORY_ENTRY`, `StoryEvent`, `StoryStepDirective`, `StoryModuleV1` | Defines ABI descriptors and output validators for seams 2–5 (`Movement`, `Combat`, `Space`, `Story`). No host loader or Bevy runner dispatches these seams yet. |
| `crates/benilla-hotpatch-bevy/src/lib.rs` | `HotpatchPlugin`, `HotpatchProbeTarget`, `HotpatchEvent`, `Runtime`, `impl Drop for Runtime`, `poll`, `apply_pose`, `restore_pose`, `capture_pose`, `clear_active_sentinel_on_app_exit`, `report`, `report_quarantine`, `report_sentinel`, `transform` | Integrates `benilla_hotpatch::Host` into Bevy: 1 Hz `poll` in `Update`, `apply_pose` in `PostUpdate` (`PoseModulePass`), and `capture_pose` + `restore_pose` in `Last`. `HotpatchPlugin::build` loads `<root>/quarantine.toml` once after `Host::new` and before the first poll, logging a load error once with `warn!` and recording it in `runtime.last_quarantine_error` without failing startup (fail-open empty stores); the 1 Hz `poll` saves it after `host.poll()`/`collect_retired()` and reports a save error through the separate `report_quarantine` dedupe slot, which a successful save clears and which cannot re-emit or mask publication/retention events. It also calls `Host::load_active_sentinel` once after `Host::new` and before the first poll (missing marker is clean; an unclean pose marker quarantines and persists the recovered digest; a malformed/future marker warns once through `runtime.last_sentinel_error` without failing startup), and the 1 Hz `poll` drains `Host::take_sentinel_error` into the separate `report_sentinel` dedupe slot. Emits `HotpatchEvent::{CandidateStaged, PublicationRejected, CandidateRejected, Activated, FrameFailed, RetentionFailed}`. When an explicit `AppExit` message is present, the `Last` system `clear_active_sentinel_on_app_exit` calls the same host method and warns only on a real clear error: a retained pending-recovery or unowned/unvalidated marker is the expected quiet outcome there and is left for the next launch. `impl Drop for Runtime` remains the fallback for a bare `App` drop with no panic in flight: a stale marker this host wrote is cleared when its removal succeeds (a failed retry retains it), while a marker recording an uncommitted startup recovery or a marker this host never validated/accepted is retained with one warning, and a clear error is warned; Drop does not intentionally panic or block and does not fail shutdown, and it returns early while `std::thread::panicking()` so a panic unwind does not delete the marker for the panicking call. |
| `crates/benilla-world/src/rig_anim.rs` | `PoseModulePass`, `PosePost`, `RigPose`, `RigPose::sanitize_locals`, `AnimParked` | Schedules `PoseModulePass` after `PosePost` and before hierarchy/world composition; provides `RigPose.locals`, `parents`, `model`, and `pose_dirty`. |
| `crates/benilla-pose-module/src/lib.rs` | `project_b_pose_module_v1`, `API`, `process` | Reference `cdylib` implementation normalizing quaternions and supporting `--features passthrough` and `--features lift-demo` (`+0.5` root Y translation). |
| `crates/benilla-app/src/hotpatch.rs` & `crates/benilla-app/src/quest_trace.rs` | `benilla_app::hotpatch::plugin`, `capture_hotpatch_events`, `mark_player_rig`, `quest_trace::PatchEvent`, `quest_trace::Stage::Hotpatch`, `History::last_observed_patch_digest`, `drain_history` | Wires `<local_state::home()>/Hotpatch` into the client and bridges `HotpatchEvent` into the resident flight recorder (`Stage::Hotpatch`) and incident generator (`IncidentOutcome::ObservedFailure`). |
| `crates/benilla-hotpatch/src/bin/hotpatch-probe.rs` & `tools/publish-pose-module.ps1` | `main` (`inject-invalid-once`), `publish-pose-module.ps1` (`publish.lock`, `ready.next` -> `ready.toml`) | Standalone loader smoke binary and Windows PowerShell atomic publisher. |

#### 5.2 Decisions `H03` Can Rely On Immediately

The `H03` implementer ("Add patch health and last-good rollback" in `crates/benilla-hotpatch/src/lib.rs`
and `crates/benilla-hotpatch-bevy/src/lib.rs`) can rely on the following settled architectural decisions:

1. **Manifest envelope (`PatchRevisionV1` + legacy V0 compatibility)**:
   - Implement `PatchRevisionV1` in `crates/benilla-hotpatch/src/lib.rs` with the sections in §1.1, and accept legacy 3-field `Publication` manifests as a compatibility fallback mapped to `trust.origin = "local_unsigned_dev"`, `capability.kind = "pose_v1"`, `validation.level = "canary_frame_batch"`, and `rollback.max_consecutive_frame_failures = 1` so existing `H00`/`H01` fixtures continue to work while `publish-pose-module.ps1` is updated.
2. **Pre-load gate ordering**:
   - Enforce quarantine lookup, capability/seam check (`ModularGameplayRegistry::is_seam_enabled`), manifest ABI size checks, file byte length check, and SHA-1/SHA-256 verification **before** invoking `unsafe { Library::new(&path) }`.
3. **Transient vs terminal failure latching in `Host::poll`**:
   - Keep `Host.observed = None` (retryable on next 1 Hz poll) **only** when `ready.toml` or the target DLL fails with transient filesystem availability (`ErrorKind::NotFound`, `ErrorKind::PermissionDenied` / sharing violation, or incomplete byte length while `publish.lock` is present).
   - When a candidate fails with a deterministic content/contract error, latch `Host.observed` so the host does not repeatedly read or `LoadLibrary` the rejected artifact at 1 Hz. The bounded store (`VecDeque<String>`, cap `MAX_QUARANTINED_DIGESTS = 32`, persisted to `quarantine.toml`) covers a **post-verification** `Loaded::open` failure and post-load pose evaluation failures (canary, active frame and failing restore predecessor), where the digest was proved against the bytes, and refuses it before `Library::new` even when a changed manifest names it again. A pre-verification `DigestMismatch`/filename/size failure only latches the text and records an `ArtifactStamp` captured before the verification read (basename-only; in-memory); an identical text whose artifact length/modification time changed (or whose artifact is missing) clears the latch and retries once, and a missing artifact for a non-quarantined digest keeps returning the transient `Err` at 1 Hz with the latch unset until it reappears, so the retention sweep is skipped meanwhile; a quarantined digest is gated before the read and re-latches text-only so the sweep can run. Malformed TOML and non-basename names record no stamp, while a post-verification load failure records one despite quarantining its digest (a later byte change yields one gate refusal). H03 may make that stamp persistent and adds the typed reasons (`UnknownPublisher`, `UnsupportedCapability`, `AbiMismatch`, `CanaryEvaluationFailed`, `FrameEvaluationFailed`); the digest quarantine itself is already persisted to `quarantine.toml`.
4. **Automatic last-good rollback on active frame failure**:
   - **Current (2026-11-09):** `Host` owns an in-memory `PatchHealth` and `Host::health_snapshot()` returns a `PatchHealthSnapshot` with the active module's `revision`/`digest`, `consecutive_frame_failures`, `total_frame_failures`, `last_eval_micros`, `max_eval_micros` and the pose `quarantined_digests` length. Only the active `Loaded::evaluate` call is timed; the candidate canary and predecessor restore probes are not, so the activation frame is unmeasured and the timings are 0 until the first post-activation evaluation. Every active evaluation updates `last_eval_micros`/`max_eval_micros` (saturating `u128`→`u64`); an active `Ok` resets `consecutive_frame_failures` and keeps `total_frame_failures`; an active `Err` saturating-increments both. `total_frame_failures` is per-activation in this slice; per-revision cumulative accounting for `max_total_frame_failures` remains H03. A candidate canary rejection/failure does not change the active counters. Candidate promotion and a successful predecessor restore reset all four values for the newly active module. After a demotion with no valid predecessor the last counters/timings remain visible while `revision`/`digest` are `None` until the next activation (`take_active_failure`/`take_restore_outcomes` still carry the failed identity). If `self.active` returns `Err` during `Host::evaluate`, the host demotes and quarantines its verified digest in the bounded store (persisted to `quarantine.toml`), drops retained copies, then pops predecessors newest-first, skipping any digest already quarantined, and evaluates the first remaining valid predecessor **within the same `Host::evaluate` call** so the scene falls back to it on that very frame. A predecessor that also fails is quarantined and the loop continues; with none valid the host clears `self.active` and reports `RestoreOutcome::Unavailable`. `PatchHealthPolicy` (defaults `max_consecutive_frame_failures = 1`, `max_total_frame_failures = 3`; `0` disables a threshold) gates demotion. Below both thresholds the active and previous slots are retained, the digest is not quarantined, output is cleared, `active_failure` is recorded, `apply_pose` forces a retry refresh and the host returns `Ok(false)` for a baseline frame; reaching either threshold runs the quarantine/demote/newest-first restore path above. Counters are per-activation; an input-dependent failure that reaches a threshold refuses that digest while it remains in the bounded 32-entry store, which is persisted to `quarantine.toml` (cap eviction or deleting the store clears it; a restart does not). **H03:** persist health/last-good state, policy and artifact stamps, add `canary_frames_remaining`, per-revision cumulative totals and `max_eval_micros` enforcement.
5. **Structured rejection & rollback events**:
   - Extend `HotpatchEvent` (and `quest_trace::PatchEvent` when authorized) with structured reason codes (`PatchRejectReason`) and rollback identity (`RolledBack { failed_digest: [u8; 20], restored_digest: Option<[u8; 20]>, reason: PatchRejectReason }`) without storing free-form filesystem paths or raw OS error strings in exported telemetry.
6. **Next-launch crash sentinel (`active-sentinel.toml`)**:
   - Write `<root>/active-sentinel.toml` before `Library::new` and clear it when the load returns; write the same marker with `stage = "canary"` immediately before the staged candidate's canary `evaluate` and clear it when that call returns; the implemented slice now writes the pose loading marker after digest verification and before `Library::new`, clears it on open return, writes and clears the canary marker around `candidate.evaluate(...)`, writes and clears the movement loading marker around `MovementLoaded::open`, writes and clears the one-shot movement canary marker around the first staged `process` call, and recovers an unclean marker of either kind through `Host::load_active_sentinel` at `HotpatchPlugin` startup (quarantine, persist, then clear the marker). Native faults during steady-state active evaluation, predecessor restore and `Loaded` drop/`FreeLibrary`/DLL-detach, active movement per-frame `process` calls, later pre-commit staged movement proposals, movement active-frame/health evaluation, canary-rejection save ordering, typed reasons, persistent health/policy/artifact stamps, per-revision totals, the production adapter and the movement active proposal window remain H03. Clean shutdown is implemented on two edges that both call `Host::clear_active_sentinel_on_clean_shutdown`: the `Last` system `clear_active_sentinel_on_app_exit` on an explicit `AppExit` message, and `Runtime` drop as the fallback for a bare `App` drop with no panic in flight. Both retry removal of a stale marker (a failed retry retains it) but retain a marker recording an uncommitted startup recovery until `persist_quarantine` commits it. The clean-shutdown safety net and the `persist_quarantine` post-commit retry share `clear_owned_active_sentinel_if_matching`, so both delete a marker only while its parsed identity still matches the `{ revision, sha1, kind, stage }` this host wrote or accepted, and both cap the re-read at 16 KiB (an oversized replacement is retained unread); the call-scoped clears remove the marker the operation itself wrote by path and are a named residual. Drop returns early during panic unwind, so a panicking operation's marker survives; `process::exit`/hard kills bypass both edges, and cross-process/file-identity root ownership, a same-identity foreign replacement, the read/compare/unlink TOCTOU, the path-based call-scoped clears and a second clear failure remain H03 (an unvalidated, never-accepted or foreign-replaced marker is retained by the compare-and-clear paths). The first staged movement canary marker is implemented (one-shot, fail-open, recovered like the loading marker). Quarantine any revision left in an unclean sentinel on the next startup; the marker is retained until the recovered digest is committed to `quarantine.toml`.

#### 5.3 Policy Choices That Still Require the Owner

The following four policy questions remain open for owner confirmation (with safe defaults
proposed above so `H03` is not blocked):

1. **Local developer signing requirement**: Should local developer publications in `<home>/Hotpatch` continue to permit `trust.origin = "local_unsigned_dev"` indefinitely (reserving mandatory Ed25519 signature verification for `X04` WM-pushed revisions), or should `H03` require local Ed25519 signing as well? *(Proposed default for H03: allow `"local_unsigned_dev"` for local filesystem `<home>/Hotpatch` when `--features hotpatch` is enabled; require Ed25519 signatures for any non-local or WM-staged revision.)*
2. **Canary graduation window (`canary_frames`)**: Is a single validated non-empty frame batch (`canary_frames = 1`, current behavior) sufficient to graduate a candidate to `last_good`, or should the default be a multi-frame window (e.g., `60` consecutive clean frames) before the candidate itself can become a `last_good` rollback target? *(Proposed default for H03: `canary_frames = 1` for immediate compatibility with `H01` A->B->A acceptance, configurable per manifest up to `600` frames.)*
3. **Evaluation latency budget enforcement (`max_eval_micros`)**: When a module's synchronous `evaluate()` exceeds `rollback.max_eval_micros` (proposed `2,000 µs`), should `H03` only record a health warning / diagnostic incident, or automatically demote and roll back the module to `last_good` after $N$ budget breaches? *(Proposed default for H03: record latency high-water marks and emit a health warning in `H03`; keep automatic latency demotion opt-in until real in-game `H01` frame timings are measured on the owner's workstation.)*
4. **Crash-sentinel quarantine lifetime**: When a revision is quarantined on startup because the previous process crashed with `active-sentinel.toml` set, should that digest stay quarantined across subsequent client launches until a new digest is published to `ready.toml`, or expire after one skipped launch? *(Proposed default for H03: keep the crashed digest quarantined until `ready.toml` publishes a different digest or the developer deletes it from `quarantine.toml`; deleting `active-sentinel.toml` after recovery no longer clears the digest.)*

---

## Reload classes

| Change | Intended activation | Status |
|---|---|---|
| WGSL implementation with compatible host bindings | Live shader watch | Opt-in bootstrap feature added; rendered check pending |
| Native pose-local correction | Frame batch swap | Host, scheduling seam and restoration adapter implemented; focused roundtrip passes, in-game acceptance pending |
| Lua/XML/addon UI content | Existing `/reload` lifecycle | Existing lifecycle; developer content overlay still required |
| Texture/material/tuning | Dependency-aware content revision | Next stage; not delivered by the DLL loader |
| M2/SKIN/animation import | Reimport affected model family and rebind visuals | Next stage; current live caches do not implement this |
| Terrain/WMO/ADT/collision | Staged affected-cell rebuild at a safe boundary | Planned; must coordinate render and collision revisions |
| New host interface, Bevy/wgpu upgrade, core component/schema changes | Declared maintenance rebuild/restart | Outside ABI 1 |

An unchanged old executable cannot acquire this interface through `/reload`; the approved
bootstrap restart is required. It is not necessary for later ABI-compatible pose publications.

## Developer workflow (first slice)

Bootstrap: `cargo build -p benilla --features hotpatch`, then launch with the existing identity
workflow. Local publications live under `local_state::home()/Hotpatch`; unattended runs use
their own state folder unless BENILLA_HOME is explicitly set. Do not modify credentials.

`tools/publish-pose-module.ps1` builds and publishes the reference pose module. Its normal
implementation normalizes finite nonzero quaternion rotations; `-PassThrough` builds the
comparison implementation. These are real native calls through the same scene adapter.
The supplied module is a presentation example, not a proven repair for the interaction bug.
The implemented adapter dispatches poses in PoseModulePass after PosePost and restores the
post-edit input locals after palette/world composition. Astra owns this integration following
the owner's transfer of graphics responsibility. The focused roundtrip check confirms repeated
patches do not accumulate in unkeyed locals and replacing the module restores the original
pose. The client with this integration builds; actual in-game module activation and visible
same-process replacement still need acceptance.

The player build without `hotpatch` keeps the original behavior. The opt-in feature also
enables embedded shader watching. No process is restarted automatically and no Git hook is
installed. A successful publish is observed on the next poll; compile failure leaves the
publication unchanged. Module load/validation is currently synchronous and its latency must
be measured before promising hitch-free activation.

## Content stage after the native slice

Create one immutable content revision shared by AssetServer, synchronous Chain readers, UI
loaders and sibling `.skin` reads. A model and its dependencies must resolve from the same
revision, including async jobs started before a publication. Discard stale job results.

Use an owner-writable overlay inside local state, above the selected read-only MPQ profile.
Track dependency edges and invalidate decoded assets, derived geometry, atlases, materials,
palettes, portraits and affected DBC indexes. Keep old handles until replacements and required
GPU pipelines are ready. Preserve logical entities, targeting and server state; rebuild only
the dependent visual children. Model/skeleton changes must remap/recreate attachments and
reseed the current animation phase without replaying events.

UI revisions run through the existing save/teardown/load/rebind lifecycle, not arbitrary Lua
function replacement that leaves old closures and frame registrations alive. Collision and
terrain changes need a separate safe activation contract; do not move actors or change server
collision authority silently. WM delivery can later publish the same revision format.

## Acceptance

- Same running client PID and session before/after two distinct module implementations;
  visible actor behavior changes through the actual pose seam, without logout or reload.
- ABI/digest/output rejection leaves the previous presentation running.
- Source shader change appears live; failed shader publication does not destroy the working
  scene. Bevy's ordinary watcher alone does not establish transactional shader rollback.
- Later content stage: edit a texture and M2/SKIN pair, see existing instances update, retain
  target/position/quest/bag state, and reject an incomplete family revision.
- UI content change survives `/reload` with the socket/character intact.
- Repeat reloads without unbounded DLL, cache or disk growth. At most 20 essential checks per
  task; count compilation, loader smoke and rendered acceptance separately and honestly.

## References

- [Bevy 0.18.1 embedded watcher](https://docs.rs/bevy/0.18.1/bevy/asset/macro.embedded_asset.html).
- [Bevy 0.18.1 asset reload and handles](https://docs.rs/bevy/0.18.1/bevy/asset/index.html).
- [Rust foreign ABI and absence of Rust ABI stability](https://doc.rust-lang.org/reference/items/external-blocks.html#abi).
