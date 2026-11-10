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
|WMQuestieCompat 0.1.0|`WMQuestieCompat.lua` 1,701 B `6edb2f3e7812161e`; `.toc` `0c167c0d7cc6a9b0`|`30300`; `OptionalDeps: Questie-335`|none|`ADDON_LOADED`/`PLAYER_LOGIN`, `Questie.Error`, `Questie.db.char.TrackedQuests`|UNKNOWN (D04)|
|Questie-335 9.5.1|`Questie-335.toc` `1ce807739228ce81`; `Questie.lua` `9645d91e7e965dfd`|Seven TOCs; `Questie-335.toc` is `30300`, `Questie-WOTLKC.toc` is `30403` and is not the 3.3.5 selection|`QuestieConfig` account; `QuestieConfigCharacter` per character|stock quest-log/map APIs plus its own `QuestieCompat.C_Map`/`C_Timer`/`C_QuestLog` shims|UNKNOWN (D04)|
|Bagnon 2.13.3|`Bagnon.toc` `835981e2b9ecab9a`; `main.lua` `c21f7a6b8be4e209`|`30300`; `SavedVariables: BagnonGlobalSettings`; `SavedVariablesPerCharacter: BagnonFrameSettings`|account and per character|`GetContainerItemInfo` 7 values, `GetContainerItemQuestInfo`, `GetItemInfo` 11 values, `GetItemQualityColor`, `GetContainerItemCooldown`|PARTIAL · focused test (D05)|
|Bartender4 4.4.2-2-g3b02ee4|`bartender4.toc` `a0b9187368da0c2a`|`30300`; `SavedVariables: Bartender4DB`|account (`Bartender4DB`)|`SecureHandlerStateTemplate`, `RegisterStateDriver`, `UnregisterStateDriver`, `WrapScript`, `GetActionInfo` 3 values, `GetShapeshiftFormInfo` 4 values, `GetPetActionInfo` 7 values|PARTIAL · focused test (D06)|

The Bartender4 directory also carries `changelog-bartender4-4.4.2-12-g94f3b58.txt`; that is a
different build's changelog, not the loaded TOC version. No version is inferred from file presence.

### B. Stock FrameXML/GlueXML surface these addons require

Return arity is the Wrath-mode contract; donor-mode divergences are called out. "Empty" means the
reference's zero-return or nil shape, not a fabricated success.

|API / widget method|Contract relied on|Host locator|Consumer|Status|
|---|---|---|---|---|
|`GetContainerItemInfo(bag, slot)`|texture, count, locked, quality, readable, lootable, link; empty/unknown = zero returns|`container.rs` 1244–1315|Bagnon `utility/itemSlotInfo.lua` 18; WMBridge stone|source matches|
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
|`GetQuestLogTitle(index)`|Wrath order title, level, tag, group, header, collapsed, complete, daily, questID; optional display-quest-id bool|`quest_log.rs` 1294–1408|Questie `QuestieCompat.GetQuestLogTitle` (Compat.lua 417–424)|source matches the compat wrapper|
|`GetQuestLogLeaderBoard(i, quest)`|text, type, finished; missing = three nils|`quest_log.rs` 1472–1494|Questie `Compat.lua` 380|source matches|
|`GetNumQuestLogEntries()` / reward/choice/count getters|two counts; detail counts/rows|`quest_log.rs` 1301, 1554–1710|Questie quest log cache|source matches|
|`GetBuildInfo()`|version, build, date, toc|`client.rs` 137; `glue_core.rs` 886|Questie `QuestieCompat.Is335`|source matches|
|`GetSpellInfo(spell)`|host exposes 9 values: name, rank, icon, cost, channeled, powerType, castMs, minRange, maxRange|`wrath_spell.rs` 112–125|WM_SpellmakingV3 277–278 assumes at least 9; stock 3.3.5 expects 9|source matches|
|`SendAddonMessage(prefix, message, distribution, target)`|WHISPER admitted and normalizes `WMBridge` fallback `transport=SELF_WHISPER`; CHANNEL intentionally refused; composed text capped at 255 bytes|`addon_message.rs` 90–140|WMBridge 135–159; WM_SpellmakingV3 36|source matches (255-byte server cap enforced)|
|`GetChannelName(nameOrIndex)`|id, name or nil, instance; requires confirmed slot on `Wrath335`|`channel.rs` 340–374|WMBridge `ensureChannel`|source matches|
|`CreateFrame(kind, name, parent, template)`|admits `SecureHandlerBaseTemplate`, `SecureHandlerClickTemplate`, `SecureHandlerStateTemplate`|`secure.rs` 54–78|Bartender4 secure bars|source matches (bounded non-executing)|
|`RegisterStateDriver` / `UnregisterStateDriver` / `WrapScript`|bounded non-executing secure state-driver globals and frame method|`secure.rs` 80–215|Bartender4 `bar.lua` 133/407/467, `bartender4.lua` 179–199, `actionbutton.lua` 88/97|source matches (bounded non-executing)|
|`hooksecurefunc`|post-hook a global or method|`posthook.rs` 29|WMBridge 465/476; WM_SpellmakingV3 290–305|source matches|
|`UIDropDownMenu_*`|stock FrameXML template and functions|shipped FrameXML, not a Rust native|WM_Multiclass 30–38|not runtime-verified|

