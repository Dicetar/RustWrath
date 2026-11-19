# Astra overnight: implement, don't report

Owner: Astra. Written 2026-11-19 at the owner's request: one long coding session that implements
features and lands them, with no stops for reports or questions.

## How to run the night

- **Do not stop.** Do not wait for the owner, and do not ask. If a task is blocked, write one
  line in its note saying why, push, and start the next task. Stop only when every task below is
  landed or blocked.
- **No reports in chat.** Each task gets one short note,
  `.scratch/project-b/handoffs/2026-11-19-<slug>.md`, with the header from
  [sync](../../../docs/agents/sync.md#the-task-note) and three sections: what changed, the CI
  run id and result, and what is not proven in game. Keep it to about 30 lines. Write it once at
  the end of the task. Do not write checkpoints, claims or release notes.
- **CI is your compiler.** Do not run Cargo locally and do not start the client or the servers.
  Push the task branch. The Windows `check` step compiles every crate in about 7 minutes, and
  the test steps run the tests of the crates you changed. While a run is in progress, keep
  coding the next part. Fix a red run in your very next commit.
- **Tests.** Each task gets at most 20 focused cases, and only ones that prove the new
  behavior. 53 known failures are `#[ignore]`d, so any failing test is yours. The `clippy` step
  is still red from old lint debt that Claude is clearing, so ignore clippy findings in lines
  you did not write.
- **Land each finished task yourself.** Once its branch compiles and its tests pass on CI:
  1. Merge `origin/main` into the branch.
  2. Push the branch, then run `git fetch origin`.
  3. Fast-forward main: `git push origin HEAD:main`. Push that one ref only (see
     [sync](../../../docs/agents/sync.md#reading-ci)).

  The owner builds main in the morning.
- **Git:** work in your own worktree, created from `origin/main` (never in
  `D:\Projects\wow\client`). Stage paths by name. No force-push, no rebase.
- **Outside the repository:** the Elden Ring reference lives at
  `D:\Projects\elden-ring-combat-rewrite-reference`. You may read it, but never edit it or
  commit it. Server changes happen only in `integrations/ale/rustwrath_souls.lua` in this
  repository, and the owner copies that file to the server. Leave
  `benilla-config/`, game data and the server installation alone.

## Tasks, in order

### 1. Land the ready queue

Run `scripts/agent-tasks.sh` and take every branch whose status is `ready to merge` (on
2026-11-19 these were `fix/wrath-talent-preview`, `fix/wrath-diagnostics-a`,
`fix/wrath-action-spell-slot-a`, `argon/say-chat-log`, `fix/wrath-action-bars` and
`integration/wrath-alpha`). For each one, read its note and diff, then merge it into an
up-to-date `main`. Push `HEAD:main` after every two or three merges, and read the Windows `check`
step before you merge more. When a merge conflict needs the branch author's intent, resolve it
from that branch's note. If the note does not settle it, skip the branch and add a `Next:`
line to its note. Delete each landed remote branch. Each note gets `Status: landed`.

### 2. The stealth flag reads the wrong byte

`unit_is_stealthed` (`crates/benilla-protocol/src/messages/update_object/fields/unit.rs:244`)
reads CREEP from `UNIT_FIELD_BYTES_1` byte 3. On 3.3.5 that byte is the anim tier, where
`0x02` means hover; the stand/vis flags are byte 2. Confirm this against the 12340 reference
([workflow](../../../docs/agents/12340-reference.md)), fix the byte, and add one case where a
hovering unit is not stealthed and one where a CREEP unit is. Find every reader of the old byte
(`git grep`) and fix those too. Remove the stealth item from the roadmap's "Still open" list
(`docs/architecture/souls-er-combat-roadmap.md`).

### 3. A kit per weapon type

Today there is one `benilla-config/combat/player.kit` (Longsword, one-handed, `--off-hand`).
Make the kit follow the equipped main-hand weapon:

- The client reads `benilla-config/combat/kits/<name>.kit` and chooses one by the main-hand
  item's weapon subclass: dagger, 1H/2H sword, 1H/2H axe, 1H/2H mace, polearm, staff, fist and
  spear. Two-handed subclasses use the `--grip two` kit. Fall back to `player.kit` when the file
  is missing, and to the stock path when that is missing too. Re-pick when the weapon changes,
  the same way F10 re-reads the kit today.
- Put the subclass → kit name table in one place, with a test for it. Map each subclass to the
  closest weapon class in the reference's `extracted.rs` (for example, Dagger → Dagger, 2H
  sword → Greatsword, polearm → Halberd, fist → Fist). Read the names from the file; do not
  guess.
- Add `scripts/import-kits.ps1`, which runs `import_kit` once for each row of that table into
  `benilla-config\combat\kits\`. The owner runs it; you do not.
- Update the roadmap row and `souls/config.rs`'s doc.

### 4. Creatures strike with their own animation

The server script plays the generic `EMOTE_ONESHOT_ATTACK1H` for every telegraphed strike
(`integrations/ale/rustwrath_souls.lua`, `SWING_LEAD_MS`). On the client, when a `STRIKE`
arrives (`crates/benilla-app/src/souls/server.rs`), play the creature's own attack sequence
for its windup's swing. Pick from the attack animations its model has, in order of preference:
Attack1H, Attack2H, Attack2HL, AttackUnarmed, then the generic attack. Keep the timing so the
hit lands at the end of the windup. Tests: the pick order, with models that lack each one.
Remove the item from "Still open".

### 5. Strike shapes

Every telegraphed strike is currently one shape: a 60° half-cone at melee reach
(`STRIKE_HALF_CONE_DEGREES`). Give the server script three kinds:

- **sweep:** wide cone, normal reach;
- **thrust:** narrow cone, +2 yd reach;
- **slam:** circle around the creature, longer windup.

Choose the kind per creature from what the script can read (for example its family or type, or
a fixed table of entries with a default). Send it as an optional field on `STRIKE`, so an old
client still parses the message. The client judges reach and cone by kind and draws the bar's
colour by kind. Keep `server.rs`'s parser tests passing and add one case per kind. Update the
roadmap row and the protocol list in the "server half" section.

### 6. Other Souls players use the Elden Ring clips

Other Souls players show generic attack, parry and dodge animations for each relayed `SWING
<action>`. Pose them from the Elden Ring clip of that action through `souls/retarget.rs`, the
same way our own body is posed. Look the action id up in our local kit, and fall back to the
generic animation when the id is unknown. Keep the per-body cost bounded, and say in the note
what the bound is. Update the roadmap rows that say "local player only".

### 7. If the night is long

Pick up a `parked` or `blocked` branch from `scripts/agent-tasks.sh` whose `Next:` line asks
for source work, or a "Still open" item in the Souls roadmap. Use the same rules.

## Morning summary

Before you stop, rewrite this file's "Night result" section as one line per task: landed (with
the commit), blocked (with why), or not started. Push it to main. That is the only report.

## Night result

1. Ready queue: landed b7856200 (all six branches; CI repairs through 33dc2bc7).
2. Stealth byte: landed 9a99ea15 (verified existing Wrath-to-donor conversion; added hover/CREEP regressions).
3. Weapon kits: landed cc124e06.
4. Creature strike animations: landed 7f75ea5e.
5. Strike shapes: landed b13d3f38.
6. Remote Souls player clips: landed 7aa2779b.
7. Retarget gameplay validation: blocked — client/server launch is prohibited overnight; owner must check alignment, speed and crowded-scene performance.
