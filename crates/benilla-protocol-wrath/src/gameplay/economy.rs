//! 12340 economy replies: bank, trade, mail and auction wire records.
//!
//! Every layout here is the pinned AzerothCore build-12340 writer named beside the reader, cross
//! checked against the pinned 3.3.5a client decomp (`Trade_C.cpp`, `MailInfo.cpp`,
//! `AuctionHouse.cpp`). This module owns no game state: it validates one packet body and returns
//! typed rows for the app's existing economy owners. Counts the client cannot store and slot
//! bytes the client indexes with are rejected rather than clamped, and a mail row's declared
//! size is checked against the bytes actually read instead of being used to skip.

use super::{DecodeError, Reader};
use anyhow::Result;

/// `TRADE_SLOT_COUNT` (`TradeHandler.cpp:79-80`) and the client's two seven-entry windows
/// (`Trade_C.cpp:370-412`).
pub const TRADE_SLOT_COUNT: usize = 7;

/// `MAX_INBOX_CLIENT_CAPACITY` (`MailHandler.cpp:35`): the most rows the client inbox holds.
pub const MAX_INBOX_CLIENT_CAPACITY: usize = 50;

/// `MAX_MAIL_ITEMS` (`Mail.h:33`): the most attachments one mail may carry.
pub const MAX_MAIL_ITEMS: usize = 12;

/// `MAX_AUCTIONS_PER_PAGE` (`AuctionHouseMgr.h:36`): one browse page.
pub const MAX_AUCTIONS_PER_PAGE: u32 = 50;

/// Conservative bound for the owner and bidder lists, which the searcher does not page
/// (`AuctionHouseSearcher.cpp:205-274`). A browse `getAll` can reach 55000 rows
/// (`MAX_GETALL_RETURN`), so that mode still needs a separate bound when it is wired.
pub const MAX_OWNED_AUCTIONS: u32 = 1000;

/// `SMSG_SHOW_BANK`: `WorldPackets::Bank::ShowBank::Write` (`BankPackets.cpp:44-49`) sends the
/// raw 64-bit banker guid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowBank {
    pub banker: u64,
}

pub fn read_show_bank(body: &[u8]) -> Result<ShowBank> {
    let mut r = Reader::new(body);
    let banker = r.u64()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(ShowBank { banker })
}

/// `SMSG_BUY_BANK_SLOT_RESULT`: `BuyBankSlotResult::Write` (`BankPackets.cpp:37-42`) sends one
/// `u32` result code.
pub fn read_buy_bank_slot_result(body: &[u8]) -> Result<u32> {
    let mut r = Reader::new(body);
    let result = r.u32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(result)
}

/// `SMSG_TRADE_STATUS` statuses, numbered by `SharedDefines.h:3808-3831` and the client's
/// `TradeFrame.h:8-31`. 13 is unassigned in 12340 and falls into [`TradeStatus::Unknown`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeStatus {
    Busy,
    BeginTrade { partner: u64 },
    OpenWindow { trade_id: u32 },
    Canceled,
    Accept,
    Busy2,
    NoTarget,
    BackToTrade,
    Complete,
    Rejected,
    TargetTooFar,
    WrongFaction,
    CloseWindow {
        result: u32,
        is_target_error: bool,
        item_limit_category: u32,
    },
    IgnoreYou,
    YouStunned,
    TargetStunned,
    YouDead,
    TargetDead,
    YouLogout,
    TargetLogout,
    TrialAccount,
    WrongRealm { slot: u8 },
    NotOnTapList { slot: u8 },
    Unknown(u32),
}

/// `SMSG_TRADE_STATUS`: `WorldSession::SendTradeStatus` (`TradeHandler.cpp:33-60`) writes the
/// status word, then exactly one conditional tail. The client reads the same tails
/// (`Trade_C.cpp:255-346`): 1 a raw `u64` trader guid, 2 a `u32` trade id, 12 the inventory
/// result, the target-error byte and the item-limit category, 22/23 one slot byte. Everything
/// else has no tail, so any remaining byte is malformed.
pub fn read_trade_status(body: &[u8]) -> Result<TradeStatus> {
    let mut r = Reader::new(body);
    let status = r.u32()?;
    let decoded = match status {
        0 => TradeStatus::Busy,
        1 => TradeStatus::BeginTrade {
            partner: r.u64()?,
        },
        2 => TradeStatus::OpenWindow {
            trade_id: r.u32()?,
        },
        3 => TradeStatus::Canceled,
        4 => TradeStatus::Accept,
        5 => TradeStatus::Busy2,
        6 => TradeStatus::NoTarget,
        7 => TradeStatus::BackToTrade,
        8 => TradeStatus::Complete,
        9 => TradeStatus::Rejected,
        10 => TradeStatus::TargetTooFar,
        11 => TradeStatus::WrongFaction,
        12 => TradeStatus::CloseWindow {
            result: r.u32()?,
            is_target_error: r.u8()? != 0,
            item_limit_category: r.u32()?,
        },
        14 => TradeStatus::IgnoreYou,
        15 => TradeStatus::YouStunned,
        16 => TradeStatus::TargetStunned,
        17 => TradeStatus::YouDead,
        18 => TradeStatus::TargetDead,
        19 => TradeStatus::YouLogout,
        20 => TradeStatus::TargetLogout,
        21 => TradeStatus::TrialAccount,
        22 => TradeStatus::WrongRealm { slot: r.u8()? },
        23 => TradeStatus::NotOnTapList { slot: r.u8()? },
        other => TradeStatus::Unknown(other),
    };
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(decoded)
}

/// One 12340 trade window row. `wrapped` is the wire `u32` flag; `charges` and
/// `random_prop_id` stay signed (`TradeHandler.cpp:102-104`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeItem {
    pub entry: u32,
    pub display_id: u32,
    pub count: u32,
    pub wrapped: bool,
    pub gift_creator: u64,
    pub perm_enchant: u32,
    pub socket_enchant: [u32; 3],
    pub creator: u64,
    pub charges: i32,
    pub suffix_factor: u32,
    pub random_prop_id: i32,
    pub lock_id: u32,
    pub max_durability: u32,
    pub durability: u32,
}

