# Wrath instance, dungeon and raid difficulty from the server

Status: landed on main (`4e7dc5466`)
Owner: Claude Code / Opus 5.5 (nightly 2026-11-27)
Branch: wrath/instance-difficulty
Issue: none
Next: owner's build and launch; right-click your portrait → Dungeon Difficulty → Heroic

## Why

The 3.3.5a client answered difficulty from nothing the server said: `GetInstanceDifficulty`
was a constant 1, `GetDungeonDifficulty`/`GetRaidDifficulty` read CVar rows the stock client
does not use for this, and `GetInstanceInfo` left the difficulty slots nil. The server's three
difficulty packets were not decoded at all.

Worse, `SetDungeonDifficulty(n)` and `SetRaidDifficulty(n)` sent the 1-based menu value as is.
The stock unit popup calls `SetDungeonDifficulty(2)` for Heroic; AzerothCore refuses mode 2 for a
dungeon (`MiscHandler.cpp:1268`, `>= MAX_DUNGEON_DIFFICULTY`), so choosing Heroic never took.
12340 sends `n - 1` (`Script_SetDungeonDifficulty` `0x526050`).

## What changed

- **Protocol** (`benilla-protocol-wrath`): `SMSG_INSTANCE_DIFFICULTY` (`0x33B`, `u32
  difficulty, u32 dynamic`) and the server forms of `MSG_SET_DUNGEON_DIFFICULTY` (`0x329`) and
  `MSG_SET_RAID_DIFFICULTY` (`0x4EB`) (`u32 difficulty, u32 player, u32 in_group`) decode into
  `ServerEvent`s, checked against AzerothCore `InstancePackets.cpp:35`, `:56`,
  `Player.cpp:11685` and the client handler `GameUI.cpp:6671-6690`, `:6796`.
- **Session events** (`benilla-protocol`): `InstanceDifficulty`, `DungeonDifficultySet`,
  `RaidDifficultySet`, and `GroupDifficulty`, which carries `SMSG_GROUP_LIST`'s three difficulty
  bytes. These were already decoded but dropped; the event now follows the list in
  `wrath_group_list_events`.
- **One owner for the cells** (`benilla-ui` `script/instance.rs`, `Difficulty` on the model): the
  six 12340 cells (`CGGameUI::m_instanceDifficulty`, `m_dungeonDifficulty`, `m_raidDifficulty`;
  `CGPartyInfo::m_dungeonDifficulty`, `m_raidDifficulty`, `m_dynamicDifficulty`), the effective
  getters `GetCurrentDungeonDifficulty` (`0x5138D0`) / `GetCurrentRaidDifficulty` (`0x513900`),
  and the reference's stores `SetDungeonDifficulty` (`0x525530`) / `SetRaidDifficulty`
  (`0x5255A0`) with their change lines `ERR_DUNGEON_DIFFICULTY_CHANGED_S` +
  `DUNGEON_DIFFICULTY<n>` and `ERR_RAID_DIFFICULTY_CHANGED_S` + `RAID_DIFFICULTY<n>`, as
  `CHAT_MSG_SYSTEM`.
- **Lua**: `GetInstanceDifficulty`, `GetDungeonDifficulty` (two returns), `GetRaidDifficulty`
  (two returns) and `GetInstanceInfo`'s seven returns follow `0x515750`, `0x515790`,
  `0x515810` and `0x51A8C0`. The setters take `n - 1`, ignore out-of-range values, refuse a
  non-leader in a group with `ERR_NOT_LEADER`, write the cells at once and send.
- **App** (`ui_instance.rs`): the four events queue in `InstanceState`; `feed_instance` hands
  them to the VM in order, after `feed_party` so the group bytes see the roster their list built.
  A `/reload`'s fresh VM is seated from the last VM's cells (the reference keeps them as engine
  statics). `ui_party::feed_party`'s re-export is no longer test-only (it is the ordering anchor).

## Deviations and what is not wired

- Inside a raid instance the reference names the instance's `MapDifficulty.dbc`
  `difficultystring`; the catalog does not load it, so the line uses `RAID_DIFFICULTY<instance+1>`
  (the same four stock texts). `GetInstanceInfo`'s difficulty name is the no-row answer `""` for
  the same reason.
- Dynamic-difficulty maps (`Map.dbc` flag `0x100`: Icecrown Citadel, Ruby Sanctum) are not
  wired: no flags in the map catalog, so `isDynamic` is false and a group raid write is a store,
  not `CMSG_CHANGEPLAYER_DIFFICULTY`. `SMSG_CHANGE_PLAYER_DIFFICULTY_RESULT`
  (`PartyFrame.cpp:0x0052C460`, the only `PLAYER_DIFFICULTY_CHANGED` producer) is not decoded.
- The group-list check compares against the pair the getters answered after the last difficulty
  write (`Difficulty::announced`), because the roster reaches the model before the list's bytes.
  A `SMSG_GROUP_DESTROYED` with no following list leaves that pair stale until the next list.

## Verification

- Source only: reviewed against the 12340 reference (`GameUI.cpp`, `GameUI_Scripts.cpp`,
  `Player_C.cpp:11618-11854`, `PartyFrame.cpp:856-1025`) and the AzerothCore writers named above.
- `rustfmt --edition 2021 --check` on every touched file: clean.
- One regression test, `script::instance::tests::heroic_goes_out_zero_based_and_the_server_packets_land`;
  the group-list test in `benilla-app/src/lib.rs` now also checks the `GroupDifficulty` event.
- Not compiled locally (owner rule), not run, no client. CI run `46060499413` on `4e7dc5466`:
  green (windows 21m24s, gates 23m1s; 6867 tests passed, including the new test,
  `every_session_event_kind_has_one_owner` and the group-list test).
