# Wrath auction classes: the 12340 class table and paired slot rows

Status: ready to merge
Owner: Claude Code (Opus 5.5)
Branch: fix/wrath-auction-classes
Issue: none
Next: source review, then the owner's batch build; run the 3 focused cases, then open Bagnon and the auction house's Browse tab in game

Matrix row F14 (stock UI and addon host). Source only: nothing here was compiled, no test was run
and nothing was seen in game.

## What was wrong

On a Wrath VM the host pushed 1.12's auction class table (`0x807060`, ten rows) and 1.12's
`GetAuctionInvTypes` shape. The 12340 reference (`WoW/Source/Ui/AuctionHouse_Scripts.cpp`, pin
`e67b6ffe`) differs in both:

- `s_auctionClasses` (`0x00A14F70`) has twelve rows: Weapon, Armor, Container, Consumable, Glyph,
  Trade Goods, Projectile, Quiver, Recipe, Gem, Miscellaneous, Quest. Reagent is gone. Every
  subclass flag is set except Quest's, so Consumable, Trade Goods and Miscellaneous offer their
  subclasses on 3.3.5a.
- `GetAuctionInvTypes` (`0x0059C590`) returns 28 values, each slot key followed by 1 or nil. The
  stock 3.3.5a Browse filter reads them in pairs (`AuctionFrameFilters_UpdateInvTypes`), so with 14
  keys it showed seven rows and passed the wrong pair index to `QueryAuctionItems`.

The visible failure this explains, from the in-world run of 12 Nov
(`benilla-config/Logs/Launcher/20261112-022858-228`, local only): Bagnon's `components/item.lua`
builds its quest search from `select(12, GetAuctionItemClasses())`. With ten names that is nil,
`string.format` raises at file load, the rest of the file never runs, `ItemSlot:GetDummyBag` is
never defined, and every bag slot errors at `item.lua:27` calling the missing method. The same position-indexed read is a recurring risk for any 3.3.5a addon, so the
table is fixed at the source rather than worked around in an addon.

## What changed

- `ui_auction/mod.rs`: `WRATH_AUCTION_CLASSES` (the twelve rows) beside the 1.12 table;
  `categories` takes the VM's `UiTarget` and picks the table. `feed_auction` passes
  `script.target()`. The addon-corpus harness passes `Vanilla112` explicitly (its tree is 1.12).
- `benilla-ui/src/script/auction.rs`: the slot rows became a small struct carrying 12340's two
  armor columns. On a Wrath VM `GetAuctionInvTypes` returns the key and 1/nil per row, with the
  1.12 gates unchanged (class flag, subclass `Flags & 0x200`, an exhausted scan skips the gate).
  Only Armor greys rows out, keyed on the subclass's menu position as the reference does:
  position 0 (Miscellaneous) offers head, neck, shirt, finger, trinket and held-in-off-hand only;
  any other position withholds neck, shirt, finger, trinket, back and held, except that position
  1 (Cloth) keeps back. The 1.12 VM still gets the 14 keys alone.
- `QueryAuctionItems` is unchanged: its fourth argument indexes the same 14 rows, which is the pair
  index the 3.3.5a filter passes.
- `GetAuctionItemSubClasses` needed nothing: host names were already verbose-first, as 12340's are.

## Focused cases to run (3)

- `benilla-ui`, `script::auction::tests::wrath_inv_types_pair_each_key_with_the_armor_rule` (new).
- `benilla-ui`, `script::auction::tests::subclass_and_inv_type_menus_follow_the_reference_gates`
  (unchanged; guards the 1.12 shape).
- `benilla-app`, `ui_auction::tests::a_wrath_vm_gets_the_twelve_12340_classes` (new; reads the
  install's DBCs, skips without one). `the_tree_carries_the_reference_gates` now passes
  `Vanilla112`; per the recorded donor-test caveat it may fail on the Wrath install regardless.

## Not done, not proven

- Not compiled. `UiScript::target()`, `Model::is_wrath()` and `UiScript::with_target` were checked
  by search only.
- In game: Bagnon's bags open without the `GetDummyBag` errors; the Browse tab lists twelve
  classes and greys the right slot rows under Armor.
- `QueryAuctionItems` on 3.3.5a takes a tenth argument, `getAll`; it is still ignored.
- Investigated and left alone (source reading did not settle them, or they belong elsewhere):
  - `MoneyFrame.lua:274` nil money, first seen 12 Nov 23:43:53 from `ui_script(input)`. Every host
    native that feeds a stock `MoneyFrame_Update` call reviewed here returns a number on both
    targets (`GetMoney`, merchant and buyback prices, bank slot cost, tooltip `OnTooltipAddMoney`,
    `GetRewardMoney`, `GetQuestMoneyToGet`, `GetQuestLogRewardMoney`,
    `GetQuestLogRequiredMoney`, `GetNextStableSlotCost`); `LFDFrame.lua:901` is guarded. Event
    payloads (`UIParent.lua:883` `CONFIRM_TALENT_WIPE` arg1, `:923` `GOSSIP_CONFIRM` arg3) and the
    StaticPopup callers were not traced. The Lua traceback from `fix/lua-error-traceback` should
    name the caller on the next run.
  - bartender4 `SimpleSticky.lua:179` (`GetCenter` nil), bartender4 `ActionButton.lua`,
    `MainMenuBar.lua:317/400`, `GameTime.lua:158`, `MiniMapTrackingButton:OnEnter`. Not examined
    against the host here. `MainMenuBar.lua:400` reads a global the stock file never assigns (the
    local is commented out), so it is not obviously a host fault.
  - `ChannelFrame.lua:716` and `VoiceChatSpeakerTemplate` are expected (owner decision).
