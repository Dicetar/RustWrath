# RustWrath Prepped Issue Queue (Auto-Generated)
- **Run Directory:** `D:\Projects\project-b-client\benilla-config\Logs\Launcher\20261112-022858-228`
- **Flight Recorder Snapshot:** `D:\Projects\project-b-client\benilla-config\Logs\InteractionDiagnostics\interaction-1794440675417481200.jsonl`
- **Generated:** 2026-11-12T02:44:40.8953099+03:00
- **Total Prepped Tickets:** 12

## [AUTO-01] Implement missing Wrath Lua global functions (18 unique)
- **Category:** UI / Lua Globals
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/`
- **Agent Prompt:**
  > Implement the missing 3.3.5a Lua global functions observed at runtime (VoiceChat_Animate, GetVoiceStatus, GetCurrentArenaSeason, GetInventoryItemsForSlot, ClearFocus, IsBattlefieldArena, GetRandomBGHonorCurrencyBonuses, HasLFGRestrictions, GetBackpackCurrencyInfo, GetCombatRatingBonus) in their owning script module under crates/benilla-ui/src/script/ or crates/benilla-app/src/, following the 12340 reference contract.
- **Captured Evidence:**
  - `GetUnitMaxHealthModifier (x13)`
  - `GetRandomBGHonorCurrencyBonuses (x9)`
  - `HasLFGRestrictions (x8)`
  - `GetBackpackCurrencyInfo (x8)`
  - `GetText (x7)`
  - `CanShowAchievementUI (x7)`
  - `GetCurrentArenaSeason (x6)`
  - `GetInventoryItemsForSlot (x6)`
  - `UnitInVehicle (x5)`
  - `IsAtStableMaster (x5)`
  - `GetVoiceStatus (x4)`
  - `VoiceChat_Animate (x4)`
  - `GetCombatRatingBonus (x4)`
  - `RequestLFDPartyLockInfo (x4)`
  - `IsBattlefieldArena (x2)`

## [AUTO-02] Implement missing FrameXML widget methods (5 unique)
- **Category:** UI / Widget Methods
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-ui/src/script/widget/`
- **Agent Prompt:**
  > Implement the missing FrameXML widget methods (IsProtected, SetTracking, SetLFGDungeonReward, HasFocus, GetDummyBag) on the appropriate widget tables in crates/benilla-ui/src/script/widget/ against the 12340 reference.
- **Captured Evidence:**
  - `IsProtected (x18)`
  - `GetDummyBag (x7)`
  - `SetTracking (x5)`
  - `SetLFGDungeonReward (x2)`
  - `HasFocus (x2)`

## [AUTO-03] Resolve nil field/global indexing in stock UI or addons (1 unique)
- **Category:** UI / Frame & Table Fields
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/`
- **Agent Prompt:**
  > Trace and fix the nil field/global accesses (normalTexture) in crates/benilla-ui/src/script/ or crates/benilla-app/src/ui_script/ using the logged Lua stack lines.
- **Captured Evidence:**
  - `normalTexture (x2)`

## [AUTO-04] Register missing 3.3.5a CVars (10 unique)
- **Category:** CVars
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/cvars/`
- **Agent Prompt:**
  > Register the missing 3.3.5a CVars (nameplateShowFriends, showCastableDebuffs, fctDamageReduction, showItemLevel, showTargetOfTarget, showArenaEnemyCastbar, UnitNameFriendlyTotemName, talentFrameShown, fctAuras, nameplateShowEnemies) with their stock 12340 default values in crates/benilla-app/src/cvars/.
- **Captured Evidence:**
  - `talentFrameShown (x6)`
  - `fctAuras (x4)`
  - `showArenaEnemyCastbar (x4)`
  - `UnitNameFriendlyTotemName (x4)`
  - `showCastableDebuffs (x4)`
  - `showTargetOfTarget (x4)`
  - `showItemLevel (x4)`
  - `fctDamageReduction (x4)`
  - `nameplateShowEnemies (x2)`
  - `nameplateShowFriends (x2)`

