//! The lossless Wrath item/container instance row.
//!
//! 1.12 gives an item seven enchant slots and puts the property seed, random-properties id,
//! durability and max durability at donor fields 43..47. 3.3.5 keeps twelve enchant slots
//! (`ITEM_FIELD_ENCHANTMENT_1_1` = 22 through `_12_3` = 57), then the same four columns at
//! 58..61 and a fifth, `ITEM_FIELD_CREATE_PLAYED_TIME` (62), which has no 1.12 field at all.
//! [`crate::wrath::push_item_fields`] therefore projects 22..42 and 58..61 into the donor layout
//! and drops 43..57 and 62 on purpose: donor readers keep their exact 1.12 view, and this row
//! keeps the rest beside it.
//!
//! Presence is explicit. A field the reducer never delivered reads [`None`]; a field the server
//! delivered as zero reads `Some(0)`. The reducer's delivered mask is cumulative for the object's
//! generation, so a digest that does not mention a field retains the last delivered value rather
//! than clearing it, and only a delivered zero is an observed clear.
//!
//! The row is not a second object owner: the item/container entity, its generation and its
//! `CONTAINER_FIELD_SLOT_*` ownership stay on the existing `ObjectStore`/`ItemObject` path. The
//! row only decodes the same window losslessly for instance-aware consumers.

use benilla_protocol::ObjectFields;
use benilla_protocol_wrath::fields;
use bevy::prelude::Component;

use crate::wrath_inventory::WrathFieldWindow;

/// 3.3.5's twelve item enchantment slots (`ITEM_FIELD_ENCHANTMENT_1_1` .. `_12_3`).
pub(crate) const WRATH_ENCHANT_SLOTS: u8 = 12;

/// The five spell-charge dwords (`ITEM_FIELD_SPELL_CHARGES` .. +4).
pub(crate) const WRATH_SPELL_CHARGES: u8 = 5;

/// A container's `CONTAINER_FIELD_SLOT_1` capacity in 3.3.5 (`CONTAINER_END` - 66) / 2.
pub(crate) const WRATH_CONTAINER_SLOTS: u8 = 36;

/// `ITEM_FIELD_FLAGS` bit 11 (`0x200`): 3.3.5's "this instance carries item text". The 12340
/// client reads it as `hasItemText` in `CGItemText::SetItem` and the use-arm gate in
/// `Item_C.cpp`; it replaces 1.12's owner-only `ITEM_FIELD_ITEM_TEXT_ID`.
pub(crate) const ITEM_FLAG_HAS_TEXT: u32 = 0x200;

/// One 3.3.5 enchant slot: id/sign, duration and charges, each independently present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WrathEnchant {
    /// The signed id. `Some(0)` is a delivered clear; the sign only selects the tooltip colour and
    /// `abs(id)` names the `SpellItemEnchantment` row.
    pub(crate) id: Option<i32>,
    /// `ITEM_FIELD_ENCHANTMENT_n_2`.
    pub(crate) duration: Option<u32>,
    /// `ITEM_FIELD_ENCHANTMENT_n_3`; a charter rides its signature count here.
    pub(crate) charges: Option<u32>,
}

impl WrathEnchant {
    fn delivered(&self) -> bool {
        self.id.is_some() || self.duration.is_some() || self.charges.is_some()
    }
}

/// A decoded item or container instance, keyed to the entity that already owns the guid.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct WrathItemRow {
    window: WrathFieldWindow,
    /// `OBJECT_FIELD_TYPE & TYPEMASK_CONTAINER`: the window may extend to `CONTAINER_END`.
    container: bool,
    /// The `(session_epoch, world_generation)` the row was decoded in.
    scope: (u64, u64),
}

impl WrathItemRow {
    pub(crate) fn new(window: WrathFieldWindow, container: bool, scope: (u64, u64)) -> Self {
        Self {
            window,
            container,
            scope,
        }
    }

    /// Decode one item/container digest window. `container` selects the `CONTAINER_END` bound; a
    /// non-container never carries its slot array.
    #[allow(dead_code)]
    pub(crate) fn from_fields(
        container: bool,
        scope: (u64, u64),
        field: impl Fn(u16) -> Option<u32>,
        delivered: impl Fn(u16) -> bool,
    ) -> Self {
        let end = if container {
            fields::CONTAINER_END
        } else {
            fields::ITEM_END
        };
        Self::new(
            WrathFieldWindow::from_fields(end, field, delivered),
            container,
            scope,
        )
    }

    /// The raw presence-aware window, for a consumer that must read a field this row does not name.
    #[allow(dead_code)]
    pub(crate) fn window(&self) -> &WrathFieldWindow {
        &self.window
    }

