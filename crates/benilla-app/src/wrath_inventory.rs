//! The lossless Wrath player-inventory owner.
//!
//! The 3.3.5 client's player array is wider than the 1.12 descriptor the established item/UI
//! owners were built on: 28 bank slots against 24, seven bank bags against six, and the
//! buyback/keyring bands sit five positions later. The donor projection in
//! [`crate::wrath::push_player_inventory_fields`] therefore drops the tail; this module keeps the
//! full 3.3.5 state beside it, with typed addresses and presence.
//!
//! One writer: [`crate::wrath::wrath_apply_world`] feeds a [`WrathFieldWindow`] through
//! [`WrathInventory::apply`]. Readers use [`PlayerInventory`], which prefers this owner and falls
//! back to the donor `ObjectStore` for un-migrated bands. The donor projection stays exactly as
//! wide as it was, so no extra field is written into an occupied donor offset.
//!
//! Presence is explicit: a missing map entry means "never observed", `Some(0)` means the server
//! delivered an empty guid, and a half-delivered guid pair is never assembled.

use std::collections::BTreeMap;

use benilla_protocol::messages::{BAG_PLAYER_INVENTORY, SLOT_BAG_FIRST, SLOT_PACK_FIRST};
use benilla_protocol::ObjectFields;
use benilla_protocol_wrath::fields;

/// The dense player window the digest carries (`0..PLAYER_END` = `0..1326`).
pub(crate) const PLAYER_WINDOW_END: u16 = fields::PLAYER_END;

/// 3.3.5 capacities (`WoW/Common/ComponentCore/Types.h:100-117`, AzerothCore `Player.h:697-717`).
pub(crate) const WRATH_EQUIPMENT_SLOTS: u8 = 19;
pub(crate) const WRATH_BAG_SLOTS: u8 = 4;
pub(crate) const WRATH_PACK_SLOTS: u8 = 16;
pub(crate) const WRATH_BANK_SLOTS: u8 = 28;
pub(crate) const WRATH_BANK_BAG_SLOTS: u8 = 7;
pub(crate) const WRATH_BUYBACK_SLOTS: u8 = 12;
pub(crate) const WRATH_KEYRING_SLOTS: u8 = 32;

/// The 3.3.5 player-array bases; donor projections use their own bands.
pub(crate) const WRATH_BANK_SLOT_FIRST: u8 = 39;
pub(crate) const WRATH_BANK_BAG_SLOT_FIRST: u8 = 67;
pub(crate) const WRATH_BUYBACK_SLOT_FIRST: u8 = 74;
pub(crate) const WRATH_KEYRING_SLOT_FIRST: u8 = 86;

/// `PLAYER_BYTES_2` byte 2: bank-bag slots bought, 0..=7 (`CGBankInfo_Scripts.cpp:37-44`).
pub(crate) const PLAYER_BANK_BAGS_PURCHASED_BYTE: u32 = 2;

/// A dense 3.3.5 field window plus the bit per dword the server actually delivered.
///
/// The reducer keeps a delivered mask per object; `WrathEntity` previously collapsed it to
/// `unwrap_or(0)`, which made an explicit clear indistinguishable from "never sent". This type
/// keeps both: [`Self::get`] returns `Some(0)` for a delivered zero and `None` for an absent
/// field, and [`Self::guid`] refuses to assemble a guid from half a pair.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WrathFieldWindow {
    values: Vec<u32>,
    /// One bit per dword; bit `i` set means `values[i]` was delivered at least once.
    delivered: Vec<u64>,
}

impl WrathFieldWindow {
    pub(crate) fn new(values: Vec<u32>, delivered: Vec<u64>) -> Self {
        Self { values, delivered }
    }

