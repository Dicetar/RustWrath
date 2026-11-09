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
  not written); the unchanged
  manifest is then a quiet `Ok(false)` until the text changes. A terminal **post-verification**
  `Loaded::open` failure (the bytes matched `publication.sha1` and the immutable filename) adds that
  verified digest to a bounded in-memory pose-digest quarantine
  (`Host.quarantined_digests`, `VecDeque<String>`, cap `MAX_QUARANTINED_DIGESTS = 32`): a later
  manifest text that names the same digest is refused before `Library::new` even after a
  `B -> C -> B` rewrite, while a genuinely new digest is still evaluated. A digest/filename/size
  failure before verification latches the text but does not quarantine the declared digest, which
  was not proved against the bytes. Transient I/O, a held `publish.lock` and malformed TOML do not
  quarantine. A changed manifest resets the latch
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
- **D00/D01–D07 update (2026-11-03, current behavior):** `benilla_app::hotpatch::capture_hotpatch_events`
  bridges `benilla_hotpatch_bevy::HotpatchEvent` into the resident recorder (`quest_trace::Stage::Hotpatch`
  wrapping `quest_trace::PatchEvent`), tracking `History.last_observed_patch_digest` on activation
  and recording `IncidentOutcome::ObservedFailure` (`IncidentConfidence::Exact`) on rejected
  publications, rejected candidates, frame failures, or retention errors.

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

The pose host validates the frame time, rig spans, local transforms and parent bounds before
invoking a module. A malformed input batch leaves the staged, active and retained revisions
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
   - If `self.observed.as_ref() == Some(&text)`, `Host::poll` returns `Ok(false)` (unchanged manifest already staged, active, or terminally rejected).
   - If an unpromoted candidate is still sitting in `self.pending` when new manifest text arrives (`self.pending.take().is_some()`), `self.observed` is cleared to `None` so reverting `ready.toml` to the prior manifest text can re-stage it.
   - `toml::from_str::<Publication>(&text)` parses `{ revision, library, sha1 }` (`#[serde(deny_unknown_fields)]`).
   - If `self.active` already has `m.digest == publication.sha1`, `self.observed = Some(text)` is latched and `Host::poll` returns `Ok(false)`.
2. **Pre-load verification & library open (`benilla_hotpatch::verify_publication` + `Loaded::open`)**:
   - Enforces `publication.library` is a single normal path component (`Component::Normal(_)`) inside `root` and `!publication.revision.is_empty()`.
   - Verifies file size `<= MAX_LIBRARY_BYTES` (`64 * 1024 * 1024`).
   - Reads the file bytes, computes lowercase hex `Sha1::digest(&bytes)`, and checks `digest == publication.sha1` and `publication.library == format!("pose-{digest}.dll")`.
   - Calls `unsafe { Library::new(&path) }`, resolves `ENTRY` (`b"project_b_pose_module_v1\0"`), invokes `entry()`, checks `!api.is_null()`, copies `Header`, checks `header.compatible()` against `Header::current()`, and extracts `(*api).process: Option<ProcessPose>`.
   - **Retry vs latch (H00 current behavior)**: Only after `Loaded::open` returns `Ok(pending)` does `Host::poll` assign `self.pending = Some(pending)` and `self.observed = Some(text)`. A transient filesystem failure anywhere in the cause chain (`NotFound`, `PermissionDenied`, `Interrupted`, `WouldBlock`, `TimedOut`, or Windows sharing/lock raw OS errors 32/33), or a held `publish.lock`, leaves `self.observed = None`, so the unchanged manifest is retried on the next 1 Hz poll. A malformed-TOML `ready.toml`, a deterministic pre-verification `verify_publication` failure (digest mismatch, wrong immutable filename, size/basename) or a post-verification `Loaded::open` failure (missing entry point, ABI, library load) instead latches `self.observed = Some(text)` and returns `Err` once; the unchanged manifest is thereafter a quiet `Ok(false)`, `self.active` is untouched, and no publication error is written to the shared candidate `self.rejection`. A terminal post-verification `Loaded::open` failure additionally quarantines the verified digest in the bounded in-memory `Host.quarantined_digests` store, so a changed manifest that still names the same bad digest is refused before any DLL read or `Library::new`; a pre-verification digest/filename/size failure only latches the text, and transient I/O, a held `publish.lock` and malformed TOML do not quarantine. A `ready.toml` that is not valid UTF-8 still takes the cheap read-error path and is retried without reading or opening the DLL. `HotpatchEvent::PublicationRejected` is emitted when `runtime.last_error` changes. H03 still adds typed `PatchRejectReason` values and the persistent quarantine store.
