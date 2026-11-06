# Route asset-source handoff (work package 01, C1-C3)

Updated: 2026-10-25. Status: **C1/C2/C3 SOURCE-IMPLEMENTED IN ISOLATED WORKTREE; CRATE CHECKS AND
REAL-ASSET PARSER PROBES PASS; APP CALL, BEVY LOADER RUNTIME, VISUAL ACCEPTANCE AND GLM v2.1
INTEGRATION PENDING**.

## Worktree identity

- Base: `D:/Projects/project-b-client` clean `1f8d33b8`.
- Isolated worktree: `D:/Projects/project-b-opencode-assets` at detached `1f8d33b8`; existing dirty
  work in the primary checkout was not touched.
- Changes:
  - `crates/benilla-profile/src/lib.rs` — candidate-first `Profile::reload`.
  - `crates/benilla-assets/Cargo.toml` — adds the local `benilla-profile` dependency.
  - `crates/benilla-assets/src/lib.rs` — exports the profile source.
  - `crates/benilla-assets/src/profile_source.rs` — Bevy `AssetReader`s and registration.

## Actual source path

`ProfileAssetReader` reads the manifest-defined `benilla_profile::Profile`; it does not auto-discover
archives. `ProfileLooseAssetReader` checks the install's loose index first and then the same profile.
`register_profile_source(app, data_dir, manifest_path, profile)` registers the existing source ids:

- `mpq://`
- `mpq-loose://`

Therefore current `benilla-assets` loaders keep their existing URLs; no second asset pipeline is
introduced. Disabled archives and absent owner toggles remain hard errors that name the archive,
manifest and profile. `Profile::reload` now opens the complete candidate set before replacing the
live set, so a failed reload leaves the previous profile usable.

## Exact GLM-owned application handoff

File: `crates/benilla-app/src/lib.rs`, currently the `register_mpq_source` call near line 337,
before `DefaultPlugins`/`AssetPlugin` builds the sources.

Do not call both registration functions. Use an explicit owner selection; no hidden discovery:

```rust
let profile_manifest = std::env::var_os("WM_ASSET_MANIFEST").map(std::path::PathBuf::from);
let profile_name = std::env::var("WM_ASSET_PROFILE").unwrap_or_else(|_| "base".to_owned());
let source_result = match profile_manifest {
    Some(manifest_path) => benilla_assets::register_profile_source(
        &mut app,
        &data_dir,
        &manifest_path,
        &profile_name,
    ),
    None => benilla_assets::register_mpq_source(&mut app, &data_dir),
};
if let Err(e) = source_result {
    return no_install::fail(
        &no_install::NoInstall::unreadable(&data_dir, &e),
        run_mode::unattended(),
    );
}
```

The app already has `benilla-assets`; no new application dependency is needed. `benilla-profile` is
used inside `benilla-assets`. If the owner chooses to expose a config field instead of environment
variables, the same three values are required: data directory, manifest path, explicit profile
name.

## Route/profile pins

- Base profile: `docs/profiles/base.mpqlist`.
- Owner profile: `docs/profiles/wm-hd.mpqlist`.
- Route terrain tiles confirmed by sampled files: `Azeroth_30..33_48.adt`, with the additional
  boundary neighbor still to be pinned when the exact walk path is fixed.
- WDT: `World\Maps\Azeroth\Azeroth.wdt`; owner profile uses the patch-6 override.
- Liquid: the sampled Elwynn route is MH2O-only. Legacy MCLQ is not the route path.
- Actor sample: `Creature\Boar\Boar.m2` v264 with `Boar00/01.skin`.
- DBC/dependency list remains the one in `docs/baseline/route-assets.md`: Map, AreaTable, Light*,
  CreatureDisplayInfo(+Extra)/CreatureModelData, ChrRaces/ChrClasses/CharSections, minimal item
  display, route WMO/doodad and BLP dependencies.

## Remaining blockers

- The app call above is unapplied because `benilla-app` is GLM-owned in the active workspace.
- C1 cannot claim rendered terrain until the app call is applied and visually exercised under
  existing authorization.
- C2 is not implemented: v264 header/bone/animation layout and external `.skin` parsing need a
  verified format source. Merely widening the version gate was rejected.
- C3 is not implemented: the route's MH2O layout and structure/doodad collision dependencies need
  verified chunk/format evidence before a parser is written. No layout was guessed.
- No execution checks were run.

## Actor/liquid parser increments in this pass

- `crates/benilla-m2/src/skin.rs` now has `parse_external_skin` for the authoritative WotLK
  `SKIN` header: vertex/index arrays, 48-byte skin sections and 24-byte batches. Shadow batches are
  deliberately unread. This is parser-only: the v264 MD20 header/bone/animation layout is still not
  implemented, so the version gate was not widened and no actor is claimed.
- `crates/benilla-adt/src/lib.rs` now parses the authoritative WotLK MH2O chunk into
  `RootAdt.mh2o`: 256 12-byte grid headers, 24-byte liquid instances, exists bitmaps, attributes and
  LVF 0-3 vertex arrays. It is not yet converted into `benilla_formats::LiquidMesh`, because that
  conversion needs the route's LiquidType/material mapping and the existing liquid mesh builder's
  kind/depth policy. No guessed mapping was inserted.
- The isolated source hashes for these files are recorded in the final handoff, not in this
  historical C1 section.

These are confirmed-layout parser increments, not completed actor/liquid acceptance. The missing
application choices are named above rather than hidden behind a placeholder success.

Parser checkpoint hashes (superseded by the C1-C3 checkpoint below).

