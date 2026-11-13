# Automated Souls action profiles — implementation plan

**Status** — In implementation since 2026-11-13. Extends [the modular Souls combat RFC](rfc-modular-souls-combat.md)
(step 1: "extract reusable action rules and a versioned profile") with the owner's direction of
2026-11-13: the combat logic must apply **automatically to all current combat actions** — every
usable ability, every enemy attack, and the animations each of them plays — instead of one
hand-authored sword profile. Source baseline `38f56810`. Slice 1 is offline source plus focused
tests; no runtime, receiver or gameplay claim follows from it.

## What the client already has (extraction sources)

Nothing below is new; the compilers only join existing seams.

| Fact | Source | Notes |
| --- | --- | --- |
| Weapon → swing clip id | [`swing_anim_main`](../../crates/benilla-app/src/creature_anim/select.rs) (subclass → 17/18/19/85/16), `swing_anim_off` (88/87/117) | Reference-verified LUTs the driver already use |
| Contact timing inside a clip | `AnimClip.events` ([`ClipEvent`](../../crates/benilla-assets/src/model/anims.rs): `time`, `ident`) with the attack-hit tags `$AH0..$AH3`/`$CAH` — the same set [`impact_tag`](../../crates/benilla-app/src/creature_anim/impact.rs) routes victim feedback on | Static, timestamped, per clip |
| Clip length | `AnimClip.duration`, `M2SequenceInfo.duration_ms` | Per resolved rig clip |
| Clip resolution & fallback | `ModelAnimations::resolve` + `AnimationData.dbc` fallback chain ([`AnimDataCatalog`](../../crates/benilla-formats/src/anim_data.rs)) | A model lacking Attack2H falls back to Attack1H → AttackUnarmed |
| Spell windup | `SpellDisplay::casting_time_index` → [`SpellCastTimeCatalog::resolved_ms`](../../crates/benilla-formats/src/spells/cast_times.rs) | Level-scaled, row 1 = instant |
| Spell channel/interruption | `SpellDisplay::channel_interrupt_flags`, `duration_index` → `SpellDurationCatalog`, `interrupt_flags` | The app already loads both catalogs in `ui_action::Spells` |
| Dodge rules | [`DodgeSpec`](../../crates/benilla-combat-core/src/data.rs) + the integer-tick core | The authored source of truth the schema must carry over exactly |
| Staging boundary | [`combat_mode.rs`](../../crates/benilla-app/src/combat_mode.rs) | `NotSent` until a server channel exists; unchanged by this plan |

## Design

1. **One action schema for everything.** `ActionSpec` (in `benilla-combat-core`) generalizes
   `DodgeSpec`/`EnemySpec`: `total_ticks`, `input_from`, `cancel_from`, half-open `iframes` and
   `contact` windows, `stamina_cost`, `interruptible_until`, `followup`. All integer ticks at the
   table's `TickRate`; all windows half-open, matching the donor's `from <= frame < to`. Every
   spec and table validates. `ActionId` layout: `0` invalid; `1..=999` host actions (dodge);
   `1_000..=999_999` encounter-assigned enemy actions; `1_000_000+` spell actions offset by the
   `Spell.dbc` id.
2. **Compile, don't hand-author.** Pure compilers (`melee_action`, `spell_action`, `enemy_action`)
   turn plain summaries into validated specs:
   - **Melee**: weapon subclass → light/heavy kind and swing clip (existing LUTs); clip duration →
     total; the first `$AH*`/`$CAH` key → contact window start; without a key, per-mille policy
     fractions. Input is heard from the end of contact; the swing commits at contact
     (`interruptible_until = contact.from`).
   - **Spells**: cast time → windup; channeled flags + duration → sustain; `interrupt_flags` →
     interruptible-until-release; the stock power cost and GCD stay with their existing owners —
     the action clock is windup + release window only.
   - **Enemies**: the creature's own attack clip → telegraph (windup = contact start), contact,
     recovery. Enemy cadence (`UNIT_FIELD_BASEATTACKTIME`) is not part of the profile; it enters
     with the enemy-FSM slice.
   Tuning lives in one authored `ProfilePolicy` (per-mille integers, no floats in tick math);
   every value is a labelled Project B policy, never an extracted Elden Ring number, and can be
   overridden per revision once WM publishes revisions per the RFC.
3. **Authority unchanged.** Profiles are vocabulary and presentation timing. Stamina, damage,
   hit outcomes and mode negotiation remain server-owned per the RFC and `combat_mode.rs`; no
   compiler output is a command, and nothing here sends traffic.

## Owner decisions, 2026-11-13

Recorded in [owner decisions](../baseline/owner-decisions.md); they shape this plan:

- **Global toggle**: the mode is a world-wide switch the server acknowledges before any rule
  applies — not a player-facing per-encounter choice. This validates the automated-compilers
  approach: a world-wide mode cannot hand-author its move set, so every action must compile
  from data. Encounter-level admission remains only as a technical staging detail (the first
  telegraphed enemy is still one creature) while the mode itself is global.
- **A new stamina bar** powers dodge, guard and melee; spells keep their own power. The
  bar is server-owned once the authority seam exists; until then the client may only show
  prediction, visibly marked.
- **Auto-attack is disabled** while the mode is on: every swing is deliberate input.
- **Native telegraphs first**: the enemy's own attack clips carry the windup; VFX or authored
  tells are added only where a clip proves unreadable.
