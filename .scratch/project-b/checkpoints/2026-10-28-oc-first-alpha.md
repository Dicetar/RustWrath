# OC — first-alpha execution, 2026-10-28

Status: bounded source/verification cycle. Base HEAD
`2cefd8a50920b0ca9c697b335561511663b6b940`; the two fixes below are committed at
`40b698facd768220d6e0d1e855d27bbf36fb12d9`. Tracked tree and index clean at start.
No server/DB/install/WM operation.

## Claims

- `crates/benilla-world/src/worldview.rs` — honor the same `WM_ASSET_MANIFEST` /
  `WM_ASSET_PROFILE` profile source the client uses, so the render check reads the selected
  Warcraft install profile instead of the 1.12 discovery chain.
- `crates/benilla-wdt/src/lib.rs` — accept a trailing partial chunk header (the selected
  `Azeroth.wdt` ends with an 8-byte zero-length `MWMO` and 3 padding bytes), with one focused
  existing-module regression case.
- `crates/benilla-app/src/ui_script/extract/mod.rs` - repair the connected-client B0002:
  `tick_script` holds `ResMut<UiClock>` while `HostedVm` lends a `Res<UiClock>` in the same
  system. Move the clock write behind a `ParamSet` boundary so the host and the mutable clock
  are never borrowed together; the clock still updates before the paint pass.
- `crates/benilla-ui/src/script/keybind.rs` - add 3.3.5's `GetBindingByKey` as the same
  key-to-action lookup as `GetBindingAction`, so `UIParent.lua`'s `GetBindingFromClick` no
  longer raises when the world map handles a key.
- `crates/benilla-ui/src/script/worldmap.rs` - define the Wrath map update's
  `GetWintergraspWaitTime` (nil) and `GetDeathReleasePosition` (0, 0) so
  `WorldMapFrame_OnUpdate` hides its unavailable Wintergrasp/death-release surfaces instead of
  raising every frame, plus a no-op `InitWorldMapPing` and the dungeon/zoom globals
  (`GetCurrentMapDungeonLevel`, `DungeonUsesTerrainMap`, `IsZoomOutAvailable`) so
  `WorldMapFrame_OnLoad` reaches and completes `WorldMapFrame_Update`.
- `crates/benilla-app/src/ui_script/world_map_tests.rs` - focused regression for those two
  globals.
- this checkpoint.

### 2026-10-29 Wrath unit-token extension

- `crates/benilla-ui/src/script/unit/resolve.rs`, `unit/mod.rs`, `unit/bindings.rs`,
  `unit/relations.rs`, `script/model.rs` - Wrath-only `focus`, `vehicle`, `boss1..boss4`
  parsing/resolution and `UnitGuids` host fields; vanilla parsing and raises stay unchanged.
- `crates/benilla-ui/src/script/wrath_unit.rs`, `wrath_spell.rs`, `wrath_aura.rs`,
  `crates/benilla-app/src/ui_aura/units.rs`, `ui_unit.rs` - use the VM's Wrath target for
  token admission and seed that mode in the aura feed; `ui_unit.rs` also carries the
  Wrath-specific focus/vehicle/boss state for the production target/assist resolver.
- `crates/benilla-ui/src/script/{char_stats,guild,inspect,pvp,quest_log,tooltip_item,
  tooltip_unit,trade}.rs` - route their existing unit-token checks through the VM's target
  contract; no vanilla behavior changes.
- `crates/benilla-ui/src/script/wrath_unit.rs` - make `UnitGUID` answer the resolver's
  authoritative identity even before a `UnitState` snapshot exists; `wrath_natives.rs` -
  Wrath `UnitGUID` and no-op vehicle pitch handlers; `script/worldmap.rs` -
  `HasDebugZoneMap`/`QuestMapUpdateAllQuests`; this checkpoint.
- `crates/benilla-app/src/ui_script/lifecycle.rs` - a two-row `WRATH_UI_CVARS` addition
  (`showTargetCastbar`, `showBattlefieldMinimap`). This is Astra's claimed file; the addition
  is limited to the startup CVar list and the rest of her changes are untouched.
- `crates/benilla-app/src/ui_script/wrath_ui_slice_tests.rs` - mark the smoke's host Wrath.

