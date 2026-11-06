# Lossless Wrath inventory: typed owner and bank/bag/item consumer migration

Status: S1/S2 implemented at source and S3 added at source (item instance payload, tooltip/bind/
visual/item-text consumers); no S3 compilation, focused test, client or server run. Baseline
`39e3c62a43f8967beb74c660fe3a76971d24924a` on `feat/wrath-item-payload` (stacked on the
S1/S2 branch, base `47cba73865a450218b99889cc61e392d7d4ba13c`). No compilation, focused test,
client or server run was made for S3.

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

**Reference mapping (resolved):** `CGBankInfo_Scripts.cpp:20-44` computes
`button = input - 1; slot = button + 63; push slot + 1`, so Wrath bag buttons 5..=11 resolve to
Lua ids **68..=74** (donor `button + 59` → 64..=69). `ContainerFrame_Scripts.cpp:48-63` computes
`id = input - 1; slot = id >= 4 ? id + 64 : id + 20`, the same 68..=74. Both `benilla-ui`
bindings are target-aware (`bank.rs::BankButtonIDToInvSlotID`, `container.rs::
ContainerIDToInventoryID`). The remaining runtime check is the installed FrameXML's button wiring
and a visible pickup, not a source ambiguity.

## 4. Consumer migration

| Consumer | Before | After |
|---|---|---|
| `push_player_inventory_fields` | 24/6 dense copy | 24/6 delivered-only projection; typed owner gets 28/7 |
| `wrath_quest_slots`, visible items, player fields | dense `Vec<u32>` | presence-aware window, `get`/`word_or_zero` |
| `ui_items::feed` bank/containers | `BANK_SLOTS`/`BANK_BAGS` = 24/6 | `PlayerInventory::bank_capacity()`/`bank_bag_capacity()` = 28/7 |
| `ui_items::slot_guid` | donor `ObjectFields` | `PlayerInventory` (bank 28, bags 7, keyring 32) |
| `ui_items::walk_inventory` (shared walks) | donor | `walk_inventory_in_player`; every `*_in_player` reader takes `&PlayerInventory`, donor wrappers keep `&ObjectFields` |
| `count_of` / `action_count` / `stack_counts` / `collect_inventory` / `find_item` | donor 24/6/16 | `*_in_player` variants; bank-scoped counts/collect see 28/7/32, mask-0 walks see the 32-slot keyring |
| `possession_count` (`GetItemCount`) | donor, hard-coded 32 keyring | typed view; bank-scoped count sees slot 25 and bank bag 7 |
| `PlayerInventory::typed` | value-or-donor | observed clear (`Some(0)`) wins; only a never-observed address falls back |
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

**Stock bindings repaired in the same pass:**

- `BankButtonIDToInvSlotID`: 3.3.5 bag step 63 (5→68, 11→74), donor 59 (5→64, 10→69).
- `ContainerIDToInventoryID`: 3.3.5 `+64` above 4 (5→68, 11→74), donor `+60`.
- `GetContainerNumSlots`: accepts bag 11 in Wrath; donor stops at 10.
- `GetBagName`: names bag 11 in Wrath; donor stops at 10.
- `UseArm::Withdraw`: bank bags 5..=11 in Wrath; 5..=10 donor.
- `SetBagPortaitTexture`/`SetBagPortraitTexture`: 1..=11 and `+64` in Wrath; 1..=10 / `+60` donor.
- `cursor::swap::bag_object`: bag 11 resolves at live id 74 in Wrath; donor stops at 10.
- `session::bags_enter_world`: fires `BAG_UPDATE(11)` in Wrath.
- `char_stats::inventory_slot_reader_accepts`: donor 40..=63 / 64..=69 / 82..=113; Wrath 40..=67 / 68..=74 / 87..=118.
- `container::bank_bag_shut`: donor 5..=10, Wrath 5..=11.

**Command-producing paths audited:** the key-at-object path (`target/click.rs::resolve_go_action`)
now calls `find_item_in_target` with the selected target, so a keyring key is sent at 86+ rather
than 81+. `ui_items`/`ui_merchant`/loot item-use producers already use `wire_pos_in_target`.
The remaining `slot_guid_donor` users are cursor sale (vendor, from the backpack/equipped bands),
cursor drop, the worn-item use, and probes; none can address the shifted bank/keyring bands, and
the probes are dev-only. `net/io.rs` is the donor writer and never receives Wrath commands.

