# Combat core integration (Project B, 2026-10-24)

Status: source-observed design plus implemented offline core. There is **no rendered combat,
collision validation, animation retargeting, network carrier, or live dodge proof**. No server
file was modified and no crate was built or executed. Everything below is a proposed integration
seam for owner review, not evidence that the running server already does this.

## 1. Ownership

| Thing | Owner |
|---|---|
| Authored timings, costs, revision | caller/config author, supplied to `benilla-combat-core` |
| Authoritative stamina, eligibility, hit outcome | the server process, not the client and not this crate by itself |
| Client input gesture and prediction | client presentation/input layer; a request only |
| Session/sequence envelope | GLM's native client channel / server adapter |
| Offline lifecycle model | `benilla-combat-core` (`CombatCore`) |
| Live server hit paths | AzerothCore under `D:/WOW/WM_BridgeLab`, read-only here |
| WM observation | WM's perception/context owners (ticket 11), not this core |

The crate is dependency-free and uses integer [`Ticks`]. It does not import Bevy, networking,
serde, a database, or server headers. Baseline 3.3.5 emits no extension traffic and is unaffected
unless an owner wires an adapter.

## 2. API and state ownership

The public surface is re-exported from `crates/benilla-combat-core/src/lib.rs`:

- `CombatConfig` / `DodgeSpec` / `StaminaConfig` / `EnemySpec`: authored, validated data. No
  defaults and no claim that any supplied number is an Elden Ring value.
- `CombatCore::new(config, session, player, start_tick)`: creates the offline authoritative state.
- `CombatCore::submit(InputFrame) -> SubmitOutcome`: validates session, player, sequence, tick,
  revision, and button shape; applies or defers a gesture. It consumes the sequence even when the
  gameplay answer is a refusal.
- `CombatCore::advance_to(Ticks) -> AdvanceReport`: caller-driven tick loop, bounded by
  `MAX_ADVANCE_TICKS = 240`.
- `CombatCore::begin_enemy_telegraph()`: starts one telegraph when ready.
- `CombatCore::snapshot() -> CombatSnapshot` and `reconcile(snapshot)`: state image and
  replacement rules.
- `CombatCore::reset_encounter()` and `reconnect(new_session, player, tick)`: death/reset and
  connection-generation boundaries.
- Views: `PlayerView`, `EnemyView`, `BufferedDodge`, `last_sequence()`, `pending_inputs()`.

`CombatCore` owns all mutable combat state. No presentation type writes it. The callers must not
construct a `PlayerView`/snapshot by hand and call it accepted; only this core produces them.

## 3. Tick model and quantisation

- One tick is a `u64` in [`Ticks`]. There is no wall clock inside the reducer.
- The server calls `advance_to` with its own monotonic tick. A zero-tick advance is a no-op; a
  backwards or over-240-tick advance is an error. Larger catch-ups must be chunked by the caller.
- Config timing fields are tick counts. `TickWindow` is always half-open: `from <= elapsed < to`.
  The dodge's i-frame eligibility and the input-buffer expiry both use half-open boundaries.
- Client timestamps in milliseconds are quantised **up** with
  `TickRate::quantise_after(now, elapsed_millis)` (equivalently
  `ticks_for_millis_ceil`). An adapter must never round down: that would move an input before the
  server tick that observed it.
- `InputFrame.tick` must be at the current tick or in the bounded future
  (`max_input_lead_ticks`). A past tick is rejected. Future frames are deferred in a bounded queue
  (`MAX_PENDING_INPUTS = 16`) and applied on their tick. Accepted input ticks are nondecreasing;
  a frame earlier than an already-queued frame is rejected as out of order.
- An adapter submits all frames for tick `t` **before** `advance_to(t)`, so they are deferred and
  evaluated in that tick. A frame submitted after the core has already advanced to `t` applies at
  `t` without rewinding: it cannot retroactively evade a strike already resolved in that tick.
- `DodgeSpec::input_from_tick` and `dodge_from_tick` preserve the donor's two input thresholds;
  the effective dodge threshold is their maximum. `cancel_dodge_from_tick` is the first offset at
  which a buffered dodge may replace the action. All are caller-authored.

## 4. Sequence, session, and replay

Order of checks in `submit`:

1. session epoch;
2. player identity;
3. per-player sequence (equal = duplicate, lower = stale, gap > `max_sequence_gap` = rejected);
4. action revision;
5. tick not past / not too far ahead;
6. button shape;
7. deferred-queue ordering and capacity.

