# Combat donor assessment (Project B, 2026-10-24)

Status: source-observed assessment. No donor code was executed, generated, built, or vendored.
This is the checkpoint-A decision record for `PROJECT_B_OPENCODE_TASK_02.md`.

## Pinned sources

| Role | Source | Revision | Licence | Evidence |
|---|---|---|---|---|
| Primary donor | `https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite` | `177a3b5ec14a9bde39b9ab458a3400b4038224e3` | MIT (`LICENSE.md`, Copyright 2026 Funny-Bones and contributors) | Read from the local read-only checkout `.opencode/references/elden-ring-combat-rewrite`; commit date 2026-10-09 |
| Comparator | `https://github.com/TeaYololo/soulslike` | `5824f4b885ff77fd024171b2b8d7aed198de7ce0` | Apache-2.0 (verified against the pinned `LICENSE`) | Remote source/readme and pinned licence fetched; implementation facts are from the files named below |

The donor is a Rust/Bevy 0.19.1 sandbox, not a library. Project B pins Bevy 0.18.1 and will not
upgrade to match it. The donor's README states that it contains code only: action tables, baked
animations, sounds, and the game's assets are absent and must be generated from an owner's own
Elden Ring installation. Nothing generated from the game is copied or claimed as available here.

## Primary donor: what was inspected

Exact paths read in the local checkout:

- `README.md`, `LICENSE.md`, `Cargo.toml`, `build.rs`
- `src/sim/mod.rs`
- `src/sim/data.rs`
- `src/sim/player.rs`
- `src/sim/dummy.rs`
- `src/sim/level.rs`
- `src/sim/tests.rs` (behaviour examples only; not executed)
- `tools/extract.py` (grepped for generated-table coupling; not executed)

No `src/sim/extracted.rs` exists in the checkout. The crate therefore cannot build as provided
without owner-generated data. That is a deliberate upstream choice, not a defect to patch here.

## Primary donor: mechanisms actually observed

### Fixed tick and animation quantisation

- `src/sim/data.rs:21-25` fixes the simulation at 60 Hz (`TICK_HZ = 60.0`, `DT = 1/60`) and
  advances animation frames at 30 fps (`ANIM_FPS = 30.0`, `DF = ANIM_FPS * DT`).
- Action timings, hit windows, i-frames and cancel windows are authored in animation frames and
  compared against `Act.f`; one tick advances half a frame.
- This is a sound mechanism to port as integer ticks. Project B must not carry `f32` frame clocks
  or a wall clock into the reducer.

### Dodge-on-release, hold-to-sprint

- `src/sim/player.rs:457-499` reads the dodge button as a gesture: a press arms it, hold time is
  accumulated, and a release produces the dodge only when the hold is shorter than
  `SPRINT_HOLD_FRAMES`. Longer holds are sprint input and release produces no dodge.
- `src/sim/data.rs:61-63` defines `SPRINT_HOLD_FRAMES = 10.0` and explicitly marks it an
  `ESTIMATE`, not an extracted Elden Ring value. The README repeats that the sprint-hold duration
  is estimated.
- `src/sim/player.rs:509-511` and `:579-591` define sprint as held dodge plus movement, with a
  stamina drain and a `sprint_spent` latch. Project B's first core only needs the gesture
  distinction; sprint locomotion and drain belong to a movement owner, not this one-dodge model.

### One-slot input buffer, newest wins

- `src/sim/player.rs:200`, `:484-498` keep exactly one `Option<Req>`; every newer accepted input
  replaces it. There is no queue and no priority ordering.
- Inputs are only heard when the current action has reached `input_from`; dodge additionally uses
  `input_dodge_from` (`src/sim/player.rs:457-466`).
- Buffered input is attempted only when the current action reaches the relevant cancel window
  (`src/sim/player.rs:711-742`). The donor has no explicit TTL: the slot survives until consumed or
  replaced. Project B must add an explicit TTL because the task requires an expiration contract;
  that is an original safety decision, not an Elden Ring fact.

