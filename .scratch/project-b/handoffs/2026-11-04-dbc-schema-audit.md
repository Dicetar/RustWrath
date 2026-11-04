# 3.3.5 DBC schema audit (2026-11-04)

Source log: `D:/Projects/project-b-client/benilla-config/Logs/Launcher/20261104-180236-197/stderr.log`

Reference: `D:/Projects/wowdecomp-reference` pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.

## Handled (13 unique schema names, 16 log occurrences)

| DBC | Old count | File count | New count | Semantic offset fixes |
|---|---:|---:|---:|---|
| TaxiPathNode | 9 | 11 | 11 | Added ArrivalEventID/DepartureEventID at cols 9-10; existing offsets unchanged |
| CreatureSoundData | 30 | 38 | 38 | Fixed fidget 5, customAttack 4, loop/impact/jump/pet offsets (+1); npcSoundID skipped |
| AreaTable | 25 | 36 | 36 (via trailing) | FactionGroupMask corrected 20 -> 28; AreaTable schema dump updated to full 36 |
| SoundEntries | 29 | 30 | 30 | Added SoundEntriesAdvancedID at col 29 |
| Material | 3 | 5 | 5 | Added SheatheSoundID/UnsheatheSoundID cols 3-4 |
| AnimationData | 7 | 8 | 8 | Added behaviorTier col 7; Fallback corrected 6 -> 5 |
| WMOAreaTable | 20 | 28 | 28 | Expanded localized name block to 16 offsets + flags |
| GroundEffectTexture | 7 | 11 | 11 | Added DoodadWeight[4]; density 5 -> 9; TerrainType read 6 -> 10 (footsteps) |
| GMSurveyQuestions | 10 | 18 | 18 | Localized name block 8 -> 16 offsets |
| ChatProfanity | 2 | 3 | 3 | Added Language col 2 |
| NamesProfanity | 2 | 3 | 3 | Added Language col 2 |
| NamesReserved | 2 | 3 | 3 | Added Language col 2 |
| CreatureDisplayInfo | 12 | 16 | 16 | cdi_schema updated to 16; schema_for dump switched to wotlk variant |

## Correction pass (supervisor)

Additional fixes after the first commit (`485c1eea`) in `44bdc541`:
- `creature_sound.rs`: `cmd_schema()` 16 -> 28 fields (CreatureModelData 12340); SoundID remains col 13. The first commit fixed CreatureSoundData (38) but the loader then failed at CreatureModelData, so creature voices were still disabled.
- `npc_greeting.rs`: this is the loader behind the logged "NPC greetings failed to load ... CreatureDisplayInfo ... 12 vs 16" line. `cdi_schema()` 12 -> 16; NPCSoundID read moved 11 -> 12. Verified against the 12340 data: display 793 has 50 at col 12 (NPCSounds 50 = hello/goodbye/pissed 5977/5978/5979) and display 89 has 161 at col 12 (NPCSounds 161 = 7094/0/7095); col 11 is BloodID (0 on both).
- Stale comments updated in creature_sound.rs, ground_effects.rs, lib.rs, area_sound.rs.

Remaining secondary table: NPCSounds.dbc (5 fields) loads unchanged.

## Unresolved

- No builds/tests run per task constraints; compilation and runtime acceptance remain unverified.
- Other historical DBC mismatches listed in older handoffs (TaxiNodes, Exhaustion, AreaPOI, etc.) are out of scope for this log and remain unresolved.
- No new fields added to Rust structs where unused; semantic offsets preserved via schema. Future consumers may read trailing columns explicitly.