The sequence is committed only after all envelope checks pass. A later gameplay refusal (dead,
interrupted, zero stamina, input window closed) still consumes it. Rejected envelope frames do not.
`reconnect` requires a strictly newer epoch and nondecreasing time; it clears sequence, deferred
input, the one-slot buffer, and both actors. `reset_encounter` keeps the session, player, clock, and
accepted sequence so a reset cannot replay an old input.

## 5. Predicted versus accepted

- An `InputFrame` is a **request**. `DodgeResult::Started` means this offline authoritative model
  accepted it; it is not a client-side proof of server i-frames.
- Client prediction may read `PlayerView` and present a dodge animation or a predicted stamina
  change. It must not set `CombatSnapshot::player_phase`, consume the server's stamina, or report
  `EnemyHit` as final damage.
- `CombatSnapshot` carries session epoch, player identity, server tick, accepted sequence,
  action revision, HP/stamina, phase, dodge start, interruption deadline, enemy phase/telegraph,
  and whether the active strike already resolved. `reconcile` rejects a different session/player/
  revision, a stale tick, a stale sequence, or fields that do not form the declared phase.
- Reconciliation replaces state and clears the local buffer, deferred queue, and gesture. It does
  not rewind time. A newer session must use `reconnect`, not `reconcile`. Because the gesture edge
  is cleared, an adapter must resend a held state or a fresh press after reconciliation, reset, or
  reconnect.
- The server's `EnemyHit`/damage is authoritative. The crate's `CombatEvent::EnemyHit` is the
  offline model's event, useful for ordering and tests that do not exist yet; it is not a network
  message.

## 6. Event order

For one accepted immediate input, events are returned in `SubmitOutcome.events`:

1. `DodgeEnded` if the previous dodge expired at this tick;
2. `DodgeBuffered` **or** `DodgeStarted` **or** `DodgeRejected`.

For one advanced tick, `AdvanceReport.events` are emitted in this order:

1. `DodgeEnded` when the active dodge expires naturally;
2. deferred inputs due on this tick: `DodgeBuffered` / `DodgeStarted` / `DodgeRejected`;
3. `DodgeBufferExpired`, then a buffered start or rejection when the cancel window opens;
4. enemy telegraph/hit resolution at the end of the tick. If a hit interrupts or kills an active
   dodge, `DodgeEnded` is emitted first, then `EnemyHit`, then `PlayerInterrupted` or `PlayerDied`.

A cancel-into-dodge emits `DodgeEnded` for the replaced action immediately before `DodgeStarted`
for the new one. A buffered request that reaches the cancel window with zero stamina stays in its
one slot until stamina is available or the TTL expires; an immediate release at zero stamina is
rejected with `InsufficientStamina`.

`EnemyTelegraphStarted`, `EncounterReset`, and `Reconnected` are returned directly by their
methods. An interrupting or lethal hit clears the one-slot buffer and the gesture edge, so a button
still held through the hit needs a fresh press before it can dodge. `reset_encounter` and
`reconnect` replace the whole action state, so they report the reset/reconnect event rather than a
separate `DodgeEnded`. A hit does not clear the accepted sequence or the session.

## 7. Lifecycle mapping

| Core phase | Donor evidence | Meaning |
|---|---|---|
| `Ready` | `State::Ground` | no dodge, no interrupt, not dead |
| `Windup` | `State::Act` before `iframes.from` | dodge has started but is not invincible yet |
| `Active` | `invincible()` true | half-open i-frame window |
| `Recovery` | `State::Act` after `iframes.end` | dodge cannot evade, still may not accept a new one |
| `Interrupted` | hurt action / hit reaction | non-lethal hit clears dodge until the authored recovery |
| `Dead` | `State::Dead` | HP zero; dodge, buffer, and deferred effects do not apply |

The enemy maps `Idle/Windup/Strike/Recover/Dead` to
`Ready/Windup/Active/Recovery/Dead`. A strike resolves exactly once, on the first tick in the
active window, and marks `active_resolved`. A snapshot in `Active` with `active_resolved == false`
is rejected, preventing a late duplicate hit after reconciliation.

## 8. Stamina policy

- Eligibility is `stamina > 0`, matching the donor; spending clamps at zero. A partial bar can
  still roll. `DodgeSpec::stamina_cost` must be non-zero and no greater than `StaminaConfig::max`.
- Spending sets a `regen_delay_ticks` deadline. Regeneration starts only once the deadline is
  reached and the player is `Ready`; it adds `regen_per_tick`, clamps to max, and clears the
  deadline at full. This delay is an original Project B policy, not an extracted Elden Ring value.
  The comparator's 1.5 s delay informed the field's existence only.
