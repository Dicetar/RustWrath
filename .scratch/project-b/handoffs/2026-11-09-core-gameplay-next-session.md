# RustWrath next-session handoff — core gameplay investigation

Prepared 2026-11-09, Europe/Moscow. This is a local continuation artifact, not permission to
merge branches, launch workers, run verification, or change the game/server installation.

## Start here: objective and current owner intent

The owner asked to continue RustWrath, supplied the earlier handoff and OC multi-return repair,
then asked for core work. An initial inspect-honor cache fix was judged too small. A full
inspect-honor packet-to-Lua implementation was then made locally. The owner subsequently asked
for bigger issues and a second sweep. Those sweeps were investigative: movement and transfer
defects were identified, but not repaired. The latest request was this detailed next-session
handoff. Do not treat this document as a new instruction to autonomously implement every finding.

Project goal: native Rust/Bevy WoW 3.3.5a build 12340 client, ultimately Firna's Northshire
route: enter world, move, accept quests, fight, loot, update objectives, turn in, receive reward.
Small compatibility fixes do not establish completion of that route. Focus on shared gameplay
mechanisms and their actual producer/consumer chains.

## Exact workspace and Git state — revalidate immediately

- Canonical checkout: `D:/Projects/project-b-client`.
- Origin: `https://github.com/Dicetar/RustWrath.git`.
- Local checked-out branch: `main`.
- Local HEAD at handoff: `6c15d921611817458acabbd91611e5d10ae062ea`.
- Remote main SHA verified using `git ls-remote origin refs/heads/main`:
  `a396546a724aee6d1ff406941f3bcf05da04e2e3`.
- Local status reports main behind origin/main by 23 commits. No pull, merge, rebase, branch
  switch, commit or push was done by this chat.
- Seven dirty tracked source files belong to THIS chat; see next section. Preserve them.
- Numerous untracked `.scratch/project-b` assets, extracted Lua/XML, logs and old handoffs
  pre-existed. Preserve them. A loose untracked root `FETCH_HEAD` also existed earlier; inspect
  rather than blindly staging it.
- Current source findings below were made against local `6c15d921` plus our inspect diff,
  NOT against a checked-out copy of latest remote main.

Suggested admission reads (read-only):

```powershell
git -C D:/Projects/project-b-client status --short --branch
git -C D:/Projects/project-b-client diff --stat
git -C D:/Projects/project-b-client diff
git -C D:/Projects/project-b-client worktree list --porcelain
git -C D:/Projects/project-b-client log --oneline HEAD..origin/main
git -C D:/Projects/project-b-client ls-remote origin refs/heads/main refs/heads/fix/wrath-ui-multi-return-a
Get-Process | Where-Object { $_.ProcessName -match 'opencode|codex|cargo|rustc|benilla' }
```

Do not pull/rebase/switch underneath the dirty shared checkout. Reconcile upstream via a safe
isolated integration approach when implementation is assigned; preserve and review the diff.
Latest remote already contains `074a899e` and many UI/hotpatch fixes. Earlier statements in this
chat that OC's multi-return work was not on main are now obsolete.

## Required instructions and reference locations

Read these before substantive continuation, resolving relative paths from the checkout:

1. `D:/Projects/project-b-client/AGENTS.md`.
2. `CONTEXT.md`, `docs/agents/workflow.md`, `docs/agents/active-ownership.md`.
3. `docs/METHOD.md`, `docs/CONTRIBUTING.md`, relevant parts of `docs/MAP.md`.
4. `docs/agents/repository.md` and `docs/agents/12340-reference.md`.
5. Installed reference skill:
   `C:/Users/Qraag/.codex/skills/project-b-12340-reference/SKILL.md`.
6. For an explicitly assigned local launch: `docs/agents/local-launch.md` and
   `docs/launching.md`.

