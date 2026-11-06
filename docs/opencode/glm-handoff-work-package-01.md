# GLM handoff — Project B work package 01

Updated: 2026-10-25. From: OpenCode. To: GLM.

Status: **OpenCode bridge v2.1 and WotLK route asset source are implemented in isolated trees.
Crate checks and real-asset parser probes pass. GLM integration, app call, Bevy loader runtime,
visual route acceptance and commit remain open.** No files were committed.

## Ownership / worktrees

- Main bridge checkout: `D:/Projects/project-b-opencode`, branch
  `opencode/project-b-profile-reload`; uncommitted.
- Asset worktree: `D:/Projects/project-b-opencode-assets`, detached from GLM
  `D:/Projects/project-b-client` at `1f8d33b8`; uncommitted.
- GLM active checkout `D:/Projects/project-b-client` was not edited. Last inspected clean at
  `1f8d33b8`.
- Full stock-client replacement remains the release gate. This package does not claim it.

## Delivered by OpenCode

### Bridge v2.1

Source of truth: `docs/opencode/bridge-health-contract-v2.1.md` (exact hashes, schema, delta).

- Schema `wm.native.observation.v2.1`: adds `world_generation` and
  `position_observed_at_unix_ms`.
- `SourceHealth` (`in_world` / `transferring` / `disconnected`) and receiver health frames.
- ACK plus `health_ack`; handoff is not receiver acceptance and not WM availability.
- Idle health every 5 s, health max age 15 s, receiver idle timeout 30 s, retry max age 60 s.
- Quiet-state clocks use `time.monotonic_ns()` after one initial wall-clock skew check.
- Replicated values remain valid while source scope/session health is valid; timestamps are never
  refreshed; aggregate and position ages stay distinct; transfer/disconnect invalidates old-scope
  eligibility.
- Publisher/receiver health counters and bounded retry/resync.

### WM production integration

- `D:/WOW/wm-project/src/wm/autoplay/service.py`: one runtime, fail-soft startup issue, health in
  status, idempotent shutdown.
- `D:/WOW/wm-project/src/wm/context/pack.py`: optional native loader with DB fallback.
- `D:/WOW/wm-project/src/wm/autoplay/world_context.py`: loader pass-through.
- `wm_native` is vendored at `D:/WOW/wm-project/src/wm_native` and is byte-identical to
  `integrations/wm-native/src/wm_native`.

### C1 profile asset source

- `ProfileAssetReader` / `ProfileLooseAssetReader` register `mpq://` and `mpq-loose://` over a
  manifest-defined `benilla_profile::Profile`.
- `Profile::reload` is candidate-first; a failed reopen leaves the previous profile live.
- Hash and exact app call site are in `docs/opencode/route-asset-handoff.md`.

### C2 WotLK v264 actor

- v264 MD20 header admitted: no PlayableAnimationLookup/M2View table; 88-byte bones; 40-byte
  attachments; 36-byte events.
- v264 0x40-byte sequences and per-sequence key arrays.
- Compressed 16-bit rotations use the biased Wrath conversion
  `(v < 0 ? v+32768 : v-32767)/32767`; identity `(32767,32767,32767,-1)`.
- External `<stem>00.skin` loaded through the registered profile source and fed to the existing
  submesh path. **LOD 0 only.**
- v264 sequence bounds for Stand/Swim corrected.
- Route probe `Creature\Boar\Boar.m2`: 2 submeshes, 61 bones, 17 animations, non-empty bounds and
  collision.

### C3 WotLK MH2O route liquid

- MH2O instances become existing `LiquidMesh` values: 1/8-chunk lattice, exists bitmaps, LVF 0-3
  heights/depths, generated UVs, part-relative positions.
- `LiquidType.dbc` mapping includes the route type 5 (Still) and the shipped low-id
  water/ocean/magma/slime families; unknown ids stay unwet.
- Terrain loader attaches each mesh to the matching `index_x/index_y` MCNK.
- Route probe `Azeroth_30..33_48.adt`: 99 MH2O instances, all `LiquidType=5`, `LVF=0`;
  `Azeroth_32_48.adt` yields 42 meshes / 1199 wet cells / 2100 vertices.

## Verification performed

```text
cargo check -p benilla-assets -p benilla-formats -p benilla-m2          PASS
cargo test -p benilla-formats -p benilla-m2 -p benilla-adt --lib       543+20+9 PASS
Boar.m2 + Boar00.skin                                                  parser probe PASS
Azeroth_30..33_48.adt                                                  parser probe PASS
```

A full Bevy example link was attempted but `rustc` exited 1 for Bevy crates after the five-minute
timeout. There is **no** Bevy loader/runtime or visual claim. The app was neither edited nor run.

## GLM actions required

1. **Apply the profile source call** in `crates/benilla-app/src/lib.rs` before `AssetPlugin`.
   Exact environment/config contract: `docs/opencode/route-asset-handoff.md`.
2. **Consume bridge v2.1** (`docs/opencode/bridge-health-contract-v2.1.md`):
   - `ProducerSnapshot::world_generation`
   - `ProducerSnapshot::position_observed_at_unix_ms`
   - `ProducerSession::source_health() -> Option<SourceHealth>`
   - current `Observation::new` argument order
   - pump health/ACK handling
   Update GLM sender and OpenCode receiver together; v2 and v2.1 do not interop for health.
3. **Pin/apply the route**: WMO/doodad paths and the exact walk path; then run stand/walk/fall,
   Boar skin/animation, terrain/liquid and screenshot acceptance.
4. Keep the bridge/asset worktrees' changes; do not overwrite them with older F08 or older
   producer files.

## Known gaps / risks

- No app call, no Bevy load, no socket/DB/WM run, no game run, no visual check.
- v264 global-sequence bones, hand-grip poses, cameras, particles, ribbons and string/fishing
  anchors still use vanilla-only paths. None is required for the Boar actor or Elwynn liquid.
- M2 companion skins use LOD 0 only.
- WMO/doodad route handling is not newly proven here; terrain and MH2O are.
- `cargo check`/unit tests are clean; the full Bevy example link failure is environmental/build,
  not a parser diagnostic, and must be re-run in the final app build.
- Cargo.lock changed only for the local `benilla-profile` dependency.

C1-C3 hashes are in `docs/opencode/route-asset-handoff.md`, "C1-C3 checkpoint hashes".
