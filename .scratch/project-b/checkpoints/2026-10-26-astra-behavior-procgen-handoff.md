# RELEASED FILE CLAIM — Astra behavior inventory and procgen handoff

Date: 2026-10-26, Europe/Moscow. Admission HEAD: `c42cbe2d`. Previous goal turn made authoritative progress: the source review, traceability matrix and Proposed ADR were committed; this turn follows their concrete workstream rather than repeating status.

Scope and exact ownership:

- `.scratch/project-b/compatibility/first-loop-behaviors.md`
- `docs/procgen/invariant-audit.md`
- `docs/procgen/artifact-handoff.md`
- `docs/procgen/design.md` (link and status corrections only)
- `crates/benilla-procgen/src/rng.rs` (bounded audit repair if needed)
- This checkpoint.

OC owns all client/protocol/lifecycle/combat files. Its recent `ccb25a5a`/`3755ed88` commits are inspected as source, not accepted runtime evidence. ADR-0001 remains Proposed. No registry changes, new test cases/files, builds/tests/formatters/linters/imports/probes, runtime/graphical/WM/server execution, installs or deployments. WM persistence/content source is read-only and its existing owners remain authoritative. No subagents or external messages.

Deliver the first connected-loop behavior inventory, complete the bounded procgen invariant audit, and produce a concrete durable request/artifact handoff proposal grounded in WM JSON and existing content/bundle owners. Implement only settled local repairs; a new serializer/publisher/schema is not silently accepted. Preserve all unrelated dirty work and the untracked GLM patch script. Use explicit-path commits and release completed claims.

## RNG repair delivery — rng.rs released

Source inspection found `max - min + 1` and the offset addition could overflow i64 in the public inclusive integer helper. Compute span/offset with i128/u128 so the full i64 domain is representable. Keep reversed-bound behavior, one random draw, and modulo mapping unchanged for the bounded ranges used by valid generator requests; no generator-version bump is needed for unchanged valid ProcgenSpec output. The old claim of exact uniformity is corrected to disclose modulo bias. No rejection-sampling or catalog/default changes.

Reviewed arithmetic bounds and the scoped diff manually. No build/test/formatter was run and no new tests were written. Commit contains only rng.rs plus this record; OC's dirty reducer is excluded. rng.rs ownership is released by this source repair commit; the document claim remains active.

## Document delivery — remaining paths released

Delivered in the document commit containing this checkpoint:

- `.scratch/project-b/compatibility/first-loop-behaviors.md`: 56 independently assessable entry/movement/combat/quest-inventory/death/re-entry behaviors, local stable IDs, existing source owners/issues, normal/refusal/transfer/persistence cases, explicit missing command arms, existing check linkage and dependency order. Every acceptance remains UNKNOWN. Source anchor `c42cbe2d`; later in-flight OC work is excluded until delivery review. This is not the exhaustive stock replacement denominator or a new registry.
- `docs/procgen/invariant-audit.md`: bounded path/shape/arithmetic/RNG/manifest audit, the source reasoning for `67f43ab5`, imported-data obligations and quality/placement/persistence limitations. No runtime/portable determinism claim.
- `docs/procgen/artifact-handoff.md`: concrete proposal grounded in existing WM DraftEnvelope, ManagedArtifact/ArtifactRegistry, ordered world bundle and typed action coordinator. Specifies full retained request/output/catalog/build identity, lossless float/u64 representation, bounded validation/refusal, version handling, domain transforms, reserved IDs/native receipts and partial-application/compensation semantics.
- `docs/procgen/design.md`: links and status only; continues to state that no codec/consumer exists.

Source audit requirements are complete for the bounded current generator. The new handoff schema/dependency/API is explicitly Proposed and needs review before a codec/consumer is implemented; no unapproved serializer, registry entry, WM publisher or additional authority was added. Settled source work was the RNG arithmetic repair already committed. All acceptance checks remain gated; no tests/new test files/cases or project execution ran.

Completion evidence for this source-only workstream is the two commits and the named current files, not generator invocations or claimed live use. OC code and the old patch script are preserved. The full replacement inventory beyond the first loop, schema adoption/implementation, connected scenario, compilation and runtime evidence remain future work, not claimed complete by these artifacts.
