# 2026-11-05 inventory-lossless (Worker A, package A)

## Identity

- Worktree: `F:\AI\tmp\opencode\rw-inventory-lossless`
- Branch: `feat/wrath-inventory-lossless`
- Stacked base: `47cba73865a450218b99889cc61e392d7d4ba13c` (`feat/bounded-session-delivery`, B)
- Pre-stack baseline: `46fd2f0c2af476b3659116f0fc8c95900bde95a0` (`origin/main`)
- Reference pin: `D:/Projects/wowdecomp-reference` `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`
- Server evidence read-only: `D:/WOW/Azerothcore_WoTLK_Rebuild/source/azerothcore`

## Source changes

New `crates/benilla-app/src/wrath_inventory.rs`:

- presence-aware `WrathFieldWindow` (values + delivered bits; half-guid refusal; delivered zero
  is an explicit clear);
- typed `InventoryAddr` over equipment, equipped bags, backpack, bank 0..27, bank bags 0..6,
  buyback and keyring 0..31, with field/wire/Lua mappings;
- `WrathInventory` resource, one writer (`wrath_apply_world`), scope identity;
- `PlayerInventory` reader view with donor fallback and Wrath capacities 28/7/32.

`crates/benilla-app/src/wrath.rs`:

- `WrathEntity.player_fields` and `WrathPlayerRaw` now carry the window; `build_digest` builds
  the delivered mask over 0..1171;
- `push_player_inventory_fields`, `wrath_quest_slots` and `push_visible_item_fields` read the
  window; the donor projection still writes 24 bank / 6 bag / 7 enchant fields only, and only
  for delivered dwords;
- `wrath_apply_world` applies the owner for the self player, clears it in the transfer block,
  feeds `item_watch` typed runs for bank slots 25..28 and bank bag 7, and seats `BagCache` with
  the typed 11-bag map;
- `wrath_item_pos`/`wrath_bank_bag` (+4/+5 adapter) deleted; commands use the 3.3.5 wire
  positions from `wire_pos_in_target`.

Consumer migrations: `ui_items` (feed containers, `slot_guid`, `container_space`, `has_key`,
walk), `ui_char` (7 bank-bag views), `ui_merchant` (repair costs, buy-into-bag), `ui_bank`
(purchase cap 7), `item_watch` (11-seat `BagCache`, typed bank runs, typed bank signal
detection), `ui_unit` (window accessor), `benilla-ui` `char_stats`/`container`/`bank`/
`cursor::bag_verbs` (target-aware 28/7/32 bands and `GetNumBankSlots` full at 7).

## What is deliberately still on the donor projection

`count_of`, `sum_of`, `action_count`, `stack_counts`, `collect_inventory`, `find_item` and the
cursor sale/drop/probe `slot_guid_donor` paths see the 24/6 donor bands (and the 32 keyring).
They do not see the four extra bank slots or the seventh bank bag. Threading `PlayerInventory`
through the action bar/tradeskill/auction/tooltip counts is the next consumer step; it is not
needed for the bank window or the bank commands.

## Tests added (NOT RUN)

- `wrath_inventory::tests`: capacity 28/7, absent vs cleared, half guid, first-empty, wire
  mapping, field indices, purchase byte, scope clear, `addr_of`.
- `wrath::tests::terminal_scope_loss_clears_ecs_and_input_without_a_digest`: seeds a bank slot
  and asserts the owner clears with the scope.
- `ui_items::tests::wrath_wire_pos_covers_the_28_bank_slots_and_seven_bags` and
  `wrath_find_item_returns_the_335_keyring_position`.
- `item_watch::tests::a_typed_bank_slot_without_a_record_still_signals` and
  `the_seventh_bank_bag_raises_bag_update_eleven`.
- `benilla-ui`: `bank_button_ids_follow_the_selected_contract`,
  `container_id_to_inventory_id_uses_the_335_step`,
  `the_seventh_bank_bag_resolves_for_the_wrath_verbs`,
  `the_seventh_bank_bag_is_a_swap_object_in_wrath`,
  `the_wrath_world_enter_updates_the_seventh_bank_bag`, and
  `inventory_slot_reader_bands_follow_the_target`.

## Verification actually performed

- Read-only source and reference inspection only.
- `git merge --ff-only feat/bounded-session-delivery`; HEAD verified.
- No `cargo check/build/test/clippy/fmt`, no client, no server, no DB, no game data.

## Deferred / gaps

- S3: 12 item enchant slots, `ITEM_FIELD_CREATE_PLAYED_TIME`, item text by guid
  (`ItemTextByGuid` has no producer, `WrathItemText` no reader), random suffix/socket/scaling.
- `player_fields` stays `0..1171`; `PLAYER_FIELD_MAX_LEVEL` (1279), daily quests (1280..1304)
  and combat ratings (1231..1255) read 0. Separate defect; do not bundle.
- Reference mapping resolved: `CGBankInfo_Scripts.cpp:20-44` (`button = input - 1; slot = button +
  63; push slot + 1`) and `ContainerFrame_Scripts.cpp:48-63` (`id - 1`, then `+64` above 4) both
  give Wrath Lua ids 68..74 (donor 64..69). `BankButtonIDToInvSlotID` and
  `ContainerIDToInventoryID` are target-aware; only an installed-FrameXML/visible pickup remains.
  The stock readers were also repaired: `GetContainerNumSlots`/`GetBagName` accept bag 11,
  `UseArm::Withdraw` and `cursor::swap::bag_object` cover 11, `bags_enter_world` fires
  `BAG_UPDATE(11)`, `SetBagPortraitTexture` covers 1..=11, and
  `inventory_slot_reader_accepts`/`bank_bag_shut` are target-aware.
- `target/click.rs::resolve_go_action` now uses `find_item_in_target` with the selected target,
  so a keyring key is sent at 86+ rather than 81+. Remaining `slot_guid_donor` users are cursor
  sale/drop, the worn-item use and dev probes; none can address the shifted bands.
- Runtime prerequisite: an owned character with seven purchased bank-bag slots and an item in
  bank slot 25. No server/DB change was made by this package.

## Runtime acceptance

One owner-driven login: bank slot 25 item visible/withdrawable, seventh bank bag visible and
withdrawable, deposit into slot 25, `/reload` and reconnect all match the server. See the RFC
§8 for the exact checks.
