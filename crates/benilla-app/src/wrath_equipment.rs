//! The 12340 equipment manager's client state (`CGEquipmentManager`, `WoW/Source/Ui/
//! EquipmentManager.cpp`): the saved gear sets, their load latch and the pending swap. One owner,
//! [`EquipmentManager`]: the login list, the saved-id reply and the use result reach it through
//! [`crate::wrath::wrath_net_drain`]; the four set verbs through [`EquipmentSteps`], applied
//! inside the VM's call; [`feed`] copies it into the VM and signals what the packets raised.
//!
//! The action bar's set buttons: a save of a numbered set signals its slots
//! (`CGActionBar::UpdateEquipmentSet`, `0x5AAA00`) between its copy and its send; the delete's
//! slot clears (`RemoveEquipmentSet`, `0x5AAE20`) run in the VM before the call. Not modelled: the
//! bar's usable refresh on a change of `CanUseEquipmentSets` (`CGActionBar::UpdateUsable`); the
//! VM reads the set gates when `IsUsableAction` asks.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_protocol_wrath::gameplay::equipment_set::{
    EquipmentSet, EquipmentSetSaved, EquipmentSetUseSlot, EQUIPMENT_SET_SLOTS, IGNORED_SLOT_GUID,
    MAX_EQUIPMENT_SETS,
};
use benilla_ui::script::{
    EquipmentItemPlace, EquipmentItemView, EquipmentSetCall, EquipmentSetIcon, EquipmentSetView,
    EquipmentSetsState, ScriptValue, UiScript, DEFAULT_SET_ICON,
};

use crate::net::{ClientCommand, Guid, GuidIndex, NetCommands, ObjectStore, SelfPlayer};
use crate::wrath_inventory::{InventoryAddr, WrathInventory};

/// `UNIT_FLAG_IN_COMBAT`, the bit `UseEquipmentSet` tests (`0x5ADA20`, `m_unitFieldFlags & 0x80000`).
const UNIT_FLAG_IN_COMBAT: u32 = 0x8_0000;

/// The player-array bands `FindItemSlot` answers in (`0x5ADA20`): buyback 74..=85 and the bank
/// vault 39..=66, which the swap refuses (the latter with no banker).
const BUYBACK_FIRST: u8 = 74;
const BUYBACK_LAST: u8 = 85;
const BANK_FIRST: u8 = 39;
const BANK_LAST: u8 = 66;
/// The wire bag bytes of the equipped bags (19..=22) and the bank bags (67..=73).
const BAG_BYTE_FIRST: u8 = 19;
const BANK_BAG_BYTE_FIRST: u8 = 67;
/// `bag = 0xFF`: the player's own inventory array.
const PLAYER_BAG: u8 = 0xFF;

/// One of the three equipment-set packets, as the IO thread forwards it.
#[derive(Clone, Debug, PartialEq)]
pub enum EquipmentSetPacket {
    /// `SMSG_LOAD_EQUIPMENT_SET` (`0x5AF490`).
    List(Vec<EquipmentSet>),
    /// `SMSG_EQUIPMENT_SET_ID` (`0x5AF6C0`).
    Saved(EquipmentSetSaved),
    /// `SMSG_USE_EQUIPMENT_SET_RESULT` (`0x5AF710`).
    UseResult(u8),
}

/// A signal a packet raised, fired by [`feed`] with the VM.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Signal {
    /// `EQUIPMENT_SWAP_FINISHED`: success, and the pending set's name.
    SwapFinished { ok: bool, name: Option<String> },
    /// `UI_ERROR_MESSAGE` with `EQUIPMENT_MANAGER_BAGS_FULL` (result 4).
    BagsFull,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedSet {
    set: EquipmentSet,
    /// `m_isNew`: created here and not yet answered by `SMSG_EQUIPMENT_SET_ID`; a save over it is
    /// refused until then (`SaveSet`, `0x5ADE50`).
    is_new: bool,
}

