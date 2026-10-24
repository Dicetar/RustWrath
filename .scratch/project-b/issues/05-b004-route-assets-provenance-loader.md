# 05 — B-004: first-route assets + provenance loader

**What to build:** manifest-defined patch-chain loader (profiles: base, wm-hd) with per-file source-MPQ provenance and failure classification (unknown-chunk vs corrupt vs missing; profile-dependent = override hypothesis, both-profile = parser hypothesis — never auto-blamed); extend parsers for the Northshire→Elwynn route only (terrain+neighbor tile, WMO, route M2s, BLP, DBC set validated against server fmt declarations + samples). WotLK layout deltas determined from samples, never by relaxing version gates. Diagnostic viewer output for the route.

**Blocked by:** 01.

**Status:** loader/sample pass done 2026-10-24; OPEN children: M2 external-skin implementation + version census, Northshire WMO/doodad pinning, neighbor-tile pinning, empirical client load-order verification

- [x] Loader honors owner's .disabled toggles; never renames/deletes user patches
- [x] Route asset dependency list closed (v1) (nothing silently missing)
- [x] Sample-verified M2/skin layout findings recorded (v264, external skins) (embedded vs external)
- [x] Elwynn liquid format established: MH2O only, no MCLQ
- [x] Provenance-tagged resolution demoed (WMBridge = WmOnly) by classifying one deliberately introduced custom-patch asset