Preserved original-client source research checkout: `D:/Projects/wowdecomp-reference`.
Required pin: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. It was clean, non-shallow and at
that pin when checked earlier in this chat. Recheck. It is read-only research material: do not
build, update, transplant its source, or commit it. Record behavior, file/symbol/address and
confidence; decompilation can contain reconstruction gaps. Donor `reference/bindings.tsv` in
RustWrath documents 1.12 contracts, not Wrath authority.

Local matching server source used read-only:
`D:/WOW/Azerothcore_WoTLK_Rebuild/source/azerothcore/src/server/game/`.
Game installation: `D:/WOW/world of warcraft 3.3.5a hd` (read-only; selected asset chain/profile
must be determined from project configuration rather than assuming the install's defaults).

## Execution boundaries

- User explicitly prohibits builds, tests, linters, type checks, browser/client/visual checks,
  and creating test files unless requested. No such checks were run in this chat. Existing test
  fixtures were updated only to supply newly required struct fields; no new tests were created.
- Source review and `git diff --check` are the verification performed. Neither establishes
  compilation, packet reception, rendering, or playable behavior.
- All authorized Cargo runs must use sccache; `.cargo/config.toml` configures the wrapper.
- Do not spawn subagents/AutoSwarm workers without current explicit user authorization.
  Prior two-supervisor prompt descriptions in the attachment were not authorization to launch.
- If delegation is later assigned, read `C:/Users/Qraag/.agents/skills/autoswarm/SKILL.md` first.
- No unrequested main merge, force-push, blanket staging, cleanup, destructive reset or stash.
- The owner enters credentials and drives gameplay/combat. Inspect actual client/build processes
  before any authorized launch; stale ownership tables are not active process evidence.
- `Launch RustWrath.cmd` uses `tools/launch-client.ps1 -BuildLatest`; pushing Git does not update
  the executable. Client runs must identify actual source, binary hash, profile and log.

## Uncommitted implementation: full inspect-honor data path

All paths below are relative to `D:/Projects/project-b-client`. This work is unfinished in the
sense of compilation/runtime acceptance and publication; source implementation is present.

1. `crates/benilla-protocol-wrath/src/gameplay.rs`
   - Around 2030: new `InspectHonor` typed wire record and `read_inspect_honor`.
   - Strict payload size 25 bytes; checked Reader reads GUID u64, highest-rank byte, today's
     kills u16, yesterday's kills u16, today's honor u32, yesterday's honor u32, lifetime kills u32.
   - New `ServerEvent::InspectHonor`.
   - Around 2650: parser dispatch for `MSG_INSPECT_HONOR_STATS`. Opcode constant is u32 while
     parser input is u16, so dispatch uses a guarded comparison `u32::from(op)`.
2. `crates/benilla-protocol/src/messages/pvp.rs`
   - Shared `InspectHonorStats` gets `today_honor: u32`.
   - Donor 50-byte decoder sets it to zero without consuming extra wire bytes.
   - Existing donor decoder assertion fixture gets the zero field.
3. `crates/benilla-app/src/wrath.rs`
   - `translate_gameplay`, around 3219: converts the typed 12340 reply into existing
     `SessionEvent::InspectHonorStats`; unavailable donor weekly/DK/rank-bar fields are zero.
   - Wrath today/yesterday kills are two separate halves; today's count alone is placed in
     donor `session_kills`, and yesterday's count goes to `yesterday_hk`.
4. `crates/benilla-app/src/ui_honor.rs`
   - `HonorFeedMemo.last_inspect` stores the COMPLETE `InspectHonorStats`, not only GUID.
   - Changed reply for same player now refreshes Lua and emits `INSPECT_HONOR_UPDATE`.
   - Projects `today_honor` into `InspectHonorData`.
   - Existing current-inspect-target mismatch filter remains. Identical replies still coalesce;
     late old-target replies and target-change semantics deserve review if that scope is assigned.
5. `crates/benilla-ui/src/script/pvp/mod.rs`
   - `InspectHonorData.today_honor` added.
   - `GetInspectHonorData` has explicit `UiTarget::Wrath335` six-value branch:
     todayHK, todayHonor, yesterdayHK, yesterdayHonor, lifetimeHK, highestRank.
   - Existing donor twelve-value branch retained; absent data returns zero snapshot at full width.
6. `crates/benilla-ui/src/script/pvp/tests.rs`: existing inspect fixture supplies today_honor=72.
7. `crates/benilla-app/src/ui_script/honor_frame_tests.rs`: existing inspect fixture supplies 0.

Evidence:
- `D:/Projects/wowdecomp-reference/WoW/Source/Ui/PaperDollInfoFrame.cpp`
  `Packet_MSG_INSPECT_HONOR_STATS`, OFFSET `0x005E7A50`: wire field order and target GUID check.
- Same reference directory, `PaperDollInfoFrame_Scripts.cpp::Script_GetInspectHonorData`,
  OFFSET `0x005E77C0`: exactly six Lua values.
- Local server `Handlers/MiscHandler.cpp::HandleInspectHonorStatsOpcode`, around 1028: 25-byte
  reply. IMPORTANT server discrepancy: it puts `uint8(player->GetHonorPoints())` in the byte the
  client treats as highest lifetime rank. Our client preserves that byte; server code was not
  changed. Do not claim that the displayed highest rank is thereby correct for this server.

Source review checked known struct literals, new enum consumer fallback, parser dispatch, and
data propagation. `git diff --check` passed. No build/test/runtime verification. No new commit.
Latest diff stat: seven files, approximately 80 insertions and 6 deletions (includes initial fix).

## Bigger finding A: server-driven movement semantics are lost

This was investigated, not changed. Review against newer origin/main before assigning repair.

### A1: controlled-player spline discarded

- `crates/benilla-app/src/wrath.rs::translate_gameplay`, around 3240–3270:
  `WrathGameplayEvent::MonsterMove` returns no events when `m.guid == self_guid`.
- Existing destination owner is `crates/benilla-app/src/player/server_ride.rs`:
  `drive_self_ride` expects a `Spline` on the embodied player, drives charge/taxi/forced movement,
  and sends `ClientCommand::MoveSplineDone` at completion/interruption.
- `crates/benilla-app/src/net/objects.rs::monster_move`, around 841, attaches/clears spline and
  handles transport membership and start/facing. The self event cannot reach it through the
  Wrath translation branch above.
- `crates/benilla-protocol-wrath/src/session.rs`, around 444: normal SMSG_MONSTER_MOVE also
  reaches `WorldState::apply_monster_move` in the reducer.
- `crates/benilla-protocol-wrath/src/reducer.rs::apply_monster_move`, around 482: records the
  LAST path point immediately as authoritative position and clears create_spline.
- `wrath.rs::wrath_apply_world`, around 9270: self pose may be applied as a correction when local
  and server movement flags are idle. Existing self branch does not install create splines.

Impact inferred from that chain: taxi/forced player paths cannot animate through the existing
ride controller; endpoint snapping and missing spline-done acknowledgment are possible.
Not runtime reproduced. Do NOT merely remove the self guard without resolving endpoint snapshot
ordering, local control ownership, stop semantics and completion acknowledgments together.

### A2: cyclic paths stop after one traversal

- `crates/benilla-protocol-wrath/src/gameplay/movement.rs` decodes flags and path.
- Wrath `translate_gameplay` does not carry cyclic into the shared MonsterMove event.
- `wrath_apply_world` DOES map create-block flag `0x80000` into `CreateSpline.cyclic`.
- `crates/benilla-app/src/net/motion/spline.rs::create_spline`, around 211, ignores that field;
  `Spline` itself has no cyclic state. `sample` clamps elapsed fraction to 1, and `sample_splines`
  removes the component at one duration, around 344.
- Local server authority: `Movement/Spline/MoveSplineFlag.h` has cyclic and enter-cycle flags.

Impact: server repeating paths become one-shot locally, including joining a running cyclic path.
Handle loop topology, time modulo, create-block elapsed time and completion semantics together.

### A3: falling/parabolic paths discarded

- `gameplay/movement.rs::read`, around 66, reads parabolic parameters when flag `0x800` is set.
- `wrath.rs::translate_gameplay` drops flags Falling `0x200`, Parabolic `0x800`, Frozen `0x400000`.
- Existing Spline only models grounded linear vs flying Catmull-Rom; trajectory parameters have
  nowhere to go. Implement a proper evaluator and ownership, not just remove the refusal guard.
- `session.rs` handles regular SMSG_MONSTER_MOVE for reducer state, but no corresponding
  SMSG_MONSTER_MOVE_TRANSPORT reducer branch was found; gameplay parser supports both.
  Treat this transport inconsistency as a follow-up to investigate, not a proven visible failure.

## Bigger finding B: refused world-transfer recovery missing

Confirmed source gap, not changed or runtime reproduced:

- `crates/benilla-protocol-wrath/src/session.rs`, around 365: SMSG_TRANSFER_PENDING moves an
  established session to `Phase::TransferPending`.
- There is no SMSG_TRANSFER_ABORTED reducer branch to restore the established phase.
- `crates/benilla-protocol-wrath/src/gameplay.rs` has no abort decoder, and `wrath.rs` function
  `is_shared_12340_server_opcode` (around 11485) excludes the abort opcode. Therefore no UI event
  is generated by either ordinary decoding path.
- `wrath.rs`, around 10800 in the worker command drain, rejects enforced-scope commands unless
  `session.phase() == Phase::InWorld`. WM requests also check InWorld around 7197.
- `session.rs`, around 514, accepts relayed observer movement only while InWorld.

If an abort arrives after pending, the connection can retain the old world but remain in a phase
that rejects interactions and ignores observer movement. Refusals without prior pending still
lose the user-facing error. Establish the actual server sequence for reproduction; do not claim
all refused instance entries necessarily send pending first.

Authoritative locations:
- Reference `D:/Projects/wowdecomp-reference/WoW/Source/Client.cpp`:
  `Packet_SMSG_TRANSFER_ABORTED`, OFFSET `0x00403910`, around 1953–2034. Reads map u32 and reason
  u8, interprets reason-specific data/text, disables loading screen. Handler registered around 1350.
- Server `Entities/Player/Player.cpp::SendTransferAborted`, around 11685: map u32, reason u8,
  optional argument u8 ONLY for INSUF_EXPAN_LVL, DIFFICULTY, UNIQUE_MESSAGE.
- Donor fallback `crates/benilla-protocol/src/messages/parse.rs`, around 415, has an abort
  decoder, but do not reuse it blindly: Wrath includes map and reason-dependent tail.
- Existing presentation owner `crates/benilla-app/src/net/session.rs::on_transfer`,
  `transfer_pending`, `transfer_aborted`, `worldport` (around 489, 956, 975, 984).
- `crates/benilla-app/src/loading_screen.rs` is the loading overlay owner.

Potential cohesive repair: typed Wrath abort decode, reducer recovery without world retirement,
reason/argument propagation to correct UI/loading owner, phase publication, and retained current
world/entities. Explicitly distinguish pre-NEW_WORLD refusal from an actual adopted world scope.
Review stale/later abort behavior and malformed packet handling. No fake successful transition.

## Earlier OC work and concurrent upstream changes

Original OC multi-return branch `fix/wrath-ui-multi-return-a`, commit
`074a899ef060ecb2cbf8063201daddfe5345f84e`, remote branch SHA still verified equal.
It corrected worldstate 11-value tuple, battlefield 7-value tuple, and Wrath self honor getters,
retaining donor shapes via target gates. It is NOW part of origin/main. Its worktree was at
`C:/Users/Qraag/.local/share/opencode/worktree/399553/rw-api-arity-a`.
Review worktree: sibling `rw-api-arity-b`, branch `review/wrath-ui-multi-return-b` at 6c15d921.
Other worktrees exist; run `git worktree list --porcelain` rather than assuming exclusive ownership.

Git may reject these OC worktrees for dubious ownership (owned by Administrators SID). Earlier
read-only inspection used per-command `git -c safe.directory=<exact-path> ...`; no global trust
configuration was changed. Do not blindly add broad global safe-directory exceptions.

Latest upstream includes 23 commits ahead of local, notably:
- `074a899e` multi-return contracts.
- `c8fe7f4d` stock globals/CVars; `192ca27e` all-learned spellbook indexing;
  `c08563f1` action-bar spell-ID queries; `023db579` error handler/debuglocals.
- `9795f199` empty QuestPOI native surface (inspect before calling it complete).
- `be0439df` quest dialogue reparenting/panel transitions/spellbook/actionbar updates.
- `a396546a` quest/gossip item lookup/icon fallback/player equipment resolution.
- Several modular hotpatch seam/quarantine repairs. Read the concurrent local handoff:
  `D:/Projects/project-b-client/.scratch/project-b/handoffs/2026-11-09-modular-hotpatch-closure.md`.

At final check, upstream-vs-local diff touched wrath.rs by 9 lines and did not list changes to
protocol-wrath/session.rs or net/motion/spline.rs. Still inspect actual latest source before
claiming the movement/transfer findings persist. Reconcile overlap in wrath.rs and PvP files.

## Diagnostics: exact locations, ages and limits

- `D:/Projects/project-b-client/benilla-config/Logs/Launcher/latest-diagnostics.md` and `.json`.
- Referenced launcher run: `benilla-config/Logs/Launcher/20261108-001747-093/`.
- Its `stderr.log` contains stock/addon Lua errors, quest deliveries and interaction logging.
- Latest retained interaction file observed:
  `benilla-config/Logs/InteractionDiagnostics/interaction-1794086638160674900.jsonl`.
  Previous: `interaction-1794086489820518400.jsonl`.
- Latest diagnostic generated Nov 8 ~00:24 Moscow; recorded build SHA
  `93c0310f0b6ba36838733435d56afcdc8b3a1c2b`, received 955 events, dropped 0, evicted 233.
  That is older than local HEAD and much older than latest upstream. Missing evidence can be due
  to eviction or unexercised route; errors from this snapshot do not prove current code still fails.
- Historical completion matrix:
  `.scratch/project-b/handoffs/2026-11-02-completion-matrix.md`.
  It is useful as a route/acceptance checklist, not a current bug list. Many fixes postdate it.

Two old armor hypotheses were explicitly retired as current leads after source inspection:
`entities/attach/redress.rs` now gates readiness before stamping AppliedEquipment;
`benilla-formats/src/characters/sections.rs::CompositePlan::run` now returns missing-layer reports,
and `benilla-app/src/entities/skin_composite.rs::work` logs them. Do not repeat the old claims
that stamping precedes all readiness checks or missing overlays are entirely silent.

## Suggested next-session sequence

1. Read instructions, revalidate branch/remotes/worktrees/processes; preserve seven-file diff.
2. Reconcile against latest upstream source without pulling under dirty main or taking another
   agent's worktree. Determine whether inspect changes need adaptation to merged OC target gates.
3. Report the chosen assignment boundary. If the owner assigns a larger repair, prefer either
   server-driven movement as one coherent subsystem or failed-transfer recovery as a bounded
   session-lifecycle fix. Do not substitute more unrelated tiny bindings.
4. Use reference/client/server authorities above before editing. Record gaps and inferred impact.
5. When testing is explicitly authorized, use focused cases for the selected mechanism and the
   original symptom; all Cargo uses sccache, coordinate one build/client slot. Owner drives gameplay.
6. Report source changes, actual checks, remaining limits and publication state separately.
   No runtime success, playable-alpha claim, commit, push or merge is implied by this handoff.
