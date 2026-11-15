# Completion matrix — 2026-11-02 (support review, no production ownership)

Assignment: `.scratch/project-b/handoffs/2026-11-02-deepseek-support.md`. Read-only inspection;
no Cargo, test, client, commit, push or production edit. HEAD `ab89c6fc` with a large dirty tree
(`git status --short`); other owners' work preserved.

How to read the columns. **Source** = the mechanism exists in the inspected tree (a file:line is
cited, not a promise). **Build** = a retained build log includes that source. **Runtime** = a
launcher log from a stated binary shows the protocol/consumer path. **Visible** = an owner-visible
run demonstrated the screen/action. They are not interchangeable; a missing error proves nothing.

## Baselines (revalidate before reuse)

- Current executable: `target/debug/benilla.exe`, SHA256
  `15CC91ABD9EF5918D877201BA0F783F87EB7940FEDB217C8E4DAD909B9FD611B`, mtime 2026-11-02 20:36:57.
  Newest run: `benilla-config/Logs/Launcher/20261102-210022-913/` (`launch.txt`: pid 15652, same
  hash). **No retained build log records 15CC91.** It postdates
  `verification/2026-10-28-oc-alpha/build-20261102-format-types.log` (finished 18:36; the 18:38
  launch `20261102-183811-186` records `C4ACD3B2...`) and predates the `wrath.rs` pose edit
  (mtime 21:29). So the newest run does **not** include the current `wrath.rs`; current dirty
  source ≠ tested binary.
- Last fully logged batch build: `.scratch/project-b/build-2026-11-02-opencode-batch4.log`
  (`Finished dev`), executable `A140D665...` per `verification/2026-11-02-opencode-batch.md`.
  Runtime `20261102-173105-388` used `A140D665...` but quit at the login screen.
- Best long session: `20261102-183811-186` (`C4ACD3B2...`, 18:38–19:11 local), which includes
  owner-driven world entry, target selection, and a real logout to character select.
- Newest play session: `20261102-210022-913` (`15CC91...`, 21:00–21:02): world entry as Firna,
  a kill, two vendor sales, logout with `quitting=true`.
- Visible play evidence (older binary, `verification/2026-11-01-astra-runtime.md`): login screen,
  Firna in Northshire with action bar/minimap, McBride greeting with a dead quest row, logout
  auto-entering Dicetar. It does not validate the current binary.

## Matrix

