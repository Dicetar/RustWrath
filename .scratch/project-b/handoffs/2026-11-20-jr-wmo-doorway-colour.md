# WMO doorway colour parity

Status: pending CI
Owner: Claude Opus 5.5
Branch: jr/wmo-doorway-colour
Issue: none
Next: Read CI run 44654700061 to completion; land only after green

## Reference

Pinned reference `D:/Projects/wow/reference/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `MapObjRead.cpp:33-54` defines `CMapObjGroup::FixColorVertexAlpha`; the loader calls it at `MapObjRead.cpp:317-325` only when the root `MOHD.flags & 0x8` is clear. It partitions by the last transparency batch's `m_vertexEnd + 1`. Earlier/interior vertices have each MOCV RGB channel shifted right once and alpha preserved. Remaining/exterior vertices get `channel = min(255, (channel + ((alpha * channel) >> 6)) >> 1)` and alpha becomes 255. Current `origin/main` includes unified WMO prerequisite `c54d862e`.

## Change and verification

`benilla-wmo::Batch` now retains MOBA `vertex_start`/`vertex_end`. `benilla-formats` applies the Wrath load-time conversion to the MOCV split after the last transparency batch, gated by `MOHD.flags & 0x8`; the existing render-time portal attenuation remains a distinct pass. Two focused unit cases cover the conversion and empty transparency-batch boundary.

`git diff --check` passes. CI run `44623297097` failed Linux rustfmt; the source formatting correction was pushed as `76249396`. Corrective CI run `44654700061` has passed Linux rustfmt so far and remains in progress. No local Cargo, client or server run; gameplay behavior is not proven.
