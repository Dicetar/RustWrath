# Souls action profiles — slice 1 — 2026-11-13

Owner direction 2026-11-13: begin the Elden Ring combat module with **automated application of
combat logic to all current combat actions** — every usable ability, every enemy attack and the
animation each plays — instead of hand-authoring one sword profile. This slice implements the
plan's step 1 (plus its automated compilers). Plan of record:
`docs/architecture/souls-combat-action-profiles.md` (new this slice), extending
`docs/architecture/rfc-modular-souls-combat.md`.

Baseline: `main` at `38f56810` (branch `feat/souls-action-profiles`, worktree
`rw-souls-action-profiles`). No client/server process was running; no gameplay was driven.

## What landed (source)

**`benilla-combat-core`** (offline, dependency-free, unchanged authority rules):

- `src/action.rs` — the generalized action schema: `ActionId` (documented layout: host
  `1..=999`, enemy `1_000..=999_999`, spell `SPELL_ACTION_BASE + spell id`), `ActionKind`
  (MeleeLight/Heavy, Dodge, Guard reserved, SpellInstant/Cast/Channel, EnemyStrike),
  `ActionSpec` (total, input/cancel offsets, half-open `iframes`/`contact`, stamina,
  `interruptible_until`, follow-up) with per-kind window validation, `ActionTable`
  (revision + tick rate + validated inserts), `dodge_action()` carrying the authored
  `DodgeSpec` over exactly.
- `src/policy.rs` — `ProfilePolicy`: the one authored tuning set (per-mille fractions and
  ms windows, integer math only) with bounds validation; every value labelled authored,
  never an extracted Elden Ring number.
- `src/profile.rs` — the pure compilers: `melee_action` (contact opens at the clip's first
  attack-hit key, else policy fractions; commits at contact; input heard from contact end),
  `spell_action` (cast→windup, release window at cast end, channel sustain, interruptibility
  from `InterruptFlags`; GCD/power stay with their stock owners), `enemy_action`
  (telegraph=windup to the contact key, hyperarmoured, no cadence — that is the FSM slice).
- `src/lib.rs` exports; `src/error.rs` + `DuplicateAction`/`MissingFollowup` variants.

**`benilla-app`**:

- `src/combat_profiles.rs` (new) — the client-side extraction: `clip_summary` (clip length +
  first `$AH0..$AH3`/`$CAH` key — the impact router's own tag set), `melee_kind` (the swing
  LUT's 2H families are heavy), `player_melee_action` (weapon LUT + `ModelAnimations::resolve`
  fallback chain + first clip variation; a rig that falls back to a non-swing clip yields
  `None`, never a Stand-as-swing), `spell_summary` (level-solved cast time; channel only when
  `ChannelInterruptFlags` set and the duration row is bounded), `spell_action_id`,
  `CombatProfiles` (the typed hand-off resource for slices 2–3, unwired).
- `src/lib.rs` `mod combat_profiles;`; `Cargo.toml` gains `benilla-combat-core`.
- `src/creature_anim.rs` re-exports `swing_anim_main`, `is_swing_id`, `impact_tag` as
  `pub(crate)` (single source of truth for the LUT and contact tags); `impact.rs`
  `impact_tag` to `pub(super)`.

No behavior change: no system reads or writes the new module yet. The combat seam
(`combat_mode.rs`) is untouched.

## Verification state

- `cargo test -p benilla-combat-core` (worktree-local target): **16 passed, 0 failed**
  (10 schema/policy, 6 compiler), plus 0 doc-tests. sccache wrapper active per checkout
  config; no cache-hit claim (stats not read after daemon restarts).
- `cargo test -p benilla-app --lib combat_profiles` (shared target
  `CARGO_TARGET_DIR=D:\Projects\project-b-client\target`, the sanctioned worktree-build
  route): **5 passed, 0 failed**, 6761 filtered out. Compile finished in 2m45s; the 37
  test-target warnings are pre-existing (dev-panel dead code, `wrath.rs` test arms), none
  from the new module.
- Two transient compile failures were fixed during the round: re-exporting `pub(super)`
  items at `pub(crate)` (E0364 — the items are now `pub(crate)` at their definition sites)
  and a missing deref in a test `ClipEvent` literal (E0308).
- No client, server, receiver or visible-gameplay evidence exists for this slice. No build of
  `benilla.exe`, no hotpatch claim. Test-run counts stay within the 20-case task ceiling.

## Named gaps (do not re-dig, do not treat as done)

- Slices 2–3 are unwired: no driver action-instance playback, no `combat_mode` staging change.
  `CombatProfiles` has no reader yet.
- Guard/parry reserved; enemy poise/stagger and cadence (`UNIT_FIELD_BASEATTACKTIME`) are
  slice 4 (FSM), as is per-encounter enemy id assignment.
- Spell delivery policy (projectile speed, radius, targeting) belongs to the RFC's step-4
  spell bindings, not this timing compiler.
- Clip variations: the profile uses the first variation; the driver rolls among weighted ones.
- Channel duration profiles at `SpellDuration` base value only; per-level scaling unmodelled.
- Server authority seam (deterministic dodge outcome, receipts/snapshots) is untouched — the
  `NotSent` boundary in `combat_mode.rs` is unchanged and still honest.

## Next concrete steps

1. Land the app check (in flight), `git diff --check`, commit on `feat/souls-action-profiles`.
2. Slice 2: driver action-instance request keyed by `ActionId` → resolved clip + phase clock
   through the existing one-shot path; contact markers vs the spec's contact window.
3. Slice 3: `combat_mode` staging on `ActionId`, table revision pinned per session, still `NotSent`.
4. Slice 4: offline enemy FSM harness over compiled enemy profiles; cadence from attack-time data.
5. Server authority work per the RFC requires owner coordination — a planned maintenance
   deployment if the installed module API cannot express the deterministic dodge outcome.