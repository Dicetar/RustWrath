# Souls action profiles — slice 1 — 2026-11-13

**Published** — merged to `origin/main` at `ffb976ae` (no-ff merge of `feat/souls-action-profiles`,
base `38f56810`); the feature branch is pushed. Seven source slices on the branch: schema and
compilers, owner decisions and the dodge compiler, playback wiring, the predicted clock,
the action engine, the enemy brain harness and the per-rig enemy move lists.

**2026-11-14 — client-authority redesign recorded.** The direction interview (full text:
`2026-11-14-souls-client-authority-direction.md`) replaced the server-authoritative model with
client-decided hits/outcomes and a thin server module; the design record is
`docs/architecture/souls-combat-client-authority.md`, the RFC and the action-profiles plan are
marked superseded, and `owner-decisions.md` carries the 2026-11-14 block.

**2026-11-14 — ZAEBAL fix round (2 AutoSwarm agents + 2 independent auditors).** Engine
defects from the review closed: `advance_to` is step-invariant (per-tick walk, 600-tick span
bound, boundary-stamped events), `input_from_ticks`/`interruptible_until`/`followup` are
honored, `ActionTable` is a `BTreeMap`, and `ActiveClosed` is emitted (and only when opened)
before an early `Ended`. App defects closed: `Duration` playback clock; prediction death via the
`SelfDeath`/health predicate and revival/reanchoring via `new_at(ticks_at())`; interrupt only on
a landed hit (`victim_state == 1`) with full message drain; `Started` events from every path
play their clips; a bounded-advance helper chunks spans and logs errors; profile-compile
failures warn once per rebuild; the provisional keys (F10/V/F9) sit behind the
`UiKeyboardCapture` typing/Super gate. The auditors' P0 (fresh engine at tick zero vs the
app-lifetime clock) and their follow-up (carried stamina missing its regen anchor — a
zero-bar deadlock) are fixed; final gate: auditor #2 CONFIRMED. Verification: core **38/0**,
app `cargo check --tests` clean, `combat_playback::` **4/0**. No client run; no gameplay claim.

Owner direction 2026-11-13: begin the Elden Ring combat module with **automated application of
combat logic to all current combat actions** — every usable ability, every enemy attack and the
animation each plays — instead of hand-authoring one sword profile. This slice implements the
plan's step 1 (plus its automated compilers). Plan of record:
`docs/architecture/souls-combat-action-profiles.md` (new this slice), extending
`docs/architecture/rfc-modular-souls-combat.md`.

Baseline: `main` at `38f56810` (branch `feat/souls-action-profiles`, worktree
`rw-souls-action-profiles`). No client/server process was running; no gameplay was driven.

## Owner decisions, later the same day (decision session)

Recorded in `docs/baseline/owner-decisions.md` (2026-11-13 block) and reflected in the plan:

- **Global toggle** (not the recommended opt-in encounters): Souls combat is a world-wide
  switch the server acknowledges; per-encounter admission stops being the player-facing model.
  This is why the compilers matter: a world-wide mode cannot hand-author its move set.
- New stamina bar; auto-attack disabled in Souls mode; native telegraphs first; **first
  playable milestone is the 1v1 melee loop** (dodge vs one telegraphed enemy, then light-attack
  punish); keyboard+mouse first.

## Slice 1b: the dodge compiles from data too

The 1v1 loop's first button, same compiler approach: `ProfilePolicy` gained
`dodge_iframe_start_ms`/`dodge_iframe_len_ms`/`dodge_stamina` (authored, default 100/300/25),
`dodge_from_clip` compiles the authored [`DodgeSpec`] from the rig's Dodge clip (policy-placed
i-frames, input thresholds at the action's end, typed refusal when the clip cannot host the
window), and `benilla-app` gained `player_dodge_spec` resolving `AnimationData` id 30 on the
rig — `None` for a rig without the clip, no substitute invented (named gap).

## Slice 4: the general action engine (core) and the light-attack bind

`benilla-combat-core` gained `ActionEngine` — the reusable rules surface the RFC's step 1 asks
for, generalized from the single-dodge fixture to any validated `ActionTable`:

- ordered requests against a monotonic tick clock: immediate start when idle, one-slot
  newest-wins buffering with a TTL while busy;
- half-open active windows reported once (`ActiveOpened`/`ActiveClosed`: contact for strikes,
  i-frames for a dodge), completion at the total, replacement in the cancel window;