    /// A window where every value counts as delivered. Used by fixtures and by callers that hold
    /// a dense legacy window; production digests go through [`Self::new`].
    pub(crate) fn from_dense(values: Vec<u32>) -> Self {
        let words = values.len().div_ceil(64);
        let mut delivered = vec![0u64; words];
        for word in 0..words {
            let bits = (values.len() - word * 64).min(64);
            delivered[word] = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
        }
        Self { values, delivered }
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(crate) fn is_delivered(&self, index: u16) -> bool {
        let index = usize::from(index);
        index < self.values.len()
            && self
                .delivered
                .get(index / 64)
                .is_some_and(|word| word & (1u64 << (index % 64)) != 0)
    }

    /// The delivered dword, `Some(0)` included; `None` when the field was never sent.
    pub(crate) fn get(&self, index: u16) -> Option<u32> {
        self.is_delivered(index)
            .then(|| self.values[usize::from(index)])
    }

    /// The dword for the donor projection: absent fields read 0, matching the create-shaped
    /// `ObjectFields` the projection inserts.
    pub(crate) fn word_or_zero(&self, index: u16) -> u32 {
        self.values.get(usize::from(index)).copied().unwrap_or(0)
    }

    /// A guid from two adjacent dwords, only when both were delivered. A delivered zero pair is
    /// `Some(0)` (an explicit clear), not `None`.
    pub(crate) fn guid(&self, lo: u16) -> Option<u64> {
        let low = self.get(lo)?;
        let high = self.get(lo.checked_add(1)?)?;
        Some(u64::from(low) | (u64::from(high) << 32))
    }

    /// The `PLAYER_WINDOW_END` window a Wrath player digest carries.
    pub(crate) fn player(values: Vec<u32>, delivered: Vec<u64>) -> Option<Self> {
        (values.len() >= usize::from(PLAYER_WINDOW_END)).then(|| Self::new(values, delivered))
    }

    /// Build a window from a reducer object's `field`/`field_delivered` pair.
    pub(crate) fn from_fields(
        end: u16,
        field: impl Fn(u16) -> Option<u32>,
        delivered: impl Fn(u16) -> bool,
    ) -> Self {
        let end = usize::from(end);
        let mut values = Vec::with_capacity(end);
        let mut mask = vec![0u64; end.div_ceil(64)];
        for index in 0..end {
            let index = index as u16;
            values.push(field(index).unwrap_or(0));
            if delivered(index) {
                let at = usize::from(index);
                mask[at / 64] |= 1u64 << (at % 64);
            }
        }
        Self::new(values, mask)
    }
}

/// One typed address in the player's 3.3.5 inventory array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum InventoryAddr {
    /// Worn equipment 0..=18.
    Equipment(u8),
    /// The equipped-bag items (player-array slots 19..=22).
    BagSlot(u8),
    /// The backpack 0..=15.
    Backpack(u8),
    /// The bank vault 0..=27.
    Bank(u8),
    /// The bank-bag items 0..=6 (player-array slots 67..=73).
    BankBag(u8),
    /// Vendor buyback 0..=11.
    Buyback(u8),
    /// Keyring 0..=31.
    Keyring(u8),
}

impl InventoryAddr {
    /// The 3.3.5 player-window dword of this address's guid low word.
    pub(crate) fn field_index(self) -> u16 {
        match self {
            Self::Equipment(i) => fields::PLAYER_FIELD_INV_SLOT_HEAD + 2 * u16::from(i),
            Self::BagSlot(i) => fields::PLAYER_FIELD_INV_SLOT_HEAD + 2 * (19 + u16::from(i)),
            Self::Backpack(i) => fields::PLAYER_FIELD_PACK_SLOT_1 + 2 * u16::from(i),
            Self::Bank(i) => fields::PLAYER_FIELD_BANK_SLOT_1 + 2 * u16::from(i),
            Self::BankBag(i) => fields::PLAYER_FIELD_BANKBAG_SLOT_1 + 2 * u16::from(i),
            Self::Buyback(i) => fields::PLAYER_FIELD_VENDORBUYBACK_SLOT_1 + 2 * u16::from(i),
            Self::Keyring(i) => fields::PLAYER_FIELD_KEYRING_SLOT_1 + 2 * u16::from(i),
        }
    }

    /// The 3.3.5 wire `(bag byte, slot)` for a command naming this player-array slot.
    /// Bank bags move as `CMSG_*_BANK_ITEM` with the bag byte 67..=73; the bag item itself is
    /// addressed through the player inventory (`255`) at the same slot.
    pub(crate) fn to_wire(self) -> (u8, u8) {
        match self {
            Self::Equipment(i) => (BAG_PLAYER_INVENTORY, i),
            Self::BagSlot(i) => (BAG_PLAYER_INVENTORY, SLOT_BAG_FIRST + i),
            Self::Backpack(i) => (BAG_PLAYER_INVENTORY, SLOT_PACK_FIRST + i),
            Self::Bank(i) => (BAG_PLAYER_INVENTORY, WRATH_BANK_SLOT_FIRST + i),
            Self::BankBag(i) => (BAG_PLAYER_INVENTORY, WRATH_BANK_BAG_SLOT_FIRST + i),
            Self::Buyback(i) => (BAG_PLAYER_INVENTORY, WRATH_BUYBACK_SLOT_FIRST + i),
            Self::Keyring(i) => (BAG_PLAYER_INVENTORY, WRATH_KEYRING_SLOT_FIRST + i),
        }
    }

