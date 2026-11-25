//! The player's reputation store: the client's one array of 64 slots (`0xb73290`, 16 bytes each),
//! whose flag byte (`+4`) and wire standing (`+0xc`) every reader shares, and the forced-reaction
//! table beside it.
//!
//! The wire writes the slots, the reputation verbs write a slot's flag byte inside their call
//! (`0x4d5fc0` from `0x4d6074` and `0x4d612c`), and the reaction reads the byte live (`0x60640c`
//! through `0x4d61b0`). Every write goes through `&mut`, the wire's handlers' and the call host's,
//! so a write marks the resource changed in the frame it happens. The slots sit behind one shared
//! handle for one reader outside the world: the Lua API's relation functions, whose snapshot the
//! VM holds between pushes, hold a clone, which is the same slots, so a verb's write is in their
//! answer on the call's next line.

use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::Arc;

use benilla_protocol::messages::WRATH_FACTION_LIST_LEN;
use bevy::prelude::*;

/// One slot's flag byte and wire standing.
#[derive(Default)]
struct Slot {
    flags: AtomicU8,
    standing: AtomicI32,
}

/// The reputation slots (64 on 1.12, 128 on Wrath).
struct Slots {
    cells: [Slot; WRATH_FACTION_LIST_LEN],
}

impl Default for Slots {
    fn default() -> Self {
        Self {
            cells: std::array::from_fn(|_| Slot::default()),
        }
    }
}

/// Our reputation state: the `(flags, standing)` per slot and the forced-reaction table.
///
/// A slot is a `Faction.dbc` `reputationIndex`, 0 to 63; one past them reads `(0, 0)`. The
/// standing excludes the DBC race/class base, which consumers add before ranking. Filled by
/// `SMSG_INITIALIZE_FACTIONS`, kept current by `SMSG_SET_FACTION_STANDING` (which also reveals and
/// sets the at-war bit by rank), `SMSG_SET_FACTION_VISIBLE` and `SMSG_SET_FACTION_ATWAR`, and by
/// the pane's verbs. Flag bit `0x08` marks a pane header.
///
/// `forced` is `SMSG_SET_FORCED_REACTIONS` (`0x4d59a0`): `(Faction.dbc id, reaction rank)` per
/// faction a `SPELL_AURA_FORCE_REACTION` aura forces, which answers with that rank whatever its
/// templates or our standing say. Both halves live on one resource so a change to either wakes the
/// unit feed and the questgiver-marker re-query that read reactions.
///
/// A clone shares the slots and copies the rest: the shared handle is there for the Lua API's
/// relation snapshot alone, which reads the slots live between pushes.
#[derive(Resource, Default, Clone)]
pub struct Reputations {
    slots: Arc<Slots>,
    pub(crate) forced: Vec<(u32, u32)>,
    /// What each message did to the pane's list, in arrival order, for the reputation feed to
    /// hand over after the snapshot ([`crate::ui_reputation::feed_reputation`]).
    pub(crate) list_ops: Vec<benilla_ui::script::ReputationListOp>,
}

impl Reputations {
    /// A store holding only `slots`, from slot 0, with nothing forced.
    #[cfg(test)]
    pub(crate) fn from_slots(slots: Vec<(u8, i32)>) -> Self {
        let mut store = Self::default();
        store.set_slots(&slots);
        store
    }

