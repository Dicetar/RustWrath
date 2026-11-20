# Clippy quest trace follow-up (dead serde default helpers)

Status: ready to merge
Owner: Big Pickle (opencode/big-pickle)
Branch: oc/clippy-quest-trace-2
Issue: none
Next: land after green CI

## What changed
- `crates/benilla-app/src/quest_trace.rs`: removed the two dead associated functions reported by player-clippy and their inert attribute paths:
  - `EvidenceNote::default_evidence_note_source` and its `#[serde(default = "EvidenceNote::default_evidence_note_source")]` attribute.
  - `Incident::default_opening_note_source`, its `impl Incident` block, and its `#[serde(default = "Incident::default_opening_note_source")]` attribute.
- No behavior change intended. Both structs derive only `Serialize` (not `Deserialize`), so `serde(default = ...)` has no effect on serialized output. The fields are still populated with the same literal values (`"observed"` / `"unavailable"`) at their construction sites.
- No `#[allow]`, no `cfg(test)` gating, no other source file, and no behavior-facing code changed.

## What was checked by source inspection
- Whole-workspace grep (`git grep` plus workspace content search) for both function names found only:
  - the two definitions and their serde attribute paths in `crates/benilla-app/src/quest_trace.rs`;
  - a historical mention in `.scratch/project-b/handoffs/2026-11-22-clippy-quest-trace.md`.
- No test, consumer, or other crate calls either function. There was therefore no caller to gate with `#[cfg(test)]`, and no import became unused.
- The other `#[serde(default)]` attributes on `recovered`, `synthetic`, and `opening_recovered` were kept untouched.

## Checks not run
- No Cargo, no build, no `cargo clippy`, no tests, no client and no server, per task.
- CI result not read in this session; CI is the compile/clippy/test gate for this branch.

## What is not proven
- Local compilation and clippy cleanliness are not proven here; CI must confirm both.
- No runtime or receiver/gameplay verification was performed. The no-behavior-change claim rests on the Serialize-only derive inspection and unchanged field values at construction.
- If a future change adds `Deserialize` to either struct, a default path would need to be reintroduced for missing fields; not needed by the current derive set.