    /// The container id a bank bag's *contents* are addressed by (`CMSG_*` bag byte).
    pub(crate) fn as_bag_byte(self) -> Option<u8> {
        match self {
            Self::BagSlot(i) => Some(SLOT_BAG_FIRST + i),
            Self::BankBag(i) => Some(WRATH_BANK_BAG_SLOT_FIRST + i),
            _ => None,
        }
    }

    /// The Lua bag id for a bag slot (1..=4 equipped, 5..=11 bank bags).
    pub(crate) fn lua_bag_id(self) -> Option<i64> {
        match self {
            Self::BagSlot(i) => Some(i64::from(i) + 1),
            Self::BankBag(i) => Some(i64::from(i) + 5),
            _ => None,
        }
    }

    pub(crate) fn is_bank(self) -> bool {
        matches!(self, Self::Bank(_) | Self::BankBag(_))
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Equipment(_) => "equipment",
            Self::BagSlot(_) => "bag",
            Self::Backpack(_) => "backpack",
            Self::Bank(_) => "bank",
            Self::BankBag(_) => "bank bag",
            Self::Buyback(_) => "buyback",
            Self::Keyring(_) => "keyring",
        }
    }
}

/// Every address the owner tracks, in player-array order.
pub(crate) fn all_addresses() -> impl Iterator<Item = InventoryAddr> {
    (0..WRATH_EQUIPMENT_SLOTS)
        .map(InventoryAddr::Equipment)
        .chain((0..WRATH_BAG_SLOTS).map(InventoryAddr::BagSlot))
        .chain((0..WRATH_PACK_SLOTS).map(InventoryAddr::Backpack))
        .chain((0..WRATH_BANK_SLOTS).map(InventoryAddr::Bank))
        .chain((0..WRATH_BANK_BAG_SLOTS).map(InventoryAddr::BankBag))
        .chain((0..WRATH_BUYBACK_SLOTS).map(InventoryAddr::Buyback))
        .chain((0..WRATH_KEYRING_SLOTS).map(InventoryAddr::Keyring))
}

/// One changed address in a digest: the value before and after (`None` = absent or cleared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InventoryChange {
    pub addr: InventoryAddr,
    pub old: Option<u64>,
    pub new: Option<u64>,
    /// The address was never observed before this digest. A first non-zero observation still
    /// needs its unlock; a first empty observation does not produce a visible event.
    pub first: bool,
}

/// What one digest changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InventoryChanges {
    pub slots: Vec<InventoryChange>,
    pub bank_bags_purchased: Option<u8>,
    pub scope: (u64, u64),
}

impl InventoryChanges {
    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.bank_bags_purchased.is_none()
    }
}

/// The authoritative app-side typed inventory for the active player.
///
/// The map's value is the last delivered guid: `Some(0)` is an explicit clear, a missing key is
/// "never observed". `scope` is the session epoch and world generation the values belong to.
#[derive(Clone, Debug, Default, PartialEq, Eq, bevy::prelude::Resource)]
pub(crate) struct WrathInventory {
    slots: BTreeMap<InventoryAddr, Option<u64>>,
    bank_bags_purchased: Option<u8>,
    scope: (u64, u64),
}

