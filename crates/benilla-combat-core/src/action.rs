//! The generalized action schema: one profile shape for every combat action the Souls seam
//! understands, whether it came from the authored fixture, a hand-tuned pack, or the automated
//! profile compiler in [`crate::profile`].
//!
//! [`DodgeSpec`](crate::data::DodgeSpec) stays the authored source of truth for the v1 dodge;
//! [`dodge_action`] carries it into this schema without inventing values. Everything is integer
//! ticks at the table's [`TickRate`](crate::time::TickRate); every window is half-open
//! `[from, to)`, matching the donor's `from <= frame < to` convention.

use std::collections::BTreeMap;

use crate::data::DodgeSpec;
use crate::error::{CombatError, CombatResult};
use crate::identity::ActionRevision;
use crate::time::{TickRate, TickWindow};

/// A stable per-revision action identifier. The layout the host and server agree on:
///
/// - `0` is invalid;
/// - `1..=999` host actions (dodge, and later guard) shared by every encounter;
/// - `1_000..=999_999` enemy actions, assigned per encounter;
/// - `1_000_000` and up spell actions, offset by the `Spell.dbc` id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ActionId(pub u32);

/// The action families this schema models today. `Guard` is reserved: no compiler emits it yet
/// and no consumer may treat it as playable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind {
    /// A light weapon swing: chains, low commitment.
    MeleeLight,
    /// A heavy weapon swing (the 2H family): slower, higher stagger, longer commitment.
    MeleeHeavy,
    /// The release-versus-sprint dodge with authored i-frames.
    Dodge,
    /// A shield guard hold. Reserved; arrives with the guard slice.
    Guard,
    /// An instant spell: no windup, a short recovery.
    SpellInstant,
    /// A timed spell cast: windup is the cast time, interruptible while casting.
    SpellCast,
    /// A channeled spell: windup then a bounded sustain window.
    SpellChannel,
    /// A telegraphed NPC strike: windup, contact, recovery; no player input buffer.
    EnemyStrike,
}

/// One action's timing, cost and interruption policy, as tick offsets from the action's start.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActionSpec {
    pub id: ActionId,
    pub kind: ActionKind,
    /// Total action length in ticks; the action ends at this offset.
    pub total_ticks: u64,
    /// First offset at which a queued input is heard (the donor's `input_from`).
    pub input_from_ticks: u64,
    /// First offset at which a queued action may replace this one, if it can be cancelled.
    pub cancel_from_ticks: Option<u64>,
    /// The half-open invincibility window; present on `Dodge` alone.
    pub iframes: Option<TickWindow>,
    /// The half-open contact window: the melee hit, the spell release, the enemy strike.
    pub contact: Option<TickWindow>,
    /// Stamina spent when the action is accepted. Zero for spell actions (power is the spell
    /// engine's) and enemy strikes (enemy stamina is not modelled).
    pub stamina_cost: u32,
    /// The last offset at which interruption applies; `None` is full hyperarmor. The compiler
    /// commits melee swings at contact and interruptible casts at release.
    pub interruptible_until: Option<u64>,
    /// A combo follow-up. `Some(x)` means only `x` may replace this action in its cancel
    /// window (the chain whitelist); `None` lets any action replace it there.
    pub followup: Option<ActionId>,
}

impl ActionSpec {
    /// The windup length: ticks before the contact window opens (before the i-frame window for a
    /// dodge, zero for an instant).
    pub fn windup_ticks(&self) -> u64 {
        if let Some(contact) = self.contact {
            contact.from
        } else if let Some(iframes) = self.iframes {
            iframes.from
        } else {
            0
        }
    }

    /// The recovery length: ticks after the active window ends, before the action ends.
    pub fn recovery_ticks(&self) -> u64 {
        let active_end = self
            .contact
            .map(|window| window.to)
            .or(self.iframes.map(|window| window.to))
            .unwrap_or(self.total_ticks);
        self.total_ticks.saturating_sub(active_end)
    }

