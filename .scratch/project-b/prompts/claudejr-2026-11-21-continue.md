You are ClaudeJr on RustWrath. Continue tonight's ticket list in
`.scratch/project-b/prompts/claudejr-2026-11-21.md`. Read it again now: its rules, ownership
limits and landing steps all still apply.

## Where you stopped
- **Ticket 0 is done.** `jr/close-stale-notes` (f2a19d17f) is on main, and all four 2026-11-20
  notes say landed.
- **Tickets 1-4 are not started.** You created the four worktrees, but each is still clean at
  d9d35507, with no commits and no pushed branch:
  - `D:\Projects\wow\worktrees\jr-combat-log-history` (`jr/combat-log-history`)
  - `D:\Projects\wow\worktrees\jr-wmo-unified-modes-v2` (`jr/wmo-unified-modes-v2`)
  - `D:\Projects\wow\worktrees\jr-spellcast-interruptibility` (`jr/spellcast-interruptibility`)
  - `D:\Projects\wow\worktrees\jr-spirit-healer-followthrough` (`jr/spirit-healer-followthrough`)
- **origin/main has moved** to 250d5721a or later. In each worktree run
  `git fetch origin && git merge --ff-only origin/main` before your first edit. The branches have
  no commits yet, so this fast-forwards cleanly.

## What changed on main since you started
- DeepSeek's swarm landed `SPELL_HEAL` (cd95c37d9) and `SPELL_ENERGIZE` (bdf7391cd) combat-log
  feeds. Ticket 1's ring must capture them through the VM's `COMBAT_LOG_EVENT_UNFILTERED` fire
  point, with no edit to `wrath_combat_log_feed.rs`. One test should show a heal event reaching
  the ring.
- A run of `oc/clippy-*` merges cleared most lib-test clippy findings (fdcbd82b0). Your diffs must
  add no new clippy finding. Check `player-clippy` and `clippy` in your CI logs.
- `combat_log.rs` on main still stubs the ring: `CombatLogGetNumEntries` returns 0 and
  `CombatLogGetCurrentEntry` returns nothing (around lines 224 and 243). Ticket 1 replaces those
  stubs.

## Do now, in order, without stopping to report
1. Ticket 1, combat-log history ring.
2. Ticket 2, unified WMO modes 0 and 2. Read eace0985 first.
3. Ticket 3, `UNIT_SPELLCAST_INTERRUPTIBLE` / `NOT_INTERRUPTIBLE`. `ui_script/wrath_cast.rs` is
   yours. `script/target.rs` is Gemini's: if the events are only listed there (line 94), raise them
   without editing that file.
4. Ticket 4, spirit-healer follow-through.

For each ticket:
- Push the branch at the first commit, so the work exists on origin.
- Write the note at `.scratch/project-b/handoffs/2026-11-21-jr-<slug>.md`.
- Read CI from the log (`gh run view <id> --repo Dicetar/RustWrath --log-failed`). Fix what it
  names, then land with `git push origin HEAD:main`, that ref only.
- If CI is still running, start the next ticket and come back to it. Do not wait idle.

No local Cargo, no client and no server. Stopping after one ticket, or with tickets that exist
only as local worktrees, does not count as done.

Before stopping, report one line per ticket: landed (commit), blocked (the exact reason from the
log or reference) or not started.
