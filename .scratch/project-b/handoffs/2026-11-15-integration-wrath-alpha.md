# Wrath private alpha integration candidate

Status: ready to merge
Owner: Pi / OpenAI
Branch: integration/wrath-alpha
Issue: none
Next: Integrator lands this documentation-only delta; owner plays the numbered Northshire checks after the next batch build.

## Source review — 2026-11-17

Fetched origin and merged `origin/main` at `7ef93503` with a merge commit, not a rebase.
The branch was 25 ahead / 85 behind at admission (the earlier 44-behind count was stale).
Git merged without conflicts. Reviewed the resulting index against origin/main: the only
remaining difference is this note; there is no candidate-only source to land or audit.
In particular, the automatic merge of `crates/benilla-app/src/wrath.rs` equals main.
Source review is complete for this integration delta, not a fresh certification of all main.

Dropped the old candidate-only character-login refusal/rename patch claim and the old
instruction to build an isolated candidate. Main already contains that work and the newer
B4 cast outcomes, A1 death cycle, A2 melee loop and B3 world-fidelity integrations.
Kept main's implementations in full: no old candidate behavior overrides them, no duplicate
patch is carried. Historical login research used AC `LoginFailureReason` and the 12340
handler at `0x006B2070`; no new implementation or reference code was copied here.

## Paths and references

- Owner's dirty checkout: `D:\Projects\wow\client` — not touched.
- This worktree: `D:\Projects\wow\worktrees\integration-wrath-alpha`.
- Server / AC source: `D:\Projects\wow\server` (read-only).
- Reference: `D:\Projects\wow\reference\wowdecomp-reference` (read-only), verified HEAD
  `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`, the AGENTS.md pin.
- Game data: `D:\Projects\wow\server\world of warcraft 3.3.5a hd\Data` (read-only).

## Northshire owner check list — after the next batch build

1. Build and launch the integrated batch by hand from the owner checkout. Record the source
   revision, binary hash, selected profile and local run/log path; enter credentials yourself.
2. Check login, realm and roster navigation, then select Firna. Verify identity and Northshire
   world entry before interacting. Do not create or delete a persistent character for this check.
3. Move and turn near the starting quest giver; confirm stable position, camera and interaction.
4. Open the quest giver, accept an available Northshire quest, then check the quest log,
   objective text, watch and marker update. Record which quest and its starting objective count.
5. Play one required combat encounter yourself. Check action activation, cast outcome,
   damage feedback and objective progress; preserve the first refusal/error if it fails.
6. Loot the defeated creature solo. Check the loot window, inventory receipt and quest-item
   progress, including closing the window. Do not infer receipt just from a disappearing row.
7. Complete and turn in the quest if available. Check reward choice, inventory/money/XP changes,
   quest removal and the next marker/dialog state. Record any unavailable prerequisite.
8. Log out normally and re-enter the same character. Verify position, inventory and quest
   progress survived; record the first failure against the exact built revision.

## Verification

`script/agent-tasks.sh` (actual path `scripts/agent-tasks.sh`) ran with Git for Windows bash;
the initial PATH bash resolved to broken WSL and was not used further. Inspected HEAD,
index, clean admission state and live processes; an existing client was left untouched.
Reviewed the merge result against main and confirmed no source delta remains.
Compilation: not run. Tests: not run. Client/runtime/visible gameplay: not run.
Ready to merge means source-reviewed only, not a completed private-alpha playthrough.