impl WrathInventory {
    pub(crate) fn scope(&self) -> (u64, u64) {
        self.scope
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Adopt one digest's delivered inventory window.
    ///
    /// Only addresses whose *value* changed produce a change entry; a first observation of a zero
    /// guid is presence-only and is not reported as a clear. Absent fields never touch the map, so
    /// a sparse digest cannot silently clear a slot it did not mention.
    pub(crate) fn apply(
        &mut self,
        window: &WrathFieldWindow,
        scope: (u64, u64),
    ) -> InventoryChanges {
        let mut changes = InventoryChanges {
            scope,
            ..InventoryChanges::default()
        };
        for addr in all_addresses() {
            let field = addr.field_index();
            let Some(guid) = window.guid(field) else {
                continue;
            };
            let new = Some(guid);
            let old_entry = self.slots.get(&addr).copied();
            let first = old_entry.is_none();
            let old = old_entry.flatten();
            // A first observation is a presence fact, not a change: an empty slot is not a
            // clear event, and a delivered guid is the initial value. Later observations
            // report every value change, including a delivered zero clear.
            let changed = if first { guid != 0 } else { old != new };
            if changed {
                changes.slots.push(InventoryChange {
                    addr,
                    old,
                    new,
                    first,
                });
            }
            self.slots.insert(addr, new);
        }
        if let Some(bytes) = window.get(fields::PLAYER_BYTES_2) {
            let purchased = ((bytes >> (8 * PLAYER_BANK_BAGS_PURCHASED_BYTE)) & 0xff) as u8;
            if self.bank_bags_purchased != Some(purchased) {
                changes.bank_bags_purchased = Some(purchased);
                self.bank_bags_purchased = Some(purchased);
            }
        }
        self.scope = scope;
        changes
    }

    /// The observed guid, `None` for absent or an explicit clear.
    pub(crate) fn guid(&self, addr: InventoryAddr) -> Option<u64> {
        self.slots.get(&addr).copied().flatten()
    }

    /// `None` = never observed; `Some(None)` = observed clear; `Some(Some(guid))` = observed item.
    pub(crate) fn presence(&self, addr: InventoryAddr) -> Option<Option<u64>> {
        self.slots.get(&addr).copied()
    }

    pub(crate) fn bank_bags_purchased(&self) -> Option<u8> {
        self.bank_bags_purchased
    }

    /// The typed address currently holding `guid`, if any. Used by the item watcher so an item in
    /// bank slot 25 or bank bag 7 still raises its bank signal.
    pub(crate) fn addr_of(&self, guid: u64) -> Option<InventoryAddr> {
        (guid != 0)
            .then(|| {
                self.slots
                    .iter()
                    .find_map(|(&addr, &value)| (value == Some(guid)).then_some(addr))
            })
            .flatten()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (InventoryAddr, Option<u64>)> + '_ {
        self.slots.iter().map(|(&addr, &value)| (addr, value))
    }
}

/// The donor projection's capacity, or the Wrath owner's when the selected client is Wrath.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayerInventory<'a> {
    store: &'a ObjectFields,
    wrath: Option<&'a WrathInventory>,
    wrath_target: bool,
}

impl<'a> PlayerInventory<'a> {
    pub(crate) fn new(
        store: &'a ObjectFields,
        wrath: Option<&'a WrathInventory>,
        wrath_target: bool,
    ) -> Self {
        Self {
            store,
            wrath,
            wrath_target,
        }
    }

