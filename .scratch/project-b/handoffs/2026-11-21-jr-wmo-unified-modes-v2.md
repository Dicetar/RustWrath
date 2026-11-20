# Unified WMO modes 0 and 2

Status: ready to merge
Owner: ClaudeJr / Claude Opus 5.5
Branch: jr/wmo-unified-modes-v2
Issue: none
Next: Read CI for the pushed commit, then land after green CI.

## What changed

- `wow_model.wgsl` now keeps the unified WMO lane out of the generic fullbright
  override. Unified mode 0 uses the reference MapObjU expression
  `clamp(2 * texture * MOCV)`, with the root's converted MOCV scale already
  supplied by the WMO loader. Non-unified M2/WMO unlit behavior is unchanged.
- `static_gx.wgsl` applies the same unified mode-0 expression to retained WMO
  batches and excludes unified items from the generic `WORD_UNLIT` overwrite.
- Unified mode 2 continues to select the window midpoint lighting for both
  unified exterior and interior batches, matching the reference's WINDOW
  branch before group-class selection. The existing unified mode 0/2 data
  plumbing and ambient packing are retained.

Reference basis: `CMapObj::UnifiedRender` in `WoW/Source/WorldClient/MapObj.cpp`
(around `MapObj.cpp:916`), including the MapObjU unlit and WINDOW paths.

## Verification

- `git diff --check`: pass.
- Local Cargo, client launch and server launch: not run per ticket rules.
- CI: not run yet; branch commit is being pushed for CI validation.
- In-game check is not proven. Owner should inspect the unified-root WMO in
  Stormwind's canal plazas, identified by the prior `c54d862e` report.
