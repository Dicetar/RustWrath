//! The stock economy windows at app level: the trade, mail and auction frames loaded through the
//! app's UiScript, fed synthetic model state and asserted on what the stock Lua paints.
//!
//! Each case keeps the verbs' calls (no host) and reads the frame's own widgets, as the merchant,
//! bag and bank kits do.

use benilla_ui::script::{
    AuctionCategory, AuctionItemRow, AuctionListState, AuctionState, AuctionSubCategory,
    MailInboxAttachment, MailInboxRow, MailState, TradeCall, TradeSideState, TradeSlotItem,
    TradeState, UiScript, UnitState, LIST,
};

use super::test_ui::{load_ui as load_xml, production_order, MERCHANT_UI};

const TRADE_FILES: &[&str] = &[
    "Interface\\FrameXML\\MoneyInputFrame.lua",
    "Interface\\FrameXML\\MoneyInputFrame.xml",
    "Interface\\FrameXML\\TradeFrame.xml",
];

const MAIL_FILES: &[&str] = &[
    "Interface\\FrameXML\\MoneyInputFrame.lua",
    "Interface\\FrameXML\\MoneyInputFrame.xml",
    "Interface\\FrameXML\\UIDropDownMenu.xml",
    "Interface\\FrameXML\\FriendsFrame.xml",
    "Interface\\FrameXML\\MailFrame.xml",
    "ScrollTemplates.xml",
];

const AUCTION_FILES: &[&str] = &[
    "Interface\\FrameXML\\MoneyInputFrame.lua",
    "Interface\\FrameXML\\MoneyInputFrame.xml",
    "Interface\\FrameXML\\UIDropDownMenu.xml",
    "Interface\\FrameXML\\DressUpFrame.xml",
    "Interface\\AddOns\\Blizzard_AuctionUI\\Blizzard_AuctionUI.xml",
    "Interface\\AddOns\\Blizzard_AuctionUI\\Blizzard_AuctionDressUp.xml",
];

/// Load MERCHANT_UI's common prefix and the extra files in the production order.
fn load(s: &UiScript, extra: &[&str]) {
    for f in production_order(&[MERCHANT_UI, extra]) {
        load_xml(s, f);
    }
}

#[rustfmt::skip]
fn trade_item(item_id: u32, name: &str, count: u32, quality: u32) -> TradeSlotItem {
    TradeSlotItem { item_id, name: Some(name.into()), texture: Some("Interface\\Icons\\INV_Fabric_Linen_01".into()),
        count, quality: Some(quality), ..Default::default() }
}

#[rustfmt::skip]
fn trade_state() -> TradeState {
    let mut player = TradeSideState { gold: 12_345, ..Default::default() };
    player.slots[0] = Some(trade_item(2589, "Linen Cloth", 5, 1));
    let mut target = TradeSideState { gold: 500, ..Default::default() };
    target.slots[0] = Some(trade_item(4306, "Silk Cloth", 1, 2));
    TradeState { player, target }
}

#[rustfmt::skip]
fn attachment(index: u8, entry: u32, name: &str, texture: &str, count: u32) -> MailInboxAttachment {
    MailInboxAttachment { index, entry, count, name: Some(name.into()), texture: Some(texture.into()),
        quality: Some(1), ..Default::default() }
}

#[rustfmt::skip]
fn mail_row() -> MailInboxRow {
    MailInboxRow {
        mail_id: 7,
        attachments: vec![
            attachment(0, 2589, "Linen Cloth", "Interface\\Icons\\INV_Fabric_Linen_01", 5),
            attachment(1, 4306, "Silk Cloth", "Interface\\Icons\\INV_Fabric_Silk_01", 1),
        ],
        item_text_id: 1, stationery: 41, sender: Some("Thrall".into()),
        subject: "Warchief's orders".into(), days_left: 29.0, can_reply: true,
        body: Some("Dear friend, the ore is late.".into()),
        ..Default::default()
    }
}

#[rustfmt::skip]
fn auction_row(id: u32, name: &str, level: u32, min_bid: u32, buyout: u32, owner: &str) -> AuctionItemRow {
    AuctionItemRow { auction_id: id, item_id: 2589, name: Some(name.into()), count: 5, quality: Some(1),
        level, min_bid, buyout_price: buyout, owner: Some(owner.into()),
        // A day out on GetTime's clock, so the row's closing-time text is populated.
        deadline: 24 * 60 * 60 * 1000, ..Default::default() }
}

fn seat_player(s: &mut UiScript) {
    let player = UnitState {
        exists: true,
        name: Some("Firna".into()),
        level: 60,
        ..Default::default()
    };
    s.set_unit("player", Some(player));
}

