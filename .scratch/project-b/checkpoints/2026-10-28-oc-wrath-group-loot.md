# OC — Wrath group loot and loot suffix factors

Date: 2026-10-28
Owner: OpenCode
Base HEAD: `2ffe7f61` (Questie animation defaults and GetParent)
Goal: wire the 12340 group-loot family into the existing donor loot owners and retain the
loot-response random suffix factor end to end, without changing Astra's UI behavior.

Status: in progress. Source-only; no build, test, addon, server, socket or graphical run is
authorized in this session.

## File claim

Ours:

- `crates/benilla-protocol-wrath/src/opcode.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-protocol/src/messages/loot.rs`
- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Mechanical `LootItem.suffix_factor: 0` constructor propagation only (no behavior edits; these
are Astra's claimed UI files):

- `crates/benilla-app/src/ui_loot/mod.rs`, `ui_loot/net.rs`, `ui_loot/call_tests.rs`,
  `ui_items/drain.rs`
- other existing `LootItem` literals in app tests/fixtures

Not touched: `ui_loot_roll.rs` (shared roll types did not change), `ui_loot/mod.rs` link
formatter behavior, `benilla-ui/src/script/**`, any `events_regions.rs`.

## AzerothCore authority (local source at `5e741c37`)

Opcodes, `Server/Protocol/Opcodes.h:700-704`:

| Opcode | Value |
|---|---|
| `SMSG_LOOT_ALL_PASSED` | `0x29E` |
| `SMSG_LOOT_ROLL_WON` | `0x29F` |
| `CMSG_LOOT_ROLL` | `0x2A0` |
| `SMSG_LOOT_START_ROLL` | `0x2A1` |
| `SMSG_LOOT_ROLL` | `0x2A2` |

`SMSG_LOOT_START_ROLL` (`Groups/Group.cpp::SendLootStartRoll`, `:861-871`; per-player
`SendLootStartRollToPlayer`, `:886-901`), in order:

1. `u64 itemGUID` — the roll's allocated item guid, the identity `CMSG_LOOT_ROLL` sends back
2. `u32 mapId` — 3.3.3-only field
3. `u32 itemSlot`
4. `u32 itemId`
5. `u32 randomSuffix` — the suffix factor
6. `u32 randomPropertyId`
7. `u32 itemCount`
8. `u32 countdown` — milliseconds
9. `u8 rollVoteMask`

The per-player form clears `ROLL_FLAG_TYPE_NEED` (`0x02`) when the player cannot need. Masks
(`Loot/LootMgr.h:42-49`): pass `0x01`, need `0x02`, greed `0x04`, disenchant `0x08`.

`SMSG_LOOT_ROLL` (`Group::SendLootRoll`, `:908-930`), in order:

1. `u64 itemGUID`
2. `u32 itemSlot`
3. `u64 rollerGuid`
4. `u32 itemId`
5. `u32 randomSuffix`
6. `u32 randomPropertyId`
7. `u8 rollNumber`
8. `u8 rollType` (`0` pass, `1` need, `2` greed; `3` disenchant)
9. `u8 autoPass` — 1 when the client should print the automatic-pass line

`SMSG_LOOT_ROLL_WON` (`Group::SendLootRollWon`, `:933-951`), in order:

1. `u64 itemGUID`
2. `u32 itemSlot`
3. `u32 itemId`
4. `u32 randomSuffix`
5. `u32 randomPropertyId`
6. `u64 winnerGuid`
7. `u8 rollNumber`
8. `u8 rollType`

There are no vote pairs in the packet body; the server sends this same body once per voter
(`:947-951`). The handoff's "vote pairs" phrasing is a loop, not a wire unit.

`SMSG_LOOT_ALL_PASSED` (`Group::SendLootAllPassed`, `:954-961`), in order:

1. `u64 itemGUID`
2. `u32 itemSlot`
3. `u32 itemId`
4. `u32 randomPropertyId`
5. `u32 randomSuffix` — note the swapped order versus the other three

`CMSG_LOOT_ROLL` (`Handlers/GroupHandler.cpp:494-499`): full `u64 itemGUID`, `u32 itemSlot`,
`u8 rollType` (`0` pass, `1` need, `2` greed, `3` disenchant). `ObjectGuid`'s byte-buffer
operators write/read the raw `u64` (`Entities/Object/ObjectGuid.cpp:69-79`), so every guid here
is a fixint, not packed.

## Contract decisions

- `benilla-protocol-wrath` gains typed `ServerEvent` variants carrying every field above,
  including `map_id`, `item_count`, `vote_mask` and `auto_pass`.
- The existing shared donor roll events (`benilla-protocol::messages::LootStartRoll` etc.) are
  not extended for `map_id`, `item_count`, `vote_mask` or `auto_pass`, because their only
  consumer (`ui_loot_roll.rs`) is Astra's claimed file and the target 12-value
  `GetLootRollItemInfo` contract is her slice. Translation passes the fields the donor owner
  already models; the target-only fields stay on the typed protocol event and are recorded as
  the owner extension below. No silent discard is claimed.
- The shared `LootItem` gains `suffix_factor` (WP3). The 1.12 decoder reads the wire word
  truthfully (vmangos sends 0); the Wrath decoder retains the real suffix factor; the app
  translation copies it. `random_property_id` stays a separate signed property, never merged
  with the factor.
- The UI consumer change in `ui_loot/mod.rs` (`resolve_item` must pass
  `item.suffix_factor` through `item_link_full_in_target` instead of the hardcoded `0`) is
  Astra's edit. It is recorded below, not made here.

## Source delivered by this claim

- `benilla-protocol-wrath::opcode`: the five group-loot opcodes, values verified against the
  local AC `Opcodes.h`.
- `benilla-protocol-wrath::gameplay`: `loot_roll` encoder, four typed `ServerEvent` variants, the
  four readers in exact wire order, and `LootItem.suffix_factor` retained from
  `SMSG_LOOT_RESPONSE`.
- `benilla-protocol::messages::LootItem`: `suffix_factor`, separate from
  `random_property_id`; the 1.12 reader keeps the wire word (vmangos sends 0).
- `benilla-app::wrath`: the four inbound events translate onto the existing donor
  `SessionEvent` roll family; the loot-response rows carry the factor; the existing
  `ClientCommand::LootRoll` is tapped to `CMSG_LOOT_ROLL`.
- Constructor propagation only in the named app files. No UI behavior changed.

## Owner extension needed (Astra)

1. `crates/benilla-app/src/ui_loot_roll.rs` (`ActiveRoll`, snapshot, `LootRollEntry`):
   retain `item_count`, `vote_mask` and `auto_pass` from the protocol event after the shared
   types grow, and expose the 3.3.5 twelve-value `GetLootRollItemInfo`
   (`texture, name, count, quality, bindOnPickUp, canNeed, canGreed, canDisenchant,
   reasonNeed, reasonGreed, reasonDisenchant, deSkillRequired`) from the mask. The current
   binding returns the 1.12 five-value tail.
2. `crates/benilla-ui/src/script/loot.rs::LootRow`: add `suffix_factor` beside
   `random_property_id`, and `crates/benilla-app/src/ui_loot/mod.rs::resolve_item` must pass it
   through `item_link_full_in_target(target, ...)` instead of the hardcoded `0` /
   `item_link_full` pair. The post-template link rebuild at `ui_loot/mod.rs:1196` has the same
   hardcoded zero.
3. `LootRoll`'s `auto_pass` target fact belongs to the chat-line composer (`RollLine::Announce`)
   when the owner is extended; the donor line law has no automatic-pass key.

## Verification plan (not run; needs an explicit grant)

- `cargo check --locked --offline -p benilla-protocol-wrath -p benilla-app --lib`
- focused decoder/translation cases only after a test-add + run grant.
- live group roll + vote round-trip only after a server/game grant.