    /// Schema-level validation of one action. Cross-action rules (duplicate ids, follow-up
    /// resolution) live on [`ActionTable::validate`].
    pub fn validate(&self) -> CombatResult<()> {
        if self.id == ActionId(0) {
            return Err(CombatError::InvalidConfig("action id 0 is reserved"));
        }
        if self.total_ticks == 0 {
            return Err(CombatError::InvalidConfig("total_ticks must be non-zero"));
        }
        if self.input_from_ticks > self.total_ticks {
            return Err(CombatError::InvalidConfig(
                "input_from_ticks must not exceed total_ticks",
            ));
        }
        if self.cancel_from_ticks.is_some_and(|from| from > self.total_ticks) {
            return Err(CombatError::InvalidConfig(
                "cancel_from_ticks must not exceed total_ticks",
            ));
        }
        if self
            .interruptible_until
            .is_some_and(|until| until > self.total_ticks)
        {
            return Err(CombatError::InvalidConfig(
                "interruptible_until must not exceed total_ticks",
            ));
        }
        let half_open_within = |window: Option<TickWindow>| {
            window.is_none_or(|window| window.from < window.to && window.to <= self.total_ticks)
        };
        if !half_open_within(self.iframes) {
            return Err(CombatError::InvalidConfig(
                "iframes must be a half-open window inside total_ticks",
            ));
        }
        if !half_open_within(self.contact) {
            return Err(CombatError::InvalidConfig(
                "contact must be a half-open window inside total_ticks",
            ));
        }
        // Window presence by kind: a dodge owns i-frames alone; a strike or spell owns a contact
        // window alone; the reserved guard owns neither yet.
        let needs_contact = matches!(
            self.kind,
            ActionKind::MeleeLight
                | ActionKind::MeleeHeavy
                | ActionKind::SpellInstant
                | ActionKind::SpellCast
                | ActionKind::SpellChannel
                | ActionKind::EnemyStrike
        );
        match self.kind {
            ActionKind::Dodge => {
                if self.iframes.is_none() {
                    return Err(CombatError::InvalidConfig("a dodge requires its iframes"));
                }
                if self.contact.is_some() {
                    return Err(CombatError::InvalidConfig(
                        "a dodge carries no contact window",
                    ));
                }
            }
            ActionKind::Guard => {
                if self.iframes.is_some() || self.contact.is_some() {
                    return Err(CombatError::InvalidConfig(
                        "the reserved guard carries no windows",
                    ));
                }
            }
            _ => {
                if !needs_contact || self.contact.is_none() {
                    return Err(CombatError::InvalidConfig(
                        "this action kind requires its contact window",
                    ));
                }
                if self.iframes.is_some() {
                    return Err(CombatError::InvalidConfig(
                        "only a dodge carries iframes",
                    ));
                }
            }
        }
        let needs_stamina = matches!(
            self.kind,
            ActionKind::MeleeLight | ActionKind::MeleeHeavy | ActionKind::Dodge
        );
        if needs_stamina && self.stamina_cost == 0 {
            return Err(CombatError::InvalidConfig(
                "melee and dodge actions must cost stamina",
            ));
        }
        if self.followup == Some(self.id) {
            return Err(CombatError::InvalidConfig(
                "an action cannot be its own follow-up",
            ));
        }
        Ok(())
    }
}

/// Carry the authored [`DodgeSpec`] into the schema without inventing values: the windows, cost
/// and total arrive exactly as authored. The dodge is interruptible for its whole length,
/// matching the fixture's rule that a non-lethal hit clears it until the authored recovery.
pub fn dodge_action(id: ActionId, dodge: &DodgeSpec) -> ActionSpec {
    ActionSpec {
        id,
        kind: ActionKind::Dodge,
        total_ticks: dodge.total_ticks,
        input_from_ticks: dodge.dodge_input_threshold(),
        cancel_from_ticks: Some(dodge.cancel_dodge_from_tick),
        iframes: Some(dodge.iframes),
        contact: None,
        stamina_cost: dodge.stamina_cost,
        interruptible_until: Some(dodge.total_ticks),
        followup: None,
    }
}

/// A validated set of actions under one immutable revision, compiled at one tick rate.
#[derive(Clone, Debug)]
pub struct ActionTable {
    /// The authored table revision inputs and snapshots must match.
    pub revision: ActionRevision,
    /// The rate this table's tick offsets were compiled at.
    pub tick_rate: TickRate,
    /// Keyed by id in a `BTreeMap` so iteration and validation are deterministic and ordered by
    /// [`ActionId`], independent of insertion order.
    actions: BTreeMap<ActionId, ActionSpec>,
}