/// The stock trade window: both columns paint their item and money, the accept event drives the
/// glows and the Trade button, and the close button queues the close.
#[test]
fn shipped_trade_frame_paints_both_offers_and_the_accept_state() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    load(&s, TRADE_FILES);
    seat_player(&mut s);
    let partner = UnitState {
        exists: true,
        name: Some("Thrall".into()),
        ..Default::default()
    };
    s.set_unit("npc", Some(partner));
    s.set_money(12_345);
    s.open_trade_window();
    s.set_trade(Some(trade_state()));
    s.fire_event("TRADE_SHOW", vec![]);
    assert!(s.errors().is_empty(), "trade show: {:?}", s.errors());
    let _ = s.take_trade_calls(); // the show's money reset offers 0

    // The window lands in the left panel; both slot 1s paint, slot 2 stays empty.
    assert_eq!(
        s.eval::<String>(
            "return GetLeftFrame():GetName()..'|'..TradePlayerItem1Name:GetText()..'|' \
             ..TradeRecipientItem1Name:GetText()..'|'..TradeFrameRecipientNameText:GetText()"
        )
        .unwrap(),
        "TradeFrame|Linen Cloth|Silk Cloth|Thrall"
    );
    assert!(s
        .eval::<bool>(
            "return TradePlayerItem1ItemButtonIconTexture:IsShown() \
             and not TradePlayerItem2ItemButtonIconTexture:IsShown()"
        )
        .unwrap());
    // The partner's 5s coalesces to the silver coin alone (TARGET_TRADE, MoneyFrame.lua:69).
    assert!(s
        .eval::<bool>(
            "return TradeRecipientMoneyFrameSilverButton:IsShown() \
             and not TradeRecipientMoneyFrameGoldButton:IsShown() \
             and not TradeRecipientMoneyFrameCopperButton:IsShown()"
        )
        .unwrap());
    // Our editable input fills from PLAYER_TRADE_MONEY, not from the getter on show.
    s.fire_event("PLAYER_TRADE_MONEY", vec![]);
    assert_eq!(
        s.eval::<String>(
            "return TradePlayerInputMoneyFrameGold:GetText()..'/' \
             ..TradePlayerInputMoneyFrameSilver:GetText()..'/' \
             ..TradePlayerInputMoneyFrameCopper:GetText()"
        )
        .unwrap(),
        "1/23/45",
        "our 1g 23s 45c fills the money input"
    );

    // Both accept: all four column glows show and the Trade button disables.
    s.fire_event(
        "TRADE_ACCEPT_UPDATE",
        vec![
            benilla_ui::script::ScriptValue::Int(1),
            benilla_ui::script::ScriptValue::Int(1),
        ],
    );
    assert!(s
        .eval::<bool>(
            "return TradeHighlightPlayer:IsShown() and TradeHighlightRecipient:IsShown() \
             and TradeHighlightPlayerEnchant:IsShown() and TradeHighlightRecipientEnchant:IsShown()"
        )
        .unwrap());
    assert!(
        !s.eval::<bool>("return TradeFrameTradeButton:IsEnabled() ~= 0")
            .unwrap(),
        "an accepted trade disables the Trade button"
    );
    s.run("TradeFrameCloseButton:Click()").unwrap();
    assert!(!s.eval::<bool>("return TradeFrame:IsShown()").unwrap());
    assert_eq!(
        s.take_trade_calls(),
        vec![TradeCall::Close],
        "the close button's OnHide queues CloseTrade"
    );
    assert!(s.errors().is_empty(), "trade close: {:?}", s.errors());
}