### Action timing and i-frames

- `src/sim/data.rs:383-419` defines `ActionDef`: total length, input thresholds, per-input cancel
  thresholds, an i-frame window, hit windows, stamina, and root motion.
- `src/sim/data.rs:353-370` defines hit windows as `from <= frame < to` and `iframes` likewise.
  The half-open convention is directly portable.
- `src/sim/player.rs:312-321` computes invincibility from the active action's i-frame window; a
  dead player is also treated as invincible in the donor's local model.
- `src/sim/player.rs:936-943` starts a roll or backstep; `start_cost()` in
  `src/sim/data.rs:488-497` charges the roll/backstep cost at action start.

### Stamina and resource accounting

- `src/sim/data.rs:31-45` separates extracted max stamina from estimated regeneration
  (`STAMINA_REGEN = 45.0`) and extracted roll/backstep/jump costs
  (`ROLL_COST = 12.0`, `BACKSTEP_COST = 8.0`, `JUMP_COST = 10.0`).
- `src/sim/player.rs:513-544` refuses to start a buffered action when stamina is zero; otherwise
  it starts the action.
- `src/sim/player.rs:919-925` spends with a clamp to zero (`(stamina - cost).max(0.0)`). It does
  not require the full cost. A partially filled bar can therefore still produce a roll and ends at
  zero. Project B preserves that behaviour and documents it.
- `src/sim/player.rs:441-454` regenerates only when the state machine reports that regeneration is
  allowed, with a guard-stance multiplier. The donor has no post-spend regen delay; the comparator
  does. Project B adds an authored delay because the task requires one, and labels it an original
  policy integration.

### Telegraph, hit resolution, interruption, death, reset

- `src/sim/dummy.rs:124-202` is a four-state enemy cycle: `Idle -> Windup -> Strike -> Recover`,
  with stagger and dead states. The strike resolves on the first tick of the `Strike` state and
  sets `connected` so one swing cannot hit twice.
- `src/sim/mod.rs:107-130` resolves the player's response to a live enemy hit, logging dodged,
  blocked, hit, and killed outcomes.
- `src/sim/player.rs:1029-1080` checks i-frames before damage, applies damage, and transitions to a
  hurt action or death. `src/sim/player.rs:1082-1091` clears buffered/air/swap state on death.
- `src/sim/dummy.rs:99-104` drops the enemy's current attack when the player dies. Project B models
  this as an explicit reset/reconnect boundary rather than an ECS side effect.

### Data coupling and dependencies

- `src/sim/data.rs:15-19` and `src/sim/player.rs:478-485` obtain all real action data from
  generated `extracted.rs`; missing data panics.
- `src/sim/mod.rs:6` and `src/sim/player.rs:7` still import Bevy math (`Vec3`, `Vec2`). The
  simulation is engine-light but not engine-free.
- `Cargo.toml` depends on `bevy = "0.19.1"`; there is no networking, database, or server code.
- Hit detection uses weapon capsules and root-motion samples
  (`src/sim/mod.rs:38-50`, `:158-184`) and is not portable without the missing animation data.

## Comparator: what it adds and what it warns about

The comparator is Unity 2022.3/C# with FishNet, not a Rust component. It was read only as a
cross-check. Relevant files named by the inspected tree include
`Assets/SoulslikeCombat/Scripts/Combat/StateMachine/States/DodgingState.cs` and
`Assets/SoulslikeCombat/Scripts/Player/Combat/PlayerStaminaSystem.cs`.

Observed/reported mechanisms:

- A dodge state with an i-frame start and duration, and stamina management.
- A stamina regeneration delay (the source uses a 1.5 s delay and a 25-point dodge cost in its
  sample tuning). These are comparator values, not Elden Ring values and not Project B defaults.
