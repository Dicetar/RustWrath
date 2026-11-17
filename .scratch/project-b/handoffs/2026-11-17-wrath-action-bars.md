# B5 — Wrath action bars, spellbook and macros

Status: in progress
Owner: Pi / OpenAI
Branch: fix/wrath-action-bars
Issue: none
Next: Trace stock callers and reference contracts through action identity/state, persistence, macros, form/vehicle paging and spec replacement; repair real-owner gaps.

Started from origin/main `7ef93503` after B4 landed at `b13c163d`; A2 also landed at `58e98b0d`.
Worktree: `D:\Projects\wow\worktrees\fix-wrath-action-bars`. Shared dirty checkout
`D:\Projects\wow\client` is untouched. Server/AC under `D:\Projects\wow\server`, reference
`D:\Projects\wow\reference\wowdecomp-reference` and installed Data are read-only.
Reference HEAD verified at AGENTS pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
Investigate behavior, do not copy implementation. Start: release-matrix F15 and ui_action/drain.rs.
B4 owns spell; consume its outcomes without rewriting it. No combat_log/combat_text edits.
Data behaviors must remain editable by the live hotpatch/Lua translation route.

Compilation: not run. Tests: not run (focused-case ceiling 20). Client: not run.
AutoSwarm availability inspection timed out; no child assignment was issued.

## Owner check list — after the next batch build

1. drag two spells to the bar;
2. relog;
3. check the cooldown swipe and the out-of-range tint;
4. make and run a /cast macro;
5. switch spec, if available.