/// `SMSG_TRADE_STATUS_EXTENDED`: the side byte, trade id, the two state words, money and the
/// enchant spell, then an optional tail of slot records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TradeExtended {
    pub their_window: bool,
    pub trade_id: u32,
    pub state_index: u32,
    pub sequence: u32,
    pub gold: u32,
    pub enchant_spell_id: u32,
    pub slots: [Option<TradeItem>; TRADE_SLOT_COUNT],
}

/// `SMSG_TRADE_STATUS_EXTENDED`: `WorldSession::SendUpdateTrade` (`TradeHandler.cpp:72-118`)
/// writes `u8 side`, `u32 trade_id`, `u32 state_index`, `u32 state`, `u32 money`, `u32 spell`,
/// then `slot u8` + the 72-byte item block for each record until the packet ends. The client
/// loop is `Trade_C.cpp:350-412`. Empty rows still carry a full 72 zero bytes; entry 0 maps to
/// `None`. Missing slots stay `None`. `side` indexes two-entry client arrays, so only 0 and 1
/// are valid.
pub fn read_trade_status_extended(body: &[u8]) -> Result<TradeExtended> {
    let mut r = Reader::new(body);
    let side = r.u8()?;
    if side > 1 {
        return Err(DecodeError::invalid_value("trade side out of range")
            .with_value(u32::from(side))
            .into());
    }
    let trade_id = r.u32()?;
    let state_index = r.u32()?;
    let sequence = r.u32()?;
    let gold = r.u32()?;
    let enchant_spell_id = r.u32()?;

    let mut slots: [Option<TradeItem>; TRADE_SLOT_COUNT] = [None; TRADE_SLOT_COUNT];
    let mut seen = 0u8;
    while r.remaining() != 0 {
        let slot = r.u8()?;
        let index = usize::from(slot);
        if index >= TRADE_SLOT_COUNT {
            return Err(DecodeError::invalid_value("trade slot out of range")
                .with_value(u32::from(slot))
                .into());
        }
        let bit = 1u8 << index;
        if seen & bit != 0 {
            return Err(DecodeError::invalid_value("duplicate trade slot")
                .with_value(u32::from(slot))
                .into());
        }
        seen |= bit;

        let entry = r.u32()?;
        let display_id = r.u32()?;
        let count = r.u32()?;
        let wrapped = r.u32()? != 0;
        let gift_creator = r.u64()?;
        let perm_enchant = r.u32()?;
        let mut socket_enchant = [0u32; 3];
        for value in &mut socket_enchant {
            *value = r.u32()?;
        }
        let creator = r.u64()?;
        let charges = r.i32()?;
        let suffix_factor = r.u32()?;
        let random_prop_id = r.i32()?;
        let lock_id = r.u32()?;
        let max_durability = r.u32()?;
        let durability = r.u32()?;

        slots[index] = (entry != 0).then_some(TradeItem {
            entry,
            display_id,
            count,
            wrapped,
            gift_creator,
            perm_enchant,
            socket_enchant,
            creator,
            charges,
            suffix_factor,
            random_prop_id,
            lock_id,
            max_durability,
            durability,
        });
    }

    Ok(TradeExtended {
        their_window: side != 0,
        trade_id,
        state_index,
        sequence,
        gold,
        enchant_spell_id,
        slots,
    })
}

/// One enchant triplet as both the mail attachment and auction row carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MailEnchant {
    pub id: u32,
    pub duration: u32,
    pub charges: u32,
}

/// One 12340 mail attachment (`MailHandler.cpp:713-742`; client `MailInfo.cpp:409-453`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MailAttachment {
    pub index: u8,
    pub item_guid_low: u32,
    pub item_entry: u32,
    pub enchants: [MailEnchant; 7],
    pub random_prop_id: i32,
    pub suffix_factor: u32,
    pub count: u32,
    pub charges: u32,
    pub max_durability: u32,
    pub durability: u32,
    pub unknown: u8,
}

/// One 12340 inbox row. `sender_guid` is present only for a normal player mail (`type 0`);
/// `sender_entry` is present for types 1..=5. The client zeroes the unused side
/// (`MailInfo.cpp:376-386`).
#[derive(Clone, Debug, PartialEq)]
pub struct MailEntry {
    pub size: u16,
    pub message_id: u32,
    pub message_type: u8,
    pub sender_guid: Option<u64>,
    pub sender_entry: Option<u32>,
    pub cod: u32,
    pub package: u32,
    pub stationery: u32,
    pub money: u32,
    pub flags: u32,
    pub expire_days: f32,
    pub mail_template_id: u32,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<MailAttachment>,
}

/// `SMSG_MAIL_LIST_RESULT`: `real_count` is the server's full mail count, `mails` the rows the
/// client can show (`mails.len()` is the wire's `shown_count`).
#[derive(Clone, Debug, PartialEq)]
pub struct MailList {
    pub real_count: u32,
    pub mails: Vec<MailEntry>,
}

/// The pinned writer starts `next_mail_size` at 2 (`MailHandler.cpp:664`), so the declared
/// `size` counts its own two bytes. The same expression counts eight header words where the
/// writer emits seven (`MailHandler.cpp:702-708`), so an authentic 12340 record is four bytes
/// shorter than its declared size. The client reads `size` but never uses it
/// (`MailInfo.cpp:373-375`), so the reader parses the field sequence and validates the declared
/// size against the bytes actually consumed; the size is not used to skip, which would desync by
/// those four bytes on the pinned server.
const MAIL_SIZE_SELF: usize = 2;
const MAIL_SIZE_WRITER_SLACK: usize = 4;

fn mail_size_matches(declared: u16, consumed: usize) -> bool {
    let declared = usize::from(declared);
    // Exact size, a size that excludes its own two bytes (the other plausible reading of the
    // field), and the pinned writer's four-byte overstatement.
    declared == consumed
        || declared + MAIL_SIZE_SELF == consumed
        || declared == consumed + MAIL_SIZE_WRITER_SLACK
}

