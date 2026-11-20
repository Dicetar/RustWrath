# Unified WMO shader modes

Status: blocked on Linux test-build
Owner: Claude Opus 5.5
Branch: jr/wmo-unified-modes
Issue: none
Next: Obtain the Linux test-build diagnostic for run 44624330753, fix it, and rerun CI

## Reference

Pinned reference `D:/Projects/wow/reference/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `CMapObj::UnifiedRender` in `WoW/Source/WorldClient/MapObj.cpp:916` selects mode 0 for unlit, mode 1 for ordinary lighting, mode 2 for WINDOW midpoint lighting, and mode 3 for the root ambient. The mode selection is material-driven: `(material.m_flags & 0x20) != 0` selects mode 2 before the group class branch.

## Change and verification

The unified WMO branches in `wow_model.wgsl` and `static_gx.wgsl` now select the existing midpoint light for WINDOW batches in both lit and ambient terms, including the mode 2 path outside interior-only conditions. The existing mode 0 fullbright override and mode 3 packed root ambient path remain explicit. CI run `44624330753`: Linux fmt, clippy, and changed checks passed; Linux test-build failed with exit code 101 and test was skipped; Windows job passed. The public check annotation provides no compiler diagnostic and the log endpoint is inaccessible, so the branch is blocked without a verified cause. No local Cargo, shader compiler, client or server run.