**Read-only consumer migration (inventory-readers package):** the shared walks now take the
typed view through `*_in_player` variants. Migrated callers: action-bar state (`stack_counts`,
`find_item` equipment stage), action-bar identity and recount (`action_count`), item lane
(`count_of`), quest log (`count_of` at `QUEST_ITEMS`), tradeskill and craft recipe reagents/tools,
auction bid's unique count, mail stationery, spell tooltip reagent/totem lines and
`reagent_state`, shapeshift reagent stacks, lock openers, `GetItemCount`
(`possession_count`), death's `HasSoulstone` collect, and the pre-send cast gate
(`ui_action::errors::reagent_totem_refusal_in_player` for totem/reagent checks and
`spell::cast_send`'s `count_of_in_player` for ammo count, both mask 0).
`count_of`/`collect_inventory` at `QUEST_ITEMS` therefore reach bank slot 25, bank bag 7 and its
contents, and all of them reach keyring slots 17..32. `find_item`, `action_count`, `stack_counts`
and the pre-send cast gate keep the reference's mask 0: they reach the full 32-slot keyring but
deliberately do not enter the bank, as before.

**Still on the donor projection:** the command producers `target/click.rs`, `target/lock.rs`,
`ui_action/drain.rs` and `ui_petition/steps.rs` (positions and wire bytes must not change),
`items::disarmed_equipment_slot` (the equipment band 0-18 is identical in both projections), the
cursor sale/drop `slot_guid_donor` paths, and the dev capture probes.

A follow-up should decide whether any command producer needs the typed view for its own read
(cursor sale/drop and key-at-object ladder). It is not required for the bank window or the bank
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
11. Read-path precedence: an observed clear (`Some(0)`) is a value and never falls back to the
    donor projection; only a never-observed address does.

## 6. Tests (added, not run)

- `wrath_inventory::tests`: 28/7 capacity, absent vs delivered-zero, half guid, first empty
  observation, wire addresses, field indices, purchase byte, scope clear, `addr_of`.
- `wrath::tests::terminal_scope_loss_clears_ecs_and_input_without_a_digest` now seeds a bank
  address and asserts the owner is cleared with the scope.
- `ui_items::tests::wrath_wire_pos_covers_the_28_bank_slots_and_seven_bags`: bank 28, bag 11,
  keyring 32, and the rejection past each band.
- `ui_items::tests::wrath_find_item_returns_the_335_keyring_position`: the key-at-object walk
  returns `(255, 86)` for a donor keyring key.
- `ui_items::wrath_read_tests` (added, not run): the quest count/collect walk reaches bank slot
  25, bank bag 7 and its contents plus keyring slot 32; action/stack counts reach keyring slot 32;
  `find_item` reaches keyring slot 32 and keeps the mask-0 bank exclusion; `GetItemCount` sees the
  bank tail only with its bank flag; donor fallback stays 24/6/16 and never invents a tail value;
  a typed clear does not resurrect a donor guid while a never-observed address still falls back.
- `ui_tradeskill::tests::a_recipe_counts_a_worn_copy_and_the_keyring_but_not_the_bank` and
  `ui_tooltip::spell_feed::tests::a_reagent_line_reaches_the_wrath_keyring_tail` (added, not
  run): the migrated recipe and spell-tooltip callers reach typed keyring slots 30..32.
- `item_watch::tests`: a typed bank slot without a record still signals/unlocks; bank bag 7
  raises `BAG_UPDATE(11)`.
- `benilla-ui` tests: `bank_button_ids_follow_the_selected_contract` (68/74 Wrath, 64/69 donor),
  `container_id_to_inventory_id_uses_the_335_step` (5→68, 11→74),
  `the_seventh_bank_bag_resolves_for_the_wrath_verbs` (`GetContainerNumSlots(11)`,
  `GetBagName(11)`, withdraw call), `the_seventh_bank_bag_is_a_swap_object_in_wrath`,
  `the_wrath_world_enter_updates_the_seventh_bank_bag`, and
  `inventory_slot_reader_bands_follow_the_target` (donor/Wrath whitelists).

## 7. S3 item payload (source-implemented, not run)

S3 closes the item half of the same loss. The item/container digest now carries a
`WrathFieldWindow` over `ITEM_END` (64) for items and `CONTAINER_END` (138) for containers, with
the reducer's delivered mask; the old dense `(0..138).map(unwrap_or(0))` is gone.

**Typed row.** `crates/benilla-app/src/wrath_item.rs` defines a `WrathItemRow` component beside
`ObjectStore` on the existing item/container entity:

- `enchant(slot 0..11)` → `WrathEnchant { id, duration, charges }`, each `Option` per delivered
  dword (`22 + 3n .. 24 + 3n`); `enchant_id/duration/charges`.
- `property_seed` (58), `random_properties_id` (59), `durability` (60), `max_durability` (61),
  `create_played_time` (62).
- `stack_count` (14), `duration` (15), `spell_charges 0..4` (16..20), `flags` (21),
  `owner/contained/creator/gift_creator` guid pairs (6..13), `container_num_slots` (64) and the
  36 container slot pairs (66..137).
- `ItemRead<'a>` is the donor fallback view: a delivered row field wins (including a delivered
  zero), and an undelivered field falls back to the translated donor `ObjectFields`. An absent row
  is exactly today's seven-slot donor read.

**Projection.** `push_item_fields` still writes only the 1.12 layout: donor 6..21, donor 22..42
(Wrath enchant slots 1..7), and donor 43,44,46,47 from Wrath 58..61. Wrath 43..57 (slots 8..12)
and 62 are never written into a donor index; donor 45 stays the unwritten 1.12 item-text id.
Container 64/66..137 still projects to donor 48/50.., on the same container entity. Container
identity, generation and slot ownership are unchanged.

**Consumers migrated** (all read `ItemRead` where the row is authoritative):
`ui_items::feed::resolve_slot` and `resolve_slot_in_target` (tooltip text/name/link/durability/
charges/readable/creator/random properties), `ui_items::object_name/link/link_in_target`
(constructors now have `_item` forms and donor wrappers), `ui_char::inventory_slot` and
`weapon_enchant`, `ui_bind_confirm::use_binds`, `spell::targeting::item::gate_item`,
`entities::item_glow`/`equipment::resolve` (12-slot fold and glow for our own equipped item; the
broadcast visible-item block remains the other-player fallback), `ui_merchant` buyback/repair, and
`spell::usable::worn_item_cached`. `items::already_bound_item`/`live_enchant_item` walk all twelve
slots when a row exists, so donor 43..47 are never read as Wrath enchant slots 8..12.

**Intentionally still donor / unsupported:**
- `item_watch::kin`/`slot_count` and the shared read-only `count_of`/`sum_of`/`action_count`/
  `stack_counts`/`find_item`/`collect_inventory` walks (Worker B's surface).
- `item_watch`'s donor 22..42 enchant notice remains the donor path; the new row-based notice
  covers exactly 43..57, so no change is noticed twice.
- Socket/gem, `RandomSuffix` factor, scaling and item-limit columns are not in the 12340 item
  query response or item descriptor used here. Names are preserved from the profile/checkpoint
  as unsupported; no field is invented.
- The seven `Countdowns` enchant cells (`SMSG_ITEM_ENCHANT_TIME_UPDATE`) are the 1.12 array; a
  Wrath slot 8..12 has a delivered row `duration` but no countdown cell in this pass.
- `create_played_time` is stored in the row and has no consumer; stated, not fabricated.

**Item text by guid (INV-15).** `ui_item_text` now owns `WrathItemTexts`, a cache keyed by item
guid with `Text`/`NoText` entries and an in-flight guid FIFO (at most one request per
guid). It reads
`MessageReader<WrathItemText>` on the native message edge (not the session-reply queue), filters
by `(session_epoch, world_generation)`, and retires entries and in-flight addresses on scope
change and on `OnExit(InWorld)`. `ClientCommand::ItemTextByGuid` is sent once per guid; the
absent `has_text == false` form names no guid, so it retires the oldest in-flight request as a
clean no-text result. `ReadSource::ItemText` uses the 12340 `ITEM_FIELD_FLAGS & 0x200` readable
bit (`CGItemText::SetItem`, `Item_C.cpp`), which also feeds the bag-read for
`dispatch_fork`. `begin_item_text` closes on `NoText` without a frame; `feed_item_text` shows the
body and the item creator line for `Text`. A mail letter on Wrath takes the same by-guid path;
1.12 keeps `ITEM_FIELD_ITEM_TEXT_ID` and `MailOpen`.

**Tests added, not run:** `wrath_item::tests` (absent vs delivered zero, twelve slots, create
played time, container ownership of the slot array, donor fallback, delivered-zero-wins,
random-property fallback/precedence, `0x200` readable, the 43..57 change band); `wrath::tests`
(donor 43..47 not overwritten, absent field not written, container projection, container
identity/generation with the row); `items::tests` (a binding enchant in Wrath slot 8 through the
typed row); `ui_items::drain::tests` (`0x200` forks to the reader); `ui_item_text::tests`
(present/absent/scope, by-guid read and no-text refusal). See
`.scratch/project-b/handoffs/2026-11-05-item-payload.md` for the exact unrun list.

## 8. Runtime acceptance (owner-side for S1/S2)

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

S3 acceptance (source-only until run): on a Wrath character, hover a bag item that has enchants
outside slots 1..7 and a random suffix; compare the tooltip enchant lines, random-suffix name,
durability and charges with the item link and the paper doll. Then right-click a readable item
(3.3.5 `ITEM_FIELD_FLAGS & 0x200`): the stock `ItemTextFrame` shows the body or closes without
error, and a second read closes it. Reconnect/world-transfer and repeat to show the text cache
retired rather than serving the old world's body.