`focus`/`vehicle`/`bossN` are now recognised and resolve from `UnitGuids` when their host
fields are populated. The production feed still seeds those fields as zero because no
focus/vehicle/boss producer exists; that remains an explicit gap, not silently claimed support.

No other Astra-claimed file is edited.

## Observed evidence

- `cargo build -p benilla` at `2cefd8a5`: pass, 7m31s, dev binary
  `target/debug/benilla.exe`.
- Wrath launch with the run-card environment but no `WOW_USER`/`WOW_PASS`:
  `wrath: FATAL - WOW_WRATH needs WOW_USER: environment variable not found`, exit 1.
  No `.probe-identity` exists at the repo root and no dedicated credential variables are present.
  This is the live-route blocker.
- Stock Wrath UI smoke with real `wm-hd` data: pass (1 test).
- `benilla-protocol-wrath` lib tests: 30 passed.
- `benilla-protocol` loot tests: 16 passed.
- First worldview run (base discovery chain): `Azeroth.wdt` failed with
  `truncated WDT chunk header`; every WMO path was reported `Path not found`. Screenshot
  `.scratch/project-b/verification/2026-10-28-oc-alpha/northshire-worldview.png`.
- After `40b698fa`, `cargo test -p benilla-wdt --lib`: 15 passed (including the new
  trailing-padding case). `cargo build -p benilla-worldview` and `cargo build -p benilla`:
  pass. `cargo test -p benilla-protocol-wrath --lib`: 30 passed.
- Profile-correct worldview at Northshire: pass, exit 0, no WDT error and no `Path not found`
  for `nsabbey`, `abbeygate01/02` or `elwynnwidebridge`. Screenshots
  `northshire-profile.png` and `northshire-south.png` show terrain, trees, the abbey roof and
  nearby props. Remaining non-fatal asset errors: `piinetree01.blp` mip level 9 out of
  bounds and `blacksmith_smoke.m2` `not a SKIN profile`; neither is yet proven to block the
  route. The first profile screenshot's near-spawn camera is partly occluded by a WMO wall;
  the south view is usable. This is a renderer/world proxy, not client or UI acceptance.
- Final client binary after `40b698fa`: SHA-256
  `A424BAB2FE8B49C4B439E5189B0FBA3D9407FAAC0808B9ACFD0C0CC5A8A69712`; the launch still
  stops at the missing-identity error.

## Revalidation after the checkpoint

At `2026-10-28T19:27:32+03:00`, HEAD is still
`3a064904fd26ce8743627f0aeafe9a7319e05dd2`; `git diff 40b698fa..HEAD -- crates` is
empty, so the built binary is still relevant. `target/debug/benilla.exe` is unchanged at
SHA-256 `A424BAB2FE8B49C4B439E5189B0FBA3D9407FAAC0808B9ACFD0C0CC5A8A69712`.
Tracked tree and index are clean; only the preserved untracked logs and procgen helper remain.

The lab server is reachable: `authserver` owns `0.0.0.0:3724` and `worldserver` owns
`0.0.0.0:8095` (same processes as the run card: 25176 and 27100). No server/DB command was run.

### Identity handoff (no secret recorded)

The Wrath launcher reads `WOW_USER` and `WOW_PASS` from its process environment. The owner can
supply them in their own PowerShell session, using a masked prompt, and launch the existing
binary from that same session; the environment is inherited and nothing is written to the repo
or this log:

```powershell
$env:WOW_USER = Read-Host 'WOW account'
$sec = Read-Host -AsSecureString 'WOW password'
$env:WOW_PASS = [System.Net.NetworkCredential]::new('', $sec).Password
Remove-Variable sec
# set WOW_WRATH / WOW_WRATH_AUTH / WOW_WRATH_WORLD / WOW_DATA / WM_ASSET_MANIFEST / WM_ASSET_PROFILE
& 'D:\Projects\project-b-client\target\debug\benilla.exe' *> 'D:\Projects\project-b-client\.scratch\project-b\verification\2026-10-28-oc-alpha\connected-run.log'
```

`.probe-identity` is gitignored but the Wrath config does not read it; using that file with a
small shell wrapper is also possible, but no wrapper or new credential path is added here.

## Acceptance ledger