## C2/C3 implementation update (supersedes the blockers above)

Implemented in `D:/Projects/project-b-opencode-assets`:

- **v264 MD20 header**: `benilla-m2` admits version 264; Wrath header has no PlayableAnimationLookup
  or embedded M2View table, uses 88-byte bones (pivot `+76`, tracks `+16/+36/+56`), 40-byte
  attachments and 36-byte events.
- **v264 sequences and keys**: 0x40-byte sequences (`duration` at `+4`, box at `+0x20/+0x2c`),
  per-sequence timestamp/value arrays; translation/scale float tracks and compressed 16-bit
  quaternion rotations using the biased decompression
  `(v < 0 ? v+32768 : v-32767)/32767`. Identity `(32767,32767,32767,-1)` decodes to `(0,0,0,1)`.
- **Wrath event tracks**: per-sequence timestamp subarrays, relative to sequence start.
- **External skins**: `M2ModelLoader` reads the sibling `<stem>00.skin` through the registered
  profile source and calls `parse_m2_render_submeshes_with_skin`. This reuses the existing
  `mpq://` pipeline; no second asset pipeline. **LOD 0 only**.
- **Route liquid**: MH2O instances convert to existing `LiquidMesh`; positions use the 1/8-chunk
  lattice, row-major exists bitmaps, heights/depths, generated UVs, and Wrath `LiquidType.dbc`
  ids. Type 5 (the route's only sampled type) is Still; 1/2/3/4 and the shipped family ids map to
  water/ocean/magma/slime. The terrain loader attaches each mesh to its `index_x/index_y` chunk.
- **Bounds**: v264 Stand/Swim boxes read from the Wrath sequence offsets, not vanilla's.

Real-asset probes (read-only, owner install):

```text
cargo check -p benilla-assets -p benilla-formats -p benilla-m2
  PASS
cargo test -p benilla-formats -p benilla-m2 -p benilla-adt --lib
  543 + 20 + 9 PASS
Boar.m2 + Boar00.skin:
  submeshes=2 bones=61 animations=17 bounds=non-empty collision=non-empty
Azeroth_32_48.adt:
  chunks=256 liquid meshes=42 all Still, wet=1199, vertices=2100
Azeroth_30..33_48.adt:
  99 MH2O instances, all LiquidType=5 LVF=0
```

A full Bevy example link was attempted but `rustc` exited 1 for Bevy crates after the 5-minute
timeout; no Bevy loader/runtime/visual claim is made. The full app was not modified or run.

Still not done / not claimed:

- `benilla-app` profile-source call remains GLM-owned and unapplied.
- GLM's bridge is still v2; it must consume v2.1 before ACK/health interop is accepted.
- v264 global-sequence bones, hand-grip poses, cameras, particles, ribbons and string/fishing
  anchors still use vanilla-only paths; none is required for the Boar actor or Elwynn liquid.
- WMO/doodad route pins and stand/walk/fall/visual acceptance remain open.
- No commit was made.

C1-C3 checkpoint hashes (2026-10-25):

```text
Cargo.lock                                    76ceb33511482e781634933a8737295287a098d40321bbbee2ed9c5747b6c7ec
crates/benilla-adt/src/lib.rs                 aa0dacec08de25c6a5208796e113e46f0904f6362ecff5858420dddf65f00730
crates/benilla-assets/Cargo.toml              f5687c10805b37f72c44abf383418e6d1294aefdcf2429b343e6cea4d2f8ea2f
crates/benilla-assets/src/lib.rs              59c06bfc7fd263884bbb518988afef5a8d1cd78330f76fe674df9f33f665c421
crates/benilla-assets/src/m2.rs               9db2505adb36b768edddcfe99d990fd928fabe70830f3281801f7415c61b1340
crates/benilla-assets/src/profile_source.rs   508d1e4b2aa2b7ef9dcb8c5d5ed82994312fda5b3f4895f4b76b5b7b3eb69434
crates/benilla-formats/src/lib.rs             b935602fa522fe4e978cf8e9ca404b816700983c55ac8d622c4e7ec0529af7ef
crates/benilla-formats/src/liquid.rs          1989171b1ee0e5712963bbf1f3577d3ba4299f5c5a29cedd4c0311a4bdad8666
crates/benilla-formats/src/models/anim.rs     c3efd9df01f2e01f48486aa1eecd3dca6ab50bc3f1895070b066a28fd78f87b8
crates/benilla-formats/src/models/bounds.rs   1c3ef9edfecb8ed69596dba47583f01e67224672adf3dd302bff42a1e1e94038
crates/benilla-formats/src/models/m2_batches.rs ebe36a8791c34936a9095ae9f4ae7a2bdd454ee69664f6dfb1f43a3cd3b7da18
crates/benilla-formats/src/terrain.rs         edd2ce496c8a0efedf5b8f4d4c080166adb32a83504b9c5262260a6094de9389
crates/benilla-m2/src/error.rs                b46609a4328fb325588d5035efb55c6092ab66759e9a15b2841b3b4c7b218553
crates/benilla-m2/src/lib.rs                  d5a8999dd60ac139ac04c9f980821d0be6b5deec87554558462cb0ec6588ac38
crates/benilla-m2/src/skin.rs                 ffab90dc34fa50ebca1e4fdc5b6df7a1a26c5e94bbf13cf3db3edf5ee1e38203
crates/benilla-profile/src/lib.rs             330aaf6d86af4ebf60e5479114483bdb581caa00fabc418ba6c97be1402c0a21
```