/// `SMSG_MAIL_LIST_RESULT`: `WorldSession::HandleGetMailList` (`MailHandler.cpp:642-750`) writes
/// `u32 real_count`, `u8 shown_count`, then `shown_count` variable records. Client loop:
/// `MailInfo.cpp:354-516`. A declared size that cannot describe the record just parsed, a shown
/// count over the client inbox cap, and a real count below the shown count are rejected.
pub fn read_mail_list_result(body: &[u8]) -> Result<MailList> {
    let mut r = Reader::new(body);
    let real_count = r.u32()?;
    let shown_count = r.u8()?;
    if usize::from(shown_count) > MAX_INBOX_CLIENT_CAPACITY {
        return Err(DecodeError::invalid_count("mail list exceeds the client inbox cap")
            .with_value(u32::from(shown_count))
            .into());
    }
    if real_count < u32::from(shown_count) {
        return Err(DecodeError::invalid_count("mail real count below shown count")
            .with_value(real_count)
            .into());
    }
    let mut mails = Vec::with_capacity(usize::from(shown_count));
    for _ in 0..shown_count {
        mails.push(read_mail_entry(&mut r)?);
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(MailList { real_count, mails })
}

fn read_mail_entry(r: &mut Reader<'_>) -> Result<MailEntry> {
    let before = r.remaining();
    let size = r.u16()?;
    let message_id = r.u32()?;
    let message_type = r.u8()?;
    let (sender_guid, sender_entry) = match message_type {
        0 => (Some(r.u64()?), None),
        1..=5 => (None, Some(r.u32()?)),
        other => {
            return Err(DecodeError::invalid_value("unknown mail message type")
                .with_value(u32::from(other))
                .into());
        }
    };
    let cod = r.u32()?;
    let package = r.u32()?;
    let stationery = r.u32()?;
    let money = r.u32()?;
    let flags = r.u32()?;
    let expire_days = r.f32()?;
    let mail_template_id = r.u32()?;
    let subject = r.cstring()?;
    let body = r.cstring()?;
    let attachment_count = r.u8()?;
    if usize::from(attachment_count) > MAX_MAIL_ITEMS {
        return Err(DecodeError::invalid_count("mail has too many attachments")
            .with_value(u32::from(attachment_count))
            .into());
    }

    let mut attachments = Vec::with_capacity(usize::from(attachment_count));
    let mut used = 0u32;
    for _ in 0..attachment_count {
        let index = r.u8()?;
        if index >= 16 {
            return Err(DecodeError::invalid_value("mail attachment index out of range")
                .with_value(u32::from(index))
                .into());
        }
        let bit = 1u32 << index;
        if used & bit != 0 {
            return Err(DecodeError::invalid_value("duplicate mail attachment index")
                .with_value(u32::from(index))
                .into());
        }
        used |= bit;

        let item_guid_low = r.u32()?;
        let item_entry = r.u32()?;
        let mut enchants = [MailEnchant {
            id: 0,
            duration: 0,
            charges: 0,
        }; 7];
        for enchant in &mut enchants {
            enchant.id = r.u32()?;
            enchant.duration = r.u32()?;
            enchant.charges = r.u32()?;
        }
        attachments.push(MailAttachment {
            index,
            item_guid_low,
            item_entry,
            enchants,
            random_prop_id: r.i32()?,
            suffix_factor: r.u32()?,
            count: r.u32()?,
            charges: r.u32()?,
            max_durability: r.u32()?,
            durability: r.u32()?,
            unknown: r.u8()?,
        });
    }

    let consumed = before - r.remaining();
    if !mail_size_matches(size, consumed) {
        return Err(DecodeError::invalid_value("mail size does not match its record")
            .with_value(u32::from(size))
            .into());
    }

    Ok(MailEntry {
        size,
        message_id,
        message_type,
        sender_guid,
        sender_entry,
        cod,
        package,
        stationery,
        money,
        flags,
        expire_days,
        mail_template_id,
        subject,
        body,
        attachments,
    })
}

/// `SMSG_SEND_MAIL_RESULT` tail. `equip_error` is the writer's
/// `MAIL_ERR_EQUIP_ERROR` (result 1) word; `item` is the `(guid_low, count)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendMailResult {
    pub mail_id: u32,
    pub action: u32,
    pub error: u32,
    pub equip_error: Option<u32>,
    pub item: Option<(u32, u32)>,
}

/// `SMSG_SEND_MAIL_RESULT`: `Player::SendMailResult` (`Player.cpp:2884-2898`) writes id, action
/// and error, then an equip word for result 1 or an item pair for `MAIL_ITEM_TAKEN` (2). The
/// client reads the item pair only for results 0 and 21 (`MailInfo.cpp:199-215`); the writer also
/// emits it for the other non-equip item-taken results, so both shapes are accepted.
pub fn read_send_mail_result(body: &[u8]) -> Result<SendMailResult> {
    let mut r = Reader::new(body);
    let mail_id = r.u32()?;
    let action = r.u32()?;
    let error = r.u32()?;
    let mut equip_error = None;
    let mut item = None;
    if error == 1 {
        if r.remaining() >= 4 {
            equip_error = Some(r.u32()?);
        }
    } else if action == 2 {
        // The writer sends the pair for every non-equip result; a client-shaped body for a
        // result the client does not read may omit it.
        if r.remaining() >= 8 {
            item = Some((r.u32()?, r.u32()?));
        } else if r.remaining() != 0 {
            return Err(DecodeError::truncated(r.remaining()).into());
        }
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(SendMailResult {
        mail_id,
        action,
        error,
        equip_error,
        item,
    })
}

/// `SMSG_RECEIVED_MAIL`: the client reads one float (`MailInfo.cpp:345-350`); the pinned server
/// sends 0 (`Player::SendNewMail`, `Player.cpp:2900-2906`).
pub fn read_received_mail(body: &[u8]) -> Result<f32> {
    let mut r = Reader::new(body);
    let days = r.f32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(days)
}

/// One recent sender in `MSG_QUERY_NEXT_MAIL_TIME` (client `MailInfo.cpp:1441-1447`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MailSenderInfo {
    pub guid: u64,
    pub entry: u32,
    pub message_type: u32,
    pub stationery: u32,
    pub time_left: f32,
}

