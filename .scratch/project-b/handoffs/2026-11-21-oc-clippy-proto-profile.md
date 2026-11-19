# Player-clippy debt: crates/benilla-assets/src/profile_source.rs

Status: ready to merge
Owner: OpenCode / MiMo-V2.6-Flash Free
Branch: oc/clippy-proto-profile
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Findings (player-clippy, CI run 44556288487): dead `read_profile_sync` (~line 321);
`explicit_auto_deref` on `validate_input(&active, &candidate, *path)` (~line 398).
Fix: removed `read_profile_sync` and its doc comment (only hit in `crates/` was the
definition); dropped the `*` so the call passes `path`. Touched that one file only,
no `#[allow(...)]`, no other change.

CI: not run yet

no behavior change; no new test; the existing profile_source stage/commit tests are the check.
