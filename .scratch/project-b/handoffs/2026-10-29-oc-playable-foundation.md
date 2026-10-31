# OC handoff — build the first visibly playable client

Read first:
1. Repository `AGENTS.md`, `docs/METHOD.md`, `docs/MAP.md`, `docs/CONTRIBUTING.md`, `CONTEXT.md` and applicable nested instructions.
2. [Playable foundation](../work-packages/2026-10-29-playable-foundation.md).
3. [Graphics recovery draft](../work-packages/2026-10-29-graphics-recovery-draft.md).
4. Existing alpha checkpoint, current claims and the earlier delivery plan linked in the foundation.

## Assignment

Deliver the private Windows stock-UI solo alpha in dependency order P0–P4. Start with actual graphics failure, especially the own-player body and white UI obstruction. Then prove movement/interaction and the full human-driven quest/combat/loot/reward/re-entry loop. Reuse the existing Rust/Bevy owners. These drafts guide implementation; they do not assert all listed systems need rewriting.

Planning source pin is `b03a919b`. The resolver getter repair there has focused test evidence but no current live binary acceptance. Your dirty changes in `crates/benilla-app/src/wrath.rs` and `crates/benilla-ui/src/script/mod.rs` were preserved. Revalidate HEAD/status/diff and active processes before touching them; do not restore files wholesale or overwrite concurrent changes. Record claimed exact paths in the existing checkpoint before source edits.

## First work block

1. Inspect current diffs and existing launch/probe helpers without displaying `.probe-identity`. Establish actual build/profile identity and whether a client already runs. Do not kill an unowned process.
2. Read the recent captures/logs and reproduce one valid same-location symptom under the existing bounded run. Confirm actual character/map/GUID rather than copying old handoff labels. If glue startup blocks the run, finish that narrow fix first.
3. Trace GUID → model → render hierarchy/draw using G1. Assess your visibility fix causally. Report the first failed stage and repair it; do not start with mass API stubbing or a broad renderer rewrite.
4. Run focused checks and build the real launcher, then demonstrate the original symptom and adjacent state. Record source/dirty patch, binary/profile hash, logs, visual evidence and unresolved limits.
5. Continue G2/G3 and P2 as dependencies permit. If a block ends without closure, retain the exact failing edge and next bounded experiment; do not declare a pass from silence or repeat random runs.

## Execution boundaries

The owner has already authorized needed implementation and bounded verification in this project. Do not ask again for routine compile/test/probe permission. No unattended combat; prepare the interactive route for the owner. No server/DB/WM administration, credential changes, account substitution, install writes, bulk cleanup, new dependencies or agent delegation is implied. Preserve all existing evidence, shadow data and unrelated untracked files.

Existing short probe: `.scratch/project-b/verification/2026-10-28-oc-alpha/astra-memory-probe.ps1`. Read it before reuse. It has a 55-second/5-GiB bound, which is suitable for smoke and cannot establish a complete gameplay loop. Read `run-from-identity.ps1` without printing the identity it loads. Keep unattended/silent flags appropriate to run mode and never expose credentials in logs. Use unique evidence names.

The version-264 particle refusal stays until a correct bounded decoder replaces it. Missing producer fields, hardcoded map answers and vehicle no-ops stay marked incomplete. Do not disable required UI/addon behavior just to clear errors; stock A1 configuration and selected-profile archive addons must be recorded precisely.

## Interactive acceptance card

Prepare the exact launch command through the existing identity wrapper, build/hash and log paths. Owner chooses the eligible character. Before gameplay, verify intended GUID/map/position, third-person body, ground/camera and readable UI. Move/turn/strafe/jump, target and interact with the intended NPC; open/close map and bags.

Use the existing Northshire quest route only after verifying eligibility and availability. Quest 6/Garrick is a candidate, not a guaranteed valid repeatable fixture; never reset it or teleport through the DB. If unavailable, request only the needed character/route choice while continuing independent graphics work.

Owner accepts quest, performs the fight, loots, checks bag/objective updates, returns, chooses reward and logs out. Fresh login must show server-persisted reward/quest/inventory state. Record a failure at its actual step; continue fixing earlier dependencies without calling the route complete. For a completed nonrepeatable quest, repeat nonconsumptive checks or use a separately eligible route.

## Required handback

- What now works for the player, exact source/binary/profile and launch instructions.
- For each repaired defect: root cause, affected paths, focused check result, actual rendered/runtime result.
- F01–F20 status at the appropriate gate: passed/failed/unverified/deferred, with evidence links. Do not manufacture full coverage by listing source files.
- Remaining first blocker, next concrete step, owned process state and preserved dirty work.
- A0/A1/A2 verdict only when their complete gate is demonstrated. No percentage or playable claim based on successful startup.

## Pasteable prompt

Read `D:/Projects/project-b-client/.scratch/project-b/handoffs/2026-10-29-oc-playable-foundation.md` and its two linked drafts. Revalidate source and ownership, preserve all existing work, and execute P0–P4 toward a visibly playable private Windows alpha. Begin with the player render chain and white UI obstruction, using the graphics draft's diagnosis and acceptance checks. Reuse existing owners, repair complete input/server/state/UI paths, and use the already-authorized bounded verification. Keep combat human-driven and server/install/credentials untouched. Finish with exact evidence and a runnable owner acceptance route; never substitute API stubs, a build pass or a world screenshot for playable acceptance.