/// `MSG_QUERY_NEXT_MAIL_TIME`: `WorldSession::HandleQueryNextMailTime`
/// (`MailHandler.cpp:824-871`) writes a float then `u32 count` and 24-byte sender rows. The
/// client stores three senders and ignores anything after the third (`MailInfo.cpp:1431-1460`),
/// so more than three is malformed for this client.
pub fn read_next_mail_time(body: &[u8]) -> Result<(f32, Vec<MailSenderInfo>)> {
    let mut r = Reader::new(body);
    let next = r.f32()?;
    let count = r.u32()?;
    if count > 3 {
        return Err(DecodeError::invalid_count("next mail time has more than three senders")
            .with_value(count)
            .into());
    }
    let mut senders = Vec::with_capacity(count as usize);
    for _ in 0..count {
        senders.push(MailSenderInfo {
            guid: r.u64()?,
            entry: r.u32()?,
            message_type: r.u32()?,
            stationery: r.u32()?,
            time_left: r.f32()?,
        });
    }
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok((next, senders))
}

/// `MSG_AUCTION_HELLO`: `WorldSession::SendAuctionHello` (`AuctionHouseHandler.cpp:53-73`)
/// writes the raw auctioneer guid, `houseId` and the 12340 enabled byte. Client:
/// `AuctionHouse.cpp:1780-1796`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuctionHello {
    pub auctioneer: u64,
    pub house_id: u32,
    pub enabled: bool,
}

pub fn read_auction_hello(body: &[u8]) -> Result<AuctionHello> {
    let mut r = Reader::new(body);
    let auctioneer = r.u64()?;
    let house_id = r.u32()?;
    let enabled = r.u8()? != 0;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(AuctionHello {
        auctioneer,
        house_id,
        enabled,
    })
}

/// `SMSG_AUCTION_COMMAND_RESULT`: id, action and error, plus the conditional word the writer
/// appends. The field is the inventory result when `error == 1`; on a successful bid the same
/// word is the minimum increment (`AuctionHouse.cpp:1828-1844`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuctionCommandResult {
    pub auction_id: u32,
    pub action: u32,
    pub error: u32,
    pub inventory_error: Option<u32>,
}

/// `SMSG_AUCTION_COMMAND_RESULT`: the pinned writer appends a word when `error == 0 && action
/// != 0` (`AuctionHouseHandler.cpp:76-85`), while the client reads its word when `error == 1`
/// (`AuctionHouse.cpp:1800-1811`). To keep every authentic server packet decodable without
/// inventing a field, the word is accepted for either condition and left `None` when absent.
pub fn read_auction_command_result(body: &[u8]) -> Result<AuctionCommandResult> {
    let mut r = Reader::new(body);
    let auction_id = r.u32()?;
    let action = r.u32()?;
    let error = r.u32()?;
    let tail_allowed = error == 1 || (error == 0 && action != 0);
    let inventory_error = match r.remaining() {
        0 => None,
        4 if tail_allowed => Some(r.u32()?),
        remaining => return Err(DecodeError::trailing(remaining).into()),
    };
    Ok(AuctionCommandResult {
        auction_id,
        action,
        error,
        inventory_error,
    })
}

/// One 12340 auction row. `min_increment` is the server's outbid floor
/// (`AuctionHouseSearcher.cpp:453`); `time_left_ms` is raw milliseconds, not a deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuctionEntry {
    pub auction_id: u32,
    pub item_entry: u32,
    pub enchants: [MailEnchant; 7],
    pub random_prop_id: i32,
    pub suffix_factor: u32,
    pub count: u32,
    pub spell_charges: i32,
    pub flags: u32,
    pub owner_guid: u64,
    pub start_bid: u32,
    pub min_increment: u32,
    pub buyout: u32,
    pub time_left_ms: u32,
    pub bidder_guid: u64,
    pub current_bid: u32,
}

/// `SMSG_AUCTION_LIST_RESULT` (and the owner/bidder variants): the row count, the rows, the
/// total count and the search delay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuctionList {
    pub auctions: Vec<AuctionEntry>,
    pub total_count: u32,
    pub search_delay: u32,
}

/// `SearchableAuctionEntry::BuildAuctionInfo` (`AuctionHouseSearcher.cpp:434-459`): id, entry,
/// seven enchant triplets, signed random property, suffix, count, signed spell charges, flags,
/// owner, start bid, outbid floor, buyout, time left, bidder and current bid. The client reads
/// the same 148-byte row (`AuctionHouse.cpp:1259-1294`).
fn read_auction_entry(r: &mut Reader<'_>) -> Result<AuctionEntry> {
    let auction_id = r.u32()?;
    let item_entry = r.u32()?;
    let mut enchants = [MailEnchant {
        id: 0,
        duration: 0,
        charges: 0,
    }; 7];
    for enchant in &mut enchants {
        enchant.id = r.u32()?;
        enchant.duration = r.u32()?;
        enchant.charges = r.u32()?;
    }
    Ok(AuctionEntry {
        auction_id,
        item_entry,
        enchants,
        random_prop_id: r.i32()?,
        suffix_factor: r.u32()?,
        count: r.u32()?,
        spell_charges: r.i32()?,
        flags: r.u32()?,
        owner_guid: r.u64()?,
        start_bid: r.u32()?,
        min_increment: r.u32()?,
        buyout: r.u32()?,
        time_left_ms: r.u32()?,
        bidder_guid: r.u64()?,
        current_bid: r.u32()?,
    })
}