impl ActionTable {
    /// An empty table; fill it with [`Self::insert`] and check [`Self::validate`] before use.
    pub fn new(revision: ActionRevision, tick_rate: TickRate) -> Self {
        Self {
            revision,
            tick_rate,
            actions: BTreeMap::new(),
        }
    }

    /// Insert one validated action; a duplicate id is an error, never a replacement.
    pub fn insert(&mut self, spec: ActionSpec) -> CombatResult<()> {
        spec.validate()?;
        if self.actions.contains_key(&spec.id) {
            return Err(CombatError::DuplicateAction(spec.id.0));
        }
        self.actions.insert(spec.id, spec);
        Ok(())
    }

    /// The action with this id, if the table holds it.
    pub fn get(&self, id: ActionId) -> Option<&ActionSpec> {
        self.actions.get(&id)
    }

    /// Every action, ordered by ascending action id.
    pub fn iter(&self) -> impl Iterator<Item = &ActionSpec> + '_ {
        self.actions.values()
    }

    /// The number of actions in the table.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the table holds no actions.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Whole-table validation: every action is valid, and every follow-up resolves to an action
    /// in this same table. Checks run in ascending action-id order, so the first error is
    /// deterministic and independent of insertion order.
    pub fn validate(&self) -> CombatResult<()> {
        for spec in self.actions.values() {
            spec.validate()?;
            if let Some(followup) = spec.followup {
                if !self.actions.contains_key(&followup) {
                    return Err(CombatError::MissingFollowup(followup.0));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TickRate;

    fn rate() -> TickRate {
        TickRate::new(100).expect("100 Hz")
    }

    fn melee(id: u32, total: u64) -> ActionSpec {
        ActionSpec {
            id: ActionId(id),
            kind: ActionKind::MeleeLight,
            total_ticks: total,
            input_from_ticks: total,
            cancel_from_ticks: Some(total),
            iframes: None,
            contact: Some(TickWindow { from: 40, to: 42 }),
            stamina_cost: 20,
            interruptible_until: Some(40),
            followup: None,
        }
    }

    /// The phase helpers read contact first, i-frames second: a melee action's windup is its
    /// contact start and its recovery runs to the total.
    #[test]
    fn phase_helpers_read_the_active_window() {
        let spec = melee(1, 50);
        assert_eq!(spec.windup_ticks(), 40);
        assert_eq!(spec.recovery_ticks(), 8);

        let dodge = dodge_action(
            ActionId(1),
            &DodgeSpec {
                revision: ActionRevision(7),
                total_ticks: 30,
                input_from_tick: 5,
                dodge_from_tick: 10,
                cancel_dodge_from_tick: 12,
                cancel_move_from_tick: 20,
                iframes: TickWindow { from: 6, to: 20 },
                stamina_cost: 25,
            },
        );
        assert_eq!(dodge.windup_ticks(), 6);
        assert_eq!(dodge.recovery_ticks(), 10);
    }

    /// The authored dodge carries over exactly: same total, same i-frames, same cost, interruptible
    /// for its whole length, and its input threshold is the donor's max rule.
    #[test]
    fn dodge_spec_carries_over_exactly() {
        let spec = dodge_action(
            ActionId(1),
            &DodgeSpec {
                revision: ActionRevision(7),
                total_ticks: 30,
                input_from_tick: 5,
                dodge_from_tick: 10,
                cancel_dodge_from_tick: 12,
                cancel_move_from_tick: 20,
                iframes: TickWindow { from: 6, to: 20 },
                stamina_cost: 25,
            },
        );
        assert_eq!(spec.kind, ActionKind::Dodge);
        assert_eq!(spec.total_ticks, 30);
        assert_eq!(spec.input_from_ticks, 10, "max(input_from, dodge_from)");
        assert_eq!(spec.cancel_from_ticks, Some(12));
        assert_eq!(spec.iframes, Some(TickWindow { from: 6, to: 20 }));
        assert_eq!(spec.stamina_cost, 25);
        assert_eq!(spec.interruptible_until, Some(30));
        spec.validate().expect("the carried-over dodge validates");
    }

    /// The schema refuses the window violations the fixture's rules imply: a dodge without
    /// i-frames, a melee action without contact, i-frames on a melee action, and windows that
    /// leave the action or are empty.
    #[test]
    fn window_presence_is_policed_by_kind() {
        let mut no_iframes = dodge_action(
            ActionId(1),
            &DodgeSpec {
                revision: ActionRevision(1),
                total_ticks: 30,
                input_from_tick: 5,
                dodge_from_tick: 10,
                cancel_dodge_from_tick: 12,
                cancel_move_from_tick: 20,
                iframes: TickWindow { from: 6, to: 20 },
                stamina_cost: 25,
            },
        );
        no_iframes.iframes = None;
        assert!(no_iframes.validate().is_err());

        let mut disarmed = melee(1, 50);
        disarmed.contact = None;
        assert!(disarmed.validate().is_err());

        let mut armed = melee(1, 50);
        armed.iframes = Some(TickWindow { from: 0, to: 1 });
        assert!(armed.validate().is_err());

        let mut overshoot = melee(1, 50);
        overshoot.contact = Some(TickWindow { from: 49, to: 51 });
        assert!(overshoot.validate().is_err());

        let mut empty = melee(1, 50);
        empty.contact = Some(TickWindow { from: 40, to: 40 });
        assert!(empty.validate().is_err());
    }

    /// Offsets past the total, a zero id, a zero total, a zero melee cost and a self follow-up
    /// are all refused.
    #[test]
    fn offsets_and_ids_are_bounded() {
        let mut late = melee(1, 50);
        late.input_from_ticks = 51;
        assert!(late.validate().is_err());

        let mut zero_id = melee(0, 50);
        zero_id.id = ActionId(0);
        assert!(zero_id.validate().is_err());

        let mut free = melee(1, 50);
        free.stamina_cost = 0;
        assert!(free.validate().is_err());

        let mut self_loop = melee(1, 50);
        self_loop.followup = Some(ActionId(1));
        assert!(self_loop.validate().is_err());
    }

    /// The table rejects a duplicate id and a follow-up that resolves to nothing, and validates
    /// only when every member does.
    #[test]
    fn table_rejects_duplicates_and_dangling_followups() {
        let mut table = ActionTable::new(ActionRevision(1), rate());
        table.insert(melee(1, 50)).expect("insert first");
        assert!(matches!(
            table.insert(melee(1, 60)),
            Err(CombatError::DuplicateAction(1))
        ));

        let mut chained = melee(2, 50);
        chained.followup = Some(ActionId(99));
        table.insert(chained).expect("insert chained");
        assert!(matches!(
            table.validate(),
            Err(CombatError::MissingFollowup(99))
        ));

        let mut resolved = melee(99, 50);
        resolved.kind = ActionKind::MeleeHeavy;
        resolved.stamina_cost = 35;
        table.insert(resolved).expect("insert follow-up");
        table.validate().expect("the chained table validates");
        assert_eq!(table.len(), 3);
        assert!(table.get(ActionId(0)).is_none());
        assert!(table.get(ActionId(1)).is_some_and(|spec| spec.kind == ActionKind::MeleeLight));
    }

    /// The table is ordered by action id, not by insertion or hash order: iteration is ascending,
    /// and validation reports the same first error for every insertion order.
    #[test]
    fn iteration_and_validation_are_ordered_by_action_id() {
        let mut table = ActionTable::new(ActionRevision(1), rate());
        for id in [30, 10, 20] {
            table.insert(melee(id, 50)).expect("insert");
        }
        let order: Vec<u32> = table.iter().map(|spec| spec.id.0).collect();
        assert_eq!(order, vec![10, 20, 30]);

        // Two tables holding the same dangling follow-ups, inserted in opposite orders, report
        // the error for the lowest action id both times.
        let dangling = |id: u32, followup: u32| {
            let mut spec = melee(id, 50);
            spec.followup = Some(ActionId(followup));
            spec
        };
        let mut ascending = ActionTable::new(ActionRevision(1), rate());
        let mut descending = ActionTable::new(ActionRevision(1), rate());
        ascending.insert(dangling(10, 998)).expect("insert");
        ascending.insert(dangling(30, 999)).expect("insert");
        descending.insert(dangling(30, 999)).expect("insert");
        descending.insert(dangling(10, 998)).expect("insert");
        assert_eq!(ascending.validate(), descending.validate());
        assert!(matches!(
            ascending.validate(),
            Err(CombatError::MissingFollowup(998))
        ));
    }
}