    /// One empty store for the life of the process, for a test's `'static` relation inputs.
    #[cfg(test)]
    pub(crate) fn empty() -> &'static Self {
        static EMPTY: std::sync::LazyLock<Reputations> = std::sync::LazyLock::new(Default::default);
        &EMPTY
    }

    /// Slot `slot`'s flag byte and wire standing, `(0, 0)` past the 128.
    pub(crate) fn slot(&self, slot: usize) -> (u8, i32) {
        self.slots.cells.get(slot).map_or((0, 0), |c| {
            (
                c.flags.load(Ordering::Relaxed),
                c.standing.load(Ordering::Relaxed),
            )
        })
    }

    /// Slot `slot`'s flag byte (`[slot * 16 + 0xb73294]`), 0 past the 128.
    pub(crate) fn flags(&self, slot: usize) -> u8 {
        self.slot(slot).0
    }

    /// Slot `slot`'s wire standing (`[slot * 16 + 0xb7329c]`), 0 past the 128.
    pub(crate) fn standing(&self, slot: usize) -> i32 {
        self.slot(slot).1
    }

    /// Every slot's `(flags, standing)`, slot 0 first.
    pub(crate) fn slots(&self) -> impl Iterator<Item = (u8, i32)> + '_ {
        (0..WRATH_FACTION_LIST_LEN).map(|i| self.slot(i))
    }

    /// `SMSG_INITIALIZE_FACTIONS`: the list whole, from slot 0; a slot it does not reach reads
    /// `(0, 0)`, and one past the 128 is dropped.
    pub(crate) fn set_slots(&mut self, slots: &[(u8, i32)]) {
        for (i, cell) in self.slots.cells.iter().enumerate() {
            let (flags, standing) = slots.get(i).copied().unwrap_or((0, 0));
            cell.flags.store(flags, Ordering::Relaxed);
            cell.standing.store(standing, Ordering::Relaxed);
        }
    }

    /// A wire message's store of slot `slot`'s wire standing (`0x4d6330`).
    pub(crate) fn set_standing(&mut self, slot: usize, standing: i32) {
        if let Some(cell) = self.slots.cells.get(slot) {
            cell.standing.store(standing, Ordering::Relaxed);
        }
    }

    /// A wire message's store of slot `slot`'s flag byte (`0x4d5fc0`).
    pub(crate) fn set_flags(&mut self, slot: usize, flags: u8) {
        if let Some(cell) = self.slots.cells.get(slot) {
            cell.flags.store(flags, Ordering::Relaxed);
        }
    }

    /// `SMSG_SET_FORCED_REACTIONS`: the table replaces the last whole (`0x4d59a0`).
    pub(crate) fn set_forced(&mut self, table: Vec<(u32, u32)>) {
        self.forced = table;
    }

    /// The teardown (`0x4d5360`): the forced table's count zeroed (`0x4d5419`) and all 64 slots
    /// with it (`0x4d541f`-`0x4d542b`).
    pub(crate) fn tear_down(&mut self) {
        self.forced.clear();
        self.set_slots(&[]);
    }
}

/// The FactionTemplate.dbc catalog; absent if it failed to load, and the faction legs then read
/// neutral. Shared, so the Lua API's copy of the relation inputs costs no table. Loaded by the
/// target ring (`crate::target::ring`), read by the standing rule here.
#[derive(Resource, Clone)]
pub struct Factions(Arc<benilla_formats::FactionCatalog>);

impl Factions {
    /// The loaded catalog, as the client builds the resource once from the DBC.
    pub(crate) fn new(catalog: benilla_formats::FactionCatalog) -> Self {
        Self(Arc::new(catalog))
    }

    pub fn catalog(&self) -> &benilla_formats::FactionCatalog {
        &self.0
    }

    /// Wrap a catalog, for tests; a running client builds the resource once, from the DBC.
    #[cfg(test)]
    pub(crate) fn from_catalog(catalog: benilla_formats::FactionCatalog) -> Self {
        Self::new(catalog)
    }
}

/// A slot's rank, as `0x4d63a0` reads it: the base of the faction whose `reputationIndex` is the
/// slot, for `race`/`class` plus the slot's wire standing, ranked; a slot no faction holds stands at
/// 0, so Neutral (`0x4d6390`).
pub(crate) fn slot_rank(
    catalog: &benilla_formats::FactionCatalog,
    store: &Reputations,
    race: u8,
    class: u8,
    slot: usize,
) -> u8 {
    let standing = catalog
        .reputation_factions()
        .find(|(_, f)| usize::try_from(f.rep_index) == Ok(slot))
        .map_or(0, |(_, f)| f.base_for(race, class) + store.standing(slot));
    benilla_formats::reputation_rank(standing)
}

/// What the reputation chat lines read of the local player.
pub(crate) struct SelfFacts {
    pub race: u8,
    pub class: u8,
    pub female: bool,
}

impl SelfFacts {
    /// The local player's race, class and gender, `None` before the self object streams in; the
    /// reference's `0x62c670` prints nothing without it (`0x62c6bf`). The gender is the real one
    /// (`UNIT_FIELD_BYTES_0` byte 2), where `0x5ed5b0` reads `PLAYER_BYTES_3` byte 0 whole and so
    /// takes a drunk female on vmangos for a male: that byte carries the drunk bits.
    pub(crate) fn read(store: &super::ObjectStore) -> Option<Self> {
        Some(Self {
            race: store.0.unit_race()?,
            class: store.0.unit_class()?,
            female: store.0.unit_gender() == Some(1),
        })
    }
}

/// What the last `SMSG_SET_FACTION_STANDING` latched past the store, for the half of the handler
/// that runs after it ([`super::handlers::HandlerSlot::StandingFollowUps`]).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StandingLatches {
    /// A slot's rank moved: the quest log rebuilds (`0x4d58ea`).
    pub rank_moved: bool,
}
