# Astra continuation: login route, runtime acceptance, AutoSwarm

## Bootstrap and constraints

Work in `D:/Projects/project-b-client`, `wotlk`, GitHub `Dicetar/RustWrath`.
Read AGENTS.md, docs/METHOD.md, CONTEXT.md, docs/agents/active-ownership.md,
docs/agents/12340-reference.md, then this handoff and
`2026-11-01-oc-next-session.md` beside it. Revalidate all state before edits.
Reference checkout: `D:/Projects/wowdecomp-reference`, pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`; behavioral evidence only, no copied implementation.

The owner wants working visible software, not another large test collection. Existing bounded
build/test/live-check authorization persists; keep checks targeted and <=20 essential cases.
Only one Cargo build at once. Never launch a competing client. Do not automate credentials,
combat, character deletion, or change the game install, server, DB, or credentials. Retain at
most three backup generations; do not copy target or game assets. Preserve other agents' edits.

## Latest revalidation at handoff

- HEAD `5715e0b8ed74f97323e64f25d214d644332f6709`, docs continuation commit.
- Parent code checkpoint `6890d9d9` includes the earlier uncommitted login/UI work.
- Seven dirty tracked files now: app `cvars/table.rs`; UI `script/cvars.rs`,
  `script/glue_select.rs`, `script/tooltip_item/mod.rs`, `script/unit/bindings.rs`,
  `script/unit/relations.rs`, `script/wrath_natives.rs`. These are concurrent OC work,
  not a license to revert or indiscriminately commit them.
- Untracked: `.scratch/project-b/build-2026-11-01-oc2.log`,
  `.scratch/project-b/test-wrath-natives-oc2.log`,
  `.scratch/project-b/handoffs/2026-11-01-oc-next-session.md`, root `FETCH_HEAD`.
  Preserve evidence. Do not stage root FETCH_HEAD.
- Current binary `target/debug/benilla.exe`, mtime 2026-11-01 23:02:47 Moscow,
  SHA256 `D9EB9F3FAC24CA0D9EBA6B1F8562C35F2A01AB9EADF578B1225A2CF159C26B24`.
  Do not infer exact dirty-source identity from mtime/hash alone.
- OC build log reports successful `cargo build`, 8m31s. Astra's older
  `astra-glue-followup-build.log` FAILED on two MerchantItem literals missing
  `extended_cost`; this older failure is not the verdict on the newer OC binary.
- At handoff Cargo PIDs 1844/11760 and rustc PIDs 348/35492 were present, with command
  lines unavailable to this tool process. No benilla process was listed. Do NOT kill or
  start another build based on stale assumptions; identify/recheck the live owner first.
- No final rendered acceptance was performed by Astra after the screenshot follow-up.

## User's reported failures and established causes

Screenshots: login Sound_OutputDriverIndex modal; roster SetGlow modal and cannot switch;
creation strupper(nil), broken layout; Enter World leaves black screen.
Baseline log: `benilla-config/Logs/Launcher/20261101-204232-720/stderr.log`.

Implemented in the checkpoint, but revalidate current source and runtime:
1. Register Wrath CVars before GlueXML OnLoad, not only in-game. Add referenced sound,
   stereo/window defaults. Wrath CVar min/max getters; shadow maximum remains a capability gap.
2. Wrath SetGlow, ResetLights, AddLight/AddCharacterLight/AddPetLight retain six light sets.
   Glow feeds real GlueFfx state. Custom background lights feed the scene GPU buffer.
   Character and pet still share the stage lighting buffer: independent custom-set consumption
   is unfinished. Do not claim full lighting parity.
3. Faction.dbc 57-field and FactionGroup 20-field readers, restoring the otherwise empty
   character-creation catalog; LoadingScreens.dbc four-field reader, restoring loading art.
4. Wrath race/class triples, six selected-class returns, expansion gates from DBC metadata,
   faction index, sex enum and global class-index mapping. Donor paths preserved.
   The class ordering currently uses sorted IDs; verify file-order fidelity on selected data.
5. The Wrath EnteredWorld edge happens AFTER self creation and authoritative placement.
   Loading screen waited forever for a later Worldport that never occurs. Stop awaiting that
   extra snap on this edge; retain residency, settling, UI and pipeline warm-up gates.

Reference anchors and fuller source reasoning are in `2026-11-01-astra-login-ui.md`.
Selected-profile probe log: `.scratch/project-b/verification/2026-10-28-oc-alpha/astra-glue-followup-profile.log`.
The owner screenshots confirm backdrops and borders now render; they do not prove UI flow.

## Concurrent OC changes requiring review

Read OC's new handoff in full. It describes seven dirty files including unknown-CVar tolerance,
silent RequestRealmSplitInfo/SetRealmSplitState, and several nil/constant API stubs.
Do not treat suppressing an error as implementing a native operation. For example,
RequestRealmSplitInfo native 0x004DE2E0 sends CMSG_REALM_SPLIT 0x38C with u32::MAX;
its server response/state is not established by returning nil. Likewise review
SetHyperlinkCompareItem and title/battlefield helpers for meaningful runtime behavior.
Preserve these edits while coordinating corrections; do not undo another owner silently.
OC's handoff also names bag cursor/drop, Kobold quest accept, live target Dead state,
action bars, loot and casts as next acceptance frontiers. Combat remains human-driven.

## AutoSwarm: use it to coordinate, do not make the user relay ownership questions

Read `C:/Users/Qraag/.agents/skills/autoswarm/SKILL.md`.
Exact Codex launcher (append arguments):

    & 'C:/Users/Qraag/AppData/Roaming/uv/tools/autoswarm/Scripts/python.exe' 'C:/Users/Qraag/.agents/skills/autoswarm/scripts/launch.py' --home C:/Users/Qraag/.autoswarm

Shared home `C:/Users/Qraag/.autoswarm` avoids Codex MSIX AppData redirection.
OpenCode v2 executable: `D:/Projects/opencode-v2-test/node_modules/.bin/opencode2.cmd`,
v2.2.5. Do NOT use the v1 CLI under local-coding-engine/integrations/opencode.
Configured model: `opencode-go/deepseek-v4.1-flash` (owner chose OpenCode Go, not the free model).
Config sets opencode `transport: acp`, `plugin_api: v2`, and model args.
Real initial reply AND native resume passed in `D:/Projects/project-b-opencode`.
OpenCode plugin `~/.config/opencode/plugins/autoswarm.js` was verified ACTIVE through v2 API.
It registers a real autoswarm tool. Results use explicit wait; no automatic callbacks.

AutoSwarm source `D:/Projects/AutoSwarm` has local compatibility patches in providers.py,
native.py, setup.py and assets/opencode-v2.js; keep them. Both installed skills have copied
runtimes: editing the checkout or one copied runtime alone does not update all hosts.
After a deliberate runtime update, reinstall package and refresh BOTH host setups.

OC's newest handoff says it added Gemini CLI to ONLY its installed runtime and added a
providers.gemini config entry. Treat Gemini integration as UNVERIFIED and potentially
inconsistent across copies. Do not assume an Antigravity GUI session equals a Gemini CLI
session or has the same model/quota. Owner's occasional preference is Gemini 3.8 Flash
from Antigravity. The existing OpenCode catalog exposes
`google/antigravity-gemini-3.8-flash`; exact AutoSwarm invocation through that route has
not been verified. Preserve DeepSeek as OC's default.

Run `calls` and inspect relevant retained tasks first. At the preceding verification, tracked
records were setup probes, not managed implementation tasks. AutoSwarm does not magically
discover or control every manually opened OC/Gemini chat. Use known native bindings or create
properly scoped managed tasks, retain call IDs and collect with wait. Never start a duplicate
implementation merely because another agent is not listed. Process state and active-ownership
remain necessary checks. The owner's instruction is to coordinate agents ourselves.

## Next actions

1. Revalidate HEAD, dirty diffs, processes, build logs and tracked AutoSwarm calls. Collect
   current OC work through the available communication/session mechanism; respect an active
   build. Correct stale build reservations in active-ownership after identifying the owner.
2. Review the seven dirty API changes for actual contracts, especially broad CVar tolerance
   and no-op realm split behavior. Agree distinct files/tasks through AutoSwarm; one build owner.
3. Finish any compilation repair, then rebuild only if the latest binary lacks final changes.
   Use `$env:RUSTC_WRAPPER='C:/Users/Qraag/.cargo/bin/sccache.exe'`; no speedup claim without hits.
4. Launch through `Launch RustWrath.cmd` or `tools/launch-client.ps1` only when no client/build
   runs. The launcher clears probe credentials/deadlines, uses local server and stock UI,
   captures logs and retains three runs. Owner enters credentials.
5. Check login without modal; select Firna AND another row; creation race/class/sex navigation
   and cancellation without creating/deleting persistent characters; Enter World cover clears.
   Record the actual binary hash and exact runtime results. Do not substitute parser tests.
6. Continue the owner-driven Firna Northshire quest/combat/loot route. Ask the owner to take
   over combat when the noncombat preflight is visibly ready. No playable-alpha claim yet.

This handoff does not commit/push source, launch/stop a client, or terminate another build.
