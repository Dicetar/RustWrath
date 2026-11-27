//! The stock economy windows at app level: the trade, mail and auction frames loaded through the
//! app's UiScript, fed synthetic model state and asserted on what the stock Lua paints.
//!
//! Each case keeps the verbs' calls (no host) and reads the frame's own widgets, as the merchant,
//! bag and bank kits do.

use benilla_ui::script::{
    AuctionCall, AuctionCategory, AuctionHighBidder, AuctionItemRow, AuctionListState,
    AuctionState, AuctionSubCategory, ContainerSlot, ContainerState, MailCall, MailInboxAttachment,
    MailInboxRow, MailRowVerb, MailState, StationeryView, TradeCall, TradeSideState, TradeSlotItem,
    TradeState, UiScript, UiTarget, UnitState, BIDDER, LIST, OWNER,
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

/// The stock mail Send tab: switching to tab 2 opens `SendMailFrame`, clicking the attachment slot
/// locks and seats the picked-up bag item, and sending queues `MailCall::Send` with recipient,
/// subject, body, money, and attachment guid.
#[test]
fn shipped_mail_send_tab_attaches_and_sends_through_the_frame() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(6.0)));
    load(&s, MAIL_FILES);
    seat_player(&mut s);
    s.set_money(10_000);
    s.set_mail_stationeries(vec![StationeryView {
        id: 41,
        name: "Default Stationery".into(),
        icon: "Interface\\Icons\\INV_Misc_Note_01".into(),
        cost: Some(0),
        texture: "StationeryTest".into(),
    }]);
    s.set_container(
        0,
        Some(ContainerState {
            name: None,
            num_slots: 16,
            slots: [(
                1,
                ContainerSlot {
                    item_id: 2589,
                    guid: 0x5501,
                    count: 5,
                    quality: Some(1),
                    texture: Some("Interface\\Icons\\INV_Fabric_Linen_01".into()),
                    link: Some("|cffffffff|Hitem:2589|h[Linen Cloth]|h|r".into()),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        }),
    );
    s.set_mail(Some(MailState {
        inbox: vec![],
        open: true,
    }));
    s.fire_event("MAIL_SHOW", vec![]);
    s.mail_request_resolved();
    let _ = s.take_mail_calls();

    s.run(
        r#"
        MailFrameTab2:Click()
        PickupContainerItem(0, 1)
        ClickSendMailItemButton()
        SelectStationery(1)
        SendMailNameEditBox:SetText("Thrall")
        SendMailSubjectEditBox:SetText("Supplies")
        SendMailBodyEditBox:SetText("Five linen bolts.")
        SetSendMailMoney(350)
        SendMail("Thrall", "Supplies", "Five linen bolts.")
        "#,
    )
    .unwrap();
    assert!(s.errors().is_empty(), "mail send: {:?}", s.errors());
    assert!(
        s.eval::<bool>("return SendMailFrame:IsShown()").unwrap(),
        "tab 2 shows SendMailFrame"
    );
    let calls = s.take_mail_calls();
    assert!(
        calls.iter().any(|c| matches!(
            c,
            MailCall::Send(req)
                if req.target == "Thrall"
                    && req.subject == "Supplies"
                    && req.body == "Five linen bolts."
                    && req.money == 350
                    && req.item_guid == 0x5501
                    && req.stationery == 41
        )),
        "expected MailCall::Send with attached guid 0x5501, got: {calls:?}"
    );
}

/// The stock auction Bids and Auctions tabs: switching to tab 2 and tab 3 paints the fed `BIDDER`
/// and `OWNER` rows into `BidButton1` and `AuctionsButton1` with zero Lua errors.
#[test]
fn shipped_auction_bidder_and_owner_tabs_paint_their_lists() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(6.0)));
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
    let mut bid_row = auction_row(11, "Runecloth", 40, 4000, 9000, "VendorAlt");
    bid_row.bid_amount = 4200;
    bid_row.high_bidder = AuctionHighBidder::Player;
    bid_row.bidder_rank = 2;
    lists[BIDDER] = AuctionListState {
        rows: vec![bid_row],
        total: 1,
        ..Default::default()
    };
    let mut own_row = auction_row(12, "Mageweave Cloth", 30, 3000, 7500, "Firna");
    own_row.bid_amount = 3200;
    own_row.high_bidder = AuctionHighBidder::Name("Buyer".into());
    lists[OWNER] = AuctionListState {
        rows: vec![own_row],
        total: 1,
        ..Default::default()
    };
    s.set_auction(AuctionState {
        lists,
        deposit_percent: 5,
    });

    s.fire_event("AUCTION_HOUSE_SHOW", vec![]);
    s.run("AuctionFrameTab2:Click()").unwrap();
    s.fire_event("AUCTION_BIDDER_LIST_UPDATE", vec![]);
    assert!(s.errors().is_empty(), "bidder tab: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return tostring(not not AuctionFrameBid:IsShown())..'|'..BidButton1Name:GetText()"
        )
        .unwrap(),
        "true|Runecloth"
    );

    s.run("AuctionFrameTab3:Click()").unwrap();
    s.fire_event("AUCTION_OWNED_LIST_UPDATE", vec![]);
    assert!(s.errors().is_empty(), "owner tab: {:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return tostring(not not AuctionFrameAuctions:IsShown())..'|'..AuctionsButton1Name:GetText()"
        )
        .unwrap(),
        "true|Mageweave Cloth"
    );
}

