# Lossless Wrath inventory: typed owner and bank/bag consumer migration

Status: implemented at source for S1/S2; S3 deferred. Baseline
`47cba73865a450218b99889cc61e392d7d4ba13c` (stacked on
`feat/bounded-session-delivery`). No compilation, focused test, client or server run was made.

This contract addresses review finding R06: the shared 1.12 projection kept 24 of 28 bank slots,
six of seven bank bags and seven of twelve item enchant slots. The donor projection stays exactly
as wide as it was; the missing state now lives in a typed owner, and the real bank/bag consumers
read it.

## 1. Why the donor projection cannot be widened

| State | 3.3.5 source | Donor field block | Conflict |
|---|---|---|---|
| bank slots | `PLAYER_FIELD_BANK_SLOT_1`, 56 dwords = 28 | donor `564..611` = 24 | donor 612 starts bank bags |
| bank bags | `PLAYER_FIELD_BANKBAG_SLOT_1`, 14 dwords = 7 | donor `612..623` = 6 | donor 624 starts buyback |
| item enchants | `ITEM_FIELD_ENCHANTMENT_1_1..12_3` = `22..57` | donor `22..42` = 7 | donor 43..47 are property seed / random properties / item text / durability / max |
| item played time | `ITEM_FIELD_CREATE_PLAYED_TIME` = 62 | none | donor item ends at 48 |

`ITEM_END` is 64 in 3.3.5 and 48 in 1.12. Writing Wrath 43..57 into donor 43..57 would overwrite
durability and the random-property columns, so the four/five extra values must live outside the
donor `ObjectFields`. An absent field is never written; the create-shaped donor store still reads
0 for it, so un-migrated readers keep their previous behaviour.

Reference anchors (read-only, pin `e67b6ffe`): `WoW/Common/ComponentCore/Types.h:100-117`
(28/7/32 bands), `WoW/Source/Ui/ContainerFrame_Scripts.cpp:115` (`GetContainerNumSlots(-1)=28`),
`WoW/Source/Ui/ContainerFrame.cpp:122` (11 total bags), `WoW/Source/Ui/
PaperDollInfoFrame.cpp:172-181` (`ResolveInventorySlotArg` bands 39..66 / 67..73 / 86..117),
`WoW/Source/Object/ObjectClient/Item_C.cpp:374-395` and `:1950-1962` (12 enchant slots).
Server anchors: `D:/WOW/Azerothcore_WoTLK_Rebuild/source/azerothcore/src/server/game/Entities/
Player/Player.h:697-717` and `.../Entities/Object/Updates/UpdateFields.h:66-81,329-332`.

## 2. Typed owner

`crates/benilla-app/src/wrath_inventory.rs`:

- `WrathFieldWindow { values, delivered }`: dense values plus one delivered bit per dword.
  `get(index)` returns `Some(0)` for a delivered zero and `None` for an absent field.
  `guid(lo)` requires both guid dwords, so a half pair is never assembled.
- `InventoryAddr`: `Equipment(0..18)`, `BagSlot(0..3)`, `Backpack(0..15)`, `Bank(0..27)`,
  `BankBag(0..6)`, `Buyback(0..11)`, `Keyring(0..31)`. `field_index()` names the 3.3.5 player
  dword; `to_wire()` names the command position; `lua_bag_id()` names the stock bag id.
- `WrathInventory { slots: BTreeMap<InventoryAddr, Option<u64>>, bank_bags_purchased, scope }`.
  A missing key is "never observed"; `Some(0)` is an explicit clear; `Some(guid)` is an item.
  `apply(window, scope)` returns only value changes, so a sparse digest cannot clear a slot it
  did not mention. `iter()` and `addr_of(guid)` serve the watcher and diagnostics.
- `PlayerInventory<'a> { store, wrath, wrath_target }`: the reader view. It prefers the typed
  owner, falls back to the donor `ObjectStore` for the shared bands, and exposes
  `bank_capacity()` (28/24), `bank_bag_capacity()` (7/6), `keyring_capacity()` (32/16) and the
  band bases.

One writer: `wrath_apply_world` calls `apply` for the self player's delivered window and stores
the resource. The transfer/terminal block clears it beside `BagCache`/`ItemLane`. No second
writer, no delta stream, no reordering relative to `wrath_net_drain`.

## 3. Wire and Lua mapping

3.3.5 direct player-array positions (internal slot, then Lua id = internal + 1):

| Band | internal | Lua id | command `(bag, slot)` |
|---|---|---|---|
| equipment | 0..18 | 1..19 | `(255, slot)` |
| equipped bags | 19..22 | 20..23 | `(19..22, inner)` |
| backpack | 23..38 | 24..39 | `(255, 23+inner)` |
| bank vault | 39..66 (28) | 40..67 | `(255, 39+inner)` |
| bank bags | 67..73 (7) | 68..74 | `(67+bag, inner)` |
| buyback | 74..85 | 75..86 | `(255, 74+inner)` |
| keyring | 86..117 (32) | 87..118 | `(255, 86+inner)` |

`ui_items::wire_pos_in_target(Wrath335, ...)` emits these directly, so the old
`wrath_item_pos` +4/+5 adapter was removed. Donor `Vanilla112` keeps its own bands.

**Residual runtime check:** the stock `BankButtonIDToInvSlotID` reconstruction
(`CGBankInfo_Scripts.cpp:20-44`) uses `button + 63` for bags, which contradicts the
`ResolveInventorySlotArg` band 67..73. This package uses 67..73 (the band the pickup/reader
functions accept) and keeps the mapping in `wire_pos_in_target`'s Wrath arm and
`slot_guid`'s `EQUIPMENT_BAG` arm. The exact installed FrameXML button band must be read on the
runtime machine before claiming the bank-bag button path visible.