/// The stock mail window: MAIL_SHOW opens the inbox, a row click opens the letter, and the stock
/// frame paints the sender, subject, body and each attachment button.
#[test]
fn shipped_mail_frame_opens_a_letter_with_its_attachments() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(6.0)));
    load(&s, MAIL_FILES);
    s.set_mail(Some(MailState {
        inbox: vec![mail_row()],
        open: true,
    }));
    s.fire_event("MAIL_SHOW", vec![]);
    s.mail_request_resolved();
    s.fire_event("MAIL_INBOX_UPDATE", vec![]);
    assert!(s.errors().is_empty(), "mail show: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return tostring(not not MailFrame:IsShown())..'|'..MailItem1Sender:GetText()..'|' \
             ..MailItem1Subject:GetText()"
        )
        .unwrap(),
        "true|Thrall|Warchief's orders"
    );

    s.run("MailItem1Button:Click()").unwrap();
    assert!(s.errors().is_empty(), "open letter: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return tostring(not not OpenMailFrame:IsShown())..'|'..OpenMailSender:GetText()..'|' \
             ..OpenMailSubject:GetText()..'|'..OpenMailBodyText:GetText()"
        )
        .unwrap(),
        "true|Thrall|Warchief's orders|Dear friend, the ore is late."
    );

    // Two occupied attachments paint their icon and count, the third button hides.
    let (one, two, three, linen, silk, count, id) = s
        .eval::<(bool, bool, bool, String, String, String, i64)>(
            "return OpenMailAttachmentButton1:IsShown(), \
             OpenMailAttachmentButton2:IsShown(), OpenMailAttachmentButton3:IsShown() or false, \
             OpenMailAttachmentButton1IconTexture:GetTexture(), \
             OpenMailAttachmentButton2IconTexture:GetTexture(), \
             OpenMailAttachmentButton1Count:GetText(), OpenMailAttachmentButton1:GetID()",
        )
        .unwrap();
    assert!(one && two && !three, "two filled and one hidden attachment");
    assert_eq!(
        linen.to_ascii_lowercase(),
        "interface\\icons\\inv_fabric_linen_01"
    );
    assert_eq!(
        silk.to_ascii_lowercase(),
        "interface\\icons\\inv_fabric_silk_01"
    );
    assert_eq!(count, "5", "a stack of five shows its count");
    assert_eq!(id, 1);
    assert!(
        !s.eval::<bool>("return OpenMailAttachmentButton2Count:IsShown()")
            .unwrap(),
        "a single item shows no count"
    );
    assert!(s.errors().is_empty(), "mail open: {:?}", s.errors());
}

/// The stock auction Browse page: two fed rows paint their name, level, current-bid and buyout
/// columns; a selected row locks its highlight and seats the required bid.
#[test]
fn shipped_auction_browse_frame_paints_and_selects_rows() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(6.0)));
    // AuctionFrameBrowse_OnLoad reads GetAuctionItemClasses() before any session.
    s.set_auction_item_classes(vec![AuctionCategory {
        class_id: 4,
        name: "Armor".into(),
        has_subclass_filter: true,
        subclasses: vec![AuctionSubCategory {
            sub_id: 1,
            name: "Cloth".into(),
            has_inv_types: true,
        }],
    }]);
    load(&s, AUCTION_FILES);
    seat_player(&mut s);
    s.set_money(500_000);
    let mut lists: [AuctionListState; 3] = Default::default();
    lists[LIST] = AuctionListState {
        rows: vec![
            auction_row(1, "Linen Cloth", 10, 1000, 5000, "Seller"),
            auction_row(2, "Wool Cloth", 20, 2500, 0, "Someone"),
        ],
        total: 2,
        ..Default::default()
    };
    s.set_auction(AuctionState {
        lists,
        deposit_percent: 5,
    });
    s.fire_event("AUCTION_HOUSE_SHOW", vec![]);
    s.fire_event("AUCTION_ITEM_LIST_UPDATE", vec![]);
    assert!(s.errors().is_empty(), "browse show: {:?}", s.errors());

    let row = |n: &str| -> String {
        s.eval::<String>(&format!(
            "return tostring(not not AuctionFrame:IsShown())..'|' \
             ..tostring(not not AuctionFrameBrowse:IsShown())..'|' \
             ..BrowseButton{n}Name:GetText()..'|'..BrowseButton{n}Level:GetText()..'|' \
             ..BrowseButton{n}MoneyFrameSilverButton:GetText()..'|'..BrowseButton{n}HighBidder:GetText()..'|' \
             ..tostring(not not BrowseButton{n}BuyoutFrame:IsShown())"
        ))
        .unwrap()
    };
    assert_eq!(row("1"), "true|true|Linen Cloth|10|10|Seller|true");
    assert_eq!(row("2"), "true|true|Wool Cloth|20|25|Someone|false");
    assert!(
        s.eval::<bool>("return BrowseButton3:IsHidden()").unwrap(),
        "row 3 is past the batch"
    );

    // Select row 2: the stock update seats its required bid in the browse input.
    s.run("SetSelectedAuctionItem('list', 2)").unwrap();
    s.fire_event("AUCTION_ITEM_LIST_UPDATE", vec![]);
    s.resolve();
    assert_eq!(
        s.eval::<i64>("return GetSelectedAuctionItem('list')")
            .unwrap(),
        2
    );
    assert_eq!(
        s.eval::<i64>("return MoneyInputFrame_GetCopper(BrowseBidPrice)")
            .unwrap(),
        2500,
        "row 2's 25s opening price is the required bid"
    );
    assert!(s.errors().is_empty(), "browse select: {:?}", s.errors());
}
