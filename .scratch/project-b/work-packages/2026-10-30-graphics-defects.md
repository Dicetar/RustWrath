# Remaining graphics defects — Astra repair pass

## Water-plane follow-up

Owner reports massive intermittent glitch planes, suspected water batching. Source inspection found each ADT liquid surface keeps its own mesh; material sharing alone does not concatenate their vertices. A concrete upstream geometry defect was found instead: MH2O vertex payloads were decoded as interleaved structs rather than contiguous height, UV and depth arrays. Confirmed format reference: https://335.trinitycore.net/d3/df0/adt_8h_source (GetLiquidHeight, GetLiquidDepth, GetLiquidTextureCoordMap). The decoder now reads each array at its own offset.

Regression `mh2o_height_and_depth_arrays_are_not_interleaved` failed before repair: expected heights `[12,12.5,13,13.5]`, obtained `[12,5.995136e-39,2.343e-41,0.010258735]`. After repair, all 10 benilla-adt tests pass. `liquid_heights` checks the real extracted camp tile's 33 MH2O layers against authored min/max bounds for vertices actually referenced by wet cells; it passes. Dry unused heights must not be interpreted as a rendered-geometry failure.

Source fix is uncommitted in `benilla-adt/src/lib.rs`, with the diagnostic example `examples/liquid_heights.rs`. No client rebuild or same-camera render comparison in this follow-up yet. Keep the owner's complete plane symptom open until that comparison; water-decoder correctness does not establish that all remaining scenery glitches share this cause.

Source baseline: `f82d12dd`, 2026-10-30. This ledger refines the graphics recovery draft. It records observations and acceptance conditions, not full replacement completion.

Ownership: Astra edits `benilla-blp/src/lib.rs`, its `examples/inspect_mips.rs`, `benilla-formats/src/lib.rs`, `benilla-assets/src/blp.rs`, and this document. OC's dirty `capture/mod.rs`, `dev.rs`, and untracked combat probe are outside this repair. No server, identity, install, or gameplay mutation is needed.

## Prioritized visible defects

| ID | Observation and evidence | Status / likely investigation boundary | Acceptance |
|---|---|---|---|
| G30-01 | Pale jagged surfaces across the upper/left background, severe texture stretching; latest `oc-run-20261030-143128-296.png` | Open. Screenshot alone does not identify terrain, WMO, model, or an intentional custom asset. Trace drawn object/asset identity and compare its selected-profile bytes before changing geometry or shaders. | Same location/camera with coherent expected geometry/materials; confirm against the selected asset/reference and one adjacent view. |
| G30-02 | Player and several nearby actors appear minimally clothed despite the equipment translation commit | Open. Separate actual equipped server fields, item/display IDs, body-atlas regions, geoset masks and attached meshes. Do not assume every NPC is expected to wear armor, or that every item is an attachment. | Known equipped piece appears in its proper atlas/mesh location, survives relog, and changing/removing it updates appearance. |
| G30-03 | White floating quest-marker shapes rather than clear expected marker appearance | Open. Establish marker asset, texture resolution, blend state and animation before changing tint. | Correct status-dependent marker appearance at the same NPC and correct adjacent quest status. |
| G30-04 | Foreground vegetation looks unusually solid/sharp and world textures are visibly repetitive | Open, appearance concern. May be selected HD art or sampling/material/UV mismatch; screenshot is insufficient to choose a cause. | Profile-correct asset/material comparison, opaque/cutout control and a distance change. No arbitrary global alpha or UV tweak. |
| G30-05 | Quest description body blank under the Description heading in `oc-run-20261030-131125-846.png` | OC-owned UI follow-up. Title, objective and buttons have improved; text in the API is not rendered acceptance. | Visible full description, readable clipping/scrolling and working controls. |
| G30-06 | Version-264 particle payloads remain intentionally unsupported by the guard | Known missing effects. Decoder/layout repair is separate from this texture change; retain allocation protection. | Correct required cast/impact effects with bounded lifetime/memory; malformed data rejected safely. |
| G30-07 | Northshire `piinetree01.blp` fails with `BLP mip level 9 out of bounds` | Reproduced and loader fallback implemented below. Full client visual acceptance pending. Not established as the cause of G30-01 or G30-04. | World-art load retains the valid image with a warning; tree visible at its original location; malformed base still fails. |

Evidence paths above are under `.scratch/project-b/verification/2026-10-28-oc-alpha/`. Earlier missing-body and white-chat-box symptoms have improved in the current screenshots; do not reclassify them as newly failing without a reproduction.

## G30-07 diagnosis and repair

Original failure retained in `worldview-profile.log`, lines 46–47. Read the texture through `profile-probe` with `docs/profiles/wm-hd.mpqlist` and the owner's read-only install. A local scratch copy is `astra-piinetree.blp` (350,724 bytes; never commit game assets).

The existing `blpx` reproduced `OutOfBounds { level: 9 }`. Header dimensions are 256x1024. Mips 0–8 have valid spans; mip 9 declares offset 350,724 (EOF) and size 4,294,616,572. This is corrupt authored metadata, not a missing file or evidence that a four-gigabyte allocation is necessary.

The rendering path now stops at the first out-of-bounds optional mip and uploads only the preceding validated levels. It does not modify game data, guess missing pixels, or recover level zero. A path-specific warning identifies the rejected level. Strict `decode`, `decode_native` and strict format adapters retain their diagnostic behavior. The fallback is limited to the WorldArt loader, rather than silently relaxing every BLP consumer.

This is an explicitly documented resilience deviation for custom malformed art, not a claim about reference-client handling of corrupt files.

Verification:
- New synthetic regression failed before implementation and passed afterward. It checks preserved mip bytes, explicit rejected-level reporting, strict rejection, and failure on corrupt level zero.
- `cargo test --locked --offline -p benilla-blp --lib`: 9 passed, zero failed/skipped, including unchanged valid BC1/BC2/BC3 chains.
- `cargo run --locked --offline -p benilla-blp --example inspect_mips -- .scratch/project-b/verification/2026-10-28-oc-alpha/astra-piinetree.blp`: strict error at level 9; rendering fallback returns 256x1024 and nine correctly sized levels, ending at 1x4/16 bytes.
- `cargo check --locked --offline -p benilla-assets --lib`: passed (includes formats and BLP dependencies). `git diff --check`: passed. No new client build or live scene comparison was performed. Keep visual acceptance open.

## OC integration instructions

Preserve these graphics edits when finishing your quest work. Rebuild the actual client before evaluating the loader repair, then use the existing Northshire/profile-correct location for the affected tree. Expect one warning naming the corrupt mip instead of an entire texture-load failure. A screenshot from the current expedition camp does not prove the Northshire asset fixed.

For G30-01, identify one offending draw/asset before another live run. For G30-02, choose one known equipped item and trace its appearance end to end. Keep the existing three-backup limit; this repair adds no binary backup, installation copy, or new dependency. Incremental compilation was disabled for this pass to reduce additional cache growth.