3. **First-frame canary evaluation, promotion, or rejection (`benilla_hotpatch_bevy::apply_pose` -> `benilla_hotpatch::Host::evaluate`)**:
   - Scheduled in `PostUpdate` in `PoseModulePass` (after `PosePost` and before hierarchy/world composition).
   - `apply_pose` exits if `!runtime.host.has_module()`. Otherwise it sanitizes dirty (or `runtime.refresh`-forced) rigs via `rig.sanitize_locals()`, packs host buffers `runtime.bones: Vec<BoneLocal>` and `runtime.spans: Vec<RigSpan>`, and calls `runtime.host.evaluate(&runtime.bones, &runtime.spans, time.elapsed_secs_f64(), &mut runtime.output)`.
   - Inside `Host::evaluate`:
     - If `bones.is_empty()`, returns `Ok(false)` without taking `self.pending` (candidate stays in `self.pending` until a non-empty rig batch exists).
     - If `let Some(candidate) = self.pending.take()`, calls `candidate.evaluate(bones, rigs, seconds, output)`.
     - `Loaded::evaluate` copies `bones` to `output`, builds `PoseFrame`, invokes `(self.process)(&frame, output.as_mut_ptr(), output.len() as u32)`, checks `status == 0`, and verifies `output.iter().zip(bones).all(|(after, before)| after.valid() && after.parent == before.parent)`.
     - **On candidate `Ok(())`**: promotes `candidate` into `self.active` (`self.active.replace(candidate)`), pushes displaced `old` onto `self.previous` (`VecDeque<Loaded>`), trims `self.previous` to `<= 3` (`pop_front()`), and returns `Ok(true)`. `apply_pose` detects the changed revision/digest, emits `HotpatchEvent::Activated { digest }`, writes `runtime.output` into `rig.locals`, and sets `runtime.restore_pending = true`.
     - **On candidate `Err(error)`**: drops `candidate`, stores `self.rejection = Some(format!("candidate rejected; active module retained: {error:#}"))`, and immediately falls through in the **same frame** to `match &self.active`, evaluating the retained active module so the scene does not suffer a one-frame unpatched snap. `apply_pose` drains `take_rejection()`, emits `HotpatchEvent::CandidateRejected`, and applies `self.active`'s output.
     - **On active module `Err(error)` (current behavior)**: `Host::evaluate` demotes the failed active module, walks `self.previous` newest-first, skips a retained copy of the same digest, evaluates the first predecessor that produces a valid batch in the same frame, and returns `Ok(true)`; `apply_pose` emits `FrameFailed` plus `RestoredPrevious`. With no valid predecessor it clears `self.active`, returns `Ok(false)`, emits `FrameFailed` plus `RestoreUnavailable`, and re-dirties rigs so the baseline pose applies. The bounded in-memory pose-digest quarantine already refuses a digest rejected by a terminal `Loaded::open` failure; H03 still adds frame-failure quarantine and persisted health/last-good state.
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
| `rollback.max_consecutive_frame_failures` | `u32` | Yes | `1..=8` (recommended `1`). When the active/canary module returns `Err` this many consecutive frames, H03 demotes and quarantines it and restores `last_good` (`previous.back()`). |
| `rollback.max_total_frame_failures` | `u32` | Yes | `1..=32` (recommended `3`). Cumulative frame failures tolerated for a revision before automatic demotion to `last_good`. |
| `rollback.max_eval_micros` | `u64` | Yes | Per-frame synchronous evaluation budget in microseconds (recommended `2000` µs). Breaches are recorded in patch health; if `demote_on_budget_breach` is active, triggers rollback. |
| `rollback.retain_previous` | `u8` | Yes | `1..=3` (default `3`, matching `Host.previous` capacity). |
| `rollback.quarantine_on_reject` | `bool` | Yes | Must be `true`: a revision that fails validation or is rolled back has its `artifact.sha1` added to the host's in-memory quarantine set so unchanged `ready.toml` is not re-staged. |

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
| **2. Digest mismatch** (`Sha1::digest(&bytes) != artifact.sha1`, `Sha256` mismatch, or `library != "<prefix><sha1>.dll"`) | `Host::poll` -> `verify_publication` **before** `Loaded::open`/`Library::new` | **`RejectedBeforeLoad`; text latch only** (unless `publish.lock` is currently held, in which case treat as Case 0 transient). No digest is quarantined: a mismatch does not prove the computed bytes are bad, and a filename/size failure is a manifest error that a corrected same-bytes republish may fix. | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::DigestMismatch`) -> `quest_trace::IncidentOutcome::ObservedFailure` | **No for identical manifest text.** The text latch stops the 1 Hz re-read; a changed text retries once. H03 may add `(manifest_text, file_len, mtime)` latching rather than a digest quarantine. |
| **3. Unknown publisher** (unrecognized `trust.origin`, unknown `trust.key_id`, missing/invalid `signature_ed25519` when required, or remote origin in local-only mode) | `Host::poll` trust gate **before** `Library::new` | **`RejectedBeforeLoad` & `Quarantined`** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::UnknownPublisher`) -> `IncidentOutcome::ObservedFailure` | **No.** `Host.observed` latches the rejected manifest text and quarantines `artifact.sha1` until a new signed manifest is published. |
| **4. Unsupported capability** (unknown `capability.kind`, `mutates_gameplay_state == true` in Baseline mode, seam disabled in `ModularGameplayRegistry`, or `required_mode` not active) | `Host::poll` capability gate **before** `Library::new` | **`RejectedBeforeLoad`** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::UnsupportedCapability`) -> `IncidentOutcome::ObservedFailure` | **No for unchanged mode/seam state.** Latched in `Host.observed`; becomes retryable only if `ready.toml` changes or the host explicitly transitions into the required capability mode/seam. |
| **5. ABI mismatch** (manifest `host_abi` mismatch before load, or post-load missing `EntryPoint`, null `ModuleApi`, `!Header::compatible()`, or `process.is_none()`) | Pre-load in `Host::poll` (`host_abi` table) **or** post-load in `Loaded::open` before staging `pending` | **`RejectedBeforeStage` & `Quarantined`** (if `Library::new` ran, handle is dropped immediately without staging) | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::AbiMismatch`) -> `IncidentOutcome::ObservedFailure` | **No.** Binary is incompatible with the running host executable; `Host.observed` latches and `artifact.sha1` is quarantined for this run. |
| **6. Missing dependency** (`min_host_commit` mismatch, `asset_profile` mismatch, or any entry in `requires_revisions` not currently active) | `Host::poll` dependency gate **before** `Library::new` | **`RejectedBeforeLoad` (`DependencyPending`)** | Previous `active` module (`A`) continues uninterrupted | `HotpatchEvent::PublicationRejected` (`PatchRejectReason::MissingDependency`) -> `IncidentOutcome::ObservedFailure` | **Conditional (State-driven, not blind 1 Hz spin).** Not retried on ordinary timer ticks while active revisions/profile are unchanged; re-evaluated when a prerequisite revision or profile activates or `ready.toml` updates. |
| **7a. Activation failure (pre-promotion canary batch failure)** (`Loaded::open` succeeded, but first `candidate.evaluate` returns non-zero status, non-finite transform, zero quaternion norm, or changed `parent`) | `PostUpdate` (`PoseModulePass`): `apply_pose` -> `Host::evaluate` on `self.pending.take()` | **`RejectedBeforePromotion` & `Quarantined`** (`candidate` dropped; `previous` keeps library pinned if required by drain policy, or dropped after synchronous return) | Previous `active` module (`A`) evaluates **in the same frame**; zero unpatched frame glitch | `HotpatchEvent::CandidateRejected` (`PatchRejectReason::CanaryEvaluationFailed`) -> `IncidentOutcome::ObservedFailure` | **No.** `Host.observed` is already latched and `artifact.sha1` is quarantined; `B` will not be re-staged unless a new digest is published. |
| **7b. Activation / post-promotion frame failure** (`B` promoted to `active`, then returns `Err` on a frame at or after promotion) | `PostUpdate` (`PoseModulePass`): `apply_pose` -> `Host::evaluate` on `self.active` | **Current:** `B` is demoted immediately; the newest retained predecessor that evaluates validly is promoted, or the slot is cleared when none is (`FrameFailed` + `RestoredPrevious`/`RestoreUnavailable`).<br>**Proposed (H03):** add digest `Quarantined` and persisted health counters after `max_consecutive_frame_failures` (`1`). | **Current:** the restored predecessor evaluates **in the same frame**; no valid predecessor falls back to the baseline pose. **Proposed (H03):** persist the rollback/health record and quarantine `B`'s digest. | `HotpatchEvent::FrameFailed` + `RestoredPrevious`/`RestoreUnavailable`; proposed structured `HotpatchEvent::RolledBack` reason codes. | **No.** H03 will extend the bounded in-memory `Host.quarantined_digests` (which today covers terminal `Loaded::open` failures) to frame failures so `Host::poll` refuses to re-stage `B` even if `ready.toml` still names `B`. |

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
   - **Current host recovery (H00)**: Transient filesystem failures (`NotFound`, `PermissionDenied`, interrupted/blocked/timeout reads, and Windows sharing/lock raw OS errors 32/33) keep `self.observed = None` so the next 1 Hz poll retries. Deterministic content/contract failures (digest/filename/ABI/entry/processor mismatch) latch `self.observed = Some(text_b)` and return `Err` once, so `poll()` does not repeatedly read or `LoadLibrary` the broken artifact every second. A terminal post-verification `Loaded::open` failure also quarantines the bytes-verified digest in the bounded in-memory `Host.quarantined_digests`, so a `B -> C -> B` rewrite is refused before `Library::new`; a pre-verification mismatch/filename failure only latches the text. **H03 additions**: typed `PatchRejectReason` values, the persistent quarantine store, frame-failure quarantine, trust/capability/dependency gates, and `publish.lock`-aware incomplete-copy detection; the pose transient/terminal latch itself is current behavior (2026-11-09).
