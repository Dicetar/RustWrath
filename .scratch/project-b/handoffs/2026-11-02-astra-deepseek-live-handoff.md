# Astra → DeepSeek: live continuation checkpoint

## Latest recovery outcome — authoritative over historical checkpoints below

- Both movement regressions PASS (2/2), log `test-2026-11-02-astra-pose-final.log`.
- `cargo build -p benilla --features hotpatch` PASS in3m04, warnings only, log `build-2026-11-02-astra-pose.log`. Cargo slot released.
- Executable SHA256 **1D853A073F916C1CBD952FA21E36ACB6BA960C29486B919CD2F6A7A13B3B43BC**.
- Normal launcher run **20261102-222346-059** started. Computer Use screenshot shows login controls, no modal at this observation, black background. No credentials entered by agent. Startup snapshot does not establish complete UI or in-world movement acceptance.
- Source changes in this continuation: `wrath.rs` consumes cached self positions once; `player/state.rs` applies ordinary pose correction without initiating mover handover. Existing dirty changes preserved; no commits/pushes.
- DeepSeek completed and corrected its support reports, then acknowledged this handoff via `call_b5c695e824dc4b9fae08f5e554546b1f`. It is idle. The LFG implementation prompt is prepared, not assigned for execution.
- Owner subsequently logged in: log records entered world0/player0x1522 at19:24:20 UTC; Computer Use screenshot confirms Firna in Northshire Valley targeting a Young Wolf with owner-driven combat, world/actionbar/minimap visible. Agent made no gameplay inputs. Current PID43304. Do not interrupt or launch competing client. Roster/create-cancel/logout were not observed. New run still reproduces WatchFrame1306 and missing GetLFGDeserterExpiration. This launch's RUST_LOG did not enable player::wire_in info, so absence of repeated take-control lines is NOT proof of the movement fix. Obtain owner assessment or repeat with that log target before runtime closure.
- No claim of full working alpha, no claim quota reached10%. Last reported quota57%. This recovery closes the bounded movement source/test/build task; broader runtime and remaining feature work stays open.

This is an in-progress handoff, not a completion claim. User requested sustained implementation with easy work delegated to OpenCode DeepSeek, then detailed handoff near 10% quota. Astra cannot observe account quota; no threshold claim is made.

Owner supplied quota reading 57% during this continuation. Continue; do not treat elapsed time/token estimates as a 10% reading.

## Ownership and tool state

Repo D:/Projects/project-b-client, HEAD at admission ab89c6fc, many existing dirty changes preserved. Read AGENTS.md, docs/METHOD.md, docs/agents/workflow.md, active-ownership.md. Reference pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a in D:/Projects/wowdecomp-reference; read-only behavioral research.

AutoSwarm launcher: C:/Users/Qraag/AppData/Roaming/uv/tools/autoswarm/Scripts/python.exe C:/Users/Qraag/.agents/skills/autoswarm/scripts/launch.py --home C:/Users/Qraag/.autoswarm --controller main_52e8b2e6f3ca1bdfc3454dea5da6520e.

Support call call_24773bde2ded47c0a3423ea012d6e9b3, native ses_e627c096effebaD67lG7P0Sgry, model opencode-go/deepseek-v4.1-flash. ACK received. Owns only completion-matrix and cvar-backend-review scratch reports specified in 2026-11-02-deepseek-support.md. No production edits/build/client. Collect its results rather than duplicating.

Manual implementation session ses_e66c07eabffe4dmCRaDPhyLMrl final inspected: reported batch complete and A140 build tested. That is older than current binary/runtime. No Cargo/rustc/benilla at admission. Astra now owns integration and the one Cargo slot.

## Newest runtime baseline

Run benilla-config/Logs/Launcher/20261102-210022-913, binary SHA256 15CC91ABD9EF5918D877201BA0F783F87EB7940FEDB217C8E4DAD909B9FD611B. Client was closed at admission. Log shows gameplay and server chat, repeated 'took control of player', LFDFrame.lua:955 missing GetLFGDeserterExpiration, CharacterFrame.lua:28 nil indexing, PaperDollFrame.lua:290 missing GetCritChanceFromAgility. It also contains an old donor 'NET OFF' preflight warning despite later real Wrath server activity; do not treat that warning alone as invalidating the Wrath connection.

## Astra's current source change

Only production edit this continuation: crates/benilla-app/src/wrath.rs, self-position branch of wrath_apply_world. The reduced entity snapshot retains position across unrelated field updates. At rest, the old code compared it to locally settled position and repeatedly called Player::apply_server_pose, setting reseat and resetting movement state on the next controller tick. Remote units already carry WrathAppliedPosition; self lacked the freshness gate.

