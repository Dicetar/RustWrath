# OpenCode checkpoint - inventory fidelity review and item-query repair (O3)

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `160d648e` (aura delivery)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O3.
Purpose: inspect the item/container projection against the 12340 sources, repair the
scenario-critical defect the inspection demonstrated, and enumerate the known narrowing for
Astra's compatibility ledger. This file is the file claim and the delivery record.

## File claim

Editing:

- `crates/benilla-protocol-wrath/src/gameplay.rs`
- this checkpoint

Not editing: the app item/loot owners, player field bands, Astra's procgen/compatibility/
architecture paths.

## Demonstrated defect repaired

`read_item_info` read a five-slot damage block, copying the 1.12 shape. 12340 defines
`MAX_ITEM_PROTO_DAMAGES 2` since 3.1.0 (`ItemTemplate.h:613`), so the parse consumed 36 bytes of
the following armour/resistance/delay/spell/description fields as damage entries; every tooltip
and template value after the damage block was wrong (this is the item-damage-count defect
Astra's review named). The parser now reads two blocks, matching `HandlerItemQuerySingleOpcode`
(`ItemHandler.cpp:451-456`). Nothing else in the record moved.

## Scenario-source path, verified against 12340 sources

- Loot 182: `CMSG_AUTOSTORE_LOOT_ITEM` is already tapped; `SMSG_LOOT_RESPONSE`
  (`LootMgr.cpp:945-954,956-...)` matches the parser: raw guid, u8 type (0 = error + u8),
  gold, u8 count, then per slot `{slot u8, item u32, count u32, display u32, randomSuffix u32,
  randomPropertyId u32}` and the trailing slot-type byte.
- Receipt: `SMSG_ITEM_PUSH_RESULT` (`PlayerStorage.cpp:4837-4861`) matches the parser through
  `count`; 12340 appends one total-held word that the 1.12 seam does not carry and that the
  parser leaves unread. Bag/slot numbering (255 backpack, 19-22 bags) matches 1.12.
- Item instance: the 12340 item window (`fields.rs:10-48`) matches the 1.12 item window for the
  16 object/owner/contained/creator/gift/stack/duration/charges/flags fields and the first
  seven enchant slots; the translation maps property seed, random properties, durability and
  max durability across the moved columns (`wrath.rs:1910-1942`).
- Reward 6076: `SMSG_QUESTGIVER_OFFER_REWARD` (`GossipDef.cpp:647-737`) matches the parser
  through the choice/reward triples, money, xp, honour, reward spell; the pinned choice-0 item
  is therefore carried by value, not by a guessed slot.
- Quest item progress: `SMSG_QUESTUPDATE_ADD_ITEM` and the kill counters were already parsed;
  no change needed.

No item or reward is spawned or invented.

## Compatibility gaps recorded for the ledger (not scenario-blocking)

- Player bank band narrows 28 to 24 slots and bank bags 7 to 6 (`push_player_inventory_fields`),
  a deliberate 1.12-layout projection.
- Item enchant slots narrow 12 to the 1.12 seven.
- Buyback price/timestamp columns (3.3.5 bases 1201/1213) are not projected; the 1.12 owners
  read bases 1226/1238 with a different count.
- 3.3.5-only template fields after `BagFamily` (socket colours/contents, socket bonus, gem
  properties, disenchant id, scaling values) are parsed only as far as the 1.12 seam needs and
  are otherwise left unread; `ITEM_FIELD_CREATE_PLAYED_TIME` and `ITEM_TEXT_ID` have no
  counterpart on the other side.
- Random suffix ids are carried on push/loot records but the 1.12 tooltip owner only consumes
  random-property ids; a lossless suffix projection belongs to the ADR-0001 owner extension.

## Pending checks

Authorized source work only: no build, test, formatter, linter, graphical, socket, WM, receiver
or live-server execution. Gate C remains unverified. Fixtures still needed: a two-block weapon
template, a zero-damage quest item template, an armour piece with resistances, and the full
loot→push→bag→quest-update sequence.
