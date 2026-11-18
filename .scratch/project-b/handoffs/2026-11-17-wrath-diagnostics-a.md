# Wrath runtime-diagnostic fixes: tooltip width, PvP timer, options CVars

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/wrath-diagnostics-a
Issue: none
Next: owner batch build; then the three checks below. Source review is complete; the branch merged `origin/main` at `80c34441`, and no Cargo, test or client run was made.

Started from the owner's runtime diagnostics rather than a report. The latest populated run is
`benilla-config/Logs/Launcher/20261117-161650-231` (generated 2026-11-17 19:49); the newer
`20261117-223136-171` contains only `launch.txt`, so it produced no client stderr to read.
`latest-diagnostics.md` and `latest-tickets.md` classify 9 ready tickets.

## Findings taken

- `GameTooltip.lua:135` calls `frame:GetMinimumWidth()` in `GameTooltip_AddMoneyLine`; the
  method was never registered on the GameTooltip method table, so every money tooltip raised
  (24+ logged hits, and the error handler itself failed 25 times). The reference implements it
  beside `SetMinimumWidth` (`Tooltip_Scripts.cpp:101`, 12340 `0x0061D0D0`) and also stores the
  second `force` argument (`0x0061D040`) that the clear respects (`Tooltip.cpp:986`).
- `PlayerFrame.lua:152` calls `IsPVPTimerRunning()`; the 12340 global reads
  `PLAYER_FLAGS_PVP_TIMER` (0x40000) off the active player and pushes 1/nil
  (`GameUI_Scripts.cpp:1001`). Its true branch immediately calls `GetPVPTimer()`
  (`PlayerFrame.lua:154`, `:237`), which was also unbound: `GetPVPTimer` is
  `GetPVPToggleTimer() + 1000` (`GameUI_Scripts.cpp:834`, `Player_C.cpp:2230-2241`), 0 without a
  player. Binding only `IsPVPTimerRunning` would have moved the error to the next line.
- `Blizzard_TimeManager.lua:89`, `InterfaceOptionsPanels.lua` and
  `InterfaceOptionsFrame.lua` raised `Couldn't find CVar named` for five 12340 options rows:
  `timeMgrUseMilitaryTime`, `fctRepChanges`, `fctFriendlyHealers`, `showArenaEnemyPets` and
  `UnitNameEnemyPlayerName`. Their registered defaults are in `GameUI_CVar.cpp:539/541/564/628`
  and `PlayerName_CVar.cpp:35`; none has a benilla consumer, so they belong in the inert table.

## Source change

- `TooltipState` gains `min_width_force`; `SetMinimumWidth` reads its second argument with
  `StringToBOOL` semantics and stores it; `GetMinimumWidth` returns the floor and 1/nil;
  the clear keeps a forced floor and drops an unforced one.
- `wrath_natives.rs` binds `IsPVPTimerRunning` off the loaded player's flags; the first true
  observation stamps `Model::pvp_timer_start` on the UI clock. `GetPVPTimer` returns 0 without a
  player, 301000 ms with the flag off, and the remaining window plus the reference's 1000 ms pad
  while set (999 once expired). The stock `PlayerFrame` decrements the value itself
  (`PlayerFrame.lua:413`).
- `cvars/inert.rs` registers the five options rows at their reference defaults, so
  `GetCVar`/`SetCVar`/the options panels stop erroring while the UI can still save them.
  `timeMgrUseMilitaryTime` uses `same_by_locale` ("0" on enUS/koKR/esMX, "1" elsewhere), not a
  single enUS default.

## Focused cases (written, not run)

1. `tooltip_size::minimum_width_and_padding_read_their_arguments_as_the_reference` now also
   reads `GetMinimumWidth` before and after a forced floor, and checks the floor survives
   `ClearLines`.
2. `cvars::tests::the_wrath_options_rows_logged_missing_are_registered` checks `get` and
   `default_of` for the five names.
3. `wrath_natives::tests::wrath_ui_contracts_read_real_player_pet_and_title_state` now checks
   `IsPVPTimerRunning` nil without the bit and 1 with `0x40000`, plus `GetPVPTimer`: 301000 with
   the flag off, 301000 at the first true observation, and 291000 after ten UI seconds.

Three focused cases (two extended); nothing else was added or run.

## Named limits / deferred findings

- `SetMinimumWidth(width, force)` still sizes as a floor in this sizer; the 12340 branch that
  skips the per-line width pass under `force` is not implemented. No stock caller reads the
  second return today.
- The PvP timer uses the UI session clock (`Model.now`), not the reference's real-time
  `FrameTime`; it resets on a reload. The stock frame only asks once per flag change and then
  decrements the value itself, so a pause can stretch the displayed window.
- `UnitNameEnemyPlayerName` is registered but inert; the reference's `ObjectNameShowCallback`
  mask is not projected, so the checkbox stops erroring without changing name rendering.
- `GetUnitHealthRegenRateFromSpirit` (`PaperDollFrame.lua:309`, x2) is deferred: the reference
  needs `gtOCTRegenHP`/`gtRegenHPPerSpt` game-table records and the unit's spirit stat, neither
  of which is projected into `UnitState` yet. A constant 0 would hide the error with a false
  sheet value.
- `GetPreviousArenaSeason` (world state 3901) remains an unsupported branch;
  `GetCurrentArenaSeason` also still warns for lack of arena-season state.
- `UIPanelTemplates.lua:365 cursorOffset` is an edit-box event-order issue (the error only
  appears inside the error handler), not diagnosed far enough to change safely here.
- `VoiceChatSpeakerTemplate` is expected: `VoiceChat.xml` is permanently skipped by the owner's
  decision (`reference_ui.rs` `WRATH_SKIPPED_FILES`), and the loader test already tolerates it.
- `GetBackpackCurrencyInfo`, the CharacterCreate textures, the DBC/asset load failures and the
  7.2 s frame hitch are untouched; they are separate tickets/families.

## Owner checks

1. Batch build with sccache and the three focused cases.
2. Hover an item or money row and confirm no `GetMinimumWidth` Lua error; open the Time Manager,
   Interface options, and Arena options panels and confirm no `Couldn't find CVar named` errors.
3. Put the character in the PvP timer state (disable PvP and wait for the 5-minute timer) and
   confirm the stock PlayerFrame shows the countdown with no `GetPVPTimer` error; also confirm
   the timer disappears when the flag clears.
