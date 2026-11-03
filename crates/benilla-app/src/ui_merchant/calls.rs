//! The merchant verbs' engine half, which the call host applies inside each call
//! ([`crate::script_calls::ScriptCallHost`]): each refuses or sends with the vendor cell as it
//! stands (`[0xbddfa0]`) before the verb returns, as the reference's bindings send in their own
//! bodies (`0x4fb850`, `0x4fb760`, `0x4fb950`, `0x4fbfe0`), and a held row's drop in its slot's
//! (`0x4f9f35`, `0x4c78fd`) or its bag's (`0x4c7e66`).

use benilla_ui::script::{MerchantCall, UiScript};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_protocol::messages::BAG_PLAYER_INVENTORY;

use super::{buyback_order, MerchantOpen, RepairPurse, BUYBACK_SLOT_FIRST};
use crate::net::{ClientCommand, Guid, NetCommands, ObjectStore, SelfPlayer};

/// The open vendor, the player, and what the verbs send through.
#[derive(SystemParam)]
pub(crate) struct MerchantSteps<'w, 's> {
    /// Absent in a world with no merchant window, which drops the calls.
    open: Option<ResMut<'w, MerchantOpen>>,
    net: Res<'w, NetCommands>,
    self_q: Query<'w, 's, (&'static ObjectStore, &'static Guid), With<SelfPlayer>>,
    purse: RepairPurse<'w, 's>,
    /// Resident interaction diagnostics; merchant producers record local boundaries here.
    trace: Option<Res<'w, crate::quest_trace::Source>>,
}

impl MerchantSteps<'_, '_> {
    /// The vendor's cell `[0xbddfa0]`.
    pub(crate) fn vendor(&self) -> Option<u64> {
        self.open.as_deref().and_then(|o| o.vendor)
    }

    /// The close core's clear of the cell (`0x4fae0e`), and the VM's window with it.
    pub(crate) fn clear_vendor(&mut self, script: &mut UiScript) {
        if let Some(open) = self.open.as_deref_mut() {
            open.clear();
        }
        script.set_merchant(None);
    }