GlueXML-specific dependencies checked: `multiclass_glue.lua` uses `GetCharacterInfo` and
`CharacterSelect.selectedIndex` (`glue_select.rs` 560/568); `QuestieCompat.Is335` uses
`GetBuildInfo`; both exist. `WM_Multiclass` gear/roster requests have no host state owner beyond the
broken D02 contract.

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
|Action slots 1..144|`SMSG_ACTION_BUTTONS` parse `gameplay.rs` 2172, decode 3591–3609; tap cap `wrath.rs` 5384|`spell/net.rs` 717–748 replaces the map on state 1|covered by `spell/net.rs` action button state tests|PARTIAL|
|Dual-spec state 0/1/2|12340 `Player_C.cpp` 4084–4124: state 0 snapshots into `ActionButtonsPending::held`, state 1 applies and clears `pending`, state 2 marks `pending` without clearing live slots|`spell/net.rs` 706–748 (`ActionButtonsPending`); `wrath.rs` 3717–3739|`spell/net.rs` state 0/1/2 unit tests|PARTIAL (source + unit-tested; no spec-switch live run)|
|Page switching|`CURRENT_ACTIONBAR_PAGE` FrameXML-local; `ChangeActionBarPage` fires event (`action.rs` 554–584)|no server packet|stock action-bar tests `ui_script/action_bar_tests.rs`|source matches|
|Bar visibility toggles|`SetActionBarToggles` sends `CMSG_SET_ACTIONBAR_TOGGLES`; getter reads server byte|`action_bar_toggles.rs` 44–86|`set_action_bar_toggles` tests|source matches|
|Per-character keybindings|account diff then `<Realm>-<Char>.txt`; character file presence activates set 2|`bindings.rs` 245–276; `bindings/store.rs` 36–96|`bindings::store::tests::the_diff_round_trips_and_defaults_stay_silent`; `...resolve_steals_across_commands...`|PARTIAL (no relog run)|
|Macros, account vs character|18 account + 18 character; character ids rebased by `0x1000000`|`macros.rs` 7–28, 152–164; `ui_macro/store.rs` 62|`ui_macro::tests` scope tests|PARTIAL (no relog run)|
|`ReloadUI` persistence|save layout, saved variables, addon variables and enable state before VM close|`lifecycle.rs` 1003–1018, 1163+; `addons.rs` 604–731|`ui_script::addons::tests::the_enable_state_round_trips_through_the_file`; `an_addons_saved_variables_survive_the_session`|PARTIAL (no reload run)|
|Addon enable/disable state|per-character `AddOns.txt` hash; realm agreement fallback; TOC `## DefaultState`|`addons.rs` 604–716|same enable-state test|PARTIAL (no relog run)|