/// `CGEquipmentManager`'s state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EquipmentManager {
    sets: Vec<OwnedSet>,
    /// `m_loaded` (`0xC1E955`).
    loaded: bool,
    /// `m_swapPending` (`0xC1E956`).
    swap_pending: bool,
    /// `m_pendingSetID` (`0xC1E958`).
    pending_set_id: u32,
    signals: Vec<Signal>,
    /// Bumped on every change the VM reads.
    generation: u64,
}

impl EquipmentManager {
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// The VM's copy.
    pub(crate) fn state(&self) -> EquipmentSetsState {
        EquipmentSetsState {
            loaded: self.loaded,
            swap_pending: self.swap_pending,
            sets: self
                .sets
                .iter()
                .map(|s| EquipmentSetView {
                    id: s.set.id,
                    name: s.set.name.clone(),
                    icon: s.set.icon.clone(),
                    items: s.set.items,
                })
                .collect(),
        }
    }

    fn by_name(&self, name: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.set.name == name)
    }

    fn by_id(&self, id: u32) -> Option<usize> {
        self.sets.iter().position(|s| s.set.id == id)
    }

    /// `GetNextFreeSetID` (`0x5AE530`): the lowest id 0..=9 no set holds.
    fn next_free_id(&self) -> Option<u32> {
        (0..MAX_EQUIPMENT_SETS as u32).find(|&id| self.by_id(id).is_none())
    }

    /// One packet the IO thread decoded.
    pub(crate) fn apply_packet(&mut self, packet: EquipmentSetPacket) {
        match packet {
            // `0x5AF490`: a list of the count already loaded is skipped whole; else it replaces
            // the list. No signal: the dialog reads the list when shown.
            EquipmentSetPacket::List(sets) => {
                if self.loaded && sets.len() == self.sets.len() {
                    return;
                }
                self.sets = sets
                    .into_iter()
                    .take(MAX_EQUIPMENT_SETS)
                    .map(|set| OwnedSet { set, is_new: false })
                    .collect();
                self.loaded = true;
                self.changed();
            }
            // `OnEquipmentSetSaved` (`0x5AE760`): the server's guid settles a set; guid 0
            // refuses it and the set goes.
            EquipmentSetPacket::Saved(saved) => {
                let Some(at) = self.by_id(saved.id) else {
                    return;
                };
                if saved.guid != 0 {
                    self.sets[at].set.guid = saved.guid;
                    self.sets[at].is_new = false;
                } else {
                    self.sets.remove(at);
                }
                self.changed();
            }
            // `0x5AF710`: the swap ends; nonzero is a failure, 4 the bags-full one.
            EquipmentSetPacket::UseResult(result) => {
                self.swap_pending = false;
                let name = self
                    .by_id(self.pending_set_id)
                    .map(|at| self.sets[at].set.name.clone());
                self.signals.push(Signal::SwapFinished {
                    ok: result == 0,
                    name,
                });
                if result == 4 {
                    self.signals.push(Signal::BagsFull);
                }
                self.pending_set_id = 0;
                self.changed();
            }
        }
    }

    /// `CGEquipmentManager::EnterWorld` (`0x5AD690`): no swap is pending in a new world.
    pub(crate) fn enter_world(&mut self) {
        if self.swap_pending || self.pending_set_id != 0 {
            self.swap_pending = false;
            self.pending_set_id = 0;
            self.changed();
        }
    }

    /// `CGEquipmentManager::ShutdownGame` (`0x5AE1B0`): the session's end drops the list.
    pub(crate) fn clear(&mut self) {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            generation,
            ..Self::default()
        };
    }
}

/// Where the swap and the location getter find an item, read off the inventory owners.
struct Inventory<'a> {
    me: &'a ObjectStore,
    wrath: Option<&'a WrathInventory>,
    wrath_target: bool,
    index: Option<&'a GuidIndex>,
    stores: &'a Query<'a, 'a, &'static ObjectStore>,
    banker: bool,
}

