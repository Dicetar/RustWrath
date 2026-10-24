# 03 — B-001B-1: live-rule source contract (Spellwright v3 A/B)

**What to build:** the executable recipe for the live-change proof: valid noncombat self/ally A and B graphs (B changes a relation — trigger invoking a different operation family/delivery — not a value), composed only from supported primitives, validated against SpellmakingV3Graph parse/validate; named authoring transport (WM_FORGE3 addon channel); selection tiers (actor-casts-B = zero-code; WM-owned logical binding = named missing implementation); WM publication route decision among candidates (server-side dispatcher entry / DB publication + reload trigger — whether a runtime definition-reload path exists is the open question / bot-client whisper rejected). Deliverable: contract doc + validated graph JSON + explicit missing-adapter list feeding later tickets.

**Blocked by:** 01 (source/tree identity).

**Status:** done 2026-10-24 — docs/live-rules/B-001B-1-contract.md + graph-A/B.json (statically checked; runtime confirmation is ticket 10 step 0)

- [x] A and B graph JSON pass Parse+Validate under purpose=QUOTE rules
- [x] Noncombat-safe during authoring (AtMaker honored, not bypassed)
- [x] WM route chosen or recorded missing with evidence
- [x] Runtime definition-reload path: exists / absent, with source evidence
- [x] Four-point evidence plan (1-2 vs 3-4 kept distinct per GLM response 02 C1)