- The README itself reports a **stamina race condition** that can drive stamina negative in
  multiplayer, and **dodge i-frame timing** that is unreliable under high latency. Its network
  state is described as server-authoritative but not perfectly reconciled.

Decision: do not port code, tuning, or trust assumptions from this comparator. Its known defects
argue for the opposite of a client-owned stamina/eligibility model. Only the state vocabulary
(dodge i-frames, regen delay) informed the authored configuration fields.

## Reuse / port / avoid decision

| Subsystem | Decision | Reason |
|---|---|---|
| Fixed-step simulation | **Port the mechanism** | 60 Hz tick and explicit tick deltas are directly useful; re-express in integer ticks |
| Dodge-on-release vs hold-to-sprint | **Port the mechanism** | Explicitly observed in primary donor; keep the `SPRINT_HOLD_FRAMES` threshold caller-supplied and marked as tuning, not extracted data |
| One-slot newest-wins buffer | **Port the mechanism, add TTL** | Donor has a real one-slot buffer; its no-TTL lifetime is unsafe for a network seam |
| Input / cancel / i-frame windows | **Port the mechanism** | Observed half-open windows and per-input cancel thresholds; replace frame floats with tick offsets |
| Action revision reference | **Port the idea** | The donor's generated table is fixed at build time; Project B needs an explicit revision so a stale intent cannot resolve against changed authored data |
| Stamina cost, clamp, regen | **Port spend/clamp; author delay** | Donor cost/clamp is source-observed; regen delay is original integration policy and must be labelled |
| Enemy telegraph and single hit | **Port the shape** | `Windup -> Active -> Recovery`, one resolved hit per swing, interruption/death/reset |
| Bevy math, ECS, rendering, audio, input devices | **Avoid** | Not portable; Project B owns presentation and engine integration separately |
| Generated `extracted.rs`, animations, assets, reaction tables | **Avoid** | Game-derived, absent, and not licensed for redistribution; caller supplies validated authored data |
| Weapon capsule/root-motion hit detection | **Avoid for v1** | Requires missing animation data and collision validation; out of scope for the offline core |
| Comparator code/tuning/network trust | **Avoid** | Known stamina and i-frame defects; different engine and licence |

## Client-authority assumptions in the donor

The donor is a local single-player sandbox. It accepts local input directly, spends stamina
locally, grants i-frames locally, and applies enemy damage locally. It has no session, sequence,
reconciliation, or server authority. Its `extracted.rs` is a build-time constant.

Those assumptions are **not** reusable for Project B. The Project B core is an offline,
server-side model and integration seam:

- Input is a request carrying session epoch, player identity, sequence, server tick, and action
  revision. Only the authoritative resolver starts a dodge, spends stamina, grants i-frames, or
  produces final damage.
- Client prediction can present a dodge, but it cannot mutate the authoritative core's stamina,
  i-frame eligibility, or hit outcome.
- Generated-data coupling is replaced by a caller-supplied `CombatConfig` that is validated before
  use. No preset in this repository is claimed to be an Elden Ring value.
- Bevy 0.18.1 remains the workspace pin. The donor's 0.19.1 is not a reason to upgrade.

## Maintenance and provenance signals

- Primary donor: MIT, one pinned public commit (2026-10-09), active fan project, README explicitly
  separates extracted facts from `ESTIMATE` values. Its generated-data requirement and Bevy 0.19
  make wholesale adoption unsuitable.
- Comparator: Apache-2.0, work-in-progress, README publishes critical and high-priority defects.
  It is a behavioural reference only.
- No uncertain-licence source, decompiled HavokScript, Unreal content, or game asset is used or
  vendored.

## Result

Proceed with B. Implement a dependency-free integer-tick combat core that ports the observed
gesture/buffer/timing/stamina/telegraph mechanisms, replaces generated data with validated
caller-supplied configuration, and makes server acceptance explicit. The core is not evidence of
rendered combat, collision, animation retargeting, network transport, or live server authority.
