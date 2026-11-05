# 2026-11-05 inventory-readers (Worker B)

## Identity

- Worktree: `F:\AI\tmp\opencode\rw-inventory-readers`
- Branch: `feat/wrath-inventory-readers`
- Base: `39e3c62a43f8967beb74c660fe3a76971d24924a`
  (`integration/release-p0-inventory-session-2026-11-05`, includes R04/R06)
- Reference: `D:/Projects/wowdecomp-reference`, pin
  `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (read-only)
- Peer boundary: `item-payload` (Worker A) owns item instance payload,
  tooltip/bind/visual/enchant/item-text. This package owns read-only
  walk/count/find and their callers. No overlapping file was edited by both.

## Read-only plumbing

`crates/benilla-app/src/ui_items/mod.rs` now has one read path for the listed
walks:

- `walk_inventory_in_player(&PlayerInventory, objects, scope, raw)` is the walk.
  It uses the view's `keyring_capacity()` and the donor whole-item gate.
- New `*_in_player` readers: `count_of_in_player`, `sum_in_player` (private),
  `action_count_in_player`, `stack_counts_in_player`, `find_item_in_player`,
  `collect_inventory_in_player`; `possession_count` now takes the view.
- Existing `&ObjectFields` entry points (`count_of`, `action_count`,
  `stack_counts`, `find_item`, `find_item_in_target`, `collect_inventory`)
  remain as donor wrappers over `PlayerInventory::new(store, None, false)` or,
  for the `_in_target` variants, a donor view with the target's capacities.
  Donor 24/6/16 and every existing caller/test keep the old answer.
- `wrath_inventory::PlayerInventory::typed` now reads `WrathInventory::presence`:
  an observed `Some(0)` clear returns 0 and stops there; only a never-observed
  address falls through to the donor projection. The donor accessors are
  bounds-checked at 24/6/32, so an absent bank slot 25/bag 6 cannot read an
  occupied neighbour.
- No second inventory authority and no reader mutation: each migrated system
  adds `Option<Res<WrathInventory>>` and builds
  `PlayerInventory::new(store, owner, script.target() == UiTarget::Wrath335)`.
  Event handlers with no VM use `crate::wrath::selected_from_env()`.

## Migrated callers (read-only)

- `ui_action/state.rs`: per-pass `stack_counts` for reagent/totem usability and
  the `find_item` equipment-only "already worn" stage.
- `ui_action/feed.rs`: item-slot display count (`action_count`).
- `ui_action/recount.rs`: `Counter` (action-bar recount on item notices).
- `item_lane.rs`: `carried`/`rebuild_lists` and the trade recount counter.
- `ui_quest_log.rs`: `ItemBook::held` (`SMSG_QUESTUPDATE_ADD_ITEM`) and
  `feed_quest_log`'s objective counts (`InventoryScope::QUEST_ITEMS`).
- `ui_tradeskill.rs`: reagent and totem counts in `resolve_recipe`.
- `ui_craft.rs`: reagent and totem counts in the craft list.
- `ui_auction/verbs.rs`: `PlaceAuctionBid`'s carried-count (mask `0x4f`).
- `ui_mail/mod.rs` + `ui_mail/net.rs`: stationery carried test (mail list and
  feed).
- `ui_tooltip/spell_feed.rs`: spell reagent/totem `find_item` red line and
  `reagent_state` counts (`ViewCtx` now carries owner + target).
- `ui_shapeshift.rs`: reagent stacks for each form's castable leg.
- `ui_items/lock_openers.rs`: held keys/opener set.
- `ui_items/calls.rs`: `ItemSteps::item_count` -> `possession_count`, i.e.
  `script_calls`' `GetItemCount`.
- `death/mod.rs`: `HasSoulstone`/`UseSoulstone` `collect_inventory` over the
  default mask (shared bands; bank is not in that mask).

## Intentionally donor-only

- `ui_action/errors.rs` (`first_missing_totem`, `first_short_reagent`) and
  `spell/cast_send.rs`'s ammo `count_of`: pre-send command gates at mask 0; a
  keyring cannot hold a reagent/ammo and changing the gate would change command
  admission.
- `target/click.rs`, `target/lock.rs`, `ui_action/drain.rs`: command producers
  (`find_item[_in_target]`). Command positions/wire bytes are untouched.
- `ui_petition/steps.rs` `TurnIn`: command producer (charter guid to CMSG).
- `items::disarmed_equipment_slot`: the equipment band 0-18 is byte-identical
  in the donor projection and typed owner; the 4/1 tail is bank-side.
- `capture/probe_charter.rs`: dev capture probe, not gameplay.
- Cursor sale/drop `slot_guid_donor` paths: owned by the previous R06 package;
  no location in this package changed them.

## Sparse / clear / ownership

- `Some(0)` from the owner is a clear; a missing map entry is unknown. The
  donor fallback runs only for unknown.
- A donor value is never resurrected over an observed clear.
- The writer remains `wrath_apply_world`; readers only borrow.
- Wrath capacities apply only when the selected target is `Wrath335` (or the
  envelope selects Wrath); donor sessions stay 24/6/16.
- `find_item`, `action_count` and `stack_counts` keep the reference's mask 0 and
  therefore do not enter the bank, even in Wrath. Bank slot 25 / bank bag 7 are
  visible through the bank-inclusive scopes (`count_of`/`collect_inventory` at
  `QUEST_ITEMS`, `possession_count(..., bank=true)`) and through keyring 17..32
  everywhere mask 0 reaches the keyring. This is the source-faithful boundary,
  not a missing migration.

## Tests added (NOT RUN)

`ui_items::wrath_read_tests`:

- `the_quest_count_and_collect_walk_reach_the_wrath_tail` (bank slot 25, bank
  bag 7 and its contents, keyring slot 32; wire positions 64/73/117).
- `the_action_and_stack_counts_include_the_thirty_two_slot_keyring`.
- `find_item_reaches_keyring_slot_thirty_two_not_the_bank`.
- `the_donor_fallback_keeps_24_6_16_and_absent_tail_addresses_read_zero`.
- `the_script_item_count_reaches_the_wrath_bank_and_keyring_tail` (bank flag
  on/off).
- `a_typed_clear_does_not_resurrect_a_donor_value`.

Existing caller tests were updated for the new signatures only (death self-res,
tradeskill `resolve_recipe`, mail `stationeries`, tooltip `reagent_state`/`ViewCtx`).

## Verification actually performed

- Source inspection only, including reference/source layout checks.
- No `cargo build/check/test/clippy/fmt`, no type check, client, server, DB,
  live session, or runtime observation.
- `git status` was clean at the base before reading; the base SHA was verified.
- No command positions, opcodes or wire bytes were changed.

## Pending / risks

- Compilation is unverified; a type/lifetime error in any of the new
  system-parameter threads would surface only at build time.
- Action-bar/recount tests exercise the count helpers, not the live Bevy pass.
- `find_item`'s bank exclusion is intentional (see above); if an owner wants a
  bank-addressable first-hit search, that is a new behavior, not this package.
- The pre-send cast gates remain donor; a Wrath-only reagent in keyring slots
  17..32 would not satisfy them (no such reagent exists in the checked data).
- Runtime acceptance still requires the owner's character.

## Runtime acceptance (one owner-driven login)

On a Wrath character with an item in bank slot 25, an item in the seventh bank
bag, and a key in keyring slot 17 or later (owner supplies all three, no DB
change by this package):

1. Open the bank: `GetContainerNumSlots(-1)` is 28 and the slot-25 item shows
   its name/stack.
2. Open bank bag 7 (`GetContainerNumSlots(11)`): its item shows and can be
   withdrawn.
3. Open the tradeskill/craft window: a reagent held only in bank slot 25 does
   not count (mask 0), while a reagent/keyring item in keyring slots 17..32 is
   visible wherever the reference's mask 0 already includes the keyring.
4. Hover a spell whose reagent is held in bank slot 25: the reagent line is red
   (mask 0), matching the reference; a key held at keyring 17+ is usable by the
   lock-opener path after the keyring widening.
5. `/reload` and reconnect: the counts above are rebuilt from the new digest,
   not from a stale donor value.

No compilation, test, or runtime success is claimed by this handoff.
