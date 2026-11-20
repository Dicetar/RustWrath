# Player-clippy debt: crates/benilla-app/src/combat_profiles.rs

Status: ready to merge
Owner: Hy4 flash Free
Branch: oc/clippy-combat-profiles
Issue: none
Next: land after CI

## Provenance
The source fix is already committed in 13b47665 ("fix(clippy): clear combat_profiles
player-clippy debt", authored by OpenCode / LongCat 2.5 Free) and is on this branch. I
(Hy4 flash Free) inspected that diff, re-verified every finding against the current file
and the whole repo, and finalized this handoff. I did not rewrite the source.

## What changed (in 13b47665)
- `melee_kind`: folded the `class == 2` match guard into the pattern (`Some((2, subclass))`)
  — clippy `redundant_guards` (old line 70).
- `CombatProfiles::built_for`: new `pub(crate) type ProfileBuildKey` for the
  `(Entity, Option<(u8, u8)>, u32)` tuple — clippy `type_complexity` (old line 239). A type
  alias, not an `#[allow]`.
- `ENEMY_ACTION_BASE`: removed. Repo-wide grep found only its own definition and doc
  comment; no test or other crate references it.
- `SPELL_ACTION_BASE`, `spell_action_id`, `spell_summary`, `enemy_melee_actions`: kept but
  gated `#[cfg(test)]`. Repo-wide greps show each is referenced only by this file's
  `#[cfg(test)]` tests (and, for `SPELL_ACTION_BASE`, by the test-only `spell_action_id`);
  no production crate calls them. The player-clippy gate builds the lib without the test
  target, so they were reported dead; gating removes the lib-build warning while keeping the
  tests and staged helpers. This follows the file's existing `#[cfg(test)]` convention.
- Their exclusive imports (`enemy_action`, `spell_action`, `SpellSummary`,
  `SpellCastTimeCatalog`, `SpellDisplay`, `SpellDurationCatalog`) moved under `#[cfg(test)]`
  so the lib build gains no `unused_imports`.
- No `#[allow]` (crate-wide, module-wide or per-item); no other source file touched.

## Findings: removed vs kept
Removed (finding resolved):
- `ENEMY_ACTION_BASE` (old 35) — deleted; no reference outside its own definition.
- redundant guard (old 70) — guard folded into pattern.
- very complex type (old 239) — `ProfileBuildKey` alias.
Kept as items (finding resolved by `#[cfg(test)]` gating, not deletion):
- `SPELL_ACTION_BASE` (old 37), `spell_action_id` (old 40), `spell_summary` (old 140),
  `enemy_melee_actions` (old 193) — each has a real consumer in this file's tests, so
  deleting them would break tests. They have no production caller yet (the combat-host
  staging slice follows), so they are test-only for now.

## Checks run
- Repo-wide grep of every named item to confirm consumers (above).
- `git diff --check` — clean.
- Inspected the 13b47665 diff against 18b51e97.

## Checks not run
No Cargo, no build, no test suite, no clippy run, no client or server — per task rules.
CI is the build. (An earlier note also claimed `rustfmt --check`; I did not re-run it.)

## Not proven
- The player-clippy gate result and CI status (CI is the build).
- Any in-game behavior. The change is behavior-neutral: a guard moved into a pattern, a type
  alias, and `#[cfg(test)]` gating of functions with no production caller.