/// A held item's place: an index in the player's own array, or a slot of a container.
enum Found {
    Player(u8),
    Bag { bag: u8, slot: u8 },
    BankBag { bag: u8, slot: u8 },
    Nowhere,
}

impl Inventory<'_> {
    fn view(&self) -> crate::wrath_inventory::PlayerInventory<'_> {
        crate::wrath_inventory::PlayerInventory::new(&self.me.0, self.wrath, self.wrath_target)
    }

    fn store(&self, guid: u64) -> Option<&ObjectStore> {
        let entity = *self.index?.0.get(&guid)?;
        self.stores.get(entity).ok()
    }

    fn equipped(&self, slot: u8) -> u64 {
        self.view().equipment(slot)
    }

    /// `CGPlayer_C::FindItemSlot` on the player, then on each container, as `0x5ADA20` walks
    /// them: the equipped bags, then the bank bags only while a banker is open.
    fn find(&self, guid: u64) -> Found {
        if let Some(addr) = self.wrath.and_then(|w| w.addr_of(guid)) {
            let index = match addr {
                InventoryAddr::Equipment(i) => i,
                InventoryAddr::BagSlot(i) => BAG_BYTE_FIRST + i,
                InventoryAddr::Backpack(i) => 23 + i,
                InventoryAddr::Bank(i) => BANK_FIRST + i,
                InventoryAddr::BankBag(i) => BANK_BAG_BYTE_FIRST + i,
                InventoryAddr::Buyback(i) => BUYBACK_FIRST + i,
                InventoryAddr::Keyring(i) => 86 + i,
            };
            return Found::Player(index);
        }
        let view = self.view();
        let in_bag = |bag_guid: u64| -> Option<u8> {
            let bag = self.store(bag_guid)?;
            let slots = bag.0.container_num_slots().unwrap_or(0).min(36) as u8;
            (0..slots).find(|&k| bag.0.container_slot(k) == Some(guid))
        };
        for b in 0..4u8 {
            if let Some(slot) = in_bag(view.bag_slot(b)) {
                return Found::Bag { bag: b, slot };
            }
        }
        if self.banker {
            for c in 0..view.bank_bag_capacity() {
                if let Some(slot) = in_bag(view.bank_bag(c)) {
                    return Found::BankBag { bag: c, slot };
                }
            }
        }
        Found::Nowhere
    }
}

/// The equipment verbs' engine half: the owner, the sends and the inventory they read.
#[derive(SystemParam)]
pub(crate) struct EquipmentSteps<'w, 's> {
    manager: Option<ResMut<'w, EquipmentManager>>,
    commands: Option<Res<'w, NetCommands>>,
    me: Query<'w, 's, (&'static ObjectStore, Option<&'static Guid>), With<SelfPlayer>>,
    inventory: Option<Res<'w, WrathInventory>>,
    index: Option<Res<'w, GuidIndex>>,
    stores: Query<'w, 's, &'static ObjectStore>,
    bank: Option<Res<'w, crate::ui_bank::BankOpen>>,
    items: Option<Res<'w, crate::items::Items>>,
    displays: Option<Res<'w, crate::entities::ItemDisplays>>,
}

impl EquipmentSteps<'_, '_> {
    fn send(&self, command: ClientCommand) {
        if let Some(net) = self.commands.as_deref() {
            let _ = net.0.send(command);
        }
    }

    fn with_inventory<R>(&self, wrath_target: bool, f: impl FnOnce(&Inventory) -> R) -> Option<R> {
        let (me, _) = self.me.single().ok()?;
        let inv = Inventory {
            me,
            wrath: self.inventory.as_deref(),
            wrath_target,
            index: self.index.as_deref(),
            stores: &self.stores,
            banker: self.bank.as_deref().is_some_and(|b| b.banker.is_some()),
        };
        Some(f(&inv))
    }