    #[allow(dead_code)]
    pub(crate) fn is_container(&self) -> bool {
        self.container
    }

    #[allow(dead_code)]
    pub(crate) fn scope(&self) -> (u64, u64) {
        self.scope
    }

    /// One enchant slot; `None` when no dword of it was ever delivered.
    pub(crate) fn enchant(&self, slot: u8) -> Option<WrathEnchant> {
        if slot >= WRATH_ENCHANT_SLOTS {
            return None;
        }
        let base = fields::ITEM_FIELD_ENCHANTMENT_1_1 + 3 * u16::from(slot);
        let enchant = WrathEnchant {
            id: self.window.get(base).map(|id| id as i32),
            duration: self.window.get(base + 1),
            charges: self.window.get(base + 2),
        };
        enchant.delivered().then_some(enchant)
    }

    /// The delivered id/sign, `Some(0)` included. `None` means never delivered.
    pub(crate) fn enchant_id(&self, slot: u8) -> Option<i32> {
        self.enchant(slot)?.id
    }

    pub(crate) fn enchant_duration(&self, slot: u8) -> Option<u32> {
        self.enchant(slot)?.duration
    }

    pub(crate) fn enchant_charges(&self, slot: u8) -> Option<u32> {
        self.enchant(slot)?.charges
    }

    /// Every delivered enchant slot with its index, in slot order.
    #[allow(dead_code)]
    pub(crate) fn enchants(&self) -> impl Iterator<Item = (u8, WrathEnchant)> + '_ {
        (0..WRATH_ENCHANT_SLOTS).filter_map(|slot| Some((slot, self.enchant(slot)?)))
    }

    /// Whether any dword of an enchant slot in `slots` differs between two rows. Used by the item
    /// watcher for the 43..57 band the donor projection cannot carry.
    pub(crate) fn enchants_changed(
        before: Option<&Self>,
        after: &Self,
        slots: std::ops::RangeInclusive<u8>,
    ) -> bool {
        slots.clone().any(|slot| {
            let old = before.and_then(|row| row.enchant(slot));
            old != after.enchant(slot)
        })
    }

    pub(crate) fn property_seed(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_PROPERTY_SEED)
    }

    pub(crate) fn random_properties_id(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_RANDOM_PROPERTIES_ID)
    }

    /// `ITEM_FIELD_CREATE_PLAYED_TIME` (62). Stored losslessly although no consumer reads it yet;
    /// 1.12 has no matching field, so it never enters the donor projection.
    pub(crate) fn create_played_time(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_CREATE_PLAYED_TIME)
    }

    pub(crate) fn durability(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_DURABILITY)
    }

    pub(crate) fn max_durability(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_MAXDURABILITY)
    }

    pub(crate) fn stack_count(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_STACK_COUNT)
    }

    pub(crate) fn duration(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_DURATION)
    }

    pub(crate) fn spell_charges(&self, i: u8) -> Option<i32> {
        (i < WRATH_SPELL_CHARGES)
            .then(|| self.window.get(fields::ITEM_FIELD_SPELL_CHARGES + u16::from(i)))
            .flatten()
            .map(|value| value as i32)
    }

    pub(crate) fn flags(&self) -> Option<u32> {
        self.window.get(fields::ITEM_FIELD_FLAGS)
    }

    pub(crate) fn owner(&self) -> Option<u64> {
        self.window.guid(fields::ITEM_FIELD_OWNER)
    }

    pub(crate) fn contained(&self) -> Option<u64> {
        self.window.guid(fields::ITEM_FIELD_CONTAINED)
    }

    pub(crate) fn creator(&self) -> Option<u64> {
        self.window.guid(fields::ITEM_FIELD_CREATOR)
    }

    pub(crate) fn gift_creator(&self) -> Option<u64> {
        self.window.guid(fields::ITEM_FIELD_GIFTCREATOR)
    }

    pub(crate) fn container_num_slots(&self) -> Option<u32> {
        self.container
            .then(|| self.window.get(fields::CONTAINER_FIELD_NUM_SLOTS))
            .flatten()
    }

    pub(crate) fn container_slot(&self, i: u8) -> Option<u64> {
        (self.container && i < WRATH_CONTAINER_SLOTS)
            .then(|| {
                self.window
                    .guid(fields::CONTAINER_FIELD_SLOT_1 + 2 * u16::from(i))
            })
            .flatten()
    }

    /// The 3.3.5 "carries text" instance flag; 1.12's `ITEM_FIELD_ITEM_TEXT_ID` has no 3.3.5
    /// descriptor field, so this bit is the readable signal the bag fork and the reader use.
    pub(crate) fn has_item_text(&self) -> bool {
        self.flags().is_some_and(|flags| flags & ITEM_FLAG_HAS_TEXT != 0)
    }
}

