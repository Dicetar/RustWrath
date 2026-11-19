# Unified WMO shader modes

Status: ready to merge
Owner: Claude Opus 5.5
Branch: jr/wmo-unified-modes
Issue: none
Next: integrator reviews and lands after CI

## Reference

Pinned reference `D:/Projects/wow/reference/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `CMapObj::UnifiedRender` in `WoW/Source/WorldClient/MapObj.cpp:916` selects mode 0 for unlit, mode 1 for ordinary lighting, mode 2 for WINDOW midpoint lighting, and mode 3 for the root ambient. The mode selection is material-driven: `(material.m_flags & 0x20) != 0` selects mode 2 before the group class branch.

## Change and verification

The unified WMO branches in `wow_model.wgsl` and `static_gx.wgsl` now select the existing midpoint light for WINDOW batches in both lit and ambient terms, including the mode 2 path outside interior-only conditions. The existing mode 0 fullbright override and mode 3 packed root ambient path remain explicit. No local Cargo, shader compiler, client or server run; CI result pending.