## [AUTO-05] Fix Lua / FrameXML runtime errors (147 unique)
- **Category:** UI / Lua Runtime
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/`
- **Agent Prompt:**
  > Fix the logged Lua/FrameXML runtime errors in crates/benilla-ui or crates/benilla-app by inspecting the exact file:line callers in the diagnostic report.
- **Captured Evidence:**
  - `benilla_app::ui_script::extract: ui_script: error handler itself failed: runtime error: Interface\FrameXML\UIPanelTemplates.lua:365: attempt to perform arithmetic on field `cursorOffset' (a nil value) (x490)`
  - `benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\libs\SimpleSticky.lua:179: attempt to perform arithmetic on local `xB' (a nil value) (x348)`
  - `benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\ChannelFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping (x168)`
  - `benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value) (x48)`
  - `benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find inherited node: QuestFontNormalHuge (x44)`
  - `benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PartyFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping (x32)`
  - `benilla_app::ui_script::extract: ui_script: unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping (x20)`
  - `benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value) (x15)`
  - `benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\RestrictedFrames.lua:99: attempt to call method `IsProtected' (a nil value) (x13)`
  - `benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:280: attempt to call global `GetUnitMaxHealthModifier' (a nil value) (x10)`
  - `benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\BattlefieldFrame.lua:509: attempt to call global `GetRandomBGHonorCurrencyBonuses' (a nil value) (x9)`
  - `benilla_app::ui_script::lifecycle: ui_script: Wrath host source integration is incomplete; addon/API and rendered acceptance remain unverified (x9)`
  - `benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ChannelFrame.lua:716: attempt to index local `texture' (a nil value) (x8)`
  - `benilla_app::ui_script::extract: ui_script: SetOwner: unknown anchor 'ANCHOR_BOTTOM' (mode 0, ANCHOR_LEFT) (x7)`
  - `benilla_app::bindings: bindings(TOGGLEACHIEVEMENT): runtime error: Interface\FrameXML\UIParent.lua:352: attempt to call global `CanShowAchievementUI' (a nil value) (x7)`

## [AUTO-06] Fix DBC schema / record layout mismatches (2 unique)
- **Category:** DBC / Formats
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-formats/src/dbc.rs`, `crates/benilla-app/src/`
- **Agent Prompt:**
  > Repair the 3.3.5a (12340) DBC record reader / field count mismatches reported in the diagnostic log.
- **Captured Evidence:**
  - `benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MainMenuBar.lua:400: attempt to perform arithmetic on local `exhaustionStateMultiplier' (a nil value) (x2)`
  - `benilla_app::entities: pet stat tables unavailable, happiness/loyalty stay blank: reading DBFilesClient\PetLoyalty.dbc: file not in patch chain: DBFilesClient\PetLoyalty.dbc (x1)`

## [AUTO-07] Fix M2 / Skin / BLP / WMO asset load failures (21 unique)
- **Category:** Assets / M2 / BLP
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-formats/src/`, `crates/benilla-assets/src/`
- **Agent Prompt:**
  > Investigate and fix the M2/Skin/BLP/WMO asset load failures recorded in the diagnostic log.
- **Captured Evidence:**
  - `benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/gryphonroost/gryphonroost01.m2: Wrath anisotropic/variable scale requires runtime support (x3)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/passivedoodads/particleemitters/blacksmith_smoke.m2: Wrath wind/tumble particles remain guarded (x3)`
  - `benilla_assets::blp: BLP mpq://dungeons/textures/brian/abbey/piinetree01.blp: rejected mip 9; keeping 9 valid levels (x2)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/smokestack/smokestack.m2: Wrath wind/tumble particles remain guarded (x2)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://particles/bloodspurts/bloodspurt.m2: Wrath geometry/recursive particles remain guarded (x2)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://spells/bestowdisease_impact_chest.m2: Wrath geometry/recursive particles remain guarded (x1)`
  - `bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu1.blp (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://world/azeroth/westfall/passivedoodads/lamppost/westfalllamppost02.m2: Wrath lifespan/emission variation requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_mainmenu_northrend/ui_mainmenu_northrend.m2: Wrath lifespan/emission variation requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://spells/lightning_precast_low_hand.m2: Wrath lifespan/emission variation requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://spells/poisonshot_missile.m2: Wrath anisotropic/variable scale requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_orc/ui_orc.m2: Wrath lifespan/emission variation requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://spells/frost_nova_area.m2: Wrath anisotropic/variable scale requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://creature/elementalearth2/elementalearth2.m2: Wrath lifespan/emission variation requires runtime support (x1)`
  - `benilla_assets::m2: M2 particle data unavailable for mpq://spells/hunter_rapidfire.m2: Wrath anisotropic/variable scale requires runtime support (x1)`