| Feature | Source | Build | Runtime | Visible | Verdict |
|---|---|---|---|---|---|
| Login screen / auth | `crates/benilla-app/src/glue_session/`, `net/session.rs`, `ui_script/lifecycle.rs` | A140, C4ACD, 15CC91 (all launched glue) | 210022: `stock glue up ... on "login" (0 load failure(s))`; account connect at 18:00:45 | yes, older binary (login screen, no modal) | Working at glue level; credentials owner-entered. |
| Realm list / select | `ui_script/glue_mgr.rs`, `crates/benilla-ui/src/script/glue_select.rs` (dirty realm-split request), `glue_session/realm.rs` | A140 batch log; C4ACD log | 210022: 13 realm types + Region 1 categories, `glue: the connect dials "AzerothCore"`; 183811 same | not cited: the 2026-11-01 observation starts at the login screen and next shows Firna in world; the realm/select UI and its controls were not observed | Working; Wrath realm-split request/event plumbing is source+unit-tested (`realm_split_notice_signs_its_two_integer_fields` PASS). |
| Character roster | `net/session.rs`, `ui_script/glue_mgr.rs`, `portrait/glue_booth.rs` | same binaries | 210022: `character select — 7 character(s)` twice, human portrait scenes; 183811 same | partial owner observation (2026-11-01): Firna appeared in world, so a character was selected; the roster control and **switching were not observed** | Roster data + portrait render work; selection-switch acceptance open. |
| Character create/cancel | `benilla-ui/src/script/glue_core.rs:196` (`CreateCharacter` → `GlueCreateRequest`), `glue_create.rs`, `net.rs` `CharRequest::Create` | source compiles in A140/C4ACD/15CC91 | 210022/183811 load `CharacterCreate.xml` (100 frames) with missing textures/inherited nodes; **no create/cancel interaction in any retained log** | no | Source path exists; UI visibly degraded; never exercised. |
| Enter world / loading | `wrath.rs` (`entered world`), `net/session.rs`, `loading_screen.rs` | A140/C4ACD/15CC91 | 210022: `wrath: entered world 0, player 0x1522, 76 entities`; loading up/cleared (5125/5125); `took control of player @ Vec3(133,80,8914)`; in-game UI up 1641 ms; 183811 same shape | yes, older binary: Firna in Northshire, cover cleared, action bar/minimap | Working runtime path; re-confirm visually on 15CC91. |
| Logout → character select | `ui_logout`, `ui_script/lifecycle.rs` teardown, `wrath.rs:472 for_login_mode`, `:905` interactive gate | source in C4ACD/15CC91; guard is source-only | **183811 (C4ACD)**: `logout: requested (quitting=false)`, `wrath session ended cleanly`, `net: logged out — back to character select`, new glue VM on `"charselect"`, stayed there. **210022 (15CC91)**: `quitting=true` and `world session ended — exiting` | 2026-11-01 older binary instead auto-entered Dicetar; guard added since | Fixed on C4ACD evidence; **not re-proven on 15CC91**, and current `wrath.rs` is newer than 15CC91. |
| Quest dialogue / greeting | `ui_gossip/{verbs,net}.rs`, `ui_quest/net.rs:249`, FrameXML `GossipFrame.xml` loaded | A140/C4ACD/15CC91 | 210022: `GossipFrame.xml loaded (43 frames)`; quest markers at 18:00:52; **no greeting/row click in 210022 or 183811** | owner-observed 2026-11-01: McBride greeting opened, row highlighted, no detail; nothing on current binaries | Dialogue shell loads; interaction unproven. |
| Quest row → detail | `ui_quest/verbs.rs:84` (dirty debug instrumentation), `protocol-wrath/gameplay.rs:2817 read_quest_details` (exact-length `bail` at :2851), `ui_quest/net.rs:259` | source in A140+ | no `net: quest detail` line in any retained log; no `left N bytes` error either | owner-observed 2026-11-01: row highlighted, greeting stayed, no detail | Not demonstrated in retained runs; the only owner observation is a failed row click. |
| Quest accept | `ui_quest/verbs.rs:155` (`QuestgiverAccept`), `protocol-wrath/gameplay.rs:167`; `verb_tests.rs` wire-order tests | source in A140+ | no accept in any retained log | no | Source path + focused tests; receiver unproven. |
| Quest tracker | `crates/benilla-ui/src/script/worldmap.rs:797-800` (`QuestMapUpdateAllQuests` is a no-op returning no values), `crates/benilla-ui/src/script/quest_log.rs` (`GetNumQuestWatches` + tests), Wrath `WatchFrame.lua` consumer | all three binaries | **210022: 94× `WatchFrame.lua:1306: 'for' limit must be a number`**; 183811: 94× same. Cause identified by Astra from `enus/patch-enus-3.mpq` WatchFrame.lua: `QuestMapUpdateAllQuests()`'s return is used as the loop limit, but the binding returns nothing; the reference computes the visible POI list | tracker not seen working on current binaries | Broken contract, not an empty state: implement the real visible-POI count/list. Returning a fake zero is not completion. |
| Quest reward / turn-in | `ui_quest/verbs.rs:233` (`QuestgiverChooseReward`), `net.rs:293`, `gameplay.rs:186` | source in A140+ | none | no | Source only. |
| Combat (auto-attack / kill) | `target/`, `spell/`, `ui_action`, `net/` | all | 210022: live + dead target ring entries (`rank 3 [dead gray]`), `You receive a bounty of 3 copper for the kill`, repeated movement poses; 183811: target ring rank 4 | owner drove combat in 2026-11-01; 210022 shows the result | Kill path works; mechanics detail not evidenced beyond the kill line. |
| Casting / casting bar | `spell/`, `ui_action` (49839 spells, 70 cast times), `CastingBarFrame.xml` loaded | all | no cast request/completion line in retained logs | not observed on current binaries | Source + data load; runtime cast unproven. |
| Target selection / ring | `target/ring.rs`, `target/hover.rs`, `target/relations.rs` | all | 210022: multiple `target: guid ... ftpl ... rank 3` including `[dead gray]`; 183811 rank 4 | not cited: runtime ring only; no screenshot/owner statement in the retained logs | Working runtime path; visible confirmation pending. |
| Loot pickup / loot frame | `ui_loot/{mod,net,calls}.rs` (large, with `receive_tests`, `call_tests`, `sweep_tests`), `ui_loot_roll`; `LootFrame.xml` loaded | all | no `You receive loot`/loot-open line in 210022/183811; only vendor-sale lines | no | Source + tests exist; loot interaction never demonstrated on these binaries. |
| Bags / containers | `ui_items`, `ui_container`/item verbs, bag buttons (`MainMenuBarBagButtons.xml` 12 frames) | all | no bag interaction in logs | no | Source only. |
| Merchant / vendor | `ui_merchant/{mod,net,calls}.rs`, `benilla-ui/src/script/merchant.rs` (7-tuple, extended cost, `RepairAllItems`; unit tests), `MerchantFrame.xml` (163 frames) | A140 log (merchant test reconciliation only; no behavior change) | 210022: `[Frayed Cloak] sold for 4 copper`, `[Ruined Pelt] sold for 5 copper`; 183811: `MerchantFrame.xml` loaded | not cited: the sale lines are server chat; no screenshot/owner observation of the vendor frame or the control used | Sell request accepted by the server; the visible frame/control is unverified. |
| Settings / CVars | `cvars/{table,inert,mod}.rs`, `ui/script/cvars.rs`, `video.rs`, `sound/mod.rs` | A140/C4ACD/15CC91 | 210022: glue + in-game `AudioOptionsPanels`/`VideoOptionsPanels` FrameXML load; `IsEnabled` tooltip errors gone vs 183811; **8 CVar misses remain** | not cited: FrameXML load only; no screenshot/owner observation of an options panel or a changed value | Load-level improvement; visible option behavior unverified; Wrath sound/filter backends remain source-only (see review artifact). |
| Keybindings / UI chrome | `bindings/`, `ui_action`, FrameXML | all | 210022: 71 slash aliases, action bar frames loaded; `CharacterFrame.lua:28` nil index, `GetCritChanceFromAgility` missing | action bar visible on older binary | Partially degraded PaperDoll/keys; not a route blocker by itself. |