Connected run at `2026-10-28T22:00` local (owner launched with credentials; none recorded):
the client reached renderer startup and then panicked before character selection with
Bevy B0002:

```text
error[B0002]: ResMut<benilla_app::ui_script::UiClock> in system
benilla_app::ui_script::extract::tick_script conflicts with a previous
Res<benilla_app::ui_script::UiClock> access
```

Crash report: `benilla-config/Diagnostics/crash-1793214023.txt`; stdout/stderr:
`.scratch/project-b/verification/2026-10-28-oc-alpha/connected-run.log` (UTF-16).
This is the first demonstrated connected-client blocker.

| Step | Result |
|---|---|
| Build current client | pass, `40b698fa` dev profile |
| Login/select character | attempted; client panicked at startup with B0002 before the roster |
| Enter Northshire | blocked by the startup panic |
| Terrain/player/stock UI visibility | terrain, abbey and route props render in the profile-correct worldview; no world-player or stock UI check yet |
| Move/turn, target/interact, quest, combat, loot, turn-in, re-entry | not run; need the live identity and human combat step |

## In-world shadow-data run (not A0/A1)

At `2026-10-29T00:35:00+03:00`, after the B0002 `ParamSet` fix, the client was launched with
`WOW_STOCK_UI=1` and a shadow data root
(`.scratch/project-b/verification/2026-10-28-oc-alpha/shadow-data`) that hard-links the installed
MPQs but exposes no `Interface/AddOns`. The real install was not modified. The stock/preload addon
pass still found 26 archive addons (`WMBridge`, `WDM`, `FlashWindow`, `Blizzard_*`), but no installed
third-party folders. The client reported `in-game UI up in 3460 ms`, then
`wrath: entered world 0, player 0x6a7, 217 entities`. A live window capture is
`shadow-world.png`; it shows a mounted Draenei at `Nessingwary's Expedition` (Nagrand), not
Northshire, with the stock UI partly populated. The process was responsive but reached a
34.6 GB working set in about 70 s, so it was stopped; private-versus-file-backed memory has not yet
been separated. Runtime errors in the 70 s log were dominated by
`UnitFrame.lua:482` missing CVar `threatShowNumeric` (438), `CombatFeedback.lua:107` (156),
`ActionButton.lua:146` missing `GetMultiCastBarOffset` (144), `WMBridge.lua:163` missing
`UnitGUID` (73), and `Blizzard_DebugTools` error-handler failures (72). The run did not show a
character-select roster and did not test movement, combat, quests, loot, turn-in or persistence.
It is therefore a connected in-world smoke, not an A0/A1 acceptance.

### Map-window follow-up (2026-10-29 01:50)

Astra's `68585d5b` added the Wrath `WorldMapArea`/`WorldMapContinent` layouts. OC then added
`GetBindingByKey` (`crates/benilla-ui/src/script/keybind.rs`) and
`GetWintergraspWaitTime`/`GetDeathReleasePosition`
(`crates/benilla-ui/src/script/worldmap.rs`), with the focused
`wrath_map_timer_globals_answer_quietly` test. Both focused tests passed; `cargo build -p
benilla` passed, binary SHA-256
`7D020D1EF4283F7A2DC54F957F2508E0AFC610F3B5D00CBB58B13684F992E9B8`.

Live short tests after the rebuild:

- `connected-run-20261029-014835-103.stderr.log`: `M` opened the map; no
  `GetWintergraspWaitTime`, `GetDeathReleasePosition`, `GetBindingByKey`, `WorldMapArea` or
  `WorldMapContinent` errors remained; 5 frame hitches. A second `M` did not close it
  (`oc-map-close-20261029-014834-716.open.png` / `.closed.png`).
- `connected-run-20261029-015003-296.stderr.log`: a synthetic close-button click did not
  visibly close it; the client has its own cursor, so this is not conclusive
  (`oc-map-x-20261029-015002-966.after-x.png`).
- After adding a no-op `InitWorldMapPing`, `connected-run-20261029-020521-108.stderr.log`
  showed `M` opening the map and a second `M` closing it (`oc-map-init-20261029-020520-721.open.png`
  / `.closed.png`; the closed image is the world again).