- Interruption and death stop regeneration because the player is not `Ready`; reset restores full.
- The client's predicted stamina must be replaced by `CombatSnapshot::stamina` on reconciliation.

## 9. Server hit-outcome call graph (read-only source map)

All line references are from the current read-only checkout
`D:/WOW/WM_BridgeLab/src/azerothcore/src/server/game/`. They are source-observed, not patched.

### Melee

- `Unit.cpp:2971-3004`: `AttackerStateUpdate` may replace the melee attack with a melee spell;
  otherwise `CalculateMeleeDamage` builds `CalcDamageInfo`.
- `Unit.cpp:3007-3010`: `DealDamageMods` runs per damage slot **before** the attack packet.
- `Unit.cpp:3012-3016`: sparring can set `HITINFO_FAKE_DAMAGE`; `SendAttackStateUpdate` sends the
  attack state the client uses for the visual outcome.
- `Unit.cpp:3018-3024`: `DealMeleeDamage`, then `ProcSkillsAndAuras` with the resulting
  `DamageInfo`. This is where stock dodge, packet state, procs, and combat log must remain
  consistent.
- `Unit.cpp:2202-2267`: `DealMeleeDamage` applies each live damage slot through
  `Unit::DealDamage` and rewards rage for stock dodge/block/parry at `:2269-2284`.

**Proposed decision point:** after `CalculateMeleeDamage` and before `SendAttackStateUpdate`
(`:3003-3016`), set the stock dodge outcome (`TargetState`/hit-info equivalent) for an eligible
attack, with damage suppressed through that normal outcome rather than by calling `DealDamage`
with zero. Then let `DealMeleeDamage` and `ProcSkillsAndAuras` run. This preserves the packet,
procs, and log. The exact enum fields are an owner/GLM change, not part of this crate.

### Direct spell

- `Spell.cpp:2796`: `CalculateSpellDamageTaken` constructs the direct-spell outcome.
- `Spell.cpp:2813`: `DealDamageMods` is applied before the spell-damage packet/log at `:2839` and
  `DealSpellDamage` at `:2846`.
- `Unit.cpp:1686-1712`: `CalculateSpellDamageTaken` applies script hooks, armor, and spell class
  mitigation.
- `Unit.cpp:1818-1829`: final damage, then `DamageInfo`/`CalcAbsorbResist`.
- `Unit.cpp:1832-1854`: `DealSpellDamage` forwards to `Unit::DealDamage` with `SPELL_DIRECT_DAMAGE`.

**Proposed decision point:** at direct-spell hit-outcome construction, before absorb and before
the damage packet. A dodge must not consume a shield absorb. Do not implement this as a global
`DealDamage` early return.

### Periodic / aura damage

- `SpellAuraEffects.cpp:6275`: `HandlePeriodicDamageAurasTick`.
- `SpellAuraEffects.cpp:6333`: script hook that may modify post-mitigation damage.
- `SpellAuraEffects.cpp:6422-6424`: `DealDamage` with `DOT`, then `ProcSkillsAndAuras`.
- `SpellAuraEffects.cpp:6518-6520`: the second periodic path repeats the same sequence.

**Proposed v1 allowlist:** not dodgeable. No early return is added here. If a later design makes a
specific periodic effect dodgeable, it needs its own source decision and packet/proc consistency;
the current core has no DOT model.

### Split / secondary damage

`Unit.cpp:268`, `:1290-1298`, `:2777-2798`, and `:2850-2871` create or propagate split/secondary
damage through `DealDamageMods`, `DealDamage`, and proc calls. **Proposed v1:** not independently
dodgeable; the parent attack's outcome governs it. No suppression at these sites.

### Shield / absorb

- `Unit.cpp:1171-1179`: `DealDamageMods` zeroes damage for dead/in-flight/evading targets.
- `Unit.cpp:2602-2649`: absorb auras are sorted and applied through `CalcAbsorbResist`, before the
  final `DealDamage`. A dodge decided after absorb would waste or wrongly consume the shield, so
  eligibility must be decided before this stage.

### Environmental

- `Player.cpp:766-800`: environmental immunity, absorb/resist, `DealDamageMods`, and the
  `EnvironmentalDamage` packet.
- `Player.cpp:808-819`: final `DealDamage`, death check, and fall durability loss.
- `Player.cpp:14099`: fall damage calls `EnvironmentalDamage`.

