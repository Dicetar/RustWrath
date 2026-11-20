# Unified WMO shader modes

Status: landed
Owner: Claude Opus 5.5
Branch: jr/wmo-unified-modes
Issue: none
Next: none; landed in main

## Reference

Pinned reference `D:/Projects/wow/reference/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `CMapObj::UnifiedRender` in `WoW/Source/WorldClient/MapObj.cpp:916` selects mode 0 for unlit, mode 1 for ordinary lighting, mode 2 for WINDOW midpoint lighting, and mode 3 for the root ambient. The mode selection is material-driven: `(material.m_flags & 0x20) != 0` selects mode 2 before the group class branch.

## Change and verification

Landed through `67e17a6c`; main's follow-up is included in `eace0985`/`89f5dad4`. CI run `44624330753` had a runner-only `wayland-sys` build-script panic during Linux test-build; rerunning cleared it. No local Cargo, shader compiler, or live gameplay acceptance is proven.
