# Handoff to Astra — in-world client failure, 2026-10-29

**From:** OC  
**Status:** connected in-world smoke failed; **no A0/A1 acceptance claim**.  
**Immediate owner report:** player character model absent, severe lag, world map opened but
would not close, then the client froze/exited.

This is the handoff for investigation. The client is stopped. No server, database, WoW install,
or WM operation was performed in this cycle.

## TL;DR

The B0002 startup panic is fixed (uncommitted), and the client now connects, enters the world and
renders the world plus stock UI. But:

- The player model is reported missing by the owner. The earlier screenshot is ambiguous; verify
  whether the mounted Draenei in `shadow-world.png` is the player or an NPC.
- The world map is structurally disabled: `WorldMapArea.dbc` fails to parse (schema 8 fields,
  Wrath file 11), and the FrameXML map then takes missing-API errors.
- Opening the map triggers a missing `GetBindingByKey` (Escape/close path), then
  `WorldMapFrame` errors every ~0.6 s on `GetWintergraspWaitTime` and
  `GetDeathReleasePosition`; the error handler itself fails on `debuglocals`, and the client
  hitches/freezes.
- Before the map, the stock UI already produces a large error burst and the process has ~35 GB
  **private** memory on a 64 GB machine. This is not just MPQ file cache.
- The current character is an existing Draenei at **Nessingwary's Expedition (Nagrand)**, not a
  level-1 Northshire character. The client auto-entered world; there was no roster/character-select
  step.

## Post-fix live update (2026-10-29 01:50)

After Astra's `WorldMapArea`/`WorldMapContinent` commits, OC added three Wrath compatibility
globals:

- `GetBindingByKey` in `crates/benilla-ui/src/script/keybind.rs`, as the same key-to-action
  lookup as `GetBindingAction`;
- `GetWintergraspWaitTime` (nil), `GetDeathReleasePosition` (0, 0), and a no-op
  `InitWorldMapPing` in `crates/benilla-ui/src/script/worldmap.rs`, so
  `WorldMapFrame_OnUpdate`/death-release tail stop raising and `WorldMapFrame_OnLoad` gets past
  its ping setup into `WorldMapFrame_Update`.

Focused tests both passed and the client rebuilt (final binary `38562695...`). Live short runs:

- `connected-run-20261029-014835-103.stderr.log`: map opened with `M`; **no**
  `GetWintergraspWaitTime`, `GetDeathReleasePosition`, `GetBindingByKey`, `WorldMapArea`, or
  `WorldMapContinent` errors; only 5 frame hitches. The map was still visually open after a
  second `M` (screenshots `oc-map-close-20261029-014834-716.open.png` / `.closed.png`).
- `connected-run-20261029-015003-296.stderr.log`: same clean map globals; a synthetic click at
  the window's close-button position did not visibly close the map
  (`oc-map-x-20261029-015002-966.after-x.png`). The client uses its own cursor, so this click
  is not conclusive; the stock close-button path still needs a real mouse test.
- `connected-run-20261029-020521-108.stderr.log` (after the `InitWorldMapPing` no-op):
  `M` opened the map; a second `M` **closed it** — `.closed.png` shows the world again
  (`oc-map-init-20261029-020520-721.open.png` / `.closed.png`). This is the first bounded run
  where the map open/close path works.
- `connected-run-20261029-022309-308.stderr.log` (after `GetCurrentMapDungeonLevel` = 0,
  `DungeonUsesTerrainMap` = false, `IsZoomOutAvailable` = false): the map **paints** the world
  sheet (Northrend / Kalimdor / Eastern Kingdoms) and closes with `M`
  (`oc-map-paint-20261029-022308-961.open.png` / `.closed.png`). This is a usable map
  open/close, but it is the **world** sheet, not Nagrand, and the zone/continent dropdowns are
  still blank.
- Remaining map log errors: `HasDebugZoneMap` (2), `QuestMapUpdateAllQuests` (8),
  `showBattlefieldMinimap` (6); 10 frame hitches including ~4 s and ~9 s. The map update no
  longer aborts on the dungeon/zoom globals.
- The closed world screenshot still shows **no player model** at the camera; only terrain and a
  blue NPC-like figure off-center. The player frame remains a Draenei with `1/1` bars.
- Private memory remains ~33 GB in these short runs; the map error loop was not the memory
  cause. B0004 warnings and WMBridge/`UnitGUID`/`DebugTools` errors remain.