**Proposed v1:** not dodgeable. No early return.

### Irreversible outputs that must stay on the normal path

`Unit::DealDamage` (`Unit.cpp:1181-1541`) is not a pure HP subtraction. It carries:

- aura removal / interrupt flags (`:1222-1234`, with breakable-aura helpers at `:1148-1168`);
- kill and loot recipient logic (`:1399-1431`);
- health mutation (`:1439`), durability (`:1457-1462`), rage (`:1465-1470`);
- cast pushback/interrupt (`:1482-1514`), duel completion (`:1516-1533`);
- the secondary-hit hook (`:1536`) and the returned-damage log (`:1538-1540`).
- spell-specific procs/logging are outside this function (`Spell.cpp:2813-2846`,
  `SpellAuraEffects.cpp:6422-6424`).

Therefore the integration must choose the outcome before these paths, not bypass `DealDamage`
globally. A path that is not on the v1 allowlist must run unchanged.

## 10. Proposed adapter interface GLM needs

No adapter exists. A future native-channel/GLM adapter would need to supply:

1. A monotonic server tick converted from the server's own clock, not the client's.
2. `InputFrame { session, player_guid, sequence, tick, action_revision, dodge }`, with the
   client's press/hold/release observed as edges. A press implies held and a release implies not
   held; one frame cannot contain both. If a physical tap's press and release quantise to the same
   tick, send them as two frames in sequence order; the zero-tick hold is a short tap, not sprint.
3. A caller-authored `CombatConfig`. GLM/owner must supply and label the timing/cost source; the
   core does not invent ER values.
4. An allowlist result for the actual hit path (melee/direct spell only in the v1 proposal).
5. The server's authoritative hit outcome, not a client-reported result. The server adapter must
   decide before absorb/packet/proc construction and must not add a global damage early return.
6. A new session epoch on reconnect; `reconnect` must be called before accepting inputs from the
   new connection.
7. Snapshots to the client, and `reconcile` on the client only when the session/player/revision
   match and the snapshot is not stale.

The core's `EnemyHit` is an offline model event. A server adapter must not serialize it as proof
of damage; the server's own packet/proc/log path remains the evidence.

## 11. Unsupported paths and gaps

- No player attack, parry, block, guard break, poise, jump, sprint locomotion, weapon hit capsule,
  root motion, animation event, or collision. The enemy is the only damage source.
- No network framing, authentication, retransmission, packet ordering, or native-channel carrier.
  Sequence/session checks are model-level only.
- No C++ server dispatcher, no live eligibility query, no stock-dodge enum mapping, and no
  integration into `Unit.cpp`/`Spell.cpp`.
- No action table extraction, no game assets, no generated data. `CombatConfig` is caller data.
- No client predictor type is provided. A presentation layer may read views and implement its own
  prediction, but cannot use them to create authoritative i-frames.
- No tests, builds, runs, benchmarks, or live probes. The core's behaviour is source-reviewed only.

## 12. Future validation matrix (prose only, not created or run)

Before any live claim, an owner-approved matrix would need to cover:

| Case | Expected |
|---|---|
| Dodge released before `sprint_hold_ticks` | buffer/start request; hold at/after threshold is sprint, never a dodge |
| Press before input window | ignored or `InputWindowClosed`, depending on phase; no i-frames |
| Release before cancel window | one buffered slot, newest wins, TTL expiry event |
| Release at/after cancel window | dodge starts, cost clamps, regen delay set |
| Immediate dodge at zero stamina | `InsufficientStamina`, no i-frames, no negative stamina |
| Buffered dodge reaches cancel window at zero stamina | stays buffered until stamina or TTL, then starts or expires |
| Partial stamina | dodge starts, clamps to zero (donor behavior) |
| Hit before i-frame start | interruption, buffer cleared |
| Hit at `iframes.from` | dodged (half-open start) |
| Hit at `iframes.end` | not dodged (half-open end) |
| Lethal hit | `Dead`, dodge/buffer cleared, reset restores |
| Duplicate/stale/out-of-order sequence | rejected without consuming the sequence |
| Future tick beyond lead or queue bound | rejected; no unbounded queue |
| Session/player/revision mismatch | rejected; reconnect path required |
| Snapshot older tick/sequence or inconsistent phase | rejected; no rewind |
| Reconnect with an older epoch | rejected |
| Server PID/binary unchanged across A/B/rollback | belongs to ticket 10, not this core |

Until that matrix is run against the actual server adapter, the honest status is: offline model
implemented; source integration mapped; live dodge **unproven**.