## [AUTO-08] Fix 3D character/entity model attachment failure or settle timeout (7 unique)
- **Category:** 3D Models & Player Visuals
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/entities/attach/`, `crates/benilla-app/src/entities/equipment/`
- **Agent Prompt:**
  > Repair the character/entity 3D model attachment failure or equipment settlement timeout recorded by diag[ENTITY_MODEL].
- **Captured Evidence:**
  - `Player 16945v9 BodyAtlas pending timeout (3.83s), forcing atlas build (x1)`
  - `Player 13571v5 BodyAtlas pending timeout (3.83s), forcing atlas build (x1)`
  - `Player 1053v5 BodyAtlas pending timeout (3.83s), forcing atlas build (x1)`
  - `Player 22063v6 BodyAtlas pending timeout (3.62s), forcing atlas build (x1)`
  - `Player 12735v4 BodyAtlas pending timeout (3.62s), forcing atlas build (x1)`
  - `Player 12734v4 BodyAtlas pending timeout (3.62s), forcing atlas build (x1)`
  - `Player 12752v4 BodyAtlas pending timeout (3.62s), forcing atlas build (x1)`

## [AUTO-09] Resolve third-party AddOn loading, version, or dependency errors (43 unique)
- **Category:** AddOns & Third-Party UI
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/ui_script/addons.rs`, `crates/benilla-ui/src/script/addon.rs`
- **Agent Prompt:**
  > Fix the AddOn load refusal, dependency missing, or version mismatch recorded by diag[ADDON].
- **Captured Evidence:**
  - `!!!garbageprotector is DISABLED in AddOn enable configuration (x4)`
  - `Specialni Znaky is DISABLED in AddOn enable configuration (x4)`
  - `Details_DeathGraphs is DISABLED in AddOn enable configuration (x4)`
  - `MultiBot is DISABLED in AddOn enable configuration (x4)`
  - `pawn is DISABLED in AddOn enable configuration (x4)`
  - `aux-addon is DISABLED in AddOn enable configuration (x4)`
  - `fizzle is DISABLED in AddOn enable configuration (x4)`
  - `Quartz is DISABLED in AddOn enable configuration (x4)`
  - `GatherMate is DISABLED in AddOn enable configuration (x4)`
  - `tweakwow is DISABLED in AddOn enable configuration (x4)`
  - `libstratafix is DISABLED in AddOn enable configuration (x4)`
  - `Questie-335 is DISABLED in AddOn enable configuration (x4)`
  - `Skillet is DISABLED in AddOn enable configuration (x4)`
  - `Details_TinyThreat is DISABLED in AddOn enable configuration (x4)`
  - `!blizzbugssuck is DISABLED in AddOn enable configuration (x4)`

