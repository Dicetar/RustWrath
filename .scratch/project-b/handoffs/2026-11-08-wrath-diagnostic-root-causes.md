# Wrath 3.3.5 diagnostic root-cause stack

Updated: 2026-11-08/09. Source-only; **no Cargo/build/test/clippy/type check/client/server/visual
run**. Base: `origin/main` at `6c15d921` (`fix/wrath: resolve quest query, dialog layout, actionbar
placement, and armor geosets`).

All branches are stacked in this order and pushed. The last branch contains the whole stack.

| # | Branch | Commit | Symptom / fix |
|---|---|---|---|
| 1 | `fix/wrath-ui-multi-return-a` | `074a899e` | `GetWorldStateUIInfo` missed `uiType`, `GetBattlefieldStatus` missed team size/registered, `GetPVP*Stats` used dishonorable kills; corrected the 12340 tuples and kept donor tuples target-specific. |
| 2 | `fix/wrath-stock-api-surface-a` | `c8fe7f4d`, `2ef08012` | Added missing 3.3.5 globals (`InCombatLockdown`, `UnitIsTalking`, `GetCritChanceFromAgility`, `BNGetNumBlockedToons`, voice-session queries, currency), eight CVar defaults, and the pre-load display-mode seed. |
| 3 | `fix/wrath-spellbook-slot-list-a` | `192ca27e` | Spellbook slot lookup indexed the displayed book instead of the all-learned list. |
| 4 | `fix/wrath-error-handler-a` | `023db579` | Every error-handler invocation now sets `debuglocals`' in-handler flag, so the stock DebugTools handler gets a locals string. |
| 5 | `fix/wrath-action-spell-slot-a` | `c08563f1` | `GetActionInfo`'s spell-ID fallback no longer raises `Invalid spell slot in GetSpellInfo` for a known action-bar spell ID; genuine invalid slots still raise. |
| 6 | `fix/wrath-combatlog-api-a` | `3cfff575` | Added the stock combat-log filter/reset/retention/query API and `CombatLog_Object_IsA`; raw history is truthfully empty until a `CombatLogEntry` producer exists. |
| 7 | `fix/wrath-gm-ticket-empty-a` | `7c643a42` | `UPDATE_TICKET` with no ticket fires no arguments, so `HelpFrame.lua:126` no longer reads nil `oldestTicketTime`. |
| 8 | `fix/wrath-fontstring-lua-tolstring-a` | `c9909ada` | `FontString:SetText`/`Button:SetText`/`EditBox:Insert` read through `lua_tolstring`/`lua_isstring`; `ChatConfigFrame.lua:752`'s dynamic `text` function clears instead of raising. |
| 9 | `fix/wrath-region-template-a` | `8d789a76` | `CreateFontString`/`CreateTexture` apply a registered region template (font, size, justify, text, anchors) instead of warning; fixes `ChatConfig…ClassColorLegend: OnLoad: Font not set`. |
| 10 | `fix/wrath-combatlog-retention-cvar-a` | `f12dda68` | Registered `combatLogRetentionTime` and made the native getter/setter follow it; corrected the combat-log module's stale `CombatLog_Object_IsA` note. |
| 11 | `fix/wrath-settext-contracts-a` | `c077916d` | Wrath `Button:SetText(nil/function/empty)` clears or skips per `CSimpleButton::SetText`; `EditBox:SetText` raises the reference `Usage:` for non-string/non-number. Shared label writes now use the FontString sink. |
| 12 | `fix/wrath-questpoi-empty-a` | `9795f199` | Added the empty QuestPOI native surface (`QuestPOIUpdateIcons`, `QuestPOIUpdateTexture`, `QuestPOIGetQuestIDByIndex`, `QuestPOIGetQuestIDByVisibleIndex`, `QuestPOIGetIconInfo`); the stock map quest list now completes with zero visible quests instead of aborting on a nil global. The request/ring/selection/blob renderer remain absent and are named. |

Final stack head: `9795f199`. It was fast-forwarded into `origin/main` (push `6c15d921..9795f199`) and the remote main SHA was verified as `9795f199fb403d404d47ffca6da050d9b9671690`.

## Evidence class

- Source/reference review only. The pinned reference is
  `D:/Projects/wowdecomp-reference@e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
- No commit above has a compilation, focused test, receiver or visible-gameplay claim. The old
  `latest-diagnostics.md` was generated at `93c0310f` and predates every branch; treat its errors
  as leads, not current fact.
- Existing focused Rust tests were authored in each branch, not executed.

## Remaining named gaps

- Raw combat-log history: no `CombatLogEntry` ring/`PushEvent` producer, so `CombatLogGetNumEntries`
  is 0 and filtered `COMBAT_LOG_EVENT` content is empty. Filter/reset/retention are real.
- `CombatTextSetActiveUnit` (`UnitCombatLog_C_Scripts.cpp` `0x0074D580`): still uninstalled and no
  Project B consumer; stock `Blizzard_CombatText` is LoadOnDemand. Not a stub.
- `QuestPOIFrame` (`WorldMapFrame.xml`): unknown frame type, so `WorldMapBlobFrame` does not exist
  and quest blobs are not drawn. `benilla-protocol-wrath::quest_poi` has a decoder but no Lua/map
  consumer. This is a feature, not a one-line loader fix.
- `VoiceChatSpeakerTemplate` warnings: intentional `WRATH_SKIPPED_FILES = ["VoiceChat.xml"]`
  (maintainer decision: no voice UI). The dangling template references are expected until the skip
  is revisited or its virtual templates are registered in isolation.
- `QuestFontNormalHuge` (`CharacterCreate.xml:463`): the font object is absent from every MPQ
  `GlueFontStyles.xml`/`FontStyles.xml` and `GlueFonts.xml` inspected; the HD install is missing it.
- CharacterCreate textures (`Interface\Glues\CharacterCreate\AlternateForm`, `Frame-Sides`): absent
  from every installed MPQ; not a loader bug.
- M2 Wrath particle gaps (`ui-autocastbutton.m2`, Northrend main menu, campfire, smoke, gryphon
  roost): unsupported spline/lifespan/wind/anisotropic features.
- `EditBox:AddHistoryLine` non-string `lua_tolstring` gap remains (no known stock caller).
- `Button:SetText` on 1.12 keeps a donor-derived early return for a non-string, where real 1.12
  raises; explicitly retained.
- `GetInspectHonorData` 6-vs-12 contract: owned by another session in the primary checkout; do not
  touch its `ui_honor.rs`/`honor_frame_tests.rs`/`wrath.rs`/protocol/pvp work.

## Next concrete step

Take a fresh diagnostic at the top of this stack (`c077916d`) before choosing the next repair. Do
not use the 93c0310f diagnostics as acceptance. Suggested order if no newer log exists:

1. `QuestPOIFrame`/world-map quest blobs (visible map feature; needs the `quest_poi` decoder wired).
2. `CombatTextSetActiveUnit` with a real consumer in the Lua `COMBAT_TEXT_UPDATE` feed.
3. Voice template isolation or a documented accept of the warnings.
4. M2 particle families.

No alpha or release claim is made from this stack.
