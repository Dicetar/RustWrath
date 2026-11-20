# WMO doorway colour parity

Status: landed
Owner: Claude Opus 5.5
Branch: jr/wmo-doorway-colour
Issue: none
Next: none; landed in main

## Reference

Pinned reference `D:/Projects/wow/reference/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `MapObjRead.cpp:33-54` defines `CMapObjGroup::FixColorVertexAlpha`; the loader calls it at `MapObjRead.cpp:317-325` only when the root `MOHD.flags & 0x8` is clear. It partitions by the last transparency batch's `m_vertexEnd + 1`. Earlier/interior vertices have each MOCV RGB channel shifted right once and alpha preserved. Remaining/exterior vertices get `channel = min(255, (channel + ((alpha * channel) >> 6)) >> 1)` and alpha becomes 255. Current `origin/main` includes unified WMO prerequisite `c54d862e`.

## Change and verification

`benilla-wmo::Batch` now retains MOBA `vertex_start`/`vertex_end`. `benilla-formats` applies the Wrath load-time conversion to the MOCV split after the last transparency batch, gated by `MOHD.flags & 0x8`; the existing render-time portal attenuation remains a distinct pass. Two focused unit cases cover the conversion and empty transparency-batch boundary.

Landed through `eea64f24` and the follow-up main fixes in `eace0985`/`89f5dad4`. The original CI formatting failure was corrected, but the first implementation caused a visible regression: load-time FixColorVertexAlpha ran for every root, while static_gx already folds that conversion into its interior term, making inns and smithies twice as bright and forcing alpha to 255, which stopped doorway fading and left corridors black. `eace0985` restricts the conversion to unified roots (MOHD 0x2). No live gameplay acceptance is proven.
