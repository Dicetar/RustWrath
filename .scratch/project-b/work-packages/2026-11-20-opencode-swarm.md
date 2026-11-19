# OpenCode swarm: the ordinary WoW route

Owner: the OpenCode DeepSeek overseer, running AutoSwarm. Written 2026-11-20 at the owner's
request: one long unattended session. The workers are free OpenRouter and OpenCode models; the
overseer falls back to DeepSeek 4.1 when a free worker fails twice on the same ticket.

This package is the **non-Souls** half of the game: normal WoW play on the Wrath route. Astra
owns Souls combat tonight ([night 2](2026-11-20-astra-night.md)) and Claude owns grip and damage.
Do not edit `crates/benilla-app/src/souls*`, `combat_kit.rs`, `combat_playback.rs`,
`creature_anim.rs`, `crates/benilla-combat-core/` or `integrations/ale/`. If a fix needs one of
those, write a `Next:` line in the ticket note and move on.

## How the overseer runs the swarm

- **Read first:** [AGENTS.md](../../../AGENTS.md), [sync](../../../docs/agents/sync.md), and
  [the 12340 reference workflow](../../../docs/agents/12340-reference.md) for any Wrath layout.
- **Small tickets.** Split each task below into tickets that one worker can finish in one sitting:
  one behavior, one or two files, under ~300 changed lines, with the acceptance written as
  1–3 checkable sentences and named test cases. Free models do well on that size and badly on
  anything larger. Never give two workers the same file at the same time.
- **Every worker:** own worktree from `origin/main`, own branch `oc/<ticket-slug>`, stage paths by
  name, no force-push or rebase, push the branch at every stopping point. No local Cargo, no client
  or server launch: CI compiles and tests the branch. At most 20 focused cases per ticket, and
  only ones that prove the new behavior.
- **The overseer reviews every diff before it lands.** Reject: invented APIs or field offsets not
  checked against the 12340 reference; nil or constant stubs presented as done; broad
  `#[allow]`/error suppression; tests that only assert the code runs; edits outside the ticket's
  files. Send it back once with the reason; on a second failure reassign to DeepSeek 4.1.
- **Landing**, once CI is green on the branch (the `check` step and the changed crates' tests;
  ignore clippy findings in lines the ticket did not write): merge `origin/main` into the branch,
  push it, `git fetch origin`, then `git push origin HEAD:main`, that one ref only. Land one
  ticket at a time and read the next `check` run before landing more. Delete the landed branch.
- **Notes:** one short note per ticket, `.scratch/project-b/handoffs/2026-11-20-oc-<slug>.md`:
  what changed, the CI run id and result, what is not proven in game. About 20 lines, written
  once at the end. No chat reports, no checkpoints.
- **Do not stop to ask.** Blocked tickets get one `Blocked:` line in their note; take the next.
  `origin` is public: no credentials, game data, extracted FrameXML/GlueXML or local config.

## Tasks, in order

### 1. Land or close the stale field audit

`origin/argon/field-mapping-audit` (GameObject/DynamicObject/Corpse and missing Unit/Player
fields) is ~190 commits behind main. Merge `origin/main` into it, resolve conflicts against the
12340 reference, check each field offset it adds against that reference, and land it if CI is
green. If main already covers a field, keep main's. If the branch is wrong or fully superseded,
write why in its note and leave it unmerged.

### 2. Issue 25: normal attacks and one real spell

Read [issue 25](../issues/25-ordinary-combat-slice.md). For each acceptance line, find the code
that meets it or record the gap (file, function, what is missing) in a gap list at the top of
the note. Then make one ticket per gap and implement them. Typical gaps: an opcode not parsed or
not sent, a combat log or error message not shown, a cooldown or cast bar state not cleared.

### 3. Issue 26: quest, loot and reward

Same method for [issue 26](../issues/26-quest-loot-reward.md): gap list first, then one ticket
per gap (quest giver dialogue, quest log update, loot window and roll, reward choice, the item
in the bag after relog).

### 4. Issue 27: death and recovery

Same method for [issue 27](../issues/27-death-and-recovery.md): release, ghost run, corpse
resurrection, spirit healer, and the server's rejection of invalid actions shown to the player
instead of silently dropped.

### 5. Issue 18: the stock replacement inventory

[Issue 18](../issues/18-stock-replacement-inventory.md) asks for an inventory. This one suits the
free models well: split it by UI area or subsystem, one worker per area, each producing a table
(stock feature, our status, file or "missing"). The overseer merges the tables into the issue's
note and turns the three smallest "missing" rows into tickets.

### 6. If the night is long

The remaining issues 19–24 and 28–32 in `.scratch/project-b/issues/`, in number order, by the
same gap-list method. Skip any that need a live client or server to judge; mark them `Blocked:
needs owner run`.

## Morning summary

Before stopping, rewrite "Swarm result" as one line per task: landed (commits), blocked (why), or
not started; and one line on how many tickets went to free models and how many fell back to
DeepSeek 4.1. Push it to main. That is the only report.

## Swarm result

(not started)