/// An item/container read that prefers the typed Wrath row and falls back to the translated donor
/// descriptor while a consumer is not migrated. The fallback is field-by-field: a row that did
/// deliver a field is authoritative even when it delivered zero, and a row that never delivered it
/// leaves the donor projection's value in charge.
#[derive(Clone, Copy)]
pub(crate) struct ItemRead<'a> {
    donor: &'a ObjectFields,
    row: Option<&'a WrathItemRow>,
}

impl<'a> ItemRead<'a> {
    pub(crate) fn new(donor: &'a ObjectFields, row: Option<&'a WrathItemRow>) -> Self {
        Self { donor, row }
    }

    /// The donor descriptor, for a consumer that still needs a 1.12-only field.
    pub(crate) fn donor(&self) -> &'a ObjectFields {
        self.donor
    }

    pub(crate) fn row(&self) -> Option<&'a WrathItemRow> {
        self.row
    }

    /// The number of enchant slots to walk: twelve with a typed row, the donor seven otherwise.
    pub(crate) fn enchant_slots(&self) -> u8 {
        if self.row.is_some() {
            WRATH_ENCHANT_SLOTS
        } else {
            crate::items::ENCHANT_SLOTS as u8
        }
    }

    /// The enchant id, with a delivered zero read as "no enchant" as the donor accessor does.
    pub(crate) fn enchant(&self, slot: u8) -> Option<i32> {
        if let Some(row) = self.row {
            if let Some(id) = row.enchant_id(slot) {
                return (id != 0).then_some(id);
            }
        }
        self.donor.item_enchant(slot)
    }

    pub(crate) fn enchant_charges(&self, slot: u8) -> u32 {
        if let Some(charges) = self.row.and_then(|row| row.enchant_charges(slot)) {
            return charges;
        }
        self.donor.item_enchant_charges(slot)
    }

    /// The row's delivered duration. 1.12's enchant duration word is never read by the stock
    /// owners, so there is no donor fallback here.
    #[allow(dead_code)]
    pub(crate) fn enchant_duration(&self, slot: u8) -> Option<u32> {
        self.row.and_then(|row| row.enchant_duration(slot))
    }

    pub(crate) fn property_seed(&self) -> u32 {
        self.row
            .and_then(WrathItemRow::property_seed)
            .unwrap_or_else(|| self.donor.item_property_seed())
    }

    pub(crate) fn random_properties_id(&self) -> u32 {
        self.row
            .and_then(WrathItemRow::random_properties_id)
            .unwrap_or_else(|| self.donor.item_random_properties_id())
    }

    #[allow(dead_code)]
    pub(crate) fn create_played_time(&self) -> Option<u32> {
        self.row.and_then(WrathItemRow::create_played_time)
    }

    pub(crate) fn durability(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::durability)
            .or_else(|| self.donor.item_durability())
    }

    pub(crate) fn max_durability(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::max_durability)
            .or_else(|| self.donor.item_max_durability())
    }

    pub(crate) fn stack_count(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::stack_count)
            .or_else(|| self.donor.item_stack_count())
    }

    pub(crate) fn duration(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::duration)
            .or_else(|| self.donor.item_duration())
    }

    pub(crate) fn spell_charges(&self, i: u8) -> Option<i32> {
        self.row
            .and_then(|row| row.spell_charges(i))
            .or_else(|| self.donor.item_spell_charges(i))
    }

    pub(crate) fn flags(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::flags)
            .or_else(|| self.donor.item_flags())
    }

    pub(crate) fn owner(&self) -> Option<u64> {
        self.row
            .and_then(WrathItemRow::owner)
            .or_else(|| self.donor.item_owner())
    }

    #[allow(dead_code)]
    pub(crate) fn contained(&self) -> Option<u64> {
        self.row
            .and_then(WrathItemRow::contained)
            .or_else(|| self.donor.item_contained())
    }

    pub(crate) fn creator(&self) -> Option<u64> {
        match self.row {
            // A delivered zero is an observed clear. Only a creator the row never delivered
            // falls back to the donor projection.
            Some(row) => match row.creator() {
                Some(0) => None,
                Some(guid) => Some(guid),
                None => self.donor.item_creator(),
            },
            None => self.donor.item_creator(),
        }
    }

    pub(crate) fn gift_creator(&self) -> Option<u64> {
        match self.row {
            Some(row) => match row.gift_creator() {
                Some(0) => None,
                Some(guid) => Some(guid),
                None => self.donor.item_gift_creator(),
            },
            None => self.donor.item_gift_creator(),
        }
    }

    pub(crate) fn container_num_slots(&self) -> Option<u32> {
        self.row
            .and_then(WrathItemRow::container_num_slots)
            .or_else(|| self.donor.container_num_slots())
    }

    pub(crate) fn container_slot(&self, i: u8) -> Option<u64> {
        self.row
            .and_then(|row| row.container_slot(i))
            .or_else(|| self.donor.container_slot(i))
    }

    /// 1.12's owner-only text id. 3.3.5 has no such descriptor field; [`Self::has_text`] is the
    /// Wrath signal.
    pub(crate) fn item_text_id(&self) -> Option<u32> {
        self.donor.item_text_id()
    }

    /// Whether the instance is readable: the 1.12 owner-only text id, or 3.3.5's
    /// `ITEM_FIELD_FLAGS & 0x200`.
    pub(crate) fn has_text(&self) -> bool {
        self.item_text_id().is_some_and(|id| id != 0) || self.row.is_some_and(WrathItemRow::has_item_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pairs: &[(u16, u32)]) -> WrathFieldWindow {
        let mut values = vec![0u32; usize::from(fields::CONTAINER_END)];
        let mut delivered = vec![0u64; values.len().div_ceil(64)];
        for &(index, value) in pairs {
            let at = usize::from(index);
            values[at] = value;
            delivered[at / 64] |= 1u64 << (at % 64);
        }
        WrathFieldWindow::new(values, delivered)
    }

    /// A row whose only delivered fields are the supplied `(index, value)` pairs.
    fn row(pairs: &[(u16, u32)]) -> WrathItemRow {
        WrathItemRow::new(window(pairs), false, (1, 2))
    }

    #[test]
    fn absent_and_delivered_zero_are_distinct() {
        let absent = row(&[]);
        assert_eq!(absent.durability(), None, "never delivered");
        let cleared = row(&[(fields::ITEM_FIELD_DURABILITY, 0)]);
        assert_eq!(cleared.durability(), Some(0), "an observed clear");
        assert_eq!(cleared.create_played_time(), None);
    }

    #[test]
    fn all_twelve_enchant_slots_decode() {
        let mut pairs = Vec::new();
        for slot in 0..WRATH_ENCHANT_SLOTS {
            let base = fields::ITEM_FIELD_ENCHANTMENT_1_1 + 3 * u16::from(slot);
            pairs.push((base, u32::from(slot) + 1));
            pairs.push((base + 1, 100 + u32::from(slot)));
            pairs.push((base + 2, 200 + u32::from(slot)));
        }
        let row = row(&pairs);
        assert_eq!(row.enchant(0).unwrap().id, Some(1));
        assert_eq!(row.enchant(7).unwrap().id, Some(8), "slot 8 starts at 43");
        assert_eq!(row.enchant(11).unwrap().charges, Some(211));
        assert_eq!(row.enchant(12), None, "the 3.3.5 array ends at twelve");
    }

    #[test]
    fn create_played_time_is_stored_but_never_projected() {
        let row = row(&[(fields::ITEM_FIELD_CREATE_PLAYED_TIME, 1234)]);
        assert_eq!(row.create_played_time(), Some(1234));
        // The donor projection has no field for it; `push_item_fields` is covered in wrath tests.
        let donor = ObjectFields::from_pairs(&[]);
        let read = ItemRead::new(&donor, Some(&row));
        assert_eq!(read.create_played_time(), Some(1234));
    }

    #[test]
    fn the_container_slot_array_is_the_container_owners() {
        let row = WrathItemRow::new(
            window(&[
                (fields::CONTAINER_FIELD_NUM_SLOTS, 2),
                (fields::CONTAINER_FIELD_SLOT_1, 0x11),
                (fields::CONTAINER_FIELD_SLOT_1 + 1, 0),
                (fields::CONTAINER_FIELD_SLOT_1 + 2, 0x22),
                (fields::CONTAINER_FIELD_SLOT_1 + 3, 0),
            ]),
            true,
            (1, 2),
        );
        assert_eq!(row.container_num_slots(), Some(2));
        assert_eq!(row.container_slot(0), Some(0x11));
        assert_eq!(row.container_slot(1), Some(0x22));
        // An item row never reads a slot array, even if its window carries bytes there.
        let item = WrathItemRow::new(window(&[(fields::CONTAINER_FIELD_SLOT_1, 0x33)]), false, (1, 2));
        assert_eq!(item.container_slot(0), None);
        assert_eq!(item.container_num_slots(), None);
    }

    #[test]
    fn donor_fallback_reads_the_unmigrated_fields() {
        let donor = ObjectFields::from_pairs(&[
            (fields::ITEM_FIELD_ENCHANTMENT_1_1, 2564),
            (fields::ITEM_FIELD_ENCHANTMENT_1_3, 5),
            (fields::ITEM_FIELD_DURABILITY, 17),
            (fields::ITEM_FIELD_MAXDURABILITY, 40),
            (fields::ITEM_FIELD_STACK_COUNT, 3),
            (fields::ITEM_FIELD_PROPERTY_SEED, 0x55),
            (fields::ITEM_FIELD_RANDOM_PROPERTIES_ID, 0x66),
        ]);
        let read = ItemRead::new(&donor, None);
        assert_eq!(read.enchant(0), Some(2564));
        assert_eq!(read.enchant_charges(0), 5);
        assert_eq!(read.durability(), Some(17));
        assert_eq!(read.stack_count(), Some(3));
        assert_eq!(read.property_seed(), 0x55);
        assert_eq!(read.random_properties_id(), 0x66);
        assert_eq!(read.enchant_slots(), 7);
    }

    #[test]
    fn a_delivered_random_property_row_wins_over_the_donor_projection() {
        let donor = ObjectFields::from_pairs(&[
            (fields::ITEM_FIELD_PROPERTY_SEED, 0x55),
            (fields::ITEM_FIELD_RANDOM_PROPERTIES_ID, 0x66),
        ]);
        let row = row(&[
            (fields::ITEM_FIELD_RANDOM_PROPERTIES_ID, 0),
            (fields::ITEM_FIELD_PROPERTY_SEED, 0x77),
        ]);
        let read = ItemRead::new(&donor, Some(&row));
        assert_eq!(read.property_seed(), 0x77);
        assert_eq!(
            read.random_properties_id(),
            0,
            "a delivered clear is not a donor fallback"
        );
    }

    #[test]
    fn a_delivered_creator_or_gift_zero_does_not_fall_back_to_donor() {
        let donor = ObjectFields::from_pairs(&[(fields::ITEM_FIELD_CREATOR, 0x11)]);
        let row = row(&[
            (fields::ITEM_FIELD_CREATOR, 0),
            (fields::ITEM_FIELD_GIFTCREATOR, 0),
        ]);
        let read = ItemRead::new(&donor, Some(&row));
        assert_eq!(read.creator(), None, "creator observed zero");
        assert_eq!(read.gift_creator(), None, "gift creator observed zero");

        // A row that never delivered them still uses the donor projection.
        let absent = row(&[]);
        let read = ItemRead::new(&donor, Some(&absent));
        assert_eq!(read.creator(), Some(0x11));
    }

    #[test]
    fn a_delivered_row_zero_wins_over_the_donor_projection() {
        // The projection still has the old durability, but the row observed an explicit clear.
        let donor = ObjectFields::from_pairs(&[(fields::ITEM_FIELD_DURABILITY, 17)]);
        let row = row(&[(fields::ITEM_FIELD_DURABILITY, 0)]);
        let read = ItemRead::new(&donor, Some(&row));
        assert_eq!(read.durability(), Some(0), "not a silent donor fallback");
    }

    #[test]
    fn the_has_text_bit_is_the_wrath_readable_signal() {
        let donor = ObjectFields::from_pairs(&[]);
        let plain = row(&[(fields::ITEM_FIELD_FLAGS, 0)]);
        assert!(!ItemRead::new(&donor, Some(&plain)).has_text());
        let readable = row(&[(fields::ITEM_FIELD_FLAGS, ITEM_FLAG_HAS_TEXT)]);
        assert!(ItemRead::new(&donor, Some(&readable)).has_text());
        // A 1.12 letter still reads through its own field.
        let letter = ObjectFields::from_pairs(&[(45, 77)]);
        assert!(ItemRead::new(&letter, None).has_text());
    }

    #[test]
    fn only_slots_eight_to_twelve_are_the_unprojected_band() {
        let before = row(&[(fields::ITEM_FIELD_ENCHANTMENT_8_1, 1)]);
        let after = row(&[(fields::ITEM_FIELD_ENCHANTMENT_8_1, 2)]);
        assert!(WrathItemRow::enchants_changed(
            Some(&before),
            &after,
            7..=11
        ));
        let stable = row(&[(fields::ITEM_FIELD_ENCHANTMENT_8_1, 1)]);
        assert!(!WrathItemRow::enchants_changed(
            Some(&before),
            &stable,
            7..=11
        ));
    }
}