fn read_auction_list(body: &[u8], cap: u32) -> Result<AuctionList> {
    let mut r = Reader::new(body);
    let count = r.u32()?;
    if count > cap {
        return Err(DecodeError::invalid_count("auction list exceeds its page cap")
            .with_value(count)
            .into());
    }
    let mut auctions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        auctions.push(read_auction_entry(&mut r)?);
    }
    let total_count = r.u32()?;
    let search_delay = r.u32()?;
    if r.remaining() != 0 {
        return Err(DecodeError::trailing(r.remaining()).into());
    }
    Ok(AuctionList {
        auctions,
        total_count,
        search_delay,
    })
}

/// `SMSG_AUCTION_LIST_RESULT`: `AuctionHouseWorkerThread::SearchListRequest`
/// (`AuctionHouseSearcher.cpp:142-203`) pages browse rows at `MAX_AUCTIONS_PER_PAGE`.
pub fn read_auction_list_result(body: &[u8]) -> Result<AuctionList> {
    read_auction_list(body, MAX_AUCTIONS_PER_PAGE)
}

/// `SMSG_AUCTION_LIST_OWNER_ITEMS_RESULT`: same row shape, unpaged by the searcher
/// (`AuctionHouseSearcher.cpp:205-233`).
pub fn read_auction_owner_list_result(body: &[u8]) -> Result<AuctionList> {
    read_auction_list(body, MAX_OWNED_AUCTIONS)
}