### D. Wave 1/2 inbound seams: real consumer, donor projection, or no owner

|Seam|Parse / decode|App translation|Real app/UI consumer|Donor projection or loss|No state owner|Test or missing test|
|---|---|---|---|---|---|---|
|Pet spells/mode/feedback/sounds|`gameplay.rs` 2233–2241; `pet_talent.rs`|`wrath.rs` 4239–4295|`ui_pet/net.rs` 53/80/91/326–411 (`PetBar::pet_guids`, `pet_spell_learned`, `pet_spell_removed`)|stable list synthesizes slots and loyalty 0|none for `PetGuids`/`PetSpellLearned`/`PetSpellRemoved`|`pet_talent.rs::tests::pet_spells_reads_family_bar_spells_and_32_bit_cooldown`; `ui_pet/net.rs::tests::pet_guids_and_learned_removed_spells_reach_the_pet_bar`|
|Talent info/reset|`gameplay.rs` 2252–2254; `pet_talent.rs`|`wrath.rs` 4296–4331|`ui_talent.rs` (`WrathTalentState`, `on_talent_info`, `on_talents_involuntarily_reset`, `build_pages_with_wrath`)|active player spec and unspent points feed `TalentUiState`; pet talent tab and glyph socket UI not yet projected to Lua|pet talent tab UI, glyph UI|`pet_talent.rs::tests::player_talents_info_reads_every_field`; `ui_talent.rs::tests::talents_info_updates_the_talent_window`; missing `pet_talent_tab`|
|Stable|`gameplay.rs` 2248–2251|`wrath.rs` 4332–4367|`ui_stable/net.rs` 26/46|12340 list has no loyalty column, filled 0|none for list/result; stable swap and purchase display unverified|`pet_talent.rs::tests::stable_list_reads_current_and_stabled_rows`; `ui_stable/mod.rs::a_refused_stabled_pet_is_asked_at_the_next_list_never_by_a_frame`|
|Trade/mail/auction|`gameplay.rs` 2266–2299; `economy.rs`|`wrath.rs` 4378–4458|`ui_trade/net.rs` 58; `ui_mail/net.rs` 54/110/131; `ui_auction/net.rs` 448|mail keeps attachment 0 and inline body; trade drops `trade_id`/socket words|multi-attachment UI, socket UI|`economy.rs::tests::mail_list_reads_rows_bodies_and_attachments`; `trade_extended_carries_sockets_creator_signed_fields_and_empty_slots`; missing receiver tests|
|Party/social/guild|`gameplay.rs` 2300–2346; `social.rs`|`wrath.rs` 4459–4533|`ui_party/net.rs` 110; `ui_social/mod.rs` 728–831 (`on_whois`); `ui_guild/mod.rs` 707–754; `ui_chat/net.rs` 90|HP/pet HP and aura ids clamp to u16; LFG roles/difficulty dropped|none for `WhoisText` (`on_whois` writes to console)|`social.rs::tests::group_list_full_preserves_lfg_loot_and_member`; `party_member_stats_full_reads_every_defined_bit`|
|LFG / battleground|`gameplay.rs` 2347–2365; `wrath_systems.rs` 182–602|`wrath.rs` 4540–4546 (discard)|none|none|join/queue/proposal/role-check, BG queue/score|`wrath_systems.rs::tests::lfg_join_result_ok_has_no_party_tail`; `battlefield_status_queued_arm`; missing `lfg_join_sends_from_the_lfd_frame`|
|Calendar / rune / vehicle|`gameplay.rs` 2366–2374; `wrath_systems.rs` 860–1391|`wrath.rs` 624–665 (`WrathFeedEvent::RuneState`/`RuneConvert`/`RunePower`/`Calendar`/`CalendarCommandResult`)|`ui_script/wrath_rune_calendar_feed.rs`|vehicle has outbound encoders only|vehicle seats|`calendar_send_calendar_round_trips`; `resync_runes_reads_progress_bytes`; `seat_and_exit_encoders_match_their_bodies`|