Added consume_wrath_position(world, entity, pose), storing the consumed position. Self correction only considers fresh poses. Consume even during local movement so an ignored stale pose cannot be applied when movement later stops. Spawn already stamps initial pose; replacement entity has independent state. Existing movement flag/rest and distance gates remain. This fixes a concrete snapshot-replay path, not yet demonstrated elimination of every possible reseat source (client control messages and embodiment also reseat).

Added focused test self_pose_is_not_replayed_by_unrelated_snapshots covering spawn duplicate, new position once, revisit earlier position, replacement entity. This is receipt/freshness evidence, not a full movement runtime test.

## Current verification

Final movement regression result: PASS, 2/2 tests (`self_pose_` filter), 7m29 compile; exec3693 completed exit0. Log `.scratch/project-b/test-2026-11-02-astra-pose-final.log`. Integration build started next: `cargo build -p benilla --features hotpatch`, wrapper sccache, log `.scratch/project-b/build-2026-11-02-astra-pose.log`. No live test yet. Earlier pending statements below are historical checkpoints.

Recovery update after owner reported unexpected error: first regression PASSED (1 case), 20m33 compile, exec49970 completed exit0. Then implemented the second correction in player/state.rs: apply_server_pose no longer raises reseat; existing pending handover remains untouched. Added self_pose_correction_preserves_mover_ownership_and_modes. Final targeted run now exec3693: cargo test -p benilla-app --lib self_pose_, log test-2026-11-02-astra-pose-final.log, pending at this update. git diff --check on both edited files passes. Do not confuse first test pass with verification of the second edit.

DeepSeek support call returned completed; both report artifacts exist. They identify persisted sound/filter alias conflicts, missing sound master/SFX consumers, out-of-range filtering, missing CVar/API errors and current evidence gaps. Feedback requested correction of any inference of visible UI from logs alone; check final artifact wording.

Tracker root cause confirmed: selected profile's highest-precedence copy found in enus/patch-enus-3.mpq, read-only extraction `.scratch/project-b/verification/astra-WatchFrame.lua` (never commit game Lua). Line1306 loops to QuestMapUpdateAllQuests() result. script/worldmap.rs:798 returns no values. Reference QuestLog_Scripts.cpp::Script_QuestMapUpdateAllQuests 0x005E63D0 returns numeric count from visible POI data, QuestLog.cpp::UpdateAllQuestPOI 0x005E5A50 filters by map/objective status. No QUEST_POI pipeline found in protocol-wrath. Do not falsely close this with an unconditional zero.

Next easy implementation is prepared in `2026-11-02-deepseek-lfg.md` (NOT launched): real aura-backed GetLFGDeserterExpiration and adjacent random cooldown API, only wrath_natives.rs. Use after shared compile/source slot release. User's recovery request is to finish current work; no claim quota reached10% (last actual reading57%).

Running at checkpoint: exec session 49970, command cargo test -p benilla-app --lib self_pose_is_not_replayed_by_unrelated_snapshots, RUSTC_WRAPPER C:/Users/Qraag/.cargo/bin/sccache.exe; log .scratch/project-b/test-2026-11-02-astra-pose.log. Last seen compiling benilla-world; no final result yet. Inspect actual processes before any next Cargo command. Do not edit sources during this build. No new runtime acceptance.

## Next actions

1. Collect focused test outcome; fix actual compile/test errors. Review pose freshness behavior against caller semantics before integration build.
   Additional causal finding (not yet edited while test compiles): Player::apply_server_pose in player/state.rs sets reseat=true. player/controller.rs passes the old body Transform to wire_in::apply_server_moves; reseat reads that transform back into player.pos and resets modes/velocity/autorun. Ordinary authoritative position correction is not a mover handover. Remove the reseat assignment from apply_server_pose, update its misleading camera comment, and add a regression proving an already-active corrected player does not request handover or lose existing modes. Leave login/possession reseat paths intact. Reference Unit_C.cpp::SetActiveMover 0x00729010 returns early for unchanged mover GUID; this supports keeping mover changes separate from position correction.
2. Collect DeepSeek reports. Then build once with hotpatch (existing accepted launch profile), using one Cargo slot; record hash and log.
3. Launch through tools/launch-client.ps1, no competing client. Owner enters credentials and drives combat. Inspect visible login/roster/create-cancel/Enter World, then observe repeated reseat symptom under actual movement and field updates. No automated account credentials, combat, persistent character creation/deletion.
4. Quest command admission remains an identified architectural gap: ui_quest/verbs.rs ignores send errors then sets acted/tears down. No change made yet. Make bounded repair with disconnected-channel regression before claiming solved. Do not mistake it for proven cause of the historic McBride click failure.
5. Prioritize current visible PaperDoll and LFD failures using real 12340 contracts and backing model data, not more constant stubs. Existing 16-task prompt is guidance, not current evidence.

Preserve all dirty work, no commits/pushes/cleanup. Keep runtime, source, test and compilation evidence distinct. Existing bounded <=20 essential cases, owner-driven gameplay, install/server/DB read-only constraints apply.
