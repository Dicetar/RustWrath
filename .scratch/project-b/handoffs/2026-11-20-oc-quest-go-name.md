# Quest log GameObject objective names

Status: ready to merge
Owner: OpenCode / DeepSeek
Branch: oc/quest-go-name
Issue: 18 (quest log row inventory: GameObject objective name)
Next: Owner launches the client and reads a quest with a GameObject objective.

## What changed
- `crates/benilla-ui/src/script/quest_log.rs`: the quest-log store keeps a GameObject name map,
  wait set and ask list beside the creature ones. `Model::quest_go_lookup` clears
  `GO_OBJECTIVE_BIT` (0x8000_0000) and answers a hit, or queues one ask per entry and holds the
  entry awaited. The `is_go` branch of `line_parts` reads that lookup, so a hit names the line
  under `QUEST_OBJECTS_FOUND` and a miss keeps the `...` placeholder. `UiScript::take_quest_go_asks`
  drains the asks; `set_quest_go_name` caches the name, clears the wait and owes one
  `QUEST_LOG_UPDATE` refire through `flush_quest_log_refires`, the same signal the creature answer
  fires.
- `crates/benilla-app/src/ui_quest_log.rs`: the feed drains the engine's GO asks, keeps them in
  `QuestLog::go_waits`, answers from the live `GameObjectTemplates` cache (`crate::go_templates`)
  and asks that owner through its ask-once lookup with no spawn guid on a miss. A drained ask is
  the leaderboard's event lookup; the frames between only read, so a refused entry is not asked
  again until the next miss. A name that lands calls `set_quest_go_name`, which repaints the line
  once.
- Reference check: 12340 keys the GameObject record by `objective & 0x7fffffff` and passes
  `params->m_objectCallback` to the same DBCache lookup (the GameObject arm around 0x4e031e);
  the same one-callback-per-call miss rule is at 0x4e05c9. No offsets were invented.

## Named tests
- `benilla-ui` `script::quest_log::tests::a_cached_gameobject_name_fills_the_object_line`
- `benilla-ui` `script::quest_log::tests::a_gameobject_miss_holds_the_placeholder_and_asks_once`
- `benilla-ui`
  `script::quest_log::tests::a_gameobject_name_landing_replaces_the_placeholder_and_fires_once`
- `benilla-app`
  `ui_quest_log::tests::a_gameobject_ask_is_answered_from_the_template_cache_without_asking_again`

## CI
Not run yet. Pushed for GitHub CI to compile and test; rustfmt was run on both changed files.

## What is not proven in game
The engine answer and the app feed are proven only by the named focused tests. No client or server
run read a real GameObject objective line, and the live query path (a GameObject template the
client has not cached, answered by `CMSG_GAMEOBJECT_QUERY` and a `SMSG_GAMEOBJECT_QUERY_RESPONSE`)
is not exercised end to end in game.
