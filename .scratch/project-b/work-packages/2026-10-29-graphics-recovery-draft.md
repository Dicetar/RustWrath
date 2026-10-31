# Graphics recovery — OC implementation draft

Companion to [playable foundation](2026-10-29-playable-foundation.md). Planning pin `b03a919b`.
Purpose: make the connected client visually usable and establish causes before changing graphics code.

## Starting evidence and limits

Historical evidence in `../verification/2026-10-28-oc-alpha/`: `oc-zoom-live.png` shows no obvious own-player body, a solid-white chat region and suspicious foreground vegetation. It does not establish the cause of any of these. `oc-model-late-20261029-170055-388.png` shows an earlier glue GetCVar failure, not proof that the newest launch still fails there. Inspect the latest source and binary before reproduction.

The uncommitted root Visibility/InheritedVisibility addition in app `wrath.rs` and glue CVar installation in UI `script/mod.rs` belong to OC. Preserve them, test their actual effect and narrow them if necessary. Do not mark all B0004 warnings as the player-body root cause.

## G0 — valid comparison

Record source SHA + dirty paths, binary hash, profile/manifest hash, selected archive provenance, window size/settings, session/map/GUID/position and camera pose. Keep identity secrets out of logs. Use one fixed view and a nearby changed state; do not search for a flattering screenshot. Where an original-client comparison is available, use the same profile/location/camera approximately and distinguish existence comparisons from numeric timing measurements. Worldview can isolate assets but cannot accept client body/UI/input.

## G1 — trace the own-player body, end to end

Inspect `benilla-app/src/wrath.rs`, `player/embody.rs`, `player/body_pose.rs`, `player/view_subject.rs`, `entities`, `benilla-assets/src/model*`, `benilla-world/src/model_render*`, `model_fade.rs`, `rig_anim.rs`.

Trace the current player GUID to replica entity → display/native display identity and appearance → resolved model and texture paths → M2/skin load result → root and mesh child entities → geoset selection and equipment attachments → bone palette/pose → global transforms/bounds → inherited visibility/layers/frustum and first-person fade → actual draw submission.

At each stage distinguish absent, pending, failed and valid. Inspect finite scale/position and ancestor components. Do not create a second player mesh or force every entity visible to bypass a broken owner. Explain the exact missing edge before editing.

Exit: identified own body visible in third person, correct identity/skin/clothes to the supported appearance contract, coherent idle/walk/turn animation, appropriate first-person hiding, body returns on zoom-out; nearby NPC remains correct. A screenshot of an uncorrelated nearby actor does not pass.

## G2 — material/texture correctness

Inspect `benilla-assets/src/materials.rs`, `blp.rs`, `gpu_blp.rs`, `m2.rs`, `wmo.rs`, `shaders/wow_model.wgsl`, and world model rendering. For one visibly defective asset record the archive/path, format version, decoded dimensions/mips/alpha, texture lookup and replaceable slot, batch material flags/blend mode, culling/depth write/test, texture coordinates and vertex color/alpha.

Test competing causes: wrong archive/texture identity, invalid skin/material indices, failed texture fallback, alpha decoding, alpha-test threshold/blending, depth ordering, UV mapping or color-space error. Terrain tiling is not alone proof of broken UVs. Keep data-layout fixes separate from renderer-state fixes.

Exit: selected plant cutout has expected transparent background, opaque model surfaces remain opaque, translucent surface does not erase unrelated geometry, no invalid bounds/index allocations. Verify original asset and one adjacent blend/opaque case. No global alpha multiplier or random shader tweak as a substitute for the source material contract.

## G3 — white UI region and core readability

Inspect `benilla-app/src/ui_pass.rs`, `ui_script` extraction, UI texture/material routing and `benilla-ui` widget/layout/texture state. Identify the exact frame/region that paints the white rectangle: frame identity, dimensions, layer, texture/color source, texture load status, vertex color/alpha, clipping and blend pipeline. Determine whether the source is a color texture, unsupported path, missing texture fallback, bad atlas/UV or incorrect paint ordering.

Fix the primitive or missing binding semantics, not stock FrameXML assets. Check chat background/text, tooltip, action icons, unit bars and a dialog against the same shared primitive. Handle absent optional texture state according to the reference instead of painting white indiscriminately.

Exit: no opaque white obstruction; readable chat/tooltips/dialogs; UI scale/resize and map open/close do not recreate it. Preserve correct opaque color textures.

## G4 — Wrath animation/effects safely

Inspect `benilla-m2/src/model.rs`, `track.rs`, `skin.rs` and world particles/ribbons alongside actual version-264 model bytes and a pinned layout reference. Existing particle refusal prevents a real misdecode/allocation failure; do not merely delete it.

Implement version-specific record sizes, offsets, nested track counts, sequence/global timing and over-life data with checked byte ranges and allocation limits derived from file size. Preserve the vanilla decoder. Validate small malformed/truncated cases and actual selected-profile assets before production effects. Confirm no bogus giant tracks, stable memory and correct spawn/update/despawn lifecycle.

Exit: required selected-route cast/projectile/impact feedback is rendered from correct state, repeat casts do not leak, stationary effects and one unsupported/malformed input fail safely. Full ambient weather/ribbon/effect coverage can follow but must remain explicitly open.

## G5 — world coherence and performance

Only after G1–G3, investigate remaining route-visible terrain layers, WMO placement, lighting/fog, water and shadows from specific examples. Cross-check asset placement transforms and format variants; do not rebuild the renderer for general visual dissatisfaction.

Measure CPU/GPU/frame-time where available, streaming hitches and private memory separately. Group recurring hierarchy warnings by actual producer and fix the offending parent/child contract; suppressing the warning is not a repair. Compare a fixed stationary tail and the same movement segment at pinned settings.

Deliver before/after captures, exact cause/fix and one adjacent-state check for each symptom. Stop calling graphics repaired if only logs improved. A1 requires G1–G3, coherent route world and adequate G4 feedback; full visual fidelity remains the replacement gate.
