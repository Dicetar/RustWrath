# 01 — B-001: complete the baseline identity inventory

**What to build:** finish `docs/baseline/identity-manifest.md` so any engineer can name the correct source/data pairing and what is unproven: deployed binary vs source reconciliation (run/worldserver.exe, module copies/links), DB schema revisions, asset profile enumeration (base vs wm-hd; what the known pair actually loads — mechanism currently unknown, enumerate empirically), locale confirmation, benilla pinned-revision reread (MAP.md, DEVIATIONS.md, Cargo.toml, AGENTS.md at fork base 359eff4).

**Blocked by:** None — can start immediately.

**Status:** done — all checklist items closed 2026-10-24; empirical MPQ load-order verification explicitly carried to ticket 05 (see identity-manifest.md)

- [x] Deployed worldserver.exe reconciled against AC HEAD 5e741c3 (identity or mismatch recorded)
- [x] wm-project ↔ WM_BridgeLab module copy/link map written
- [x] World + Characters DB schema revisions recorded
- [x] Archive enumeration: which MPQs the known-original pair actually opens; patch precedence recorded (empirical, not assumed)
- [x] Locale confirmed (enUS assumed)
- [x] Extension-ID ledger seeded with 0x051b/0x051a reservations and any others found
- [x] Benilla fork-base reread notes filed under docs/baseline/
