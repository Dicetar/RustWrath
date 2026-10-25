# First gameplay acceptance scenario - source-pinned, unexecuted

Date: 2026-10-25
Owner: OpenCode
Status: prepared; graphical/live execution is NOT authorized.
Source identity: gameplay commits `720c83b6`, `2f61b877` and the item slice checkpoint
`2026-10-25-oc-item-replication.md` on `wotlk`.

This is the exact scenario the source path should be exercised against. Static SQL is from the
local AzerothCore 12340 base database. The live server may differ; every live assumption is named
below and must be verified rather than silently assumed.

## Chosen scenario

**Bounty on Garrick Padfoot**, quest ID `6`, in Northshire/Elwynn.

- Character: Alliance mage, level 2-5, with Fireball `133` in the spellbook/action bar. The base
  `playercreateinfo_action` row for a human mage is `(1,8,0,133,0)`; a live character must still
  be checked because level/rank supersession can differ.
- Questgiver and turn-in: Deputy Willem, creature entry `823`, base spawn GUID `79942` at
  `(-8947.64, -132.319, 83.7199)` on map `0`.
- Quest: level `5`, minimum level `2`, Elwynn (`QuestSortID=9`).
- Target: Garrick Padfoot, creature entry `103`, base spawn GUID `80247` at
  `(-9056.48, -460.903, 72.8318)` on map `0`.
- Loot: Garrick's Head, item `182`, from Garrick Padfoot at `100%`.
- Turn-in requirement: one Garrick's Head (`RequiredItemId1=182`, count 1).
- Reward choice: choose `0` = Tapered Pants, item `6076`. The other choices are
  `1` = Layered Tunic (`60`) and `2` = Ensign Cloak (`3070`).
- Spell: Fireball `133`, cast at Garrick while target-selected; melee/auto-attack is used only
  for the required attack/movement slice or if the spell is refused.

Static evidence:

- `data/sql/base/db_world/quest_template.sql:143` (quest 6 row; `Flags=0x80008`,
  `RequiredItemId1=182`, reward choices 6076/60/3070).
- `data/sql/base/db_world/creature_queststarter.sql:286` and
  `creature_questender.sql:269` (`(823,6)` for both).
- `data/sql/base/db_world/creature_template.sql:141` (Garrick 103) and `:693` (Willem 823).
- `data/sql/base/db_world/creature.sql:78320` (Garrick spawn 80247) and `:78019` (Willem spawn
  79942).
- `data/sql/base/db_world/creature_loot_template.sql:185` (`(103,182,0,100,...)`).
- `data/sql/base/db_world/item_template.sql:269` (Garrick's Head 182), `:5093` (Tapered Pants
  6076), `:202` (Layered Tunic 60), `:2191` (Ensign Cloak 3070).
- `data/sql/base/db_world/playercreateinfo_action.sql:65` (human mage action 0 = Fireball 133).

## Exact flow

1. Enter the world with the chosen mage and record character GUID, map, position, level, class,
   known spellbook and action buttons.
2. Select Deputy Willem and interact via the stock target/gossip path. Expect the NPC query,
   gossip/quest rows, and `QuestGiverStatus` (3.3.5 values are mapped to 1.12 marker values).
3. Open quest 6. Expect `SMSG_QUESTGIVER_QUEST_DETAILS`.
   - Quest 6 has `Flags=0x80008`: `SHARABLE | AUTO_ACCEPT`. On 3.3.5 the server may add the quest
     while handling the query, before a client Accept command. Record the actual flow.
   - If an explicit accept command must be proven, use quest `5261` from the same Deputy Willem
     as a control: `quest_template.sql:2714`, `Flags=0x8`, starter `(823,5261)`
     (`creature_queststarter.sql:290`), ender Eagan Peltskinner entry `196`
     (`creature_questender.sql:38`). That quest has no item loot and is not the primary combat
     scenario.
4. Walk from the abbey toward Garrick Padfoot using the stock movement/controller. The quest text
   names a bridge; the exact WMO/doodad/collision path is a live route dependency.
5. Target Garrick, start/stop attack, and cast Fireball 133. Expected source events:
   `SMSG_ATTACKSTART`, cast start/go or cast failure, spell-go misses with reflect byte where
   applicable, health field changes, and `SMSG_ATTACKSTOP` on death/stop.
6. Loot the corpse. Expect `SMSG_LOOT_RESPONSE` with Garrick's Head in a row; take it with
   `CMSG_AUTOSTORE_LOOT_ITEM`; expect `SMSG_LOOT_REMOVED`, `SMSG_ITEM_PUSH_RESULT`, and the
   item template query/answer. The item-slice source expectation is then an item `182` entity
   (`Guid` + `ObjectStore` + `ItemObject`), a player backpack slot referencing its GUID, and
   quest 6's held count becoming 1.
7. Return to Willem, complete the quest, request reward, choose reward 0. Expect
   `SMSG_QUESTGIVER_REQUEST_ITEMS`/`OFFER_REWARD`, `SMSG_QUESTGIVER_QUEST_COMPLETE`,
   the Tapered Pants item push, and the stock quest/chat surfaces. The reward item `6076`
   should follow the same item-entity/player-slot path and have its template fields populated.
8. Logout through the supported path and reconnect. Record whether the world returns to a
   coherent state. Automatic reconnect is a separate task; the current worker ends cleanly on
   `SMSG_LOGOUT_COMPLETE`, so a manual relaunch may be required.

## What static data does not establish

These require account/server/live-state access:

- The character's real name, GUID, class, level, location, known spells and action bar.
  The level must be at least 2 for quest 6; Fireball 133 must not have been superseded.
- Whether quest 6 is still available (not already completed, failed, or removed) on that
  character.
- Whether the live world database matches the base SQL for quest flags, spawn GUIDs, positions,
  loot conditions and reward rows. Base SQL is evidence of the default dataset, not the running
  database.
- The actual `AUTO_ACCEPT` behavior and whether `CMSG_QUESTGIVER_ACCEPT_QUEST` is sent.
- Whether the bridge, WMO, terrain collision and water path from Willem to Garrick are usable.
  This is the same route gap tracked by issues 21/24.
- Whether the live action bar contains 133 and whether the stock UI routes it through the
  production `ClientCommand` tap.
- Server acceptance of the movement, cast, loot and quest packets, including the missing
  full-target/spell-go decoding and cooldown/aura deltas noted in the source checkpoint.

## Separation of evidence

- Source-pinned here: static SQL rows, coordinates and the expected packet/event order.
- Reported checks: see `2026-10-25-oc-gameplay-path.md`. No build/test/renderer run was made for
  the final repairs; those remain UNKNOWN under the active execution restriction.
- Live acceptance: NOT RUN. Rendered acceptance remains UNKNOWN. Full stock-client replacement
  remains the release gate; this scenario only closes one ordinary gameplay path.
