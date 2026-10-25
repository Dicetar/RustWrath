# OpenCode checkpoint - item/container replication and inventory ownership

Date: 2026-10-25
Owner: OpenCode (gameplay/integration)
Checkout: `wotlk`, base HEAD `5a55d3d3`; this checkpoint's commit adds the item slice.
Scope: source-only completion of item/container object replication, item-template fields and
inventory ownership for the pinned quest 6 scenario. No build, test, renderer, WM, socket,
database or live-server execution.

## Published ownership

Owned by this slice:

- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- this checkpoint and the acceptance-scenario update

Not touched by this slice:

- `crates/benilla-procgen` (GLM-owned; its current dirty `dungeon.rs`/`spawns.rs` work is not
  part of this commit)
- existing inventory owners (`items.rs`, `ui_items`, `item_watch`, `item_lane`, `ui_quest_log`)
  and unrelated bridge/catalog files

## Source changes

`crates/benilla-app/src/wrath.rs`:

- `WrathEntity.item_fields` carries the raw 3.3.5 item/container field window (`0..138`).
- `apply_item_world` creates or updates an established `ItemObject` + `ObjectStore` in
  `GuidIndex`, with `WrathStreamed` and generation-safe `WrathGeneration`. Items deliberately
  have no `NetEntity` or transform.
- `push_player_inventory_fields` maps the player bands: equipment/equipped bags, backpack,
  24 of 28 bank slots, six of seven bank bags, buyback, keyring and farsight.
- `push_item_fields` maps item fields 6..=21 unchanged, the first seven 3.3.5 enchant slots to
  the 1.12 array, and property seed/random property/durability/max durability to 43/44/46/47;
  container slot count and 36 slot GUIDs map to 48 and 50..=121.
- World transfer clears `BagCache` and `ItemLane` in addition to the existing streamed-scope
  teardown, so stale item watcher state cannot survive a map change.
- Item and container `OBJECT_FIELD_TYPE` masks are corrected to `0x3` and `0x7`.

`crates/benilla-protocol-wrath/src/gameplay.rs`:

- `ItemInfo` now carries the complete stable 1.12 template shape: stats, damage, armor and
  resistances, delay/ammo/ranged, five spell blocks, first ON_USE spell, bonding, description,
  page/language/material, start quest, lock, material/sheath, random property, block, item set,
  durability, area, map and bag family.
- `read_item_info` parses the whole 3.3.5 response through `BagFamily`, including the
  count-prefixed stat block and the fixed five damage/resistance/spell blocks.

## Inventory ownership expectation

The item entity is the same shape the stock path uses: a `Guid` in `GuidIndex`, an
`ObjectStore` with `OBJECT_FIELD_ENTRY`/stack/enchant/durability fields, and `ItemObject`.
`Inventory`/`count_of` therefore walks the player's mapped slots and resolves the item GUID
through the existing object lookup.

For the pinned scenario the source expectation is:

1. Garrick's Head item `182` creates an item entity and a player backpack slot references its
   GUID.
2. `ItemObject`/`ObjectStore` feed `count_of(..., 182, QUEST_ITEMS)` after the item update, so
   quest 6's held count reaches 1 and the quest log refreshes.
3. Reward choice 0 (`6076`, Tapered Pants) follows the same create/update path and becomes the
   item referenced by the reward slot.
4. Item template `182` and `6076` answers now populate the 1.12 fields the tooltip, icon,
   equip and quest surfaces read.

This is a source-path expectation only; it has not been exercised against a live character or
rendered client.

## Explicit translation defaults and omissions

- 3.3.5 has 12 enchant slots; the 1.12 item block has seven. Slots 8-12 are dropped.
- 1.12 `ITEM_FIELD_ITEM_TEXT_ID` (field 45) has no 3.3.5 counterpart; it stays 0 rather than
  copying a 3.3.5 enchant/played-time value.
- 3.3.5 `ITEM_FIELD_CREATE_PLAYED_TIME` (62), the item pad (63) and the container pad (65) are
  dropped.
- `RandomSuffix` is read and discarded: the 1.12 `ItemInfo` has only `random_property`, so the
  suffix cannot be represented faithfully. `RandomProperty` is carried.
- 3.3.5-only columns (flags2, scaling, sockets/gems, disenchant skill, duration, item-limit
  category, holiday) are not represented by the 1.12 protocol and are not translated.
- The bank is narrowed from 28 to 24 slots and bank bags from seven to six; the extra tail is
  dropped rather than folded into the next band.
- `SMSG_ITEM_TIME_UPDATE`/enchant-time feeds are not synthesized by this slice; new items get
  default countdowns.

## Verification state

- `git diff --check` is clean for the owned files.
- No build, test, formatter, renderer, WM, socket, database or live-server run was made after
  the item slice. Current compile/test status is UNKNOWN.
- Earlier checks reported `cargo check -p benilla-app -p benilla-protocol-wrath` passed,
  protocol gameplay tests 7 passed, and command-tap 1 passed, but they predate the adapter
  repairs and this slice. They are not current evidence.
- Graphical/live acceptance remains NOT RUN and UNKNOWN. The full stock-client replacement is
  still the release gate.

## Next slice

Event-generation rejection and reconnect lifecycle semantics are separate from inventory
ownership and are not implemented here. Do not fold them into this commit.