3. **Case A2 — `B` passes `Loaded::open` (`CandidateStaged`) and fails batch validation on its first frame (`Host::evaluate`)**:
   - `Host::poll` succeeds: `Host.pending = Some(B)`, `Host.observed = Some("<manifest-b>")`, `runtime.refresh = true`, and `HotpatchEvent::CandidateStaged` is emitted. `Host.active` is still `Some(A)`.
   - In `PostUpdate` (`PoseModulePass`), `apply_pose` forces all non-`AnimParked` rigs into `runtime.bones` (because `runtime.refresh == true`) and calls `runtime.host.evaluate(&runtime.bones, &runtime.spans, seconds, &mut runtime.output)`.
   - `Host::evaluate` takes `B` via `self.pending.take()` and runs `B.evaluate(bones, rigs, seconds, output)`.
   - Suppose `B` writes a `NaN` translation, a zero-norm quaternion, mutates `output[i].parent != bones[i].parent`, or returns non-zero status `3`.
   - **Current host recovery (`crates/benilla-hotpatch/src/lib.rs:251-277`)**:
     - `B.evaluate` returns `Err(...)`.
     - `B` is **not** placed into `self.active` and is dropped at the end of the `if let Some(candidate) = self.pending.take()` block.
     - `self.rejection` is set to `Some("candidate rejected; active module retained: ...")`.
     - Execution immediately falls through in the **same `Host::evaluate` call** to `match &self.active`, which runs `A.evaluate(bones, rigs, seconds, output)` (Note: `A.evaluate` starts with `output.clear(); output.extend_from_slice(bones);`, completely wiping any partial/invalid writes `B` made to `output`).
     - `Host::evaluate` returns `Ok(true)`.
     - Back in `apply_pose`, `runtime.host.take_rejection()` returns `Some(error)`, emitting `HotpatchEvent::CandidateRejected` (`quest_trace::PatchEvent::CandidateRejected` -> `IncidentOutcome::ObservedFailure`). Because `result` is `Ok(true)` and `host.revision()` is still `A`, `apply_pose` does **not** emit `HotpatchEvent::Activated`; it writes `A`'s valid `runtime.output` into `rig.locals`, and `restore_pose` restores `rig.locals` in `Last`.
   - **Current (2026-11-09)**: the bounded in-memory `Host.quarantined_digests` store exists and refuses a bytes-verified digest rejected by a terminal post-verification `Loaded::open` failure; a pre-verification mismatch/filename failure only latches the text. **H03 addition**: extend it to canary-batch failures so the bad digest is quarantined after `Host::evaluate` rejects `B`, and a `B -> C -> B` rewrite is then refused before `Library::new`.

