# OC handoff — RustWrath playable route, 2026-11-01

Read `AGENTS.md`, `docs/METHOD.md`, `docs/agents/active-ownership.md`, and this file first.
Astra is out of use for now; the owner asked to continue with Gemini/Antigravity. The client is
human-driven only: no bots, GM shortcuts, quest resets, automated attacks/kills, server/DB/WM/
install changes, or credential changes.

## Repository and build state

- Worktree: `D:\Projects\project-b-client`, branch `wotlk`.
- HEAD: `5715e0b8` "Record the Gemini continuation handoff for the Wrath login route".
- Local is **ahead 3, behind 1** of `origin/wotlk`. The missing remote commit is `0fcb1bea`
  "Add web preview applet configuration and structure", an unrelated web-preview commit. Do not
  blind-pull: it adds web files that are currently untracked here. Integrate deliberately.
- Last successful build: `cargo build -p benilla`, 2026-11-01 22:46:31, binary SHA-256
  `D337A5A768A2EFBE698DB8553F90EEB47E1A9704C9A61F092CB2CC4419A1DDBF`.
- A client is running from that binary, PID `11660`, at the login screen. Launcher log:
  `benilla-config/Logs/Launcher/20261101-224646-937`.
- **The working tree is dirty and not yet built** after the latest edits (see "Uncommitted
  work" below). Build once before judging anything.

## What was verified this session

- The stock 3.3.5a login/realm/character-select path works:
  - realm wizard now loads (the `IsTournamentRealmCategory` fix);
  - the old modal `Sound_OutputDriverIndex` is gone;
  - character select is reached and an earlier run entered Northshire (`Dicetar` was later
    selected in Stranglethorn Vale as well).
- `cargo build -p benilla` passes with Astra's integrated login/roster work.
- Focused `UnitSelectionColor` test passes.
- A live run exposed the remaining blockers below.

## Live log findings (latest client)

Log: `benilla-config/Logs/Launcher/20261101-224646-937/stderr.log`.

### 1. "Can't throw items from the bags" — exact cause found

At 19:47:49–50 UTC the log repeats:

```
Interface\FrameXML\ContainerFrame.lua:697: attempt to call global `GetCursorInfo' (a nil value)
```

This is the bag item click/drag path dying before `PickupContainerItem`. `GetCursorInfo` is not
registered. It needs the 1.12 shape (`type, id, ...`): `"item"` with the item id, `"money"` with
copper, `"spell"` with spell id, `"merchant"`/`"action"` as applicable. The client already has
the cursor payload state in `benilla-ui/src/script/cursor.rs` (`CursorPayload`, `cursor_item`,
`GetCursorInfo`-like helpers); register the global and add a focused test that a bag item click
reaches `PickupContainerItem`/`DELETE_ITEM_CONFIRM`. This is the highest-value fix.

### 2. "Can't accept Kobold Camp Cleanup" — not yet reproduced with debug

The earlier Firna session was level 2 in Northshire; the log later shows `Dicetar` at level 36
in Stranglethorn Vale. No `ui_quest: accept quest` debug line was captured, so the Accept click
probably never reaches `QuestVerbs::accept` (`crates/benilla-app/src/ui_quest/verbs.rs:136`).
Relaunch with
`RUST_LOG=benilla_app::ui_quest=debug,benilla_app::ui_script=warn`, open the quest, click
Accept, then grep the log for `ui_quest: accept quest`:
- If absent: inspect the stock `QuestFrame` Accept button/`AcceptQuest()` call and any missing
  global on that path. Add focused UI test.
- If present: the packet was sent; inspect `ClientCommand::QuestgiverAccept`/Wrath opcode and
  server response (`SMSG_QUESTGIVER_QUEST_DETAILS`/`QUEST_ACCEPTED`).
Do not reset or modify Dicetar or his quests.

### 3. Login menu errors — one modal remains

The current binary still shows a modal at login:

```
Interface\GlueXML\OptionsPanelTemplates.lua:200: Couldn't find CVar named 'Sound_NumChannels'
```

The **uncommitted** `tolerates_unknown_cvars` change in
`crates/benilla-ui/src/script/cvars.rs` should fix this: the glue VM (and Wrath target) now
returns `"0"` for unknown `GetCVar`/`GetCVarDefault` and accepts unknown `SetCVar` instead of
raising. It is **not built yet**. Build and relaunch, then check for another modal; the options
panels may name further 3.3.5 CVars. Prefer the glue-only tolerance over adding hundreds of fake
rows.

Other log noise still present (non-modal, lower priority):
`GetNumArenaOpponents`, `CanExitVehicle`, `GetNumCompanions`, `IsPetAttackAction`,
`GetNumTitles`, `WatchFrame.lua:1306 'for' limit must be a number`,
`SetHyperlinkCompareItem` (a stub was added), `UnitSelectionColor` (fixed and built).
`GetNumTitles` wants a 0/nil stub in `wrath_natives.rs`.

### 4. Creature frame "not correct" — target frame showed "Dead" on a live tiger

The screenshot showed a live Young Stranglethorn Tiger with a full green bar but a stale
"Dead" label. The repeated `UnitSelectionColor` error was aborting `TargetFrame_Update` before
the dead label refreshed. `UnitSelectionColor` is now implemented in
`crates/benilla-ui/src/script/unit/relations.rs` and registered in `unit/bindings.rs`; the
focused test passes and the binary includes it. **Verify in the fresh build** by targeting a
live wolf/tiger: the dead label must clear and the bar must take the reaction color. If it
still says Dead, inspect `UnitIsDead`/`unit_reads_dead` and the Wrath `UNIT_DYNAMIC_FLAGS`
projection (`wrath.rs` writes `remote.dynamic_flags` to 1.12 field 143).

## Uncommitted work (build before commit)

Seven files are modified and **not built**:

- `crates/benilla-app/src/cvars/table.rs` — added 3.3.5 CVars
  (`Sound_OutputDriverIndex`, `showKeyring`, `autoQuestWatch`, `questFadingDisable`,
  `playerStatLeft/RightDropdown`, `xpBarText`, `extShadowQuality`, `gxStereoConvergence`,
  `gxStereoEnabled`, `Sound_EnableSFX`, `Sound_EnableAllSound`, `windowResizeLock`,
  `autojoinPartyVoice`, `autojoinBGVoice`, `environmentDetail`, `groundEffectDensity`,
  `groundEffectDist`, `gxStereoSeparation`, `Sound_EnableErrorSpeech`,
  `Sound_EnableEmoteSounds`, `Sound_EnablePetSounds`).
- `crates/benilla-ui/src/script/cvars.rs` — glue/Wrath unknown-CVar tolerance
  (`tolerates_unknown_cvars`), unconditional nil `GetCVarMin/Max` for unknown names.
- `crates/benilla-ui/src/script/glue_select.rs` — non-Wrath `GetSelectBackgroundModel`
  fallback; silent `RequestRealmSplitInfo`/`SetRealmSplitState`.
- `crates/benilla-ui/src/script/tooltip_item/mod.rs` — `SetHyperlinkCompareItem` nil stub.
- `crates/benilla-ui/src/script/unit/bindings.rs` — `UnitSelectionColor` registration.
- `crates/benilla-ui/src/script/unit/relations.rs` — `unit_selection_color` + focused test.
- `crates/benilla-ui/src/script/wrath_natives.rs` — `GetCurrentTitle`,
  `IsPlayerResolutionAvailable`, `IsActiveBattlefieldArena`, `GetNumTrackedAchievements` stubs.

After a successful build and a clean login/target/bag check, commit these separately from the
three already-pushed commits. Do not stage `FETCH_HEAD` or the untracked web-preview files
(`.aistudio/`, `package.json`, `node_modules/`, `src/`, `public/`, `vite.config.ts`, etc.).

## AutoSwarm / Gemini status

The owner authorized connecting Gemini in Antigravity to AutoSwarm. Findings:

- Antigravity is a VS Code fork; `antigravity-ide` only opens files/merges, and
  `language_server.exe` is its internal Gemini HTTP service, not an ACP child. AutoSwarm cannot
  call the GUI directly.
- The official Gemini CLI is already installed at
  `C:\Users\Qraag\AppData\Roaming\npm\gemini.cmd`, version `0.57.0`, and supports `--acp`.
  A direct call `gemini -p "Reply with exactly OK"` returned `OK`, so auth works.
- AutoSwarm was patched:
  - `C:\Users\Qraag\.config\opencode\skills\autoswarm\scripts\runtime\autoswarm\providers.py`
    now lists `gemini` in `BINARIES` and `ACP_PROVIDERS` (ACP invocation is `gemini --acp`).
  - `C:\Users\Qraag\.autoswarm\config.json` has a `providers.gemini` entry pointing at the
    absolute npm shim.
- `agents` after the patch returned an empty list in this session; debug the launcher/config
  first, then run:
  `doctor --agent gemini --probe --cwd D:\Projects\project-b-client`.
  If it verifies, run the continuation prompt
  `.scratch/project-b/handoffs/2026-11-01-gemini-continuation.md` through
  `run --agent gemini --input <file> --cwd ...`.
- Do not remove the opencode provider config; preserve it.

## Immediate next-session order

1. Check no `cargo`/`rustc`/`benilla` build is running (a client PID 11660 may be at login).
2. `cargo build -p benilla` with the seven dirty files; fix only compile breaks first.
3. Relaunch via `Launch RustWrath.cmd` / `tools\launch-client.ps1` with
   `RUST_LOG=benilla_app::ui_quest=debug,benilla_app::ui_action=debug,benilla_app::ui_script=warn,
   benilla_app::wrath=info,benilla_app::target=info`.
4. Verify the login screen has no modal. If a new `Couldn't find CVar` appears, decide between
   one more real row or the glue tolerance (already built next time).
5. Implement `GetCursorInfo` and test the bag drop by both click-then-world-click and drag-release.
6. Reproduce the Kobold accept click; use the debug line to tell UI vs packet.
7. Target a live tiger/wolf and verify the target frame is not "Dead"; then confirm action bar,
   bags, loot and casts remain usable.
8. Commit the verified fixes (not the web preview), record evidence, then hand the remaining
   work to Gemini with the prompt file.

## Environment notes

- Cargo is installed through scoop at `C:\Users\Qraag\scoop\shims\cargo.exe`; it is not in
  `C:\Users\Qraag\.cargo\bin` (that only has `sccache.exe`). In Git Bash use
  `export PATH="/c/Users/Qraag/scoop/shims:$PATH"`.
- One Cargo build at a time. The client uses `WOW_DATA` from the launcher/shadow root; do not
  move or modify the WoW install.
