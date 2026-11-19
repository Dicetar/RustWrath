# 18 — Inventory the complete stock replacement gate

**Status:** ready-for-agent

**Plan reference:** C03. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A complete, reviewable compatibility ledger makes it impossible to label the fork a stock-client replacement while required behavior is missing.

**Blocked by:** None — can start immediately.

**Related existing issues:** [14](14-g1-expansion-gate.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** Astra with owner review; workers supply existing coverage facts.

## Acceptance

- [ ] Inventory pinned client-facing server messages, stock UI/API/events, asset variants, settings and installed addon builds; reconcile against all families in the plan.
- [ ] Record one row per independently assessable behavior with owner, dependencies, reference evidence, normal/error/reconnect/persistence scenarios and status.
- [ ] Separate baseline 3.3.5 features, owner-installed extensions, and experimental capabilities without dropping any mandatory release requirement.
- [ ] Record all required rows as WORKING, PARTIAL, BROKEN or UNKNOWN with source-observed/reported/reproduced evidence labels.
- [ ] Identify server-disabled or unavailable scenarios as blocked requirements; do not call them unsupported-by-choice without owner agreement.
- [ ] Produce the first dependency-ordered compatibility slices and a measurable fixed denominator for completion reporting.
- [ ] Record full stock-client replacement as a mandatory release gate; no subset milestone closes it.

## Boundaries and handoff

This is a bounded inventory/decision deliverable, not a claim that the broad compatibility implementation fits one agent context. Do not modify the owner's scope through inference.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Inventory family (R06)

Source-level package A (2026-11-05, stacked on `47cba738`): the typed Wrath inventory owner
(`crates/benilla-app/src/wrath_inventory.rs`) now carries 28 bank slots, seven bank bags, the
32-slot keyring and presence-aware item addresses; the donor projection remains 24/6/7 and is
never widened into occupied item offsets. Bank/bag consumers migrated at source: bank containers
and `GetContainerNumSlots(-1)=28`, bags 5..11, 7th purchase/full state, deposit/withdraw wire
39..66 and 67..73, bank-bag button band, `HasKey`, repair/cooldown scans,
`PLAYERBANKSLOTS_CHANGED`/`BAG_UPDATE`, `ui_bank`, `ui_items`, `ui_char`, `ui_merchant` and the
`benilla-ui` script ranges. The item payload tail (12 enchant slots, create-played-time, item text
by guid) is deferred as S3; the general `count_of`/`find_item`/action-bar walks remain donor and
are named in the RFC.

Evidence: SOURCE-OBSERVED only. Not compiled, not focused-checked, not receiver-verified, not
visible gameplay. Acceptance boxes stay unchecked. Runtime acceptance needs an owned character
with an item in bank slot 25 and a bag in bank bag slot 7 (seven purchased bank-bag slots); no
server/DB change was made. Contract and 18-row ledger:
[lossless Wrath inventory](../architecture/rfc-lossless-wrath-inventory.md) and
[traceability](../architecture/architecture-traceability.md).

## Wave 1/2 compatibility addendum — U1–U3, L2–L3 (2026-11-05, base `906a0812`)

This addendum is the audit result for packages U1–U3 and L2–L3. It appends to the historical R06
inventory above and does not replace it. Every row is **SOURCE-OBSERVED** unless a focused test is
named; no row is compiled, receiver-verified or visible gameplay. Addon files were read from the
actual install at `D:\WOW\world of warcraft 3.3.5a hd\Interface\AddOns`; hashes are the first 16
hex digits of SHA-256 and identify the file only, not a release decision. The `patch-z.mpq` copy of
`WMBridge` was not hashed, and no loose-vs-archive winner is inferred.

**Fixed L2/L3 denominator.** 17 F rows in
[release-matrix-2026-11-05.md](../../docs/architecture/release-matrix-2026-11-05.md) + 15 D rows
there = **32**. Status: **WORKING 0 · PARTIAL 25 · BROKEN 3 · UNKNOWN 4**. This addendum's
family tables are diagnostic detail under those 32 rows, not extra denominator entries.

### A. Installed build inventory

|Addon|Actual file(s) and hash prefix|TOC / interface|Declared saved state|Required host contract|Status|
|---|---|---|---|---|---|
|WMBridge 0.1.0|`WMBridge\WMBridge.lua` 20,086 B `267ec1624f3f774e`; `.toc` `27e9145ae34dc08a`|`30300`; `## SavedVariables:` empty|none|`SendAddonMessage` CHANNEL-or-WHISPER (`transport=SELF_WHISPER` normalized on fallback), `GetChannelName` (confirmed only on `Wrath335`), channel join/remove, chat filters, `COMBAT_LOG_EVENT_UNFILTERED`, item/cursor/tooltip calls|PARTIAL · focused test (D01)|
|WM_Multiclass (interface 30300)|`WM_Multiclass.lua` 13,175 B `cc4b994ad01c2ab3`; `.toc` `80d96e18a4d28dcb`; `WM_SpellmakingV3.lua` 68,856 B `c6bdf814772a8f9e`|`30300`, no `## Version`; `SavedVariablesPerCharacter: WM_MulticlassSettings`|per character `WM_MulticlassSettings` (`spellwrightV3.draft`, `xpAnchor`)|`MulticlassRequest` returning `1`, `MulticlassPoll` returning one hex packet, `UnitClass`, `UnitPower`/`UnitPowerMax` runic, `GetSpellInfo`, `UIDropDownMenu_*`, secure action attributes|PARTIAL · focused test (D02)|
|WMQuestieCompat 0.1.0|`WMQuestieCompat.lua` 1,701 B `6edb2f3e7812161e`; `.toc` `0c167c0d7cc6a9b0`|`30300`; `OptionalDeps: Questie-335`|none|`ADDON_LOADED`/`PLAYER_LOGIN`, `Questie.Error`, `Questie.db.char.TrackedQuests`|PARTIAL · focused test (D04)|
|Questie-335 9.5.1|`Questie-335.toc` `1ce807739228ce81`; `Questie.lua` `9645d91e7e965dfd`|Seven TOCs; `Questie-335.toc` is `30300`, `Questie-WOTLKC.toc` is `30403` and is not the 3.3.5 selection|`QuestieConfig` account; `QuestieConfigCharacter` per character|stock quest-log/map APIs plus its own `QuestieCompat.C_Map`/`C_Timer`/`C_QuestLog` shims|PARTIAL · focused test (D04)|
|Bagnon 2.13.3|`Bagnon.toc` `835981e2b9ecab9a`; `main.lua` `c21f7a6b8be4e209`|`30300`; `SavedVariables: BagnonGlobalSettings`; `SavedVariablesPerCharacter: BagnonFrameSettings`|account and per character|`GetContainerItemInfo` 7 values, `GetContainerItemQuestInfo`, `GetItemInfo` 11 values, `GetItemQualityColor`, `GetContainerItemCooldown`|PARTIAL · focused test (D05)|
|Bartender4 4.4.2-2-g3b02ee4|`bartender4.toc` `a0b9187368da0c2a`|`30300`; `SavedVariables: Bartender4DB`|account (`Bartender4DB`)|`SecureHandlerStateTemplate`, `RegisterStateDriver`, `UnregisterStateDriver`, `WrapScript`, `GetActionInfo` 3 values, `GetShapeshiftFormInfo` 4 values, `GetPetActionInfo` 7 values|PARTIAL · focused test (D06)|

The Bartender4 directory also carries `changelog-bartender4-4.4.2-12-g94f3b58.txt`; that is a
different build's changelog, not the loaded TOC version. No version is inferred from file presence.

### B. Stock FrameXML/GlueXML surface these addons require

Return arity is the Wrath-mode contract; donor-mode divergences are called out. "Empty" means the
reference's zero-return or nil shape, not a fabricated success.

|API / widget method|Contract relied on|Host locator|Consumer|Status|
|---|---|---|---|---|
|`GetContainerItemInfo(bag, slot)`|texture, count, locked, quality, readable, lootable, link; empty/unknown = zero returns|`container.rs` 1244–1315|Bagnon `utility/itemSlotInfo.lua` 18; WMBridge stone|source matches (`bagnon_item_slot_info_reads_wrath_seven_tuple`)|
|`GetContainerItemQuestInfo(bag, slot)`|isQuestItem, questID, isActive|`container.rs` 1003|Bagnon `components/item.lua` 574|source matches|
|`GetContainerItemCooldown(bag, slot)`|start, duration, enable|`container.rs` 1177|Bagnon `utility/itemEvents.lua` 88/119/145|source matches|
|`GetContainerNumSlots(bag)`|number, 0 outside the range; Wrath accepts -2..11|`container.rs` 1116|Bagnon, bank|source matches|
|`GetItemInfo(item)`|name, link, quality, itemLevel, reqLevel, type, subType, stack, equipLoc, texture, sellPrice; unknown = zero returns|`item_stats/wrath.rs` 147–167|Bagnon `itemSlotInfo.lua` 22; ActionBar tooltips|source matches|
|`GetItemCount(item[, bank[, uses]])`|number, 0 on miss in Wrath 3.3.5|`item_stats/wrath.rs` 131–160|stock bags/Bagnon neighbors|source matches|
|`GetItemQualityColor(quality)`|r, g, b, hex|`item_stats.rs` 608|Bagnon `components/item.lua` 383/400|source matches|
|`GetActionInfo(slot)`|3.3.5 returns type, id, subType (3 values in `Wrath335`)|`action.rs` 408–445|Bartender4 `actionbutton.lua` 106 (`subtype` feeds `IsHelpfulSpell`); WM_SpellmakingV3 281|source matches|
|`GetActionCooldown(slot)`|start, duration, enabled|`action.rs` 735|Bartender4 action button|source matches|
|`GetActionTexture(slot)`|texture or nil|`action.rs` 375|Bartender4 `actionbutton.lua` 266|source matches|
|`IsUsableAction` / `IsActionInRange` / `ActionHasRange`|1/nil or boolean pairs as the reference pushes|`action.rs` 611/623/645|Bartender4 `actionbutton.lua` 25/228|source matches|
|`GetShapeshiftFormInfo(index)`|texture, name, active, castable; out of range four nils|`shapeshift.rs` 94–113|Bartender4 `stancebar.lua` 82/170|source matches|
|`GetPetActionInfo(index)`|name, subtext, texture, isToken, isActive, autocastAllowed, autocastEnabled; empty seven nils|`pet.rs` 475–492|Bartender4 `petbutton.lua` 96/200|source matches|
|`GetBindingKey(command)`|every bound key as a multi-return; none = zero returns|`keybind.rs` 581|Bartender4 `actionbutton.lua` 377–424|source matches|
|`GetQuestLogTitle(index)`|Wrath order title, level, tag, group, header, collapsed, complete, daily, questID; optional display-quest-id bool|`quest_log.rs` 1294–1408|Questie `QuestieCompat.GetQuestLogTitle` (Compat.lua 417–424)|source matches (`wm_questie_compat_suppresses_only_900000_plus_errors`)|
|`GetQuestLogLeaderBoard(i, quest)`|text, type, finished; missing = three nils|`quest_log.rs` 1472–1494|Questie `Compat.lua` 380|source matches|
|`GetNumQuestLogEntries()` / reward/choice/count getters|two counts; detail counts/rows|`quest_log.rs` 1301, 1554–1710|Questie quest log cache|source matches|
|`GetBuildInfo()`|version, build, date, toc|`client.rs` 137; `glue_core.rs` 886|Questie `QuestieCompat.Is335`|source matches|
|`GetSpellInfo(spell)`|host exposes 9 values: name, rank, icon, cost, channeled, powerType, castMs, minRange, maxRange|`wrath_spell.rs` 112–125|WM_SpellmakingV3 277–278 assumes at least 9; stock 3.3.5 expects 9|source matches|
|`SendAddonMessage(prefix, message, distribution, target)`|WHISPER admitted, normalizes `WMBridge` fallback `transport=SELF_WHISPER`, and chunks oversized `WM_FORGE3` whispers into `<=150`-byte `DATA|` frames within the 255-byte server cap|`addon_message.rs` 90–165|WMBridge 135–159; WM_SpellmakingV3 36|source matches (`wm_spellmaking_self_whisper_round_trip`)|
|`GetChannelName(nameOrIndex)`|id, name or nil, instance; requires confirmed slot on `Wrath335`|`channel.rs` 340–374|WMBridge `ensureChannel`|source matches|
|`CreateFrame(kind, name, parent, template)`|admits `SecureHandlerBaseTemplate`, `SecureHandlerClickTemplate`, `SecureHandlerStateTemplate`|`secure.rs` 54–78|Bartender4 secure bars|source matches (bounded non-executing)|
|`RegisterStateDriver` / `UnregisterStateDriver` / `WrapScript`|bounded non-executing secure state-driver globals and frame method|`secure.rs` 80–215|Bartender4 `bar.lua` 133/407/467, `bartender4.lua` 179–199, `actionbutton.lua` 88/97|source matches (bounded non-executing)|
|`hooksecurefunc`|post-hook a global or method|`posthook.rs` 29|WMBridge 465/476; WM_SpellmakingV3 290–305|source matches|
|`UIDropDownMenu_*`|stock FrameXML template and functions|shipped FrameXML, not a Rust native|WM_Multiclass 30–38|not runtime-verified|

GlueXML-specific dependencies checked: `multiclass_glue.lua` uses `GetCharacterInfo` and
`CharacterSelect.selectedIndex` (`glue_select.rs` 560/568); `QuestieCompat.Is335` uses
`GetBuildInfo`; both exist. `WM_Multiclass` world and glue requests are bound via `MulticlassRequest`
and `MulticlassPoll` (`wrath_multiclass_request_matches_installed_addon_contract`).

Generic widget methods used by Bagnon and Bartender4 (`SetBackdrop`, `SetBackdropColor`, `SetMovable`,
`StartMoving`/`StopMovingOrSizing`, `StartSizing`, `SetResizable`, `SetHitRectInsets`,
`SetClampedToScreen`, `SetToplevel`, `SetUserPlaced`, `CreateTexture`/`CreateFontString`,
`SetTexCoord`, `SetJustifyH`/`SetJustifyV`, `SetShadowColor`/`SetShadowOffset`) are implemented in
`object/frame_state.rs`, `object/movable.rs`, `object/toplevel.rs` and `region/text.rs`; this audit
did not re-verify every call. `UIPanelButtonTemplate` and `ActionBarButtonTemplate` ship in the XML
(`ui_script/action_bar_tests.rs` 1596–1657). The blocking gaps are semantic and secure, not method
absence.

### C. Action slots, pages, bindings, macros and reload state

|Behavior|Producer / writer|Host locator|Test or missing test|Status|
|---|---|---|---|---|
|Action slots 1..144|`SMSG_ACTION_BUTTONS` parse `gameplay.rs` 2172, decode 3591–3609; tap cap `wrath.rs` 5384|`spell/net.rs` 717–748 replaces the map on state 1|`spell::net::tests::wrath_action_buttons_144_cap`|PARTIAL|
|Dual-spec state 0/1/2|12340 `Player_C.cpp` 4084–4124: state 0 snapshots into `ActionButtonsPending::held`, state 1 applies and clears `pending`, state 2 marks `pending` without clearing live slots|`spell/net.rs` 706–748 (`ActionButtonsPending`); `wrath.rs` 3717–3739|`spell::net::tests::wrath_action_buttons_state_two_keeps_the_bar`|PARTIAL (source + unit-tested; no spec-switch live run)|
|Page switching|`CURRENT_ACTIONBAR_PAGE` FrameXML-local; `ChangeActionBarPage` fires event (`action.rs` 554–584)|no server packet|stock action-bar tests `ui_script/action_bar_tests.rs`|source matches|
|Bar visibility toggles|`SetActionBarToggles` sends `CMSG_SET_ACTIONBAR_TOGGLES`; getter reads server byte|`action_bar_toggles.rs` 44–86|`set_action_bar_toggles` tests|source matches|
|Per-character keybindings|account diff then `<Realm>-<Char>.txt`; character file presence activates set 2|`bindings.rs` 245–276; `bindings/store.rs` 36–96|`bindings::store::tests::the_diff_round_trips_and_defaults_stay_silent`; `...resolve_steals_across_commands...`|PARTIAL (no relog run)|
|Macros, account vs character|18 account + 18 character; character ids rebased by `0x1000000`|`macros.rs` 7–28, 152–164; `ui_macro/store.rs` 62|`ui_macro::tests` scope tests|PARTIAL (no relog run)|
|`ReloadUI` persistence|save layout, saved variables, addon variables and enable state before VM close|`lifecycle.rs` 1003–1018, 1163+; `addons.rs` 604–731|`ui_script::addons::tests::the_enable_state_round_trips_through_the_file`; `an_addons_saved_variables_survive_the_session`|PARTIAL (no reload run)|
|Addon enable/disable state|per-character `AddOns.txt` hash; realm agreement fallback; TOC `## DefaultState`|`addons.rs` 604–716|same enable-state test|PARTIAL (no relog run)|

### D. Wave 1/2 inbound seams: real consumer, donor projection, or no owner

|Seam|Parse / decode|App translation|Real app/UI consumer|Donor projection or loss|No state owner|Test or missing test|
|---|---|---|---|---|---|---|
|Pet spells/mode/feedback/sounds|`gameplay.rs` 2233–2241; `pet_talent.rs`|`wrath.rs` 4239–4295|`ui_pet/net.rs` 53/80/91/326–411 (`PetBar::pet_guids`, `pet_spell_learned`, `pet_spell_removed`)|stable list synthesizes slots and loyalty 0|none|`pet_talent.rs::tests::pet_spells_reads_family_bar_spells_and_32_bit_cooldown`; `ui_pet/net.rs::tests::pet_guids_and_learned_removed_spells_reach_the_pet_bar`; `wrath_pet_order_reaches_handle_pet_action`|
|Talent info/reset|`gameplay.rs` 2252–2254; `pet_talent.rs`|`wrath.rs` 4296–4331|`ui_talent.rs` (`WrathTalentState`, `build_wrath_talent_groups`, `build_pet_talent_views`, `set_wrath_talents`); `talent.rs` (`isPet` & multi-group support)|none; dual-spec groups, pet talent tab, and glyph sockets projected to Lua|none|`pet_talent.rs::tests::player_talents_info_reads_every_field`; `ui_talent::tests::talents_info_updates_the_talent_window`; `ui_talent::tests::pet_talent_tab`; `wrath_dual_spec_talents_and_glyph_sockets_read_groups`|
|Stable|`gameplay.rs` 2248–2251|`wrath.rs` 4332–4367|`ui_stable/net.rs` 26/46|12340 list has no loyalty column, filled 0|none for list/result; stable swap and purchase display unverified|`pet_talent.rs::tests::stable_list_reads_current_and_stabled_rows`; `ui_stable/mod.rs::a_refused_stabled_pet_is_asked_at_the_next_list_never_by_a_frame`|
|Trade/mail/auction|`gameplay.rs` 2266–2299; `economy.rs`|`wrath.rs` 5076–5149|`ui_trade/net.rs` 58; `ui_mail/net.rs` 54/110/131; `ui_auction/net.rs` 448|none; all 12 mail attachments, inline body, `trade_id`, and `socket_enchant` preserved|none|`economy::tests::wrath_mail_list_preserves_all_attachments_and_inline_body`; `economy::tests::wrath_trade_extended_preserves_trade_id_and_socket_enchants`|
|Party/social/guild|`gameplay.rs` 2300–2346; `social.rs`|`wrath.rs` 4493, 4709–4772|`ui_party/net.rs` 110; `ui_social/mod.rs` 722 (`WhoisText`); `ui_guild/mod.rs` 707–754; `ui_chat/net.rs` 90|none; 32-bit HP/pet HP, 64 aura slots, `vehicle_seat`, and 12340 LFG role bits preserved|none|`social::tests::wrath_party_stats_preserves_32bit_hp_64_aura_slots_and_vehicle_seat`; `social::tests::wrath_group_list_packs_subgroup_and_assistant_flags_for_self_and_members`|
|LFG / battleground|`gameplay.rs` 2347–2365; `wrath_systems.rs` 182–602|`wrath.rs` (`LfgJoinResult`, `LfgQueueStatus`, `LfgUpdatePlayer`, `LfgUpdateParty`, `LfgProposalUpdate`, `wrath_battlefield_status`, `wrath_pvp_log_data`)|`ui_script/wrath_rune_calendar_feed.rs`; `wrath_natives.rs`|none|none|`wrath_natives.rs::tests::lfg_join_sends_from_the_lfd_frame`; `wrath_natives.rs::tests::wrath_lfg_and_vehicle_natives_read_server_state_and_emit_scoped_commands`|
|Calendar / rune / vehicle|`gameplay.rs` 2366–2374; `wrath_systems.rs` 860–1391|`wrath.rs` (`WrathFeedEvent::RuneState`/`RuneConvert`/`RunePower`/`Calendar`/`CalendarCommandResult`, `vehicle_guid`)|`ui_script/wrath_rune_calendar_feed.rs`; `wrath_natives.rs`|none|none|`wrath_natives.rs::tests::calendar_get_calendar_refreshes_native_date_and_invites`; `wrath_natives.rs::tests::rune_frame_reads_resync_state`; `wrath_natives.rs::tests::vehicle_exit_uses_the_seat_transport`|

`SessionEvent` producer additions in this workstream: `PetSpellLearned`/`PetSpellRemoved`/
`PetGuids`/`TalentInfo`/`TalentsInvoluntarilyReset` ([events.rs] 627–645), `WhoisText` (1149), and
the widened `AuctionHello { enabled }` (1225). `PetLearnedSpell` is a wrath-gameplay spelling mapped
to `PetSpellLearned` at `wrath.rs` 4290. Calendar, rune, LFG, and vehicle events travel
`WrathFeedEvent` into `WrathWorldView` and `ui_script/wrath_rune_calendar_feed.rs`.

### E. Decoded records and state owners (resolved)

|Decoded record|Decoder locator|State owner & consumer|Focused test|
|---|---|---|---|
|LFG join result / queue status / player+party update / proposal|`wrath_systems.rs` 268–560|`WrathWorldView` + `WrathLfgState` in `wrath_rune_calendar_feed.rs` and `wrath_natives.rs`|`lfg_join_sends_from_the_lfd_frame`; `wrath_lfg_and_vehicle_natives_read_server_state_and_emit_scoped_commands`|
|Battleground status / PvP log|`wrath_systems.rs` 620–859|`wrath.rs::wrath_battlefield_status` and `wrath_pvp_log_data`|`battlefield_status_queued_arm`|
|Vehicle seat/exit|`wrath_systems.rs` 1259–1391|`WrathWorldView::vehicle_guid` + `WrathScopedCommand::Vehicle*` in `wrath_natives.rs`|`vehicle_exit_uses_the_seat_transport`|
|Pet talent tab UI & glyph sockets|`pet_talent.rs` `read_talents_info`; `ui_talent.rs` `WrathTalentState`|`WrathTalentUiState` (`pet_tabs`, `pet_talent_views`, `groups`) in `talent.rs`|`pet_talent_tab`; `wrath_dual_spec_talents_and_glyph_sockets_read_groups`|
|Mail multi-attachment|`economy.rs` mail reader; `wrath.rs` 5076|`MailListEntry::attachments` (12 slots) and `inline_body` in `ui_mail/net.rs`|`wrath_mail_list_preserves_all_attachments_and_inline_body`|
|Trade socket enchant and 3.3.5 trade id|`economy.rs` trade reader; `wrath.rs` 5047|`TradeItem::socket_enchant` and `TradeStatusExtended::trade_id` in `ui_trade/net.rs`|`wrath_trade_extended_preserves_trade_id_and_socket_enchants`|

### F. `wrath_natives.rs`: real state versus stubs

Real, state-backed at source: `GetNumTitles`/`GetCurrentTitle`/`IsTitleKnown`/`GetTitleName`
(304–356), `GetCombatRating` (367–383), `IsPetAttackAction` (392–415), `UnitIsAFK`/`UnitIsDND`
(417–443), LFG aura predicates and cooldown expirations (918–936), `GetAvailableRoles` (941–961),
`GetModifiedClick`/`IsModifiedClick` (239–274), `GetExpansionLevel`/`GetAccountExpansionLevel`
(741–752), `CanExitVehicle`/`VehicleExit`/`VehiclePrevSeat`/`VehicleNextSeat`/`UnitSwitchToVehicleSeat`,
`GetLFGQueuedList`/`GetLFGProposal`/`GetLFGProposalMember`/`GetLFGInfoServer`/`GetLFGQueueStats`/`IsPartyLFG`/`GetLFGRoles`/`SetLFGRoles`/`SetLFGComment`/`SetLFGDungeon`/`ClearAllLFGDungeons`/`JoinLFG`/`LeaveLFG`/`AcceptProposal`/`RejectProposal`,
`CalendarGetDate`/`CalendarGetNumPendingInvites`/`OpenCalendar`, and `GetRuneType`/`GetRuneCooldown`/`GetRuneCount`.

Constant/stub (not required by Northshire alpha route): `GetNumBattlegroundTypes` and
`GetNumArenaOpponents` (0), `GetWorldPVPQueueStatus` ("none"), `GetPossessInfo`/`IsPossessBarVisible`
(no possession), `GetTotemInfo` (empty), `GetMirrorTimerInfo` (UNKNOWN), `GetHonorCurrency` (0).

### G. Dependency-ordered next slices

1. **U4 contract repair (D02)** — make `MulticlassRequest` return `1` and enqueue the caller's own
   serial/args; make `MulticlassPoll` return one bounded `WMC1` hex packet or nil. Add the
   installed-addon contract test before any WMC1 receiver work.
2. **U1 load-blocker surface (D06)** — implement or explicitly reject `SecureHandlerStateTemplate`,
   `RegisterStateDriver`/`UnregisterStateDriver` and `WrapScript`; then load Bartender4 alone.
3. **U2 transport truth (D01)** — emit `COMBAT_LOG_EVENT_UNFILTERED` in Wrath mode, report the
   actual WMBridge transport in HELLO, and receiver-check HELLO/KILL/TOWM.
4. **Pet/talent state owners (D10)** — feed `TalentInfo`/`PetGuids` into the talent/pet UI and add
   the pet talent tab; then owner-driven pet order and stable swap.
5. **Economy state widening (D11)** — extend `MailListEntry` and `TradeStatusExtended`, render all
   attachments/body and the three socket words; receiver-check with a second character.
6. **Social widening (D12)** — add the whois text consumer and widen party HP/aura fields; keep the
   donor structs' nil semantics elsewhere.
7. **Action-state correctness (D07)** — retain `SMSG_ACTION_BUTTONS` state, ignore state 2 payload,
   apply only state 1, and cap at 144; then the spec-switch gate.
8. **LFG/calendar/rune/vehicle owners (D13/D14)** — start with LFG join + proposal because it uses
   the real 3.3.5 queue and has a two-client acceptance gate; calendar and rune follow; vehicle
   last because it has no inbound decoder yet.

**Verification gaps in this audit.** The AzerothCore handler bodies under
`D:/WOW/WM_BridgeLab/src/azerothcore/src/server/game/Handlers` were not re-read; the handler names
above come from the existing matrix. WM `MulticlassProtocol.h`/`MulticlassTransport.cpp` were not
re-read; the addon-side expectations were read directly. The `patch-z.mpq` WMBridge copy was not
hashed or extracted. No Cargo, rustc, client, server or database command was run.



## 2026-11-20 stock-area inventory (OpenCode swarm)

Four workers surveyed one stock UI area each from `origin/main` at `8b7a42fd`. Every row is
SOURCE-OBSERVED or names one focused test; no Cargo, client, server or database command was run.
The installed 3.3.5 `Interface` folder has no loose FrameXML, so the conventional stock file names
are used. Rows already in the tables above are not repeated.

### Bags, bank, merchant, trade, mail, auction

| Stock feature | Status | Evidence | File or "missing" | What our host/consumer does |
|---|---|---|---|---|
| Backpack and equipped bag windows (ContainerFrame) | PARTIAL | focused test: bag_tests::backpack_toggle_plays_open_and_close_kits | crates/benilla-ui/src/script/container.rs (ContainerState, set_container); stock: ContainerFrame.xml | UiScript model stores per-bag snapshots; B/Shift-B toggles backpack/all bags; item-push updates land in the owning bag icon |
| Bag bar buttons and open-state lighting | PARTIAL | focused test: bag_tests::bag_bar_buttons_light_while_their_bag_is_open | crates/benilla-ui/src/script/container.rs (bag bar); stock: ContainerFrame.xml | Bag-bar buttons track open state, name themselves on hover, and drive equipped-bag windows |
| Item slot interactions (pickup, split, use, cooldown) | PARTIAL | focused test: bag_tests::drag_across_two_slots_queues_the_same_move_a_click_pickup_would | crates/benilla-ui/src/script/container.rs (pickup_container_item, queue_split, take_container_uses); stock: ContainerFrame.xml | Pickup/split/use queue UseContainerItem/SplitItem calls; slot cooldowns sweep from XML; gift-wrap arming locks paper and sets cast cursor |
| Keyring (32 slots) and HasKey gating | PARTIAL | focused test: bag_tests::the_keyring_button_opens_a_keyring_window | crates/benilla-ui/src/script/container.rs (set_has_key, keyring surface); stock: ContainerFrame.xml | HasKey-driven keyring button on the bar; first key shows it; hovered keys tooltip through their inventory slot; dropped keys file themselves |
| Bank frame and 28 bank slots (-1) | PARTIAL | focused test: bank_tests::item_slot_paints_from_container_minus_one_and_repaints_on_playerbankslots_changed | crates/benilla-ui/src/script/bank.rs (set_bank) + crates/benilla-app/src/ui_bank/mod.rs,net.rs; stock: BankFrame.xml | SMSG_SHOW_BANK opens bank; container -1 paints 28 slots; repaints on PLAYERBANKSLOTS_CHANGED; close queues close and popouts |
| Bank bags and slot purchase flow | PARTIAL | focused test: bank_tests::purchase_flow_shows_popup_queues_the_intent_and_the_row_hides_when_full | crates/benilla-ui/src/script/bank.rs (BankCall::Purchase) + crates/benilla-app/src/ui_bank; stock: BankFrame.xml | Bags 5..11, bank-bag button band tints by purchase count, purchase popup queues CMSG_BUY_BANK_SLOT, row hides when full |
| Merchant buy/sell/buyback/repair | PARTIAL | focused test: merchant_tests::merchant_tabs_drive_buyback_page_and_repair_pair | crates/benilla-ui/src/script/merchant.rs + crates/benilla-app/src/ui_merchant/mod.rs,net.rs; stock: MerchantFrame.xml | Vendor inventory opens all equipped bags; buy/sell/buyback pages and repair tab; stock counts update on SMSG_BUY_ITEM |
| Merchant hover, sell price and buy cursor | PARTIAL | focused test: merchant_tests::shipped_merchant_frame_arms_the_buy_cursor_on_hover | crates/benilla-ui/src/script/merchant.rs + container.rs (UiCursorMode::Buy/UnableBuy); stock: MerchantFrame.xml | Hover scopes highlight and anchors item tooltip; arms buy/unable-buy cursor; ctrl/shift preview and post without buying |
| Trade window and offers/gold | PARTIAL | focused test: ui_trade::tests::the_trade_verbs_send_inside_their_calls | crates/benilla-ui/src/script/trade.rs + crates/benilla-app/src/ui_trade; stock: TradeFrame.xml | Trade status/extended state drives open/accept/close; slot clicks, money edits and accepts send inside their calls; trade id and socket enchants preserved |
| Mail inbox, read, send and attachments | PARTIAL | focused test: ui_mail::verb_tests::each_mail_verb_is_on_the_wire_before_the_next_line | crates/benilla-ui/src/script/mail.rs + crates/benilla-app/src/ui_mail; stock: MailFrame.xml | Mail list/send/result/item-text handlers; all 12 attachments, inline body, checked COD/return flags; inbox verbs send in call order |
| Auction house browse/bid/buyout/post | PARTIAL | focused test: ui_auction::verb_tests::a_bid_refuses_or_sends_inside_the_call | crates/benilla-ui/src/script/auction.rs + crates/benilla-app/src/ui_auction; stock: AuctionFrame.xml | Hello/list/owner/bidder handlers, house lane; browse/owner/bidder pages sort and patch rows; bid refusal and money checks |
| Trainer window | PARTIAL | focused test: trainer_tests::shipped_trainer_frame_drives_end_to_end | crates/benilla-app/src/ui_trainer/mod.rs,net.rs + crates/benilla-ui script; stock: ClassTrainerFrame.xml | Trainer show/update, service list collapse/filter, learn spell, buy succeeded logging; saved filter state |
| Craft/tradeskill window | PARTIAL | focused test: ui_tradeskill::tests::law_c_paints_the_created_items_icon | crates/benilla-app/src/ui_tradeskill.rs + crates/benilla-ui/src/script/craft.rs; stock: TradeSkillFrame.xml | TradeSkill show/update, recipe list, skill rank with permanent bonus, create-item icon resolution |
| Item tooltip (GameTooltip item) | PARTIAL | focused test: tooltip_compare_tests::doll_hover_renders_the_live_instance | crates/benilla-ui/src/script/tooltip_*.rs + crates/benilla-app/src/ui_unit/tooltip_facts.rs; stock: GameTooltip.xml | Wrath GetItemInfo 11 values, quality color, link identity, compare plates, repair cost and hover anchors |
| Cursor modes for items and containers | PARTIAL | focused test: bag_tests::vendor_bag_hover_shows_sell_price_and_arms_the_pouch_cursor | crates/benilla-ui/src/script/container.rs (UiCursorMode, custom_cursor, take_cursor_write); stock: Cursor.xml | Buy/unable-buy/inspect/point/cast/attack/custom modes; custom 32x32 texture load; reset preserves armed spell cursor |
| Money frame and purse display | PARTIAL | focused test: money_frame_tests::a_money_frame_loads_as_the_player_purse_and_splits_the_denominations | crates/benilla-ui/src/script/cursor/money.rs + money_frame tests; stock: MoneyFrame.xml | Purse splits across copper/silver/gold buttons; seven money types; color and collapse handling |

Smallest missing rows:
- Bank slot 25+ and seventh bank-bag runtime proof is unverified: no receiver check for the 28-slot/7-bag widening (crates/benilla-app/src/wrath_inventory.rs, ui_bank/net.rs).
- Trade/mail/auction UI surfaces lack app-level ui_script tests equivalent to bag/bank/merchant; only net/verb tests exist (crates/benilla-app/src/ui_trade, ui_mail, ui_auction).
- Item tooltip line-law and negative-suffix scaling remain incomplete for native item links (crates/benilla-app/src/ui_unit/tooltip_facts.rs, item_stats).

### Action bars, spellbook, talents, macros, key bindings

| Stock feature | Status | Evidence | File or "missing" | What our host/consumer does |
|---|---|---|---|---|
| Action button drag/drop and cursor placement | PARTIAL | `ui_script/action_bar_tests.rs` (`shipped_action_bar_drives_end_to_end`) | `crates/benilla-ui/src/script/cursor/bar.rs` (stock: `ActionBarFrame.xml`) | Pushes action slot payloads, accepts drops onto bar slots, clears cursor on dangling macros, and feeds `ActionButton.lua`. |
| Action button state flags (usable, range, cooldown, equipped, auto-repeat, attack) | PARTIAL | `ui_script/action_bar_tests.rs`; `action.rs` unit tests | `crates/benilla-ui/src/script/action.rs` (stock: `ActionBarFrame.xml`) | Computes and exposes `IsUsableAction`, `IsActionInRange`, `ActionHasRange`, `IsCurrentAction`, `IsAutoRepeatAction`, `IsAttackAction`, `IsEquippedAction`. |
| Bonus action bars (bottom-left/right) and layout | PARTIAL | `ui_script/multibar_stance_tests.rs` (`the_bonus_action_bar_exists_hidden_and_takes_layout_calls`) | `crates/benilla-ui/src/script/action.rs` (stock: `ActionBarFrame.xml`) | Keeps the bonus bars present but hidden, applies bar toggles and layout calls from `MultiBarBottomLeft/Right`. |
| Stance bar (shapeshift forms) | PARTIAL | `ui_script/multibar_stance_tests.rs` | `crates/benilla-ui/src/script/shapeshift.rs` (stock: `ActionBarFrame.xml`) | Exposes `GetNumShapeshiftForms`, `GetShapeshiftFormInfo`, cooldown and `CastShapeshiftForm` for the stance buttons. |
| Pet action bar | PARTIAL | `ui_script/pet_bar_tests.rs` (`shift_clicking_a_pet_button_picks_it_up_rather_than_casting`); `ui_pet/net.rs` tests | `crates/benilla-ui/src/script/pet.rs` (stock: `PetActionBarFrame.xml`) | Feeds pet actions, autocast flags, cooldowns and drag/pickup for `PetActionButton`. |
| Possession bar (mind control/charm) | BROKEN | Issue 18 Table F: `GetPossessInfo` returns three nils | `crates/benilla-ui/src/script/wrath_natives.rs` (stock: `PossessBarFrame.xml`) | No possessed unit is modelled; `GetPossessInfo` answers nils and `IsPossessBarVisible` stays false. |
| Multi-cast/totem bar | BROKEN | Source: `GetMultiCastBarOffset` returns 0; `GetTotemInfo` empty | `crates/benilla-ui/src/script/action.rs` / `wrath_natives.rs` (stock: `MultiCastActionBarFrame.xml`) | No totem bar state is fed; multi-cast stays on the default page. |
| Player spellbook (tabs, slots, highest rank, cooldown) | PARTIAL | `ui_script/spellbook_tests.rs` (`shipped_spellbook_drives_end_to_end`, `shipped_spellbook_shows_the_cooldown_pie`); `wrath_ui_slice_tests.rs` (`stock_wrath_spellbook_maps_displayed_ranks_to_known_slots`) | `crates/benilla-app/src/ui_spellbook.rs` + `crates/benilla-ui/src/script/spellbook.rs` (stock: `SpellBookFrame.xml`) | Builds tabs from `SkillLine.dbc`, sorts spells, feeds cooldown triples and fires `SPELLS_CHANGED`/`LEARNED_SPELL_IN_TAB`. |
| Pet spellbook | PARTIAL | `ui_pet/net.rs` tests (`pet_guids_and_learned_removed_spells_reach_the_pet_bar`) | `crates/benilla-ui/src/script/spellbook.rs` (`PetBookState`) (stock: `SpellBookFrame.xml`) | Maintains a separate pet slot list with packed words, autocast bits and pet-specific casts. |
| Spell targeting cursor | PARTIAL | `spellbook.rs` tests (`spell_is_targeting_answers_one_or_nil`) | `crates/benilla-ui/src/script/spellbook.rs` (stock: `SpellBookFrame.xml`) | Tracks `spell_targeting`, supports `SpellIsTargeting`, `SpellStopTargeting` and ESC-chain integration. |
| Spell tooltips (incl. talent refs) | PARTIAL | `benilla-ui/src/script/tests/tooltip_spell.rs` (`spellbook_hover_renders_the_verified_shape`) | `crates/benilla-ui/src/script/tooltip_spell.rs` (stock: `GameTooltip` spell template) | Renders spell/aura hover text, reagent and requirement lines, and talent-driven spell changes. |
| Casting bar (cast/channel) | PARTIAL | `ui_script/cast_tests.rs` (`cast_fills_then_completes_green_and_fades`, `the_casting_bar_publishes_its_status_bar_alias`) | `crates/benilla-app/src/ui_cast.rs` + `crates/benilla-ui/src/script/wrath_spell.rs` (stock: `CastingBarFrame.xml`) | Publishes cast/channel status, timers, failure state and status-bar alias for `CastingBarFrame.lua`. |
| Talent frame (tabs, points, learn) | PARTIAL | `ui_talent.rs` tests (`talents_info_updates_the_talent_window`); `talent_frame.rs` (`an_unavailable_talent_reaches_the_renderer_desaturated`) | `crates/benilla-app/src/ui_talent.rs` + `crates/benilla-ui/src/script/talent.rs` (stock: `Blizzard_TalentUI`) | Feeds talent tabs, ranks, prereqs, point counts and learn/preview calls. |
| Dual spec and talent groups | PARTIAL | `wrath_natives.rs` tests (`wrath_dual_spec_talents_and_glyph_sockets_read_groups`) | `crates/benilla-ui/src/script/talent.rs` + `ui_talent.rs` (stock: `Blizzard_TalentUI`) | Exposes `GetNumTalentGroups`, `GetActiveTalentGroup`, `SetActiveTalentGroup`, preview points and group state. |
| Pet talents | PARTIAL | `ui_talent.rs` tests (`pet_talent_tab`) | `crates/benilla-app/src/ui_talent.rs` (stock: `Blizzard_TalentUI`) | Builds pet talent views and a pet talent tab from `SMSG_PET_TALENTS_INFO`. |
| Talent wipe (respec) | PARTIAL | `ui_talent_call_tests.rs` (`confirm_talent_wipe_answers_inside_its_call`) | `crates/benilla-app/src/ui_talent_wipe.rs` (stock: `Blizzard_TalentUI`) | Feeds wipe cost/confirm state and sends `CMSG_TALENT_WIPE_CONFIRM`. |
| Macro frame/editor (create/edit/delete/icons) | PARTIAL | `ui_script/macro_tests.rs` (`new_then_pick_an_icon_then_okay_creates_the_macro`, `delete_removes_the_macro_and_re_selects`) | `crates/benilla-ui/src/script/macros.rs` + `crates/benilla-app/src/ui_macro/mod.rs` (stock: `Blizzard_MacroUI`) | Stores 18 account + 18 character macros, icon picker, edit/delete and dirty save. |
| Macro execution (run macro lines) | PARTIAL | `ui_macro::tests` (`running_a_macro_runs_its_lines_through_the_references_edit_box`) | `crates/benilla-app/src/ui_macro/mod.rs` (stock: `Blizzard_MacroUI`) | Runs macro body lines through the script VM and tracks running macro/button. |
| Key binding UI | PARTIAL | `ui_script/keybindings_tests.rs` (`the_action_bar_abbreviation_is_the_refs_own_getbindingtext`) + `keybind.rs` tests | `crates/benilla-ui/src/script/keybind.rs` + `crates/benilla-app/src/bindings.rs` (stock: `Blizzard_BindingUI`) | Registers binding sets, commands, modified clicks and exposes `GetBindingKey`/`GetBindingAction`/`GetCurrentBindingSet`. |
| Spell/item/action cooldown display (Cooldown widget) | PARTIAL | `ui_script/spellbook_tests.rs` (`shipped_spellbook_shows_the_cooldown_pie`) | `crates/benilla-ui/src/script/cooldown.rs` (stock: `Cooldown.xml`) | Implements the Wrath `Cooldown` widget (`SetCooldown`, reverse, hide/show) used by action/spell/item buttons. |

## Smallest missing rows

- Possession bar: implement possessed-unit/charm state and `GetPossessInfo` instead of three nils (`crates/benilla-ui/src/script/wrath_natives.rs`).
- Multi-cast totem bar: feed totem slots and a non-zero `GetMultiCastBarOffset` (`crates/benilla-ui/src/script/action.rs`, `wrath_natives.rs`).
- Pet spellbook/tooltips parity: ensure pet-book hover and autocast state are receiver-checked against the stock pet book (`crates/benilla-ui/src/script/spellbook.rs`, `tooltip_spell.rs`).

### Quest log, world map, quest markers, gossip

| Stock feature | Status | Evidence | File or "missing" | What our host/consumer does |
|---|---|---|---|---|
| Quest log list frame (QuestLogFrame.xml/.lua): titles, levels, tags, headers, collapse, selection | PARTIAL | SOURCE-OBSERVED + `a_selected_quest_answers_its_own_detail_text_not_a_placeholder_or_another_row` | `crates/benilla-ui/src/script/quest_log.rs` (`GetQuestLogTitle`, `GetQuestLogSelection`); `crates/benilla-app/src/ui_quest_log.rs`; stock `QuestLogFrame.xml/.lua` | Feeds 25 `PLAYER_QUEST_LOG` slots plus cached records, rebuilds display rows, folds headers, fires `QUEST_LOG_UPDATE` |
| Quest detail/reward panel: description, objectives, money, XP, rewards/choices, spell, honor, talents, arena, title, reputation | PARTIAL | SOURCE-OBSERVED + `a_reward_list_answers_each_reward_once_in_index_order` | `crates/benilla-ui/src/script/quest_log.rs`; `crates/benilla-app/src/ui_quest_log.rs`; stock `QuestLogFrame.xml` detail scroll | Detail getters read the selected row's record; reward/choice lists use item templates; money and XP formulas implemented |
| Quest accept / quest-giver query and turn-in flow | PARTIAL | SOURCE-OBSERVED | `crates/benilla-app/src/ui_gossip/verbs.rs`; stock `GossipFrame.lua` and `QuestFrame` | Sends `CMSG_QUESTGIVER_QUERY` or `COMPLETE` based on the row icon's one-click flag; gossip rows split active vs available |
| Quest abandon and party share (push/accept confirm) | PARTIAL | SOURCE-OBSERVED + `every_verdict_key_resolves_and_names_a_member` | `crates/benilla-app/src/ui_quest_share.rs`; `crates/benilla-ui/src/script/quest_log.rs` abandon list; stock `QuestLogFrame.lua` | Queues `MSG_QUEST_PUSH_RESULT` verdicts, resolves member names, fires `QUEST_ACCEPT_CONFIRM`; abandon mark holds the selected quest id |
| Quest tracker/watch (WatchFrame): add/remove, completion text, special item | PARTIAL | SOURCE-OBSERVED + `the_wrath_watch_list_adds_removes_and_holds_twenty_five` and `a_watch_special_item_answers_the_held_starting_item_only` | `crates/benilla-ui/src/script/quest_log.rs` (`AddQuestWatch`, `GetQuestIndexForWatch`, `GetQuestLogSpecialItemInfo`); stock `WatchFrame.lua` | 25-entry Wrath watch list, dedupes repeated arms, special item link/icon/charges only while held |
| Quest markers over creatures (gold !, ?, grey/blue variants, flight master green !) | PARTIAL | SOURCE-OBSERVED + `the_status_is_re_asked_on_a_ding_on_re_entry_and_dropped_with_the_flag` | `crates/benilla-app/src/quest_markers/mod.rs` and `query.rs`; stock `Interface\Buttons\TalkToMe*.m2` | Queries dialog status per questgiver, renders marker M2 under the unit overhead slot, trivial gating via minimap tracking |
| Quest markers over GameObjects | BROKEN | SOURCE-OBSERVED (`a_gameobject_is_asked_on_a_sweep_and_never_at_first_sight`) | `crates/benilla-app/src/quest_markers/query.rs` (missing render path) | GameObjects are queried on a sweep, but the answer handler resolves the guid with typemask 8 and drops the status; no marker is rendered |
| Quest log objective names for GameObject objectives | BROKEN | SOURCE-OBSERVED ("Not built") | `crates/benilla-ui/src/script/quest_log.rs` (missing) | A GameObject objective name wears the placeholder `"..."`; no name resolution is implemented |
| World map frame and navigation (WorldMapFrame) | PARTIAL | SOURCE-OBSERVED + `the_engine_selects_the_players_zone_on_first_world_enter` | `crates/benilla-ui/src/script/worldmap.rs`; `crates/benilla-app/src/ui_world_map.rs`; stock `WorldMapFrame.xml/.lua` | Catalog, selection, overlays, landmarks, player/party/raid/corpse positions, clicks, instance maps |
| World map explored overlays and zone hover/click | PARTIAL | SOURCE-OBSERVED + `the_zone_level_icon_gate_drops_rows_without_flags_0x80` | same files; stock `WorldMapFrame.lua` | Revealed overlays by explore bits, hit rects, hover names, continent/zone drill-down |
| World map POI/landmarks (AreaPOI.dbc plus SMSG_GOSSIP_POI guard marker) | PARTIAL | SOURCE-OBSERVED + `gossip_poi_wire_and_event` | `crates/benilla-ui/src/script/worldmap.rs` (`GetMapLandmarkInfo`); `crates/benilla-app/src/ui_world_map.rs`; stock `WorldMapFrame.lua` | Landmark sources projected onto the displayed map; gossip directions marker is ungated |
| World map quest objective display (QuestMapUpdateAllQuests) | BROKEN | SOURCE-OBSERVED (no-op returns 0) | `crates/benilla-ui/src/script/worldmap.rs` (missing quest POI feed) | `QuestMapUpdateAllQuests` is a no-op; no quest objective feed is pushed to the map |
| Minimap and zoom/tracking (MinimapFrame) | PARTIAL | SOURCE-OBSERVED + `minimap_zoom_api_and_extract` | `crates/benilla-ui/src/script/minimap.rs`; `crates/benilla-app/src/minimap/mod.rs`; stock `Minimap.xml` | Zoom levels, engine children, tracking cvar `minimapTrackedInfo`, trivial-quest gating |
| Minimap quest/POI blips and tracking menu | PARTIAL | SOURCE-OBSERVED | `crates/benilla-app/src/minimap/blips/mod.rs`; `crates/benilla-ui/src/script/minimap_tracking.rs`; stock `Minimap.xml` | Blips for units and POIs; tracking menu low-level quests gate marker display |
| Gossip frame (GossipFrame): text, options, quest rows, close | PARTIAL | SOURCE-OBSERVED + `gossip_snapshot_reads_and_selects_queue` | `crates/benilla-ui/src/script/gossip.rs`; `crates/benilla-app/src/ui_gossip/mod.rs`; stock `GossipFrame.xml/.lua` | Menu pushed only after greeting; flat text/type pairs; Wrath 5/4-field strides; reset and close |
| Gossip quest rows (available/active) and one-click turn-in | PARTIAL | SOURCE-OBSERVED + `the_two_gossip_quest_lists_split_by_active_and_select_within_themselves` | `crates/benilla-app/src/ui_gossip/verbs.rs`; stock `GossipFrame.lua` | Rows split by active flag; selects query vs. complete based on the one-click icon |
| NPC text/greeting draw (SMSG_NPC_TEXT_UPDATE) | PARTIAL | SOURCE-OBSERVED + `npc_text_update_greeting_extraction` | `crates/benilla-app/src/ui_gossip/mod.rs`; `crates/benilla-protocol/tests/gossip_vendor.rs`; stock `GossipFrame.lua` | Weighted greeting pick by gender; menu opens only once the text has arrived |
| Gossip POI directions marker on world map | PARTIAL | SOURCE-OBSERVED + `gossip_poi_wire_and_event` | `crates/benilla-protocol` plus `crates/benilla-ui/src/script/worldmap.rs`; stock `WorldMapFrame.lua` | `SMSG_GOSSIP_POI` decoded and projected as a landmark on the map |
| Quest log Wrath watch helpers (`GetQuestWatchIndex`, `GetQuestSortIndex`, `ShiftQuestWatches`, `SortQuestWatches`, `GetQuestLogCompletionText`, `GetQuestLogSpecialItemInfo`) | PARTIAL | SOURCE-OBSERVED + `the_wrath_watch_helpers_answer_real_records_or_nothing` | `crates/benilla-ui/src/script/quest_log.rs`; stock `WatchFrame.lua` | Installed only on the Wrath target so the tracker's objective list does not abort |

## Smallest missing rows

- GameObject quest markers are never rendered: `crates/benilla-app/src/quest_markers/query.rs` queries them on a sweep but the answer dies at typemask 8, so the marker render path is missing.
- World map quest objective display is a no-op: `QuestMapUpdateAllQuests` returns 0 in `crates/benilla-ui/src/script/worldmap.rs` and no quest POI feed is pushed.
- GameObject objective names are not built: `crates/benilla-ui/src/script/quest_log.rs` documents them as not built and uses the `"..."` placeholder.

### Chat, combat log, unit frames, social and guild

| Stock feature | Status | Evidence | File or "missing" | What our host/consumer does |
|---|---|---|---|---|
| ChatFrame message rendering (say/yell/system/loot/emote) | WORKING | `ui_chat/tests.rs::a_say_line_fires_chat_msg_say_in_the_references_arg_positions` | `crates/benilla-app/src/ui_chat/net.rs` (`chat`); stock `Interface/FrameXML/ChatFrame.lua` | `SMSG_MESSAGECHAT` is parsed into `ChatMessage`, filtered for ignored speakers/addon traffic, and pushed to `ChatLog` for the stock frame |
| Chat edit box send path (say/yell/emote/channel/whisper) | WORKING | SOURCE-OBSERVED | `crates/benilla-ui/src/script/chat_send.rs` + `crates/benilla-app/src/ui_chat/input/*`; stock `ChatFrame.lua` edit box | Slash parsing, AFK/DND handling, language and target tokens, then `CMSG_CHAT_MESSAGE`/channel sends |
| Channel join/leave/list and numbered slots | WORKING | `ui_chat/channels.rs::an_ordinary_zone_joins_general_and_local_defense` | `crates/benilla-app/src/ui_chat/channels.rs`, `slots.rs`; stock `ChatFrame.lua` | Zone-driven auto-join, custom channel verbs, server `ChannelNotify` confirmation and slot reuse |
| Whispers and offline targets | WORKING | `ui_chat/tests.rs::a_received_whisper_triggers_replying_to_tells` | `crates/benilla-app/src/ui_chat/net.rs` (`on_whisper_refusal`); stock `ChatFrame.lua` | Whisper refusal packets print the not-found/wrong-faction lines; ignored senders get `CMSG_CHAT_IGNORED` |
| Chat filters, languages and addon lane | WORKING | `ui_chat/net.rs::addon_chat_never_reaches_the_chat_window` | `crates/benilla-app/src/ui_chat/net.rs` (`chat`), `language.rs`; stock `ChatFrame.lua` | Addon traffic fires `CHAT_MSG_ADDON` without rendering; language headers and GM mode are applied |
| Combat log frame (COMBAT_LOG_EVENT_UNFILTERED) | PARTIAL | `wrath_combat_log_feed.rs::party_kill_event_has_the_eight_wrath_combat_log_values` | `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`; stock `Blizzard_CombatLog/Blizzard_CombatLog.lua` | Only `PARTY_KILL` is emitted, with endpoint names/guids/flags after name resolution |
| Combat log filter and history API | PARTIAL | `benilla-ui/src/script/combat_log.rs::apply_filters_sequence_stores_complete_masks_and_guids` | `crates/benilla-ui/src/script/combat_log.rs`; stock `Blizzard_CombatLog.lua` | Filter rows and retention are real state, but `CombatLogGetNumEntries` is always 0 and no entry ring/matcher exists |
| Combat log retention (combatLogRetentionTime) | WORKING | `combat_log.rs::retention_round_trips_from_the_stock_default` | `crates/benilla-ui/src/script/combat_log.rs`; stock `Blizzard_CombatLog.lua` | Getter/setter follow the registered CVar or the 300s fallback, including read-only refusal |
| Player frame (health/power/level/class/status) | WORKING | SOURCE-OBSERVED | `crates/benilla-ui/src/script/unit.rs`, `wrath_unit.rs`, `crates/benilla-app/src/ui_unit/*`; stock `Interface/FrameXML/PlayerFrame.lua` | Unit tokens, GUIDs, health/power and power type come from the model and Wrath host getters |
| Player/target auras (UnitAura/UnitBuff/UnitDebuff) | PARTIAL | SOURCE-OBSERVED | `crates/benilla-ui/src/script/wrath_aura.rs`, `aura/mod.rs`; stock `PlayerFrame.lua`/`TargetFrame.lua` | Aura list, counts, durations and filters work; `RAID` aura filtering is explicitly unsupported |
| Target and focus frames | WORKING | `ui_unit/field_events.rs::a_new_targets_slots_are_pushed_before_its_target_event` | `crates/benilla-ui/src/script/target.rs`, `unit/*`; stock `Interface/FrameXML/TargetFrame.lua`, `FocusFrame.lua` | Target/focus tokens resolve through the unit resolver and fire target/aura/health/power events |
| Party and raid frames | WORKING | `ui_party/feed.rs::the_raid_roster_maps_the_wire_to_get_raid_roster_info` | `crates/benilla-app/src/ui_party/*`, `crates/benilla-ui/src/script/party.rs`; stock `PartyMemberFrame.lua` | Party/raid rosters, subgroups, leader/loot, pets and ready checks are projected into stock frames |
| Unit frame update events (UNIT_*) | WORKING | `ui_unit/field_events.rs::each_id_fires_the_event_the_table_names` | `crates/benilla-app/src/ui_unit/field_events.rs`; stock FrameXML | Health, power, aura, target and status changes fire the stock `UNIT_*` events |
| Friends list | WORKING | `ui_social/call_tests.rs::the_list_signals_once_its_last_name_arrives` | `crates/benilla-app/src/ui_social/*`; stock `Interface/FrameXML/FriendsFrame.lua` | 50 friend slots, presence, notes and selection are maintained and signalled |
| Ignore list | WORKING | `ui_social/call_tests.rs::the_ignore_list_signals_once_its_last_name_arrives` | `crates/benilla-app/src/ui_social/*`; stock `FriendsFrame.lua` | 25 ignore slots, add/remove results and name resolution |
| /who query and results | WORKING | `ui_social/who_tests.rs::send_who_sends_its_query_inside_the_call` | `crates/benilla-app/src/ui_social/whois.rs`, `query.rs`; stock `WhoFrame.lua`/FriendsFrame | Query parsing, 50-row answers, chat vs frame presentation and count line |
| Guild roster and ranks | WORKING | `ui_guild/feed.rs::an_offline_row_keeps_its_level_class_and_zone` | `crates/benilla-app/src/ui_guild/*`; stock `Interface/FrameXML/GuildFrame.lua` | Roster, ranks, notes, last-online and identity cache are projected to stock frames |
| Guild events and commands | WORKING | `ui_guild/call_tests.rs::the_guild_verbs_send_inside_their_calls` | `crates/benilla-app/src/ui_guild/*`; stock `GuildFrame.lua` | Invite, promote, demote, remove, note and MOTD verbs send and roster updates signal |

Smallest missing rows:
- Combat log raw entry ring and matcher: `crates/benilla-ui/src/script/combat_log.rs` keeps filters and retention, but `CombatLogGetNumEntries` is hard-coded to 0 and `CombatLogGetCurrentEntry` returns no values; stock `Blizzard_CombatLog.lua` history walk cannot work.
- Combat log event coverage: `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs` emits only `COMBAT_LOG_EVENT_UNFILTERED` for `PARTY_KILL`; no other combat events reach the stock combat log frame.
- Aura raid filtering: `crates/benilla-ui/src/script/wrath_aura.rs` rejects the `RAID` filter with "requires the native raid policy"; raid-frame aura buttons using that filter are incomplete.

### Overseer selection: the three smallest missing rows

1. **GameObject objective names.** A GameObject objective's leaderboard line wears `...` in
   `crates/benilla-ui/src/script/quest_log.rs`; the app already owns the template name. Ticket
   `oc/quest-go-name`.
2. **Combat-log swing damage/miss.** `wrath_combat_log_feed.rs` emits only `PARTY_KILL`; feed the
   decoded `SMSG_ATTACKERSTATEUPDATE` as `SWING_DAMAGE`/`SWING_MISSED`. Ticket
   `oc/wrath-combat-log-swing`; this is also the issue 25 combat-log gap.
3. **Trade/mail/auction app-level UI coverage.** The inbound seams and verb tests exist, but the
   three stock frames have no app-level `ui_script` case like bags/bank/merchant. Ticket
   `oc/economy-ui-coverage`.

Not ticketed tonight: the bank 28-slot/7-bag proof needs an owned character with items in those
slots (`Blocked: needs owner run`); the world-map quest POI feed, the combat-log entry ring and
`CombatLogGetNumEntries` are real but larger than one sitting and are named under "Next" in the
area notes; the GameObject quest-marker `BROKEN` row is not a gap, because the 12340 answer
handler at `0x5dca2f` drops a GameObject's questgiver status, so neither client renders a marker
over one.

