# Northshire quest, loot and reward route (issue 26)

Status: landed on main; owner playthrough open
Owner: Claude Code (Opus 5.5)
Branch: fix/northshire-quest-loot-reward
Issue: 26
Next: the owner plays quest 6 once with the recorder on and reads the four lines named below

Base: `e67bd8d6a`. Source investigation against the 12340 reference (pin `e67b6ffe`, clean) and
the WM_BridgeLab AzerothCore source and base data. No Cargo, client, server, account or DB use.
`rustfmt --check` passes on the changed files; CI compiles the push.

## What the client knows after a loot

The gap between "the loot slot disappeared" and "the bag holds the item" is three separate
facts, each reaching a different surface:

| Fact | Packet | Reaches | Recorder |
|---|---|---|---|
| The row is gone | `SMSG_LOOT_REMOVED` (slot only) | loot window gap, `LOOT_SLOT_CLEARED` | `LootSlotRemoved`, now with the row's item |
| The server stored it | `SMSG_ITEM_PUSH_RESULT` (entry, count, bag, slot, total held; `PlayerStorage.cpp:4846-4858`) | loot chat line; the "Garrick's Head: 1/1" toast, computed from the packet's total (`CheckForQuestItemLoot` `0x6d7de0`, `ui_quest_log.rs:on_item_looted`) | `ItemPushResult`, now only the active player's |
| It is in the bags | the item's create and `PLAYER_FIELD_PACK_SLOT_*` in a later batched `SMSG_UPDATE_OBJECT` | bag frame; held counts for `GetQuestLogLeaderBoard` (`ui_quest_log.rs`, `0x622130`); quest 6's completion state fires `QUEST_LOG_UPDATE` | new `QuestItemHeld` |

AzerothCore's `SMSG_QUESTUPDATE_ADD_ITEM` is empty (`PlayerQuest.cpp:2483`) and is dropped by the
Wrath decoder (`gameplay.rs:3191`). The toast therefore proves the push only; the quest-log line
and the bag prove the third fact.

The 2026-11-25 recorder files (`Logs/InteractionDiagnostics`, local) never covered quest 6 or item
182: their loots (22644, 3174) and casts (Victory Rush 34428, Battle Shout 5242) are a later
character's. Their "loot slot removed (strong); server outcome unconfirmed" was a recorder
limitation: the summary stopped at the first evidence row, the removal, although the matching
`ItemPushResult` with item and count was in the same tick.

## Boundaries

| Boundary | Disposition | Decisive anchors |
|---|---|---|
| 1. Offer → row → detail → accept → journal | source sound; live open | row: `ui_gossip/verbs.rs:93-113` (1-based, same active filter as the snapshot); query `gameplay.rs:449` and accept `:458` match `QuestHandler.cpp:209`/`:119`; slot add from `PLAYER_QUEST_LOG` (`ui_quest_log.rs`, `QUEST_ACCEPTED` then deferred `QUEST_LOG_UPDATE`) |
| 2. Target/attack → ability → combat → lootable | source sound; live open | on-next-swing Heroic Strike held as `Held::Strike` (`spell/cast_send.rs:711`); lootable from `UNIT_DYNAMIC_FLAGS` 79 remapped to 143 (`wrath.rs:13005`); corpse `CMSG_LOOT` behind `CanLootNow` (`target/click.rs:904-926`) |
| 3. Loot → response → bag/item/count → objective | source sound; recorder fixed; live open | `read_loot_response` matches `LootMgr.cpp:945-1066`; `read_item_push` matches `SendNewItem`; held counts `ui_quest_log.rs:1268-1315` |
| 4. Reward choice → turn-in → completion → reward item | source sound; recorder fixed; live open | `read_quest_offer` matches `SendQuestGiverOfferReward` to its tail; `GetQuestReward(i)` → 0-based choice → `chosen_reward` (`ui_quest/verbs.rs:526-596`); `CMSG_QUESTGIVER_CHOOSE_REWARD` matches `QuestHandler.cpp:255`; `RewardQuest` stores `RewardChoiceItemId[0]` = 6076 and pushes it as received from the NPC (`PlayerQuest.cpp:697-705`) |

## Eligibility and the auto-accept flow

Quest 6: level 5, minimum level 2, `Flags=524296` (`0x80008`, includes `QUEST_FLAGS_AUTO_ACCEPT`),
`RequiredItemId1=182`, choices 6076/60/3070 (`quest_template.sql:143`; no update changes it). The
live `Quests.IgnoreAutoAccept = 0`, and `IsAutoAccept` is the flag alone (`QuestDef.cpp:270`). So
the server adds quest 6 while answering the query (`QuestHandler.cpp:228`), before the details
show. 12340's `AcceptQuest` (`QuestFrame.cpp:725`) still sends the accept, and so does ours; the
server then fails `CanTakeQuest` (`QuestHandler.cpp:148`) with `SMSG_QUESTGIVER_QUEST_INVALID`
("already on that quest") and closes the gossip. That is stock behaviour, not a client defect.
The recorder's Accept line will show that refusal; the slot add precedes the accept.

Quest 5261 (`Flags=8`, minimum level 1) is the control for an explicit accept if Firna cannot take
quest 6; it has no loot or reward choice.

If the bags are full at turn-in, `RewardQuest` mails the reward and sends no push: the turn-in
line then reports the missing chosen reward push, correctly.

## Changes

`71374b9be`, recorder only; no gameplay path changed.

- `QuestItemHeld { item_id, count }`: the held count the objective line reads, when it changes.
- `LootSlotRemoved` carries the row's item and count (`LootState::row_at`).
- `ItemPushResult`/`RewardItemPushed` only for the active player; `RewardItemPushed` only for a
  from-NPC push, so a loot is never counted as the reward.
- The loot action summary names the push: `; item 182 x1 pushed (item match)`.
- The turn-in line requires the chosen reward's own push and names it.

Focused test: `q04_turn_in_requires_the_chosen_rewards_own_push`. Not run locally.

## Handoff to ClaudeJr: last authoritative state before disconnect

After a clean turn-in the recorder should hold, in order:

1. `QuestSlotAdded` for quest 6 (at the details, before Accept).
2. `LootSlotRemoved { item_id: Some(182), count: Some(1) }`, `ItemPushResult { 182, 1 }`, then
   `QuestItemHeld { 182, 1 }`.
3. `QuestCompletePacket` for quest 6 with subject item 6076, `QuestSlotRemoved` for quest 6,
   `RewardItemPushed { 6076, 1 }`; turn-in line `... chosen reward 6076 pushed`.

Identity: Firna, the active player (`WrathWorldView.player_guid`; the 2026-11-20 note records
0x1522). Quest state: quest 6 rewarded, no longer in the log. Objective: 182 destroyed at turn-in
(`RewardQuest`'s `DestroyItemCount`); no `QuestItemHeld` row is written once the quest has left
the log. Reward: 6076 x1 in the bags. After reconnect Jr compares: quest 6 absent and not
offered by Willem, no 182 in the bags, one 6076.

## Not proven

Every boundary's runtime result; the stock `QuestFrame.lua` handling of `QuestGetAutoAccept`;
the reward and objective in the rendered bag frame.
