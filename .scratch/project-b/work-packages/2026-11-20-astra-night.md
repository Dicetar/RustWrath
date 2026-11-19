# Astra night 2: Souls combat depth

Owner: Astra. Written 2026-11-20 at the owner's request: a second long coding session. The rules
are the same as [the first night's](2026-11-19-astra-overnight.md#how-to-run-the-night): do not
stop, ask or report in chat; one short handoff note per task; CI is the compiler (no local
Cargo, no client or server launch); at most 20 focused test cases per task; land each finished
task yourself (merge `origin/main`, push the branch, `git push origin HEAD:main`, one ref); own
worktree from `origin/main`; stage paths by name; no force-push or rebase. Server changes only in
`integrations/ale/rustwrath_souls.lua`; leave `benilla-config/`, game data, the server install and
the Elden Ring reference (read-only) alone.

## Do not collide with Claude

Claude is landing **weapon grip and Elden Ring damage** on main: the `G` two-hand toggle and
`-twohand` kits in `combat_kit.rs`, `souls/input.rs` `grip_toggle`, the `HIT … <hands>` field, and
in the Lua `defended`/`er_defense_*`, `TWO_HAND_BONUS`, `COUNTER_BONUS`, and the float attack-time
read in `start_strike`. Do not rewrite those. Merge `origin/main` before starting each task and
build on what is there. If a task needs to change one of those functions, add to it, do not
replace it.

## Tasks, in order

### 1. Clear the clippy debt

The CI `clippy` step is red from old lints. Make it green across the workspace with real fixes,
not blanket `#[allow]`s (a targeted `allow` with a one-line reason is fine where the lint is
wrong for the code). Land in small commits per crate so a red run points at one crate.

### 2. Lock-on stops flapping

The lock-on sometimes jumps between targets or drops and re-acquires every few frames
(`souls/targets.rs`: `toggle_lock`, `lock_follows_target`). Find the cause (the WoW target and
the lock writing each other is the first suspect), make the lock stable: it changes only on the
lock key, a target switch press, the target dying or leaving range. Tests: no change over many
frames with two units in range; a switch press moves it once; death releases it.

### 3. Jump, left-hand and parry kit rows

The roadmap's "Still open" item: `import_kit` does not emit the `JumpLightLand`/`JumpHeavyLand`,
`LeftLight1…`/`PairedLight1…` and `Parry` rows, so those presses do nothing. Add `kit_import`
options (or default rows) for them from the reference's `extracted.rs`, choose per weapon which
exist (no parry on a two-handed weapon, paired rows only for paired weapons), and have
`scripts/import-kits.ps1` pass them. Tests: the rows appear for a Longsword and Dagger import
and are absent where the weapon lacks them. Remove the item from "Still open".

### 4. Stance break and the critical hit

The server tracks stance (`STANCE_PER_MOTION`, `m.broken_until`). Make a broken stance an
Elden Ring opening:

- Server: when stance breaks, send `BROKEN <guid> <ms>` to watchers; the creature stays rooted
  and kneels (a stun emote/state) for the window.
- Client: on a light press within melee reach and the frontal cone of a broken creature, play
  the kit's critical row (or a heavy row if none) and report `CRIT <guid>`; show a marker on the
  broken unit.
- Server: `CRIT` deals the weapon roll × a critical multiplier (ESTIMATE, named constant) through
  `defended`, once per break, and ends the window.

Tests: the message parsers, the client's opening check (in front, behind, out of reach,
expired). Add a roadmap row.

### 5. Backstab

A light press from behind (rear 90° cone, melee reach) on a creature that is not targeting the
player plays the critical row and reports `CRIT <guid> back`. The server accepts it only if the
creature is not in combat with that player or faces away, then deals the critical damage.
Tests: the cone check and the not-engaged check. Roadmap row.

### 6. Creatures react to hits

A struck managed creature flinches by the hit's stance damage: small hits play a short wound
emote without stopping its windup, hits that spend at least a third of its stance cancel the
windup (`RESOLVE … interrupted`, which the client already must treat as a miss), and a stance
break plays the knockdown from task 4. Keep the rates bounded so a dagger flurry does not lock
a creature. Tests where the logic is client-side; trace lines on the server.

### 7. Remote Souls players' locomotion

The roadmap says remote locomotion still uses stock clips. Pose other Souls players' idle, walk,
run and sprint from the ER locomotion clips, choosing by their movement speed, with the same
per-body cost bound as their relayed actions. Update the roadmap row.

### 8. If the night is long

A "Still open" roadmap item or a `parked`/`blocked` branch from `scripts/agent-tasks.sh` whose
`Next:` asks for source work. Same rules.

## Morning summary

Before you stop, rewrite "Night result" as one line per task: landed (commit), blocked (why), or
not started. Push it to main. That is the only report.

## Night result

(not started)
