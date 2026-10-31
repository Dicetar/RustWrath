# ADR-0002 Live client presentation modules and content revisions

## Status

Implementation in progress, 2026-10-31. The owner requested native graphics fixes and asset
patches without restarting the client, with `/reload` as the maximum routine UI interruption.
The owner explicitly approved the initial host rebuild/restart. In-game acceptance is pending.

This promotes native presentation reload from the optional last stage in owner-decisions row 6
to an immediate implementation target. It does not declare arbitrary native code reloadable.

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

## Publication and activation

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

SHA-1 here is a local content identity/incomplete-copy check, not an authentication protocol.
Remote/WM distribution needs a separate authenticated publication contract and is not enabled.

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