- **First playable milestone: the 1v1 melee loop** — dodge versus one telegraphed enemy, then
  a light-attack punish.
- **Keyboard and mouse first**, with rebindable experimental binds; gamepad later.

## Nuances checklist (living status)

Legend: **done** = source plus focused tests (not runtime/gameplay evidence); **pending** = named
gap with its owner or blocker. Server-seam items cannot be completed client-side.

| Nuance | State | Where / what remains |
| --- | --- | --- |
| One action schema for every action | done | `benilla-combat-core` `action.rs` |
| Automated melee/spell/enemy profiling | done | `profile.rs`, `combat_profiles.rs` |
| Dodge profile compiled from the rig clip | done | `dodge_from_clip`, `player_dodge_spec` |
| Action playback through the animation driver | done | `ActionAnim`/`OneShotReq::Action` |
| Predicted action clock (windup/active/recovery/finished) | done | `combat_playback.rs` `PredictedAction` |
| One-slot buffering, newest wins, TTL, cancel window | done | core `ActionEngine`; playback prototype |
| General action engine (multi-action, interruption, death) | done (core) | `ActionEngine`; app playback not yet refactored onto it |
| Stamina: spend/clamp/regen-delay, zero-bar eligibility | done (predicted) | `PredictedStamina`; server value pending the seam |
| Light attack | provisional bind (`C`) | rebindable controls slice |
| Heavy/charge attacks | profiling done; charge semantics pending | policy + input slice |
| Combos (`followup`) | schema supports; no authored chains | policy |
| Guard/block | kind reserved; no compiler | guard slice |
| Parry/riposte | not started | needs a server-owned riposte window |
| Poise/stagger | not started | engine hooks + server |
| Enemy telegraph FSM, native clips first | enemy profiles compile; FSM pending | core FSM harness + server adapter |
| Enemy cadence (base attack time) | not started | FSM slice |
| Auto-attack disabled in Souls mode | pending | server gate |
| Global toggle negotiation with receipts/snapshots | pending | RFC steps 2/5, owner-coordinated |
| Authoritative dodge/i-frames (hit outcome) | pending | server `MELEE_HIT_DODGE` seam |
| Spells through the action clock | profiling done; bindings pending | spell-binding slice |
| Projectile pilot (interrupt, release, cooldown) | pending | spell binding + server |
| Lock-on | not started | camera/target slice |
| Sprint (hold versus dodge) | not started | input slice + movement owner |
| Roll displacement / dodge movement | not started | movement controller + server validation |
| Authored animations (roll, swings, telegraphs) | not started | Blender pack per the RFC |
| Rebindable experimental controls | provisional keys only | controls slice |
| UI: stamina bar and mode state | pending | UI host slice |
| WM revision publication | pending | RFC step 6 |

## Slice roadmap

| Slice | Deliverable | Consumer / acceptance |
| --- | --- | --- |
| 1 (this) | `ActionSpec`/`ActionTable`/`ProfilePolicy` + pure compilers in `benilla-combat-core`; app-side extraction (`ClipSummary` from `AnimClip`, melee kit from the main-hand weapon, spell summaries from `SpellDisplay` + cast/duration tables); focused tests | The action-instance playback (slice 2) and the combat host staging (slice 3). Verified by `cargo test -p benilla-combat-core` and `cargo check -p benilla-app` only |
| 2 | Driver action-instance requests keyed by `ActionId`: resolved clip + phase clock through the existing one-shot path | Local prediction plays the compiled profile; contact markers line up with the spec's contact window |
| 3 | `combat_mode` staging carries `ActionId` (replacing the coarse `CombatWindowKind` vocabulary), still `NotSent` | Typed refusals unchanged; table revision pinned per session |
| 4 | Enemy FSM: offline demo harness over compiled enemy profiles; then the server `CreatureAI`/SmartAI adapter per the RFC | Telegraph → commit → strike → recovery loop observable offline before any server work |
| 5 | Server authority seam per the RFC (deterministic `MELEE_HIT_DODGE` at outcome construction, receipts/snapshots) | Needs owner coordination; a planned maintenance deployment if the installed module API cannot express it |
| 6 | Revision publication through WM per the RFC content contract | Stage and roll back a changed action profile at a fresh encounter |

## Gaps named for slice 1

- `Guard` is a reserved kind; no compiler emits it and no parry exists yet.
- Enemy attack cadence, poise/stagger, and per-encounter assignment enter with slices 4–5.
- Spell delivery policy (projectile speed, radius, target policy) enters with the spell-binding
  work (RFC step 4), not the timing compiler.
- Clip variations: a model may carry several weighted variations of one swing; the compiler
  profiles the first, while the driver rolls among them. Per-variation profiles are a documented
  refinement.
- Channel duration uses the `SpellDuration` row resolution the tooltip tokens path uses;
  level-scaling at runtime may shorten real channels.
- No client, server or visible-gameplay evidence exists for any slice; slice 1 is source and
  focused tests only.

## Slice 1 verification

- `cargo test -p benilla-combat-core` — the crate suite including the new schema/compiler tests.
- `cargo check -p benilla-app` — the extraction module compiles against the real types.
- Both through sccache (the checkout config); no cache-hit claim without `sccache --show-stats`.
- Nothing else is run; no build, client, server or combat claim is made.
