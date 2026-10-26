# Astra — procedural content system source handoff

Admission base: 5a6eaaa4. Source claim released with this handoff: procgen crate, procgen docs, system work package, this checkpoint and the procgen-only lockfile dependency stanza. OC's concurrent client/bridge work and the old untracked patch helper were preserved. No shared history was rewritten and no broad index commit was made.

## Implemented scope and source audit

This records source inspection, not compiler/test/runtime acceptance. The substantial implementation goal was the owned artifact and encounter-planning pipeline, not authoritative item publication, generated geometry deployment or full client replacement.

| Requirement | Actual source evidence | Review conclusion / limit |
|---|---|---|
| Owned versioned request/output | `artifact.rs`: Candidate, Request, owned items/part definitions/terrain/actors, explicit schemas | Native static references are not persisted; whole-slice default item level retained |
| Lossless numeric capture | Hex64/FloatBits codecs; request conversion; ordered fields and complete part definitions | u64 and finite f32 bits preserved by representation; actual roundtrip unverified |
| Bounded strict import/export | `json.rs`: structural scan, duplicate-key rejection, typed bounded_vec, capped writer; DTO deny_unknown_fields | Field limits precede extra-element decoding; WM report profile is separate |
| Build/catalog support | BuildIdentity validation/exact caller-pin match; full ordered catalog identity/equality; supported static part/archetype restoration | Caller must independently retain/verify source archive and database pins; hashes are not authorization |
| Complete manifest verification | Candidate::restore invokes the existing full v2 manifest after shape checks | Every request/output family maps back to native types; FNV is non-cryptographic; no regeneration/portability proof |
| Defensive imported world validation | `validation.rs`: checked dimensions, room bounds/area/disjoint walkability, reciprocal bounded graph, reachability, safe entrance/boss, terrain and spawn guards | Helpers are called only after their indexing assumptions are established; finite resource limits are not server capacity claims |
| Deterministic encounter planning | `planning.rs`: explicit scope, domain basis vectors, three height rules, exact kind/template/service mappings, spatial spacing and ordered local IDs | No inferred templates, relocation or dropped actors; coordinates are proposals requiring live placement checks |
| Imported-plan/draft verification | EncounterPlan and WmDraft rederive complete bodies from candidate/request/options | Edited derived fields fail equality; fingerprint is independent of generator version |
| Existing WM owner reuse | `wm_draft.rs`: nested wm.world.bundle.v1, creature_spawn/despawn only, template faction and bounded lifetime | No parallel runner, network mutation, ID allocation or permanent spawn action; owner preflight always required |
| Partial application and cleanup | Report prefix/status validation; actual returned ObjectIDs; unknown versus unattempted/preview; reverse cleanup and cleanup-result normalization | Zero/missing/mismatched receipts retain uncertainty; no blind replay, guessed IDs or timer-based retirement |
| Verified receipt persistence | ApplicationReceipt verified encode/decode; CleanupReceipt with retained CleanupEvidence | Saved normalized receipts must match original report reconstruction; reports are caller-supplied evidence |
| Bounded offline entry point | `bin/procgen-offline.rs`: seven commands from generation through cleanup receipt | Regular explicit inputs capped at 64 MiB; create-new output only under explicit workspace config; CLI never invoked |
| Reused dependencies/ownership | Existing serde/serde_json lock packages, procgen-only dependency entry; no OC/WM source edits | No install, new transport, content registry or accepted ADR introduced |
| OC substantial assignment | `../work-packages/2026-10-26-system-goals.md` and prompt delivered in conversation | Observation/action/context/recovery/diagnostic system goal; source/execution boundaries preserved |

Source review traced the current manifest's request, item/parts, dungeon/graph, terrain and spawn fields against the owned DTO restoration. It also inspected the existing WM runner's ordered non-transaction behavior, native placement/faction/lifetime rules, returned identity fields and despawn ownership lookup. Final review tightened capture-before-copy bounds, unsupported parts/mappings, typed sequence limits, tiny spacing arithmetic, report diagnostic budgets and receipt reconstruction. These are inspections and source changes, not executed proofs.

## Cohesive implementation commits

- 523eb4a9 — owned candidates, strict bounded JSON, catalog restoration and world validation.
- b7f51737 — deterministic scoped encounter planning and explicit template mapping.
- 8c523c81 — inert existing-owner WM bundles and receipt-based cleanup drafts.
- e32a82ed — offline CLI, capture guards and partial cleanup-result lifecycle.
- 9eee0a36 — typed collection limits during decoding and current documentation.
- This handoff's commit — separate WM-report bounds, verified receipt codecs and final source audit.

No generator algorithm or canonical v2 manifest was changed in this system implementation. Schema versions for artifacts/plans/drafts/receipts are separate. ADR-0001 remains Proposed; registries were not altered.

## Unverified gates and integration responsibilities

No build, test/new test cases/files, linter/formatter/typecheck, generator/CLI invocation, Python import, receiver/WM/game/server/browser/graphical process, socket, install, DB or deployment operation was performed. Compilation and behavior are UNKNOWN. Prepared verification scenarios are documented in artifact-api.md, encounter-planning.md, wm-drafts.md and offline-cli.md; they are not test results.

Before runtime use: authorized compile/checks must establish Rust/serde acceptance, exact capture roundtrip, negative/upper-bound behavior, deterministic plan derivation, real WM report compatibility and partial/cleanup outcomes. Existing WM owners must independently enforce current player/map/instance/phase/generation, retained template/service/level facts, collision/navigation and execution authority. The native temporary-spawn owner follows the current player's instance/phase; package metadata alone does not enforce them. Do not extract/apply the nested bundle as a substitute for that preflight.

This is a substantial source implementation milestone. It is not production-WM consumption, rendered gameplay, authoritative generated loot, deployed custom geometry or full stock-client replacement. Those remain independent open requirements.