## Highest-priority remaining gaps (max 12, ordered)

1. **Quest detail is not demonstrated in retained runs.** No retained run has clicked a
   questgiver row since the dirty instrumentation; absence is not proof that the path cannot
   work. Next: owner clicks McBride's `Kobold Camp Cleanup`; capture `ui_quest: select` debug +
   outbound `CMSG_QUESTGIVER_QUERY_QUEST` + reply/decode. Check `read_quest_details`'
   exact-length tail (`gameplay.rs:2851`) against the actual reply before changing the latch.
   (`ui_quest/verbs.rs`, `gameplay.rs`, run 210022.)
2. **Quest tracker UI is erroring 94× per session** (`WatchFrame.lua:1306`, both C4ACD and
   15CC91). Cause identified by Astra from selected `enus/patch-enus-3.mpq`: the Wrath
   WatchFrame uses `QuestMapUpdateAllQuests()`'s return as its loop limit, but
   `crates/benilla-ui/src/script/worldmap.rs:797-800` binds it as a no-op returning no values.
   The reference computes the visible POI list. Implement that real count/list; a fake zero is
   not completion. This blocks route visibility even if accept works.
3. **Quest accept and reward are source-only.** `verbs.rs:155/233` and wire encoders have focused
   tests, but no receiver evidence. Needs (1) first.