    /// Apply one merchant call; the close is the host's ([`MerchantCall::Close`]).
    pub(crate) fn apply(&mut self, script: &mut UiScript, call: MerchantCall) {
        use crate::quest_trace::{Operation, Stage};
        let op = match &call {
            MerchantCall::Sell { .. } => Some(Operation::Sell),
            MerchantCall::Buy { .. } | MerchantCall::BuyIntoBag { .. }
            | MerchantCall::BuyInSlot { .. } => Some(Operation::Buy),
            MerchantCall::Buyback(_) | MerchantCall::RepairAll | MerchantCall::Close => None,
        };
        let vendor_npc = self.open.as_deref().and_then(|o| o.vendor);
        let trace = self
            .trace
            .as_ref()
            .zip(op)
            .map(|(s, op)| s.begin_with_npc(None, None, vendor_npc, op));
        let record = |trace: &Option<crate::quest_trace::Interaction>, stage| {
            if let Some(t) = trace {
                t.record(stage);
            }
        };
        let Self {
            open,
            net,
            self_q,
            purse,
            trace: _,
        } = self;
        let Some(open) = open.as_deref() else {
            record(&trace, Stage::GuardRefused);
            debug!("ui_merchant: no merchant window in this world; {call:?} dropped");
            return;
        };
        let me_pair = self_q.iter().next();
        let me = me_pair.map(|(store, _)| store);
        match call {
            // With a player (`0x4fb88e`), the row in range (`0x4fb904`-`0x4fb90e`), a vendor cell
            // (`0x4fb91d`) and a row slot (`0x4fb92e`): the row's item, `CMSG_BUY_ITEM`
            // (`0x4fb93c` → `0x5e1e90`), the quantity a byte.
            MerchantCall::Buy { row, quantity } => {
                let (Some(vendor), Some(_)) = (open.vendor, me) else {
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let Some(item) = open.row_at(row).filter(|it| it.slot != 0) else {
                    record(&trace, Stage::GuardRefused);
                    debug!("ui_merchant: BuyMerchantItem({row}) out of range — ignored");
                    return;
                };
                let count = quantity.clamp(1, u32::from(u8::MAX)) as u8;
                debug!("ui_merchant: buy row {row} (entry {} ×{count})", item.entry);
                let result = net.0.send(ClientCommand::BuyItem {
                    vendor,
                    entry: item.entry,
                    count,
                });
                record(
                    &trace,
                    if result.is_ok() {
                        Stage::ChannelAdmitted
                    } else {
                        Stage::ChannelDisconnected
                    },
                );
            }
            // The vendor cell as it stands, never tested, so 0 with none (`0x4fb799`).
            MerchantCall::Sell { item_guid, count } => {
                let vendor = open.vendor.unwrap_or(0);
                debug!("ui_merchant: cursor sell of {item_guid:#x} ×{count} to {vendor:#x}");
                let result = net.0.send(ClientCommand::SellItem {
                    vendor,
                    item_guid,
                    count,
                });
                record(
                    &trace,
                    if result.is_ok() {
                        Stage::ChannelAdmitted
                    } else {
                        Stage::ChannelDisconnected
                    },
                );
            }
            // With a vendor, a player and an item in the slot the list names (`0x4fb9b5`,
            // `0x4fb9fc`): the absolute slot, 69 to 80 (`0x4fba02`).
            MerchantCall::Buyback(index) => {
                let (Some(vendor), Some(store)) = (open.vendor, me) else {
                    return;
                };
                let order = buyback_order(&store.0);
                match index.checked_sub(1).and_then(|i| order.get(i as usize)) {
                    Some(&idx) => {
                        let slot = BUYBACK_SLOT_FIRST + u32::from(idx);
                        debug!("ui_merchant: buyback list index {index} → absolute slot {slot}");
                        let _ = net.0.send(ClientCommand::BuybackItem { vendor, slot });
                    }
                    None => debug!("ui_merchant: BuybackItem({index}) out of range — ignored"),
                }
            }
            // With a player, the purse test over the equipped slots 0-18 only (`0x4fc026`-
            // `0x4fc063`), not the bags `GetRepairAllCost` also sweeps; then `CMSG_REPAIR_ITEM`
            // with guid 0 (`0x4fc0d5`).
            MerchantCall::RepairAll => {
                let (Some(vendor), Some(me)) = (open.vendor, me) else {
                    return;
                };
                let worn = (0..19u8).filter_map(|i| me.0.player_inv_slot(i));
                if !purse.refuses(script, open, me, worn) {
                    debug!("ui_merchant: repair all");
                    let _ = net.0.send(ClientCommand::RepairItem {
                        vendor,
                        item_guid: 0,
                    });
                }
            }
            // The bag's guid as the drop read it, or the player's own for the backpack
            // (`0x4c7c7d`), slot `0xFF` so the server picks, count the fixed 1 (`0x4c7e9c`).
            MerchantCall::BuyIntoBag { bag_guid, entry } => {
                let (Some(vendor), Some((_, me))) = (open.vendor, me_pair) else {
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let bag_guid = bag_guid.unwrap_or(me.0);
                debug!("ui_merchant: bag buy entry {entry} → bag {bag_guid:#x}");
                let result = net.0.send(ClientCommand::BuyItemInSlot {
                    vendor,
                    entry,
                    bag_guid,
                    bag_slot: 0xFF,
                    count: 1,
                });
                record(
                    &trace,
                    if result.is_ok() {
                        Stage::ChannelAdmitted
                    } else {
                        Stage::ChannelDisconnected
                    },
                );
            }
            // The slot's list names the container (`0x4f9f32`): the player's guid for the
            // player's own array (`BAG_PLAYER_INVENTORY`), the bag's for an equipped or a bank
            // bag; the count is the fixed 1.
            MerchantCall::BuyInSlot { bag, slot, entry } => {
                let (Some(vendor), Some((store, guid))) = (open.vendor, me_pair) else {
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let Some((bag_index, bag_slot)) = crate::ui_items::wire_pos_in_target(script.target(), bag, slot) else {
                    record(&trace, Stage::GuardRefused);
                    debug!(
                        "ui_merchant: slot buy to an unaddressable slot ({bag}, {slot}) — ignored"
                    );
                    return;
                };
                let bag_guid = if bag_index == BAG_PLAYER_INVENTORY {
                    Some(guid.0)
                } else if let Some(bank_bag) =
                    bag_index.checked_sub(crate::ui_items::BANK_BAG_SLOT_FIRST)
                {
                    store.0.player_bank_bag_slot(bank_bag).filter(|g| *g != 0)
                } else {
                    store.0.player_inv_slot(bag_index).filter(|g| *g != 0)
                };
                let Some(bag_guid) = bag_guid else {
                    record(&trace, Stage::GuardRefused);
                    debug!("ui_merchant: slot buy into an absent bag {bag} — ignored");
                    return;
                };
                debug!("ui_merchant: slot buy entry {entry} → bag {bag_guid:#x} slot {bag_slot}");
                let result = net.0.send(ClientCommand::BuyItemInSlot {
                    vendor,
                    entry,
                    bag_guid,
                    bag_slot,
                    count: 1,
                });
                record(
                    &trace,
                    if result.is_ok() {
                        Stage::ChannelAdmitted
                    } else {
                        Stage::ChannelDisconnected
                    },
                );
            }
            // The close core needs the latch, which the host lends beside this.
            MerchantCall::Close => {}
        }
    }
}
