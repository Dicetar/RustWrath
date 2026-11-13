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