## [AUTO-10] Triage and resolve 64 recorded interaction incident(s)
- **Category:** Flight Recorder / Incidents
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/quest_trace.rs`, `crates/benilla-app/src/wrath.rs`, `crates/benilla-app/src/ui_quest/`
- **Agent Prompt:**
  > Inspect the flight-recorder incidents in latest-diagnostics.md (from D:\Projects\project-b-client\benilla-config\Logs\InteractionDiagnostics\interaction-1794440675417481200.jsonl), trace the terminal_stage and action chain, and fix any client-side refusal, decode rejection, or unconfirmed write defect.
- **Captured Evidence:**
  - `outcome=ServerReportedFailure confidence=Unmatched stage={"Cast":{"attempt":null,"edge":{"Interrupted":"@{reason=40}"},"spell_id":75,"wire_cast_count":1}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":42,"vm_session":5}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptLoadError":{"occurrences":1,"row_seq":4,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":15,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":16,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":2,"row_seq":18,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":19,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":20,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":21,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":22,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":23,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":24,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":25,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":26,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`
  - `outcome=ObservedFailure confidence=Exact stage={"ScriptError":{"occurrences":1,"row_seq":27,"vm_session":7}} subject={"count":null,"interaction":0,"item":null,"npc_alias":null,"operation":"Observed","quest":null,"reload_generation":0,"scope":[2,2],"slot":null,"ui_dispatch_id":null}`

## [AUTO-11] Investigate 4 quest/journal/turn-in correlation anomaly(ies)
- **Category:** Quests / Correlations
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/ui_quest/`, `crates/benilla-app/src/ui_quest_log.rs`, `crates/benilla-protocol-wrath/src/gameplay.rs`
- **Agent Prompt:**
  > Review the quest/journal/turn-in correlation anomalies captured by the flight recorder and fix any projection or packet-handling gap in crates/benilla-app/src/ui_quest/ or ui_quest_log.rs.
- **Captured Evidence:**
  - `[quest_ui_correlations] Quest action 33 Open (npc Some(6), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
  - `[quest_ui_correlations] Quest action 32 Open (npc Some(5), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
  - `[quest_ui_correlations] Quest action 31 Open (npc Some(4), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
  - `[quest_ui_correlations] Quest action 30 Open (npc Some(3), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`

## [AUTO-12] Investigate 11 cast or loot/vendor correlation anomaly(ies)
- **Category:** Combat / Cast / Loot / Vendor
- **Triage Label:** `ready-for-agent`
- **Target Files:** `crates/benilla-app/src/spell/`, `crates/benilla-app/src/ui_loot/`, `crates/benilla-app/src/ui_merchant/`, `crates/benilla-app/src/wrath.rs`
- **Agent Prompt:**
  > Inspect the cast and loot/vendor correlation anomalies from the flight recorder and repair any client-side state or protocol mismatch.
- **Captured Evidence:**
  - `[cast_correlations] Cast action unknown (local action not retained) spell=8492 wire=8 attempt=9 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
  - `[cast_correlations] Cast action 46 spell=8492 wire=8 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
  - `[cast_correlations] Cast action unknown (local action not retained) spell=8422 wire=7 attempt=8 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
  - `[cast_correlations] Cast action 45 spell=8422 wire=7 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
  - `[cast_correlations] Cast action 44 spell=8422 wire=unknown attempt=unknown scope=Some((2, 2)): Requested confidence=unverified; no native evidence retained; trace has gaps`
  - `[cast_correlations] Cast action unknown (local action not retained) spell=8422 wire=6 attempt=7 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
  - `[cast_correlations] Cast action 43 spell=8422 wire=6 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
  - `[cast_correlations] Cast action 42 spell=8422 wire=unknown attempt=unknown scope=Some((2, 2)): Requested confidence=unverified; no native evidence retained; trace has gaps`
  - `[loot_vendor_correlations] Action 34 Sell (npc Some(7), item Some(4377), count None, slot Some(16)): sent; no matching server evidence in 30s (unverified); trace has gaps`
  - `[loot_vendor_correlations] Action 16 Loot (npc Some(1), item Some(2934), count Some(1), slot Some(0)): loot release observed (ambiguous); trace has gaps`
  - `[loot_vendor_correlations] Action 14 Loot (npc Some(1), item Some(769), count Some(1), slot Some(0)): loot release observed (ambiguous); trace has gaps`


