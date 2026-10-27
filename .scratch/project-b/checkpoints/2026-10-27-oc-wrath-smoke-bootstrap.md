# OC — Wrath early-UI smoke bootstrap

Evidence run: `cargo test -p benilla-app --lib ui_script::wrath_ui_slice_tests::stock_player_frame_host_smoke_test`
with `WM_ASSET_MANIFEST=docs/profiles/wm-hd.mpqlist`, `WM_ASSET_PROFILE=wm-hd`,
`WOW_DATA` = the local 3.3.5a HD install, `BENILLA_REQUIRE_DATA=1`.
Logs: `.scratch/project-b/verification/2026-10-27-oc-wrath-smoke/run9.log`..`run59.log`.

**State: passing (`run59.log`).** The fixture loads the selected profile's whole TOC prefix
through `PlayerFrame.xml` (all 75 entries, `VoiceChat.xml` skipped by decision), fires
`UNIT_HEALTH`, reads `UnitHealth`, `UnitExists` and a CVar, and asserts every stock handler and
file evaluated without a recorded error. The frontier at the previous checkpoint was
`Constants.lua` line 469.

## Failure progression (runtime-verified, one log per run)

| run | first failure |
| --- | --- |
| 9 | `Constants.lua:469 RegisterStaticConstants` |
| 10–12 | `WorldFrame.lua:28 IgnoreDepth`; `GameTooltip` `OnTooltipSetUnit`; `RestrictedFrames.lua:67 newproxy` |
| 13–27 | options/action-bar/audio/minimap/chat sections: `SetPoint("RIGHT")`, `RegisterStaticConstants`, `newproxy`, `GetModifiedClick`, sound-driver queries, `GetActionBarPage`, `VehicleExit`, `CanHearthAndResurrectFromArea`, `CalendarGetDate`, `Cooldown` type |
| 28–36 | `FloatingChatFrame.xml`: `SetFormattedText`, 10 chat windows, `chatStyle`, saved window geometry, `BNGetNumFriends`, `SetClampRectInsets`, `GetOwner` |
| 37–51 | `PlayerFrame.xml`: `GetDungeonDifficulty`, `GetRaidDifficulty`, `GetOptOutOfLoot`, `UnitThreatSituation`, `IsThreatWarningEnabled`, `GetInstanceInfo`, `GetSummonFriendCooldown`, `UnitHasVehicleUI`, `IsStackableAction` |
| 52–59 | fixture seeding + assertion repairs; green |

## What landed (paths are in this diff)

- Sandbox (`benilla-ui` script): `newproxy` engine verb (previous commit), `issecure`,
  `securecall`, `RegisterStaticConstants`, sound-driver and chat-driver queries,
  `GetModifiedClick`, `BNFeaturesEnabled(AndConnected)`, `GetExistingLocales`, `VehicleExit`,
  LFG/world-PvP/tracking queries, `CalendarGetDate`, `BNGetMaxPlayersInConversation`,
  `CooldownFrame_SetTimer`, chat window APIs, threat/difficulty/instance/vehicle queries,
  `IsStackableAction`, `GameTooltip:GetOwner`, the voice-chat absent-set, `SetClampedToScreen`
  clamp insets (`SetClampRectInsets`, `GetClampRectInsets` + layout clamp), `SetFormattedText`,
  ten chat windows on the Wrath host (`ChatFrame1`..`ChatFrame10`, Combat Log docked at 2),
  `GetNumFriends`' two-value Wrath form.
- Loader: `<Animations>` object graph (named, parent-keyed, `Play`/`Stop` state), `function=""`
  clears a named handler, wrath `SetPoint(point)` short form.
- Attributes: `SetAttribute` notifies `OnAttributeChanged` on every set — the stock dropdown
  re-sets `createframes=true` to grow its button list, which a value-diff guard stalled.
- Host: `VoiceChat.xml` is skipped forever (maintainer's decision,
  `reference_ui::WRATH_SKIPPED_FILES`); the fixture allows the one template notice that the skip
  causes, and the stock GlobalStrings key-miss notices, which the real client shows as raw keys.

## Known incomplete, named so they are not silent

- Animation objects carry XML parameters and play state; the timeline that interpolates them and
  fires `OnPlay`/`OnStop`/`OnFinished` is not built.
- `Cooldown` is a plain frame: no sweep rendering; `CooldownFrame_SetTimer` writes the 1.12
  `start`/`duration` fields.
- `GetNumTrackingTypes` is empty (the host pushes no tracking-spell masks), `CalendarGetDate` is
  zero before a calendar push, `GetInstanceInfo`/difficulty/threat/vehicle answers are the
  open-world defaults, and the `BNGetMaxPlayersInConversation` cap is not recoverable from the
  shipped data.
- Chat-window geometry is not persisted (the 1.12 cache has no position/dimension keys).
- Gates unavailable on this machine: no `cargo fmt`/`clippy` component installed; `cargo check`
  is green for `benilla-lua`, `benilla-ui`, `benilla-app`.