    /// The donor descriptor, for callers that still need a 1.12-only field.
    pub(crate) fn store(&self) -> &'a ObjectFields {
        self.store
    }

    pub(crate) fn wrath(&self) -> Option<&'a WrathInventory> {
        self.wrath
    }

    /// The typed value when the owner observed the address. An observed clear (`Some(0)`) is a
    /// value, not a miss: it must not fall through to the donor projection, which could still
    /// hold the slot's previous guid. Only a never-observed address falls back.
    fn typed(&self, addr: InventoryAddr) -> Option<u64> {
        self.wrath
            .and_then(|inv| inv.presence(addr).map(|value| value.unwrap_or(0)))
    }

    pub(crate) fn equipment(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::Equipment(i))
            .or_else(|| self.store.player_inv_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn bag_slot(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::BagSlot(i))
            .or_else(|| self.store.player_inv_slot(19 + i))
            .unwrap_or(0)
    }

    pub(crate) fn backpack(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::Backpack(i))
            .or_else(|| self.store.player_pack_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn bank(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::Bank(i))
            .or_else(|| self.store.player_bank_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn bank_bag(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::BankBag(i))
            .or_else(|| self.store.player_bank_bag_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn buyback(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::Buyback(i))
            .or_else(|| self.store.player_buyback_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn keyring(&self, i: u8) -> u64 {
        self.typed(InventoryAddr::Keyring(i))
            .or_else(|| self.store.player_keyring_slot(i))
            .unwrap_or(0)
    }

    pub(crate) fn bank_capacity(&self) -> u8 {
        if self.wrath_target {
            WRATH_BANK_SLOTS
        } else {
            crate::ui_items::BANK_SLOTS
        }
    }

    pub(crate) fn bank_bag_capacity(&self) -> u8 {
        if self.wrath_target {
            WRATH_BANK_BAG_SLOTS
        } else {
            crate::ui_items::BANK_BAGS
        }
    }

    pub(crate) fn keyring_capacity(&self) -> u8 {
        if self.wrath_target {
            WRATH_KEYRING_SLOTS
        } else {
            crate::ui_items::KEYRING_SLOTS
        }
    }

    pub(crate) fn bank_slot_first(&self) -> u8 {
        WRATH_BANK_SLOT_FIRST
    }

    pub(crate) fn bank_bag_slot_first(&self) -> u8 {
        if self.wrath_target {
            WRATH_BANK_BAG_SLOT_FIRST
        } else {
            crate::ui_items::BANK_BAG_SLOT_FIRST
        }
    }

    pub(crate) fn keyring_slot_first(&self) -> u8 {
        if self.wrath_target {
            WRATH_KEYRING_SLOT_FIRST
        } else {
            crate::ui_items::KEYRING_SLOT_FIRST
        }
    }

    pub(crate) fn bank_bags_purchased(&self) -> u8 {
        self.wrath
            .and_then(WrathInventory::bank_bags_purchased)
            .or_else(|| self.store.player_bank_bag_slots_purchased())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK: u16 = fields::PLAYER_FIELD_BANK_SLOT_1;
    const BANK_BAG: u16 = fields::PLAYER_FIELD_BANKBAG_SLOT_1;
    const BUYBACK: u16 = fields::PLAYER_FIELD_VENDORBUYBACK_SLOT_1;
    const KEYRING: u16 = fields::PLAYER_FIELD_KEYRING_SLOT_1;
    const BYTES_2: u16 = fields::PLAYER_BYTES_2;

    /// A window with `(lo, guid)` pairs delivered; every other dword is absent.
    fn window(pairs: &[(u16, u64)]) -> WrathFieldWindow {
        let mut values = vec![0u32; usize::from(PLAYER_WINDOW_END)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        for &(lo, guid) in pairs {
            let (low, high) = (lo, lo + 1);
            values[usize::from(low)] = guid as u32;
            values[usize::from(high)] = (guid >> 32) as u32;
            for index in [low, high] {
                let index = usize::from(index);
                delivered[index / 64] |= 1u64 << (index % 64);
            }
        }
        WrathFieldWindow::new(values, delivered)
    }

    #[test]
    fn all_twenty_eight_bank_slots_and_seven_bags_are_addressable() {
        let mut pairs = Vec::new();
        for i in 0..WRATH_BANK_SLOTS {
            pairs.push((BANK + 2 * u16::from(i), u64::from(i) + 1));
        }
        for i in 0..WRATH_BANK_BAG_SLOTS {
            pairs.push((BANK_BAG + 2 * u16::from(i), u64::from(i) + 100));
        }
        let mut inv = WrathInventory::default();
        let changes = inv.apply(&window(&pairs), (1, 1));
        assert_eq!(changes.slots.len(), 35);
        assert_eq!(inv.guid(InventoryAddr::Bank(27)), Some(28));
        assert_eq!(inv.guid(InventoryAddr::BankBag(6)), Some(106));
    }

    #[test]
    fn absent_fields_never_clear_and_delivered_zero_does() {
        let mut inv = WrathInventory::default();
        inv.apply(&window(&[(BANK + 2 * 24, 0x55)]), (1, 1));
        assert_eq!(inv.guid(InventoryAddr::Bank(24)), Some(0x55));

        // A digest that never mentions slot 24 leaves it alone.
        let quiet = inv.apply(&window(&[(BANK + 2 * 25, 0x66)]), (1, 2));
        assert!(quiet.slots.iter().all(|c| c.addr != InventoryAddr::Bank(24)));
        assert_eq!(inv.guid(InventoryAddr::Bank(24)), Some(0x55));

        // A delivered zero pair is an explicit clear.
        let cleared = inv.apply(&window(&[(BANK + 2 * 24, 0)]), (1, 3));
        assert!(cleared
            .slots
            .iter()
            .any(|c| c.addr == InventoryAddr::Bank(24) && c.new == Some(0)));
        assert_eq!(inv.guid(InventoryAddr::Bank(24)), None);
        assert_eq!(inv.presence(InventoryAddr::Bank(24)), Some(Some(0)));
    }

    #[test]
    fn a_half_guid_pair_is_never_assembled() {
        let mut values = vec![0u32; usize::from(PLAYER_WINDOW_END)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        let low = usize::from(BANK + 2 * 20);
        values[low] = 0x1234;
        delivered[low / 64] |= 1u64 << (low % 64);
        let mut inv = WrathInventory::default();
        let changes = inv.apply(&WrathFieldWindow::new(values, delivered), (1, 1));
        assert!(changes.slots.is_empty());
        assert_eq!(inv.presence(InventoryAddr::Bank(20)), None);
    }

    #[test]
    fn a_first_empty_observation_is_not_a_clear_event() {
        let mut inv = WrathInventory::default();
        let changes = inv.apply(&window(&[(BANK + 2 * 26, 0)]), (1, 1));
        assert!(changes.slots.is_empty());
        assert_eq!(inv.presence(InventoryAddr::Bank(26)), Some(Some(0)));
    }

    #[test]
    fn wire_addresses_match_the_3335_array() {
        assert_eq!(InventoryAddr::Bank(0).to_wire(), (BAG_PLAYER_INVENTORY, 39));
        assert_eq!(InventoryAddr::Bank(27).to_wire(), (BAG_PLAYER_INVENTORY, 66));
        assert_eq!(InventoryAddr::BankBag(0).to_wire(), (BAG_PLAYER_INVENTORY, 67));
        assert_eq!(InventoryAddr::BankBag(6).to_wire(), (BAG_PLAYER_INVENTORY, 73));
        assert_eq!(InventoryAddr::Buyback(0).to_wire(), (BAG_PLAYER_INVENTORY, 74));
        assert_eq!(InventoryAddr::Keyring(0).to_wire(), (BAG_PLAYER_INVENTORY, 86));
        assert_eq!(InventoryAddr::Keyring(31).to_wire(), (BAG_PLAYER_INVENTORY, 117));
        assert_eq!(InventoryAddr::BankBag(6).as_bag_byte(), Some(73));
        assert_eq!(InventoryAddr::BankBag(6).lua_bag_id(), Some(11));
    }

    #[test]
    fn field_indices_cover_the_server_sizes() {
        assert_eq!(InventoryAddr::Bank(27).field_index(), BANK + 54);
        assert_eq!(InventoryAddr::BankBag(6).field_index(), BANK_BAG + 12);
        assert_eq!(InventoryAddr::Buyback(11).field_index(), BUYBACK + 22);
        assert_eq!(InventoryAddr::Keyring(31).field_index(), KEYRING + 62);
    }

    #[test]
    fn bank_bag_purchase_count_tracks_play_bytes_2() {
        let mut values = vec![0u32; usize::from(PLAYER_WINDOW_END)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        values[usize::from(BYTES_2)] = 7 << 16;
        let index = usize::from(BYTES_2);
        delivered[index / 64] |= 1u64 << (index % 64);
        let mut inv = WrathInventory::default();
        let changes = inv.apply(&WrathFieldWindow::new(values, delivered), (1, 1));
        assert_eq!(changes.bank_bags_purchased, Some(7));
        assert_eq!(inv.bank_bags_purchased(), Some(7));
    }

    #[test]
    fn scope_change_can_be_cleared_without_dropping_the_type() {
        let mut inv = WrathInventory::default();
        inv.apply(&window(&[(BANK + 2 * 24, 0x99)]), (4, 5));
        assert_eq!(inv.scope(), (4, 5));
        inv.clear();
        assert_eq!(inv.scope(), (0, 0));
        assert_eq!(inv.guid(InventoryAddr::Bank(24)), None);
    }

    #[test]
    fn addr_of_finds_an_item_in_the_extra_tail() {
        let mut inv = WrathInventory::default();
        inv.apply(&window(&[(BANK + 2 * 26, 0xAB)]), (1, 1));
        assert_eq!(
            inv.addr_of(0xAB),
            Some(InventoryAddr::Bank(26))
        );
        assert_eq!(inv.addr_of(0), None);
    }
}
