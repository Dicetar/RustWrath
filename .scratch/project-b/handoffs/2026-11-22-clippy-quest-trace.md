Status: ready to merge
Owner: opencode/iceman-free
Branch: oc/clippy-quest-trace
Issue: none
Next: land after CI

## What changed
- `crates/benilla-app/src/quest_trace.rs`: fixed the reported clippy findings in this file only (no behavior changes, no other source files, no crate/module-wide allow).

## Findings removed
- Dead/never-used items removed after repo-wide reference checks:
  - `DomainEvent::Roster` variant and its only match arm (never constructed; `Roster` references elsewhere are `glue_session::roster::Roster` / other types, not this enum variant).
  - `UiDispatchToken` fields `consumed`, `clipped`, `handler_invoked` (never read; token construction updated in `record_ui_press`/`record_ui_click`, call signatures preserved for existing callers).
  - `Source::update_packets_sampled_out()` getter (never called; the `update_packets_sampled_out` atomic counter remains and is still incremented/read).
  - `default_evidence_note_source` / `default_opening_note_source` free functions replaced with associated fns referenced by serde defaults (`EvidenceNote::default_evidence_note_source`, `Incident::default_opening_note_source`) so the serde paths remain valid without dead functions.
  - `IncidentOutcome::JournalMismatch` variant (never constructed).
  - Unused convenience methods `delivery_counts`, `one_second_movement_pressure`, `movement_pressure_events`, `movement_corrections`, `audio_output`, `asset_window` (no call sites; underlying `*_attribution`/`*_summary`/`movement_pressure` helpers remain in use).
- Mechanical/idiomatic rewrites:
  - Removed explicit auto-derefs in `same_physical_click(*t, release)` and `cast_attribution(*event, true)`.
  - Introduced type aliases to reduce type complexity: `RetainedTransfer`, `RetainedReload` for `RetainedSessionState`, and `MovementPollSnapshot`, `AudioPollSnapshot` for poll locals.
  - Simplified `map_or(true, ...)` checks to `is_none_or(...)` in eviction/session-state retention paths.
  - Collapsed `CastEdge::Requested` nested if into a guarded match arm.
  - Iterated map values directly (`.values()`) where keys were unused in the reported sites (panel/cast/slot/turn-in/session/diagnostic/loot-vendor correlation paths).
  - Replaced `if vector == "hotpatch" { cfg!(feature = "hotpatch") } else { true }` with a boolean expression.

## Findings kept and why
- None of the reported findings were intentionally kept unfixed in this file. `Stage::UiClick` still carries `consumed`/`visible`/`clipped`/`handler_invoked` and `record_ui_press`/`record_ui_click` still accept/propagate those values because they are part of the recorded event/call contract used by other modules; only the unread `UiDispatchToken` copies were removed.
- `movement_corrections` remains as inspector/export/snapshot field and data populated via `movement_correction_attribution()`; only the unused alias method was removed.

## Checks not run
- No Cargo, no build, no tests, no client/server (per task). CI is the build/gate.

## Not proven
- No local compilation or clippy confirmation of this file/crate; CI must validate the fixes and that no dependent code relied on the removed dead items/APIs.
- No runtime/behavioral verification; changes are intended to be lint-only refactors/removals of unused code.