    /// The inventory art of the item worn in `slot` (`Script_SaveEquipmentSet`, `0x5AF9C0`'s
    /// negative index): the visible item's display, else the worn item object's.
    fn slot_icon(&self, slot: u8, wrath_target: bool) -> Option<String> {
        let items = self.items.as_deref()?;
        let entry = self.with_inventory(wrath_target, |inv| {
            inv.me
                .0
                .player_visible_item_entry(slot)
                .filter(|&e| e != 0)
                .or_else(|| inv.store(inv.equipped(slot))?.0.object_entry())
        })??;
        let display = items
            .template(entry, 0, crate::query_cache::Ask::Read)?
            .display_info_id;
        crate::ui_items::item_icon(self.displays.as_deref(), display)
    }

    /// The held item `guid` a set names, for the getters (`0x5AEF90`, `0x5AE860`).
    pub(crate) fn item(&self, guid: u64, wrath_target: bool) -> Option<EquipmentItemView> {
        self.with_inventory(wrath_target, |inv| {
            let entry = inv.store(guid)?.0.object_entry().unwrap_or(0);
            let place = match inv.find(guid) {
                Found::Player(i) if i < 19 => EquipmentItemPlace::Equipped(i),
                Found::Player(i) if (23..39).contains(&i) => EquipmentItemPlace::Backpack(i - 23),
                Found::Player(i) if (BANK_FIRST..=BANK_LAST).contains(&i) && inv.banker => {
                    EquipmentItemPlace::Bank(i - BANK_FIRST)
                }
                Found::Player(i) if (BUYBACK_FIRST..=BUYBACK_LAST).contains(&i) => {
                    EquipmentItemPlace::Buyback
                }
                Found::Bag { bag, slot } => EquipmentItemPlace::Bag { bag, slot },
                Found::BankBag { bag, slot } => EquipmentItemPlace::BankBag { bag, slot },
                _ => EquipmentItemPlace::Elsewhere,
            };
            Some(EquipmentItemView { entry, place })
        })
        .flatten()
    }

    /// `SaveSet` (`0x5ADE50`) over set `at`: refused while the set awaits its id; the name and
    /// icon copied; each slot the worn item's guid, or the ignored marker; the send; the signal.
    fn save_set(
        &mut self,
        script: &mut UiScript,
        at: usize,
        name: &str,
        icon: &str,
        ignored: &[bool; EQUIPMENT_SET_SLOTS],
    ) {
        let wrath_target = script.target() == benilla_ui::script::UiTarget::Wrath335;
        let worn: Option<[u64; EQUIPMENT_SET_SLOTS]> = self.with_inventory(wrath_target, |inv| {
            std::array::from_fn(|slot| inv.equipped(slot as u8))
        });
        let Some(manager) = self.manager.as_deref_mut() else {
            return;
        };
        let owned = &mut manager.sets[at];
        if owned.is_new {
            return;
        }
        owned.set.name = name.to_string();
        owned.set.icon = icon.to_string();
        let Some(worn) = worn else {
            manager.changed();
            return;
        };
        for (slot, item) in owned.set.items.iter_mut().enumerate() {
            *item = if ignored[slot] {
                IGNORED_SLOT_GUID
            } else {
                worn[slot]
            };
        }
        let set = owned.set.clone();
        manager.changed();
        let state = manager.state();
        script.set_equipment_sets(state);
        // `0x5ADE50`: a set the server has numbered repaints its bar slots before the send.
        if set.guid != 0 {
            script.signal_equipment_set_slots(set.id);
        }
        self.send(ClientCommand::EquipmentSetSave(Box::new(set)));
        script.fire_event("EQUIPMENT_SETS_CHANGED", Vec::new());
    }