- the donor's stamina rules: zero bar refuses an immediate request, a partial bar clamps, a
  held request at zero waits for regeneration instead of dropping (the fixture's nuance);
- typed refusals (UnknownAction/InsufficientStamina/Dead), interruption, and death ending the
  action exactly once. 7 focused tests cover the window events, buffering, TTL expiry, the
  zero-bar nuance, interruption/death and the time-regression error.

The app still runs its own playback prototype (same rules, smaller); moving it onto the engine
is the next slice's refactor. `combat_playback.rs` also gained the provisional light-attack bind
(`C`) through the same clock and buffer.

Baseline: `main` at `38f56810` (branch `feat/souls-action-profiles`, worktree
`rw-souls-action-profiles`). No client/server process was running; no gameplay was driven.

## Slice 5: playback on the engine, and the enemy brain harness

- `combat_playback.rs` now runs the core `ActionEngine`; the prototype rules are gone. The app
  keeps only presentation: the ms→tick clock, `phase_of` over the active spec, typed refusals,
  clip playback on `Started`. Provisional binds unchanged (F10/V/C).
- `benilla-combat-core` gained `EnemyBrain`: round-robin moves on an authored cadence that never
  overlaps the strike, telegraph at start, one `ContactClosed`, `StrikeEnded`, poise-break
  staggering, death, and move-list validation (`EnemyStrike` only). This is the RFC's offline
  harness; the server adapter feeds `UNIT_FIELD_BASEATTACKTIME` into the cadence field.
- Evidence: core **28/0** (3 new enemy-brain tests), app `combat_playback::` **3/0**
  (phase helper, engine-backed state, compiled bindings).

## Slice 2: action playback wired (the dodge is visible)

`combat_playback.rs` (new) turns `CombatProfiles` into a live path without touching authority:

- `fill_local_profiles` compiles the local player's table (dodge + main-hand melee) when the
  rig or weapon changes, binding each `ActionId` to the rig's clip;
- `toggle_and_stage_dodge` is the provisional input: **F10** toggles the client's *requested*
  mode (never the effective one), **V** stages one dodge while Experimental is requested
  (inert with no local player; rebindable binds come with the controls slice);
- `play_combat_actions` resolves each request against the table and hands the clip to the
  driver as an `ActionAnim` one-shot.

Driver support: `ActionAnim { entity, anim_id, seq }` joins the existing one-shot intake as
`OneShotReq::Action(u16)` — the same routing, combat fast path, deferral and ordering as a
swing; no new animation logic. The driver's parameter ceiling required the reader to live in
its existing `aux` tuple; the driver test harness registers the new message.

Presentation only: no traffic, no stamina debit, no i-frames, no hit. `CombatModeView::effective`
still never leaves `Baseline`.

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

## Verification state (post-repair; the recorded evidence)

Both suites below ran after the environment repairs in the note below — sccache daemon
restarted, every workspace crate force-rebuilt fresh into the shared
`CARGO_TARGET_DIR=D:\Projects\project-b-client\target`, and the run directory pinned with
`Set-Location` — plus one real fix in the new test code (a `let rig = rig(..)` shadowing in
`player_dodge_spec_profiles_the_rigs_dodge_clip`).

- `cargo test -p benilla-combat-core`: **18 passed, 0 failed** (10 schema/policy, 8 compiler
  including the two dodge tests), 0 doc-tests. sccache wrapper active per checkout config; a
  freshly restarted daemon reports zero requests, so no cache-hit claim.
- `cargo test -p benilla-app --lib combat_profiles`: **6 passed, 0 failed**, 6761 filtered out —
  the five slice-1 tests plus `player_dodge_spec_profiles_the_rigs_dodge_clip`. The remaining
  test-target warnings are pre-existing (dev-panel dead code, `wrath.rs` test arms), none from
  the new module.
- Slice 2 (playback): `cargo check -p benilla-app --tests` clean; `creature_anim::driver::`
  **90/90**; `combat_playback::` **1/1**; `combat_profiles::` **6/6**; `combat_mode::` **7/7**.
  The `ui_chat` failures the wide `combat_` filter also matched are pre-existing 1.12-kit
  drift (`UIParent.lua:329: unexpected symbol near '...'` — the documented donor-install gap),
  reproduced before this slice's changes to the same test kit.
- Earlier compile failures in the round, all diagnosed: re-exporting `pub(super)` items at
  `pub(crate)` (E0364 — the items are now `pub(crate)` at their definition sites); a missing
  deref in a test `ClipEvent` literal (E0308); the `rig` shadowing (E0618); and the three
  environment failures below that produced error output which was never this slice's.
- The earlier shared-target slice-1 "5/5" result is superseded by this 6/6 run. Test counts
  stay within the 20-case task ceiling.
- No client, server, receiver or visible-gameplay evidence exists for this slice. No build of
  `benilla.exe`, no hotpatch claim.

## Environment note (2026-11-13, revalidate before relying on it)

Two independent hazards made this slice's app verification report nonsense:

1. **Shell working directory is not reliable for background runs**: three verification runs
   silently executed in the session's original CWD, `D:\Projects\rustwrath-main-integration`
   — a stale checkout 100 commits behind `main` whose `benilla-app` lib test does not compile
   (E0618/E0004 against its own old `wrath.rs`, match at line 3218 there vs 3268 on current
   `main`). Pin the directory with `Set-Location` inside the command itself and confirm the
   resolved `cargo tree`/`Compiling` paths before believing any output. The stale checkout is
   not this ticket's to repair.
2. **The shared target dir + sccache served stale dependency artifacts**: a correctly
   directory-pinned run in the worktree still failed with 61 "unresolved import" errors
   (`benilla_formats::donor_data_or_skip!`, `benilla_ui::script::WrathTalent*`) — symbols that
   demonstrably exist in the worktree's own sources. The rlibs linked were stale versions,
   consistent with the fix-round handoff's sccache daemon-restart instability. **The slice-1
   "5/5 passed" app result ran through the shared target and is not trusted**; the
   `benilla-combat-core` results (16/16, then 18/18) were always worktree-local and stand.
3. **Root cause found: the C: system drive hit 0 bytes free.** A hermetic cold build in the
   worktree-local `target` on C: exhausted the disk mid-build (os error 112; sccache's server
   also crashed). Disk exhaustion plausibly explains the day's truncated/corrupt artifacts and
   daemon instability across sessions. Countermeasures taken: the incomplete worktree-local
   `target` was deleted (C: back to ~5.7 GB free — still tight; the system drive needs
   attention beyond this ticket), the sccache daemon restarted, and the workspace crates were
   force-rebuilt fresh into the shared `D:\Projects\project-b-client\target` (registry
   dependencies stay warm and are shared with the primary checkout's units). The recorded app
   evidence is the post-repair run in the handoff's verification section.

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