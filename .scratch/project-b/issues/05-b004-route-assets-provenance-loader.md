# 05 — B-004: first-route assets + provenance loader

**What to build:** manifest-defined patch-chain loader (profiles: base, wm-hd) with per-file source-MPQ provenance and failure classification (unknown-chunk vs corrupt vs missing; profile-dependent = override hypothesis, both-profile = parser hypothesis — never auto-blamed); extend parsers for the Northshire→Elwynn route only (terrain+neighbor tile, WMO, route M2s, BLP, DBC set validated against server fmt declarations + samples). WotLK layout deltas determined from samples, never by relaxing version gates. Diagnostic viewer output for the route.

**Blocked by:** 01.

**Status:** ready-for-agent

- [ ] Loader honors owner's .disabled toggles; never renames/deletes user patches
- [ ] Route asset dependency list closed (nothing silently missing)
- [ ] Sample-verified M2/skin layout findings recorded (embedded vs external)
- [ ] Elwynn liquid format established from samples
- [ ] Provenance-tagged resolution demoed by classifying one deliberately introduced custom-patch asset
