# OC — Wrath early-UI smoke bootstrap

Evidence run: `cargo test -p benilla-app --lib ui_script::wrath_ui_slice_tests::stock_player_frame_host_smoke_test`
with `WM_ASSET_MANIFEST=docs/profiles/wm-hd.mpqlist`, `WM_ASSET_PROFILE=wm-hd`,
`WOW_DATA` = the local 3.3.5a HD install, `BENILLA_REQUIRE_DATA=1`.
Logs: `.scratch/project-b/verification/2026-10-27-oc-wrath-smoke/run9.log`..`run33.log`.

The fixture loads the selected profile's TOC prefix through `PlayerFrame.xml`
(`FrameXML.toc` entries 1..75). At checkpoint time the load reaches entry 72,
`FloatingChatFrame.xml`; the previous frontier was `Constants.lua` line 469.

## Failure progression (runtime-verified, one log per run)

| run | first failure |
| --- | --- |
| 9 | `Constants.lua:469 RegisterStaticConstants` |
| 10 | `WorldFrame.lua:28 IgnoreDepth` |
| 11 | `GameTooltip` `OnTooltipSetUnit` unsupported |
| 12 | `RestrictedFrames.lua:67 newproxy` |
| 13 | `GameTooltip.OnTooltipSetUnit` handler (extra script list) |
| 14–23 | `StaticPopup`/`MoneyFrame` `SetPoint("RIGHT")`, `RegisterStaticConstants`, `newproxy`, nested security loaders |
| 24 | `MainMenuBar.lua:185 GetActionBarPage`, `VehicleExit` handler |
| 25–26 | `Minimap.xml` (driver queries, ping texture setters, LFG), `GameTime.lua:247 CalendarGetDate` |
| 27–30 | `GameTime`..`ActionBarFrame.xml` (`Unknown frame type: Cooldown`) |
| 31–33 | `BNConversations.lua:3 BNGetMaxPlayersInConversation`; now `FloatingChatFrame.xml` |

## What landed (paths are in this diff)

- `benilla-lua`: 5.1 `newproxy` (`lib_base::newproxy`, re-exported), full-userdata semantics —
  no argument/`false` unmetatabled, `true` private empty metatable, a userdata shares its
  metatable. Installed only in the Wrath sandbox (`script/newproxy.rs`).
- `benilla-ui` wrath sandbox globals (`script/wrath_security.rs`, `script/wrath_natives.rs`,
  `script/static_constants.rs`): `issecure`, `securecall`, `RegisterStaticConstants`,
  `IsStereoVideoAvailable`, the `Sound_GameSystem_*`/`Sound_ChatSystem_*` driver queries,
  `GetModifiedClick` (3.3.5a `Bindings.xml` defaults), `BNFeaturesEnabled(AndConnected)`,
  `GetExistingLocales`, `VehicleExit`, tracking/LFG/world-PvP queries, `CalendarGetDate`,
  `BNGetMaxPlayersInConversation`, `CooldownFrame_SetTimer`.
- Loader/objects: `<Animations>` graph (`loader/animations.rs`, `script/animation.rs`) with named
  and parent-keyed `AnimationGroup`/animation objects and `Play`/`Stop` state; `function=""`
  handler clears (`UIPanelTemplates.xml:365`); wrath `SetPoint(point)` short form
  (`MoneyFrame.lua:253`); `WorldFrame:IgnoreDepth`; `OnEnable`/`OnDisable` script slots and their
  enable-edge firing; `SetNormalFontObject`/`GetNormalFontObject` aliases; `GetActionBarPage`/
  `ChangeActionBarPage(page)`; `Minimap:SetPlayerTextureWidth/Height`; wrath `Cooldown` type
  admitted as a plain game-layer frame.
- Fixture: seats the localized class catalog, registers the wrath audio/minimap CVars, and
  narrows the warning gate to the stock GlobalStrings key-miss notices (OPTIONS,
  BATTLEGROUND_LORETEXT_1) that the real client shows as raw keys.

## Known incomplete, named so they are not silent

- Animation objects carry XML parameters and play state; the timeline that interpolates them and
  fires `OnPlay`/`OnStop`/`OnFinished` is not built.
- `Cooldown` is a plain frame: no sweep rendering, and `CooldownFrame_SetTimer` writes the 1.12
  `start`/`duration` fields.
- `GetNumTrackingTypes` is empty: the host does not push the known tracking spells' aura masks.
- `BNGetMaxPlayersInConversation` answers 10; the live service's cap is not in the shipped data,
  and its only consumer is the unreachable conversation dialog.
- `CalendarGetDate` answers zeros until a calendar date is pushed (pre-login state).
- Gates unavailable on this machine: no `cargo fmt`/`clippy` component installed; `cargo check`
  is green for `benilla-lua`, `benilla-ui`, `benilla-app`.

## Next frontier (`FloatingChatFrame.xml`)

- `SetFormattedText` on regions (the stock GeneralDockManager OnLoad calls it).
- Wrath has ten chat windows; `script/chat_window.rs` exposes seven (1.12 `NUM_CHAT_WINDOWS`).
- `chatStyle` CVar is not registered by the host.
- `GetChatWindowSavedDimensions`/`GetChatWindowSavedPosition` (wrath window geometry APIs).
- `BNGetNumFriends` (the friends micro button's inline OnLoad).
After that: `VoiceChat.xml`, `ReadyCheck.xml`, then `PlayerFrame.xml` itself.