- No A0/A1 claim. Next map work: seed the map at the player's zone and fill the
  continent/zone dropdowns; next model work: why the player is invisible; next performance
  work: the memory spike.

## Source and binary pin

- Repo: `D:/Projects/project-b-client`
- Branch: `wotlk`
- HEAD at the post-fix live check: `2ed2686f683490c4dc4b3a40b3dc6a37214956c1`
  (Astra's map DBC and memory commits: `68585d5b`, `c9f2b3e5`, `849af7d8`, `2ed2686f`).
- Latest built binary: `target/debug/benilla.exe`
- Binary SHA-256: `385626957B32C940ADCFED6BAF16150FC4F639783F9D5F9CACC2F46FFE5DB27A`
  (built `2026-10-29 02:22:15`; includes the uncommitted B0002 and map-window changes below).
- Earlier binary used for the first reports: SHA-256
  `FD963FEF649005814885BD1B9EABBC69552A79B44F096C253B8A71493DA1E3E7`.
- Profile: `docs/profiles/wm-hd.mpqlist`
- Manifest SHA-256: `439BDC21D09F5509073CD88CC341ADB2EB05854B425F4D9DEC1D2BCEE5083D59`
- Data: production MPQs under `D:\WOW\world of warcraft 3.3.5a hd\data`
- **Uncommitted source fixes:**
  - `crates/benilla-app/src/ui_script/extract/mod.rs` — B0002 `UiClock` `ParamSet` repair.
  - `crates/benilla-ui/src/script/keybind.rs` — `GetBindingByKey` alias.
  - `crates/benilla-ui/src/script/worldmap.rs` — Wrath map timer/death-release stubs,
    `InitWorldMapPing`, and the dungeon/zoom globals `WorldMapFrame_Update` expects.
  - `crates/benilla-app/src/ui_script/world_map_tests.rs` — focused regression.
  - Astra's temporary allocator probe is also uncommitted in `crates/benilla-app/src/lib.rs`
    and `perf/journal.rs`; do not overwrite it.
- **Uncommitted checkpoints/handoffs:** `2026-10-28-oc-first-alpha.md`,
  `2026-10-26-astra-wrath-ui-host.md` (Astra's), and this handoff.
- Focused tests after the map changes: `get_binding_by_key_is_the_action_lookup` passed;
  `wrath_map_timer_globals_answer_quietly` passed; `cargo build -p benilla` passed.

## Reproduction route

The installed addons were isolated without modifying the WoW installation:

- A shadow data root was created at
  `.scratch/project-b/verification/2026-10-28-oc-alpha/shadow-data`.
- It hard-links the installed MPQs and junctions the locale folders; it exposes **no**
  `Interface/AddOns` beside it.
- The client was launched with:
  - `WOW_STOCK_UI=1`
  - `WOW_DATA_OVERRIDE=<that shadow-data path>`
  - `run-from-identity.ps1` (it reads `.probe-identity`; never print its values)
- The runner is
  `.scratch/project-b/verification/2026-10-28-oc-alpha/run-from-identity.ps1`.
- Chain addons still load: `WMBridge`, `WDM`, `FlashWindow`, and the archive `Blizzard_*` rows.
  The 50 installed third-party folders are absent from the shadow root.

Launch:

```powershell
$env:WOW_STOCK_UI = '1'
$env:WOW_DATA_OVERRIDE = 'D:\Projects\project-b-client\.scratch\project-b\verification\2026-10-28-oc-alpha\shadow-data'
& 'D:\Projects\project-b-client\.scratch\project-b\verification\2026-10-28-oc-alpha\run-from-identity.ps1'
```

Do not modify `.probe-identity`; it contains the WOW user/password/character and must not be
printed, copied, or committed.

## Observed owner symptoms

1. Client auto-entered world on the existing Draenei. Zone text: **Nessingwary's Expedition**;
   map is Nagrand.
2. Player character model reported missing.
3. Severe lag.
4. Owner opened the world map. It would not close.
5. Client froze or became unresponsive; the window then closed/exited.

## Evidence

Latest run:

- Start: `2026-10-29T00:40:33+03:00`
- Window exited: `2026-10-29T00:44:18` (`No windows are open, exiting`)
- `connected-run-20261029-004032-972.meta.txt`
- `connected-run-20261029-004032-972.stderr.log` — 1,064,270 bytes, 5,311 lines
- `connected-run-20261029-004032-972.stdout.log`
- `connected-run-20261029-004032-972.status.txt`
- No new diagnostic crash report; exit was a window close, not a Rust panic.

Earlier successful in-world smoke:

- `connected-run-20261029-003500-142.*`
- Screenshot: `shadow-world.png` (stock UI, world visible, mounted Draenei centered near the
  camera; also a white chat frame and partly empty unit frames)
- In-game UI: 3,460 ms; `wrath: entered world 0, player 0x6a7, 217 entities`.
- Process reached ~34.6 GB working set / ~35 GB private, then was stopped by OC after capture.

The earlier B0002 run is not this failure:

- `benilla-config/Diagnostics/crash-1793214023.txt`
- `connected-run.log` (UTF-16)
- `connected-run-20261029-002003-671.stderr.log` (non-stock add-on load hang)
- `connected-run-20261029-002424-172.stderr.log` (stock UI + installed add-ons, Questie reload)

## Findings

### 1. World map is disabled at load, then the FrameXML map errors

At world entry the engine reports:

```text
world map: DBC load failed, map window disabled:
applying WorldMapArea schema (field-count mismatch?): schema has 8 fields but file has 11
```

Source: `crates/benilla-formats/src/world_map_area.rs` (`Schema::new("WorldMapArea")` and
`parse`). This is a Wrath-vs-vanilla schema mismatch. The map window is disabled before the UI
opens it.

FrameXML map load then reports:

```text
WorldMapFrame.xml: Unknown frame type: QuestPOIFrame
WorldMapFrame.lua:86: attempt to call global `InitWorldMapPing' (a nil value)
FrameXML/Interface\FrameXML\WorldMapFrame.xml loaded (103 frames materialized)
WorldMapFrame.lua:718: Couldn't find CVar named 'showBattlefieldMinimap'
```

### 2. Map cannot close; map update loop blocks the client

When the owner opened the map at `21:44:11`:

```text
UIParent.lua:3195: attempt to call global `GetBindingByKey' (a nil value)   (5 times)
WorldMapFrame.lua:208: attempt to call global `GetWintergraspWaitTime' (a nil value)   (94 times)
WorldMapFrame.lua:908: attempt to call global `GetDeathReleasePosition' (a nil value)  (94 times)
Blizzard_DebugTools.lua:398: attempt to call global `debuglocals' (a nil value)        (253 times)
```

The two `WorldMapFrame` errors repeat roughly every 0.6 s. Near the end:

```text
frame hitch: 600 ms (main thread blocked this long)
```

The map's `OnUpdate` path is effectively error-looping, and the error handler also fails. That is
the strongest candidate for "map will not close / client freezes". The missing
`GetBindingByKey` is the likely Escape/close-binding failure.

### 3. The stock UI is already error-heavy before the map

Latest log counts (5,311 lines):

| Pattern | Count |
|---|---:|
| `runtime error` | 2,581 |
| `B0004` hierarchy warnings | 1,628 |
| `threatShowNumeric` CVar missing | 912 |
| `CombatFeedback.lua:107` nil text | 320 |
| `debuglocals` missing | 253 |
| `WMBridge.lua:163` `UnitGUID` missing | 246 |
| `WorldMapFrame` | 196 |
| `GetMultiCastBarOffset` missing | 168 |
| `GetWintergraspWaitTime` / `GetDeathReleasePosition` | 94 each |
| `GetBindingByKey` missing | 5 |
| Panics / explicit OOM | 0 |

Large startup hitches include `6030 ms`, `11826 ms`, and repeated ~700–1000 ms hitches. The UI is
not just slow; it is being driven through repeated Lua errors and UI-entity churn.

### 4. Private memory is ~35 GB before the map

At ~40 s after launch, `PrivateMemorySize64` was ~35 GB and `WorkingSet64` ~34 GB on a 64 GB
machine. This is private memory, not just mapped MPQ pages. The earlier run reached ~34.6 GB and
was stopped. No OOM/panic appeared; the process was responsive until the map interaction.

This needs a separate memory investigation after the map error loop is fixed. Candidate areas:
texture/asset retention (HD profile), UI entity/texture churn (`B0004` 1,628), and repeated error
handling. Do not assume the map alone caused the memory level; it was already high before the map.

### 5. Player model and unit frame are open defects

The owner reports no player character model. The earlier `shadow-world.png` shows a mounted
Draenei centered near the camera. It is **not established** whether that is the player or an NPC;
the owner's live observation says the player model was absent. The player GUID in the log is
`0x6a7`. The top-left unit frame shows a Draenei portrait and `1/1` health/mana, which is
suspicious.

Please verify:

- whether `player 0x6a7` has a spawned/visible model entity and a valid camera target;
- whether the mounted Draenei in the screenshot is the player or an NPC;
- why the unit frame reads `1/1`;
- whether the model disappears only after the map/freeze or is absent from world entry.

### 6. This is not the intended starting-zone run

The client auto-entered the existing Draenei in Nagrand. There was no character roster/selection
step, and no Northshire level-1 character. A0 (roster/selection → Northshire) is not demonstrated.
The in-world smoke does prove network login, world entry, 216–217 entities, terrain, and stock UI
rendering, but not the alpha route.

## Suggested investigation order for Astra

1. **Map DBC first.** Make `WorldMapArea.dbc` parse the Wrath 11-field layout in
   `crates/benilla-formats/src/world_map_area.rs`; confirm `build_catalog` and
   `ui_world_map` seed a valid map. This removes the `map window disabled` condition.
2. **Map close/update path.** Add or correctly stub the missing Wrath globals used by
   `WorldMapFrame` and `UIParent`: `InitWorldMapPing`, `GetWintergraspWaitTime`,
   `GetDeathReleasePosition`, and `GetBindingByKey`; add the `showBattlefieldMinimap` CVar.
   Resolve `QuestPOIFrame` as a known/handled frame type. Ensure `Escape` and the close button
   both close the map.
3. **Error-handler fallback.** `Blizzard_DebugTools` calls missing `debuglocals`. Make the error
   handler degrade safely instead of failing recursively; this is likely amplifying every other
   error and the frame churn.
4. **Stock-UI CVar/API gaps.** Add Wrath defaults for `threatShowNumeric`, `chatStyle`,
   `playerStatusText`, `partyStatusText`, `targetStatusText`, `petStatusText`, and related
   `TextStatusBar`/`UnitFrame` CVars. Stub or implement `GetMultiCastBarOffset` and
   `GetNumVoiceSessions`; decide whether `WMBridge`/`WDM`/`FlashWindow` archive addons should be
   excluded or made tolerant of missing `UnitGUID`.
5. **Profile memory.** After the map/error loop is fixed, measure private memory and asset
   retention. Check texture cache/eviction, UI entity/template creation, and the 1,628 `B0004`
   hierarchy warnings.
6. **Player model.** Confirm model spawn/visibility and camera for `player 0x6a7`; reconcile with
   `shadow-world.png`.
7. **Then the alpha route.** Roster/selection and a level-1 Northshire character; movement,
   target/interact, quest, combat, loot, turn-in, re-entry. Combat must remain a human step.

## Constraints and ownership

- **Do not overwrite** the uncommitted `extract/mod.rs` UiClock fix or the checkpoint; preserve
  the binary/source identity above.
- Check the checkpoint's claims before editing shared UI files. OC's only UI claim is
  `crates/benilla-app/src/ui_script/extract/mod.rs`; no Astra-claimed file was edited in this
  cycle.
- Do not print, copy, or commit `.probe-identity` values. The runner reads it without echo.
- Do not modify the WoW install, add/remove addons there, change server/DB state, or touch WM.
- Preserve the shadow data root and all evidence logs/screenshots. The shadow root is read-only
  for this purpose; it is not a second install.
- The per-character `benilla-config/addon-state/AzerothCore-<char>.txt` file currently disables
  50 **installed** third-party addons for this test character. It was an isolation attempt; the
  shadow root is what actually removed them. Do not print the character name.
- Do not commit logs, screenshots, `.probe-identity`, or generated shadow data.

## Acceptance result

| Step | Result |
|---|---|
| Build binary with B0002 fix | pass; check/build; binary SHA above |
| Login and enter world | **pass for smoke only**: player `0x6a7`, 216–217 entities, Nagrand |
| Stock UI render | in-game UI up; map/unit/action UI incomplete and error-heavy |
| Player model | **fail/open**: owner reports absent; screenshot ambiguous |
| Map open/close | **fail**: open, cannot close, error loop, hitch/freeze |
| Movement/target/interact/quest/combat/loot/turn-in/re-entry | not run |
| A0/A1 acceptance | **not claimed** |

No alpha or full-replacement claim follows.
