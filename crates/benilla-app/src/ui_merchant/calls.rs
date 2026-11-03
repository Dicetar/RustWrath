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
    /// Current Wrath scope for scoped merchant interactions.
    wrath: Option<Res<'w, crate::wrath::WrathWorldView>>,
}

impl MerchantSteps<'_, '_> {
    fn trace_scope(&self) -> Option<(u64, u64)> {
        self.wrath
            .as_ref()
            .map(|view| (view.session_epoch, view.world_generation))
    }

    fn tracked(
        trace: &Option<crate::quest_trace::Interaction>,
        command: ClientCommand,
    ) -> ClientCommand {
        match trace {
            Some(trace) => ClientCommand::tracked(Some((*trace).clone()), true, command),
            None => command,
        }
    }

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
        let scope = self.trace_scope();
        let default_op = match &call {
            MerchantCall::Sell { .. } => Some(Operation::Sell),
            MerchantCall::Buy { .. }
            | MerchantCall::BuyIntoBag { .. }
            | MerchantCall::BuyInSlot { .. }
            | MerchantCall::Buyback(_)
            | MerchantCall::RepairAll => Some(Operation::Buy),
            MerchantCall::Close => None,
        };

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
            trace,
            ..
        } = self;
        let make_trace = |op: Operation,
                          vendor: Option<u64>,
                          item: Option<u32>,
                          count: Option<u32>,
                          slot: Option<u32>| {
            trace.as_ref().map(|trace| {
                trace.begin_with_details(scope, None, vendor, item, count, slot, op)
            })
        };
        let open_vendor = open.as_deref().and_then(|open| open.vendor);
        let guard_trace = |op: Option<Operation>| {
            op.and_then(|op| make_trace(op, open_vendor, None, None, None))
        };
        let Some(open) = open.as_deref() else {
            let trace = guard_trace(default_op);
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
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let Some(item) = open.row_at(row).filter(|it| it.slot != 0) else {
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    debug!("ui_merchant: BuyMerchantItem({row}) out of range — ignored");
                    return;
                };
                let count = quantity.clamp(1, u32::from(u8::MAX)) as u8;
                debug!("ui_merchant: buy row {row} (entry {} ×{count})", item.entry);
                let trace = make_trace(
                    Operation::Buy,
                    Some(vendor),
                    Some(item.entry),
                    Some(u32::from(count)),
                    Some(row),
                );
                let command = Self::tracked(
                    &trace,
                    ClientCommand::BuyItem {
                        vendor,
                        entry: item.entry,
                        count,
                    },
                );
                let result = net.0.send(command);
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
                let trace = make_trace(
                    Operation::Sell,
                    (vendor != 0).then_some(vendor),
                    None,
                    Some(u32::from(count)),
                    None,
                );
                let command = Self::tracked(
                    &trace,
                    ClientCommand::SellItem {
                        vendor,
                        item_guid,
                        count,
                    },
                );
                let result = net.0.send(command);
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
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let order = buyback_order(&store.0);
                match index.checked_sub(1).and_then(|i| order.get(i as usize)) {
                    Some(&idx) => {
                        let slot = BUYBACK_SLOT_FIRST + u32::from(idx);
                        debug!("ui_merchant: buyback list index {index} → absolute slot {slot}");
                        let trace = make_trace(
                            Operation::Buy,
                            Some(vendor),
                            None,
                            Some(1),
                            Some(slot),
                        );
                        let command =
                            Self::tracked(&trace, ClientCommand::BuybackItem { vendor, slot });
                        let result = net.0.send(command);
                        record(
                            &trace,
                            if result.is_ok() {
                                Stage::ChannelAdmitted
                            } else {
                                Stage::ChannelDisconnected
                            },
                        );
                    }
                    None => {
                        let trace = guard_trace(Some(Operation::Buy));
                        record(&trace, Stage::GuardRefused);
                        debug!("ui_merchant: BuybackItem({index}) out of range — ignored");
                    }
                }
            }
            // With a player, the purse test over the equipped slots 0-18 only (`0x4fc026`-
            // `0x4fc063`), not the bags `GetRepairAllCost` also sweeps; then `CMSG_REPAIR_ITEM`
            // with guid 0 (`0x4fc0d5`).
            MerchantCall::RepairAll => {
                let (Some(vendor), Some(me)) = (open.vendor, me) else {
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let worn = (0..19u8).filter_map(|i| me.0.player_inv_slot(i));
                if !purse.refuses(script, open, me, worn) {
                    debug!("ui_merchant: repair all");
                    let trace = make_trace(Operation::Buy, Some(vendor), None, None, None);
                    let command = Self::tracked(
                        &trace,
                        ClientCommand::RepairItem {
                            vendor,
                            item_guid: 0,
                        },
                    );
                    let result = net.0.send(command);
                    record(
                        &trace,
                        if result.is_ok() {
                            Stage::ChannelAdmitted
                        } else {
                            Stage::ChannelDisconnected
                        },
                    );
                } else {
                    let trace = make_trace(Operation::Buy, Some(vendor), None, None, None);
                    record(&trace, Stage::GuardRefused);
                }
            }
            // The bag's guid as the drop read it, or the player's own for the backpack
            // (`0x4c7c7d`), slot `0xFF` so the server picks, count the fixed 1 (`0x4c7e9c`).
            MerchantCall::BuyIntoBag { bag_guid, entry } => {
                let (Some(vendor), Some((_, me))) = (open.vendor, me_pair) else {
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let bag_guid = bag_guid.unwrap_or(me.0);
                debug!("ui_merchant: bag buy entry {entry} → bag {bag_guid:#x}");
                let trace = make_trace(Operation::Buy, Some(vendor), Some(entry), Some(1), None);
                let command = Self::tracked(
                    &trace,
                    ClientCommand::BuyItemInSlot {
                        vendor,
                        entry,
                        bag_guid,
                        bag_slot: 0xFF,
                        count: 1,
                    },
                );
                let result = net.0.send(command);
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
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    return;
                };
                let Some((bag_index, bag_slot)) = crate::ui_items::wire_pos_in_target(script.target(), bag, slot) else {
                    let trace = guard_trace(Some(Operation::Buy));
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
                    let trace = guard_trace(Some(Operation::Buy));
                    record(&trace, Stage::GuardRefused);
                    debug!("ui_merchant: slot buy into an absent bag {bag} — ignored");
                    return;
                };
                debug!("ui_merchant: slot buy entry {entry} → bag {bag_guid:#x} slot {bag_slot}");
                let trace = make_trace(Operation::Buy, Some(vendor), Some(entry), Some(1), None);
                let command = Self::tracked(
                    &trace,
                    ClientCommand::BuyItemInSlot {
                        vendor,
                        entry,
                        bag_guid,
                        bag_slot,
                        count: 1,
                    },
                );
                let result = net.0.send(command);
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