    /// One set verb, inside the VM's call.
    pub(crate) fn apply(&mut self, script: &mut UiScript, call: EquipmentSetCall) {
        if self.manager.is_none() {
            return;
        }
        match call {
            // `0x5AF9C0`: over a set of that name (`SaveSetByName` `0x5AF8E0`), else a new one
            // (`CreateEquipmentSet` `0x5AF7E0`), which takes the lowest free id, saves, then
            // waits for its id.
            EquipmentSetCall::Save {
                name,
                icon,
                ignored,
            } => {
                let wrath_target = script.target() == benilla_ui::script::UiTarget::Wrath335;
                let icon = match icon {
                    EquipmentSetIcon::Named(n) => Some(n),
                    EquipmentSetIcon::Slot(slot) => self.slot_icon(slot, wrath_target),
                }
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| DEFAULT_SET_ICON.to_string());
                let Some(manager) = self.manager.as_deref_mut() else {
                    return;
                };
                if let Some(at) = manager.by_name(&name) {
                    self.save_set(script, at, &name, &icon, &ignored);
                    return;
                }
                if manager.sets.len() >= MAX_EQUIPMENT_SETS {
                    return;
                }
                let Some(id) = manager.next_free_id() else {
                    return;
                };
                manager.sets.push(OwnedSet {
                    set: EquipmentSet {
                        guid: 0,
                        id,
                        name: name.clone(),
                        icon: icon.clone(),
                        items: [0; EQUIPMENT_SET_SLOTS],
                    },
                    is_new: false,
                });
                let at = manager.sets.len() - 1;
                self.save_set(script, at, &name, &icon, &ignored);
                if let Some(manager) = self.manager.as_deref_mut() {
                    manager.sets[at].is_new = true;
                    manager.changed();
                    script.set_equipment_sets(manager.state());
                }
            }
            // `0x5AE260`: the set goes, the delete is sent with its guid, the list changed.
            EquipmentSetCall::Delete { name } => {
                let Some(manager) = self.manager.as_deref_mut() else {
                    return;
                };
                let Some(at) = manager.by_name(&name) else {
                    return;
                };
                let removed = manager.sets.remove(at);
                manager.changed();
                let state = manager.state();
                self.send(ClientCommand::EquipmentSetDelete {
                    set_guid: removed.set.guid,
                });
                script.set_equipment_sets(state);
                script.fire_event("EQUIPMENT_SETS_CHANGED", Vec::new());
            }
            // `ModifyEquipmentSet` (`0x5AF400`): refused onto a name in use; the name taken, the
            // set saved under it with its own icon, and the list changed once more.
            EquipmentSetCall::Rename { from, to, ignored } => {
                let Some(manager) = self.manager.as_deref_mut() else {
                    return;
                };
                if manager.by_name(&to).is_some() {
                    return;
                }
                let Some(at) = manager.by_name(&from) else {
                    return;
                };
                manager.sets[at].set.name = to.clone();
                let icon = manager.sets[at].set.icon.clone();
                manager.changed();
                script.set_equipment_sets(manager.state());
                self.save_set(script, at, &to, &icon, &ignored);
                script.fire_event("EQUIPMENT_SETS_CHANGED", Vec::new());
            }
            EquipmentSetCall::Use { name } => self.use_set(script, &name),
        }
    }

    /// `UseEquipmentSet` (`0x5ADA20`) past the VM's gates: each slot's item and where it is
    /// (in combat only the weapons move), the warnings, the send, the pending swap and
    /// `EQUIPMENT_SWAP_PENDING`.
    fn use_set(&mut self, script: &mut UiScript, name: &str) {
        let wrath_target = script.target() == benilla_ui::script::UiTarget::Wrath335;
        let Some(set) = self
            .manager
            .as_deref()
            .and_then(|m| m.by_name(name).map(|at| m.sets[at].set.clone()))
        else {
            return;
        };
        let mut warnings: Vec<&'static str> = Vec::new();
        let slots = self.with_inventory(wrath_target, |inv| {
            let in_combat = inv.me.0.unit_flags() & UNIT_FLAG_IN_COMBAT != 0;
            let mut combat_warned = false;
            let mut missing_warned = false;
            let mut out = [EquipmentSetUseSlot::default(); EQUIPMENT_SET_SLOTS];
            for (slot, row) in out.iter_mut().enumerate() {
                let guid = set.items[slot];
                *row = EquipmentSetUseSlot {
                    item: IGNORED_SLOT_GUID,
                    bag: 0,
                    slot: 0,
                };
                let weapon = matches!(slot, 15..=17);
                if (in_combat && !weapon) || guid == IGNORED_SLOT_GUID {
                    if in_combat
                        && !combat_warned
                        && guid != IGNORED_SLOT_GUID
                        && guid != inv.equipped(slot as u8)
                    {
                        warnings.push("EQUIPMENT_MANAGER_COMBAT_SWAP");
                        combat_warned = true;
                    }
                } else if guid == 0 {
                    row.item = 0;
                } else {
                    if inv.store(guid).is_some() {
                        match inv.find(guid) {
                            Found::Player(i) if (BUYBACK_FIRST..=BUYBACK_LAST).contains(&i) => {}
                            Found::Player(i)
                                if (BANK_FIRST..=BANK_LAST).contains(&i) && !inv.banker => {}
                            Found::Player(i) => {
                                *row = EquipmentSetUseSlot {
                                    item: guid,
                                    bag: PLAYER_BAG,
                                    slot: i,
                                };
                            }
                            Found::Bag { bag, slot } => {
                                *row = EquipmentSetUseSlot {
                                    item: guid,
                                    bag: BAG_BYTE_FIRST + bag,
                                    slot,
                                };
                            }
                            Found::BankBag { bag, slot } => {
                                *row = EquipmentSetUseSlot {
                                    item: guid,
                                    bag: BANK_BAG_BYTE_FIRST + bag,
                                    slot,
                                };
                            }
                            Found::Nowhere => {}
                        }
                    }
                    if row.item == IGNORED_SLOT_GUID && !missing_warned {
                        warnings.push("EQUIPMENT_MANAGER_MISSING_ITEM");
                        missing_warned = true;
                    }
                }
            }
            out
        });
        let Some(slots) = slots else {
            return;
        };
        for key in warnings {
            let format: String = script
                .lua()
                .globals()
                .get(key)
                .unwrap_or_else(|_| key.to_string());
            script.fire_event(
                "UI_ERROR_MESSAGE",
                vec![ScriptValue::Str(format.replacen("%s", &set.name, 1))],
            );
        }
        self.send(ClientCommand::EquipmentSetUse(Box::new(slots)));
        let Some(manager) = self.manager.as_deref_mut() else {
            return;
        };
        manager.swap_pending = true;
        manager.pending_set_id = set.id;
        manager.changed();
        script.set_equipment_sets(manager.state());
        script.fire_event("EQUIPMENT_SWAP_PENDING", Vec::new());
    }
}