#### Sequence B: Candidate `B` activates and then fails on its first frame (or a subsequent frame after promotion)

1. **How this sequence arises**:
   - **Scenario B1 (Empty-rig window followed by first active rig frame, or frame-2 input trigger)**:
     - Currently, when `runtime.bones.is_empty()` (e.g., while on a loading screen or scene with zero dirty rigs), `Host::evaluate` returns `Ok(false)` at line 248 before inspecting `self.pending`. Once at least one rig is evaluated on Frame $T_0$, `B` succeeds on that rig batch and is promoted to `Host.active = Some(B)`, moving `A` into `Host.previous.push_back(A)` and emitting `HotpatchEvent::Activated { digest: Some(sha1_b) }`.
     - On the immediately following frame $T_1$ (or when `validation.level` promotes `B` and `B` encounters a new rig/animation input or time delta `time_seconds`), `B.evaluate` returns `Err(...)` (non-zero status, `NaN`/`Inf` output, or topology violation).
2. **What the current host does (`main`)**:
   - In `Host::evaluate` (`crates/benilla-hotpatch/src/lib.rs:271-277`), `self.pending` is `None`, so it executes `self.active` (`B`): `module.evaluate(bones, rigs, seconds, output)?`.
   - `B.evaluate` returns `Err(error)`, which bubbles out of `Host::evaluate` as `Err(error)`.
   - In `benilla_hotpatch_bevy::apply_pose` (`crates/benilla-hotpatch-bevy/src/lib.rs:241-248`):
     - The `Err(error)` branch skips writing `runtime.output` to `rig.locals` (so for Frame $T_1$, rigs render with their unpatched `PosePost` local transforms rather than `B`'s corrupted output).
     - `runtime.refresh` is set to `false`; `HotpatchEvent::FrameFailed` is emitted once per distinct error string; `restore_pose` is a no-op because `runtime.restore_pending` remains `false`.
   - **Critical gap in current host**: `B` **remains** in `Host.active`! Even though `A` is sitting loaded and verified in `Host.previous.back()`, the current host never demotes `B` or restores `A`. On frames $T_2, T_3, \dots$, the host keeps calling broken `B`, either failing every frame (leaving `A`'s patches missing) or flickering between `B` and unpatched poses.
3. **What H03 must implement**:
   - **In-frame demotion and `last_good` restoration**:
     - When `self.active` (`B`) returns `Err(error)` in `Host::evaluate`, increment `B`'s `consecutive_frame_failures` and `total_frame_failures`.
     - When `consecutive_frame_failures >= rollback.max_consecutive_frame_failures` (default `1`) or `total_frame_failures >= rollback.max_total_frame_failures`:
       1. Remove `B` from `self.active` and insert `B.digest` into `self.quarantined_digests`.
       2. Retain `B` in a bounded retired/quarantined list for the remainder of the run (or until safe unload) so any in-process code pages stay mapped if required, while never invoking `B.process` again.
       3. Pop the most recent known-good module `A` from `self.previous.pop_back()` and install `self.active = Some(A)`.
       4. **In that same `Host::evaluate` call**, invoke `A.evaluate(bones, rigs, seconds, output)` (which clears `output`, copies `bones`, and computes `A`'s valid transforms).
       5. Record a structured rollback transition so `apply_pose` emits `HotpatchEvent::RolledBack { failed_revision: B.revision, failed_digest: B.digest, restored_revision: Some(A.revision), restored_digest: Some(A.digest), reason: PatchRejectReason::FrameEvaluationFailed }` and updates `runtime.reported_revision` / `runtime.reported_digest` back to `A` (and `quest_trace::History.last_observed_patch_digest` back to `A`).
       6. If `self.previous` is empty (no prior `last_good` module existed in this session), set `self.active = None`, leave `rig.locals` at their unpatched `PosePost` values, and emit `HotpatchEvent::RolledBack` with `restored_revision: None, restored_digest: None`.
   - **Next-launch recovery for fatal native faults (`active-sentinel.toml`)**:
     - If `B` faults the process (`STATUS_ACCESS_VIOLATION`, illegal instruction, stack overflow, or `abort`) during `Library::new`, `entry()`, or `process()`, in-process Rust code cannot recover that process.
     - H03 must write `<root>/active-sentinel.toml` containing `{ revision, sha1, stage = "loading" | "canary" }` and flush it before calling `Library::new`. Once `B` completes its canary validation window (or on clean client shutdown), the sentinel is marked `"clean"` or removed.
     - On the next client startup, before `Host::poll` reads `ready.toml`, `Host::new` inspects `active-sentinel.toml`. If an unclean sentinel is present, H03 adds its `sha1` to `quarantined_digests` (and persists it to `<root>/quarantine.toml`), emits a startup diagnostic incident (`PatchRejectReason::PriorLaunchCrashQuarantined`), and refuses to load `B` even if `ready.toml` still points to `B`.

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

1. **Crash-sentinel & quarantine check**: Reject any candidate whose `artifact.sha1` appears in `quarantined_digests` or an unclean `active-sentinel.toml` from a prior crashed run.
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
| `crates/benilla-hotpatch/src/lib.rs` | `MAX_LIBRARY_BYTES`, `MAX_QUARANTINED_DIGESTS`, `Publication`, `verify_publication`, `Loaded`, `Loaded::open`, `Loaded::evaluate`, `Host`, `Host::new`, `Host::poll`, `Host::quarantine_digest`, `Host::is_quarantined`, `Host::has_module`, `Host::revision`, `Host::digest`, `Host::take_rejection`, `Host::take_active_failure`, `Host::take_restore_outcomes`, `Host::collect_retired`, `Host::evaluate` | Reads `<root>/ready.toml`, verifies basename, `<= 64 MiB` cap, and SHA-1 (`pose-<sha1>.dll`), opens DLL via `libloading::Library::new`, checks `ENTRY` and `Header::compatible()`, stages `Host.pending`, promotes on first valid `Host::evaluate` batch, retains `Host.active` on candidate rejection, demotes a failing active module and restores the newest valid predecessor in the same frame (else the baseline pose), pins up to 3 predecessors in `Host.previous`, and garbage-collects unpinned pose/movement artifacts under `publish.lock`. A terminal post-verification `Loaded::open` failure adds the bytes-verified digest to the bounded in-memory `Host.quarantined_digests` (`VecDeque<String>`, cap 32) so a changed manifest naming the same digest is refused before `Library::new`; pre-load verification failures (basename/revision/size/digest/filename) only latch the text, and persistent/typed quarantine remains H03. |
| `crates/benilla-hotpatch/src/lib.rs` | `ModularGameplayRegistry`, `ModularGameplayRegistry::new`, `ModularGameplayRegistry::set_seam_enabled`, `ModularGameplayRegistry::is_seam_enabled`, `Host::set_seam_enabled`, `Host::is_seam_enabled`, `MovementRefusal::SeamDisabled` | Tracks runtime enablement booleans for `ModuleKind::{Pose, Movement, Combat, Space, Story}` (`Pose` is always `true`; gameplay seams default to `false`). `Host::stage_movement_processor` consults the movement seam before opening the artifact, latches a disabled publication so retention keeps it, and clears that latch on enable so the next stage call can publish the same manifest. Disabling evicts the active/staged processor but keeps the manifest latch until enable, so the GC sweep still pins the publication. The production Bevy movement adapter and the H02 manifest capability gate remain future work. The pose transient/terminal latch (2026-11-09) was not extended here: `stage_movement_processor` still leaves `movement_observed = None` on a TOML or open failure, so deterministic movement artifacts remain retried until H03. |
| `crates/benilla-hotpatch/src/lib.rs` | `transient_io`, `publish_in_progress`, `tests::poll_retries_unchanged_manifest_after_transient_load_failure`, `tests::malformed_manifest_latches_after_one_error_and_retries_when_rewritten`, `tests::publish_lock_keeps_a_digest_mismatch_retryable_until_the_publisher_finishes`, `tests::transient_io_classifies_only_filesystem_availability`, `tests::quarantined_digest_is_refused_before_loaded_open_across_manifest_rewrites`, `tests::digest_mismatch_latches_without_quarantining_the_declared_digest`, `tests::wrong_immutable_filename_latches_without_quarantining_the_verified_digest`, `tests::publish_lock_keeps_a_post_verification_failure_retryable_until_the_publisher_finishes`, `tests::missing_dll_transient_failure_does_not_quarantine`, `tests::malformed_manifest_does_not_quarantine_a_digest`, `tests::quarantine_store_dedupes_and_is_bounded` | Focused unit tests (H00) proving the split: a transient `NotFound` or a held `publish.lock` leaves `Host.observed == None` and retries the unchanged manifest; a malformed-TOML manifest or a deterministic `verify_publication`/`Loaded::open` error latches `Host.observed` and reports `Err` once, then is a quiet `Ok(false)` until the text changes; `transient_io` classifies only `NotFound`, `PermissionDenied`, `Interrupted`, `WouldBlock`, `TimedOut`, plus Windows sharing/lock raw OS errors 32/33. A non-UTF-8 manifest retries the cheap read without opening the DLL. The quarantine cases prove a terminal post-verification `Loaded::open` failure quarantines the verified digest so a `B -> C -> B` rewrite is refused before `Library::new` (even with the DLL deleted), while a pre-verification digest mismatch, a wrong immutable filename, a transient missing DLL and malformed TOML do not quarantine, and that the store dedupes and evicts oldest at cap 32. |
| `crates/benilla-hotpatch-abi/src/lib.rs` | `MAGIC`, `ABI_VERSION`, `ENTRY`, `BoneLocal`, `BoneLocal::valid`, `RigSpan`, `PoseFrame`, `ProcessPose`, `Header`, `Header::current`, `Header::compatible`, `ModuleApi`, `EntryPoint` | Defines C-ABI pose seam (`project_b_pose_module_v1`, `0x5042_504F_5345_3031`, version `1`), struct layout size checks, and finite transform + nonzero quaternion validation (`norm_sq > 1e-12`). |
| `crates/benilla-hotpatch-abi/src/lib.rs` | `ModuleKind`, `ModuleKind::prefix`, `ModuleKind::entry_symbol`, `MOVEMENT_MAGIC`, `MOVEMENT_ENTRY`, `MovementFrame`, `MovementOutput::valid`, `MovementModuleV1`, `COMBAT_MAGIC`, `COMBAT_ENTRY`, `CombatFrame`, `CombatOutput::valid`, `CombatModuleV1`, `SPACE_MAGIC`, `SPACE_ENTRY`, `SpaceRequest`, `ProceduralSpawn::valid`, `SpaceModuleV1`, `STORY_MAGIC`, `STORY_ENTRY`, `StoryEvent`, `StoryStepDirective`, `StoryModuleV1` | Defines ABI descriptors and output validators for seams 2–5 (`Movement`, `Combat`, `Space`, `Story`). No host loader or Bevy runner dispatches these seams yet. |
| `crates/benilla-hotpatch-bevy/src/lib.rs` | `HotpatchPlugin`, `HotpatchProbeTarget`, `HotpatchEvent`, `Runtime`, `poll`, `apply_pose`, `restore_pose`, `capture_pose`, `report`, `transform` | Integrates `benilla_hotpatch::Host` into Bevy: 1 Hz `poll` in `Update`, `apply_pose` in `PostUpdate` (`PoseModulePass`), and `capture_pose` + `restore_pose` in `Last`. Emits `HotpatchEvent::{CandidateStaged, PublicationRejected, CandidateRejected, Activated, FrameFailed, RetentionFailed}`. |
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
   - When a candidate fails with a deterministic content/contract error, latch `Host.observed` so the host does not repeatedly read or `LoadLibrary` the rejected artifact at 1 Hz. The bounded in-memory store (`VecDeque<String>`, cap `MAX_QUARANTINED_DIGESTS = 32`) already covers a **post-verification** `Loaded::open` failure, where the digest was proved against the bytes, and refuses it before `Library::new` even when a changed manifest names it again. A pre-verification `DigestMismatch`/filename/size failure only latches the text; H03 may add `(manifest_text, file_len, mtime)` metadata latching plus the typed reasons (`UnknownPublisher`, `UnsupportedCapability`, `AbiMismatch`, `CanaryEvaluationFailed`, `FrameEvaluationFailed`) and persistence.
4. **Automatic last-good rollback on active frame failure**:
   - Track per-active-module health (`consecutive_frame_failures`, `total_frame_failures`, `last_eval_micros`, `max_eval_micros`, `canary_frames_remaining`) and expose a read-only `PatchHealthSnapshot` from `Host`.
   - If `self.active` returns `Err` during `Host::evaluate` and reaches `max_consecutive_frame_failures` (`1`) or `max_total_frame_failures` (`3`), demote `self.active`, quarantine its digest, pop `self.previous.pop_back()` (`last_good`) into `self.active`, and evaluate that restored `last_good` module **within the same `Host::evaluate` call** so the scene falls back to `A` on that very frame (or falls back cleanly to `None` if `self.previous` is empty).
5. **Structured rejection & rollback events**:
   - Extend `HotpatchEvent` (and `quest_trace::PatchEvent` when authorized) with structured reason codes (`PatchRejectReason`) and rollback identity (`RolledBack { failed_digest: [u8; 20], restored_digest: Option<[u8; 20]>, reason: PatchRejectReason }`) without storing free-form filesystem paths or raw OS error strings in exported telemetry.
6. **Next-launch crash sentinel (`active-sentinel.toml`)**:
   - Write `<root>/active-sentinel.toml` before `Library::new` and clear it when the candidate graduates its canary check or the host shuts down cleanly; quarantine any revision left in an unclean sentinel on the next `Host::new` initialization.

#### 5.3 Policy Choices That Still Require the Owner

The following four policy questions remain open for owner confirmation (with safe defaults
proposed above so `H03` is not blocked):

1. **Local developer signing requirement**: Should local developer publications in `<home>/Hotpatch` continue to permit `trust.origin = "local_unsigned_dev"` indefinitely (reserving mandatory Ed25519 signature verification for `X04` WM-pushed revisions), or should `H03` require local Ed25519 signing as well? *(Proposed default for H03: allow `"local_unsigned_dev"` for local filesystem `<home>/Hotpatch` when `--features hotpatch` is enabled; require Ed25519 signatures for any non-local or WM-staged revision.)*
2. **Canary graduation window (`canary_frames`)**: Is a single validated non-empty frame batch (`canary_frames = 1`, current behavior) sufficient to graduate a candidate to `last_good`, or should the default be a multi-frame window (e.g., `60` consecutive clean frames) before the candidate itself can become a `last_good` rollback target? *(Proposed default for H03: `canary_frames = 1` for immediate compatibility with `H01` A->B->A acceptance, configurable per manifest up to `600` frames.)*
3. **Evaluation latency budget enforcement (`max_eval_micros`)**: When a module's synchronous `evaluate()` exceeds `rollback.max_eval_micros` (proposed `2,000 µs`), should `H03` only record a health warning / diagnostic incident, or automatically demote and roll back the module to `last_good` after $N$ budget breaches? *(Proposed default for H03: record latency high-water marks and emit a health warning in `H03`; keep automatic latency demotion opt-in until real in-game `H01` frame timings are measured on the owner's workstation.)*
4. **Crash-sentinel quarantine lifetime**: When a revision is quarantined on startup because the previous process crashed with `active-sentinel.toml` set, should that digest stay quarantined across subsequent client launches until a new digest is published to `ready.toml`, or expire after one skipped launch? *(Proposed default for H03: keep the crashed digest quarantined until `ready.toml` publishes a different digest or the developer deletes `active-sentinel.toml`.)*

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