/// MPQ-free Wrath app-level `ui_script` coverage for Trade, Mail (12 attachments + inline body +
/// send), and Auction (`list`/`bidder`/`owner` + bid/cancel calls).
#[test]
fn wrath_trade_mail_and_auction_ui_contracts_run_without_mpq() {
    let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
    s.keep_calls();
    seat_player(&mut s);
    s.set_money(100_000);

    // --- 1. Trade: offers, slot 7 enchant, money, accept/cancel/close calls ---
    let mut trade = trade_state();
    trade.target.slots[6] = Some(TradeSlotItem {
        item_id: 2589,
        name: Some("Enchanted Bracer".into()),
        texture: Some("Interface\\Icons\\INV_Bracer_03".into()),
        count: 1,
        quality: Some(2),
        enchantment: Some("Enchant Bracer - Minor Health".into()),
        ..Default::default()
    });
    s.open_trade_window();
    s.set_trade(Some(trade));
    let (p_name, t_name, t_ench, p_gold, t_gold): (String, String, String, i64, i64) = s
        .eval(
            "local pn = GetTradePlayerItemInfo(1)
             local tn = GetTradeTargetItemInfo(1)
             local _, _, _, _, _, ench = GetTradeTargetItemInfo(7)
             return pn, tn, ench, GetPlayerTradeMoney(), GetTargetTradeMoney()",
        )
        .unwrap();
    assert_eq!(p_name, "Linen Cloth");
    assert_eq!(t_name, "Silk Cloth");
    assert_eq!(t_ench, "Enchant Bracer - Minor Health");
    assert_eq!(p_gold, 12_345);
    assert_eq!(t_gold, 500);
    s.run("SetTradeMoney(2500) AcceptTrade() CancelTradeAccept() CloseTrade()")
        .unwrap();
    let trade_calls = s.take_trade_calls();
    assert!(trade_calls.contains(&TradeCall::Gold(2500)));
    assert!(trade_calls.contains(&TradeCall::Accept(true)));
    assert!(trade_calls.contains(&TradeCall::Accept(false)));
    assert!(trade_calls.contains(&TradeCall::Close));

    // --- 2. Mail: Wrath 12-attachment inbox row + inline body + take/send calls ---
    let attachments: Vec<MailInboxAttachment> = (0u8..12)
        .map(|idx| MailInboxAttachment {
            index: idx,
            guid_low: 0x9000 + u32::from(idx),
            entry: 2589 + u32::from(idx),
            count: u32::from(idx) + 1,
            name: Some(format!("Parcel {}", idx + 1)),
            texture: Some("Interface\\Icons\\INV_Misc_Bag_08".into()),
            quality: Some(2),
            ..Default::default()
        })
        .collect();
    s.set_mail_stationeries(vec![StationeryView {
        id: 41,
        name: "Default".into(),
        icon: "Interface\\Icons\\INV_Misc_Note_01".into(),
        cost: Some(0),
        texture: "StationeryTest".into(),
    }]);
    s.set_mail(Some(MailState {
        inbox: vec![MailInboxRow {
            mail_id: 77,
            attachments,
            stationery: 41,
            sender: Some("Sylvanas".into()),
            subject: "Twelve attachments".into(),
            money: 1500,
            days_left: 28.0,
            can_reply: true,
            body: Some("Inline Wrath body.".into()),
            inline_body: true,
            ..Default::default()
        }],
        open: true,
    }));
    let (num_items, item12_name, item12_count): (i64, String, i64) = s
        .eval(
            "local n = GetInboxNumItems()
             local name, _, count = GetInboxItem(1, 12)
             return n, name, count",
        )
        .unwrap();
    assert_eq!(
        (num_items, item12_name.as_str(), item12_count),
        (1, "Parcel 12", 12)
    );
    s.run("TakeInboxItem(1, 12)").unwrap();
    s.mail_request_resolved();
    s.run("TakeInboxMoney(1)").unwrap();
    s.mail_request_resolved();
    s.run(
        r#"
        SelectStationery(1)
        SetSendMailMoney(400)
        SendMail("Thrall", "Reply", "Received all twelve.")
        "#,
    )
    .unwrap();
    let mail_calls = s.take_mail_calls();
    assert!(mail_calls.contains(&MailCall::Row {
        verb: MailRowVerb::TakeItem,
        mail_id: 77,
        template_id: 0,
        item_guid_low: 0x9000 + 11,
    }));
    assert!(mail_calls.contains(&MailCall::Row {
        verb: MailRowVerb::TakeMoney,
        mail_id: 77,
        template_id: 0,
        item_guid_low: 0,
    }));
    assert!(mail_calls.iter().any(|c| matches!(
        c,
        MailCall::Send(req)
            if req.target == "Thrall"
                && req.subject == "Reply"
                && req.body == "Received all twelve."
                && req.money == 400
    )));

    // --- 3. Auction: list/bidder/owner getters + bid & cancel calls ---
    let mut lists: [AuctionListState; 3] = Default::default();
    lists[LIST] = AuctionListState {
        rows: vec![auction_row(
            101,
            "Frostweave Cloth",
            70,
            2000,
            6000,
            "Seller",
        )],
        total: 1,
        ..Default::default()
    };
    lists[BIDDER] = AuctionListState {
        rows: vec![auction_row(
            102,
            "Eternal Fire",
            80,
            15000,
            25000,
            "Alchemist",
        )],
        total: 1,
        ..Default::default()
    };
    lists[OWNER] = AuctionListState {
        rows: vec![auction_row(
            103,
            "Titansteel Bar",
            80,
            40000,
            55000,
            "Firna",
        )],
        total: 1,
        ..Default::default()
    };
    s.set_auction(AuctionState {
        lists,
        deposit_percent: 5,
    });
    let (n_list, n_bid, n_own, list_name, bid_name, own_name): (
        i64,
        i64,
        i64,
        String,
        String,
        String,
    ) = s
        .eval(
            "return (GetNumAuctionItems('list')), (GetNumAuctionItems('bidder')),
                    (GetNumAuctionItems('owner')), (GetAuctionItemInfo('list', 1)),
                    (GetAuctionItemInfo('bidder', 1)), (GetAuctionItemInfo('owner', 1))",
        )
        .unwrap();
    assert_eq!(
        (
            n_list,
            n_bid,
            n_own,
            list_name.as_str(),
            bid_name.as_str(),
            own_name.as_str()
        ),
        (
            1,
            1,
            1,
            "Frostweave Cloth",
            "Eternal Fire",
            "Titansteel Bar"
        )
    );
    s.run(
        "SetSelectedAuctionItem('list', 1)
         PlaceAuctionBid('list', 1, 6000)
         CancelAuction(1)",
    )
    .unwrap();
    let ah_calls = s.take_auction_calls();
    assert!(ah_calls
        .iter()
        .any(|c| matches!(c, AuctionCall::Bid(b) if b.auction_id == 101 && b.amount == 6000)));
    assert!(ah_calls.iter().any(|c| matches!(
        c,
        AuctionCall::Cancel {
            auction_id: 103,
            ..
        }
    )));
    assert!(s.errors().is_empty(), "wrath economy: {:?}", s.errors());
}