4. **Logout→roster is not re-proven on the current binary.** C4ACD evidence is good; 15CC91 ran
   with `quitting=true`. Re-run the interactive logout on the next built binary and confirm the
   roster stays (and that the current `wrath.rs` pose/selection guard does not reseat/re-enter).
5. **Create/cancel and roster switching have never been exercised.** Create screen also logs
   missing inherited nodes/textures (`QuestFontNormalHuge`, `AlternateForm`, `CharacterCreate-Arrow`)
   and `CharCreateClassButton6`; this is visible-quality work, not just wiring.
6. **Current binary provenance is unrecorded.** 15CC91 has no build log and predates the 21:29
   `wrath.rs` edit; the next integration build must record command, log and hash before any
   acceptance claim.
7. **Missing Wrath natives that break visible frames on 15CC91:** `GetCritChanceFromAgility`
   (PaperDoll + `CharacterFrame.lua:28` nil index), `GetLFGDeserterExpiration` (LFDFrame, 10×),
   `GetAvailableRoles`, `GetPossessInfo`, `GetNumDungeonMapLevels`, `SetHyperlinksEnabled`
   (chat, 10×), `GetActiveVoiceChannel`/`GetVoiceSessionInfo`/`GetChannelDisplayInfo`,
   `GetArenaCurrency`, `BNGetNumBlockedToons`, `UnitIsTalking`. Prioritize the ones the route
   touches (PaperDoll/chat) over optional services.
8. **Eight stock CVars still missing in 15CC91:** `wholeChatWindowClickable`,
   `UnitNameFriendlyGuardianName`, `showVKeyCastbar`, `showArenaEnemyFrames`, `colorblindMode`,
   `consolidateBuffs`, `fctDodgeParryMiss`, `enableCombatText`. They are panel/OnLoad errors, not
   all route blockers; add real rows with the 12340 defaults and a consumer, per the existing
   `ours`/`inert` policy.
9. **Wrath audio backend gaps:** `Sound_EnableAllSound`/`Sound_EnableSFX`/`Sound_EnableEmoteSounds`
   rows have no consumer; the reference music handler ORs the master switch. Also the donor/Wrath
   spelling split can re-mute or mis-display a persisted setting (details in
   `2026-11-02-cvar-backend-review.md`).
10. **`textureFilteringMode` integration gaps:** out-of-range writes are accepted (reference
    refuses), and a persisted donor `trilinear`/`anisotropic` overrides the Wrath value at boot
    while the dropdown still reports the Wrath value. Source-only; no live visual check yet.
11. **Loot and bag interaction remain unproven.** `ui_loot` has real tests, but no retained run
    opened a corpse or moved an item; only vendor sales are evidenced. Route acceptance needs one
    owner-driven loot/bag pass without deleting items.
12. **DBC/data blockers still visible:** `TaxiPathNode` 9-vs-11 disables the taxi map (ERROR in
    210022) and `GroundEffectTexture` 7-vs-11 disables ground clutter. Neither blocks Northshire
    combat, but taxi is route-relevant later.

## Explicit non-claims

- No feature above is "complete" from absence of errors, a loaded FrameXML file, or a passing
  unit test. `15CC91` did not exercise quest, create/cancel, roster switch, loot or casting.
- No new runtime, build or test was run for this matrix. All statements are from source
  inspection and the logs cited above.
- Visible gameplay on the 2026-11-01 binary is historical evidence, not current acceptance.