- After also defining `GetCurrentMapDungeonLevel` (0), `DungeonUsesTerrainMap` (false) and
  `IsZoomOutAvailable` (false), `connected-run-20261029-022309-308.stderr.log` paints the
  **world** sheet and closes with `M` (`oc-map-paint-20261029-022308-961.open.png` /
  `.closed.png`). The map no longer aborts on those globals, but it is the world sheet rather
  than Nagrand and the zone/continent dropdowns are blank. Remaining map errors are
  `HasDebugZoneMap` (2), `QuestMapUpdateAllQuests` (8), `showBattlefieldMinimap` (6); 10 frame
  hitches include ~4 s and ~9 s. Private memory remains ~33 GB.
- The closed world screenshot still has **no player model** at the camera; only terrain and a
  blue NPC-like figure off-center. The OnUpdate error loop and the M-close path are fixed; map
  zone selection, the player model and memory are not.
- No A0/A1 claim.

### Wrath unit-token live verification (2026-10-29)

- `cargo test --locked --offline -p benilla-ui --lib wrath_`: 12 passed, including absent,
  present, target-hop, known-GUID/no-snapshot, disappearance, target-change, `UnitLevel` and
  `UnitIsPartyLeader` cases; vanilla `parse_unit_token`/resolver tests still reject the Wrath
  names.
- `cargo test --locked --offline -p benilla-app --lib wrath_target_path_resolves_focus_vehicle_and_boss_state`:
  pass; the production target/assist resolver reads the same focus/vehicle/boss fields.
- `cargo test --locked --offline -p benilla-app --lib stock_player_frame_host_smoke_test`:
  pass.
- `cargo build --locked --offline -p benilla`: pass.
- Bounded probe `astra-track-probe-20261029-162902-399`, client
  `connected-run-20261029-162902-788.stderr.log`: zero `Unknown unit name`, zero `TargetFrame`
  OnLoad errors, zero `UnitGUID` errors, zero map debug/CVar errors; UI came up and the client
  entered world; peak private memory `2,446,798,848` bytes; 2 frame hitches. The probe stops at
  its 55 s deadline, not a client failure.
- Remaining errors are other missing Wrath APIs/settings (`WatchFrame` quest iteration,
  `IsActiveBattlefieldArena`, `PaperDollFrame` stat CVars, `GetSelectedDisplayChannel`,
  `UnitGroupRolesAssigned`, `displayWorldPVPObjectives`, `displayFreeBagSlots`,
  `worldMapOpacity`, `GetNumMapDebugObjects`, `GameTooltip:IsUnit`, `UnitSelectionColor`,
  `IsModifiedClick`, `SpellCanTargetItem`, `GetPlayerFacing`, and `debuglocals`), 437 B0004
  hierarchy warnings, and the still-empty focus/vehicle/boss production state. The player
  model/geosets and map zone selection remain open; no movement/combat/quest/loot acceptance is
  claimed. The vehicle pitch handlers are callable no-ops and are not vehicle functionality.

No alpha or full-replacement claim follows.

## 2026-10-30 quest exchange and stock quest-log rendering

Status: source/verification cycle continuing from `76e108da`. Branch `wotlk`; commits this
block: `76e108da` (12340 quest exchange), `94153b34` (quest-log detail/controls),
`f82d12dd` (25 Wrath quest slots), `75bff9e8` (description gradient clear). No server, DB,
WM, install or credential operation. Astra's concurrent dirty files
(`crates/benilla-assets/src/blp.rs`, `crates/benilla-blp/src/lib.rs`,
`crates/benilla-formats/src/{items,lib}.rs`, untracked `crates/benilla-blp/examples/`) are
preserved untouched; the verified binary includes them, so the exact source is HEAD plus that
concurrent work, not a clean HEAD checkout.

### Verified fixes

- The quest-giver chain reaches the stock frames: `SMSG_QUESTGIVER_QUEST_DETAILS` 12340
  layout (second GUID, extra dword, reputation/emote tail), `SMSG_QUEST_QUERY_RESPONSE`
  12340 layout (six choices, completed text, relocated item requirements), quest-log seat
  through the watcher, and the adapter's `WrathQuestSlots` notices.
