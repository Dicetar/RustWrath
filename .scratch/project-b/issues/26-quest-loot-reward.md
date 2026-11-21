# 26 — Complete a quest, loot and preserve the reward

**Status:** ready-for-agent

**Plan reference:** C11. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The player accepts a real quest, completes its objective, loots and turns it in, then reconnects with the correct reward and inventory.

**Blocked by:** [25 — Perform normal attacks and one real spell](25-ordinary-combat-slice.md)

**Related existing issues:** [09](09-b007-gameplay-loop-slice.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM.

## Acceptance

- [ ] Pin an existing nearby quest and initial character/inventory state.
- [ ] Implement the narrow gossip, quest log, objective, loot, inventory and reward UI/protocol path.
- [ ] Show objective and reward state come from the server; failed/repeated interactions do not duplicate reward or loot.
- [ ] Reconnect after the reward and confirm inventory/quest state persists.
- [ ] Record protocol correctness, UI completeness and gameplay outcome separately; list uncovered quest types.

## Boundaries and handoff

Do not construct a client-only demo quest or modify production content merely to make the test pass.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


## Source gap table (2026-11-22)

Read-only source audit on branch `oc/gap-26-quest-loot` (HEAD 52749c0f). Every symbol below was
confirmed by grep against the working tree; no file, function or field is invented. "Covered" means
the source path exists, not that it was exercised. Runtime proof is called out as Blocked.

| Acceptance / behavior | Status | Source owner (file:function) | Gap or ticket |
|---|---|---|---|
| Pin an existing nearby quest and initial character/inventory state | Blocked: needs owner run | crates/benilla-app/src/ui_quest/net.rs:quest_giver_status; crates/benilla-app/src/quest_markers/query.rs (marker query) | No client "pin" feature exists; choosing a real giver/quest is a live setup act. Owner must walk to a giver in the client and record the quest id + starting inventory. |
| Gossip / questgiver window opens and lists the quest | Covered | crates/benilla-app/src/ui_gossip/mod.rs:GossipState; crates/benilla-app/src/ui_quest/net.rs:quest_greeting / quest_detail | Live open needs owner run. |
| Accept quest sends the accept command and fills a quest-log slot | Covered | crates/benilla-app/src/ui_quest/verbs.rs:QuestVerbs::accept; crates/benilla-app/src/ui_quest_log.rs (slot watch) | Slot-add confirmation is runtime (quest_trace Stage::QuestSlotAdded). |
| Objective progress (kill / item / complete) updates counters | Covered | crates/benilla-app/src/ui_quest/net.rs:quest_objective_kill / quest_objective_item / quest_objectives_complete; crates/benilla-ui/src/script/quest_log.rs:QuestSlotView.counters | Kill credit depends on issue 25 (ordinary combat); cross-slice dependency, not a gap here. |
| Loot window opens on the corpse / chest | Covered | crates/benilla-app/src/ui_loot/net.rs:on_response; crates/benilla-app/src/ui_loot/mod.rs:LootState::open | Live window paint needs owner run. |
| Take a loot item (CMSG_AUTOSTORE_LOOT_ITEM) and it lands in a bag | Covered | crates/benilla-app/src/ui_loot/calls.rs (take dispatcher); crates/benilla-app/src/wrath.rs:ActionIntent::LootTake → ClientCommand::AutostoreLootItem | Item-in-bag confirmation is runtime. |
| Group / party loot roll window and RollOnLoot vote | Covered | crates/benilla-app/src/ui_loot_roll.rs:LootRolls | Only the party-loot branch; solo loot uses the take path above. |
| Reward choice selected at turn-in | Covered | crates/benilla-app/src/ui_quest/verbs.rs:QuestVerbs::reward (chosen_reward latch) | — |
| Turn-in sends CMSG_QUESTGIVER_CHOOSE_REWARD and the reward item is pushed to a bag | Covered | crates/benilla-app/src/ui_quest/verbs.rs:QuestVerbs::complete; crates/benilla-app/src/ui_quest/net.rs:on_complete / quest_complete; crates/benilla-app/src/wrath.rs (ItemPushResult → RewardItemPushed) | Reward-in-bag confirmation is runtime. |
| Objective and reward state come from the server | Covered | crates/benilla-app/src/field_watch.rs (FieldNotice::QuestLog); crates/benilla-app/src/ui_quest/net.rs:quest_complete; crates/benilla-app/src/wrath_inventory.rs:WrathInventory::apply | Slots and counters are read from PLAYER_QUEST_LOG; reward via QuestComplete. |
| Failed / repeated interactions do not duplicate reward or loot | Covered | crates/benilla-app/src/ui_quest/verbs.rs (acted latch, pending_accepts dedup, pending_selection); crates/benilla-app/src/ui_loot/mod.rs:LootState::remove_slot; crates/benilla-app/src/ui_loot_roll.rs:LootRolls::start (duplicate roll ignored) | Source guards exist and are unit-tested; duplicate-free live behavior is Blocked: needs owner run. |
| Reconnect after the reward and confirm inventory/quest state persists | Blocked: needs owner run | crates/benilla-app/src/ui_quest_log.rs:QuestLog::clear_session (templates kept); crates/benilla-app/src/ui_quest/net.rs:on_login / on_worldport / world_enter_reset; crates/benilla-app/src/wrath_inventory.rs | Persistence is server-authoritative; the client re-reads on reconnect. Must confirm quest log + bags in a live reconnect. |
| Record protocol correctness, UI completeness and gameplay outcome separately | Covered | crates/benilla-app/src/quest_trace.rs:quest_ui_correlations / turn_in_correlations / slot_correlations / loot_vendor_correlations | Diagnostics exist; a run must collect and file the three records. |
| List uncovered quest types | Missing | (none) | No source or doc enumerates which quest types the narrow slice does not cover (escort, dungeon, raid, daily, world, auto-complete). Small doc ticket below. |

## Small tickets

No source gap blocks the core slice; the path (gossip → accept → objective → loot → turn-in →
reward push → inventory) has owners and unit tests. The remaining work is one documentation gap
and the live-only checks below.

- **Enumerate uncovered quest types (doc-only).** Files: `.scratch/project-b/issues/26-quest-loot-reward.md` (this note) or a linked note. Acceptance: a short list names the quest types the narrow path does not cover (escort, dungeon, raid, daily, world, auto-complete) and points each at a future ticket. Check: review finds every named type with a one-line rationale. No source gap, so no new owner.
- **Live-only check — accept + slot add (owner run).** Files: none (runtime). Acceptance: after accepting the pinned quest, the quest-log slot appears with the server's objective text. Check: quest_trace Stage::QuestSlotAdded recorded with no telemetry gap; screenshot of the open quest log.
- **Live-only check — loot window + item in bag (owner run).** Files: none (runtime). Acceptance: killing/ opening the pinned source shows a loot window; taking the item removes the row (Stage::LootSlotRemoved) and the item appears in a bag. Check: LootSlotRemoved and RewardItemPushed/ItemPushResult both observed; bag shows the item.
- **Live-only check — turn-in reward, no duplicate (owner run).** Files: none (runtime). Acceptance: choosing the reward fires one CMSG_QUESTGIVER_CHOOSE_REWARD, the reward lands once in a bag, and a repeated turn-in click does not add a second copy. Check: QuestCompletePacket + one RewardItemPushed; turn_in_correlations names a single ChooseReward write; bag count is one.
- **Live-only check — reconnect persistence (owner run).** Files: none (runtime). Acceptance: after the reward, reconnecting shows the same quest-log state and the reward item still in bags. Check: post-reconnect quest log and bag match the pre-reconnect snapshot; no client error.

## Uncovered quest types (2026-11-23)

The narrow gossip → accept → objective → loot → turn-in → reward path covers one outdoor, solo,
one-shot, kill/item quest. The types below are outside it. `TBD-*` values are placeholders for
future tickets, not assigned issue numbers; no runtime behavior is claimed for any row.

| Quest type | Why the narrow path misses it | Source hint | Future ticket |
|---|---|---|---|
| Escort / party-accept | Accept needs the party verdict and `SMSG_QUEST_CONFIRM_ACCEPT`; the narrow path accepts solo and never escorts. | `quest_flags::PARTY_ACCEPT`; `ui_quest_share.rs` | `TBD-ESCORT` |
| Dungeon | Instance map, reset and dungeon-group objectives; the route is one outdoor giver. | `QuestInfo.dbc` tag 81 "Dungeon" | `TBD-DUNGEON` |
| Raid | Raid group, instance and raid loot rules; the narrow path is solo with party-loot only. | `QuestInfo.dbc` tag 62 "Raid"; `quest_flags::RAID` | `TBD-RAID` |
| Daily | Daily flag, cap and reset; the narrow path has one accept/turn-in and no replay. | `ui_quest_log.rs` `flags & 0x1000`; `PLAYER_FIELD_DAILY_QUESTS_1` | `TBD-DAILY` |
| World (world event) | Zone/world-state and calendar-driven objectives, not one giver's kill/item row. | `QuestInfo.dbc` tag 82 "World Event"; `CalendarHoliday` | `TBD-WORLD` |
| Auto-complete | Granted and completed in one step, never in the log; the narrow path needs a log slot and turn-in. | `quest_flags::AUTO_REWARDED`; `_REWARD_REP` | `TBD-AUTOCOMPLETE` |
| Exploration | Completes on an area trigger; no kill/item counter to update. | `quest_flags::EXPLORATION` | `TBD-EXPLORATION` |
| Stay-alive | Survival objective with no kill/item counter. | `quest_flags::STAY_ALIVE` | `TBD-STAYALIVE` |
| Hidden rewards | Rewards ship only in `SMSG_QUESTGIVER_OFFER_REWARD`; the turn-in choice/push path is not exercised. | `quest_flags::HIDDEN_REWARDS` | `TBD-HIDDENREWARD` |
| Group / Elite | Suggested party size and party coordination; the narrow path is one character. | `QuestInfo.dbc` tag 1 "Elite"; `suggested_players` | `TBD-GROUP` |
| PvP | PvP-event/flag objectives, not creature kills or item collection. | `QuestInfo.dbc` tag 41 "PvP" | `TBD-PVP` |
| Repeatable / weekly | Replay and reset semantics; the narrow path's acted latch assumes one-shot. | giver `repeatable`; reset machinery | `TBD-REPEATABLE` |
| Vehicle | Vehicle seat/control objectives; the client currently boards no vehicle. | `gameplay.rs` "This client boards no vehicle" | `TBD-VEHICLE` |

## Evidence refresh (2026-11-24)

The issue 26 source gap table and uncovered-type inventory remain current. The landed evidence
reviewed for this refresh is `5c4786a91`, which merges the landed issue-27 ledger ancestry into the
branch, and `efdbf2a75`, which enumerates the uncovered quest types. No later landed commit supplies
new quest, loot, reward, duplicate-prevention, or reconnect runtime evidence. The existing Covered,
Missing, and Blocked classifications are therefore preserved; connected behavior remains UNKNOWN
without the prohibited live client/server run.


