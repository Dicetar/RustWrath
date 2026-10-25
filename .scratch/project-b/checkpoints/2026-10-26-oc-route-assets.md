# OpenCode checkpoint - pinned route asset manifest (O5)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `07e1aefe` (Astra's loop inventory on top of the death slice)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O5.
Purpose: pin the route's data dependencies from static sources, verify the source pipeline
stages, name the demonstrated parser repairs, and hand the future graphical/live gate a
concrete manifest and failure checklist. Source-only: no launch, no archive extraction, no
install writes.

## File claim

Editing: this checkpoint only. No client source change in this slice; the WMO finding below
needs a version-aware layout decision before code moves.

## Install and profile provenance (read-only inspection)

- Client install: `D:/WOW/world of warcraft 3.3.5a hd/data` (build-12340 MPQ set). Locale
  archives live under `data/enus/`; `patch-5/j/u.mpq` are owner-disabled.
- Approved profile manifests (protected worktree, read-only): `base.mpqlist` (stock archives
  only) and `wm-hd.mpqlist` (stock + owner's custom `patch-6/7/8/S/a/b/c/e/f/k/w/y/z` and
  `enus/patch-enus-{a,c,f,k,m,q,w}`). Selection: `WM_ASSET_MANIFEST` + `WM_ASSET_PROFILE`;
  without the variable the vanilla discovery path is used.
- In-repo source chain read: `benilla-profile` (line-based manifest, ascending precedence,
  absent/disabled archive is a hard error, `Status` reports the winning archive), then
  `ProfileAssetReader`/`ProfileLooseAssetReader` (`mpq://`, `mpq-loose://`), then Bevy loaders.

## Pinned route dependencies (static)

- Map 0 = `Azeroth` (`Map.dbc` row 0, directory name field), confirmed from the server's
  3.3.5 DBC extract.
- Willem `(-8947.64, -132.319, 83.7199)` and Garrick `(-9056.48, -460.903, 72.8318)` both land
  in ADT tile `(tile_x, tile_y) = (32, 48)` by `world_to_tile`'s axis-swapped formula. Central
  tile: `World/Maps/Azeroth/Azeroth_32_48.adt`. The chunk-window streamer keeps a 3×3
  neighbourhood ready and widens to the farclip window, so the manifest must carry tiles
  31–33 × 47–49 at minimum (WDT `MAIN` gates the rest; `Azeroth.wdt` and `Azeroth.wdl` are the
  map-level files).
- Willem display 2072 and Garrick display 2073 (`creature_template_model.sql:667,94`) both map
  to `CreatureDisplayInfo` 2072/2073 → model 49 → `Character\Human\Male\HumanMale.mdx`, i.e.
  `Character/Human/Male/HumanMale.m2` plus its external LOD-0 `HumanMale00.skin`, with no
  texture variations on either display. The `.mdx`/`.mdl`→`.m2` extension rule and the external
  skin load already exist (`benilla-assets/src/lib.rs:206-324`, `m2.rs:238-247`), and the M2
  reader accepts version 264 (`benilla-m2/src/lib.rs:45-49`).
- Item 182 (Garrick's Head) display 7038 → icon `INV_Misc_Head_Human_01`; item 6076 (Tapered
  Pants) display 16843 → icon `INV_Pants_11` (`ItemDisplayInfo.dbc`, inventory-icon columns).
  Both resolve under `Interface/Icons/…blp` through the same profile reader. The reward's worn
  leg texture columns are not required for the route's quest receipt.
- Fireball 133 → `SpellVisual` 67 (precast kit 30, cast kit 38, impact kit 286), from
  `Spell.dbc`/`SpellVisual.dbc`; the existing creature-anim spell-visual path consumes these
  kit ids from the active profile's DBCs.

## Pipeline stages verified in source

- Terrain: ADT chunks read by reversed magic with per-chunk `MCNK` offsets; the header offsets
  are the 12340 layout (position at 0x68), and `MH2O` is parsed alongside the legacy `MCLQ`
  (`benilla-adt/src/lib.rs:499-568`, 171-234). 3.3.5 layer/alpha data is handled
  (`combined_alpha.rs`). WDT `MAIN` gates streaming; WDL exists for the far shell.
- Models/doodads: ADT placements (MDDF/MODF) feed the same `.m2`/`.wmo` loaders; WMO group
  files are `<stem>_NNN.wmo`; collision accumulation for terrain and WMO groups is wired.
- DBC path: the 3.3.5 reader loads from the same profile; Map/CreatureDisplayInfo/
  CreatureModelData/ItemDisplayInfo/Spell/SpellVisual chains are available.

## Demonstrated parser mismatch (needs a version decision before code)

The WMO group header is read with vanilla offsets: `flags` at 0x08, portal refs at 0x24,
fog indices at 0x30, `group_liquid` at 0x34 (`benilla-formats/src/models/wmo/group.rs:59-80`,
`benilla-wmo/src/lib.rs:236-241`). The 12340 layout, per the local build's own vmap extractor
(`src/tools/vmap4_extractor/wmo.cpp:196-210`), is: groupName 0x08, descGroupName 0x0C, flags
0x10, bbox 0x14–0x2C, moprIdx 0x2C, moprNItems 0x2E, nBatchA 0x30, nBatchB 0x32, nBatchC 0x34,
fogIdx 0x38, groupLiquid 0x3C, groupWMOID 0x40; the sub-chunk list still starts at 68 in both.
Consequence: 3.3.5 WMO geometry, batches and textures load correctly, but indoor/outdoor flags,
WMO liquids, fog and portal refs are read from the wrong fields. Repair requires a
version-aware header (or a 1.12-client confirmation that the donor offsets are the vanilla
ones), so it is recorded here rather than edited blind. This is the first WMO repair for the
graphical gate; the tracked file claim for it will name the two parsers and the version source.

## Route and failure checklist for the gate (not executed)

1. Launch with the `base` profile and the source identity recorded (launch card:
   `2026-10-25-oc-graphical-acceptance.md`).
2. Expect a non-empty world, camera follow, and the player body from `HumanMale.m2`; a
   placeholder cube means the display/model chain or the profile read failed.
3. Tiles 32/48 and neighbours must appear with terrain, doodads and the abbey WMO; a void
   horizon means the WDT/tile window or the streamer failed.
4. Willem must render as a human male NPC (display 2072); Garrick as display 2073.
5. Icons `INV_Misc_Head_Human_01` and `INV_Pants_11` must resolve in the loot/reward UI; a white
   icon means the ItemDisplayInfo icon column or the BLP read failed.
6. Fireball must show its precast/cast/impact kits (30/38/286).
7. Failure triage order stays: source/profile identity, catalog/asset path (Map,
   CreatureDisplayInfo, CreatureModelData, M2/SKIN/BLP), translation/camera, server response.

## Pending

Graphical and live acceptance remain separately gated (G/L), so every item above is
source-observed only. The WMO header decision and the Fireball kit-to-texture resolution are the
two facts the gate itself must confirm against the install.