- Stock controls now render: 3.3.5 `<NormalFont style="...">` links the font object (the
  `UIPanelButtonTemplate` labels had no font and measured 1x1), `Slider:Enable`/`Disable`
  exists for `HybridScrollFrame`, and the quest log's Share button resolves to its anchored
  94-px rect (its `GetWidth` stays the declared 1 px per the 1.12 getter rule).
- The description body now draws: the 3.3.5 log template has no fading frame, so
  `SetAlphaGradient(0, 0)` must clear the reveal. `gradient_alpha` now returns opaque for a
  zero-length gradient at start 0 while keeping the hard edge for a positive start.
- Wrath-only globals are installed only in the Wrath host: gossip counts/`ForceGossip`, the
  quest-log reward getters and `ProcessQuestLogRewardFactions`, `GetBindingByKey`, and the
  map/Wintergrasp helpers. `the_engine_adds_no_global_past_1_12` passes again. The reward
  getters read translated 12340 honor/talent/arena/title/reputation data; XP remains 0
  (the `QuestXP.dbc` calculation is not wired) and `ProcessQuestLogRewardFactions` is a
  compatibility entry, not an implemented routine.

### 25-slot verification

`QUEST_LOG_SLOTS` is 25. The adapter keeps the full 12340 window in a `WrathQuestSlots`
component beside the 1.12 descriptor's 20-slot projection; the feed and field notices read
it, and the VM's rows/selection iterate all 25.

- `cargo test -p benilla-app --lib wrath_quest_slots_keep_all_twenty_five`: pass (slot 25 id,
  counters, state and timer).
- `cargo test -p benilla-ui --lib the_log_holds_all_twenty_five_wrath_slots`: pass (26 rows =
  header + 25 quests; the 25th row selects and returns its text).
- `cargo test -p benilla-ui --lib wrath_reward_lines_come_from_the_selected_record`: pass.

### Observed evidence

- Quest accept/display PASS: `connected-run-20261030-065229-083.stderr.log`
  (`panel shown=true title="Panther Mastery" desc=present objectives="...kill 10 Panthers." -> PASS`),
  screenshot `oc-run-20261030-065228-716.png`.
- Controls/title/objectives rendered (description still blank): `connected-run-20261030-131126-237.stderr.log`,
  screenshot `oc-run-20261030-131125-846.png`; probe showed the labels' font
  `Fonts\FRIZQT__.TTF@12` and the Share rect `l=128 r=222`.
- Description fixed: `connected-run-20261030-170441-580.stderr.log` has the explicit
  `probe-log: INSTRUMENT START quest-log detail render check` and `INSTRUMENT END`, with
  `detail desc ... text=Now you're ready to turn it up a notch...`; screenshot
  `oc-run-20261030-170440-837.png` shows the full description body, objectives and labeled
  Abandon/Share/Track/Close controls.
- Binary SHA-256 of that run: `DBAAB5CD38C8720A980167F66569B1B041B4F97689BB8097AA3B4FC2455F3A9B`
  (`target/debug/benilla.exe`, built from HEAD `f82d12dd` plus the `75bff9e8` edits and
  Astra's concurrent dirty files above).
- Full `benilla-ui` lib: 3355 passed, 3 failed, 1 ignored. The three failures are outside this
  slice: `script::tests::glue_surface::the_glue_vm_answers_the_glue_surface` (in-game CVar/video
  globals reachable in the glue VM since `1542e4e1`) and two local-timezone tests
  (`stdlib::date_and_time_read_the_local_zone`, `stdlib::time_converts_a_date_table`).
  `benilla-protocol-wrath`: 32 passed.

### Remaining graphics/UI defects

- The quest log's zone headers are blank: `AreaTable.dbc` (schema 25 vs 36 fields) and
  `QuestInfo.dbc` (10 vs 18) still fail their Wrath layouts, so grouping names are empty.
- The Share button's state texture is visually weaker than its neighbours; its label, hit rect
  and click path work. `GetWidth` reporting 1 is the reference rule, not the rect.
- The green wedge remains when the camera enters an M2 doodad interior (tent): no M2 camera
  collision.
- 12340 particle data is still refused (`3b0b0e0f` containment); real 264 effects are disabled.
- Map zone dropdowns remain blank; `debuglocals` makes the stock error handler noisy; the
  broader missing-API list from Astra's handoff still stands (vehicle/boss production state,
  `GameTooltip:IsUnit`, etc.).