`SessionEvent` producer additions in this workstream: `PetSpellLearned`/`PetSpellRemoved`/
`PetGuids`/`TalentInfo`/`TalentsInvoluntarilyReset` ([events.rs] 627–645), `WhoisText` (1149), and
the widened `AuctionHello { enabled }` (1225). `PetLearnedSpell` is a wrath-gameplay spelling mapped
to `PetSpellLearned` at `wrath.rs` 4290. Calendar and rune events travel `WrathFeedEvent` into
`WrathWorldView` and `ui_script/wrath_rune_calendar_feed.rs`.

### E. Decoded records with no state owner

|Decoded record|Decoder locator|What is missing|Named next test|
|---|---|---|---|
|LFG join result / queue status / player+party update / proposal|`wrath_systems.rs` 268–560|queue and proposal store, role-check owner, LFD frame consumer, `ClientCommand`/tap arm|`lfg_join_sends_from_the_lfd_frame`|
|Battleground status / PvP log|`wrath_systems.rs` 620–859|queue slots, scoreboard state, battlefield frame consumer|`battleground_status_reaches_the_scoreboard`|
|Vehicle seat/exit|`wrath_systems.rs` 1259–1391 (outbound only)|seat model, `CanExitVehicle`, seat/exit/next/prev natives and a control consumer|`vehicle_exit_uses_the_seat_transport`|
|Pet talent tab UI & glyph sockets|`pet_talent.rs` `read_talents_info`; `ui_talent.rs` `WrathTalentState`|`GetTalentInfo(..., isPet)` Lua projection and glyph frame bindings|`pet_talent_tab`|
|Mail multi-attachment|`economy.rs` mail reader; `wrath.rs` 4945|shared `MailListEntry` multi-attachment array (inline body is already mapped at `wrath.rs` 4974)|`mail_two_attachments_and_inline_body`|
|Trade socket enchant and 3.3.5 trade id|`economy.rs` trade reader; `wrath.rs` 4916|shared `TradeItem.socket_enchant` and `TradeStatusExtended.trade_id`, trade frame|`trade_socket_enchant_reaches_the_trade_frame`|

### F. `wrath_natives.rs`: real state versus stubs

Real, state-backed at source: `GetNumTitles`/`GetCurrentTitle`/`IsTitleKnown`/`GetTitleName`
(304–356), `GetCombatRating` (367–383), `IsPetAttackAction` (392–415), `UnitIsAFK`/`UnitIsDND`
(417–443), LFG aura predicates and cooldown expirations (918–936), `GetAvailableRoles` (941–961),
`GetModifiedClick`/`IsModifiedClick` (239–274), `GetExpansionLevel`/`GetAccountExpansionLevel`
(741–752).

Constant/stub or absent: `CanExitVehicle` (385–391, always nil), `VehicleExit` and aim handlers
(553–574, no-ops), `GetLFGQueuedList` (444–457, empty table), `GetNumBattlegroundTypes` and
`GetNumArenaOpponents` (458–467, 0), `GetWorldPVPQueueStatus` (589–602, "none"), `GetLFGProposal`,
`GetLFGInfoServer`, `GetLFGRoleUpdate`, `IsListedInLFR`, `IsPartyLFG` (604–641, idle shapes),
`GetPossessInfo`/`IsPossessBarVisible` (778–782, 882–903, no possession), `CalendarGetDate` and
`CalendarGetNumPendingInvites` (698–770, zeros), `GetRuneType` (806–818, fixed 1/1/2/2/3/3 layout),
`GetTotemInfo` (800–804, empty), `GetMirrorTimerInfo` (760–764, UNKNOWN), `GetHonorCurrency`
(820–824, 0). `GetRuneCooldown` has no binding anywhere in `crates/`; do not fabricate it from the
aggregate runic-power field.

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


