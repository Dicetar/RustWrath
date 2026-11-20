# Player-clippy debt: crates/benilla-app/src/combat_profiles.rs

Status: ready to merge
Owner: OpenCode / LongCat 2.5 Free
Branch: oc/clippy-combat-profiles
Issue: none
Next: land after CI

## What changed
- `melee_kind`: folded the `class == 2` match guard into the pattern (`Some((2, subclass))`) —
  clippy `redundant_guards` (old line 70).
- `CombatProfiles::built_for`: new `pub(crate) type ProfileBuildKey` for the
  `(Entity, Option<(u8, u8)>, u32)` tuple — clippy `type_complexity` (old line 239).
- `ENEMY_ACTION_BASE`: removed. Repo-wide grep found only its own definition and doc comment;
  no test or other crate references it.
- `SPELL_ACTION_BASE`, `spell_action_id`, `spell_summary`, `enemy_melee_actions`: kept and
  gated `#[cfg(test)]`. Each is referenced only by this file's `#[cfg(test)]` tests (and, for
  `SPELL_ACTION_BASE`, by the test-only `spell_action_id`). The player-clippy gate
  (`cargo clippy -p benilla -p benilla-app --no-default-features -- -D warnings`) builds the lib
  without the test target, so they were reported dead. `#[cfg(test)]` is the file's existing
  convention (the `spell_action` import was already gated; the proto-header ticket used the same
  fix), and it keeps the tests and the staged helpers.
- Their exclusive imports (`enemy_action`, `SpellSummary`, `SpellCastTimeCatalog`,
  `SpellDisplay`, `SpellDurationCatalog`) moved under `#[cfg(test)]` so the lib build does not
  gain `unused_imports`.
- No `#[allow]`; no other source file touched.

## Findings: removed vs kept
Removed: all seven. `ENEMY_ACTION_BASE` deleted; the four test-only items now compiled only for
tests; `redundant_guards` and `type_complexity` rewritten.
Kept (item, not finding): `spell_summary`, `spell_action_id`, `enemy_melee_actions` and
`SPELL_ACTION_BASE` are kept because tests reference them; they have no production caller yet
(the combat-host staging slice follows), so they are test-only for now rather than deleted.

## Checks not run
No Cargo, no build, no test, no client or server. `git diff --check` clean;
`rustfmt --check --edition 2021` clean on the file. CI compiles and lints.

## Not proven
- The player-clippy gate result (CI is the build).
- Any in-game behavior: the change is behavior-neutral (a guard moved into a pattern, a type
  alias, and test-only gating of functions with no production caller).