### Owner's interactive combat/loot/turn-in route

Launch without probe variables (stock UI, shadow data; the identity wrapper prints no secret):

```powershell
$env:WOW_STOCK_UI = '1'
$env:WOW_DATA_OVERRIDE = 'D:\Projects\project-b-client\.scratch\project-b\verification\2026-10-28-oc-alpha\shadow-data'
powershell -NoProfile -ExecutionPolicy Bypass -File '.scratch\project-b\verification\2026-10-28-oc-alpha\run-from-identity.ps1'
```

Character: Dicetar (`0x6a7`), Nesingwary's Expedition, 20/25 quests. Current mastery progress:
Tiger Mastery 186 7/10, Raptor Mastery 194 8/10, Panther Mastery 191 0/10.

1. Kill 10 Stranglethorn Panthers (entry 683) near the camp with melee; the Panther Mastery
   counter in the log must advance 0/10 -> 10/10 and the row show `(Complete)`.
2. Loot one panther corpse; check the loot window and the coin/item update.
3. Return to Hemet Nesingwary (`0xf1300002cc003dba`), open the gossip window, select the
   active Panther Mastery row, complete the quest and choose a reward. Record XP/coin.
4. Log out and back in; the quest must be gone, the reward must persist, and no other quest
   may be reset. Do not reset completed quests; the five Green Hills chapters and the other
   log entries stay in progress.

Combat remains human-driven. No kill-probe or unattended combat is authorized; the temporary
kill probe and its registrations were removed. Any further diagnostic run must print an
explicit instrument-start signal and state its expected result first.

### Astra's work in this block

Reviewed Astra's handoff (`2026-10-29-astra-to-oc.md`) and commits: `68585d5b` Wrath
WorldMapArea/Continent layouts, `849af7d8` bone-track bounds, `3b0b0e0f` particle containment,
`7a3ab86a` Wrath startup CVars. The 264 particle decoder, map zone selection/dropdowns and the
vehicle/boss production state remain open. Her opencode worktree
(`D:/Projects/project-b-opencode`, B-ticket branch) is a separate project and was not touched.

## 2026-10-30 graphics data: BLP tails, item display and quest headers

Continued Astra's graphics work from her preserved dirty tree. Commits: `532a527a` (corrupt BLP
mip-tail recovery, extended from world art to terrain layers and point sprites; 12340
ItemDisplayInfo 25-field layout with `InventoryIcon[2]`) and `8174473b` (id-to-name tables read
the file's own field count instead of 1.12's; 12340 AreaTable 36 fields, QuestInfo 18). Both
committed only the intended files; her untracked `benilla-blp/examples/inspect_mips.rs`
diagnostic is included.

- Real-data tests, selected `wm-hd` profile: `the_selected_profile_item_display_is_the_25_field_wrath_layout`
  (25 fields, >10,000 rows) and `the_selected_profile_names_quest_headers_and_tags`
  (`AreaTable 33 -> Stranglethorn Vale`, `QuestInfo 41 -> PvP`) pass; `benilla-blp` 9/9.
- Rendered evidence: `connected-run-20261030-213012-338.stderr.log` (signal
  `INSTRUMENT START quest-log header and detail check`, rows named Duskwood, Mage, Stormwind City,
  Stranglethorn Vale, Wailing Caverns; The Hermit's description and objectives) and screenshot
  `oc-run-20261030-213011-603.png` (named headers, full description, labeled controls).
- Binary SHA-256 `0B6F82B3EAD0DC8C1E75E54B55AD82352617E7C15F3D32DA81447464038438AA` from clean
  HEAD `8174473b`.

Remaining graphics defects: the glue character-create textures
`Interface\Glues\CharacterCreate\{AlternateForm,CharacterCreate-Arrow,Frame-Sides}` fail to load
(16 warnings; profile/archive gap); the green wedge when the camera enters an M2 doodad interior;
12340 particle data still refused (real 264 effects disabled, Astra's containment); map zone
dropdowns blank; `debuglocals` keeps the stock error handler noisy; the Share button's state
texture remains visually weak (label, rect and click work; `GetWidth` 1 is the reference rule).

No alpha or full-replacement claim follows.
