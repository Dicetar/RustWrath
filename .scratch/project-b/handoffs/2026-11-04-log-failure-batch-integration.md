# Log-failure batch integration (2026-11-04)

Batch base: `ba05951eb6566f6843c1077f717ec89f69a17522` (`origin/main` at handoff).
Supervisor: OpenCode session with AutoSwarm controller `main_cc478c196b3cbac499b1ea9bcbc9ead7`.
Evidence log: `D:/Projects/project-b-client/benilla-config/Logs/Launcher/20261104-180236-197/stderr.log`.
Interaction report: `D:/Projects/project-b-client/benilla-config/Logs/InteractionDiagnostics/interaction-1793805267387608500.jsonl`.

## Accepted commits

| Branch | Commits | Result |
|---|---|---|
| `fix/log-dbc` | `485c1eea`, `44bdc541` | 13 logged DBC schema names / 16 occurrences fixed; two follow-on loader failures found and fixed on review. |
| `fix/log-ui` | `221311a2`, `dd2df6b3` | Stock 3.3.5 UI verbs added; compile errors in the first commit corrected; LFG deserter now reads real aura state. |
| `fix/log-pet-model` | `f047f9a3` (revert of `6ff3e418`), `a704cec8` | Rejected field-table rewrite reverted; ownership GUID projection added instead. |

`6ff3e418` (12340 indices written into the 1.12 translated field table) is retained only as a
reverted commit. The final tree keeps the 1.12 descriptor constants and the Wrath translator.

## DBC result

- Schema counts were checked against the real 12340 data in
  `D:/WOW/Azerothcore_WoTLK_Repack/data/dbc` and TrinityCore 3.3.5 layouts:
  TaxiPathNode 11, CreatureSoundData 38, AreaTable 36, SoundEntries 30, Material 5,
  AnimationData 8, WMOAreaTable 28, GroundEffectTexture 11, GMSurveyQuestions 18,
  ChatProfanity/NamesProfanity/NamesReserved 3, CreatureDisplayInfo 16.
- Semantic offsets verified: AnimationData FallbackID 5; AreaTable FactionGroupMask 28;
  CreatureSoundData custom attack 19-22 / loop 24 / impact 25 / jump 26-27 / pet 28-30;
  GroundEffectTexture Density 9 / TerrainID 10; WMOAreaTable keeps the 1.12 prefix and
  16 locale offsets plus mask at 27.
- The first commit still left two logged loaders failing after their first fixed table:
  `creature_sound.rs` stopped at CreatureModelData (16 vs 28; fixed to 28, SoundID still 13),
  and `npc_greeting.rs` was the actual CreatureDisplayInfo 12-vs-16 loader (fixed to 16 and
  NPCSoundID moved 11 -> 12; verified display 793 = 50 and display 89 = 161 in column 12).
- AreaSound.dbc is absent from the 3.3.5 data set; the changed `area_schema` belongs to
  `AreaTable` in the zone-audio loader.

## UI result

Fixed and verified against the 12340 reference where a binding exists:

- `UnitDetailedThreatSituation` (0x613B40): defined; no value when no mob/status. The client has
  no threat ingress, so no five-value answer is invented.
- `QuestMapUpdateAllQuests` (0x5E63D0): returns the updated count (0 here), fixing
  `WatchFrame.lua:1305-1307`.
- `GetNumDungeonMapLevels` (0x546290) and `GetNumBattlefieldVehicles` (0x54A140): zero counts
  (no dungeon-floor or battlefield-vehicle state).
- `TriggerTutorial` (0x5308D0): queues a 0-based id on the existing app trigger feed; ids 60+
  are dropped, matching the reference.
- Chat: `SetHyperlinksEnabled` stores the flag with Lua truthiness; no hyperlink render consumer.
- Voice/possess: `GetActiveVoiceChannel`, `GetVoiceSessionInfo`, `GetChannelDisplayInfo` answer
  empty/nil; `GetPossessInfo` answers three nils. No voice or possess state exists.
- LFG: `GetAvailableRoles` matches the reference `s_classRoles` table
  `{0x0B,0x0F,0x09,0x09,0x0D,0x0B,0x0D,0x09,0x09,0x00,0x0F,0x0F}` and reads the live class;
  `GetLFGDeserterExpiration` (0x5580E0) reads player aura 71041 and returns its end time;
  backfill/random-dungeon/lock-info answers are false/zero because no LFG packets are sent.

Still open or unsupported (not repaired): `GetArenaCurrency`, `GetCritChanceFromAgility`,
`UnitIsTalking`, `BNGetNumBlockedToons`, minimap `SetTracking`/tracking projection,
`SetUIVisibility`, `CanShowAchievementUI`, UIParent achievement nil arithmetic, exhaustion state,
CharacterFrame nil field, `ChatConfigFrame.lua:752` function-to-string (not traced), the
OptionsPanelTemplates/NAMEPLATES CVar gaps, and the `<unnamed>:SetText(): Font not set` errors.
These are outside the assigned API set or have no local state owner.

## Pet and paladin result

- `a704cec8` adds `charmed_by`, `summoned_by` and `created_by` to `WrathEntity`, reads
  `UNIT_FIELD_CHARMEDBY` 12 / `SUMMONEDBY` 14 / `CREATEDBY` 16 from the 12340 reducer, and
  projects them onto the 1.12 slots 10/11, 12/13, 14/15. This reaches `target/relations.rs`
  (`owner`/`CanAttack`), `ui_unit`, `vplates`, minimap blips, combat log and pet UI, which
  previously saw no owner for a hunter pet. Source-level fix; live reaction acceptance unrun.
- Paladin player appearance remains open. The Wrath path already projects `display_id` (67 ->
  131/132), `bytes_0` (23 -> 36) and `player_bytes`/`player_bytes_2` (153/154 -> 193/194);
  the saved report does not distinguish a present/valid display id from an absent/zero one, nor
  display resolution from skin/appearance failure. No evidence-backed edit was made.

## Verification level

Source inspection and Git integration only. Not run: `cargo check`, `cargo build`, `cargo test`,
`cargo clippy`, `rustfmt`, any new test file, any client/server launch, or rendered acceptance.
Compilation, receiver behavior and visible gameplay remain unverified. The 1.12-data
`wow_data_or_skip` tests may now fail on the changed schemas if run against a 5875 install;
they were not run.
