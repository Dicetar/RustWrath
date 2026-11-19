# Economy stock frames at app level: trade, mail and auction

Status: ready to merge
Owner: OC / DeepSeek
Branch: oc/economy-ui-coverage
Issue: 18
Next: CI compiles and runs the three new tests; then the owner checks the same windows in game.

## What changed
Added `crates/benilla-app/src/ui_script/economy_tests.rs` and registered it from
`ui_script/mod.rs`. The module loads the shipped chain XML through `test_ui`, feeds the
existing UiScript model and asserts the stock frames' own widgets:

- `shipped_trade_frame_paints_both_offers_and_the_accept_state` - `TradeFrame.xml` paints
  both slot 1s, the partner name, the coalesced recipient 5s and our 1g 23s 45c input; a
  `TRADE_ACCEPT_UPDATE(1, 1)` lights all four accept glows and disables Trade; the close
  button hides the window and queues `TradeCall::Close`.
- `shipped_mail_frame_opens_a_letter_with_its_attachments` - `MailFrame.xml` paints the
  inbox sender/subject, then the open letter's sender, subject, body, two attachment icons
  and the five-stack count; the third attachment button stays hidden.
- `shipped_auction_browse_frame_paints_and_selects_rows` - `Blizzard_AuctionUI.xml` paints
  two Browse rows' names, levels, current bids and row 1's buyout, hides the empty row 3,
  and after `SetSelectedAuctionItem("list", 2)` seats row 2's required 25s bid in the
  Browse bid input.

No production behavior changed. Fixtures use the existing model types (`TradeState`,
`MailState`, `AuctionState`) and the stock XML path helpers.

## CI run id and result
The three cases are data-gated (`wow_data_or_skip!`): CI with `WOW_DATA=""` compiles them but
they skip; the owner's install is needed to execute them. Not run yet on the branch. No local Cargo, client or server run. The changed .rs files were rustfmt'd
with the pinned toolchain's `rustfmt 1.9.0-stable`.

## What is not proven in game
The tests prove Lua-side paint and verb state against synthetic models, not a live server
session, actual mouse hover/click routing, textures on screen, or the real wire timing of
trade, mail and auction offers. Attachment layout, auction sorting beyond the two-row page,
and the frames' behaviour during a real mailbox/auction-house visit remain unproven.