## 4. Consumer migration

| Consumer | Before | After |
|---|---|---|
| `push_player_inventory_fields` | 24/6 dense copy | 24/6 delivered-only projection; typed owner gets 28/7 |
| `wrath_quest_slots`, visible items, player fields | dense `Vec<u32>` | presence-aware window, `get`/`word_or_zero` |
| `ui_items::feed` bank/containers | `BANK_SLOTS`/`BANK_BAGS` = 24/6 | `PlayerInventory::bank_capacity()`/`bank_bag_capacity()` = 28/7 |
| `ui_items::slot_guid` | donor `ObjectFields` | `PlayerInventory` (bank 28, bags 7, keyring 32) |
| `ui_items::walk_inventory` (shared walks) | donor | stays donor internally; `has_key` uses `has_key_in_player` |
| `has_key_in_target` | donor 24/6 | `has_key_in_player` with the typed owner |
| `ui_items::container_space` | donor 24/6 | typed 28/7 |
| `ui_char::bank_bag_slots` | 6 donor slots | 7 typed slots |
| `ui_merchant::repair_costs` | donor 24/6 | typed 28/7 |
| `ui_merchant::calls` buy-into-bag | donor bank-bag base 63 | Wrath base 67 |
| `ui_bank::purchase` | cap 6 | cap 7 in Wrath |
| `item_watch::BagCache` | 10 bags, donor seats | 11 bags; `seat_with`/`rescan_with` use typed seats in Wrath |
| `item_watch::slot_changed` | donor seat scan | typed `addr_of` for the bank tail |
| `benilla-ui::char_stats` | 6 bank bags, 24/6/16 bands | 7, 28/7/32 in Wrath via `Model::target` |
| `benilla-ui::container` | bank bags 5..10 | 5..11 |
| `benilla-ui::bank::GetNumBankSlots` | full at 6 | full at 7 in Wrath |
| `benilla-ui::cursor::bag_verbs` | 64..69 | 68..74 in Wrath, 64..69 donor |

**Intentionally left on the donor projection:** the shared `count_of`/`sum_of`/
`action_count`/`stack_counts`/`collect_inventory`/`find_item` walks, `find_item_in_target`'s
callers in the action bar/merchant slots, and the cursor sale/drop/probe `slot_guid_donor`
paths. They see the 24/6 donor bands and the 32-slot keyring, but not the four extra bank slots
or the seventh bank bag. A follow-up should thread `PlayerInventory` through the action bar,
tradeskill, auction and tooltip counts; it is not required for the bank window or the bank
commands.

## 5. Invariants

1. Presence: both guid dwords must be delivered; a half pair is not a guid.
2. Retain on omission: a sparse digest never clears an address it did not mention.
3. Explicit clear: a delivered zero pair is `Some(0)` and raises a change only if the old value
   was non-zero.
4. Full-snapshot replacement: `WorldDigest`/`WrathWorldView` stays latest-only; a dropped
   intermediate digest loses nothing.
5. Single writer: `wrath_apply_world`; the donor `ObjectStore` is a derived projection.
6. No donor widening: 24/6/7 stay; the typed owner carries 28/7.
7. Ownership: a bag move changes only the player address; contents stay on the container entity.
8. Reconnect/transfer: clear the owner in the same scope-loss block as `BagCache`/`ItemLane`;
   the new create repopulates it.
9. No inventory backlog: state rides the digest, not the session-reply queue.
10. Delayed template: a missing template changes no guid; `template_epoch` fills names/icons.

## 6. Tests (added, not run)

- `wrath_inventory::tests`: 28/7 capacity, absent vs delivered-zero, half guid, first empty
  observation, wire addresses, field indices, purchase byte, scope clear, `addr_of`.
- `wrath::tests::terminal_scope_loss_clears_ecs_and_input_without_a_digest` now seeds a bank
  address and asserts the owner is cleared with the scope.
- `ui_items::tests::wrath_wire_pos_covers_the_28_bank_slots_and_seven_bags`: bank 28, bag 11,
  keyring 32, and the rejection past each band.
- `item_watch::tests`: a typed bank slot without a record still signals/unlocks; bank bag 7
  raises `BAG_UPDATE(11)`.

## 7. Deferred S3

- 12 item enchant slots retained in a typed `WrathItemRow` (`Item_C.cpp` reads 12).
- `ITEM_FIELD_CREATE_PLAYED_TIME` (62).
- Item text by guid: `ItemTextByGuid` has no producer and `WrathItemText` no reader; a
  `MessageReader<WrathItemText>` consumer plus a UI `ItemTextByGuid` send is the next step.
- `RandomSuffix`, socket/gem, scaling and item-limit columns from the 2026-10-25 checkpoint.
- Raw player window length: `0..1171` stays; the always-zero `PLAYER_FIELD_MAX_LEVEL`,
  daily-quest and combat-rating tail is a separate defect (window should be `PLAYER_END` 1326).

## 8. Runtime acceptance (owner-side)

One owner-driven login on a character with an item in **bank slot 25** and a bag in **bank bag
slot 7** (7 bank-bag slots purchased). Open the bank and verify in one session:

1. `GetContainerNumSlots(-1)` = 28 and the bank frame has 28 rows;
2. the slot-25 item renders name/icon/stack and right-click withdraws to the backpack;
3. the 7th bank-bag button exists, `GetBagName(11)` is its name, `GetContainerNumSlots(11)` is
   its capacity; an item inside withdraws;
4. a backpack item deposits into slot 25;
5. `/reload` and reconnect; all four observations match the server.

If no owned character has seven purchased bank bags, the 28-slot checks stand alone and the
seventh-bag check is blocked on an owner-approved server-state action. This package does not
create/delete characters, grant slots or touch the DB.