/// `SMSG_AUCTION_LIST_BIDDER_ITEMS_RESULT`: same row shape, unpaged by the searcher
/// (`AuctionHouseSearcher.cpp:235-274`).
pub fn read_auction_bidder_list_result(body: &[u8]) -> Result<AuctionList> {
    read_auction_list(body, MAX_OWNED_AUCTIONS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_trade_item(p: &mut Vec<u8>) {
        for value in [40_000u32, 1234, 3, 1] {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        p.extend_from_slice(&0x8000_0001u32.to_le_bytes());
        for value in [11u32, 22, 33] {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.extend_from_slice(&0x8877_6655_4433_2211u64.to_le_bytes());
        p.extend_from_slice(&(-3i32).to_le_bytes());
        p.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        p.extend_from_slice(&(-42i32).to_le_bytes());
        for value in [1_001u32, 120, 119] {
            p.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn trade_extended() -> Vec<u8> {
        let mut p = vec![1u8];
        for value in [0x1122_3344u32, 7, 7, 987_654, 0x00AB_CDEF] {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.push(0);
        push_trade_item(&mut p);
        for slot in 1..TRADE_SLOT_COUNT as u8 {
            p.push(slot);
            p.extend_from_slice(&[0u8; 72]);
        }
        p
    }

    #[test]
    fn show_bank_and_buy_bank_slot_are_fixed_records() {
        let mut body = 0x1122_3344_5566_7788u64.to_le_bytes().to_vec();
        assert_eq!(
            read_show_bank(&body).unwrap(),
            ShowBank {
                banker: 0x1122_3344_5566_7788
            }
        );
        for end in 0..body.len() {
            assert!(read_show_bank(&body[..end]).is_err());
        }
        body.push(0);
        assert!(read_show_bank(&body).is_err());

        let result = 0x102u32.to_le_bytes().to_vec();
        assert_eq!(read_buy_bank_slot_result(&result).unwrap(), 0x102);
        assert!(read_buy_bank_slot_result(&result[..3]).is_err());
        let mut extra = result.clone();
        extra.push(0);
        assert!(read_buy_bank_slot_result(&extra).is_err());
    }

    #[test]
    fn trade_status_reads_each_12340_tail_and_rejects_extra_bytes() {
        let mut close = 12u32.to_le_bytes().to_vec();
        close.extend_from_slice(&0x2Au32.to_le_bytes());
        close.push(1);
        close.extend_from_slice(&84u32.to_le_bytes());
        assert_eq!(
            read_trade_status(&close).unwrap(),
            TradeStatus::CloseWindow {
                result: 0x2A,
                is_target_error: true,
                item_limit_category: 84,
            }
        );
        let mut close_extra = close.clone();
        close_extra.push(0);
        assert!(read_trade_status(&close_extra).is_err());

        let mut begin = 1u32.to_le_bytes().to_vec();
        begin.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(
            read_trade_status(&begin).unwrap(),
            TradeStatus::BeginTrade {
                partner: 0x1122_3344_5566_7788
            }
        );

        let mut open = 2u32.to_le_bytes().to_vec();
        open.extend_from_slice(&7u32.to_le_bytes());
        assert_eq!(
            read_trade_status(&open).unwrap(),
            TradeStatus::OpenWindow { trade_id: 7 }
        );

        let mut wrong_realm = 22u32.to_le_bytes().to_vec();
        wrong_realm.push(0xFF);
        assert_eq!(
            read_trade_status(&wrong_realm).unwrap(),
            TradeStatus::WrongRealm { slot: 0xFF }
        );

        assert_eq!(
            read_trade_status(&13u32.to_le_bytes()).unwrap(),
            TradeStatus::Unknown(13)
        );
        assert_eq!(read_trade_status(&8u32.to_le_bytes()).unwrap(), TradeStatus::Complete);
        let mut status_extra = 8u32.to_le_bytes().to_vec();
        status_extra.push(0);
        assert!(read_trade_status(&status_extra).is_err());
    }

    #[test]
    fn trade_extended_carries_sockets_creator_signed_fields_and_empty_slots() {
        let body = trade_extended();
        let decoded = read_trade_status_extended(&body).unwrap();
        assert!(decoded.their_window);
        assert_eq!(decoded.trade_id, 0x1122_3344);
        assert_eq!(decoded.state_index, 7);
        assert_eq!(decoded.sequence, 7);
        assert_eq!(decoded.gold, 987_654);
        assert_eq!(decoded.enchant_spell_id, 0x00AB_CDEF);

        let item = decoded.slots[0].unwrap();
        assert_eq!(item.entry, 40_000);
        assert_eq!(item.display_id, 1234);
        assert_eq!(item.count, 3);
        assert!(item.wrapped);
        assert_eq!(item.gift_creator, 0x1122_3344_5566_7788);
        assert_eq!(item.perm_enchant, 0x8000_0001);
        assert_eq!(item.socket_enchant, [11, 22, 33]);
        assert_eq!(item.creator, 0x8877_6655_4433_2211);
        assert_eq!(item.charges, -3);
        assert_eq!(item.suffix_factor, 0xDEAD_BEEF);
        assert_eq!(item.random_prop_id, -42);
        assert_eq!(item.lock_id, 1_001);
        assert_eq!(item.max_durability, 120);
        assert_eq!(item.durability, 119);
        assert!(decoded.slots[1..].iter().all(Option::is_none));

        // Fewer than seven records is authentic: missing slots stay empty.
        let header_and_one = 1 + 4 * 5 + 1 + 72;
        let partial = read_trade_status_extended(&body[..header_and_one]).unwrap();
        assert!(partial.slots[0].is_some());
        assert!(partial.slots[1..].iter().all(Option::is_none));

        // Duplicate, out-of-range and partial records are all rejected.
        let mut duplicate = body.clone();
        duplicate.push(0);
        duplicate.extend_from_slice(&[0u8; 72]);
        assert!(read_trade_status_extended(&duplicate).is_err());
        let mut out_of_range = body.clone();
        out_of_range.push(7);
        out_of_range.extend_from_slice(&[0u8; 72]);
        assert!(read_trade_status_extended(&out_of_range).is_err());
        let mut bad_side = body.clone();
        bad_side[0] = 2;
        assert!(read_trade_status_extended(&bad_side).is_err());
        assert!(read_trade_status_extended(&body[..body.len() - 1]).is_err());

        // Every prefix either ends on a record boundary (valid, with only those slots) or cuts
        // a block (truncated).
        let header = 1 + 4 * 5;
        let row = 1 + 72;
        for end in 0..body.len() {
            let complete = end == header
                || (end > header && (end - header) % row == 0);
            assert_eq!(
                read_trade_status_extended(&body[..end]).is_ok(),
                complete,
                "prefix {end}"
            );
        }
    }

    fn push_attachment(p: &mut Vec<u8>, index: u8, guid_low: u32, entry: u32) {
        p.push(index);
        p.extend_from_slice(&guid_low.to_le_bytes());
        p.extend_from_slice(&entry.to_le_bytes());
        for i in 0..7u32 {
            p.extend_from_slice(&(100 + i).to_le_bytes());
            p.extend_from_slice(&(200 + i).to_le_bytes());
            p.extend_from_slice(&(300 + i).to_le_bytes());
        }
        p.extend_from_slice(&(-7i32).to_le_bytes());
        p.extend_from_slice(&11u32.to_le_bytes());
        p.extend_from_slice(&5u32.to_le_bytes());
        p.extend_from_slice(&u32::MAX.to_le_bytes());
        p.extend_from_slice(&60u32.to_le_bytes());
        p.extend_from_slice(&59u32.to_le_bytes());
        p.push(3);
    }

    fn mail_list() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&7u32.to_le_bytes());
        p.push(2);

        // Player mail with two attachments, the second an empty item block.
        let start = p.len();
        p.extend_from_slice(&0u16.to_le_bytes());
        p.extend_from_slice(&0x0BAD_F00Du32.to_le_bytes());
        p.push(0);
        p.extend_from_slice(&0x0000_0001_0000_00ABu64.to_le_bytes());
        for value in [25u32, 0, 41, 99, 0x20A] {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.extend_from_slice(&3.5f32.to_le_bytes());
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(b"Auction won\0Your bid won.\0");
        p.push(2);
        push_attachment(&mut p, 0, 12_345, 2589);
        push_attachment(&mut p, 1, 0, 0);
        let len = (p.len() - start) as u16;
        p[start..start + 2].copy_from_slice(&len.to_le_bytes());

        // Creature mail with no attachment.
        let start = p.len();
        p.extend_from_slice(&0u16.to_le_bytes());
        p.extend_from_slice(&0x0BAD_F00Eu32.to_le_bytes());
        p.push(3);
        p.extend_from_slice(&1234u32.to_le_bytes());
        for value in [0u32, 0, 1, 0, 0] {
            p.extend_from_slice(&value.to_le_bytes());
        }
        p.extend_from_slice(&1.0f32.to_le_bytes());
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(b"GM\0Hello.\0");
        p.push(0);
        let len = (p.len() - start) as u16;
        p[start..start + 2].copy_from_slice(&len.to_le_bytes());
        p
    }

    #[test]
    fn mail_list_reads_rows_bodies_and_attachments() {
        let body = mail_list();
        let list = read_mail_list_result(&body).unwrap();
        assert_eq!(list.real_count, 7);
        assert_eq!(list.mails.len(), 2);

        let first = &list.mails[0];
        assert_eq!(first.message_id, 0x0BAD_F00D);
        assert_eq!(first.message_type, 0);
        assert_eq!(first.sender_guid, Some(0x0000_0001_0000_00AB));
        assert_eq!(first.sender_entry, None);
        assert_eq!(first.cod, 25);
        assert_eq!(first.stationery, 41);
        assert_eq!(first.money, 99);
        assert_eq!(first.flags, 0x20A);
        assert_eq!(first.expire_days, 3.5);
        assert_eq!(first.subject, "Auction won");
        assert_eq!(first.body, "Your bid won.");
        assert_eq!(first.attachments.len(), 2);
        let attached = first.attachments[0];
        assert_eq!(attached.index, 0);
        assert_eq!(attached.item_guid_low, 12_345);
        assert_eq!(attached.item_entry, 2589);
        assert_eq!(
            attached.enchants[0],
            MailEnchant {
                id: 100,
                duration: 200,
                charges: 300
            }
        );
        assert_eq!(
            attached.enchants[6],
            MailEnchant {
                id: 106,
                duration: 206,
                charges: 306
            }
        );
        assert_eq!(attached.random_prop_id, -7);
        assert_eq!(attached.suffix_factor, 11);
        assert_eq!(attached.count, 5);
        assert_eq!(attached.charges, u32::MAX);
        assert_eq!(attached.max_durability, 60);
        assert_eq!(attached.durability, 59);
        assert_eq!(attached.unknown, 3);
        let empty = first.attachments[1];
        assert_eq!(empty.index, 1);
        assert_eq!(empty.item_entry, 0);
        assert_eq!(empty.enchants[3].id, 103);

        let second = &list.mails[1];
        assert_eq!(second.message_id, 0x0BAD_F00E);
        assert_eq!(second.message_type, 3);
        assert_eq!(second.sender_guid, None);
        assert_eq!(second.sender_entry, Some(1234));
        assert_eq!(second.subject, "GM");
        assert_eq!(second.body, "Hello.");
        assert!(second.attachments.is_empty());

        // Every prefix cuts the declared rows short.
        for end in 0..body.len() {
            assert!(read_mail_list_result(&body[..end]).is_err(), "prefix {end}");
        }

        // The pinned writer's four-byte overstatement is accepted; the row boundary stays the
        // field sequence, not the declared size.
        let mut server_sized = body.clone();
        let declared = u16::from_le_bytes([server_sized[5], server_sized[6]]);
        server_sized[5..7].copy_from_slice(
            &(declared + MAIL_SIZE_WRITER_SLACK as u16).to_le_bytes(),
        );
        let server_list = read_mail_list_result(&server_sized).unwrap();
        assert_eq!(server_list.real_count, list.real_count);
        assert_eq!(server_list.mails.len(), list.mails.len());
        assert_eq!(
            server_list.mails[0].size,
            declared + MAIL_SIZE_WRITER_SLACK as u16
        );
        assert_eq!(server_list.mails[0].body, list.mails[0].body);
        assert_eq!(server_list.mails[0].attachments, list.mails[0].attachments);
        assert_eq!(server_list.mails[1], list.mails[1]);

        // A declared size that points into the next mail (or past the packet) is rejected.
        let mut overrun = body.clone();
        let first_size_at = 5;
        overrun[first_size_at..first_size_at + 2]
            .copy_from_slice(&(body.len() as u16).to_le_bytes());
        assert!(read_mail_list_result(&overrun).is_err());

        // The client inbox cap is enforced before any row is decoded.
        let mut too_many = 51u32.to_le_bytes().to_vec();
        too_many.push(51);
        assert!(read_mail_list_result(&too_many).is_err());

        // Duplicate attachment indices are rejected.
        let mut duplicate = Vec::new();
        duplicate.extend_from_slice(&1u32.to_le_bytes());
        duplicate.push(1);
        let start = duplicate.len();
        duplicate.extend_from_slice(&0u16.to_le_bytes());
        duplicate.extend_from_slice(&1u32.to_le_bytes());
        duplicate.push(3);
        duplicate.extend_from_slice(&1234u32.to_le_bytes());
        for value in [0u32; 5] {
            duplicate.extend_from_slice(&value.to_le_bytes());
        }
        duplicate.extend_from_slice(&0f32.to_le_bytes());
        duplicate.extend_from_slice(&0u32.to_le_bytes());
        duplicate.extend_from_slice(b"s\0b\0");
        duplicate.push(2);
        push_attachment(&mut duplicate, 0, 1, 2);
        push_attachment(&mut duplicate, 0, 3, 4);
        let len = (duplicate.len() - start) as u16;
        duplicate[start..start + 2].copy_from_slice(&len.to_le_bytes());
        assert!(read_mail_list_result(&duplicate).is_err());
    }

    #[test]
    fn mail_result_received_and_next_time_follow_their_tails() {
        let mut plain = 4u32.to_le_bytes().to_vec();
        plain.extend_from_slice(&0u32.to_le_bytes());
        plain.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_send_mail_result(&plain).unwrap(),
            SendMailResult {
                mail_id: 4,
                action: 0,
                error: 0,
                equip_error: None,
                item: None
            }
        );

        let mut equip = 4u32.to_le_bytes().to_vec();
        equip.extend_from_slice(&2u32.to_le_bytes());
        equip.extend_from_slice(&1u32.to_le_bytes());
        equip.extend_from_slice(&0x28u32.to_le_bytes());
        assert_eq!(read_send_mail_result(&equip).unwrap().equip_error, Some(0x28));

        let mut item = 4u32.to_le_bytes().to_vec();
        item.extend_from_slice(&2u32.to_le_bytes());
        item.extend_from_slice(&0u32.to_le_bytes());
        item.extend_from_slice(&77u32.to_le_bytes());
        item.extend_from_slice(&5u32.to_le_bytes());
        assert_eq!(read_send_mail_result(&item).unwrap().item, Some((77, 5)));

        assert!(read_send_mail_result(&[0u8; 11]).is_err());
        let mut trailing = plain.clone();
        trailing.push(0);
        assert!(read_send_mail_result(&trailing).is_err());

        assert_eq!(read_received_mail(&0.0f32.to_le_bytes()).unwrap(), 0.0);
        assert!(read_received_mail(&[0u8; 3]).is_err());

        let mut next = 0.0f32.to_le_bytes().to_vec();
        next.extend_from_slice(&3u32.to_le_bytes());
        for i in 0..3u32 {
            next.extend_from_slice(&(0x1000u64 + u64::from(i)).to_le_bytes());
            next.extend_from_slice(&(5000 + i).to_le_bytes());
            next.extend_from_slice(&3u32.to_le_bytes());
            next.extend_from_slice(&41u32.to_le_bytes());
            next.extend_from_slice(&(60.0f32 + i as f32).to_le_bytes());
        }
        let (delay, senders) = read_next_mail_time(&next).unwrap();
        assert_eq!(delay, 0.0);
        assert_eq!(senders.len(), 3);
        assert_eq!(senders[2].guid, 0x1002);
        assert_eq!(senders[2].entry, 5002);
        assert_eq!(senders[2].message_type, 3);
        assert_eq!(senders[2].stationery, 41);
        assert_eq!(senders[2].time_left, 62.0);

        let mut four = next.clone();
        four[4..8].copy_from_slice(&4u32.to_le_bytes());
        four.extend_from_slice(&[0u8; 24]);
        assert!(read_next_mail_time(&four).is_err());
        let mut trailing = next.clone();
        trailing.push(0);
        assert!(read_next_mail_time(&trailing).is_err());
    }

    fn push_auction_row(p: &mut Vec<u8>, auction_id: u32, item_entry: u32, random: i32) {
        p.extend_from_slice(&auction_id.to_le_bytes());
        p.extend_from_slice(&item_entry.to_le_bytes());
        for i in 0..7u32 {
            p.extend_from_slice(&(10 + i).to_le_bytes());
            p.extend_from_slice(&(20 + i).to_le_bytes());
            p.extend_from_slice(&(30 + i).to_le_bytes());
        }
        p.extend_from_slice(&random.to_le_bytes());
        p.extend_from_slice(&0xCAFEu32.to_le_bytes());
        p.extend_from_slice(&5u32.to_le_bytes());
        p.extend_from_slice(&(-2i32).to_le_bytes());
        p.extend_from_slice(&7u32.to_le_bytes());
        p.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        p.extend_from_slice(&100u32.to_le_bytes());
        p.extend_from_slice(&5u32.to_le_bytes());
        p.extend_from_slice(&500u32.to_le_bytes());
        p.extend_from_slice(&60_000u32.to_le_bytes());
        p.extend_from_slice(&0x8877_6655_4433_2211u64.to_le_bytes());
        p.extend_from_slice(&120u32.to_le_bytes());
    }

    fn auction_list() -> Vec<u8> {
        let mut p = 2u32.to_le_bytes().to_vec();
        push_auction_row(&mut p, 1, 2589, -7);
        push_auction_row(&mut p, 2, 2590, 42);
        p.extend_from_slice(&9u32.to_le_bytes());
        p.extend_from_slice(&300u32.to_le_bytes());
        p
    }

    #[test]
    fn auction_list_reads_rows_totals_and_signed_random_property() {
        let body = auction_list();
        let list = read_auction_list_result(&body).unwrap();
        assert_eq!(list.auctions.len(), 2);
        assert_eq!(list.total_count, 9);
        assert_eq!(list.search_delay, 300);
        assert_eq!(list.auctions[0].auction_id, 1);
        assert_eq!(list.auctions[0].item_entry, 2589);
        assert_eq!(
            list.auctions[0].enchants[0],
            MailEnchant {
                id: 10,
                duration: 20,
                charges: 30
            }
        );
        assert_eq!(list.auctions[0].random_prop_id, -7);
        assert_eq!(list.auctions[0].spell_charges, -2);
        assert_eq!(list.auctions[0].owner_guid, 0x1122_3344_5566_7788);
        assert_eq!(list.auctions[0].min_increment, 5);
        assert_eq!(list.auctions[0].bidder_guid, 0x8877_6655_4433_2211);
        assert_eq!(list.auctions[0].current_bid, 120);
        assert_eq!(list.auctions[1].random_prop_id, 42);
        assert_eq!(
            read_auction_owner_list_result(&body).unwrap(),
            read_auction_list_result(&body).unwrap()
        );
        assert_eq!(
            read_auction_bidder_list_result(&body).unwrap(),
            read_auction_list_result(&body).unwrap()
        );

        assert!(read_auction_list_result(&body[..body.len() - 1]).is_err());
        let mut over = 51u32.to_le_bytes().to_vec();
        over.extend_from_slice(&0u32.to_le_bytes());
        over.extend_from_slice(&300u32.to_le_bytes());
        assert!(read_auction_list_result(&over).is_err());
    }

    #[test]
    fn auction_hello_and_command_result_follow_12340_tails() {
        let mut hello = 0x1122_3344_5566_7788u64.to_le_bytes().to_vec();
        hello.extend_from_slice(&2u32.to_le_bytes());
        hello.push(1);
        assert_eq!(
            read_auction_hello(&hello).unwrap(),
            AuctionHello {
                auctioneer: 0x1122_3344_5566_7788,
                house_id: 2,
                enabled: true
            }
        );
        for end in 0..hello.len() {
            assert!(read_auction_hello(&hello[..end]).is_err());
        }
        hello.push(0);
        assert!(read_auction_hello(&hello).is_err());

        let mut plain = 1u32.to_le_bytes().to_vec();
        plain.extend_from_slice(&0u32.to_le_bytes());
        plain.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_auction_command_result(&plain).unwrap(),
            AuctionCommandResult {
                auction_id: 1,
                action: 0,
                error: 0,
                inventory_error: None
            }
        );

        // An action-0 success has no writer tail, so a fourth word is malformed.
        let mut inventory = plain.clone();
        inventory.extend_from_slice(&0x28u32.to_le_bytes());
        assert!(read_auction_command_result(&inventory).is_err());

        let mut failed = 1u32.to_le_bytes().to_vec();
        failed.extend_from_slice(&0u32.to_le_bytes());
        failed.extend_from_slice(&1u32.to_le_bytes());
        failed.extend_from_slice(&0x28u32.to_le_bytes());
        assert_eq!(
            read_auction_command_result(&failed).unwrap().inventory_error,
            Some(0x28)
        );
        // The server does not append the word for a non-zero error; the client would read past
        // the body, so absence is the authentic shape.
        let mut failed_short = 1u32.to_le_bytes().to_vec();
        failed_short.extend_from_slice(&0u32.to_le_bytes());
        failed_short.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_auction_command_result(&failed_short)
                .unwrap()
                .inventory_error,
            None
        );

        let mut bid = 1u32.to_le_bytes().to_vec();
        bid.extend_from_slice(&2u32.to_le_bytes());
        bid.extend_from_slice(&0u32.to_le_bytes());
        bid.extend_from_slice(&5u32.to_le_bytes());
        assert_eq!(
            read_auction_command_result(&bid).unwrap().inventory_error,
            Some(5),
            "the writer's bid tail is the minimum increment"
        );

        let mut trailing = plain.clone();
        trailing.push(0);
        assert!(read_auction_command_result(&trailing).is_err());
        assert!(read_auction_command_result(&failed[..15]).is_err());
    }
}