/// Copy the owner into the VM when it moved or the VM is new, then signal what its packets
/// raised: `EQUIPMENT_SWAP_FINISHED` and the bags-full line (`0x5AF710`).
pub(crate) fn feed(
    mut vm: crate::script_calls::VmWith<Option<ResMut<'static, EquipmentManager>>>,
    mut memo: Local<crate::ui_script::VmMemo<Option<u64>>>,
) {
    vm.feed(|script, manager, fires| {
        let (Some(script), Some(mut manager)) = (script, manager) else {
            return;
        };
        let seen = memo.get(script);
        if *seen != Some(manager.generation) {
            *seen = Some(manager.generation);
            script.set_equipment_sets(manager.state());
        }
        for signal in std::mem::take(&mut manager.signals) {
            match signal {
                Signal::SwapFinished { ok, name } => fires.post(
                    "EQUIPMENT_SWAP_FINISHED",
                    vec![
                        ScriptValue::Bool(ok),
                        name.map_or(ScriptValue::Nil, ScriptValue::Str),
                    ],
                ),
                Signal::BagsFull => {
                    let text: String = script
                        .lua()
                        .globals()
                        .get("EQUIPMENT_MANAGER_BAGS_FULL")
                        .unwrap_or_else(|_| "EQUIPMENT_MANAGER_BAGS_FULL".to_string());
                    fires.post("UI_ERROR_MESSAGE", vec![ScriptValue::Str(text)]);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests;
