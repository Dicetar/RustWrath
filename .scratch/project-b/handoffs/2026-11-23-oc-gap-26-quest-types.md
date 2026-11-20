# Handoff — oc-gap-26 uncovered quest types (doc-only)

Status: ready to merge
Owner: opencode / LongCat 2.5 Free (longcat-2.5-free)
Branch: oc/gap-26-quest-types
Issue: 26 — Complete a quest, loot and preserve the reward
Next: integrator reviews and lands; the per-type rows stay `TBD-*` placeholders until the owner numbers tickets.

## Method
Read-only documentation task on branch oc/gap-26-quest-types. Read issue 26 (gap table and small
tickets), the 2026-11-22 gap-26 handoff and docs/agents/sync.md, then grep-verified the quest
vocabulary in the current tree before writing: `quest_flags` in
crates/benilla-protocol/src/messages/quest/log.rs, the QuestInfo tags in
crates/benilla-formats/tests/quest_tag_names.rs, the daily bit in crates/benilla-app/src/ui_quest_log.rs,
the auto-complete row in crates/benilla-app/src/ui_quest/mod.rs, `ui_quest_share.rs`, and the
vehicle note in crates/benilla-protocol-wrath/src/gameplay.rs. No Cargo, build, test, clippy, client,
server, CI or source edit; no branch switch, rebase, merge or force-push.

## What changed
Appended "## Uncovered quest types (2026-11-23)" to .scratch/project-b/issues/26-quest-loot-reward.md:
13 types (escort, dungeon, raid, daily, world, auto-complete, exploration, stay-alive, hidden
rewards, group/elite, PvP, repeatable/weekly, vehicle), each with a one-line rationale, a source
hint and a `TBD-*` future-ticket placeholder. The existing gap table is unchanged. This note is new.

## Checks not run
No Cargo check/test/clippy, no sccache, no client or server run, no live quest of any type, no CI.
Markdown-only pushes are skipped by CI. The branch was behind origin/main by 1 at push time; no
merge or rebase was done.

## What is not proven
Documentation only. No claim that any listed type works or fails, and no runtime proof for the
core quest/loot/reward slice. Source hints are grep-verified, not exercised. The issue's live-only
checks remain owner-run UNKNOWN.
